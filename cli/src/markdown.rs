use crate::fx::FxRates;
use crate::models::{CabinFilter, PricedAwardFlight};
use chrono::Local;
use std::fmt::Write;

pub struct ReportMetadata<'a> {
    pub origin: &'a str,
    pub destination: &'a str,
    pub depart_date: &'a str,
    pub return_date: Option<&'a str>,
    pub adults: u8,
    pub cabin: CabinFilter,
    pub max_stops: u32,
    pub programs: &'a [String],
    pub fx: &'a FxRates,
}

pub struct LegReport<'a> {
    pub title: &'a str,
    pub origin: &'a str,
    pub destination: &'a str,
    pub date: &'a str,
    pub flights: &'a [PricedAwardFlight],
    pub errors: &'a [(String, String)],
}

pub fn render_markdown_report(
    meta: &ReportMetadata<'_>,
    outbound: &LegReport<'_>,
    inbound: Option<&LegReport<'_>>,
) -> String {
    let mut out = String::new();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S %Z").to_string();

    let route_summary = if meta.return_date.is_some() {
        format!("{} <-> {}", meta.origin, meta.destination)
    } else {
        format!("{} -> {}", meta.origin, meta.destination)
    };

    let _ = writeln!(out, "# Award Flight Search Report: {}", route_summary);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Search Metadata");
    let _ = writeln!(out);
    let _ = writeln!(out, "- **Route:** `{}`", route_summary);
    match meta.return_date {
        Some(ret) => {
            let _ = writeln!(
                out,
                "- **Dates:** Outbound `{}` | Return `{}`",
                meta.depart_date, ret
            );
        }
        None => {
            let _ = writeln!(out, "- **Date:** Outbound `{}` (One-Way)", meta.depart_date);
        }
    }
    let _ = writeln!(out, "- **Passengers:** {} Adult(s)", meta.adults);
    let _ = writeln!(out, "- **Cabin Filter:** `{}`", meta.cabin);
    let _ = writeln!(
        out,
        "- **Max Stops:** {}",
        if meta.max_stops == 0 {
            "0 (Direct / Nonstop only)".to_string()
        } else {
            format!("Up to {} stop(s)", meta.max_stops)
        }
    );
    let _ = writeln!(
        out,
        "- **Programs Queried:** `{}`",
        meta.programs.join(", ")
    );
    let _ = writeln!(out, "- **Generated At:** `{}`", timestamp);
    let _ = writeln!(
        out,
        "- **Active FX Rates ({}):** {}",
        meta.fx.source.as_str(),
        format_fx_summary(meta.fx)
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "---");
    let _ = writeln!(out);

    render_leg_section(&mut out, outbound);

    if let Some(return_leg) = inbound {
        let _ = writeln!(out, "---");
        let _ = writeln!(out);
        render_leg_section(&mut out, return_leg);
    }

    out
}

