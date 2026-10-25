//! The methods Sluis knows, with how the user's sites did the last time
//! each one was in use. Kept in `~/.local/share/sluis/methods.json`;
//! the first time, the strategies saved by the old app are imported.
//! Also which method opened every site on each network, so switching
//! networks can switch methods too.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;
use serde::{Deserialize, Serialize};
use sluis_core::Strategy;

/// Speeds are compared in steps of this many milliseconds, about what one
/// scan's measurements vary by, so a difference nobody would notice doesn't
/// reorder methods.
pub const SAME_SPEED: u32 = 50;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub opened: Vec<String>,
    pub blocked: Vec<String>,
    /// Seconds since 1970.
    pub checked: u64,
    /// How long a blocked site took to open with it (the median), from the
    /// last scan that measured it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Method {
    /// nfqws arguments without site filters ([`Strategy::method`]).
    pub args: String,
    #[serde(default)]
    pub result: Option<Outcome>,
}

impl Method {
    pub fn strategy(&self) -> Strategy {
        Strategy::parse(&self.args).expect("stored methods are checked on load")
    }

    /// The share of the given sites that opened last time, or `None` if it
    /// was never checked with any of them.
    fn score(&self, sites: &[String]) -> Option<(usize, usize)> {
        let r = self.result.as_ref()?;
        let opened = sites.iter().filter(|s| r.opened.contains(s)).count();
        let blocked = sites.iter().filter(|s| r.blocked.contains(s)).count();
        (opened + blocked > 0).then_some((opened, blocked))
    }
}

/// Where a method stands for the user's current sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Opened every site checked.
    Works,
    /// Some sites stayed blocked.
    Partly {
        opened: usize,
        total: usize,
    },
    NotTested,
}

/// The method that worked on one network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// What the network is called, for messages.
    pub name: String,
    pub method: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    methods: Vec<Method>,
    /// network id (see network.rs) → its method
    #[serde(default)]
    networks: BTreeMap<String, Saved>,
}

impl Store {
    fn path() -> PathBuf {
        glib::user_data_dir().join("sluis").join("methods.json")
    }

    pub fn load() -> Self {
        let mut store: Store = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(Self::import_0_5);
        // drop anything that no longer passes the parser
        store.methods.retain(|m| Strategy::parse(&m.args).is_ok());
        store
    }

    /// The strategies the old app saved, once each.
    fn import_0_5() -> Self {
        #[derive(Deserialize)]
        struct Old {
            strategy: String,
        }
        let path = glib::user_config_dir()
            .join("zapret-gtk")
            .join("strategies.json");
        let old: Vec<Old> = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let mut store = Self::default();
        for o in old {
            if let Some(method) = Strategy::parse(&o.strategy).ok().and_then(|s| s.method()) {
                store.add(&method);
            }
        }
        store
    }

    pub fn save(&self) {
        let path = Self::path();
        let result = std::fs::create_dir_all(path.parent().unwrap()).and_then(|_| {
            std::fs::write(
                &path,
                serde_json::to_string_pretty(self).unwrap_or_default(),
            )
        });
        if let Err(e) = result {
            eprintln!("cannot save {}: {e}", path.display());
        }
    }

    /// Adds a method if it is new. Returns its arguments as stored.
    pub fn add(&mut self, method: &Strategy) -> String {
        let args = method.to_string();
        if !self.methods.iter().any(|m| m.args == args) {
            self.methods.push(Method {
                args: args.clone(),
                result: None,
            });
        }
        args
    }

    /// Records how the sites did with a method. `ms` is its speed when a
    /// scan measured it; a check of the sites alone keeps the last one, and
    /// a method that opened nothing has no speed.
    pub fn record(
        &mut self,
        args: &str,
        opened: Vec<String>,
        blocked: Vec<String>,
        ms: Option<u32>,
    ) {
        let checked = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        if let Some(m) = self.methods.iter_mut().find(|m| m.args == args) {
            let ms = if opened.is_empty() {
                None
            } else {
                ms.or_else(|| m.result.as_ref().and_then(|r| r.ms))
            };
            m.result = Some(Outcome {
                opened,
                blocked,
                checked,
                ms,
            });
        }
    }

    /// Remembers that `method` opened every site on network `id`.
    pub fn remember(&mut self, id: &str, name: &str, method: &str) {
        self.networks.insert(
            id.to_owned(),
            Saved {
                name: name.to_owned(),
                method: method.to_owned(),
            },
        );
    }

    pub fn saved(&self, id: &str) -> Option<&Saved> {
        self.networks.get(id)
    }

