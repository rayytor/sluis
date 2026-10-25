//! The Thorough Scan: blockcheck.sh with zapret paused, since its own
//! nfqws would open every site and every method would look like it works.
//! zapret comes back when the scan ends, and when the helper starts after
//! a crash (a marker in its runtime directory says it was paused).

use std::io::{self, BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sluis_scan::blockcheck::{self, Line, Parser};

/// systemd's RuntimeDirectory=, kept while the helper isn't running.
const PAUSED: &str = "/run/sluis-helper/zapret-paused";
const UNIT: &str = "zapret.service";

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Turns zapret back on if a scan paused it and never finished.
pub fn resume_if_paused() {
    if std::fs::remove_file(PAUSED).is_ok() {
        systemctl(&["start", UNIT]);
    }
}

/// Pauses zapret while alive; starts it again when dropped.
struct Pause {
    was_running: bool,
}

impl Pause {
    fn new() -> io::Result<Self> {
        let was_running = systemctl(&["is-active", "--quiet", UNIT]);
        if was_running {
            std::fs::write(PAUSED, "")?;
            systemctl(&["stop", UNIT]);
        }
        Ok(Self { was_running })
    }
}

impl Drop for Pause {
    fn drop(&mut self) {
        if self.was_running {
            systemctl(&["start", UNIT]);
            let _ = std::fs::remove_file(PAUSED);
        }
    }
}

/// Runs blockcheck for `sites` in zapret's directory `dir`, handing each
/// line it understood to `on_line` with the number of attempts so far.
pub fn run(
    dir: &str,
    sites: &[String],
    stop: &AtomicBool,
    on_line: &dyn Fn(Line, usize),
) -> Result<(), String> {
    let _pause = Pause::new().map_err(|e| e.to_string())?;
    let (reader, writer) = io::pipe().map_err(|e| e.to_string())?;
    let mut command = Command::new("sh");
    command
        .arg(format!("{dir}/blockcheck.sh"))
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .envs(blockcheck::environment(sites))
        .stdin(Stdio::null())
        .stdout(writer.try_clone().map_err(|e| e.to_string())?)
        .stderr(writer)
        // its own group, so Stop reaches the nfqws and curl it starts
        .process_group(0);
    // SAFETY: prctl is async-signal-safe; nothing else runs here.
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    drop(command);
    let group = child.id() as libc::pid_t;

    let done = AtomicBool::new(false);
    let done = &done;
    std::thread::scope(|scope| {
        // Stop: SIGINT runs blockcheck's own cleanup; SIGKILL if it hangs
        scope.spawn(move || {
            while !done.load(Ordering::SeqCst) && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(200));
            }
            if done.load(Ordering::SeqCst) {
                return;
            }
            // SAFETY: signals our own child's process group.
            unsafe { libc::kill(-group, libc::SIGINT) };
            let asked = Instant::now();
            while !done.load(Ordering::SeqCst) {
                if asked.elapsed() > Duration::from_secs(10) {
                    // SAFETY: as above.
                    unsafe { libc::kill(-group, libc::SIGKILL) };
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        let mut parser = Parser::default();
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let parsed = parser.line(&line);
            if parsed != Line::Other {
                on_line(parsed, parser.tried());
            }
        }
        let _ = child.wait();
        done.store(true, Ordering::SeqCst);
    });
    Ok(())
}
