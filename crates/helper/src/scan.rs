//! The quick scan's sandbox: a temporary nftables table that sends each
//! range of source ports to its own private nfqws, so methods are tried in
//! parallel without touching anyone's traffic or zapret's own nfqws.
//!
//! Test traffic gets zapret's desync mark, which zapret's rules skip; the
//! private nfqws mark their own packets with [`TEST_MARK`], which this
//! table skips. The nfqws die with the helper (PR_SET_PDEATHSIG), and the
//! table is removed when the sandbox is dropped and when the helper starts.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use sluis_core::{Config, Strategy};
use sluis_scan::{Ports, Sandbox};

pub const TABLE: &str = "sluis_scan";
const SLOTS: usize = 8;
/// Above zapret's own queue (200).
const FIRST_QUEUE: u16 = 300;
/// Below the kernel's ephemeral ports, so no other program's connection
/// lands in a test range by chance.
const RAW: Ports = Ports {
    first: 20000,
    last: 20999,
};
const SLOT_PORTS: u16 = 1000;
const TEST_MARK: u32 = 0x0800_0000;
const DEFAULT_DESYNC_MARK: u32 = 0x4000_0000;
/// The user curl and nfqws run as once set up.
pub const NOBODY: (u32, u32) = (65534, 65534);

pub struct NftSandbox {
    nfqws: PathBuf,
    slots: Vec<Mutex<Option<Child>>>,
}

impl NftSandbox {
    /// Adds the table for zapret in `dir`.
    pub fn start(dir: &str) -> io::Result<Self> {
        let config = std::fs::read_to_string(format!("{dir}/config"))
            .map(Config::parse)
            .ok();
        let desync_mark = config
            .as_ref()
            .and_then(|c| c.get("DESYNC_MARK")?.literal())
            .and_then(|m| u32::from_str_radix(m.trim_start_matches("0x"), 16).ok())
            .unwrap_or(DEFAULT_DESYNC_MARK);
        nft(&rules(desync_mark))?;
        Ok(Self {
            nfqws: format!("{dir}/nfq/nfqws").into(),
            slots: (0..SLOTS).map(|_| Mutex::new(None)).collect(),
        })
    }
}

impl Drop for NftSandbox {
    fn drop(&mut self) {
        for slot in &self.slots {
            if let Some(mut child) = slot.lock().unwrap().take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        clean_up();
    }
}

/// Removes the table if a crashed helper left it behind.
pub fn clean_up() {
    let _ = nft(&format!("destroy table inet {TABLE}\n"));
}

fn slot_ports(slot: usize) -> Ports {
    let first = RAW.last + 1 + SLOT_PORTS * slot as u16;
    Ports {
        first,
        last: first + SLOT_PORTS - 1,
    }
}

fn rules(desync_mark: u32) -> String {
    let all = format!("{}-{}", RAW.first, slot_ports(SLOTS - 1).last);
    let mut post = format!(
        "\t\tmeta mark & {TEST_MARK:#x} != 0 meta mark set meta mark | {desync_mark:#x} return\n\
         \t\ttcp sport {all} ct mark set ct mark | {TEST_MARK:#x} meta mark set meta mark | {desync_mark:#x}\n"
    );
    let mut pre = format!(
        "\t\ticmp type time-exceeded ct mark & {TEST_MARK:#x} != 0 drop\n\
         \t\ticmpv6 type time-exceeded ct mark & {TEST_MARK:#x} != 0 drop\n"
    );
    for slot in 0..SLOTS {
        let p = slot_ports(slot);
        let queue = FIRST_QUEUE + slot as u16;
        post.push_str(&format!(
            "\t\ttcp sport {}-{} tcp dport {{ 80, 443 }} ct original packets 1-12 queue num {queue} bypass\n",
            p.first, p.last
        ));
        pre.push_str(&format!(
            "\t\ttcp dport {}-{} tcp sport {{ 80, 443 }} ct reply packets 1-3 queue num {queue} bypass\n",
            p.first, p.last
        ));
    }
    // zapret's chains run at 99/101 (postrouting) and -99 (prerouting)
    format!(
        "destroy table inet {TABLE}\n\
         table inet {TABLE} {{\n\
         \tchain post {{\n\t\ttype filter hook postrouting priority 98; policy accept;\n{post}\t}}\n\
         \tchain pre {{\n\t\ttype filter hook prerouting priority -100; policy accept;\n{pre}\t}}\n\
         }}\n"
    )
}

fn nft(script: &str) -> io::Result<()> {
    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// nfqws prints this once its queue and raw sockets are set up.
const READY: &str = "initializing raw sockets";

impl Sandbox for NftSandbox {
    fn slots(&self) -> usize {
        SLOTS
    }

    fn raw(&self) -> Ports {
        RAW
    }

    fn ports(&self, slot: usize) -> Ports {
        slot_ports(slot)
    }

    fn load(&self, slot: usize, strategy: &Strategy) -> Result<(), String> {
        let mut current = self.slots[slot].lock().unwrap();
        if let Some(mut old) = current.take() {
            let _ = old.kill();
            let _ = old.wait();
        }
        let (reader, writer) = io::pipe().map_err(|e| e.to_string())?;
        let mut command = Command::new(&self.nfqws);
        command
            .arg(format!("--qnum={}", FIRST_QUEUE + slot as u16))
            .arg(format!("--uid={}:{}", NOBODY.0, NOBODY.1))
            .arg(format!("--dpi-desync-fwmark={TEST_MARK:#x}"))
            .args(strategy.args())
            .env_clear()
            .stdin(Stdio::null())
            .stdout(writer.try_clone().map_err(|e| e.to_string())?)
            .stderr(writer);
        // SAFETY: prctl is async-signal-safe; nothing else runs here.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        drop(command); // closes the parent's copies of the pipe's write end

        // wait for READY, keep draining the pipe afterwards
        let (ready, wait) = mpsc::channel();
        std::thread::spawn(move || {
            let mut log = String::new();
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                if line.contains(READY) {
                    let _ = ready.send(Ok(()));
                } else if log.len() < 2000 {
                    log.push_str(&line);
                    log.push('\n');
                }
            }
            let _ = ready.send(Err(log));
        });
        match wait.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => {
                *current = Some(child);
                Ok(())
            }
            Ok(Err(log)) => {
                let _ = child.wait();
                Err(log)
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                Err("nfqws did not start".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_do_not_overlap() {
        assert_eq!(slot_ports(0).first, RAW.last + 1);
        for slot in 1..SLOTS {
            assert_eq!(slot_ports(slot).first, slot_ports(slot - 1).last + 1);
        }
        assert!(slot_ports(SLOTS - 1).last < 32768);
    }

    #[test]
    fn rules_steer_each_range_to_its_queue() {
        let rules = rules(DEFAULT_DESYNC_MARK);
        assert!(rules.starts_with("destroy table inet sluis_scan\n"));
        assert!(rules.contains("tcp sport 21000-21999 tcp dport { 80, 443 } ct original packets 1-12 queue num 300 bypass"));
        assert!(rules.contains(
            "tcp dport 28000-28999 tcp sport { 80, 443 } ct reply packets 1-3 queue num 307 bypass"
        ));
        assert!(
            rules
                .contains("meta mark & 0x8000000 != 0 meta mark set meta mark | 0x40000000 return")
        );
        assert!(rules.contains("tcp sport 20000-28999 ct mark set"));
    }
}