    pub fn get(&self, args: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.args == args)
    }

    pub fn standing(&self, args: &str, sites: &[String]) -> Standing {
        match self.get(args).and_then(|m| m.score(sites)) {
            Some((opened, 0)) if opened > 0 => Standing::Works,
            Some((opened, blocked)) => Standing::Partly {
                opened,
                total: opened + blocked,
            },
            None => Standing::NotTested,
        }
    }

    /// Best first: methods that opened every site, then the most sites
    /// opened, then untested ones; among those, the fastest (in
    /// [`SAME_SPEED`] steps), then the one in use, so "Method 1" doesn't
    /// change for a difference nobody would notice, then the simplest.
    /// Method numbers come from this order, and the scan dialog ranks the
    /// same way, so its "Method N" match.
    pub fn ranked(&self, sites: &[String], in_use: Option<&str>) -> Vec<&Method> {
        let mut ranked: Vec<&Method> = self.methods.iter().collect();
        ranked.sort_by_key(|m| {
            let other = Some(m.args.as_str()) != in_use;
            let simpler = m.args.split_whitespace().count();
            let speed = m
                .result
                .as_ref()
                .and_then(|r| r.ms)
                .map_or(u32::MAX, |ms| ms / SAME_SPEED);
            match m.score(sites) {
                Some((opened, 0)) => (0, Reverse(opened), 0, speed, other, simpler, &m.args),
                Some((opened, blocked)) => {
                    (1, Reverse(opened), blocked, speed, other, simpler, &m.args)
                }
                None => (2, Reverse(0), 0, u32::MAX, other, simpler, &m.args),
            }
        });
        ranked
    }

    /// The method that opens every site fastest, when there is more than
    /// one to choose from and their speeds were measured.
    pub fn fastest(&self, sites: &[String]) -> Option<&str> {
        let working: Vec<(&Method, u32)> = self
            .methods
            .iter()
            .filter(|m| self.standing(&m.args, sites) == Standing::Works)
            .filter_map(|m| Some((m, m.result.as_ref()?.ms?)))
            .collect();
        (working.len() > 1)
            .then(|| working.iter().min_by_key(|(_, ms)| *ms))
            .flatten()
            .map(|(m, _)| m.args.as_str())
    }

    /// "Method N" for the method with these arguments.
    pub fn number(&self, args: &str, sites: &[String], in_use: Option<&str>) -> Option<usize> {
        self.ranked(sites, in_use)
            .iter()
            .position(|m| m.args == args)
            .map(|i| i + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL8: &str = "--dpi-desync=fake --dpi-desync-ttl=8";
    const TTL9: &str = "--dpi-desync=fake --dpi-desync-ttl=9";
    const MD5: &str = "--dpi-desync=fake --dpi-desync-fooling=md5sig";

    fn sites() -> Vec<String> {
        vec!["discord.com".into(), "roblox.com".into()]
    }

    fn store(methods: &[(&str, Option<u32>)]) -> Store {
        let mut store = Store::default();
        for (args, ms) in methods {
            let args = store.add(&Strategy::parse(args).unwrap());
            store.record(&args, sites(), Vec::new(), *ms);
        }
        store
    }

    fn order<'a>(store: &'a Store, in_use: Option<&str>) -> Vec<&'a str> {
        store
            .ranked(&sites(), in_use)
            .iter()
            .map(|m| m.args.as_str())
            .collect()
    }

    #[test]
    fn the_fastest_working_method_comes_first() {
        let store = store(&[(TTL8, Some(900)), (TTL9, Some(300)), (MD5, None)]);
        assert_eq!(order(&store, Some(TTL8)), [TTL9, TTL8, MD5]);
        assert_eq!(store.fastest(&sites()), Some(TTL9));
    }

    #[test]
    fn opening_more_sites_beats_speed() {
        let mut store = store(&[(TTL8, Some(900)), (TTL9, Some(100))]);
        store.record(
            TTL9,
            vec!["discord.com".into()],
            vec!["roblox.com".into()],
            Some(100),
        );
        assert_eq!(order(&store, None), [TTL8, TTL9]);
        // one working method: nothing to compare it with
        assert_eq!(store.fastest(&sites()), None);
    }

    #[test]
    fn the_method_in_use_wins_a_tie() {
        let store = store(&[(TTL8, Some(320)), (TTL9, Some(310))]);
        assert_eq!(order(&store, Some(TTL9)), [TTL9, TTL8]);
        assert_eq!(order(&store, Some(TTL8)), [TTL8, TTL9]);
    }

    #[test]
    fn a_check_without_a_scan_keeps_the_speed() {
        let mut store = store(&[(TTL8, Some(400))]);
        store.record(TTL8, sites(), Vec::new(), None);
        assert_eq!(
            store.get(TTL8).unwrap().result.as_ref().unwrap().ms,
            Some(400)
        );
    }

    #[test]
    fn stores_from_before_speeds_still_load() {
        let old = r#"{"methods": [{"args": "--dpi-desync=fake --dpi-desync-ttl=8",
            "result": {"opened": ["discord.com"], "blocked": [], "checked": 1}}]}"#;
        let store: Store = serde_json::from_str(old).unwrap();
        assert_eq!(store.get(TTL8).unwrap().result.as_ref().unwrap().ms, None);
    }
}
