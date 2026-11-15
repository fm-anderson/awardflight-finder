use crate::models::{CabinFilter, NormalizedAwardFlight};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct LegQuery {
    pub origin: String,
    pub destination: String,
    pub date: String,
    pub adults: u8,
    pub cabin: CabinFilter,
    pub max_stops: u32,
    pub use_smiles_club: bool,
}

#[derive(Debug, Default)]
pub struct LegScrapeOutput {
    pub flights: Vec<NormalizedAwardFlight>,
    pub errors: Vec<(String, String)>,
}

pub async fn run_leg_scrapers(
    query: &LegQuery,
    programs: &[String],
    mock_json_path: Option<&Path>,
) -> Result<LegScrapeOutput> {
    if let Some(mock_path) = mock_json_path {
        let raw = std::fs::read_to_string(mock_path)
            .with_context(|| format!("Failed to read mock JSON file at {}", mock_path.display()))?;
        let all_flights: Vec<NormalizedAwardFlight> =
            serde_json::from_str(&raw).with_context(|| {
                format!(
                    "Invalid NormalizedAwardFlight JSON in {}",
                    mock_path.display()
                )
            })?;

        let filtered: Vec<NormalizedAwardFlight> = all_flights
            .into_iter()
            .filter(|f| {
                programs
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(&f.program_key))
                    && f.origin.eq_ignore_ascii_case(&query.origin)
                    && f.destination.eq_ignore_ascii_case(&query.destination)
            })
            .collect();

        return Ok(LegScrapeOutput {
            flights: filtered,
            errors: Vec::new(),
        });
    }

    let scraper_dir = resolve_scraper_dir()?;
    let entrypoint = scraper_dir.join("src").join("index.ts");
    if !entrypoint.exists() {
        bail!(
            "Scraper entrypoint not found at {}. Initialize scraper/src/index.ts or pass --mock-json <FILE> to test the CLI.",
            entrypoint.display()
        );
    }

    let mut join_set = JoinSet::new();

    for program in programs {
        let program_key = program.clone();
        let leg = query.clone();
        let dir = scraper_dir.clone();

        join_set.spawn(async move {
            let res = spawn_single_scraper(&dir, &program_key, &leg).await;
            (program_key, res)
        });
    }

    let mut output = LegScrapeOutput::default();
    while let Some(joined) = join_set.join_next().await {
        let (program_key, result) = joined.context("Scraper worker task panicked")?;
        match result {
            Ok(mut flights) => {
                eprintln!(
                    "[runner] {} ({} -> {} on {}): {} flight(s) returned",
                    program_key,
                    query.origin,
                    query.destination,
                    query.date,
                    flights.len()
                );
                output.flights.append(&mut flights);
            }
            Err(err) => {
                let msg = format!("{:#}", err);
                eprintln!(
                    "[runner] WARN: scraper for '{}' failed ({} -> {} on {}): {}",
                    program_key, query.origin, query.destination, query.date, msg
                );
                output.errors.push((program_key, msg));
            }
        }
    }

    Ok(output)
}

async fn spawn_single_scraper(
    scraper_dir: &Path,
    program_key: &str,
    query: &LegQuery,
) -> Result<Vec<NormalizedAwardFlight>> {
    let mut cmd = build_ts_runner_command(scraper_dir);
    cmd.current_dir(scraper_dir)
        .arg("src/index.ts")
        .arg("--program")
        .arg(program_key)
        .arg("--from")
        .arg(&query.origin)
        .arg("--to")
        .arg(&query.destination)
        .arg("--date")
        .arg(&query.date)
        .arg("--adults")
        .arg(query.adults.to_string())
        .arg("--cabin")
        .arg(query.cabin.as_str())
        .arg("--max-stops")
        .arg(query.max_stops.to_string())
        .arg("--smiles-club")
        .arg(if query.use_smiles_club {
            "true"
        } else {
            "false"
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child_output = cmd
        .output()
        .await
        .with_context(|| format!("Failed to spawn TypeScript worker for '{}'", program_key))?;

    let stderr_text = String::from_utf8_lossy(&child_output.stderr);
    for line in stderr_text.lines() {
        if !line.trim().is_empty() {
            eprintln!("[scraper:{}] {}", program_key, line);
        }
    }

    if !child_output.status.success() {
        bail!(
            "Worker exited with status {} ({})",
            child_output.status,
            stderr_text.lines().last().unwrap_or("no stderr output")
        );
    }

    let stdout_text = String::from_utf8(child_output.stdout)
        .context("Scraper worker stdout was not valid UTF-8")?;
    let trimmed = stdout_text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let flights: Vec<NormalizedAwardFlight> = serde_json::from_str(trimmed).with_context(|| {
        format!(
            "Failed to parse JSON array from '{}' worker stdout: {}",
            program_key,
            truncate_for_error(trimmed, 240)
        )
    })?;

    Ok(flights)
}

fn build_ts_runner_command(_scraper_dir: &Path) -> Command {
    Command::new("node")
}

fn resolve_scraper_dir() -> Result<PathBuf> {
    let candidates = [
        PathBuf::from("scraper"),
        PathBuf::from("../scraper"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../scraper"),
    ];

    for candidate in candidates {
        if candidate.exists() && candidate.is_dir() {
            return Ok(candidate);
        }
    }

    bail!(
        "Could not locate 'scraper/' directory relative to current working directory or project root"
    )
}

fn truncate_for_error(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
