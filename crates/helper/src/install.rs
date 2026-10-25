//! Getting zapret: install it from the latest upstream release, update an
//! install this helper made, and uninstall it again.
//!
//! A release is downloaded, checked against the SHA-256 GitHub publishes
//! for it, unpacked next to the old directory and only then swapped in, so
//! a failed download or update leaves the old zapret untouched. An update
//! keeps the config and the user's lists; if zapret then fails to start,
//! the old directory comes back.
//!
//! zapret installed some other way (a distribution package, as on torOS,
//! or upstream's install_easy.sh) is left alone: only an install with this
//! helper's marker is updated or removed.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};
use sluis_core::Config;

/// Written into zapret's directory by [`install`], with the version.
const MARKER: &str = ".sluis";
/// The user's own files an update keeps.
const KEEP: &[&str] = &[
    "config",
    "ipset/zapret-hosts-user.txt",
    "ipset/zapret-hosts-user-exclude.txt",
    "ipset/zapret-hosts-user-ipban.txt",
    "ipset/zapret-ip-user.txt",
    "ipset/zapret-ip-exclude.txt",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    None,
    /// Installed by this helper, which may update and remove it.
    App {
        version: String,
    },
    /// Comes with the system (its unit is in /usr/lib); the system updates it.
    System,
    /// Installed by hand.
    Manual,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::App { .. } => "app",
            Self::System => "system",
            Self::Manual => "manual",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug)]
pub enum Error {
    /// Not possible for this kind of install (see [`Kind`]).
    NotOurs,
    AlreadyInstalled,
    UpToDate,
    Download(String),
    /// The download doesn't match the published checksum.
    Corrupt,
    ServiceFailed(String),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotOurs => f.write_str("zapret was not installed by Sluis"),
            Self::AlreadyInstalled => f.write_str("zapret is already installed"),
            Self::UpToDate => f.write_str("zapret is up to date"),
            Self::Download(e) => write!(f, "download failed: {e}"),
            Self::Corrupt => f.write_str("the download is damaged"),
            Self::ServiceFailed(e) => write!(f, "zapret did not start: {e}"),
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

/// Steps for the progress signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Downloading,
    Installing,
    Starting,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Installing => "installing",
            Self::Starting => "starting",
        }
    }
}

/// Everything outside the directory: the network and systemd. Real in the
/// helper, fake in tests.
pub trait Host {
    fn latest(&self) -> Result<Release, String>;
    fn download(&self, release: &Release, to: &Path) -> Result<(), String>;
    fn is_active(&self) -> bool;
    fn start(&self) -> Result<(), String>;
    fn stop(&self);
    /// Disables zapret at boot and stops it.
    fn disable(&self);
    /// Re-reads unit files.
    fn reload(&self);
}

pub struct Paths {
    /// /opt/zapret
    pub dir: PathBuf,
    /// Where this helper writes zapret's unit.
    pub unit: PathBuf,
    /// Where a distribution package puts it.
    pub system_unit: PathBuf,
}

impl Paths {
    fn staging(&self) -> PathBuf {
        self.dir.with_extension("sluis-new")
    }

    fn old(&self) -> PathBuf {
        self.dir.with_extension("sluis-old")
    }
}

pub fn kind(paths: &Paths) -> Kind {
    if !paths.dir.join("config").exists() {
        return Kind::None;
    }
    if let Ok(version) = fs::read_to_string(paths.dir.join(MARKER)) {
        return Kind::App {
            version: version.trim().to_owned(),
        };
    }
    if paths.system_unit.exists() {
        Kind::System
    } else {
        Kind::Manual
    }
}

/// "72.13" > "72.8"
pub fn newer(candidate: &str, than: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(candidate) > parts(than)
}

/// Removes what an interrupted install or update left behind; an update
/// interrupted after moving the old directory aside gets it back.
pub fn clean_up(paths: &Paths) {
    if !paths.dir.exists() && paths.old().exists() {
        let _ = fs::rename(paths.old(), &paths.dir);
    }
    let _ = fs::remove_dir_all(paths.staging());
}

