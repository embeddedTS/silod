use std::io;
use std::time::Duration;

use neli::{
    consts::socket::NlFamily,
    nl::{Nlmsghdr, NlPayload},
    socket::NlSocketHandle,
};

const UEVENT_MCAST_GRP: u32 = 1;

pub struct Uevent {
    sock: NlSocketHandle,
}

impl Uevent {
    pub fn connect() -> io::Result<Self> {
        let sock = NlSocketHandle::connect(NlFamily::Generic, None, &[])?;
        sock.add_mcast_membership(&[UEVENT_MCAST_GRP])?;

        Ok(Uevent { sock })
    }

    pub fn wait_event(&mut self, driver_name: &str, _timeout: Option<Duration>) -> io::Result<bool> {
        let nlhdr: Nlmsghdr<u16, Vec<u8>> =
            self.sock
                .recv()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                .ok_or_else(|| io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "no message available",
                ))?;

        match nlhdr.nl_payload {
            NlPayload::Payload(p) => self.parse_blob(&p, driver_name),
            _ => Ok(false),
        }
    }

    fn parse_blob(&self, payload: &[u8], sysname: &str) -> io::Result<bool> {
        let mut from_this_device = false;
        let mut is_change = false;

        for field in payload.split(|&b| b == 0).filter_map(|s| std::str::from_utf8(s).ok()) {
            if let Some(name) = field.strip_prefix("POWER_SUPPLY_NAME=") {
                from_this_device = name == sysname;
            } else if let Some(val) = field.strip_prefix("ACTION=") {
                is_change = val == "change";
            }
        }

        Ok(from_this_device && is_change)
    }
}
