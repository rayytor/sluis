//! A quick scan: find out which sites are blocked and how, then try methods
//! on several private nfqws instances at once until enough of them open
//! every blocked site, twice, without breaking sites that already opened.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;

use sluis_core::Strategy;

use crate::candidates;
use crate::probe::{Outcome, Ports, Prober, Target};

/// The private nfqws instances the helper runs for a scan.
pub trait Sandbox: Sync {
    fn slots(&self) -> usize;
    /// Source ports that bypass every nfqws, zapret's own included.
    fn raw(&self) -> Ports;
    /// Source ports that go through slot `slot`'s nfqws.
    fn ports(&self, slot: usize) -> Ports;
    /// Restarts slot `slot`'s nfqws with `strategy`; the error says why it
    /// did not start.
    fn load(&self, slot: usize, strategy: &Strategy) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagnosis {
    Opens,
    /// The name doesn't resolve, even over encrypted DNS.
    NotFound,
    /// The ISP's DNS gives a wrong address; the real one opens.
    DnsSpoofed,
    /// Connections are cut during the handshake: what zapret fixes.
    Blocked,
    /// Both: the ISP's DNS gives a wrong address, and at the real one
    /// connections are cut. Zapret fixes the second; the first needs
    /// encrypted DNS.
    SpoofedAndBlocked,
    /// No connection at all; zapret can't fix that.
    Unreachable,
}

impl Diagnosis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opens => "opens",
            Self::NotFound => "not-found",
            Self::DnsSpoofed => "dns-spoofed",
            Self::Blocked => "blocked",
            Self::SpoofedAndBlocked => "dns-spoofed-blocked",
            Self::Unreachable => "unreachable",
        }
    }
}

/// How one method did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tried {
    pub strategy: String,
    /// Blocked sites it opened, both times.
    pub opened: Vec<String>,
    pub blocked: Vec<String>,
    /// Sites that opened without it and don't with it.
    pub broken: Vec<String>,
    /// Median time to open a blocked site.
    pub ms: u32,
    pub passed: bool,
    /// Place in the candidate list (lower is more common).
    pub order: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Diagnosed {
        site: String,
        diagnosis: Diagnosis,
        /// The address tests use.
        addr: Option<Ipv4Addr>,
    },
    Tried(Tried),
    Progress {
        done: usize,
        total: usize,
    },
}

pub struct Request {
    /// The sites to open (normalized domains).
    pub sites: Vec<String>,
    /// Extra sites that must keep opening, besides those of `sites` that
    /// already open; the shipped defaults when empty.
    pub controls: Vec<String>,
    /// Methods to try before the shortlist, such as the last one that
    /// worked on this network.
    pub first: Vec<Strategy>,
    /// Stop after this many methods pass; 0 tries them all.
    pub want: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub diagnosis: Vec<(String, Diagnosis)>,
    /// Best first, see [`rank`].
    pub tried: Vec<Tried>,
    pub stopped: bool,
}

