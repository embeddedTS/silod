use std::{
    fs,
    io,
    path::Path,
    time::Duration,
};

use neli::{
    consts::socket::NlFamily,
    nl::{Nlmsghdr, NlPayload},
    socket::NlSocketHandle,
};

use crate::config::Config;

const DT_TABLE: &[(&str, &str)] = &[("technologic,tssilo", "tssilo")];

const DT_ROOTS: &[&str] = &["/proc/device-tree", "/sys/firmware/devicetree/base"];
const PS_DIR: &str = "/sys/class/power_supply";
const UEVENT_MCAST_GRP: u32 = 1;
const NAME_KEY: &str = "POWER_SUPPLY_NAME=";
const EVENT_KEY: &str = "SILO_EVENT=";

#[derive(Debug, Copy, Clone)]
pub enum Event {
    None,
    PowerFail,
    PowerRestored,
    FullyCharged,
    Critical,
}

pub struct Supply {
    sock: NlSocketHandle,
    sysname: String,
}

impl Supply {
    pub fn init(cfg: &Config) -> io::Result<Self> {
        let sysname = find_dt_compatible()?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no silo in DT"))?
            .to_owned();

        check_sysfs_device(&sysname)?;

        let _ = fs::write(
            format!("{PS_DIR}/{sysname}/charge_current_ma"),
            format!("{}\n", cfg.charge_current_ma),
        );

        let sock = NlSocketHandle::connect(NlFamily::Generic, None, &[])
            .map_err(to_io)?;
        sock.add_mcast_membership(&[UEVENT_MCAST_GRP])
            .map_err(to_io)?;

        Ok(Supply { sock, sysname })
    }

    pub fn wait_event(&mut self, _timeout: Option<Duration>) -> io::Result<Event> {
        let nlhdr: Nlmsghdr<u16, Vec<u8>> =
            self.sock.recv().map_err(to_io)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::WouldBlock, "no message available")
            })?;

        let payload = match nlhdr.nl_payload {
            NlPayload::Payload(p) => p,
            _ => return Ok(Event::None),
        };

        parse_blob(&payload, &self.sysname)
    }
}

fn to_io<E: std::error::Error + Send + Sync + 'static>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e)
}

fn parse_blob(payload: &[u8], sysname: &str) -> io::Result<Event> {
    let mut match_device = false;
    let mut ev = Event::None;

    for s in payload.split(|b| *b == 0) {
        let s = std::str::from_utf8(s).unwrap_or("");
        if let Some(rest) = s.strip_prefix(NAME_KEY) {
            match_device = rest == sysname;
        } else if let Some(val) = s.strip_prefix(EVENT_KEY) {
            ev = match val {
                "POWER_FAIL"      => Event::PowerFail,
                "POWER_RESTORED"  => Event::PowerRestored,
                "FULLY_CHARGED"   => Event::FullyCharged,
                "CRITICAL"        => Event::Critical,
                _                 => Event::None,
            }
        }
    }
    Ok(if match_device { ev } else { Event::None })
}

fn find_dt_compatible() -> io::Result<Option<&'static str>> {
    for root in DT_ROOTS {
        if !Path::new(root).exists() {
            continue;
        }
        if let Some(name) = recurse_dt(Path::new(root))? {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

fn recurse_dt(dir: &Path) -> io::Result<Option<&'static str>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let node = entry.path();

        // read compatible property
        let compat = fs::read(node.join("compatible")).unwrap_or_default();
        for (compat_str, drv) in DT_TABLE {
            let mut p = compat.as_slice();
            while let Some(pos) = p.iter().position(|b| *b == 0) {
                if &p[..pos] == compat_str.as_bytes()
                    && status_ok(&node)?
                {
                    return Ok(Some(*drv));
                }
                p = &p[pos + 1..];
            }
        }
        // recurse
        if let Some(d) = recurse_dt(&node)? {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

fn status_ok(node: &Path) -> io::Result<bool> {
    let s = fs::read(node.join("status")).unwrap_or_default();
    Ok(matches!(&s[..], b"okay"))
}

fn check_sysfs_device(name: &str) -> io::Result<()> {
    for entry in fs::read_dir(PS_DIR)? {
        let entry = entry?;
        if entry.file_name() == name {
            return Ok(());
        }
    }
    Err(io::Error::from(io::ErrorKind::NotFound))
}
