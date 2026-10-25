//! The app's connection to sluis-helper on the system bus (M2).
//!
//! Property caching is off: the helper exits when idle and D-Bus starts a
//! new one on the next call, so the app reads fresh values whenever it
//! needs them, after every change and when it starts.

use futures_util::stream::{self, BoxStream, StreamExt};
use gettextrs::gettext;
use sluis_helper::client::HelperProxy;
use zbus::proxy::CacheProperties;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub installed: bool,
    pub enabled: bool,
    pub running: bool,
    pub strategy: String,
    pub problems: Vec<String>,
    /// none, app (installed by this app), system or manual
    pub install_kind: String,
    /// When this app installed zapret.
    pub version: String,
}

#[derive(Clone, Debug)]
pub struct Helper {
    proxy: HelperProxy<'static>,
}

impl Helper {
    pub async fn connect() -> zbus::Result<Self> {
        let connection = zbus::Connection::system().await?;
        let proxy = HelperProxy::builder(&connection)
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        Ok(Self { proxy })
    }

    pub async fn status(&self) -> zbus::Result<Status> {
        Ok(Status {
            installed: self.proxy.installed().await?,
            enabled: self.proxy.enabled().await?,
            running: self.proxy.running().await?,
            strategy: self.proxy.strategy().await?,
            problems: self.proxy.problems().await?,
            install_kind: self.proxy.install_kind().await.unwrap_or_default(),
            version: self.proxy.version().await.unwrap_or_default(),
        })
    }

    pub async fn apply(&self, strategy: &str) -> Result<(), String> {
        self.proxy.apply(strategy).await.map_err(message)
    }

    pub async fn set_enabled(&self, on: bool) -> Result<(), String> {
        self.proxy.set_enabled(on).await.map_err(message)
    }

    pub async fn repair(&self) -> Result<Vec<String>, String> {
        self.proxy.repair().await.map_err(message)
    }

    /// Installs zapret; returns its version.
    pub async fn install(&self) -> Result<String, String> {
        self.proxy.install().await.map_err(message)
    }

    pub async fn update(&self) -> Result<String, String> {
        self.proxy.update().await.map_err(message)
    }

    pub async fn uninstall(&self) -> Result<(), String> {
        self.proxy.uninstall().await.map_err(message)
    }

    /// The newest zapret release, `None` when it can't be found out.
    pub async fn latest_version(&self) -> Option<String> {
        self.proxy.latest_version().await.ok()
    }

    /// Steps of an install or update: downloading, installing, starting.
    pub async fn setup_steps(&self) -> zbus::Result<BoxStream<'static, String>> {
        Ok(self
            .proxy
            .receive_setup_progress()
            .await?
            .filter_map(|s| async move { s.args().ok().map(|a| a.step) })
            .boxed())
    }
}

/// What a running scan reports (the helper's Scan* signals).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanEvent {
    Site {
        site: String,
        diagnosis: String,
    },
    Tried(Tried),
    /// `total` is 0 for a Thorough Scan, which can't tell in advance.
    Progress {
        done: u32,
        total: u32,
    },
    Finished {
        stopped: bool,
        error: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tried {
    pub strategy: String,
    pub opened: Vec<String>,
    pub blocked: Vec<String>,
    pub broken: Vec<String>,
    pub ms: u32,
    pub passed: bool,
}

impl Helper {
    /// Every scan event from now on, in order.
    pub async fn scan_events(&self) -> zbus::Result<BoxStream<'static, ScanEvent>> {
        let p = &self.proxy;
        let sites = p.receive_scan_site().await?.filter_map(|s| async move {
            let a = s.args().ok()?;
            Some(ScanEvent::Site {
                site: a.site,
                diagnosis: a.diagnosis,
            })
        });
        let tried = p.receive_scan_tried().await?.filter_map(|s| async move {
            let a = s.args().ok()?;
            Some(ScanEvent::Tried(Tried {
                strategy: a.strategy,
                opened: a.opened,
                blocked: a.blocked,
                broken: a.broken,
                ms: a.ms,
                passed: a.passed,
            }))
        });
        let progress = p.receive_scan_progress().await?.filter_map(|s| async move {
            let a = s.args().ok()?;
            Some(ScanEvent::Progress {
                done: a.done,
                total: a.total,
            })
        });
        let finished = p.receive_scan_finished().await?.filter_map(|s| async move {
            let a = s.args().ok()?;
            Some(ScanEvent::Finished {
                stopped: a.stopped,
                error: a.error,
            })
        });
        Ok(stream::select_all([
            sites.boxed(),
            tried.boxed(),
            progress.boxed(),
            finished.boxed(),
        ])
        .boxed())
    }

    /// A quick scan; `first` are methods to try before the shortlist, and
    /// it stops after `want` pass (0: tries them all).
    pub async fn start_scan(
        &self,
        sites: &[String],
        first: &[String],
        want: u32,
    ) -> Result<(), String> {
        let sites: Vec<&str> = sites.iter().map(String::as_str).collect();
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        self.proxy
            .start_scan(&sites, &[], &first, want)
            .await
            .map_err(message)
    }

    pub async fn start_thorough_scan(&self, sites: &[String]) -> Result<(), String> {
        let sites: Vec<&str> = sites.iter().map(String::as_str).collect();
        self.proxy
            .start_thorough_scan(&sites)
            .await
            .map_err(message)
    }

    pub async fn stop_scan(&self) -> Result<(), String> {
        self.proxy.stop_scan().await.map_err(message)
    }
}

/// What went wrong, for a toast. The helper's own text goes to the log.
fn message(error: zbus::Error) -> String {
    let name = match &error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str().rsplit('.').next().unwrap_or_default()
        }
        _ => "",
    };
    eprintln!("sluis-helper: {error}");
    match name {
        "NotAuthorized" => gettext("You’re not allowed to change Zapret’s settings"),
        "ServiceFailed" => {
            gettext("Zapret stopped with that change, so the previous settings are back")
        }
        "NotInstalled" => gettext("Zapret isn’t installed"),
        "Invalid" => gettext("That method isn’t valid"),
        "Download" => gettext("Couldn’t download Zapret. Check your internet connection."),
        "UpToDate" => gettext("Zapret is up to date"),
        "Failed" if error.to_string().contains("scan is already running") => {
            gettext("A scan is already running")
        }
        _ => gettext("Couldn’t change Zapret’s settings"),
    }
}
