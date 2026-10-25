//! Checks against what is really on the desktop PC (see tests/data/README.md).

use std::collections::HashMap;

use sluis_core::config::Problem;
use sluis_core::{Config, Strategy, describe};

fn data(name: &str) -> String {
    let path = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn blockcheck_strategies() -> Vec<String> {
    data("strategies-blockcheck.txt")
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_blockcheck_strategy_parses_and_prints_back_exactly() {
    let strategies = blockcheck_strategies();
    assert_eq!(strategies.len(), 111);
    for text in &strategies {
        let strategy = Strategy::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(&strategy.to_string(), text);
        assert_eq!(strategy.to_config_value(), *text);
    }
}

/// blockcheck reports a strategy once per test it passes (HTTP, TLS 1.2,
/// TLS 1.3) and 0.5.4 stored every report, so its 111 are 63 strategies.
#[test]
fn every_distinct_blockcheck_strategy_gets_its_own_name() {
    let mut distinct = blockcheck_strategies();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 63);
    let mut seen: HashMap<String, String> = HashMap::new();
    for text in distinct {
        let name = describe(&Strategy::parse(&text).unwrap()).to_string();
        assert!(!name.is_empty() && !name.contains("--"), "{text} → {name}");
        if let Some(other) = seen.insert(name.clone(), text.clone()) {
            panic!("“{name}” names both\n  {other}\n  {text}");
        }
    }
}

#[test]
fn configs_round_trip_byte_for_byte() {
    for name in ["config.default", "config.toros-damaged"] {
        let text = data(name);
        assert_eq!(Config::parse(text.clone()).as_str(), text, "{name}");
    }
}

#[test]
fn reads_the_live_config() {
    let config = Config::parse(data("config.toros-damaged"));
    let opt = config.get("NFQWS_OPT").unwrap().literal().unwrap();
    assert_eq!(opt, "--dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>");
    assert_eq!(
        describe(&Strategy::parse(&opt).unwrap()).to_string(),
        "Fake packet, TTL 8"
    );
    assert_eq!(
        config.get("NFQWS_ENABLE").unwrap().literal().as_deref(),
        Some("1")
    );
    assert_eq!(
        config.get("MODE_FILTER").unwrap().literal().as_deref(),
        Some("none")
    );
    // expansions stay unexpanded
    assert_eq!(config.get("IPSET_OPT").unwrap().literal(), None);
    // swallowed by the damaged TPWS_SOCKS_OPT, exactly as the shell reads it
    assert!(config.get("TPWS_ENABLE").is_none());
}

#[test]
fn finds_the_damage_in_the_live_config() {
    let problems = Config::parse(data("config.toros-damaged")).problems();
    assert_eq!(
        problems,
        [Problem::Swallowed {
            name: "TPWS_SOCKS_OPT".into(),
            swallowed: vec!["TPWS_ENABLE".into(), "TPWS_PORTS".into(), "TPWS_OPT".into()],
        }]
    );
    assert!(Config::parse(data("config.default")).problems().is_empty());
}

#[test]
fn the_default_multi_profile_strategy_parses() {
    let config = Config::parse(data("config.default"));
    let opt = config.get("NFQWS_OPT").unwrap().literal().unwrap();
    let strategy = Strategy::parse(&opt).unwrap();
    assert_eq!(strategy.profiles().len(), 3);
    assert_eq!(strategy.to_config_value(), opt);
    assert_eq!(
        describe(&strategy).to_string(),
        "HTTP: Fake packet + split at HTTP method +2, MD5 signature; \
         HTTPS: Fake packet + reordered split at byte 1 and middle of domain, bad sequence number and MD5 signature; \
         QUIC: Fake packet, sent 6 times"
    );
}

#[test]
fn applying_a_strategy_changes_only_nfqws_opt() {
    let before = data("config.toros-damaged");
    let mut config = Config::parse(before.clone());
    let multi = Strategy::parse(
        "--filter-tcp=80,443 --dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST> --new --filter-udp=443 --dpi-desync=fake --dpi-desync-repeats=6",
    )
    .unwrap();
    config.set("NFQWS_OPT", &multi.to_config_value());
    let after = config.as_str();

    let old = "\"--dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>\"";
    let at = before.find(old).unwrap();
    let new_value = config.get("NFQWS_OPT").unwrap().raw();
    assert_eq!(&after[..at], &before[..at]);
    assert_eq!(&after[at + new_value.len()..], &before[at + old.len()..]);
    let read_back = Strategy::parse(&config.get("NFQWS_OPT").unwrap().literal().unwrap()).unwrap();
    assert_eq!(read_back, multi);
}

#[test]
fn repair_restores_the_damaged_config() {
    let damaged = data("config.toros-damaged");
    let defaults = Config::parse(data("config.default"));
    let mut config = Config::parse(damaged.clone());
    assert_eq!(config.repair(&defaults), ["TPWS_SOCKS_OPT", "TPWS_OPT"]);
    assert!(config.problems().is_empty());

    // the TPWS block is back to exactly what zapret ships
    let block = |text: &str| {
        let start = text.find("TPWS_SOCKS_OPT=").unwrap();
        let end = text.find("NFQWS_ENABLE=").unwrap();
        text[start..end].to_owned()
    };
    assert_eq!(block(config.as_str()), block(defaults.as_str()));
    let literal = |c: &Config, name: &str| c.get(name).unwrap().literal().unwrap();
    assert_eq!(literal(&config, "TPWS_ENABLE"), "0");
    assert_eq!(literal(&config, "TPWS_PORTS"), "80,443");

    // and nothing else moved
    let (before, after) = (block(&damaged), block(config.as_str()));
    assert_eq!(config.as_str().replacen(&after, &before, 1), damaged);
    assert_eq!(
        literal(&config, "NFQWS_OPT"),
        "--dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>"
    );

    // a healthy file is left alone
    let mut healthy = defaults.clone();
    assert!(healthy.repair(&defaults).is_empty());
    assert_eq!(healthy, defaults);
}