/// Downloads, checks and unpacks `release` into the staging directory and
/// picks the binaries for this machine.
fn prepare(
    host: &dyn Host,
    paths: &Paths,
    release: &Release,
    step: &dyn Fn(Step),
) -> Result<(), Error> {
    step(Step::Downloading);
    let staging = paths.staging();
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let tarball = staging.with_extension("tar.gz");
    let result = (|| {
        host.download(release, &tarball).map_err(Error::Download)?;
        let digest = Sha256::digest(fs::read(&tarball)?);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        if !hex.eq_ignore_ascii_case(&release.sha256) {
            return Err(Error::Corrupt);
        }
        step(Step::Installing);
        run(Command::new("tar")
            .arg("-xzf")
            .arg(&tarball)
            .arg("-C")
            .arg(&staging)
            .args(["--strip-components=1", "--no-same-owner"]))?;
        // links the prebuilt nfqws for this CPU into nfq/
        run(Command::new("sh")
            .arg("install_bin.sh")
            .current_dir(&staging))?;
        Ok(())
    })();
    let _ = fs::remove_file(&tarball);
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn run(command: &mut Command) -> Result<(), Error> {
    let output = command.stdin(Stdio::null()).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::Io(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        )))
    }
}

/// zapret's unit from its own directory, for systemd.
fn write_unit(host: &dyn Host, paths: &Paths) -> Result<(), Error> {
    let unit = fs::read_to_string(paths.dir.join("init.d/systemd/zapret.service"))?;
    fs::write(&paths.unit, unit)?;
    host.reload();
    Ok(())
}

/// Installs the latest release: zapret is set up for every site, off until
/// the app applies a method and turns it on.
pub fn install(host: &dyn Host, paths: &Paths, step: &dyn Fn(Step)) -> Result<String, Error> {
    if kind(paths) != Kind::None {
        return Err(Error::AlreadyInstalled);
    }
    let release = host.latest().map_err(Error::Download)?;
    prepare(host, paths, &release, step)?;
    let staging = paths.staging();
    let mut config = Config::parse(fs::read_to_string(staging.join("config.default"))?);
    config.set("MODE_FILTER", "none");
    config.set("NFQWS_ENABLE", "1");
    config.set("FWTYPE", "nftables");
    fs::write(staging.join("config"), config.to_string())?;
    fs::write(staging.join(MARKER), &release.version)?;
    // a directory without config (a failed manual attempt) goes aside
    if paths.dir.exists() {
        fs::rename(&paths.dir, paths.old())?;
    }
    fs::rename(&staging, &paths.dir)?;
    let _ = fs::remove_dir_all(paths.old());
    write_unit(host, paths)?;
    Ok(release.version)
}

/// Replaces an install this helper made with the latest release, keeping
/// the config and lists; puts the old one back if zapret then fails.
pub fn update(host: &dyn Host, paths: &Paths, step: &dyn Fn(Step)) -> Result<String, Error> {
    let Kind::App { version } = kind(paths) else {
        return Err(Error::NotOurs);
    };
    let release = host.latest().map_err(Error::Download)?;
    if !newer(&release.version, &version) {
        return Err(Error::UpToDate);
    }
    prepare(host, paths, &release, step)?;
    let staging = paths.staging();
    for file in KEEP {
        let from = paths.dir.join(file);
        if from.exists() {
            fs::copy(&from, staging.join(file))?;
        }
    }
    fs::write(staging.join(MARKER), &release.version)?;

    step(Step::Starting);
    let was_running = host.is_active();
    host.stop();
    fs::rename(&paths.dir, paths.old())?;
    fs::rename(&staging, &paths.dir)?;
    write_unit(host, paths)?;
    if was_running && let Err(e) = host.start() {
        host.stop();
        fs::rename(&paths.dir, &staging)?;
        fs::rename(paths.old(), &paths.dir)?;
        let _ = fs::remove_dir_all(&staging);
        write_unit(host, paths)?;
        let _ = host.start();
        return Err(Error::ServiceFailed(e));
    }
    let _ = fs::remove_dir_all(paths.old());
    Ok(release.version)
}

