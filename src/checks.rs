//! Does a site open? One HTTPS request with curl, the way a browser would
//! start: any HTTP answer means the site opens; a connection cut during
//! the TLS handshake is what DPI blocking looks like.
//!
//! When it doesn't open, the internet provider's DNS may be the reason:
//! some send blocked sites to a block server, which cuts the connection or
//! answers with a certificate that isn't the site's. Then the address that
//! encrypted DNS gives is tried too; if the site opens there, no method can
//! help, and secure DNS does.

use std::net::IpAddr;

use gettextrs::gettext;
use gtk::gio;
use gtk::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    Pending,
    Opens,
    /// Not reachable, with the reason in plain words.
    Blocked(String),
    /// The internet provider's DNS gives a wrong address for it. Says
    /// nothing about the method in use: the site never gets that far.
    WrongAddress,
}

impl Check {
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked(_))
    }
}

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) Sluis/0.9";

/// curl's exit code, or `None` when it couldn't run.
async fn curl(args: &[&str]) -> Option<i32> {
    let argv: Vec<&std::ffi::OsStr> = std::iter::once("curl")
        .chain(args.iter().copied())
        .map(std::ffi::OsStr::new)
        .collect();
    let process = gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .ok()?;
    process.wait_future().await.ok()?;
    Some(process.exit_status())
}

/// The site's addresses from encrypted DNS (Cloudflare's, over HTTPS),
/// which an internet provider can't change.
async fn resolve_encrypted(domain: &str) -> Vec<IpAddr> {
    let mut addrs = Vec::new();
    for kind in ["A", "AAAA"] {
        let url = format!("https://1.1.1.1/dns-query?name={domain}&type={kind}");
        let argv = [
            "curl",
            "--silent",
            "--max-time",
            "5",
            "--header",
            "accept: application/dns-json",
            url.as_str(),
        ];
        let Ok(process) = gio::Subprocess::newv(
            &argv.map(std::ffi::OsStr::new),
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
        ) else {
            continue;
        };
        if let Ok((Some(out), _)) = process.communicate_future(None).await {
            addrs.extend(parse_doh(&out));
        }
    }
    addrs
}

fn parse_doh(json: &[u8]) -> Vec<IpAddr> {
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
                // A and AAAA, not the CNAMEs on the way
                .filter(|a| a.kind == 1 || a.kind == 28)
                .filter_map(|a| a.data.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The addresses the system's DNS gives, which may be the provider's.
async fn resolve_system(domain: &str) -> Vec<IpAddr> {
    gio::Resolver::default()
        .lookup_by_name_future(domain)
        .await
        .map(|list| {
            list.iter()
                .filter_map(|a| a.to_string().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// The certificate can't be verified, or is for another name: whoever
/// answered isn't the site.
fn wrong_certificate(code: i32) -> bool {
    matches!(code, 51 | 60)
}

pub async fn check(domain: &str) -> Check {
    let url = format!("https://{domain}/");
    let common = [
        "--silent",
        "--output",
        "/dev/null",
        "--max-time",
        "12",
        "--user-agent",
        USER_AGENT,
    ];
    let mut args: Vec<&str> = common.to_vec();
    args.push(&url);
    let Some(code) = curl(&args).await else {
        return Check::Blocked(gettext("Couldn’t check"));
    };
    if code == 0 {
        return Check::Opens;
    }

    // Does it open at its real address?
    let system = resolve_system(domain).await;
    let real = resolve_encrypted(domain).await;
    if let Some(addr) = real.iter().find(|a| !system.contains(a)) {
        let pin = match addr {
            IpAddr::V4(a) => format!("{domain}:443:{a}"),
            IpAddr::V6(a) => format!("{domain}:443:[{a}]"),
        };
        let mut args: Vec<&str> = common.to_vec();
        args.extend(["--resolve", &pin, &url]);
        if curl(&args).await == Some(0) {
            return Check::WrongAddress;
        }
    }
    if wrong_certificate(code) && !real.is_empty() {
        // a block page; the real address may be blocked as well, but the
        // address has to be fixed first
        return Check::WrongAddress;
    }

    Check::Blocked(match code {
        6 => gettext("Can’t find this site"),
        7 => gettext("Can’t connect"),
        28 => gettext("Took too long to answer"),
        // SSL connect error, receive failure, empty reply: cut mid-handshake
        35 | 52 | 56 => gettext("Blocked by your internet provider"),
        _ => gettext("Doesn’t open"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doh_reply_has_both_kinds_of_address() {
        let json = br#"{"Status":0,"Answer":[
            {"name":"x.","type":5,"data":"y."},
            {"name":"y.","type":1,"data":"162.159.136.232"},
            {"name":"y.","type":28,"data":"2606:4700::6810:1"}]}"#;
        assert_eq!(
            parse_doh(json),
            vec![
                "162.159.136.232".parse::<IpAddr>().unwrap(),
                "2606:4700::6810:1".parse().unwrap()
            ]
        );
        assert!(parse_doh(b"nonsense").is_empty());
    }

    #[test]
    fn a_block_page_has_the_wrong_certificate() {
        assert!(wrong_certificate(60));
        assert!(wrong_certificate(51));
        assert!(!wrong_certificate(35));
        assert!(!wrong_certificate(28));
    }
}
