use std::io::{self, ErrorKind};
use serde::Deserialize;

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub charge_current_ma: u32,
    pub critical_pct:      u8,
}
impl Config {
    pub fn load(path: &str) -> io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        toml::from_str(&data)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }
}
