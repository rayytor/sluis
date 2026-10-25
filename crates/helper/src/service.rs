//! The helper's D-Bus interface on the system bus. Reading is open to
//! everyone; every change is checked with polkit first.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;
use zbus::{Connection, interface, proxy};

use sluis_core::{Strategy, site};
use sluis_scan::blockcheck::Line;
use sluis_scan::{Curl, Event, Request};

use crate::install::{self, Kind, Real, Step};
use crate::ops::{self, Systemd, Zapret};
use crate::scan::{self, NftSandbox};
use crate::thorough;
use crate::{
    ACTION_CHANGE, ACTION_MANAGE, ACTION_SCAN, ACTION_START_STOP, ZAPRET_DIR, install_paths,
};

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "io.github.rayytor.Sluis.Helper.Error")]
pub enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),
    NotAuthorized(String),
    NotInstalled(String),
    Invalid(String),
    ServiceFailed(String),
    UpToDate(String),
    /// The network, or a damaged download.
    Download(String),
    Failed(String),
}

impl From<install::Error> for Error {
    fn from(e: install::Error) -> Self {
        let message = e.to_string();
        match e {
            install::Error::NotOurs | install::Error::AlreadyInstalled => Self::Invalid(message),
            install::Error::UpToDate => Self::UpToDate(message),
            install::Error::Download(_) | install::Error::Corrupt => Self::Download(message),
            install::Error::ServiceFailed(_) => Self::ServiceFailed(message),
            install::Error::Io(_) => Self::Failed(message),
        }
    }
}

impl From<ops::Error> for Error {
    fn from(e: ops::Error) -> Self {
        let message = e.to_string();
        match e {
            ops::Error::NotInstalled => Self::NotInstalled(message),
            ops::Error::Invalid(_) => Self::Invalid(message),
            ops::Error::ServiceFailed(_) => Self::ServiceFailed(message),
            ops::Error::Io(_) => Self::Failed(message),
        }
    }
}

#[proxy(
    interface = "org.freedesktop.PolicyKit1.Authority",
    default_service = "org.freedesktop.PolicyKit1",
    default_path = "/org/freedesktop/PolicyKit1/Authority"
)]
trait Authority {
    fn check_authorization(
        &self,
        subject: &(&str, HashMap<&str, Value<'_>>),
        action_id: &str,
        details: &HashMap<&str, &str>,
        flags: u32,
        cancellation_id: &str,
    ) -> zbus::Result<(bool, bool, HashMap<String, String>)>;
}

const ALLOW_USER_INTERACTION: u32 = 1;

/// Asks polkit whether the caller of this message may do `action`; polkit
/// asks for a password through the desktop's agent when the policy wants one.
/// The helper counts as busy meanwhile: typing a password can take longer
/// than the idle timeout, and exiting would drop the caller's request.
async fn authorize(
    idle: &Idle,
    conn: &Connection,
    header: &Header<'_>,
    action: &str,
) -> Result<(), Error> {
    let _busy = idle.busy();
    let sender = header
        .sender()
        .ok_or_else(|| Error::NotAuthorized("unknown caller".into()))?;
    let subject = (
        "system-bus-name",
        HashMap::from([("name", Value::from(sender.as_str()))]),
    );
    let (authorized, _, _) = AuthorityProxy::new(conn)
        .await?
        .check_authorization(
            &subject,
            action,
            &HashMap::new(),
            ALLOW_USER_INTERACTION,
            "",
        )
        .await
        .map_err(|e| Error::NotAuthorized(e.to_string()))?;
    if authorized {
        Ok(())
    } else {
        Err(Error::NotAuthorized(format!("not allowed: {action}")))
    }
}

/// When the helper last did something, and whether it is doing something
/// now, so it can exit when idle (D-Bus starts it again on the next call).
#[derive(Debug)]
pub struct Idle {
    last: Mutex<Instant>,
    busy: AtomicUsize,
}

impl Idle {
    fn new() -> Self {
        Self {
            last: Mutex::new(Instant::now()),
            busy: AtomicUsize::new(0),
        }
    }

    fn touch(&self) {
        *self.last.lock().unwrap() = Instant::now();
    }

    /// Busy until the guard is dropped.
    fn busy(&self) -> Busy<'_> {
        self.busy.fetch_add(1, Ordering::SeqCst);
        Busy(self)
    }

    pub fn expired(&self, after: Duration) -> bool {
        self.busy.load(Ordering::SeqCst) == 0 && self.last.lock().unwrap().elapsed() >= after
    }
}

