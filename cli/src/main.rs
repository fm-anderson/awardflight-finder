pub mod config;
pub mod fx;
pub mod markdown;
pub mod models;
pub mod pricing;
pub mod runner;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use clap::Parser;
use config::AppConfig;
use fx::FxRates;
use markdown::{LegReport, ReportMetadata, render_markdown_report};
use models::CabinFilter;
use pricing::price_and_rank_flights;
use runner::{LegQuery, run_leg_scrapers};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "awardflight-finder",
    version,
    about = "Search, normalize, value, and categorize airline award flights across loyalty programs"
)]
pub struct CliArgs {
    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,

    #[arg(long)]
    pub depart: String,

    #[arg(long = "return")]
    pub return_date: Option<String>,

    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=9))]
    pub adults: u8,

    #[arg(long, value_enum, default_value_t = CabinFilter::All, ignore_case = true)]
    pub cabin: CabinFilter,

    #[arg(long = "max-stops", default_value_t = 0)]
    pub max_stops: u32,

    #[arg(long, default_value = "all")]
    pub programs: String,

    #[arg(long, default_value = "results.md")]
    pub output: PathBuf,

    #[arg(long, default_value = "config.json")]
    pub config: PathBuf,

    #[arg(long, hide = true)]
    pub mock_json: Option<PathBuf>,
}

fn validate_iata(code: &str, flag_name: &str) -> Result<String> {
    let normalized = code.trim().to_uppercase();
    if normalized.len() != 3 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
        bail!(
            "Invalid IATA airport code '{}' for {}: must be a 3-letter code (e.g., GRU, MCO)",
            code,
            flag_name
        );
    }
    Ok(normalized)
}

fn validate_date(date_str: &str, flag_name: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").with_context(|| {
        format!(
            "Invalid date '{}' for {}: expected YYYY-MM-DD format",
            date_str, flag_name
        )
    })
}

fn resolve_output_path(
    output: &Path,
    depart_date: &str,
    return_date: Option<&str>,
) -> PathBuf {
    let date_suffix = match return_date {
        Some(ret) => format!("{}_{}", depart_date, ret),
        None => depart_date.to_string(),
    };

    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("results");

    if stem.contains(depart_date) {
        return output.to_path_buf();
    }

    let ext = output.extension().and_then(|e| e.to_str());
    let new_filename = match ext {
        Some(e) if !e.is_empty() => format!("{}-{}.{}", stem, date_suffix, e),
        _ => format!("{}-{}", stem, date_suffix),
    };

    match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(new_filename),
        _ => PathBuf::from(new_filename),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    let origin = validate_iata(&args.from, "--from")?;
    let destination = validate_iata(&args.to, "--to")?;
    if origin == destination {
        bail!(
            "Origin (--from) and destination (--to) cannot be the same airport ('{}')",
            origin
        );
    }

    let depart_date = validate_date(&args.depart, "--depart")?;
    let depart_str = depart_date.format("%Y-%m-%d").to_string();

    let return_str = if let Some(ref ret) = args.return_date {
        let ret_date = validate_date(ret, "--return")?;
        if ret_date < depart_date {
            bail!(
                "Return date ({}) cannot be earlier than departure date ({})",
                ret_date,
                depart_date
            );
        }
        Some(ret_date.format("%Y-%m-%d").to_string())
    } else {
        None
    };

    let (app_config, resolved_config_path) = AppConfig::load(&args.config)?;
    let selected_programs = app_config.resolve_programs(&args.programs)?;

    eprintln!(
        "[cli] Loaded configuration from {} (programs: {})",
        resolved_config_path.display(),
        selected_programs.join(", ")
    );

    let fx_rates = FxRates::fetch_or_fallback(&app_config.fallback_fx_rates).await;
    eprintln!("[cli] FX rates initialized ({})", fx_rates.source.as_str());

    let outbound_query = LegQuery {
        origin: origin.clone(),
        destination: destination.clone(),
        date: depart_str.clone(),
        adults: args.adults,
        cabin: args.cabin,
        max_stops: args.max_stops,
        use_smiles_club: app_config.use_smiles_club,
    };

    let outbound_raw = run_leg_scrapers(
        &outbound_query,
        &selected_programs,
        args.mock_json.as_deref(),
    )
    .await?;

    let outbound_priced = price_and_rank_flights(
        outbound_raw.flights,
        &app_config,
        &fx_rates,
        args.cabin,
        args.max_stops,
    )?;

    let return_data = if let Some(ref ret_date_str) = return_str {
        let return_query = LegQuery {
            origin: destination.clone(),
            destination: origin.clone(),
            date: ret_date_str.clone(),
            adults: args.adults,
            cabin: args.cabin,
            max_stops: args.max_stops,
            use_smiles_club: app_config.use_smiles_club,
        };

        let return_raw =
            run_leg_scrapers(&return_query, &selected_programs, args.mock_json.as_deref()).await?;

        let return_priced = price_and_rank_flights(
            return_raw.flights,
            &app_config,
            &fx_rates,
            args.cabin,
            args.max_stops,
        )?;

        Some((ret_date_str.clone(), return_priced, return_raw.errors))
    } else {
        None
    };

    let meta = ReportMetadata {
        origin: &origin,
        destination: &destination,
        depart_date: &depart_str,
        return_date: return_str.as_deref(),
        adults: args.adults,
        cabin: args.cabin,
        max_stops: args.max_stops,
        programs: &selected_programs,
        fx: &fx_rates,
    };

    let outbound_report = LegReport {
        title: "Outbound",
        origin: &origin,
        destination: &destination,
        date: &depart_str,
        flights: &outbound_priced,
        errors: &outbound_raw.errors,
    };

    let inbound_report = return_data
        .as_ref()
        .map(|(ret_date_str, flights, errors)| LegReport {
            title: "Return",
            origin: &destination,
            destination: &origin,
            date: ret_date_str,
            flights,
            errors,
        });

    let markdown = render_markdown_report(&meta, &outbound_report, inbound_report.as_ref());
    let output_path = resolve_output_path(&args.output, &depart_str, return_str.as_deref());

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory {}", parent.display()))?;
    }

    fs::write(&output_path, &markdown).with_context(|| {
        format!(
            "Failed to write Markdown report to {}",
            output_path.display()
        )
    })?;

    eprintln!(
        "[cli] Wrote award flight report to {} (Outbound: {} option(s){})",
        output_path.display(),
        outbound_priced.len(),
        return_data
            .as_ref()
            .map(|(_, f, _)| format!(", Return: {} option(s)", f.len()))
            .unwrap_or_default()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_output_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn appends_depart_date_to_output_filename() {
        let path = resolve_output_path(Path::new("mia-bsb-direct.md"), "2026-12-10", None);
        assert_eq!(path, PathBuf::from("mia-bsb-direct-2026-12-10.md"));
    }

    #[test]
    fn appends_depart_and_return_dates_for_round_trip() {
        let path = resolve_output_path(
            Path::new("reports/mia-bsb-rt.md"),
            "2026-12-10",
            Some("2026-12-20"),
        );
        assert_eq!(
            path,
            PathBuf::from("reports/mia-bsb-rt-2026-12-10_2026-12-20.md")
        );
    }

    #[test]
    fn does_not_duplicate_date_when_already_present() {
        let path = resolve_output_path(
            Path::new("mia-bsb-direct-2026-12-10.md"),
            "2026-12-10",
            None,
        );
        assert_eq!(path, PathBuf::from("mia-bsb-direct-2026-12-10.md"));
    }
}