fn render_leg_section(out: &mut String, leg: &LegReport<'_>) {
    let _ = writeln!(
        out,
        "## {} ({} -> {} on {})",
        leg.title, leg.origin, leg.destination, leg.date
    );
    let _ = writeln!(out);

    if !leg.errors.is_empty() {
        let _ = writeln!(out, "> [!WARNING]");
        let _ = writeln!(out, "> Some program workers reported errors on this leg:");
        for (prog, err) in leg.errors {
            let _ = writeln!(out, "> - **{}:** {}", prog, err);
        }
        let _ = writeln!(out);
    }

    if leg.flights.is_empty() {
        let _ = writeln!(
            out,
            "_No matching award flights found for {} -> {} on {}._",
            leg.origin, leg.destination, leg.date
        );
        let _ = writeln!(out);
        return;
    }

    let _ = writeln!(
        out,
        "_Found **{}** option(s), sorted by lowest **SUBTOTAL (USD)**:_",
        leg.flights.len()
    );
    let _ = writeln!(out);

    for (idx, item) in leg.flights.iter().enumerate() {
        let f = &item.flight;
        let rank = idx + 1;
        let stops_label = match f.stops {
            0 => "Direct".to_string(),
            1 => "1 stop".to_string(),
            n => format!("{} stops", n),
        };
        let cabin_label = if f.mixed_cabin {
            format!("{} (Mixed Cabin)", f.cabin)
        } else {
            f.cabin.to_string()
        };

        let _ = writeln!(
            out,
            "### {}. {} ({}) — {} | {}",
            rank, f.operating_airline, f.flight_numbers, cabin_label, stops_label
        );
        let _ = writeln!(out);

        if !item.partner_overlaps.is_empty() {
            let comparisons: Vec<String> = item
                .partner_overlaps
                .iter()
                .map(|p| {
                    format!(
                        "**{}** ({} miles / USD ${:.2} subtotal)",
                        p.program,
                        format_miles(p.miles),
                        p.subtotal_usd
                    )
                })
                .collect();
            let _ = writeln!(
                out,
                "- **Partner Arbitrage Flag:** Same physical flight (`{}`) also available via {}",
                f.flight_numbers,
                comparisons.join(", ")
            );
        }

        let _ = writeln!(
            out,
            "- **Operating Airline & Flight(s):** {} (`{}`)",
            f.operating_airline, f.flight_numbers
        );
        let _ = writeln!(
            out,
            "- **Cabin & Stops:** `{}` | {}",
            cabin_label, stops_label
        );
        let _ = writeln!(
            out,
            "- **Program & Rate Tier:** {} ({})",
            f.program, f.rate_type
        );
        let _ = writeln!(
            out,
            "- **Schedule:** `{}` ({}) -> `{}` ({}) | Duration: {}",
            f.origin,
            f.departure,
            f.destination,
            f.arrival,
            format_duration(f.duration_minutes)
        );
        let _ = writeln!(
            out,
            "- **Miles Required:** {} miles (valued at {} {:.2} / 1,000 miles)",
            format_miles(f.miles),
            f.native_currency,
            item.cost_per_1k
        );
        let _ = writeln!(
            out,
            "- **SUBTOTAL:** {}",
            format_dual_currency(item.subtotal_usd, item.subtotal_native, &f.native_currency)
        );

        if let (Some(taxes_nat), Some(taxes_usd), Some(tot_nat), Some(tot_usd)) = (
            f.taxes_native,
            item.taxes_usd,
            item.total_native,
            item.total_usd,
        ) {
            let _ = writeln!(
                out,
                "- **Fees / Taxes:** {}",
                format_dual_currency(taxes_usd, taxes_nat, &f.native_currency)
            );
            let _ = writeln!(
                out,
                "- **TOTAL:** **{}**",
                format_dual_currency(tot_usd, tot_nat, &f.native_currency)
            );
        }

        let _ = writeln!(
            out,
            "- **Booking Link:** [Search on {}]({})",
            f.program, f.deep_link
        );
        let _ = writeln!(out);
    }
}

fn format_dual_currency(usd: f64, native: f64, native_currency: &str) -> String {
    let curr = native_currency.trim().to_uppercase();
    if curr == "USD" {
        format!("USD ${:.2} ({} {:.2})", usd, curr, native)
    } else {
        format!("USD ${:.2} | {} {:.2}", usd, curr, native)
    }
}

