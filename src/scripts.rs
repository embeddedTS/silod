use std::{fs, io, path::PathBuf, process::Command};
use crate::supply::Event;

const SCRIPT_DIR: &str = "/etc/silod/scripts.d";


pub fn run(ev: Event) -> io::Result<()> {
    let subdir = match ev {
        Event::PowerFail     => "power-fail",
        Event::PowerRestored => "power-restored",
        Event::FullyCharged  => "fully-charged",
        Event::Critical      => "critical",
        /* We do not run any scripts for the initial charge, or if there is no change */
        Event::InitialCharge => return Ok(()),
        Event::NoChange      => return Ok(()),
    };

    let dir_path = PathBuf::from(SCRIPT_DIR).join(subdir);

    let entries = match fs::read_dir(&dir_path) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            log::warn!("script directory {dir_path:?} not found");
            return Ok(());
        },
        Err(e) => {
            log::error!("can't read script directory {dir_path:?}: {e}");
            return Ok(());
        }
    };

    for entry in entries {
        let path = match entry {
            Ok(e) => e.path(),
            Err(e) => {
                log::error!("error reading script in {dir_path:?}: {e}");
                continue;
            }
        };

        match Command::new(&path).status() {
            Ok(st) if st.success() => {}
            Ok(st) => log::error!("{path:?} exited with status {st}"),
            Err(e) => log::error!("failed to exec {path:?}: {e}"),
        }
    }

    Ok(())
}
