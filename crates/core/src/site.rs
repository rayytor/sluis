//! Site names as people type or paste them, turned into the plain domain
//! names zapret's hostlists use: `https://www.Discord.com/app` becomes
//! `discord.com`, `bilgi.türkiye.gov.tr` becomes its punycode form.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SiteError {
    Empty,
    /// Not a domain name: an IP address, a single word, invalid characters.
    NotADomain(String),
}

impl fmt::Display for SiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("no site name"),
            Self::NotADomain(s) => write!(f, "“{s}” is not a site name"),
        }
    }
}

impl std::error::Error for SiteError {}

/// The domain in `input`, which may be a full link.
pub fn normalize(input: &str) -> Result<String, SiteError> {
    let mut s = input.trim();
    if s.is_empty() {
        return Err(SiteError::Empty);
    }
    if let Some((_, rest)) = s.split_once("://") {
        s = rest;
    }
    s = s.split(['/', '?', '#']).next().unwrap_or_default();
    if let Some((_, host)) = s.rsplit_once('@') {
        s = host;
    }
    if let Some((host, port)) = s.rsplit_once(':')
        && port.bytes().all(|b| b.is_ascii_digit())
    {
        s = host;
    }
    let s = s.trim_end_matches('.');
    let not_domain = || SiteError::NotADomain(input.trim().to_owned());
    let ascii = idna::domain_to_ascii(s).map_err(|_| not_domain())?;
    let domain = ascii.strip_prefix("www.").unwrap_or(&ascii).to_owned();
    if is_domain(&domain) {
        Ok(domain)
    } else {
        Err(not_domain())
    }
}

/// Whether `s` is already a plain lower-case ASCII domain name with at least
/// two labels and a non-numeric top-level domain.
pub fn is_domain(s: &str) -> bool {
    let labels: Vec<&str> = s.split('.').collect();
    s.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        && !labels[labels.len() - 1].bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_what_people_paste() {
        for (input, domain) in [
            ("discord.com", "discord.com"),
            ("  Discord.COM ", "discord.com"),
            ("https://www.discord.com/channels/@me", "discord.com"),
            (
                "http://user:pw@cdn.discordapp.com:443/x?y#z",
                "cdn.discordapp.com",
            ),
            ("wattpad.com.", "wattpad.com"),
            ("www.youtube.com", "youtube.com"),
            ("türkiye.gov.tr", "xn--trkiye-3ya.gov.tr"),
        ] {
            assert_eq!(normalize(input).as_deref(), Ok(domain), "{input}");
        }
    }

    #[test]
    fn rejects_what_is_not_a_site() {
        assert_eq!(normalize("  "), Err(SiteError::Empty));
        for input in [
            "localhost",
            "192.168.1.1",
            "https://10.0.0.1/",
            "dis cord.com",
            "-bad.com",
            "a..b",
        ] {
            assert!(
                matches!(normalize(input), Err(SiteError::NotADomain(_))),
                "{input}"
            );
        }
    }
}
