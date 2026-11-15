use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[clap(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CabinFilter {
    All,
    Economy,
    PremiumEconomy,
    Business,
    First,
}

impl CabinFilter {
    pub fn matches(&self, cabin: FlightCabin) -> bool {
        match self {
            CabinFilter::All => true,
            CabinFilter::Economy => cabin == FlightCabin::Economy,
            CabinFilter::PremiumEconomy => cabin == FlightCabin::PremiumEconomy,
            CabinFilter::Business => cabin == FlightCabin::Business,
            CabinFilter::First => cabin == FlightCabin::First,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CabinFilter::All => "ALL",
            CabinFilter::Economy => "ECONOMY",
            CabinFilter::PremiumEconomy => "PREMIUM_ECONOMY",
            CabinFilter::Business => "BUSINESS",
            CabinFilter::First => "FIRST",
        }
    }
}

impl fmt::Display for CabinFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlightCabin {
    Economy,
    PremiumEconomy,
    Business,
    First,
}

impl FlightCabin {
    pub fn as_str(&self) -> &'static str {
        match self {
            FlightCabin::Economy => "ECONOMY",
            FlightCabin::PremiumEconomy => "PREMIUM_ECONOMY",
            FlightCabin::Business => "BUSINESS",
            FlightCabin::First => "FIRST",
        }
    }
}

impl fmt::Display for FlightCabin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedAwardFlight {
    pub program: String,
    pub program_key: String,
    pub operating_airline: String,
    pub flight_numbers: String,
    pub origin: String,
    pub destination: String,
    pub departure: String,
    pub arrival: String,
    pub duration_minutes: u32,
    pub stops: u32,
    pub cabin: FlightCabin,
    pub mixed_cabin: bool,
    pub miles: f64,
    pub rate_type: String,
    pub taxes_native: Option<f64>,
    pub native_currency: String,
    pub deep_link: String,
}

#[derive(Debug, Clone)]
pub struct PartnerOverlap {
    pub program: String,
    pub subtotal_usd: f64,
    pub miles: f64,
}

#[derive(Debug, Clone)]
pub struct PricedAwardFlight {
    pub flight: NormalizedAwardFlight,
    pub cost_per_1k: f64,
    pub fx_rate_usd_to_native: f64,
    pub subtotal_native: f64,
    pub subtotal_usd: f64,
    pub taxes_usd: Option<f64>,
    pub total_native: Option<f64>,
    pub total_usd: Option<f64>,
    pub partner_overlaps: Vec<PartnerOverlap>,
}