/// Stops zapret and removes everything [`install`] made.
pub fn uninstall(host: &dyn Host, paths: &Paths) -> Result<(), Error> {
    if !matches!(kind(paths), Kind::App { .. }) {
        return Err(Error::NotOurs);
    }
    host.disable();
    let _ = fs::remove_file(&paths.unit);
    host.reload();
    fs::remove_dir_all(&paths.dir)?;
    Ok(())
}

/// The real network and systemd.
pub struct Real;

const RELEASES: &str = "https://api.github.com/repos/bol-van/zapret/releases/latest";
const UNIT: &str = "zapret.service";

fn systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// The release tarball from GitHub's answer for the latest release.
fn parse_release(json: &[u8]) -> Option<Release> {
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
        digest: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Latest {
        tag_name: String,
        assets: Vec<Asset>,
    }
    let latest: Latest = serde_json::from_slice(json).ok()?;
    let name = format!("zapret-{}.tar.gz", latest.tag_name);
    let asset = latest.assets.into_iter().find(|a| a.name == name)?;
    Some(Release {
        version: latest.tag_name.trim_start_matches('v').to_owned(),
        url: asset.browser_download_url,
        sha256: asset.digest?.strip_prefix("sha256:")?.to_owned(),
    })
}

impl Host for Real {
    fn latest(&self) -> Result<Release, String> {
        let output = Command::new("curl")
            .args(["--silent", "--fail", "--location", "--max-time", "20"])
            .args(["--header", "Accept: application/vnd.github+json", RELEASES])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!("curl exited with {}", output.status));
        }
        parse_release(&output.stdout).ok_or_else(|| "no release tarball found".into())
    }

    fn download(&self, release: &Release, to: &Path) -> Result<(), String> {
        let status = Command::new("curl")
            .args([
                "--silent",
                "--fail",
                "--location",
                "--max-time",
                "600",
                "--output",
            ])
            .arg(to)
            .arg(&release.url)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("curl exited with {status}"))
        }
    }

    fn is_active(&self) -> bool {
        systemctl(&["--quiet", "is-active", UNIT]).is_ok()
    }

    fn start(&self) -> Result<(), String> {
        systemctl(&["start", UNIT])
    }

    fn stop(&self) {
        let _ = systemctl(&["stop", UNIT]);
    }

    fn disable(&self) {
        let _ = systemctl(&["disable", "--now", UNIT]);
    }

    fn reload(&self) {
        let _ = systemctl(&["daemon-reload"]);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNT: AtomicU32 = AtomicU32::new(0);

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zapret-install-test-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A release tarball like upstream's: a top directory with
    /// config.default, install_bin.sh and the unit.
    fn tarball(root: &Path, version: &str, nfqws: &str) -> (PathBuf, String) {
        let src = root.join(format!("src-{version}"));
        let top = src.join(format!("zapret-v{version}"));
        fs::create_dir_all(top.join("init.d/systemd")).unwrap();
        fs::create_dir_all(top.join("ipset")).unwrap();
        fs::write(
            top.join("config.default"),
            "FWTYPE=iptables\nMODE_FILTER=none\nNFQWS_ENABLE=0\nNFQWS_OPT=\"--dpi-desync=fake\"\n",
        )
        .unwrap();
        fs::write(
            top.join("install_bin.sh"),
            format!("mkdir -p nfq && echo {nfqws} > nfq/nfqws\n"),
        )
        .unwrap();
        fs::write(
            top.join("init.d/systemd/zapret.service"),
            format!("[Service]\n# {version}\n"),
        )
        .unwrap();
        let file = root.join(format!("zapret-v{version}.tar.gz"));
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&file)
            .arg("-C")
            .arg(&src)
            .arg(format!("zapret-v{version}"))
            .status()
            .unwrap();
        assert!(status.success());
        let hex: String = Sha256::digest(fs::read(&file).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        (file, hex)
    }

    struct Fake {
        release: RefCell<(PathBuf, Release)>,
        active: Cell<bool>,
        fail_start: Cell<bool>,
        reloads: Cell<u32>,
    }

    impl Fake {
        fn new(root: &Path, version: &str) -> Self {
            let fake = Self {
                release: RefCell::new((
                    PathBuf::new(),
                    Release {
                        version: String::new(),
                        url: String::new(),
                        sha256: String::new(),
                    },
                )),
                active: Cell::new(false),
                fail_start: Cell::new(false),
                reloads: Cell::new(0),
            };
            fake.publish(root, version, "nfqws");
            fake
        }

        fn publish(&self, root: &Path, version: &str, nfqws: &str) {
            let (file, sha256) = tarball(root, version, nfqws);
            *self.release.borrow_mut() = (
                file,
                Release {
                    version: version.into(),
                    url: String::new(),
                    sha256,
                },
            );
        }
    }

    impl Host for Fake {
        fn latest(&self) -> Result<Release, String> {
            Ok(self.release.borrow().1.clone())
        }
        fn download(&self, _: &Release, to: &Path) -> Result<(), String> {
            fs::copy(&self.release.borrow().0, to)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        fn is_active(&self) -> bool {
            self.active.get()
        }
        fn start(&self) -> Result<(), String> {
            if self.fail_start.get() {
                return Err("nfqws: bad binary".into());
            }
            self.active.set(true);
            Ok(())
        }
        fn stop(&self) {
            self.active.set(false);
        }
        fn disable(&self) {
            self.active.set(false);
        }
        fn reload(&self) {
            self.reloads.set(self.reloads.get() + 1);
        }
    }

    fn paths(root: &Path) -> Paths {
        Paths {
            dir: root.join("opt/zapret"),
            unit: root.join("zapret.service"),
            system_unit: root.join("usr-lib-zapret.service"),
        }
    }

    fn steps() -> impl Fn(Step) {
        |_| {}
    }

    #[test]
    fn installs_the_latest_release_for_every_site() {
        let root = temp();
        fs::create_dir_all(root.join("opt")).unwrap();
        let (host, p) = (Fake::new(&root, "72.13"), paths(&root));
        assert_eq!(kind(&p), Kind::None);
        assert_eq!(install(&host, &p, &steps()).unwrap(), "72.13");
        assert_eq!(
            kind(&p),
            Kind::App {
                version: "72.13".into()
            }
        );
        let config = fs::read_to_string(p.dir.join("config")).unwrap();
        assert!(config.contains("NFQWS_ENABLE=1"));
        assert!(config.contains("FWTYPE=nftables"));
        assert_eq!(
            fs::read_to_string(p.dir.join("nfq/nfqws")).unwrap(),
            "nfqws\n"
        );
        assert!(fs::read_to_string(&p.unit).unwrap().contains("72.13"));
        assert!(!p.staging().exists());
        assert!(host.reloads.get() > 0);
        assert!(matches!(
            install(&host, &p, &steps()),
            Err(Error::AlreadyInstalled)
        ));
    }

    #[test]
    fn a_damaged_download_changes_nothing() {
        let root = temp();
        fs::create_dir_all(root.join("opt")).unwrap();
        let (host, p) = (Fake::new(&root, "72.13"), paths(&root));
        host.release.borrow_mut().1.sha256 = "00".repeat(32);
        assert!(matches!(install(&host, &p, &steps()), Err(Error::Corrupt)));
        assert_eq!(kind(&p), Kind::None);
        assert!(!p.staging().exists());
    }

    #[test]
    fn updates_keep_the_config_and_lists() {
        let root = temp();
        fs::create_dir_all(root.join("opt")).unwrap();
        let (host, p) = (Fake::new(&root, "72.8"), paths(&root));
        install(&host, &p, &steps()).unwrap();
        fs::write(
            p.dir.join("config"),
            "NFQWS_OPT=\"--dpi-desync=fake --dpi-desync-ttl=8\"\n",
        )
        .unwrap();
        fs::write(p.dir.join("ipset/zapret-hosts-user.txt"), "discord.com\n").unwrap();
        host.active.set(true);

        assert!(matches!(update(&host, &p, &steps()), Err(Error::UpToDate)));
        host.publish(&root, "72.13", "new nfqws");
        assert_eq!(update(&host, &p, &steps()).unwrap(), "72.13");
        assert_eq!(
            kind(&p),
            Kind::App {
                version: "72.13".into()
            }
        );
        assert!(
            fs::read_to_string(p.dir.join("config"))
                .unwrap()
                .contains("ttl=8")
        );
        assert_eq!(
            fs::read_to_string(p.dir.join("ipset/zapret-hosts-user.txt")).unwrap(),
            "discord.com\n"
        );
        assert_eq!(
            fs::read_to_string(p.dir.join("nfq/nfqws")).unwrap(),
            "new nfqws\n"
        );
        assert!(host.active.get());
        assert!(!p.old().exists());
    }

    #[test]
    fn a_failed_update_puts_the_old_zapret_back() {
        let root = temp();
        fs::create_dir_all(root.join("opt")).unwrap();
        let (host, p) = (Fake::new(&root, "72.8"), paths(&root));
        install(&host, &p, &steps()).unwrap();
        host.active.set(true);
        host.publish(&root, "72.13", "broken nfqws");
        host.fail_start.set(true);
        assert!(matches!(
            update(&host, &p, &steps()),
            Err(Error::ServiceFailed(_))
        ));
        assert_eq!(
            kind(&p),
            Kind::App {
                version: "72.8".into()
            }
        );
        assert_eq!(
            fs::read_to_string(p.dir.join("nfq/nfqws")).unwrap(),
            "nfqws\n"
        );
        assert!(fs::read_to_string(&p.unit).unwrap().contains("72.8"));
        assert!(!p.staging().exists() && !p.old().exists());
    }

    #[test]
    fn leaves_other_installs_alone() {
        let root = temp();
        let (host, p) = (Fake::new(&root, "72.13"), paths(&root));
        fs::create_dir_all(&p.dir).unwrap();
        fs::write(p.dir.join("config"), "").unwrap();
        assert_eq!(kind(&p), Kind::Manual);
        fs::write(&p.system_unit, "").unwrap();
        assert_eq!(kind(&p), Kind::System);
        assert!(matches!(update(&host, &p, &steps()), Err(Error::NotOurs)));
        assert!(matches!(uninstall(&host, &p), Err(Error::NotOurs)));
        assert!(p.dir.join("config").exists());
    }

    #[test]
    fn uninstalls_what_it_installed() {
        let root = temp();
        fs::create_dir_all(root.join("opt")).unwrap();
        let (host, p) = (Fake::new(&root, "72.13"), paths(&root));
        install(&host, &p, &steps()).unwrap();
        host.active.set(true);
        uninstall(&host, &p).unwrap();
        assert!(!p.dir.exists() && !p.unit.exists());
        assert!(!host.active.get());
        assert_eq!(kind(&p), Kind::None);
    }

    #[test]
    fn an_interrupted_update_is_undone() {
        let root = temp();
        let p = paths(&root);
        fs::create_dir_all(p.old()).unwrap();
        fs::create_dir_all(p.staging()).unwrap();
        clean_up(&p);
        assert!(p.dir.exists() && !p.old().exists() && !p.staging().exists());
    }

    #[test]
    fn versions_and_releases() {
        assert!(newer("72.13", "72.8"));
        assert!(newer("v73", "72.13"));
        assert!(!newer("72.8", "72.8"));
        let json = br#"{"tag_name":"v72.13","assets":[
            {"name":"zapret-v72.13.zip","browser_download_url":"https://x/z.zip","digest":"sha256:aa"},
            {"name":"zapret-v72.13.tar.gz","browser_download_url":"https://x/z.tar.gz","digest":"sha256:bb"}]}"#;
        assert_eq!(
            parse_release(json),
            Some(Release {
                version: "72.13".into(),
                url: "https://x/z.tar.gz".into(),
                sha256: "bb".into()
            })
        );
    }
}
