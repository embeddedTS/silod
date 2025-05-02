mod config;
mod supply;
mod scripts;

use syslog::{Facility, Formatter3164};
use std::os::unix::process::CommandExt;
use std::process::{exit, Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    syslog::unix(Formatter3164 {
        facility: Facility::LOG_DAEMON,
        hostname: None,
        process: "silod".into(),
        pid: 0,
    })?;
    
    let cfg = match config::Config::load("/etc/silod/silo.conf") {
        Ok(c)  => c,
        Err(e) => {
            log::error!("failed to load configuration: {e}");
            exit(1);
        }
    };
    let mut supply = supply::Supply::init(&cfg)?;

    loop {
        match supply.wait_event(None)? {
            supply::Event::None => continue,

            e @ supply::Event::Critical => {
                scripts::run(e)?;
                log::warn!("critical event processed — shutting down");

                // exec returns only on error
                let err = Command::new("shutdown")
                    .args(["-p", "now"])
                    .exec();

                log::error!("failed to exec shutdown: {err}");
                exit(1);
            }

            e => scripts::run(e)?,
        }
    }
}
