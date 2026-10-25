//! What the helper does, with no D-Bus in it: read and change zapret's
//! config safely and drive its service.
//!
//! Every config change is written atomically (temporary file, fsync,
//! rename) after the previous file is copied to `config.sluis.bak`, so
//! a crash leaves either the old or the new file, never half of one. A
//! change that stops zapret from starting is rolled back.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use sluis_core::config::Problem;
use sluis_core::{Config, ParseError, Strategy};

#[derive(Debug)]
pub enum Error {
    /// `/opt/zapret` or its config is missing.
    NotInstalled,
    Invalid(ParseError),
    /// zapret did not start after the change; the old config is back.
    ServiceFailed(String),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("zapret is not installed"),
            Self::Invalid(e) => write!(f, "not a valid strategy: {e}"),
            Self::ServiceFailed(log) => write!(
                f,
                "zapret did not start, so the previous settings are back: {log}"
            ),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Starts and stops zapret. [`Systemd`] in real life, a fake in tests.
pub trait Service: Send {
    fn is_active(&self) -> bool;
    fn is_enabled(&self) -> bool;
    /// Restarts zapret and waits until it is up; the error says why not.
    fn restart(&self) -> Result<(), String>;
    /// Enables and starts it, or stops and disables it.
    fn set_enabled(&self, on: bool) -> Result<(), String>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub installed: bool,
    pub enabled: bool,
    pub running: bool,
    /// `NFQWS_OPT` as written, empty when missing or not a plain value.
    pub strategy: String,
    pub problems: Vec<String>,
}

pub struct Zapret<S> {
    dir: PathBuf,
    service: S,
}

impl<S: Service> Zapret<S> {
    pub fn new(dir: impl Into<PathBuf>, service: S) -> Self {
        Self {
            dir: dir.into(),
            service,
        }
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join("config")
    }

    fn temp_path(&self) -> PathBuf {
        self.dir.join("config.sluis.tmp")
    }

    pub fn backup_path(&self) -> PathBuf {
        self.dir.join("config.sluis.bak")
    }

    /// Removes what an interrupted write left behind.
    pub fn clean_up(&self) {
        let _ = fs::remove_file(self.temp_path());
    }

