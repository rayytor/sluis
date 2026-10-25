//! Names for strategies, built from what they do: `--dpi-desync=fake
//! --dpi-desync-ttl=8` is "Fake packet, TTL 8".
//!
//! These are the technical names behind the app's plain "Method 1, 2, 3"
//! labels (the method details page and expert lists). They come back as
//! structured parts so the app can translate each part; [`fmt::Display`]
//! gives the English.

use std::fmt;

use crate::strategy::{Profile, Strategy, Token};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    SynAck,
    SynData,
    Fake,
    FakeKnown,
    Rst,
    RstAck,
    HopByHop,
    DestOpt,
    IpFrag1,
    IpFrag2,
    Split,
    Disorder,
    FakedSplit,
    FakedDisorder,
    HostFakeSplit,
    UdpLen,
    Tamper,
}

impl Mode {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "synack" => Self::SynAck,
            "syndata" => Self::SynData,
            "fake" => Self::Fake,
            "fakeknown" => Self::FakeKnown,
            "rst" => Self::Rst,
            "rstack" => Self::RstAck,
            "hopbyhop" => Self::HopByHop,
            "destopt" => Self::DestOpt,
            "ipfrag1" => Self::IpFrag1,
            "ipfrag2" => Self::IpFrag2,
            "multisplit" => Self::Split,
            "multidisorder" => Self::Disorder,
            "fakedsplit" => Self::FakedSplit,
            "fakeddisorder" => Self::FakedDisorder,
            "hostfakesplit" => Self::HostFakeSplit,
            "udplen" => Self::UdpLen,
            "tamper" => Self::Tamper,
            _ => return None,
        })
    }

    fn splits(&self) -> bool {
        matches!(
            self,
            Self::Split
                | Self::Disorder
                | Self::FakedSplit
                | Self::FakedDisorder
                | Self::HostFakeSplit
        )
    }

    fn english(&self) -> &'static str {
        match self {
            Self::SynAck => "Fake SYN-ACK",
            Self::SynData => "Data in SYN",
            Self::Fake => "Fake packet",
            Self::FakeKnown => "Fake packet for known protocols",
            Self::Rst => "Fake reset",
            Self::RstAck => "Fake reset-ACK",
            Self::HopByHop => "IPv6 hop-by-hop header",
            Self::DestOpt => "IPv6 destination header",
            Self::IpFrag1 | Self::IpFrag2 => "IP fragments",
            Self::Split => "Split",
            Self::Disorder => "Reordered split",
            Self::FakedSplit => "Fake + split",
            Self::FakedDisorder => "Fake + reordered split",
            Self::HostFakeSplit => "Fake host + split",
            Self::UdpLen => "Padded UDP",
            Self::Tamper => "Tampered packet",
        }
    }
}

/// The trick that makes the fake packet die before the site, or the site
/// drop it, while the filter still reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fooling {
    Md5Sig,
    Timestamp,
    BadSeq,
    BadSum,
    DataNoAck,
    HopByHop,
    HopByHop2,
}

impl Fooling {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "md5sig" => Self::Md5Sig,
            "ts" => Self::Timestamp,
            "badseq" => Self::BadSeq,
            "badsum" => Self::BadSum,
            "datanoack" => Self::DataNoAck,
            "hopbyhop" => Self::HopByHop,
            "hopbyhop2" => Self::HopByHop2,
            _ => return None,
        })
    }

    fn english(&self) -> &'static str {
        match self {
            Self::Md5Sig => "MD5 signature",
            Self::Timestamp => "old timestamp",
            Self::BadSeq => "bad sequence number",
            Self::BadSum => "bad checksum",
            Self::DataNoAck => "no ACK flag",
            Self::HopByHop => "IPv6 hop-by-hop header",
            Self::HopByHop2 => "two IPv6 hop-by-hop headers",
        }
    }
}

/// A place in the packet: a byte offset or a named point, plus an offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    Byte(String),
    FromEnd(String),
    Marker {
        marker: Marker,
        offset: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Method,
    Host,
    EndHost,
    Sld,
    EndSld,
    MidSld,
    SniExt,
}