/// While it lives, the helper doesn't exit for being idle.
struct Busy<'a>(&'a Idle);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.touch();
        self.0.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Helper {
    zapret: Arc<Mutex<Zapret<Systemd>>>,
    idle: Arc<Idle>,
    /// Set to stop the running scan.
    scan: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl Helper {
    pub fn new(zapret: Zapret<Systemd>) -> Self {
        Self {
            zapret: Arc::new(Mutex::new(zapret)),
            idle: Arc::new(Idle::new()),
            scan: Arc::default(),
        }
    }

    pub fn idle(&self) -> Arc<Idle> {
        self.idle.clone()
    }

    /// Runs `f` off the D-Bus thread, one operation at a time.
    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Zapret<Systemd>) -> Result<T, ops::Error> + Send + 'static,
    ) -> Result<T, Error> {
        self.idle.busy.fetch_add(1, Ordering::SeqCst);
        let zapret = self.zapret.clone();
        let result = blocking::unblock(move || f(&zapret.lock().unwrap())).await;
        self.idle.touch();
        self.idle.busy.fetch_sub(1, Ordering::SeqCst);
        result.map_err(Error::from)
    }

    async fn status(&self) -> ops::Status {
        self.run(|z| Ok(z.status())).await.unwrap_or_default()
    }

    /// Runs an install, update or uninstall off the D-Bus thread while no
    /// other change runs, sending SetupProgress.
    async fn setup<T: Send + 'static>(
        &self,
        emitter: SignalEmitter<'static>,
        job: impl FnOnce(&dyn Fn(Step)) -> Result<T, install::Error> + Send + 'static,
    ) -> Result<T, Error> {
        if self.scan.lock().unwrap().is_some() {
            return Err(Error::Failed("a scan is running".into()));
        }
        self.idle.busy.fetch_add(1, Ordering::SeqCst);
        let zapret = self.zapret.clone();
        let result = blocking::unblock(move || {
            let _changing = zapret.lock().unwrap();
            job(&|step| {
                let _ = zbus::block_on(Helper::setup_progress(&emitter, step.as_str()));
            })
        })
        .await;
        self.idle.touch();
        self.idle.busy.fetch_sub(1, Ordering::SeqCst);
        result.map_err(Error::from)
    }

    /// Runs `job` on its own thread as the one running scan, then sends
    /// ScanFinished with its error, if any.
    fn spawn_scan(
        &self,
        emitter: SignalEmitter<'static>,
        job: impl FnOnce(&AtomicBool, &SignalEmitter<'static>) -> Result<(), String> + Send + 'static,
    ) -> Result<(), Error> {
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut current = self.scan.lock().unwrap();
            if current.is_some() {
                return Err(Error::Failed("a scan is already running".into()));
            }
            *current = Some(stop.clone());
        }
        self.idle.busy.fetch_add(1, Ordering::SeqCst);
        let (idle, current) = (self.idle.clone(), self.scan.clone());
        std::thread::spawn(move || {
            let error = job(&stop, &emitter).err().unwrap_or_default();
            *current.lock().unwrap() = None;
            let stopped = stop.load(Ordering::SeqCst);
            let _ = zbus::block_on(Helper::scan_finished(&emitter, stopped, &error));
            idle.touch();
            idle.busy.fetch_sub(1, Ordering::SeqCst);
        });
        Ok(())
    }

    async fn changed(&self, emitter: &SignalEmitter<'_>) {
        // best effort: a client that misses these re-reads on its next call
        let _ = self.enabled_changed(emitter).await;
        let _ = self.running_changed(emitter).await;
        let _ = self.strategy_changed(emitter).await;
        let _ = self.problems_changed(emitter).await;
        let _ = self.installed_changed(emitter).await;
        let _ = self.install_kind_changed(emitter).await;
        let _ = self.version_changed(emitter).await;
    }
}

#[interface(name = "io.github.rayytor.Sluis.Helper1")]
impl Helper {
    /// Makes `strategy` (nfqws arguments) zapret's strategy, restarting
    /// zapret if it runs; rolls back if zapret then fails to start.
    async fn apply(
        &self,
        strategy: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), Error> {
        authorize(&self.idle, conn, &header, ACTION_CHANGE).await?;
        let result = self.run(move |z| z.apply(&strategy)).await;
        self.changed(&emitter).await;
        result
    }

