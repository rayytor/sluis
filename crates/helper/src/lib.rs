//! The privileged helper: a system D-Bus service that applies strategies,
//! turns zapret on and off, repairs its config, runs quick scans and
//! installs, updates and removes zapret itself, each change checked by
//! polkit. The app talks to it through [`client::HelperProxy`].

pub mod client;
pub mod install;
pub mod ops;
pub mod scan;
pub mod service;
pub mod thorough;

pub const BUS_NAME: &str = "io.github.rayytor.Sluis.Helper";
pub const ZAPRET_DIR: &str = "/opt/zapret";
pub const OBJECT_PATH: &str = "/io/github/rayytor/Sluis/Helper";

/// polkit actions (data/helper/io.github.rayytor.Sluis.policy)
pub const ACTION_CHANGE: &str = "io.github.rayytor.Sluis.change-settings";
pub const ACTION_START_STOP: &str = "io.github.rayytor.Sluis.start-stop";
pub const ACTION_SCAN: &str = "io.github.rayytor.Sluis.scan";
pub const ACTION_MANAGE: &str = "io.github.rayytor.Sluis.manage";

/// Where zapret and its unit go (see [`install`]).
pub fn install_paths() -> install::Paths {
    install::Paths {
        dir: ZAPRET_DIR.into(),
        unit: "/etc/systemd/system/zapret.service".into(),
        system_unit: "/usr/lib/systemd/system/zapret.service".into(),
    }
}
