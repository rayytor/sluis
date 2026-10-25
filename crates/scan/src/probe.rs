//! Does a site open from a given range of source ports? One HTTPS request
//! with curl to an address resolved beforehand, so a test never waits for
//! DNS. Only IPv4: zapret's IPv6 handling is the same, and this PC's AAAA
//! lookups alone take five seconds.

use std::ffi::CString;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::ptr;
use std::time::Duration;

/// A range of local ports; the sandbox steers each range to its own nfqws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ports {
    pub first: u16,
    pub last: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub site: String,
    pub addr: Ipv4Addr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Any HTTP answer over verified TLS; an ISP can't fake that.
    Opens { ms: u32 },
    /// Connected, then cut or stalled during the handshake: DPI.
    Blocked,
    /// Something answered, but with a certificate that isn't the site's:
    /// an internet provider's block page, so the address itself is wrong.
    WrongSite,
    /// No connection at all: the address is blocked or the site is down.
    Unreachable,
}

impl Outcome {
    pub fn opens(self) -> bool {
        matches!(self, Self::Opens { .. })
    }
}

pub trait Prober: Sync {
    /// IPv4 addresses from the system resolver.
    fn resolve(&self, site: &str) -> Vec<Ipv4Addr>;
    /// IPv4 addresses from encrypted DNS, to tell DNS spoofing apart.
    fn resolve_encrypted(&self, site: &str) -> Vec<Ipv4Addr>;
    fn probe(&self, target: &Target, ports: Option<Ports>) -> Outcome;
}

/// The real prober. As root (in the helper), curl runs as `uid`/`gid`.
pub struct Curl {
    pub user: Option<(u32, u32)>,
    /// Seconds for a whole request; DPI usually stalls the handshake, so
    /// a blocked site costs this much.
    pub timeout: u32,
}

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) Sluis/0.9";

impl Curl {
    fn command(&self) -> Command {
        let mut command = Command::new("curl");
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        if let Some((uid, gid)) = self.user {
            use std::os::unix::process::CommandExt;
            command.uid(uid).gid(gid);
        }
        command
    }
}

impl Prober for Curl {
    fn resolve(&self, site: &str) -> Vec<Ipv4Addr> {
        resolve4(site)
    }

    fn resolve_encrypted(&self, site: &str) -> Vec<Ipv4Addr> {
        let url = format!("https://1.1.1.1/dns-query?name={site}&type=A");
        let output = self
            .command()
            .args(["--silent", "--ipv4", "--max-time", "4"])
            .args(["--header", "accept: application/dns-json", &url])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        parse_doh(&output.stdout)
    }

    fn probe(&self, target: &Target, ports: Option<Ports>) -> Outcome {
        let mut command = self.command();
        command
            .args(["--silent", "--head", "--ipv4", "--output", "/dev/null"])
            .args(["--write-out", "%{time_connect} %{time_total}"])
            .args(["--connect-timeout", &self.timeout.to_string()])
            .args(["--max-time", &self.timeout.to_string()])
            .args(["--user-agent", USER_AGENT])
            .arg("--resolve")
            .arg(format!("{}:443:{}", target.site, target.addr));
        if let Some(p) = ports {
            command
                .arg("--local-port")
                .arg(format!("{}-{}", p.first, p.last));
        }
        command.arg(format!("https://{}/", target.site));
        let outcome = match command.output() {
            Ok(output) => outcome(output.status.code(), &output.stdout),
            Err(_) => return Outcome::Unreachable,
        };
        // curl counts the TLS handshake as connecting, so a handshake DPI
        // stalls looks like no connection; ask TCP alone
        let address = SocketAddr::from((target.addr, 443));
        let timeout = Duration::from_secs(u64::from(self.timeout).min(3));
        match outcome {
            Outcome::Unreachable if TcpStream::connect_timeout(&address, timeout).is_ok() => {
                Outcome::Blocked
            }
            other => other,
        }
    }
}

