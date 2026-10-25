//! A strategy: the nfqws arguments that get past a DPI filter, parsed into a
//! checked model that prints back exactly as it was written.
//!
//! The helper hands these arguments to nfqws as root, so parsing is strict:
//! every option must be in [`crate::options`] and allowed in a strategy, and
//! every value must match its option's format and use only safe characters.

use std::fmt;
use std::str::FromStr;

use crate::options::{self, Arg, Kind, OptionSpec, Value};
use crate::site;

/// Placeholders in zapret's `NFQWS_OPT`, which its init script replaces with
/// the standard hostlist options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placeholder {
    /// `<HOSTLIST>`: the user and auto hostlists, when MODE_FILTER uses them.
    Hostlist,
    /// `<HOSTLIST_NOAUTO>`: the same, with the auto hostlist as a plain list.
    HostlistNoAuto,
}

impl Placeholder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hostlist => "<HOSTLIST>",
            Self::HostlistNoAuto => "<HOSTLIST_NOAUTO>",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Option {
        spec: &'static OptionSpec,
        value: Option<String>,
    },
    Placeholder(Placeholder),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Option { spec, value: None } => write!(f, "--{}", spec.name),
            Self::Option {
                spec,
                value: Some(v),
            } => write!(f, "--{}={v}", spec.name),
            Self::Placeholder(p) => f.write_str(p.as_str()),
        }
    }
}

/// One nfqws profile: the options up to the next `--new`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    tokens: Vec<Token>,
}

impl Profile {
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// The last occurrence of option `name` (nfqws keeps the last one):
    /// `Some(None)` when it is there without a value.
    pub fn value(&self, name: &str) -> Option<Option<&str>> {
        self.tokens.iter().rev().find_map(|t| match t {
            Token::Option { spec, value } if spec.name == name => Some(value.as_deref()),
            _ => None,
        })
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.value(name).flatten()
    }

    pub fn has(&self, name: &str) -> bool {
        self.value(name).is_some()
    }

    /// The comma-separated items of option `name`'s value.
    pub fn list(&self, name: &str) -> Vec<&str> {
        self.get(name)
            .map(|v| v.split(',').collect())
            .unwrap_or_default()
    }

    pub fn desync_modes(&self) -> Vec<&str> {
        self.list("dpi-desync")
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, token) in self.tokens.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{token}")?;
        }
        Ok(())
    }
}

/// One or more profiles, separated by `--new` on the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strategy {
    profiles: Vec<Profile>,
}

impl Strategy {
    /// Parses whitespace-separated nfqws arguments, as written in
    /// `NFQWS_OPT` (which the init script word-splits, so no quoting).
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut profiles = Vec::new();
        let mut current = Profile::default();
        for word in text.split_whitespace() {
            let token = parse_token(word)?;
            if matches!(&token, Token::Option { spec, .. } if spec.name == "new") {
                if current.tokens.is_empty() {
                    return Err(ParseError::EmptyProfile);
                }
                profiles.push(std::mem::take(&mut current));
            } else {
                current.tokens.push(token);
            }
        }
        if current.tokens.is_empty() {
            return Err(if profiles.is_empty() {
                ParseError::Empty
            } else {
                ParseError::EmptyProfile
            });
        }
        profiles.push(current);
        Ok(Self { profiles })
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// The value for `NFQWS_OPT`: one line, or one line per profile in
    /// zapret's own multi-line style when there are several.
    pub fn to_config_value(&self) -> String {
        if self.profiles.len() == 1 {
            return self.to_string();
        }
        let lines: Vec<String> = self.profiles.iter().map(Profile::to_string).collect();
        format!("\n{}\n", lines.join(" --new\n"))
    }

