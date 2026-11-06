mod config;
mod supply;
mod scripts;
mod uevent;

use std::io::{self, IsTerminal};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use syslog::{Facility, Formatter3164};
use std::os::unix::process::CommandExt;
use std::process::{exit, Command};

use supply::{Supply, Event};

const CONFIG_PATH: &str = "/etc/silod/silo.toml";

fn configured_log_level(value: Option<&str>) -> LevelFilter {
    value
        .and_then(|level| level.parse().ok())
        .unwrap_or(LevelFilter::Info)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dry_run = std::env::args().any(|arg| arg == "--dry-run" || arg == "-n");
    let log_level = configured_log_level(std::env::var("RUST_LOG").ok().as_deref());

    /* If run in a tty, print to the terminal. Otherwise, log to syslog. */
    if io::stdout().is_terminal() {
        SimpleLogger::new()
            .with_level(log_level)
            .init()?;
    } else {
        let logger = syslog::unix(Formatter3164 {
            facility: Facility::LOG_DAEMON,
            hostname: None,
            process: "silod".into(),
            pid: 0,
        })?;
        log::set_boxed_logger(Box::new(syslog::BasicLogger::new(logger)))?;
        log::set_max_level(log_level);
    }

    let cfg = match config::Config::load(CONFIG_PATH) {
        Ok(c)  => c,
        Err(e) => {
            log::error!("failed to load {CONFIG_PATH}: {e}");
            exit(1);
        }
    };

    let mut supply = match Supply::new(cfg.critical_pct) {
        Ok(s) => s,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                log::info!("no silo hardware found — exiting");
                exit(0);
            } else {
                log::error!("failed to initialize supply: {e}");
                exit(1);
            }
        }
    };
    supply.apply_config(&cfg)?;

    loop {
        match supply.wait_event()? {
            supply::Event::NoChange => continue,

            e @ Event::Critical => {
                if dry_run {
                    log::warn!("(dry-run) critical event would have triggered shutdown and script");
                    continue;
                }

                scripts::run(e)?;
                log::warn!("critical event processed — shutting down");

                // exec returns only on error
                let err = Command::new("shutdown")
                    .args(["-P", "now"])
                    .exec();

                log::error!("failed to exec shutdown: {err}");
                exit(1);
            }

            e =>  {
                if dry_run {
                    log::info!("(dry-run) would run script for event: {e:?}");
                    continue;
                }

                scripts::run(e)?
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_defaults_to_info() {
        assert_eq!(configured_log_level(None), LevelFilter::Info);
    }

    #[test]
    fn log_level_uses_rust_log_value() {
        assert_eq!(configured_log_level(Some("debug")), LevelFilter::Debug);
    }

    #[test]
    fn invalid_log_level_defaults_to_info() {
        assert_eq!(configured_log_level(Some("silod=debug")), LevelFilter::Info);
    }
}