/// curl's exit code and `time_connect time_total` to an outcome.
fn outcome(code: Option<i32>, stdout: &[u8]) -> Outcome {
    let text = String::from_utf8_lossy(stdout);
    let mut times = text
        .split_whitespace()
        .map(|t| t.parse::<f64>().unwrap_or(0.0));
    let connect = times.next().unwrap_or(0.0);
    let total = times.next().unwrap_or(0.0);
    match code {
        Some(0) => Outcome::Opens {
            ms: (total * 1000.0).round() as u32,
        },
        // the certificate can't be verified, or is for another name
        Some(51 | 60) => Outcome::WrongSite,
        _ if connect > 0.0 => Outcome::Blocked,
        _ => Outcome::Unreachable,
    }
}

fn parse_doh(json: &[u8]) -> Vec<Ipv4Addr> {
    #[derive(serde::Deserialize)]
    struct Answer {
        #[serde(rename = "type")]
        kind: u16,
        data: String,
    }
    #[derive(serde::Deserialize)]
    struct Reply {
        #[serde(rename = "Answer", default)]
        answer: Vec<Answer>,
    }
    serde_json::from_slice::<Reply>(json)
        .map(|r| {
            r.answer
                .iter()
                .filter(|a| a.kind == 1)
                .filter_map(|a| a.data.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// getaddrinfo for A records only (std asks for AAAA as well).
fn resolve4(site: &str) -> Vec<Ipv4Addr> {
    let Ok(host) = CString::new(site) else {
        return Vec::new();
    };
    // SAFETY: addrinfo is plain data; all-zero is the documented empty hint.
    let mut hints: libc::addrinfo = unsafe { std::mem::zeroed() };
    hints.ai_family = libc::AF_INET;
    hints.ai_socktype = libc::SOCK_STREAM;
    let mut list = ptr::null_mut();
    // SAFETY: valid C string and hints; `list` is freed below.
    if unsafe { libc::getaddrinfo(host.as_ptr(), ptr::null(), &hints, &mut list) } != 0 {
        return Vec::new();
    }
    let mut addrs = Vec::new();
    let mut entry = list;
    while !entry.is_null() {
        // SAFETY: a node of the list getaddrinfo returned.
        let info = unsafe { &*entry };
        if info.ai_family == libc::AF_INET && !info.ai_addr.is_null() {
            // SAFETY: AF_INET entries hold a sockaddr_in.
            let sin = unsafe { &*(info.ai_addr as *const libc::sockaddr_in) };
            let addr = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            if !addrs.contains(&addr) {
                addrs.push(addr);
            }
        }
        entry = info.ai_next;
    }
    // SAFETY: the list from the successful call above, freed once.
    unsafe { libc::freeaddrinfo(list) };
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes() {
        assert_eq!(
            outcome(Some(0), b"0.010 0.2504"),
            Outcome::Opens { ms: 250 }
        );
        assert_eq!(outcome(Some(28), b"0.012 4.0"), Outcome::Blocked);
        assert_eq!(outcome(Some(35), b"0.012 0.1"), Outcome::Blocked);
        assert_eq!(outcome(Some(60), b"0.012 0.1"), Outcome::WrongSite);
        assert_eq!(outcome(Some(28), b"0.000000 4.0"), Outcome::Unreachable);
        assert_eq!(outcome(Some(7), b""), Outcome::Unreachable);
    }

    #[test]
    fn doh_reply() {
        let json = br#"{"Status":0,"Answer":[{"name":"x.","type":5,"data":"y."},{"name":"y.","type":1,"data":"162.159.1.2"}]}"#;
        assert_eq!(parse_doh(json), vec![Ipv4Addr::new(162, 159, 1, 2)]);
        assert!(parse_doh(b"{}").is_empty());
        assert!(parse_doh(b"nonsense").is_empty());
    }
}
