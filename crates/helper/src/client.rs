//! The app's side of the helper's D-Bus interface (see `service.rs`).

use zbus::proxy;

#[proxy(
    interface = "io.github.rayytor.Sluis.Helper1",
    default_service = "io.github.rayytor.Sluis.Helper",
    default_path = "/io/github/rayytor/Sluis/Helper"
)]
pub trait Helper {
    fn apply(&self, strategy: &str) -> zbus::Result<()>;
    fn set_enabled(&self, on: bool) -> zbus::Result<()>;
    fn repair(&self) -> zbus::Result<Vec<String>>;
    fn start_scan(
        &self,
        sites: &[&str],
        controls: &[&str],
        first: &[&str],
        want: u32,
    ) -> zbus::Result<()>;
    fn start_thorough_scan(&self, sites: &[&str]) -> zbus::Result<()>;
    fn stop_scan(&self) -> zbus::Result<()>;
    fn install(&self) -> zbus::Result<String>;
    fn update(&self) -> zbus::Result<String>;
    fn uninstall(&self) -> zbus::Result<()>;
    fn latest_version(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn setup_progress(&self, step: String) -> zbus::Result<()>;
    #[zbus(property)]
    fn install_kind(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn scan_site(&self, site: String, diagnosis: String) -> zbus::Result<()>;
    #[zbus(signal)]
    #[allow(clippy::too_many_arguments)]
    fn scan_tried(
        &self,
        strategy: String,
        opened: Vec<String>,
        blocked: Vec<String>,
        broken: Vec<String>,
        ms: u32,
        passed: bool,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn scan_progress(&self, done: u32, total: u32) -> zbus::Result<()>;
    #[zbus(signal)]
    fn scan_finished(&self, stopped: bool, error: String) -> zbus::Result<()>;

    #[zbus(property)]
    fn scanning(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn installed(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn running(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn strategy(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn problems(&self) -> zbus::Result<Vec<String>>;
}
