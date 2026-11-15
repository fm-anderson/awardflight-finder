use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const OPEN_ER_API_URL: &str = "https://open.er-api.com/v6/latest/USD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxSource {
    Live,
    Fallback,
}

impl FxSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            FxSource::Live => "Live (open.er-api.com)",
            FxSource::Fallback => "Fallback (config.json)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FxRates {
    pub rates: HashMap<String, f64>,
    pub source: FxSource,
}

#[derive(Debug, Deserialize)]
struct ErApiResponse {
    result: Option<String>,
    rates: Option<HashMap<String, f64>>,
}

impl FxRates {
    pub async fn fetch_or_fallback(fallback_rates: &HashMap<String, f64>) -> Self {
        let mut rates: HashMap<String, f64> = fallback_rates
            .iter()
            .map(|(k, v)| (k.to_uppercase(), *v))
            .collect();
        rates.insert("USD".to_string(), 1.0);

        match fetch_live_rates().await {
            Ok(live_map) => {
                for (currency, rate) in live_map {
                    if rate > 0.0 {
                        rates.insert(currency.to_uppercase(), rate);
                    }
                }
                rates.insert("USD".to_string(), 1.0);
                Self {
                    rates,
                    source: FxSource::Live,
                }
            }
            Err(err) => {
                eprintln!(
                    "[fx] Could not fetch live FX rates ({}). Using fallback rates from config.json.",
                    err
                );
                Self {
                    rates,
                    source: FxSource::Fallback,
                }
            }
        }
    }

    pub fn from_fallback(fallback_rates: &HashMap<String, f64>) -> Self {
        let mut rates: HashMap<String, f64> = fallback_rates
            .iter()
            .map(|(k, v)| (k.to_uppercase(), *v))
            .collect();
        rates.insert("USD".to_string(), 1.0);
        Self {
            rates,
            source: FxSource::Fallback,
        }
    }

    pub fn get_usd_to(&self, currency: &str) -> Result<f64> {
        let key = currency.trim().to_uppercase();
        if key == "USD" {
            return Ok(1.0);
        }
        match self.rates.get(&key) {
            Some(&rate) if rate > 0.0 => Ok(rate),
            Some(&rate) => bail!("Invalid non-positive FX rate {} for currency {}", rate, key),
            None => bail!(
                "Missing FX rate for currency '{}' in live/fallback table",
                key
            ),
        }
    }
}

async fn fetch_live_rates() -> Result<HashMap<String, f64>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let resp = client
        .get(OPEN_ER_API_URL)
        .send()
        .await?
        .error_for_status()?;

    let parsed: ErApiResponse = resp.json().await?;
    if let Some(status) = parsed.result.as_deref()
        && status != "success"
    {
        bail!("API returned status '{}'", status);
    }

    match parsed.rates {
        Some(rates) if !rates.is_empty() => Ok(rates),
        _ => bail!("API response contained no rates"),
    }
}