impl Position {
    fn parse(p: &str) -> Self {
        if let Some(n) = p.strip_prefix('-') {
            return Self::FromEnd(n.to_owned());
        }
        let end = p.find(['+', '-']).unwrap_or(p.len());
        let marker = match &p[..end] {
            "method" => Marker::Method,
            "host" => Marker::Host,
            "endhost" => Marker::EndHost,
            "sld" => Marker::Sld,
            "endsld" => Marker::EndSld,
            "midsld" => Marker::MidSld,
            "sniext" => Marker::SniExt,
            _ => return Self::Byte(p.to_owned()),
        };
        let offset = (end < p.len()).then(|| p[end..].to_owned());
        Self::Marker { marker, offset }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Byte(n) => write!(f, "byte {n}"),
            Self::FromEnd(n) => write!(f, "{n} bytes from the end"),
            Self::Marker { marker, offset } => {
                f.write_str(match marker {
                    Marker::Method => "HTTP method",
                    Marker::Host => "host name",
                    Marker::EndHost => "end of host name",
                    Marker::Sld => "domain",
                    Marker::EndSld => "end of domain",
                    Marker::MidSld => "middle of domain",
                    Marker::SniExt => "SNI",
                })?;
                match offset {
                    Some(o) => write!(f, " {}", o.replacen('-', "−", 1)),
                    None => Ok(()),
                }
            }
        }
    }
}

/// Something about a profile besides its modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Ttl(String),
    /// `delta` is empty for nfqws's default; `fallback` is the plain TTL
    /// used when the hop count cannot be guessed.
    AutoTtl {
        delta: String,
        fallback: Option<String>,
    },
    Fooling(Vec<Fooling>),
    SplitAt(Vec<Position>),
    Overlap(Position),
    AltOrder,
    Repeats(String),
    CustomFake,
    /// A low TTL on the real packets themselves.
    OrigTtl(String),
    HostSpell(String),
    HostCase,
    HostNoSpace,
    DomCase,
    MethodEol,
    /// Any other option, shown as written.
    Option {
        name: &'static str,
        value: Option<String>,
    },
}

impl Detail {
    /// The details that work on their own, without a desync mode, and so
    /// can name a profile.
    fn standalone(&self) -> Option<&'static str> {
        Some(match self {
            Self::HostSpell(_) => "Respelled Host header",
            Self::HostCase => "Mixed-case Host header",
            Self::HostNoSpace => "No space in Host header",
            Self::DomCase => "Mixed-case site name",
            Self::MethodEol => "Line break after HTTP method",
            Self::OrigTtl(_) => "Low TTL",
            _ => return None,
        })
    }
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ttl(n) => write!(f, "TTL {n}"),
            Self::AutoTtl { delta, fallback } => {
                f.write_str("automatic TTL")?;
                if !delta.is_empty() {
                    write!(f, " {}", delta.replacen('-', "−", 1))?;
                }
                match fallback {
                    Some(t) => write!(f, " or {t}"),
                    None => Ok(()),
                }
            }
            Self::Fooling(list) => {
                let names: Vec<&str> = list.iter().map(Fooling::english).collect();
                f.write_str(&names.join(" and "))
            }
            Self::SplitAt(positions) => write!(f, "split at {}", join(positions)),
            Self::Overlap(p) => write!(f, "overlap to {p}"),
            Self::AltOrder => f.write_str("alternate order"),
            Self::Repeats(n) => write!(f, "sent {n} times"),
            Self::CustomFake => f.write_str("custom fake"),
            Self::OrigTtl(n) => write!(f, "real packets TTL {n}"),
            Self::HostSpell(s) => write!(f, "Host spelled “{s}”"),
            Self::HostCase => f.write_str("mixed-case Host header"),
            Self::HostNoSpace => f.write_str("no space in Host header"),
            Self::DomCase => f.write_str("mixed-case site name"),
            Self::MethodEol => f.write_str("line break after HTTP method"),
            Self::Option { name, value: None } => write!(f, "--{name}"),
            Self::Option {
                name,
                value: Some(v),
            } => write!(f, "--{name}={v}"),
        }
    }
}

