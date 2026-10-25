//! The Thorough Scan: zapret's own `blockcheck.sh`, run unattended, its
//! output read line by line into the same results as the quick scan.
//!
//! blockcheck prints each attempt as
//! `- curl_test_https_tls12 ipv4 discord.com : nfqws --dpi-desync=fake ...`
//! and then `!!!!! AVAILABLE !!!!!` when the site opened with it.

use std::collections::BTreeMap;

use sluis_core::Strategy;

/// Environment for an unattended run: HTTPS (TLS 1.2, which DPI reads) over
/// IPv4 with nfqws only, no questions asked.
pub fn environment(sites: &[String]) -> Vec<(&'static str, String)> {
    [
        ("BATCH", "1"),
        ("IPVS", "4"),
        ("ENABLE_HTTP", "0"),
        ("ENABLE_HTTPS_TLS12", "1"),
        ("ENABLE_HTTPS_TLS13", "0"),
        ("ENABLE_HTTP3", "0"),
        ("REPEATS", "2"),
        ("PARALLEL", "0"),
        ("SCANLEVEL", "standard"),
        ("SKIP_TPWS", "1"),
        ("SKIP_DNSCHECK", "1"),
        ("SKIP_IPBLOCK", "1"),
    ]
    .into_iter()
    .map(|(k, v)| (k, v.to_owned()))
    .chain([("DOMAINS", sites.join(" "))])
    .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// An attempt started.
    Trying,
    /// `site` opens without any bypass; it needs no method.
    OpensWithout {
        site: String,
    },
    /// The last attempt opened `site`; `opened` is every site this method
    /// opened so far.
    Works {
        strategy: String,
        site: String,
        opened: Vec<String>,
    },
    Other,
}

#[derive(Debug, Default)]
pub struct Parser {
    /// The site of the test section (`* curl_test_... ipv4 site`).
    section: String,
    /// The attempt running: a method and its site, or `None` for the
    /// check without bypass.
    current: Option<Option<(String, String)>>,
    tried: usize,
    /// strategy → the sites it opened
    works: BTreeMap<String, Vec<String>>,
}

impl Parser {
    pub fn tried(&self) -> usize {
        self.tried
    }

    /// Methods that worked, with the sites each opened.
    pub fn works(&self) -> &BTreeMap<String, Vec<String>> {
        &self.works
    }

    pub fn line(&mut self, line: &str) -> Line {
        let line = line.trim();
        // "* curl_test_https_tls12 ipv4 <site>"
        if let Some(rest) = line.strip_prefix("* curl_test") {
            self.section = rest
                .split_whitespace()
                .nth(2)
                .unwrap_or_default()
                .to_owned();
            return Line::Other;
        }
        if line == "- checking without DPI bypass" {
            self.current = Some(None);
            return Line::Other;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            self.current = None;
            // "<test> ipv4 <site> : nfqws <args>"
            let Some((head, command)) = rest.split_once(" : ") else {
                return Line::Other;
            };
            let site = head.split_whitespace().nth(2).unwrap_or_default();
            self.tried += 1;
            let Some(args) = command.strip_prefix("nfqws ") else {
                self.current = None;
                return Line::Other;
            };
            // blockcheck's own options and anything the app won't run
            self.current = Strategy::parse(args)
                .ok()
                .and_then(|s| s.method())
                .map(|s| Some((s.to_string(), site.to_owned())));
            return Line::Trying;
        }
        if !line.contains("!!!!! AVAILABLE !!!!!") {
            return Line::Other;
        }
        match self.current.take() {
            Some(None) => Line::OpensWithout {
                site: self.section.clone(),
            },
            Some(Some((strategy, site))) => {
                let opened = self.works.entry(strategy.clone()).or_default();
                if !opened.contains(&site) {
                    opened.push(site.clone());
                }
                Line::Works {
                    opened: opened.clone(),
                    strategy,
                    site,
                }
            }
            None => Line::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = "\
* checking already running DPI bypass processes

* curl_test_https_tls12 ipv4 youtube.com
- checking without DPI bypass
!!!!! AVAILABLE !!!!!

* curl_test_https_tls12 ipv4 discord.com
- checking without DPI bypass
UNAVAILABLE code=28
- curl_test_https_tls12 ipv4 discord.com : nfqws --dpi-desync=multisplit --dpi-desync-split-pos=1
UNAVAILABLE code=28
- curl_test_https_tls12 ipv4 discord.com : nfqws --dpi-desync=fake --dpi-desync-ttl=8
[attempt 1] AVAILABLE
[attempt 2] AVAILABLE
!!!!! AVAILABLE !!!!!
- curl_test_https_tls12 ipv4 roblox.com : nfqws --dpi-desync=fake --dpi-desync-ttl=8
!!!!! AVAILABLE !!!!!
- curl_test_https_tls12 ipv4 roblox.com : nfqws --dpi-desync=fake --dpi-desync-ttl=8 --user=root
!!!!! AVAILABLE !!!!!
- curl_test_https_tls12 ipv4 roblox.com : tpws --split-pos=1
!!!!! AVAILABLE !!!!!
* SUMMARY
curl_test_https_tls12 ipv4 discord.com : nfqws --dpi-desync=fake --dpi-desync-ttl=8
";

    #[test]
    fn reads_what_worked_for_which_site() {
        let mut parser = Parser::default();
        let lines: Vec<Line> = OUTPUT.lines().map(|l| parser.line(l)).collect();
        assert_eq!(parser.tried(), 5);
        let works: Vec<&Line> = lines
            .iter()
            .filter(|l| matches!(l, Line::Works { .. }))
            .collect();
        assert_eq!(works.len(), 2);
        assert!(lines.contains(&Line::OpensWithout {
            site: "youtube.com".into()
        }));
        assert_eq!(
            lines
                .iter()
                .filter(|l| matches!(l, Line::OpensWithout { .. }))
                .count(),
            1
        );
        assert_eq!(
            works[1],
            &Line::Works {
                strategy: "--dpi-desync=fake --dpi-desync-ttl=8".into(),
                site: "roblox.com".into(),
                opened: vec!["discord.com".into(), "roblox.com".into()],
            }
        );
        // --user=root is refused by the parser, tpws isn't run by the app
        assert_eq!(parser.works().len(), 1);
    }

    #[test]
    fn unattended() {
        let env = environment(&["discord.com".into(), "roblox.com".into()]);
        assert!(env.contains(&("BATCH", "1".into())));
        assert!(env.contains(&("DOMAINS", "discord.com roblox.com".into())));
    }
}
