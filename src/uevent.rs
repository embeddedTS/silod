use std::io;
use netlink_sys::{protocols::NETLINK_KOBJECT_UEVENT, Socket, SocketAddr};
use kobject_uevent::{ActionType, UEvent};

pub struct UeventListener {
    socket: Socket,
}

impl UeventListener {
    pub fn connect() -> io::Result<Self> {
        let mut socket = Socket::new(NETLINK_KOBJECT_UEVENT)?;
        // 1 = multicast group for all uevents
        socket.bind(&SocketAddr::new(std::process::id(), 1))?;
        log::debug!("uevent socket ready (NETLINK_KOBJECT_UEVENT, group 1)");
        Ok(Self { socket })
    }

    /// Block until the next uevent; return true if it's a `change`
    /// for our power-supply device.
    pub fn wait_event(&mut self, sysname: &str) -> io::Result<Option<UEvent>> {
        loop {
            let (buf, _) = self.socket.recv_from_full()?;
            let evt = UEvent::from_netlink_packet(&buf).map_err(io::Error::other)?;

            // Only propagate “change” events for our driver
            if evt.action == ActionType::Change
                && evt.env.get("POWER_SUPPLY_NAME").map(|s| s == sysname).unwrap_or(false)
            {
                return Ok(Some(evt));
            }
        }
    }
}