fn join(positions: &[Position]) -> String {
    positions
        .iter()
        .map(Position::to_string)
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Which traffic a profile is for, shown when a strategy has several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Http,
    Https,
    Web,
    Quic,
    Tcp(String),
    Udp(String),
    Protocols(String),
    Sites(String),
    All,
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http => f.write_str("HTTP"),
            Self::Https => f.write_str("HTTPS"),
            Self::Web => f.write_str("Web"),
            Self::Quic => f.write_str("QUIC"),
            Self::Tcp(p) => write!(f, "TCP {p}"),
            Self::Udp(p) => write!(f, "UDP {p}"),
            Self::Protocols(p) => f.write_str(&p.to_uppercase()),
            Self::Sites(s) => f.write_str(s),
            Self::All => f.write_str("Everything else"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileDescription {
    pub target: Target,
    pub modes: Vec<Mode>,
    pub details: Vec<Detail>,
}

impl fmt::Display for ProfileDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut details = self.details.iter().peekable();
        // a splitting mode reads "Fake + split at byte 1", not "…, split at byte 1"
        let mut split_at = None;
        if self.modes.is_empty() {
            match details.peek().and_then(|d| d.standalone()) {
                Some(name) => {
                    f.write_str(name)?;
                    if let Some(Detail::HostSpell(s)) = details.next() {
                        write!(f, " “{s}”")?;
                    }
                }
                None => f.write_str("Unchanged")?,
            }
        } else {
            for (i, mode) in self.modes.iter().enumerate() {
                if i == 0 {
                    f.write_str(mode.english())?;
                } else {
                    write!(f, " + {}", lower_first(mode.english()))?;
                }
            }
            if self.modes.iter().any(Mode::splits) {
                split_at = self
                    .details
                    .iter()
                    .find(|d| matches!(d, Detail::SplitAt(_)));
                if let Some(Detail::SplitAt(positions)) = split_at {
                    write!(f, " at {}", join(positions))?;
                }
            }
        }
        for detail in details {
            if split_at != Some(detail) {
                write!(f, ", {detail}")?;
            }
        }
        Ok(())
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        // keep acronyms: "IP fragments", "IPv6 ..."
        Some(c) if chars.clone().next().is_some_and(char::is_lowercase) => {
            c.to_lowercase().chain(chars).collect()
        }
        _ => s.to_owned(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description(pub Vec<ProfileDescription>);

impl fmt::Display for Description {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let [only] = self.0.as_slice() {
            return write!(f, "{only}");
        }
        for (i, p) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{}: {p}", p.target)?;
        }
        Ok(())
    }
}

pub fn describe(strategy: &Strategy) -> Description {
    Description(strategy.profiles().iter().map(describe_profile).collect())
}

fn describe_profile(profile: &Profile) -> ProfileDescription {
    let modes = profile
        .desync_modes()
        .into_iter()
        .filter_map(Mode::from_name)
        .collect();
    let mut details = Vec::new();
    let tokens = profile.tokens();
    for (i, token) in tokens.iter().enumerate() {
        let Token::Option { spec, value } = token else {
            continue;
        };
        // nfqws keeps an option's last occurrence: describe that one only
        let again = tokens[i + 1..]
            .iter()
            .any(|t| matches!(t, Token::Option { spec: s, .. } if s.name == spec.name));
        if again {
            continue;
        }
        let v = || value.clone().unwrap_or_default();
        let detail = match spec.name {
            // modes, filters and helpers folded into other details
            "dpi-desync"
            | "filter-tcp"
            | "filter-udp"
            | "filter-l7"
            | "filter-l3"
            | "hostlist-domains"
            | "dpi-desync-badseq-increment"
            | "orig-mod-start"
            | "orig-mod-cutoff" => continue,
            "dpi-desync-ttl"
                if profile.has("dpi-desync-autottl")
                    && profile.get("dpi-desync-autottl") != Some("-") =>
            {
                continue;
            }
            "dpi-desync-ttl" => Detail::Ttl(v()),
            "dpi-desync-autottl" if value.as_deref() == Some("-") => continue,
            "dpi-desync-autottl" => Detail::AutoTtl {
                delta: v(),
                fallback: profile.get("dpi-desync-ttl").map(str::to_owned),
            },
            "dpi-desync-fooling" => {
                let list: Vec<Fooling> = profile
                    .list("dpi-desync-fooling")
                    .into_iter()
                    .filter_map(Fooling::from_name)
                    .collect();
                if list.is_empty() {
                    continue;
                }
                Detail::Fooling(list)
            }
            "dpi-desync-split-pos" => Detail::SplitAt(
                profile
                    .list("dpi-desync-split-pos")
                    .into_iter()
                    .map(Position::parse)
                    .collect(),
            ),
            "dpi-desync-split-seqovl" => Detail::Overlap(Position::parse(&v())),
            "dpi-desync-fakedsplit-mod" | "dpi-desync-hostfakesplit-mod"
                if v()
                    .split(',')
                    .any(|m| m.starts_with("altorder=") && m != "altorder=0") =>
            {
                Detail::AltOrder
            }
            "dpi-desync-repeats" => Detail::Repeats(v()),
            name if name.starts_with("dpi-desync-fake-") && name != "dpi-desync-fake-tcp-mod" => {
                if details.contains(&Detail::CustomFake) {
                    continue;
                }
                Detail::CustomFake
            }
            "orig-ttl" => Detail::OrigTtl(v()),
            "hostspell" => Detail::HostSpell(v()),
            "hostcase" => Detail::HostCase,
            "hostnospace" => Detail::HostNoSpace,
            "domcase" => Detail::DomCase,
            "methodeol" => Detail::MethodEol,
            name => Detail::Option {
                name,
                value: value.clone(),
            },
        };
        details.push(detail);
    }
    ProfileDescription {
        target: target(profile),
        modes,
        details,
    }
}

fn target(profile: &Profile) -> Target {
    if let Some(sites) = profile.get("hostlist-domains") {
        return Target::Sites(sites.replace(',', ", "));
    }
    if let Some(l7) = profile.get("filter-l7") {
        return Target::Protocols(l7.replace(',', ", "));
    }
    match (profile.get("filter-tcp"), profile.get("filter-udp")) {
        (Some("80"), None) => Target::Http,
        (Some("443"), None) => Target::Https,
        (Some("80,443" | "443,80"), None) => Target::Web,
        (None, Some("443")) => Target::Quic,
        (Some(p), None) => Target::Tcp(p.to_owned()),
        (None, Some(p)) => Target::Udp(p.to_owned()),
        _ => Target::All,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> String {
        describe(&Strategy::parse(s).unwrap()).to_string()
    }

    #[test]
    fn names() {
        assert_eq!(
            name("--dpi-desync=fake --dpi-desync-ttl=8"),
            "Fake packet, TTL 8"
        );
        assert_eq!(
            name("--filter-tcp=80,443 --dpi-desync=fake --dpi-desync-ttl=8 <HOSTLIST>"),
            "Fake packet, TTL 8"
        );
        assert_eq!(
            name("--dpi-desync=fake --dpi-desync-fooling=ts"),
            "Fake packet, old timestamp"
        );
        assert_eq!(
            name(
                "--dpi-desync=fakeddisorder --dpi-desync-ttl=1 --dpi-desync-autottl=-3 --dpi-desync-split-pos=1"
            ),
            "Fake + reordered split at byte 1, automatic TTL −3 or 1"
        );
        assert_eq!(
            name(
                "--dpi-desync=fake,multisplit --dpi-desync-split-pos=method+2,midsld --dpi-desync-fooling=badseq,md5sig"
            ),
            "Fake packet + split at HTTP method +2 and middle of domain, bad sequence number and MD5 signature"
        );
        assert_eq!(name("--hostspell=hoSt"), "Respelled Host header “hoSt”");
        assert_eq!(
            name("--dpi-desync=ipfrag2,udplen"),
            "IP fragments + padded UDP"
        );
    }

    #[test]
    fn several_profiles_name_their_traffic() {
        assert_eq!(
            name(
                "--filter-tcp=80 --dpi-desync=fake <HOSTLIST> --new --filter-udp=443 --dpi-desync=fake --dpi-desync-repeats=6"
            ),
            "HTTP: Fake packet; QUIC: Fake packet, sent 6 times"
        );
    }

    #[test]
    fn a_repeated_option_counts_once_as_its_last_value() {
        // what the old app wrote: every option it tried, one after another
        let s = Strategy::parse(
            "--dpi-desync=fake --dpi-desync-fooling=badseq --dpi-desync-badseq-increment=0 \
             --dpi-desync=fake --dpi-desync-fooling=ts --dpi-desync=fakedsplit \
             --dpi-desync-fooling=ts --dpi-desync-split-pos=1 --dpi-desync=fakedsplit \
             --dpi-desync-fooling=md5sig --dpi-desync-split-pos=1 --dpi-desync=fakeddisorder \
             --dpi-desync-ttl=1 --dpi-desync-autottl=-4 --dpi-desync-split-pos=1 \
             --dpi-desync=fakeddisorder --dpi-desync-ttl=1 --dpi-desync-autottl=-5 \
             --dpi-desync-split-pos=1",
        )
        .unwrap();
        assert_eq!(
            describe(&s).to_string(),
            "Fake + reordered split at byte 1, MD5 signature, automatic TTL −5 or 1"
        );
    }
}
