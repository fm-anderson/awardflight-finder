use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROGRAM_ORDER: &[&str] =
    &["smiles", "aeroplan", "aadvantage", "skymiles", "latam"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramConfig {
    #[serde(default)]
    pub display_name: Option<String>,
    pub native_currency: String,
    pub cost_per_1k: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_true")]
    pub use_smiles_club: bool,
    pub programs: HashMap<String, ProgramConfig>,
    pub fallback_fx_rates: HashMap<String, f64>,
}

fn default_true() -> bool {
    true
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<(Self, PathBuf)> {
        let resolved_path = resolve_config_path(path)?;
        let raw = fs::read_to_string(&resolved_path).with_context(|| {
            format!("Failed to read config file at {}", resolved_path.display())
        })?;
        let config: AppConfig = serde_json::from_str(&raw)
            .with_context(|| format!("Invalid JSON in config file {}", resolved_path.display()))?;
        Ok((config, resolved_path))
    }

    pub fn resolve_programs(&self, programs_arg: &str) -> Result<Vec<String>> {
        let trimmed = programs_arg.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
            let mut ordered = Vec::new();
            for &known in DEFAULT_PROGRAM_ORDER {
                if self.programs.contains_key(known) {
                    ordered.push(known.to_string());
                }
            }
            let mut extras: Vec<String> = self
                .programs
                .keys()
                .filter(|k| !DEFAULT_PROGRAM_ORDER.contains(&k.as_str()))
                .cloned()
                .collect();
            extras.sort();
            ordered.extend(extras);
            return Ok(ordered);
        }

        let mut selected = Vec::new();
        for part in trimmed.split(',') {
            let key = part.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            if !self.programs.contains_key(&key) {
                let mut available: Vec<&str> = self.programs.keys().map(|s| s.as_str()).collect();
                available.sort_unstable();
                bail!(
                    "Unknown program '{}'. Available programs in config.json: {}",
                    key,
                    available.join(", ")
                );
            }
            if !selected.contains(&key) {
                selected.push(key);
            }
        }

        if selected.is_empty() {
            bail!(
                "No valid programs specified in --programs '{}'",
                programs_arg
            );
        }

        Ok(selected)
    }
}

fn resolve_config_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.to_path_buf());
    }

    if path == Path::new("config.json") {
        let parent_candidate = PathBuf::from("../config.json");
        if parent_candidate.exists() {
            return Ok(parent_candidate);
        }

        let manifest_candidate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../config.json");
        if manifest_candidate.exists() {
            return Ok(manifest_candidate);
        }
    }

    bail!("Configuration file not found at '{}'", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AppConfig {
        let raw = r#"{
            "use_smiles_club": true,
            "fallback_fx_rates": { "BRL": 5.67, "CAD": 1.43 },
            "programs": {
                "smiles": { "native_currency": "BRL", "cost_per_1k": 16.0 },
                "aeroplan": { "native_currency": "CAD", "cost_per_1k": 18.5 }
            }
        }"#;
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn resolves_all_programs_in_order() {
        let cfg = sample_config();
        let programs = cfg.resolve_programs("all").unwrap();
        assert_eq!(programs, vec!["smiles", "aeroplan"]);
    }

    #[test]
    fn resolves_subset_programs() {
        let cfg = sample_config();
        let programs = cfg.resolve_programs("aeroplan, smiles").unwrap();
        assert_eq!(programs, vec!["aeroplan", "smiles"]);
    }

    #[test]
    fn rejects_unknown_program() {
        let cfg = sample_config();
        assert!(cfg.resolve_programs("unknown").is_err());
    }
}