fn format_fx_summary(fx: &FxRates) -> String {
    let mut keys: Vec<&String> = fx
        .rates
        .keys()
        .filter(|k| k.as_str() == "BRL" || k.as_str() == "CAD" || k.as_str() == "USD")
        .collect();
    keys.sort();

    if keys.is_empty() {
        return "1 USD = 1.00 USD".to_string();
    }

    let parts: Vec<String> = keys
        .into_iter()
        .filter(|k| k.as_str() != "USD")
        .filter_map(|k| fx.rates.get(k).map(|v| format!("1 USD = {:.4} {}", v, k)))
        .collect();

    if parts.is_empty() {
        "1 USD = 1.00 USD".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_duration(minutes: u32) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 {
        format!("{}h {:02}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

fn format_miles(miles: f64) -> String {
    let rounded = miles.round() as i64;
    let s = rounded.abs().to_string();
    let mut out = String::new();
    for (count, ch) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let formatted: String = out.chars().rev().collect();
    if rounded < 0 {
        format!("-{}", formatted)
    } else {
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FlightCabin, NormalizedAwardFlight, PartnerOverlap};
    use std::collections::HashMap;

    #[test]
    fn omits_taxes_and_total_when_taxes_native_is_null() {
        let mut fallback = HashMap::new();
        fallback.insert("BRL".to_string(), 5.0);
        fallback.insert("CAD".to_string(), 1.4);
        let fx = FxRates::from_fallback(&fallback);

        let flight_with_tax = PricedAwardFlight {
            flight: NormalizedAwardFlight {
                program: "Gol Smiles".to_string(),
                program_key: "smiles".to_string(),
                operating_airline: "GOL".to_string(),
                flight_numbers: "G37602".to_string(),
                origin: "GRU".to_string(),
                destination: "MCO".to_string(),
                departure: "2026-12-10T10:00:00".to_string(),
                arrival: "2026-12-10T18:30:00".to_string(),
                duration_minutes: 510,
                stops: 0,
                cabin: FlightCabin::Economy,
                mixed_cabin: false,
                miles: 50_000.0,
                rate_type: "Clube Smiles".to_string(),
                taxes_native: Some(250.0),
                native_currency: "BRL".to_string(),
                deep_link: "https://www.smiles.com.br".to_string(),
            },
            cost_per_1k: 16.0,
            fx_rate_usd_to_native: 5.0,
            subtotal_native: 800.0,
            subtotal_usd: 160.0,
            taxes_usd: Some(50.0),
            total_native: Some(1050.0),
            total_usd: Some(210.0),
            partner_overlaps: vec![PartnerOverlap {
                program: "Air Canada Aeroplan".to_string(),
                subtotal_usd: 200.0,
                miles: 25_000.0,
            }],
        };

        let flight_without_tax = PricedAwardFlight {
            flight: NormalizedAwardFlight {
                program: "Air Canada Aeroplan".to_string(),
                program_key: "aeroplan".to_string(),
                operating_airline: "GOL".to_string(),
                flight_numbers: "G37602".to_string(),
                origin: "GRU".to_string(),
                destination: "MCO".to_string(),
                departure: "2026-12-10T10:00:00".to_string(),
                arrival: "2026-12-10T18:30:00".to_string(),
                duration_minutes: 510,
                stops: 0,
                cabin: FlightCabin::Economy,
                mixed_cabin: false,
                miles: 25_000.0,
                rate_type: "Aeroplan Standard".to_string(),
                taxes_native: None,
                native_currency: "CAD".to_string(),
                deep_link: "https://www.aircanada.com".to_string(),
            },
            cost_per_1k: 18.5,
            fx_rate_usd_to_native: 1.4,
            subtotal_native: 462.5,
            subtotal_usd: 330.36,
            taxes_usd: None,
            total_native: None,
            total_usd: None,
            partner_overlaps: vec![],
        };

        let programs = vec!["smiles".to_string(), "aeroplan".to_string()];
        let meta = ReportMetadata {
            origin: "GRU",
            destination: "MCO",
            depart_date: "2026-12-10",
            return_date: None,
            adults: 1,
            cabin: CabinFilter::All,
            max_stops: 0,
            programs: &programs,
            fx: &fx,
        };

        let outbound = LegReport {
            title: "Outbound",
            origin: "GRU",
            destination: "MCO",
            date: "2026-12-10",
            flights: &[flight_with_tax, flight_without_tax],
            errors: &[],
        };

        let md = render_markdown_report(&meta, &outbound, None);
        assert!(md.contains("## Outbound (GRU -> MCO on 2026-12-10)"));
        assert!(md.contains("Partner Arbitrage Flag"));
        assert!(md.contains("USD $160.00 | BRL 800.00"));
        assert!(md.contains("- **Fees / Taxes:** USD $50.00 | BRL 250.00"));
        assert!(md.contains("- **TOTAL:** **USD $210.00 | BRL 1050.00**"));
        assert!(md.contains("[Search on Gol Smiles](https://www.smiles.com.br)"));
        assert!(md.contains("USD $330.36 | CAD 462.50"));
        assert!(!md.contains("CAD 462.50**"));
    }
}
