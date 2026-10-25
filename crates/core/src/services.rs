//! Popular services and the addresses they use besides their main one, so
//! "Discord" unblocks Discord's images, voice and invite links too.
//! nfqws matches subdomains by itself, so each entry lists only the
//! registrable domains.

pub struct Service {
    pub name: &'static str,
    /// The main domain first.
    pub domains: &'static [&'static str],
}

pub static SERVICES: &[Service] = &[
    Service {
        name: "Discord",
        domains: &[
            "discord.com",
            "discord.gg",
            "discordapp.com",
            "discordapp.net",
            "discord.media",
            "discordcdn.com",
        ],
    },
    Service {
        name: "YouTube",
        domains: &[
            "youtube.com",
            "youtu.be",
            "googlevideo.com",
            "ytimg.com",
            "ggpht.com",
        ],
    },
    Service {
        name: "Roblox",
        domains: &["roblox.com", "rbxcdn.com", "rbx.com", "robloxlabs.com"],
    },
    Service {
        name: "Wattpad",
        domains: &["wattpad.com"],
    },
    Service {
        name: "X",
        domains: &["x.com", "twitter.com", "twimg.com", "t.co"],
    },
    Service {
        name: "Instagram",
        domains: &["instagram.com", "cdninstagram.com"],
    },
    Service {
        name: "Telegram",
        domains: &["telegram.org", "t.me", "telegram.me"],
    },
    Service {
        name: "Imgur",
        domains: &["imgur.com"],
    },
    Service {
        name: "Twitch",
        domains: &["twitch.tv", "ttvnw.net", "jtvnw.net"],
    },
];

/// The service `domain` (or a parent of it) belongs to.
pub fn find(domain: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| {
        s.domains.iter().any(|d| {
            domain == *d
                || domain
                    .strip_suffix(d)
                    .is_some_and(|rest| rest.ends_with('.'))
        })
    })
}

/// Every domain to give zapret for `sites`: a service's sites bring all of
/// its domains. Sorted, without repeats.
pub fn expand(sites: &[String]) -> Vec<String> {
    let mut domains: Vec<String> = sites
        .iter()
        .flat_map(|site| match find(site) {
            Some(service) => service.domains.iter().map(|d| (*d).to_owned()).collect(),
            None => vec![site.clone()],
        })
        .collect();
    domains.sort();
    domains.dedup();
    domains
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_services_by_any_of_their_domains() {
        assert_eq!(find("discord.gg").unwrap().name, "Discord");
        assert_eq!(find("cdn.discordapp.com").unwrap().name, "Discord");
        assert_eq!(find("www.youtube.com").unwrap().name, "YouTube");
        assert!(find("notdiscord.com").is_none());
        assert!(find("example.com").is_none());
    }

    #[test]
    fn expands_services_and_keeps_other_sites() {
        let sites = vec![
            "wattpad.com".to_owned(),
            "x.com".into(),
            "example.org".into(),
            "t.co".into(),
        ];
        assert_eq!(
            expand(&sites),
            [
                "example.org",
                "t.co",
                "twimg.com",
                "twitter.com",
                "wattpad.com",
                "x.com"
            ]
        );
    }
}