    fn read_config(&self) -> Result<Config, Error> {
        match fs::read_to_string(self.config_path()) {
            Ok(text) => Ok(Config::parse(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(Error::NotInstalled),
            Err(e) => Err(e.into()),
        }
    }

    /// Backs up the current file, then replaces it atomically, keeping its
    /// permissions.
    fn write_config(&self, config: &Config) -> Result<(), Error> {
        let path = self.config_path();
        let permissions = fs::metadata(&path)?.permissions();
        fs::copy(&path, self.backup_path())?;
        let temp = self.temp_path();
        {
            let mut file = File::create(&temp)?;
            file.write_all(config.as_str().as_bytes())?;
            file.set_permissions(permissions)?;
            file.sync_all()?;
        }
        fs::rename(&temp, &path)?;
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    pub fn status(&self) -> Status {
        let config = self.read_config().ok();
        let installed = config.is_some() && self.dir.join("nfq/nfqws").exists();
        let strategy = config
            .as_ref()
            .and_then(|c| c.get("NFQWS_OPT"))
            .and_then(|v| v.literal())
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        let problems = config
            .map(|c| c.problems().iter().map(Problem::to_string).collect())
            .unwrap_or_default();
        Status {
            installed,
            enabled: self.service.is_enabled(),
            running: self.service.is_active(),
            strategy,
            problems,
        }
    }

    /// Writes `config` and restarts zapret if it is running. If it fails to
    /// come back up, restores `old` and restarts again.
    fn commit(&self, old: &Config, config: &Config) -> Result<(), Error> {
        self.write_config(config)?;
        if !self.service.is_active() {
            return Ok(());
        }
        if let Err(log) = self.service.restart() {
            self.write_config(old)?;
            let _ = self.service.restart();
            return Err(Error::ServiceFailed(log));
        }
        Ok(())
    }

    /// Makes `strategy` zapret's strategy.
    pub fn apply(&self, strategy: &str) -> Result<(), Error> {
        let strategy = Strategy::parse(strategy).map_err(Error::Invalid)?;
        let old = self.read_config()?;
        let mut config = old.clone();
        config.set("NFQWS_OPT", &strategy.to_config_value());
        if config == old {
            return Ok(());
        }
        self.commit(&old, &config)
    }

    /// Turns zapret on (now and at boot) or off. Turning it on also makes
    /// sure the config enables nfqws.
    pub fn set_enabled(&self, on: bool) -> Result<(), Error> {
        let old = self.read_config()?;
        if on && old.get("NFQWS_ENABLE").and_then(|v| v.literal()).as_deref() != Some("1") {
            let mut config = old.clone();
            config.set("NFQWS_ENABLE", "1");
            self.write_config(&config)?;
        }
        self.service.set_enabled(on).map_err(|log| {
            if on {
                Error::ServiceFailed(log)
            } else {
                Error::Io(io::Error::other(log))
            }
        })
    }

    /// Repairs damaged values from zapret's `config.default` (see
    /// [`Config::repair`]) and returns their names.
    pub fn repair(&self) -> Result<Vec<String>, Error> {
        let old = self.read_config()?;
        let defaults = match fs::read_to_string(self.dir.join("config.default")) {
            Ok(text) => Config::parse(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Config::parse(""),
            Err(e) => return Err(e.into()),
        };
        let mut config = old.clone();
        let fixed = config.repair(&defaults);
        if !fixed.is_empty() {
            self.commit(&old, &config)?;
        }
        Ok(fixed)
    }
}

/// zapret's systemd unit, driven through `systemctl`.
pub struct Systemd {
    pub unit: &'static str,
}

impl Systemd {
    fn systemctl(&self, args: &[&str]) -> io::Result<std::process::Output> {
        Command::new("systemctl").args(args).arg(self.unit).output()
    }

    fn ok(&self, args: &[&str]) -> Result<(), String> {
        match self.systemctl(args) {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => Err(self.why(&String::from_utf8_lossy(&out.stderr))),
            Err(e) => Err(e.to_string()),
        }
    }

    /// systemctl's message plus the unit's last log lines.
    fn why(&self, stderr: &str) -> String {
        let log = Command::new("journalctl")
            .args([
                "--unit",
                self.unit,
                "--lines=8",
                "--no-pager",
                "--output=cat",
            ])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        format!("{}\n{}", stderr.trim(), log.trim())
            .trim()
            .to_owned()
    }
}

impl Service for Systemd {
    fn is_active(&self) -> bool {
        self.systemctl(&["--quiet", "is-active"])
            .is_ok_and(|o| o.status.success())
    }

    fn is_enabled(&self) -> bool {
        self.systemctl(&["--quiet", "is-enabled"])
            .is_ok_and(|o| o.status.success())
    }

    fn restart(&self) -> Result<(), String> {
        self.ok(&["restart"])?;
        if self.is_active() {
            Ok(())
        } else {
            Err(self.why("zapret stopped right after starting"))
        }
    }

    fn set_enabled(&self, on: bool) -> Result<(), String> {
        if on {
            self.ok(&["enable", "--now"])
        } else {
            self.ok(&["disable", "--now"])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

    /// A service that is running and fails its next `failures` restarts.
    struct Fake {
        active: AtomicBool,
        enabled: AtomicBool,
        failures: AtomicU32,
        restarts: AtomicU32,
    }

    impl Fake {
        fn running() -> Self {
            Self {
                active: AtomicBool::new(true),
                enabled: AtomicBool::new(true),
                failures: AtomicU32::new(0),
                restarts: AtomicU32::new(0),
            }
        }
    }

    impl Service for Fake {
        fn is_active(&self) -> bool {
            self.active.load(Relaxed)
        }
        fn is_enabled(&self) -> bool {
            self.enabled.load(Relaxed)
        }
        fn restart(&self) -> Result<(), String> {
            self.restarts.fetch_add(1, Relaxed);
            if self.failures.load(Relaxed) > 0 {
                self.failures.fetch_sub(1, Relaxed);
                return Err("nfqws: bad option".into());
            }
            Ok(())
        }
        fn set_enabled(&self, on: bool) -> Result<(), String> {
            self.enabled.store(on, Relaxed);
            self.active.store(on, Relaxed);
            Ok(())
        }
    }

    fn holds(path: &Path, text: &str) -> bool {
        fs::read_to_string(path).is_ok_and(|t| t == text)
    }

    fn data(name: &str) -> String {
        let path = format!("{}/../core/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
        fs::read_to_string(path).unwrap()
    }

    /// A fresh `/opt/zapret` look-alike holding the damaged live config.
    fn zapret(service: Fake) -> (Zapret<Fake>, PathBuf) {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sluis-helper-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("nfq")).unwrap();
        fs::write(dir.join("nfq/nfqws"), "").unwrap();
        fs::write(dir.join("config"), data("config.toros-damaged")).unwrap();
        fs::write(dir.join("config.default"), data("config.default")).unwrap();
        (Zapret::new(&dir, service), dir)
    }

    const MULTI: &str = "--filter-tcp=80,443 --dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST> --new --filter-udp=443 --dpi-desync=fake --dpi-desync-repeats=6";

    #[test]
    fn status_reads_the_config_and_service() {
        let (z, _) = zapret(Fake::running());
        let s = z.status();
        assert!(s.installed && s.enabled && s.running);
        assert_eq!(
            s.strategy,
            "--dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>"
        );
        assert_eq!(s.problems.len(), 1);
    }

    #[test]
    fn apply_writes_backs_up_and_restarts() {
        let (z, dir) = zapret(Fake::running());
        z.apply(MULTI).unwrap();
        let config = Config::parse(fs::read_to_string(dir.join("config")).unwrap());
        let written = config.get("NFQWS_OPT").unwrap().literal().unwrap();
        assert_eq!(Strategy::parse(&written).unwrap().to_string(), MULTI);
        assert!(holds(&z.backup_path(), &data("config.toros-damaged")));
        assert_eq!(z.service.restarts.load(Relaxed), 1);
        assert!(!z.temp_path().exists());
    }

    #[test]
    fn apply_rolls_back_when_zapret_does_not_start() {
        let fake = Fake::running();
        fake.failures.store(1, Relaxed);
        let (z, dir) = zapret(fake);
        assert!(matches!(z.apply(MULTI), Err(Error::ServiceFailed(_))));
        assert!(holds(&dir.join("config"), &data("config.toros-damaged")));
        assert_eq!(z.service.restarts.load(Relaxed), 2);
    }

    #[test]
    fn apply_only_writes_when_zapret_is_off() {
        let fake = Fake::running();
        fake.active.store(false, Relaxed);
        let (z, _) = zapret(fake);
        z.apply(MULTI).unwrap();
        assert_eq!(z.service.restarts.load(Relaxed), 0);
        assert_eq!(
            Strategy::parse(&z.status().strategy).unwrap().to_string(),
            MULTI
        );
    }

    #[test]
    fn apply_rejects_bad_strategies_without_touching_anything() {
        let (z, dir) = zapret(Fake::running());
        assert!(matches!(z.apply("--user=root"), Err(Error::Invalid(_))));
        assert!(holds(&dir.join("config"), &data("config.toros-damaged")));
        assert!(!z.backup_path().exists());
    }

    #[test]
    fn repair_fixes_the_live_config() {
        let (z, _) = zapret(Fake::running());
        assert_eq!(z.repair().unwrap(), ["TPWS_SOCKS_OPT", "TPWS_OPT"]);
        assert!(z.status().problems.is_empty());
        assert_eq!(z.repair().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn enabling_turns_nfqws_on_in_the_config() {
        let (z, dir) = zapret(Fake::running());
        let mut config = Config::parse(fs::read_to_string(dir.join("config")).unwrap());
        config.set("NFQWS_ENABLE", "0");
        fs::write(dir.join("config"), config.as_str()).unwrap();
        z.set_enabled(false).unwrap();
        assert!(!z.status().running);
        z.set_enabled(true).unwrap();
        let config = Config::parse(fs::read_to_string(dir.join("config")).unwrap());
        assert_eq!(config.get("NFQWS_ENABLE").unwrap().raw(), "1");
        assert!(z.status().running);
    }

    #[test]
    fn missing_zapret_is_reported() {
        let z = Zapret::new("/nonexistent/zapret", Fake::running());
        assert!(!z.status().installed);
        assert!(matches!(
            z.apply("--dpi-desync=fake"),
            Err(Error::NotInstalled)
        ));
    }

    #[test]
    fn interrupted_writes_are_cleaned_up() {
        let (z, _) = zapret(Fake::running());
        fs::write(z.temp_path(), "half").unwrap();
        z.clean_up();
        assert!(!z.temp_path().exists());
    }
}
