use std::{fs, io, process::Command};
use crate::supply::Event;

const SCRIPT_DIR: &str = "/etc/silod/scripts.d";

pub fn run(ev: Event) -> io::Result<()> {
    let arg = match ev {
        Event::PowerFail     => "power-fail",
        Event::PowerRestored => "power-restored",
        Event::FullyCharged  => "fully-charged",
        Event::Critical      => "critical",
        Event::None          => return Ok(()),
    };

    let entries = match fs::read_dir(SCRIPT_DIR) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(e),
        Err(e) => {
            log::error!("can’t read {SCRIPT_DIR}: {e}");
            return Ok(());
        }
    };

    for entry in entries {
        let path = match entry {
            Ok(e) => e.path(),
            Err(e) => { log::error!("dir entry error in {SCRIPT_DIR}: {e}"); continue; }
        };

        match Command::new(&path).arg(arg).status() {
            Ok(st) if st.success() => {}
            Ok(st) => log::error!("{:?} exited with {st}", path),
            Err(e) => log::error!("failed to exec {:?}: {e}", path),
        }
    }

    Ok(())
}