    /// The arguments to start nfqws with, `--new` between profiles;
    /// zapret's hostlist placeholders are left out.
    pub fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for (i, profile) in self.profiles.iter().enumerate() {
            if i > 0 {
                args.push("--new".to_owned());
            }
            args.extend(
                profile
                    .tokens
                    .iter()
                    .filter(|t| matches!(t, Token::Option { .. }))
                    .map(Token::to_string),
            );
        }
        args
    }

    /// The method alone: this strategy without the site filters the app
    /// adds (see [`Strategy::for_sites`]). Profiles left with nothing are
    /// dropped; `None` if none are left.
    pub fn method(&self) -> Option<Strategy> {
        let profiles: Vec<Profile> = self
            .profiles
            .iter()
            .map(|p| Profile {
                tokens: p
                    .tokens
                    .iter()
                    .filter(|t| !matches!(t, Token::Option { spec, .. } if SITE_OPTIONS.contains(&spec.name)))
                    .cloned()
                    .collect(),
            })
            .filter(|p| !p.tokens.is_empty())
            .collect();
        (!profiles.is_empty()).then_some(Strategy { profiles })
    }

    /// This method limited to `only` (when given) and never applied to
    /// `excluded`: every profile gets `--hostlist-domains` and
    /// `--hostlist-exclude-domains`. Domains must already be normalized.
    pub fn for_sites(
        &self,
        only: Option<&[String]>,
        excluded: &[String],
    ) -> Result<Strategy, ParseError> {
        let method = self.method().ok_or(ParseError::Empty)?;
        let mut extra = String::new();
        if let Some(only) = only.filter(|o| !o.is_empty()) {
            extra.push_str(&format!(" --hostlist-domains={}", only.join(",")));
        }
        if !excluded.is_empty() {
            extra.push_str(&format!(
                " --hostlist-exclude-domains={}",
                excluded.join(",")
            ));
        }
        let text: Vec<String> = method
            .profiles
            .iter()
            .map(|p| format!("{p}{extra}"))
            .collect();
        Strategy::parse(&text.join(" --new "))
    }
}

/// The options [`Strategy::for_sites`] adds.
const SITE_OPTIONS: &[&str] = &["hostlist-domains", "hostlist-exclude-domains"];

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, profile) in self.profiles.iter().enumerate() {
            if i > 0 {
                f.write_str(" --new ")?;
            }
            write!(f, "{profile}")?;
        }
        Ok(())
    }
}

