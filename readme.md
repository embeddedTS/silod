# silod

**silod** is a tiny Rust daemon that monitors the *TS‑SILO* super‑capacitor backup power supply and orchestrates a graceful shutdown (plus any user‑defined helper scripts) when input power is lost. It is intended for embedded Linux systems that use the `tssilo` kernel driver (exposed via `sysfs`).

Features:

* Watches the kernel `uevent` multicast stream for power‑supply **change** events (no polling).
* Executes event‑specific hooks in `/etc/silod/scripts.d/`:
  * `power-fail`
  * `power-restored`
  * `fully-charged`
  * `critical`
* Applies configuration to the driver on startup (`charge_behaviour`, capacity thresholds, current limits…).
* Logs to **stdout** when run interactively, or to **syslog** (`LOG_DAEMON`) when run as a background service.
* Re‑execs `shutdown -p now` automatically once a *critical* capacity threshold is crossed.

---

## Table of Contents
1. [Install](#install)
   1. [Build on the target platform](#build-on-the-target-platform)
   2. [Cross‑compile](#cross-compile)
2. [Usage](#usage)
   1. [Default behaviour](#default-behaviour)
   2. [Configuration file](#configuration-file)
   3. [Scripts](#scripts)
3. [Running as a systemd service](#running-as-a-systemd-service)
4. [Logging](#logging)
5. [Contributing](#contributing)
6. [License](#license)hvae a 

---

## Install

### Build on the target platform

```bash
# Install Rust (if you don't have it yet)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Fetch and build
git clone https://github.com/embeddedTS/silod.git
cd silod
cargo build --release

# Install the binary (requires root for /usr/sbin)
sudo install -m 0755 target/release/silod /usr/sbin/
sudo mkdir -p /etc/silod/scripts/{power-fail,power-restored,fully-charged,critical}
```

## Usage

```bash
# As root:
silod
systemctl start silod.service
```

### Default behaviour

| Event | Condition | Hook executed|
|-------|-----------|--------------|
| **Initial charge** | First run after boot | – |
| **Fully charged**  | Online **and** capacity == 100 % | `fully-charged` |
| **Power‑fail**     | Input lost while capacity ≥ threshold | `power-fail` |
| **Power‑restored** | Input restored | `power-restored` |
| **Critical**       | `capacity < critical_pct` | `critical` |

### Configuration file

`/etc/silod/silo.toml`

```toml
# Mandatory: percent below which the system shuts down
critical_pct = 8

# Optional: enable or inhibit charging (maps to charge_behaviour)
enable_charging = true

# Optional: minimum capacity required before the board may power on again
min_power_on_pct = 20

# Optional: initial charge current (mA) applied after a brown‑out
startup_charge_current_ma = 500
```

All values are written to `/sys/class/power_supply/tssilo/…` and logged back for verification.

### Scripts

Place any executable files in `/etc/silod/scripts.d` in one of the directories:

* `power-fail`
* `power-restored`
* `fully-charged`
* `critical`

Example:

```bash
#!/bin/sh
#/etc/silod/scripts.d/critical/01-send-shutdown-message.sh

curl -X POST http://your-server.example.com/log \
     -H "Content-Type: application/json" \
     -d '{"level": "info", "message": "System is shutting down due supercaps draining", "timestamp": "'"$(date -Is)"'"}'

```

Scripts are executed sequentially in filename order. Failures are logged, but do **not** abort the chain.

---

## Running as a systemd service

```ini
# /etc/systemd/system/silod.service
[Unit]
Description=TS‑SILO supercap backup monitor
After=multi-user.target

[Service]
Type=simple
ExecStart=/usr/sbin/silod
Restart=always
User=root
Group=root

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now silod.service
```

---

## Logging

* Interactive run → **stderr/stdout** with ANSI colours.  
* Background/service run → **syslog** (`LOG_DAEMON`), view with `journalctl -t silod` on systemd systems.

Set the log level with the `RUST_LOG` environment variable, e.g.:

```bash
sudo RUST_LOG=debug silod
```

---
