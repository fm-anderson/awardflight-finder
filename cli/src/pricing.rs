use crate::config::AppConfig;
use crate::fx::FxRates;
use crate::models::{CabinFilter, NormalizedAwardFlight, PartnerOverlap, PricedAwardFlight};
use anyhow::{Context, Result};
use std::collections::HashMap;

// sort flights by lowest SUBTOTAL.
pub fn price_and_rank_flights(
    flights: Vec<NormalizedAwardFlight>,
    config: &AppConfig,
    fx: &FxRates,
    cabin_filter: CabinFilter,
    max_stops: u32,
) -> Result<Vec<PricedAwardFlight>> {
    let mut priced: Vec<PricedAwardFlight> = Vec::new();

    for flight in flights {
        if !cabin_filter.matches(flight.cabin) {
            continue;
        }
        if flight.stops > max_stops {
            continue;
        }

        let prog_key = flight.program_key.to_lowercase();
        let prog_cfg = config.programs.get(&prog_key).with_context(|| {
            format!(
                "Flight program_key '{}' not found in config.json programs",
                flight.program_key
            )
        })?;

        let currency = if flight.native_currency.trim().is_empty() {
            prog_cfg.native_currency.as_str()
        } else {
            flight.native_currency.as_str()
        };

        let fx_rate = fx.get_usd_to(currency)?;
        let cost_per_1k = prog_cfg.cost_per_1k;

        let subtotal_native = (flight.miles / 1000.0) * cost_per_1k;
        let subtotal_usd = subtotal_native / fx_rate;

        let (taxes_usd, total_native, total_usd) = match flight.taxes_native {
            Some(taxes_nat) => {
                let t_usd = taxes_nat / fx_rate;
                let tot_nat = subtotal_native + taxes_nat;
                let tot_usd = subtotal_usd + t_usd;
                (Some(t_usd), Some(tot_nat), Some(tot_usd))
            }
            None => (None, None, None),
        };

        priced.push(PricedAwardFlight {
            flight,
            cost_per_1k,
            fx_rate_usd_to_native: fx_rate,
            subtotal_native,
            subtotal_usd,
            taxes_usd,
            total_native,
            total_usd,
            partner_overlaps: Vec::new(),
        });
    }

    annotate_partner_overlaps(&mut priced);

    priced.sort_by(|a, b| {
        a.subtotal_usd
            .total_cmp(&b.subtotal_usd)
            .then_with(|| {
                let a_tot = a.total_usd.unwrap_or(a.subtotal_usd);
                let b_tot = b.total_usd.unwrap_or(b.subtotal_usd);
                a_tot.total_cmp(&b_tot)
            })
            .then_with(|| a.flight.duration_minutes.cmp(&b.flight.duration_minutes))
            .then_with(|| a.flight.flight_numbers.cmp(&b.flight.flight_numbers))
    });

    Ok(priced)
}

fn physical_flight_key(flight: &NormalizedAwardFlight) -> (String, String, String) {
    let normalized_numbers: String = flight
        .flight_numbers
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    let normalized_departure = flight.departure.trim().to_string();
    let normalized_cabin = flight.cabin.as_str().to_string();
    (normalized_numbers, normalized_departure, normalized_cabin)
}

fn annotate_partner_overlaps(priced: &mut [PricedAwardFlight]) {
    let mut groups: HashMap<(String, String, String), Vec<usize>> = HashMap::new();
    for (idx, item) in priced.iter().enumerate() {
        groups
            .entry(physical_flight_key(&item.flight))
            .or_default()
            .push(idx);
    }

    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }

        for &idx in indices {
            let current_prog_key = priced[idx].flight.program_key.to_lowercase();
            let mut overlaps: Vec<PartnerOverlap> = indices
                .iter()
                .filter(|&&other_idx| {
                    other_idx != idx
                        && priced[other_idx].flight.program_key.to_lowercase() != current_prog_key
                })
                .map(|&other_idx| PartnerOverlap {
                    program: priced[other_idx].flight.program.clone(),
                    subtotal_usd: priced[other_idx].subtotal_usd,
                    miles: priced[other_idx].flight.miles,
                })
                .collect();

            overlaps.sort_by(|a, b| a.subtotal_usd.total_cmp(&b.subtotal_usd));
            priced[idx].partner_overlaps = overlaps;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FlightCabin;

    fn test_config() -> AppConfig {
        let raw = r#"{
            "use_smiles_club": true,
            "fallback_fx_rates": { "BRL": 5.0, "CAD": 1.4 },
            "programs": {
                "smiles": { "native_currency": "BRL", "cost_per_1k": 15.0 },
                "aeroplan": { "native_currency": "CAD", "cost_per_1k": 14.0 }
            }
        }"#;
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn calculates_subtotal_total_and_partner_arbitrage() {
        let cfg = test_config();
        let fx = FxRates::from_fallback(&cfg.fallback_fx_rates);

        let flights = vec![
            NormalizedAwardFlight {
                program: "Gol Smiles".to_string(),
                program_key: "smiles".to_string(),
                operating_airline: "GOL".to_string(),
                flight_numbers: "G37602".to_string(),
                origin: "GRU".to_string(),
                destination: "MCO".to_string(),
                departure: "2026-12-10T10:00:00".to_string(),
                arrival: "2026-12-10T17:00:00".to_string(),
                duration_minutes: 540,
                stops: 0,
                cabin: FlightCabin::Economy,
                mixed_cabin: false,
                miles: 50_000.0,
                rate_type: "Clube Smiles".to_string(),
                taxes_native: Some(250.0),
                native_currency: "BRL".to_string(),
                deep_link: "https://smiles.com.br".to_string(),
            },
            NormalizedAwardFlight {
                program: "Air Canada Aeroplan".to_string(),
                program_key: "aeroplan".to_string(),
                operating_airline: "GOL".to_string(),
                flight_numbers: "G3 7602".to_string(),
                origin: "GRU".to_string(),
                destination: "MCO".to_string(),
                departure: "2026-12-10T10:00:00".to_string(),
                arrival: "2026-12-10T17:00:00".to_string(),
                duration_minutes: 540,
                stops: 0,
                cabin: FlightCabin::Economy,
                mixed_cabin: false,
                miles: 25_000.0,
                rate_type: "Aeroplan Standard".to_string(),
                taxes_native: None,
                native_currency: "CAD".to_string(),
                deep_link: "https://aircanada.com".to_string(),
            },
        ];

        let ranked = price_and_rank_flights(flights, &cfg, &fx, CabinFilter::All, 0).unwrap();
        assert_eq!(ranked.len(), 2);

        assert_eq!(ranked[0].flight.program_key, "smiles");
        assert!((ranked[0].subtotal_native - 750.0).abs() < 1e-6);
        assert!((ranked[0].subtotal_usd - 150.0).abs() < 1e-6);
        assert!((ranked[0].total_usd.unwrap() - 200.0).abs() < 1e-6);
        assert_eq!(ranked[0].partner_overlaps.len(), 1);
        assert_eq!(ranked[0].partner_overlaps[0].program, "Air Canada Aeroplan");

        assert_eq!(ranked[1].flight.program_key, "aeroplan");
        assert!((ranked[1].subtotal_usd - 250.0).abs() < 1e-6);
        assert!(ranked[1].total_usd.is_none());
        assert_eq!(ranked[1].partner_overlaps.len(), 1);
        assert_eq!(ranked[1].partner_overlaps[0].program, "Gol Smiles");
    }
}