impl FromStr for Strategy {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    /// `--new` with nothing before or after it.
    EmptyProfile,
    /// A word that is neither `--option` nor a placeholder.
    NotAnOption(String),
    Unknown(String),
    /// A real nfqws option that a strategy may not set (see [`Kind`]).
    NotAllowed(&'static str),
    MissingValue(&'static str),
    UnexpectedValue(&'static str),
    BadValue {
        option: &'static str,
        value: String,
        expected: &'static str,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("no options"),
            Self::EmptyProfile => f.write_str("--new with no options on one side"),
            Self::NotAnOption(w) => write!(f, "“{w}” is not an option"),
            Self::Unknown(name) => write!(f, "nfqws has no option --{name}"),
            Self::NotAllowed(name) => write!(f, "--{name} cannot be part of a strategy"),
            Self::MissingValue(name) => write!(f, "--{name} needs a value"),
            Self::UnexpectedValue(name) => write!(f, "--{name} takes no value"),
            Self::BadValue {
                option,
                value,
                expected,
            } => {
                write!(f, "--{option}={value}: expected {expected}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

fn parse_token(word: &str) -> Result<Token, ParseError> {
    match word {
        "<HOSTLIST>" => return Ok(Token::Placeholder(Placeholder::Hostlist)),
        "<HOSTLIST_NOAUTO>" => return Ok(Token::Placeholder(Placeholder::HostlistNoAuto)),
        _ => {}
    }
    let rest = word
        .strip_prefix("--")
        .ok_or_else(|| ParseError::NotAnOption(word.to_owned()))?;
    let (name, value) = match rest.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (rest, None),
    };
    let spec = options::lookup(name).ok_or_else(|| ParseError::Unknown(name.to_owned()))?;
    if !spec.allowed_in_strategy() {
        return Err(ParseError::NotAllowed(spec.name));
    }
    match (spec.arg, value) {
        (Arg::None, Some(_)) => return Err(ParseError::UnexpectedValue(spec.name)),
        (Arg::Required, None) => return Err(ParseError::MissingValue(spec.name)),
        _ => {}
    }
    if let Some(value) = value {
        check_value(spec, value)?;
    }
    debug_assert!(matches!(spec.kind, Kind::Desync | Kind::Filter));
    Ok(Token::Option {
        spec,
        value: value.map(str::to_owned),
    })
}

const MODES: &[&str] = &[
    "synack",
    "syndata",
    "fake",
    "fakeknown",
    "rst",
    "rstack",
    "hopbyhop",
    "destopt",
    "ipfrag1",
    "multisplit",
    "multidisorder",
    "fakedsplit",
    "hostfakesplit",
    "fakeddisorder",
    "ipfrag2",
    "udplen",
    "tamper",
];
const FOOLING: &[&str] = &[
    "none",
    "md5sig",
    "ts",
    "badseq",
    "badsum",
    "datanoack",
    "hopbyhop",
    "hopbyhop2",
];
pub(crate) const MARKERS: &[&str] = &[
    "method", "host", "endhost", "sld", "endsld", "midsld", "sniext",
];
const L7: &[&str] = &[
    "http",
    "tls",
    "quic",
    "wireguard",
    "dht",
    "discord",
    "stun",
    "unknown",
];
/// The only place a fake payload file may come from.
const FAKE_DIR: &str = "/opt/zapret/files/fake/";

fn check_value(spec: &'static OptionSpec, value: &str) -> Result<(), ParseError> {
    let bad = |expected| ParseError::BadValue {
        option: spec.name,
        value: value.to_owned(),
        expected,
    };
    let extra: &[char] = if spec.value == Value::Ports {
        &['*', '~']
    } else {
        &[]
    };
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".,:+-=_/@!".contains(c) || extra.contains(&c))
    {
        return Err(bad("letters, digits and . , : + - = _ / @ ! only"));
    }
    let items = || value.split(',');
    let ok = match spec.value {
        Value::Text => true,
        Value::Modes => {
            let modes: Vec<&str> = items().collect();
            modes.len() <= 3
                && modes.iter().all(|m| MODES.contains(m))
                && modes
                    .iter()
                    .enumerate()
                    .all(|(i, m)| !modes[..i].contains(m))
        }
        Value::Ttl => in_range(value, 1, 255),
        Value::AutoTtl => value.is_empty() || value == "-" || autottl(value),
        Value::Fooling => items().all(|f| FOOLING.contains(&f)),
        Value::Positions => items().all(position),
        Value::Position => position(value),
        Value::PacketLimit => {
            let n = value.strip_prefix(['n', 'd', 's']).unwrap_or(value);
            in_range(n, 0, u32::MAX as u64)
        }
        Value::Count => in_range(value, 1, 1000),
        Value::Ports => items().all(|p| {
            let p = p.strip_prefix('~').unwrap_or(p);
            p == "*"
                || match p.split_once('-') {
                    Some((a, b)) => {
                        in_range(a, 1, 65535)
                            && in_range(b, 1, 65535)
                            && a.parse::<u32>().ok() <= b.parse().ok()
                    }
                    None => in_range(p, 1, 65535),
                }
        }),
        Value::L7 => items().all(|p| L7.contains(&p)),
        Value::L3 => items().all(|p| p == "ipv4" || p == "ipv6"),
        Value::Domains => items().all(site::is_domain),
        Value::HostSpell => value.len() == 4 && value.eq_ignore_ascii_case("host"),
        Value::Mods => items().all(|m| {
            m == "none"
                || m.strip_prefix("altorder=")
                    .is_some_and(|n| in_range(n, 0, 31))
                || m.strip_prefix("host=").is_some_and(site::is_domain)
        }),
        Value::Payload => payload(value),
    };
    if ok {
        Ok(())
    } else {
        Err(bad(expected(spec.value)))
    }
}

fn expected(value: Value) -> &'static str {
    match value {
        Value::Text => "a value",
        Value::Modes => "up to three different desync modes",
        Value::Ttl => "a number from 1 to 255",
        Value::AutoTtl => "delta[:min[-max]] or -",
        Value::Fooling => "fooling methods such as ts or md5sig",
        Value::Positions => "split positions such as 1, -2, midsld or method+2",
        Value::Position => "a split position such as 1 or midsld",
        Value::PacketLimit => "a packet count, optionally after n, d or s",
        Value::Count => "a number from 1 to 1000",
        Value::Ports => "ports or ranges such as 80,443 or 50000-50099",
        Value::L7 => "protocols such as http, tls or quic",
        Value::L3 => "ipv4 or ipv6",
        Value::Domains => "domain names such as discord.com",
        Value::HostSpell => "four letters spelling “host”",
        Value::Mods => "none, altorder=N or host=domain",
        Value::Payload => "0x followed by hex digits, !, or a file in /opt/zapret/files/fake/",
    }
}

fn in_range(s: &str, min: u64, max: u64) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u64>().is_ok_and(|n| (min..=max).contains(&n))
}

fn signed(s: &str) -> bool {
    in_range(s.strip_prefix(['-', '+']).unwrap_or(s), 0, 255)
}

/// `delta[:min[-max]]`
fn autottl(value: &str) -> bool {
    let (delta, range) = match value.split_once(':') {
        Some((d, r)) => (d, Some(r)),
        None => (value, None),
    };
    signed(delta)
        && range.is_none_or(|r| match r.split_once('-') {
            Some((min, max)) => in_range(min, 0, 255) && in_range(max, 0, 255),
            None => in_range(r, 0, 255),
        })
}

/// `N`, `-N`, `marker`, `marker+N` or `marker-N`
fn position(p: &str) -> bool {
    if let Some(n) = p.strip_prefix('-') {
        return in_range(n, 1, 65535);
    }
    if in_range(p, 0, 65535) {
        return true;
    }
    let marker_end = p.find(['+', '-']).unwrap_or(p.len());
    MARKERS.contains(&&p[..marker_end])
        && (marker_end == p.len() || in_range(&p[marker_end + 1..], 0, 65535))
}

/// `0xHEX`, `![+N]` or `[+N@]/opt/zapret/files/fake/NAME`
fn payload(v: &str) -> bool {
    if let Some(hex) = v.strip_prefix("0x") {
        return !hex.is_empty() && hex.len() % 2 == 0 && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(rest) = v.strip_prefix('!') {
        return rest.is_empty()
            || rest
                .strip_prefix('+')
                .is_some_and(|n| in_range(n, 0, 65535));
    }
    let path = match v.split_once('@') {
        Some((offset, path)) => {
            if !(offset.is_empty()
                || offset
                    .strip_prefix('+')
                    .is_some_and(|n| in_range(n, 0, 65535)))
            {
                return false;
            }
            path
        }
        None => v,
    };
    path.strip_prefix(FAKE_DIR)
        .is_some_and(|name| !name.is_empty() && !name.contains('/') && name != "." && name != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(s: &str) -> ParseError {
        Strategy::parse(s).unwrap_err()
    }

    #[test]
    fn round_trip_and_access() {
        let s =
            Strategy::parse("--filter-tcp=80,443 --dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>")
                .unwrap();
        assert_eq!(
            s.to_string(),
            "--filter-tcp=80,443 --dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>"
        );
        let p = &s.profiles()[0];
        assert_eq!(p.desync_modes(), ["fake"]);
        assert_eq!(p.get("dpi-desync-ttl"), Some("8"));
        assert!(
            p.tokens()
                .contains(&Token::Placeholder(Placeholder::Hostlist))
        );
    }

    #[test]
    fn last_value_wins() {
        let s = Strategy::parse("--dpi-desync=fake --dpi-desync-ttl=1 --dpi-desync-ttl=9").unwrap();
        assert_eq!(s.profiles()[0].get("dpi-desync-ttl"), Some("9"));
    }

    #[test]
    fn profiles_split_on_new() {
        let text = "--filter-tcp=80 --dpi-desync=fake <HOSTLIST> --new\n--filter-udp=443 --dpi-desync=fake --dpi-desync-repeats=6";
        let s = Strategy::parse(text).unwrap();
        assert_eq!(s.profiles().len(), 2);
        assert_eq!(
            s.to_config_value(),
            "\n--filter-tcp=80 --dpi-desync=fake <HOSTLIST> --new\n--filter-udp=443 --dpi-desync=fake --dpi-desync-repeats=6\n"
        );
        assert_eq!(Strategy::parse(&s.to_config_value()).unwrap(), s);
        assert_eq!(err("--dpi-desync=fake --new"), ParseError::EmptyProfile);
        assert_eq!(err("--new --dpi-desync=fake"), ParseError::EmptyProfile);
        assert_eq!(err("  "), ParseError::Empty);
    }

    #[test]
    fn site_filters_come_and_go() {
        let method = Strategy::parse("--filter-tcp=443 --dpi-desync=fake <HOSTLIST> --new --filter-udp=443 --dpi-desync=fake").unwrap();
        let only = ["discord.com".to_owned(), "discord.gg".into()];
        let excluded = ["turkiye.gov.tr".to_owned()];
        let applied = method.for_sites(Some(&only), &excluded).unwrap();
        assert_eq!(
            applied.to_string(),
            "--filter-tcp=443 --dpi-desync=fake <HOSTLIST> --hostlist-domains=discord.com,discord.gg --hostlist-exclude-domains=turkiye.gov.tr \
             --new --filter-udp=443 --dpi-desync=fake --hostlist-domains=discord.com,discord.gg --hostlist-exclude-domains=turkiye.gov.tr"
        );
        assert_eq!(applied.method().unwrap(), method);
        assert_eq!(applied.for_sites(None, &[]).unwrap(), method);
        assert!(
            method
                .for_sites(None, &["not a domain".to_owned()])
                .is_err()
        );
    }

    #[test]
    fn rejects_options_the_helper_owns() {
        for s in [
            "--user=root",
            "--qnum=200",
            "--daemon",
            "--pidfile=/tmp/x",
            "--hostlist=/etc/shadow",
            "--debug=@/etc/passwd",
            "--wf-tcp=80",
        ] {
            assert!(matches!(err(s), ParseError::NotAllowed(_)), "{s}");
        }
    }

    #[test]
    fn rejects_shell_and_bad_values() {
        assert!(matches!(
            err("--dpi-desync=fake;reboot"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync=fake$(id)"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync=fake `id`"),
            ParseError::NotAnOption(_)
        ));
        assert!(matches!(
            err("--dpi-desync=bogus"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync=fake,fake"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync-ttl=0"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync-ttl=300"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync-split-pos=nowhere+1"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(err("--filter-tcp=0"), ParseError::BadValue { .. }));
        assert!(matches!(
            err("--filter-tcp=443-80"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--hostspell=hst"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync-fake-tls=/etc/shadow"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync-fake-tls=/opt/zapret/files/fake/../../../etc/shadow"),
            ParseError::BadValue { .. }
        ));
        assert!(matches!(
            err("--dpi-desync-ttl"),
            ParseError::MissingValue(_)
        ));
        assert!(matches!(
            err("--hostcase=1"),
            ParseError::UnexpectedValue(_)
        ));
        assert!(matches!(err("--nonsense"), ParseError::Unknown(_)));
        assert!(matches!(err("fake"), ParseError::NotAnOption(_)));
    }

    #[test]
    fn accepts_value_formats_from_the_docs() {
        for s in [
            "--dpi-desync=fake,multisplit --dpi-desync-split-pos=method+2,midsld,5,-10",
            "--dpi-desync=fakeddisorder --dpi-desync-autottl=-3 --dpi-desync-autottl6=1:3-20",
            "--dpi-desync=fake --dpi-desync-autottl --dpi-desync-autottl6=-",
            "--dpi-desync=fake --dpi-desync-fooling=badseq,md5sig --dpi-desync-badseq-increment=0",
            "--dpi-desync=fake --dpi-desync-fake-tls=0x00000000 --dpi-desync-fake-quic=!+2",
            "--dpi-desync=fake --dpi-desync-fake-tls=+4@/opt/zapret/files/fake/tls_clienthello_www_google_com.bin",
            "--dpi-desync=fakedsplit --dpi-desync-fakedsplit-mod=altorder=1",
            "--dpi-desync=hostfakesplit --dpi-desync-hostfakesplit-mod=host=www.google.com,altorder=1",
            "--orig-ttl=1 --orig-mod-start=s1 --orig-mod-cutoff=d1 --dpi-desync=fake",
            "--filter-tcp=80,443,50000-50099 --filter-l7=tls,quic --hostlist-domains=discord.com,discord.gg",
            "--filter-udp=* --filter-l3=ipv4 --dpi-desync=fake --dpi-desync-repeats=6",
            "--hostspell=hoSt",
        ] {
            assert_eq!(Strategy::parse(s).map(|x| x.to_string()), Ok(s.to_owned()));
        }
    }
}
