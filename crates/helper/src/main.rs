//! sluis-helper: started by D-Bus on the system bus when the app first
//! calls it, as root; exits after two idle minutes.

use std::time::Duration;

use sluis_helper::ops::{Systemd, Zapret};
use sluis_helper::service::Helper;
use sluis_helper::{BUS_NAME, OBJECT_PATH, ZAPRET_DIR, install, install_paths, scan, thorough};

const IDLE_EXIT: Duration = Duration::from_secs(120);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let zapret = Zapret::new(
        ZAPRET_DIR,
        Systemd {
            unit: "zapret.service",
        },
    );
    zapret.clean_up();
    scan::clean_up();
    thorough::resume_if_paused();
    install::clean_up(&install_paths());
    let helper = Helper::new(zapret);
    let idle = helper.idle();
    // zbus serves the interface on its own thread
    let _connection = zbus::blocking::connection::Builder::system()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, helper)?
        .build()?;
    while !idle.expired(IDLE_EXIT) {
        std::thread::sleep(Duration::from_secs(10));
    }
    Ok(())
}