    /// Turns zapret on (now and at boot) or off.
    async fn set_enabled(
        &self,
        on: bool,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), Error> {
        authorize(&self.idle, conn, &header, ACTION_START_STOP).await?;
        let result = self.run(move |z| z.set_enabled(on)).await;
        self.changed(&emitter).await;
        result
    }

    /// Restores config values that damaged quoting swallowed; returns the
    /// names it restored.
    async fn repair(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<Vec<String>, Error> {
        authorize(&self.idle, conn, &header, ACTION_CHANGE).await?;
        let result = self.run(|z| z.repair()).await;
        self.changed(&emitter).await;
        result
    }

    /// Starts a quick scan for `sites` (see sluis_scan::Request) and
    /// returns at once; results come as Scan* signals, ScanFinished last.
    #[allow(clippy::too_many_arguments)]
    async fn start_scan(
        &self,
        sites: Vec<String>,
        controls: Vec<String>,
        first: Vec<String>,
        want: u32,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), Error> {
        authorize(&self.idle, conn, &header, ACTION_SCAN).await?;
        let request = Request {
            sites: domains(sites)?,
            controls: domains(controls)?,
            first: first
                .iter()
                .map(|s| Strategy::parse(s).map_err(|e| Error::Invalid(e.to_string())))
                .collect::<Result<_, _>>()?,
            want: want as usize,
        };
        if request.sites.is_empty() {
            return Err(Error::Invalid("no sites to scan".into()));
        }
        self.spawn_scan(emitter.into_owned(), move |stop, emitter| {
            let sandbox = NftSandbox::start(ZAPRET_DIR).map_err(|e| e.to_string())?;
            let prober = Curl {
                user: Some(scan::NOBODY),
                timeout: 6,
            };
            sluis_scan::run(&prober, &sandbox, &request, stop, &|event| {
                let _ = zbus::block_on(emit(emitter, event));
            });
            Ok(())
        })
    }

    /// Starts a Thorough Scan (zapret's blockcheck.sh, with zapret paused)
    /// for `sites`. ScanProgress has 0 as its total; each method that
    /// opened a site comes as ScanTried with every site it opened so far.
    async fn start_thorough_scan(
        &self,
        sites: Vec<String>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), Error> {
        authorize(&self.idle, conn, &header, ACTION_SCAN).await?;
        let sites = domains(sites)?;
        if sites.is_empty() {
            return Err(Error::Invalid("no sites to scan".into()));
        }
        self.spawn_scan(emitter.into_owned(), move |stop, emitter| {
            // sites that open without bypass need nothing
            let needed = Mutex::new(sites.clone());
            thorough::run(ZAPRET_DIR, &sites, stop, &|line, tried| {
                let _ = zbus::block_on(Helper::scan_progress(emitter, tried as u32, 0));
                match line {
                    Line::OpensWithout { site } => {
                        needed.lock().unwrap().retain(|s| *s != site);
                        let _ = zbus::block_on(Helper::scan_site(emitter, &site, "opens"));
                    }
                    Line::Works {
                        strategy, opened, ..
                    } => {
                        let blocked: Vec<String> = needed
                            .lock()
                            .unwrap()
                            .iter()
                            .filter(|s| !opened.contains(s))
                            .cloned()
                            .collect();
                        let passed = blocked.is_empty();
                        let _ = zbus::block_on(Helper::scan_tried(
                            emitter,
                            &strategy,
                            opened,
                            blocked,
                            Vec::new(),
                            0,
                            passed,
                        ));
                    }
                    _ => {}
                }
            })
        })
    }

    /// Stops the running scan; ScanFinished follows.
    async fn stop_scan(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), Error> {
        authorize(&self.idle, conn, &header, ACTION_SCAN).await?;
        if let Some(stop) = self.scan.lock().unwrap().as_ref() {
            stop.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// How a site did without any bypass: opens, blocked, dns-spoofed,
    /// dns-spoofed-blocked, unreachable or not-found.
    #[zbus(signal)]
    async fn scan_site(
        emitter: &SignalEmitter<'_>,
        site: &str,
        diagnosis: &str,
    ) -> zbus::Result<()>;

    /// One method tried: the blocked sites it opened and didn't, the sites
    /// it broke, the median time in ms, and whether it passed.
    #[zbus(signal)]
    #[allow(clippy::too_many_arguments)]
    async fn scan_tried(
        emitter: &SignalEmitter<'_>,
        strategy: &str,
        opened: Vec<String>,
        blocked: Vec<String>,
        broken: Vec<String>,
        ms: u32,
        passed: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn scan_progress(emitter: &SignalEmitter<'_>, done: u32, total: u32) -> zbus::Result<()>;

    /// The scan ended: stopped by StopScan, or failed with `error`.
    #[zbus(signal)]
    async fn scan_finished(
        emitter: &SignalEmitter<'_>,
        stopped: bool,
        error: &str,
    ) -> zbus::Result<()>;

    /// ScanFinished tells when this turns false.
    #[zbus(property(emits_changed_signal = "false"))]
    async fn scanning(&self) -> bool {
        self.scan.lock().unwrap().is_some()
    }

    /// Installs the latest zapret release (see install.rs), off until a
    /// method is applied and it's turned on. Returns the version.
    async fn install(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<String, Error> {
        authorize(&self.idle, conn, &header, ACTION_MANAGE).await?;
        let result = self
            .setup(emitter.to_owned(), |step| {
                install::install(&Real, &install_paths(), step)
            })
            .await;
        self.changed(&emitter).await;
        result
    }

    /// Updates zapret if this helper installed it; returns the new version.
    async fn update(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<String, Error> {
        authorize(&self.idle, conn, &header, ACTION_MANAGE).await?;
        let result = self
            .setup(emitter.to_owned(), |step| {
                install::update(&Real, &install_paths(), step)
            })
            .await;
        self.changed(&emitter).await;
        result
    }

    /// Stops zapret and removes it, if this helper installed it.
    async fn uninstall(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), Error> {
        authorize(&self.idle, conn, &header, ACTION_MANAGE).await?;
        let result = self
            .setup(emitter.to_owned(), |_| {
                install::uninstall(&Real, &install_paths())
            })
            .await;
        self.changed(&emitter).await;
        result
    }

    /// The latest zapret release's version, for showing an update.
    async fn latest_version(&self) -> Result<String, Error> {
        blocking::unblock(|| install::Host::latest(&Real))
            .await
            .map(|r| r.version)
            .map_err(Error::Download)
    }

    /// downloading, installing or starting
    #[zbus(signal)]
    async fn setup_progress(emitter: &SignalEmitter<'_>, step: &str) -> zbus::Result<()>;

    /// none, app (installed by this helper), system (a distribution
    /// package, updated with the system) or manual.
    #[zbus(property)]
    async fn install_kind(&self) -> String {
        install::kind(&install_paths()).as_str().to_owned()
    }

    /// The installed version when this helper installed zapret, else "".
    #[zbus(property)]
    async fn version(&self) -> String {
        match install::kind(&install_paths()) {
            Kind::App { version } => version,
            _ => String::new(),
        }
    }

    #[zbus(property)]
    async fn installed(&self) -> bool {
        self.status().await.installed
    }

    #[zbus(property)]
    async fn enabled(&self) -> bool {
        self.status().await.enabled
    }

    #[zbus(property)]
    async fn running(&self) -> bool {
        self.status().await.running
    }

    #[zbus(property)]
    async fn strategy(&self) -> String {
        self.status().await.strategy
    }

    #[zbus(property)]
    async fn problems(&self) -> Vec<String> {
        self.status().await.problems
    }
}

async fn emit(emitter: &SignalEmitter<'_>, event: Event) -> zbus::Result<()> {
    match event {
        Event::Diagnosed {
            site,
            diagnosis,
            addr,
        } => {
            let addr = addr.map(|a| a.to_string()).unwrap_or_default();
            eprintln!("scan: {site} ({addr}): {}", diagnosis.as_str());
            Helper::scan_site(emitter, &site, diagnosis.as_str()).await
        }
        Event::Tried(t) => {
            eprintln!(
                "scan: {} opened {:?} blocked {:?} broke {:?}",
                t.strategy, t.opened, t.blocked, t.broken
            );
            Helper::scan_tried(
                emitter,
                &t.strategy,
                t.opened,
                t.blocked,
                t.broken,
                t.ms,
                t.passed,
            )
            .await
        }
        Event::Progress { done, total } => {
            Helper::scan_progress(emitter, done as u32, total as u32).await
        }
    }
}

fn domains(list: Vec<String>) -> Result<Vec<String>, Error> {
    list.iter()
        .map(|s| site::normalize(s).map_err(|_| Error::Invalid(format!("not a site: {s}"))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_a_password_is_not_idle() {
        let idle = Idle::new();
        let busy = idle.busy();
        assert!(!idle.expired(Duration::ZERO));
        drop(busy);
        assert!(idle.expired(Duration::ZERO));
        // the wait counts as activity: the idle time starts over
        assert!(!idle.expired(Duration::from_secs(60)));
    }
}
