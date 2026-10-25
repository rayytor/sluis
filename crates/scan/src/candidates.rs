//! The methods a quick scan tries, from `data/candidates.json`, which is
//! ordered by real-world success and can change without code changes.

use serde::Deserialize;
use sluis_core::Strategy;

#[derive(Deserialize)]
struct Data {
    methods: Vec<String>,
    controls: Vec<String>,
}

fn data() -> Data {
    serde_json::from_str(include_str!("../data/candidates.json")).expect("valid candidates.json")
}

/// The shortlist in order, `{ttl}` expanded to TTL 1–12.
pub fn shortlist() -> Vec<Strategy> {
    data()
        .methods
        .iter()
        .flat_map(|m| {
            if m.contains("{ttl}") {
                (1..=12)
                    .map(|ttl| m.replace("{ttl}", &ttl.to_string()))
                    .collect()
            } else {
                vec![m.clone()]
            }
        })
        .filter_map(|m| Strategy::parse(&m).ok())
        .collect()
}

/// Sites that open almost everywhere; a method that breaks them is wrong
/// even if it opens the blocked ones.
pub fn default_controls() -> Vec<String> {
    data().controls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_method_parses() {
        let data = data();
        let expected: usize = data
            .methods
            .iter()
            .map(|m| if m.contains("{ttl}") { 12 } else { 1 })
            .sum();
        assert_eq!(shortlist().len(), expected);
        assert!(shortlist().iter().all(|s| !s.args().is_empty()));
    }

    #[test]
    fn ttl_sweep_is_in_place() {
        let list = shortlist();
        assert_eq!(list[0].to_string(), "--dpi-desync=fake --dpi-desync-ttl=1");
        assert_eq!(
            list[11].to_string(),
            "--dpi-desync=fake --dpi-desync-ttl=12"
        );
    }
}
