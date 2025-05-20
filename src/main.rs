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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dry_run = std::env::args().any(|arg| arg == "--dry-run" || arg == "-n");

    /* If run in a tty, print to the terminal. Otherwise, log to syslog. */
    if io::stdout().is_terminal() {
        SimpleLogger::new()
            .with_level(LevelFilter::Info)
            .init()?;
    } else {
        syslog::unix(Formatter3164 {
            facility: Facility::LOG_DAEMON,
            hostname: None,
            process: "silod".into(),
            pid: 0,
        })?;
    }

    let cfg = match config::Config::load(CONFIG_PATH) {
        Ok(c)  => c,
        Err(e) => {
            log::error!("failed to load {CONFIG_PATH}: {e}");
            exit(1);
        }
    };

    let mut supply = match Supply::new() {
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
