use std::fs;
use std::io;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::uevent::UeventListener;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Event {
    NoChange,
    PowerFail,
    PowerRestored,
    FullyCharged,
    InitialCharge,
    Critical,
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Event::NoChange => "NoChange",
            Event::PowerFail => "PowerFail",
            Event::PowerRestored => "PowerRestored",
            Event::FullyCharged => "FullyCharged",
            Event::InitialCharge => "InitialCharge",
            Event::Critical => "Critical",
        };
        write!(f, "{s}")
    }
}

pub struct Supply {
    driver_name: String,
    base_path: PathBuf,
    uevent: UeventListener,
    current_event: Event,
}

impl Supply {
    pub fn new() -> io::Result<Self> {
        let driver_name = "silo".to_string();
        let base_path = PathBuf::from("/sys/class/power_supply").join(&driver_name);

        Self::verify_power_supply(&base_path)?;

        let uevent = UeventListener::connect()?;
        Ok(Supply {
            driver_name,
            base_path,
            uevent,
            current_event: Event::InitialCharge,
        })
    }

    pub fn wait_event(&mut self) -> io::Result<Event> {
        let ret = self._wait_event()?;
        if ret != Event::NoChange {
            self.current_event = ret;
            log::info!(
                "New event: {}",
                self.current_event
            );
        }
        Ok(ret)
    }

    fn _wait_event(&mut self) -> io::Result<Event> {
        /* Blocks until uevent notifies us of a 'change' for this driver. */
        let evt = match self.uevent.wait_event(&self.driver_name)? {
            Some(e) => e.env,
            None    => return Ok(Event::NoChange),
        };

        let capacity = evt.get("POWER_SUPPLY_CAPACITY")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing capacity"))?
            .parse::<u32>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad capacity: {e}")))?;

        let crit_pct = evt.get("POWER_SUPPLY_CAPACITY_ALERT_MIN")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing POWER_SUPPLY_CAPACITY_ALERT_MIN"))?
            .parse::<u32>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid POWER_SUPPLY_CAPACITY_ALERT_MIN: {e}")))?;

        let online = evt.get("POWER_SUPPLY_ONLINE")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing POWER_SUPPLY_ONLINE"))?
            .parse::<u32>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid POWER_SUPPLY_ONLINE: {e}")))
            .map(|v| v == 1)?;

        let status = evt.get("POWER_SUPPLY_STATUS")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing POWER_SUPPLY_STATUS"))?
            .to_string();

        log::info!(
            "wait_event: status='{status}', capacity={capacity}%, critical_threshold={crit_pct}%, online={online}, last_event={}",
            self.current_event
        );

        let ret = if capacity < crit_pct && self.current_event != Event::InitialCharge {
            /* If this isn't our initial startup, and our capacity is < critical, we always
             * consider this critical regardless of any other state */
            Event::Critical
        } else {
            match self.current_event {
                Event::InitialCharge => {
                    if online && (status == "Full" || capacity == 100) {
                        Event::FullyCharged
                    } else if !online {
                        Event::PowerFail
                    } else {
                        Event::NoChange
                    }
                }
                Event::FullyCharged => {
                    if online {
                        Event::NoChange
                    } else {
                        Event::PowerFail
                    }
                }
                Event::PowerFail => {
                    if online {
                        Event::PowerRestored
                    } else {
                        Event::NoChange
                    }
                }
                Event::PowerRestored => {
                    if online && capacity == 100 {
                        Event::FullyCharged
                    } else if !online {
                        Event::PowerFail
                    } else {
                        Event::NoChange
                    }
                }
                _ => Event::NoChange
            }
        };

        Ok(ret)
    }

    pub fn apply_config(&self, cfg: &Config) -> io::Result<()> {
        if let Some(enable_charging) = cfg.enable_charging {
            let key = "charge_behaviour";
            let value = if enable_charging { "auto" }
                        else { "inhibit-charging" };
            
            let res = self.sysfs_write_str(key, value);
            self.log_attr_result_str(key, value, res);
        }

        if let Some(min_power_on_pct) = cfg.min_power_on_pct {
            let key = "device/min_power_on_pct";
            let value = min_power_on_pct;
            
            let res = self.sysfs_write_u32(key, value);
            self.log_attr_result(key, value, res);
        }

        if let Some(startup_charge_current_ma) = cfg.startup_charge_current_ma {
            let key = "startup_charge_current_ma";
            let value = startup_charge_current_ma;
            
            let res = self.sysfs_write_u32(key, value);
            self.log_attr_result(key, value, res);
        } else if let Some(startup_charge_current_pct) = cfg.startup_charge_current_pct {
            let max_ma = self.sysfs_read_u32("charge_current_max")?;
            let key = "startup_charge_current_ma";
            let value = (startup_charge_current_pct.saturating_mul(max_ma) / 100).clamp(0, max_ma);
            let res = self.sysfs_write_u32(key, value);
            self.log_attr_result(key, value, res);
        }

        let key = "capacity_alert_min";
        let value = cfg.critical_pct;
        let res = self.sysfs_write_u32(key, value);
        self.log_attr_result(key, value, res);

        Ok(())
    }

    fn verify_power_supply(dir: &Path) -> io::Result<()> {
        if dir.is_dir() {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn log_attr_result_str(&self, key: &str, requested: &str, res: io::Result<String>) {
        match res {
            Ok(actual) if actual == requested => {
                log::info!("{key} set to {actual}");
            }
            Ok(actual) => {
                log::warn!("requested {key}={requested}, but driver reports {actual}");
            }
            Err(e) => {
                log::warn!("failed to set/read {key}: {e}");
            }
        }
    }

    fn log_attr_result(&self, key: &str, requested: u32, res: io::Result<u32>) {
        match res {
            Ok(actual) if actual == requested => {
                log::info!("{key} set to {actual}");
            }
            Ok(actual) => {
                log::warn!("requested {key}={requested}, but driver reports {actual}");
            }
            Err(e) => {
                log::warn!("failed to set/read {key}: {e}");
            }
        }
    }

    fn sysfs_read_u32(&self, attr: &str) -> io::Result<u32> {
        let path = self.base_path.join(attr);
        let value = fs::read_to_string(&path)?;
        let value = value.trim().parse::<u32>().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Failed to parse '{value}': {e}")))?;

        Ok(value)
    }

    fn sysfs_read_str(&self, attr: &str) -> io::Result<String> {
        let path = self.base_path.join(attr);
        let content = fs::read_to_string(&path)?;
        Ok(content.trim().to_string())
    }

    fn sysfs_write_u32(&self, attr: &str, requested: u32) -> io::Result<u32> {
        let path = self.base_path.join(attr);
        fs::write(&path, format!("{requested}\n"))?;
        self.sysfs_read_u32(attr)
    }

    fn sysfs_write_str(&self, attr: &str, requested: &str) -> io::Result<String> {
        let path = self.base_path.join(attr);
        fs::write(&path, format!("{requested}\n"))?;

        let actual = self.sysfs_read_str(attr)?;
        Ok(actual)
    }
}