pub fn run(
    prober: &dyn Prober,
    sandbox: &dyn Sandbox,
    request: &Request,
    stop: &AtomicBool,
    on_event: &(dyn Fn(Event) + Sync),
) -> Report {
    let limited = Limited {
        prober,
        free: Mutex::new(MAX_PROBES),
        freed: Condvar::new(),
    };
    let prober: &dyn Prober = &limited;
    let controls = if request.controls.is_empty() {
        candidates::default_controls()
    } else {
        request.controls.clone()
    };
    let mut everything = request.sites.clone();
    everything.extend(
        controls
            .iter()
            .filter(|c| !request.sites.contains(c))
            .cloned(),
    );

    // 1. diagnose, without any bypass
    let found = parallel(&everything, |site| diagnose(prober, sandbox.raw(), site));
    let mut report = Report::default();
    let mut targets = Vec::new();
    let mut intact = Vec::new();
    for (site, (diagnosis, target)) in everything.iter().zip(found) {
        let asked = request.sites.contains(site);
        if asked {
            report.diagnosis.push((site.clone(), diagnosis));
            on_event(Event::Diagnosed {
                site: site.clone(),
                diagnosis,
                addr: target.as_ref().map(|t| t.addr),
            });
        }
        match (diagnosis, target) {
            (Diagnosis::Blocked | Diagnosis::SpoofedAndBlocked, Some(t)) if asked => {
                targets.push(t)
            }
            (Diagnosis::Opens, Some(t)) => intact.push(t),
            _ => {}
        }
    }
    if targets.is_empty() || stop.load(Ordering::SeqCst) {
        report.stopped = stop.load(Ordering::SeqCst);
        return report;
    }

    // 2. candidates, the caller's first
    let mut list: Vec<Strategy> = Vec::new();
    for strategy in request.first.iter().cloned().chain(candidates::shortlist()) {
        if !list.iter().any(|s| s.args() == strategy.args()) {
            list.push(strategy);
        }
    }

    // 3. one worker per slot, taking the next candidate until done
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let passed = AtomicUsize::new(0);
    let tried = Mutex::new(Vec::new());
    let enough = || request.want > 0 && passed.load(Ordering::SeqCst) >= request.want;
    on_event(Event::Progress {
        done: 0,
        total: list.len(),
    });
    thread::scope(|scope| {
        for slot in 0..sandbox.slots() {
            let (list, targets, intact) = (&list, &targets, &intact);
            let (next, done, passed, tried, enough) = (&next, &done, &passed, &tried, &enough);
            scope.spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) || enough() {
                        return;
                    }
                    let order = next.fetch_add(1, Ordering::SeqCst);
                    let Some(strategy) = list.get(order) else {
                        return;
                    };
                    let result = match sandbox.load(slot, strategy) {
                        Ok(()) => try_one(prober, sandbox.ports(slot), targets, intact, stop),
                        Err(_) => None,
                    };
                    let result = result.unwrap_or_else(|| Trial {
                        blocked: targets.iter().map(|t| t.site.clone()).collect(),
                        ..Trial::default()
                    });
                    let t = result.finish(strategy, order);
                    if t.passed {
                        passed.fetch_add(1, Ordering::SeqCst);
                    }
                    tried.lock().unwrap().push(t.clone());
                    on_event(Event::Tried(t));
                    on_event(Event::Progress {
                        done: done.fetch_add(1, Ordering::SeqCst) + 1,
                        total: list.len(),
                    });
                }
            });
        }
    });

    report.tried = tried.into_inner().unwrap();
    rank(&mut report.tried);
    report.stopped = stop.load(Ordering::SeqCst);
    report
}

/// Requests in flight at once, over all slots. On the PC this was
/// measured on, blocked sites took 0.8 s with 4 at once, 1.2 s with 16 and
/// 2.4 s with 32, even through the working zapret: the ISP's DPI slows
/// down, and too many at once look like a method that doesn't work.
const MAX_PROBES: usize = 12;

/// A prober that lets at most [`MAX_PROBES`] requests run at once.
struct Limited<'a> {
    prober: &'a dyn Prober,
    free: Mutex<usize>,
    freed: Condvar,
}

impl Prober for Limited<'_> {
    fn resolve(&self, site: &str) -> Vec<Ipv4Addr> {
        self.prober.resolve(site)
    }

    fn resolve_encrypted(&self, site: &str) -> Vec<Ipv4Addr> {
        self.prober.resolve_encrypted(site)
    }

    fn probe(&self, target: &Target, ports: Option<Ports>) -> Outcome {
        {
            let free = self.free.lock().unwrap();
            let mut free = self.freed.wait_while(free, |f| *f == 0).unwrap();
            *free -= 1;
        }
        let outcome = self.prober.probe(target, ports);
        *self.free.lock().unwrap() += 1;
        self.freed.notify_one();
        outcome
    }
}

/// Best first: passed, fewer sites left blocked, fewer broken, faster,
/// simpler (fewer options), more common.
pub fn rank(tried: &mut [Tried]) {
    tried.sort_by_key(|t| {
        (
            !t.passed,
            t.blocked.len(),
            t.broken.len(),
            t.ms / 50,
            t.strategy.split_whitespace().count(),
            t.order,
        )
    });
}

fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    thread::scope(|scope| {
        let handles: Vec<_> = items.iter().map(|i| scope.spawn(|| f(i))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

fn diagnose(prober: &dyn Prober, raw: Ports, site: &str) -> (Diagnosis, Option<Target>) {
    let target = |addr| Target {
        site: site.to_owned(),
        addr,
    };
    let system = prober.resolve(site);
    let mut blocked = None;
    // the system's address answers with someone else's certificate: an
    // internet provider's block page, so the address is certainly wrong
    let mut fake = false;
    if let Some(&addr) = system.first() {
        match prober.probe(&target(addr), Some(raw)) {
            Outcome::Opens { .. } => return (Diagnosis::Opens, Some(target(addr))),
            Outcome::Blocked => blocked = Some(target(addr)),
            Outcome::WrongSite => fake = true,
            Outcome::Unreachable => {}
        }
    }
    // Compare with the address encrypted DNS gives, even when the
    // connection was cut: a provider's block server can cut connections
    // too, and then only the real address tells the two apart.
    let real = prober.resolve_encrypted(site);
    let mut spoofed = fake;
    if let Some(&addr) = real.iter().find(|a| !system.contains(a)) {
        match prober.probe(&target(addr), Some(raw)) {
            // the real address opens where the system's doesn't
            Outcome::Opens { .. } => return (Diagnosis::DnsSpoofed, Some(target(addr))),
            // the system's address went nowhere, and at the real one DPI
            // blocks too: test methods there
            Outcome::Blocked if blocked.is_none() => {
                spoofed = true;
                blocked = Some(target(addr));
            }
            _ => {}
        }
    }
    match blocked {
        // a slow site isn't a blocked one: it has to fail twice
        Some(t) => match prober.probe(&t, Some(raw)) {
            Outcome::Opens { .. } if spoofed => (Diagnosis::DnsSpoofed, Some(t)),
            Outcome::Opens { .. } => (Diagnosis::Opens, Some(t)),
            _ if spoofed => (Diagnosis::SpoofedAndBlocked, Some(t)),
            _ => (Diagnosis::Blocked, Some(t)),
        },
        // wrong for sure, though the real address couldn't be reached
        None if fake => (Diagnosis::DnsSpoofed, None),
        None if system.is_empty() && real.is_empty() => (Diagnosis::NotFound, None),
        None => (Diagnosis::Unreachable, None),
    }
}

#[derive(Default)]
struct Trial {
    opened: Vec<String>,
    blocked: Vec<String>,
    broken: Vec<String>,
    times: Vec<u32>,
}

impl Trial {
    fn finish(mut self, strategy: &Strategy, order: usize) -> Tried {
        self.times.sort_unstable();
        Tried {
            strategy: strategy.to_string(),
            passed: self.blocked.is_empty() && self.broken.is_empty() && !self.opened.is_empty(),
            ms: self.times.get(self.times.len() / 2).copied().unwrap_or(0),
            opened: self.opened,
            blocked: self.blocked,
            broken: self.broken,
            order,
        }
    }
}

/// The first blocked site alone (most methods fail, and a failure costs a
/// whole timeout), then the others; if all open, all again with the
/// control sites. Sites not tried count as blocked. `None` when stopped.
fn try_one(
    prober: &dyn Prober,
    ports: Ports,
    targets: &[Target],
    intact: &[Target],
    stop: &AtomicBool,
) -> Option<Trial> {
    let mut trial = Trial::default();
    let Some(Outcome::Opens { ms }) = targets.first().map(|t| prober.probe(t, Some(ports))) else {
        trial.blocked = targets.iter().map(|t| t.site.clone()).collect();
        return Some(trial);
    };
    trial.opened.push(targets[0].site.clone());
    trial.times.push(ms);
    let rest = &targets[1..];
    let first = parallel(rest, |t| prober.probe(t, Some(ports)));
    for (t, outcome) in rest.iter().zip(&first) {
        match outcome {
            Outcome::Opens { ms } => {
                trial.opened.push(t.site.clone());
                trial.times.push(*ms);
            }
            _ => trial.blocked.push(t.site.clone()),
        }
    }
    if !trial.blocked.is_empty() {
        return Some(trial);
    }
    if stop.load(Ordering::SeqCst) {
        return None;
    }
    let all: Vec<&Target> = targets.iter().chain(intact).collect();
    let second = parallel(&all, |t| prober.probe(t, Some(ports)));
    for (t, outcome) in all.iter().zip(second) {
        let is_target = targets.iter().any(|x| x.site == t.site);
        match outcome {
            Outcome::Opens { ms } if is_target => trial.times.push(ms),
            Outcome::Opens { .. } => {}
            _ if is_target => {
                trial.opened.retain(|s| *s != t.site);
                trial.blocked.push(t.site.clone());
            }
            // one retry: a control that fails twice is broken, not flaky
            _ => {
                if !prober.probe(t, Some(ports)).opens() {
                    trial.broken.push(t.site.clone());
                }
            }
        }
    }
    Some(trial)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A fake network: which strategy each slot runs decides what opens.
    struct Net {
        slots: Mutex<HashMap<usize, String>>,
        sites: HashMap<&'static str, FakeSite>,
        /// site → the wrong address the system resolver gives for it
        spoofed: HashMap<&'static str, Ipv4Addr>,
        loads: AtomicUsize,
    }

    /// system address, blocked, opens with these strategies, broken by these
    type FakeSite = (Option<Ipv4Addr>, bool, Vec<&'static str>, Vec<&'static str>);

    const REAL: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
    /// Block servers: one that goes nowhere, one with a block page (a
    /// certificate that isn't the site's), one that cuts connections.
    const FAKE: Ipv4Addr = Ipv4Addr::new(10, 9, 9, 9);
    const BLOCK_PAGE: Ipv4Addr = Ipv4Addr::new(10, 9, 9, 10);
    const CUTTER: Ipv4Addr = Ipv4Addr::new(10, 9, 9, 11);

    impl Net {
        fn slot_of(&self, ports: Ports) -> Option<usize> {
            (ports.first >= 100).then(|| (ports.first / 100 - 1) as usize)
        }
    }

    impl Sandbox for Net {
        fn slots(&self) -> usize {
            4
        }
        fn raw(&self) -> Ports {
            Ports { first: 0, last: 99 }
        }
        fn ports(&self, slot: usize) -> Ports {
            let first = 100 * (slot as u16 + 1);
            Ports {
                first,
                last: first + 99,
            }
        }
        fn load(&self, slot: usize, strategy: &Strategy) -> Result<(), String> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.slots
                .lock()
                .unwrap()
                .insert(slot, strategy.to_string());
            Ok(())
        }
    }

    impl Prober for Net {
        fn resolve(&self, site: &str) -> Vec<Ipv4Addr> {
            if let Some(&fake) = self.spoofed.get(site) {
                return vec![fake];
            }
            self.sites.get(site).and_then(|s| s.0).into_iter().collect()
        }
        fn resolve_encrypted(&self, site: &str) -> Vec<Ipv4Addr> {
            self.sites.get(site).map(|_| REAL).into_iter().collect()
        }
        fn probe(&self, target: &Target, ports: Option<Ports>) -> Outcome {
            // real probes take a while; instant ones would let every slot
            // race through the list before the first result is in
            std::thread::sleep(std::time::Duration::from_millis(5));
            match target.addr {
                FAKE => return Outcome::Unreachable,
                BLOCK_PAGE => return Outcome::WrongSite,
                CUTTER => return Outcome::Blocked,
                _ => {}
            }
            let (_, blocked, fixes, breaks) = &self.sites[target.site.as_str()];
            let strategy = ports
                .and_then(|p| self.slot_of(p))
                .and_then(|s| self.slots.lock().unwrap().get(&s).cloned())
                .unwrap_or_default();
            let used = |list: &Vec<&str>| list.iter().any(|s| *s == strategy);
            if used(breaks) || (*blocked && !used(fixes)) {
                Outcome::Blocked
            } else {
                Outcome::Opens { ms: 100 }
            }
        }
    }

    const TTL8: &str = "--dpi-desync=fake --dpi-desync-ttl=8";
    const TTL11: &str = "--dpi-desync=fake --dpi-desync-ttl=11";

    fn net() -> Net {
        let mut sites = HashMap::new();
        sites.insert("discord.com", (Some(REAL), true, vec![TTL8, TTL11], vec![]));
        sites.insert("roblox.com", (Some(REAL), true, vec![TTL8, TTL11], vec![]));
        sites.insert("isbank.com.tr", (Some(REAL), false, vec![], vec![TTL11]));
        sites.insert("www.gnome.org", (Some(REAL), false, vec![], vec![]));
        sites.insert("www.wikipedia.org", (Some(REAL), false, vec![], vec![]));
        sites.insert("down.example", (Some(REAL), true, vec![], vec![]));
        sites.insert("spoofed.example", (Some(REAL), false, vec![], vec![]));
        sites.insert("both.example", (Some(REAL), true, vec![TTL8], vec![]));
        sites.insert("blockpage.example", (Some(REAL), false, vec![], vec![]));
        sites.insert("cut.example", (Some(REAL), false, vec![], vec![]));
        sites.insert(
            "blockpage-dpi.example",
            (Some(REAL), true, vec![TTL8], vec![]),
        );
        Net {
            slots: Mutex::default(),
            sites,
            spoofed: HashMap::from([
                ("spoofed.example", FAKE),
                ("both.example", FAKE),
                ("blockpage.example", BLOCK_PAGE),
                ("cut.example", CUTTER),
                ("blockpage-dpi.example", BLOCK_PAGE),
            ]),
            loads: AtomicUsize::new(0),
        }
    }

    fn request(sites: &[&str], want: usize) -> Request {
        Request {
            sites: sites.iter().map(|s| s.to_string()).collect(),
            controls: Vec::new(),
            first: Vec::new(),
            want,
        }
    }

    fn scan(net: &Net, request: &Request) -> (Report, Vec<Event>) {
        let events = Mutex::new(Vec::new());
        let report = run(net, net, request, &AtomicBool::new(false), &|e| {
            events.lock().unwrap().push(e)
        });
        (report, events.into_inner().unwrap())
    }

    #[test]
    fn finds_the_method_that_keeps_nearby_sites_open() {
        let net = net();
        let (report, _) = scan(
            &net,
            &request(&["discord.com", "roblox.com", "isbank.com.tr"], 0),
        );
        let best = &report.tried[0];
        assert_eq!(best.strategy, TTL8);
        assert!(best.passed);
        assert_eq!(best.opened, ["discord.com", "roblox.com"]);
        // ttl 11 opens them too but breaks the bank: second, not passed
        let ttl11 = report.tried.iter().find(|t| t.strategy == TTL11).unwrap();
        assert!(!ttl11.passed);
        assert_eq!(ttl11.broken, ["isbank.com.tr"]);
        assert_eq!(report.tried[1].strategy, TTL11);
        assert_eq!(report.tried.len(), candidates::shortlist().len());
    }

    #[test]
    fn diagnoses_each_site() {
        let net = net();
        let (report, events) = scan(
            &net,
            &request(
                &[
                    "discord.com",
                    "isbank.com.tr",
                    "spoofed.example",
                    "nowhere.example",
                    "both.example",
                ],
                1,
            ),
        );
        let d: HashMap<_, _> = report.diagnosis.iter().cloned().collect();
        assert_eq!(d["discord.com"], Diagnosis::Blocked);
        assert_eq!(d["isbank.com.tr"], Diagnosis::Opens);
        assert_eq!(d["spoofed.example"], Diagnosis::DnsSpoofed);
        assert_eq!(d["nowhere.example"], Diagnosis::NotFound);
        assert_eq!(d["both.example"], Diagnosis::SpoofedAndBlocked);
        // tested at its real address, which ttl 8 opens
        assert!(report.tried[0].passed);
        assert!(report.tried[0].opened.contains(&"both.example".to_string()));
        // default controls are checked but not reported
        assert_eq!(report.diagnosis.len(), 5);
        assert!(events.iter().any(|e| matches!(e, Event::Progress { .. })));
    }

    #[test]
    fn a_fake_address_is_found_even_when_it_cuts_the_connection() {
        let net = net();
        let (report, _) = scan(
            &net,
            &request(
                &["blockpage.example", "cut.example", "blockpage-dpi.example"],
                1,
            ),
        );
        let d: HashMap<_, _> = report.diagnosis.iter().cloned().collect();
        // a block page at the system's address, the real one opens
        assert_eq!(d["blockpage.example"], Diagnosis::DnsSpoofed);
        // the system's address cuts the connection like DPI would, but the
        // real address opens: it's the address, not DPI
        assert_eq!(d["cut.example"], Diagnosis::DnsSpoofed);
        // a block page, and DPI at the real address: methods are tried there
        assert_eq!(d["blockpage-dpi.example"], Diagnosis::SpoofedAndBlocked);
        assert!(report.tried[0].passed);
        assert_eq!(report.tried[0].opened, ["blockpage-dpi.example"]);
    }

    #[test]
    fn stops_early_when_enough_pass() {
        let net = net();
        let (report, _) = scan(&net, &request(&["discord.com"], 1));
        assert!(report.tried[0].passed);
        // the four slots finish what they started, nothing more
        assert!(net.loads.load(Ordering::SeqCst) <= 8 + 2 * net.slots());
        assert!(report.tried.len() < candidates::shortlist().len());
    }

    #[test]
    fn nothing_to_do_when_everything_opens() {
        let net = net();
        let (report, _) = scan(&net, &request(&["isbank.com.tr"], 1));
        assert!(report.tried.is_empty());
        assert_eq!(net.loads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn known_good_goes_first() {
        let net = net();
        let mut r = request(&["discord.com"], 1);
        r.first = vec![Strategy::parse(TTL8).unwrap()];
        let (report, _) = scan(&net, &r);
        let first = report.tried.iter().find(|t| t.order == 0).unwrap();
        assert_eq!(first.strategy, TTL8);
        assert!(first.passed);
    }

    #[test]
    fn nothing_passes_when_no_method_works() {
        let net = net();
        let (report, _) = scan(&net, &request(&["down.example"], 0));
        // blocked with every method: nothing passes, all are tried
        assert!(report.tried.iter().all(|t| !t.passed));
        assert_eq!(report.tried.len(), candidates::shortlist().len());
    }
}
