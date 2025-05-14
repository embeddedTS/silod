use std::io::{self, ErrorKind};
use serde::Deserialize;

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub critical_pct: u32,
    pub startup_charge_current_ma: Option<u32>,
    pub min_power_on_pct: Option<u32>,
    pub enable_charging: Option<bool>,
}

impl Config {
    pub fn load(path: &str) -> io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        toml::from_str(&data)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }
}
