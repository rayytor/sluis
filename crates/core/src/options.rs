//! nfqws's command-line options (zapret 72.8, `nfq/nfqws.c`) and which of
//! them a strategy may contain.
//!
//! A strategy is passed to nfqws by the privileged helper, so this table is
//! an allow-list: only [`Kind::Desync`] and [`Kind::Filter`] options are
//! accepted; everything that names files, users, queues or daemon settings is
//! the helper's business and is rejected.

/// Whether an option takes a value (`--name=value`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg {
    None,
    Required,
    Optional,
}

/// What an option does, which decides whether a strategy may use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Changes how packets are sent: the strategy itself.
    Desync,
    /// Chooses which traffic a profile applies to (`--filter-*`, `--new`,
    /// inline domain and IP lists).
    Filter,
    /// Reads a list from a file. Zapret's own lists come in through the
    /// `<HOSTLIST>` placeholders instead.
    File,
    /// Process settings the helper owns: queue, user, pidfile, debug.
    Daemon,
    /// Only in winws, the Windows build.
    Windows,
}

/// How an option's value is checked (see `strategy.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// Any value made of the safe characters.
    Text,
    /// `--dpi-desync` modes, comma separated.
    Modes,
    /// 1..=255.
    Ttl,
    /// `delta[:min[-max]]`, or `-` to switch it off.
    AutoTtl,
    /// Fooling methods, comma separated.
    Fooling,
    /// Split positions: `N`, `-N`, `marker`, `marker+N`, comma separated.
    Positions,
    /// One split position.
    Position,
    /// `[n|d|s]N`: packet number, data packet number or relative sequence.
    PacketLimit,
    /// A small positive count.
    Count,
    /// Ports and ranges, comma separated.
    Ports,
    /// Protocol names for `--filter-l7`.
    L7,
    /// `ipv4` or `ipv6`.
    L3,
    /// Domain names, comma separated.
    Domains,
    /// Exactly four letters spelling "host".
    HostSpell,
    /// Split modifiers such as `altorder=1`.
    Mods,
    /// A fake payload: `0x` hex, `!` for nfqws's own, or a file in zapret's
    /// fake payload folder.
    Payload,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OptionSpec {
    pub name: &'static str,
    pub arg: Arg,
    pub kind: Kind,
    pub value: Value,
}

impl OptionSpec {
    pub fn allowed_in_strategy(&self) -> bool {
        matches!(self.kind, Kind::Desync | Kind::Filter)
    }
}

const fn opt(name: &'static str, arg: Arg, kind: Kind, value: Value) -> OptionSpec {
    OptionSpec {
        name,
        arg,
        kind,
        value,
    }
}

/// Every long option nfqws 72.8 accepts, in its own order.
pub static OPTIONS: &[OptionSpec] = &[
    opt("debug", Arg::Optional, Kind::Daemon, Value::Text),
    opt("dry-run", Arg::None, Kind::Daemon, Value::Text),
    opt("version", Arg::None, Kind::Daemon, Value::Text),
    opt("comment", Arg::Optional, Kind::Daemon, Value::Text),
    opt("qnum", Arg::Required, Kind::Daemon, Value::Text),
    opt("port", Arg::Required, Kind::Daemon, Value::Text),
    opt("daemon", Arg::None, Kind::Daemon, Value::Text),
    opt("pidfile", Arg::Required, Kind::Daemon, Value::Text),
    opt("user", Arg::Required, Kind::Daemon, Value::Text),
    opt("uid", Arg::Required, Kind::Daemon, Value::Text),
    opt("wsize", Arg::Required, Kind::Desync, Value::Text),
    opt("wssize", Arg::Required, Kind::Desync, Value::Text),
    opt(
        "wssize-cutoff",
        Arg::Required,
        Kind::Desync,
        Value::PacketLimit,
    ),
    opt(
        "wssize-forced-cutoff",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt("synack-split", Arg::Optional, Kind::Desync, Value::Text),
    opt("ctrack-timeouts", Arg::Required, Kind::Daemon, Value::Text),
    opt("ctrack-disable", Arg::Optional, Kind::Daemon, Value::Text),
    opt("ipcache-lifetime", Arg::Required, Kind::Daemon, Value::Text),
    opt("ipcache-hostname", Arg::Optional, Kind::Daemon, Value::Text),
    opt("hostcase", Arg::None, Kind::Desync, Value::Text),
    opt("hostspell", Arg::Required, Kind::Desync, Value::HostSpell),
    opt("hostnospace", Arg::None, Kind::Desync, Value::Text),
    opt("domcase", Arg::None, Kind::Desync, Value::Text),
    opt("methodeol", Arg::None, Kind::Desync, Value::Text),
    opt("ip-id", Arg::Required, Kind::Desync, Value::Text),
    opt("dpi-desync", Arg::Required, Kind::Desync, Value::Modes),
    opt(
        "dpi-desync-fwmark",
        Arg::Required,
        Kind::Daemon,
        Value::Text,
    ),
    opt(
        "dpi-desync-sockarg",
        Arg::Required,
        Kind::Daemon,
        Value::Text,
    ),
    opt("dup", Arg::Required, Kind::Desync, Value::Count),
    opt("dup-ttl", Arg::Required, Kind::Desync, Value::Ttl),
    opt("dup-ttl6", Arg::Required, Kind::Desync, Value::Ttl),
    opt("dup-autottl", Arg::Optional, Kind::Desync, Value::AutoTtl),
    opt("dup-autottl6", Arg::Optional, Kind::Desync, Value::AutoTtl),
    opt(
        "dup-tcp-flags-set",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dup-tcp-flags-unset",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt("dup-fooling", Arg::Required, Kind::Desync, Value::Fooling),
    opt("dup-ts-increment", Arg::Required, Kind::Desync, Value::Text),
    opt(
        "dup-badseq-increment",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dup-badack-increment",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt("dup-replace", Arg::Optional, Kind::Desync, Value::Text),
    opt("dup-ip-id", Arg::Required, Kind::Desync, Value::Text),
    opt("dup-start", Arg::Required, Kind::Desync, Value::PacketLimit),
    opt(
        "dup-cutoff",
        Arg::Required,
        Kind::Desync,
        Value::PacketLimit,
    ),
    opt("orig-ttl", Arg::Required, Kind::Desync, Value::Ttl),
    opt("orig-ttl6", Arg::Required, Kind::Desync, Value::Ttl),
    opt("orig-autottl", Arg::Optional, Kind::Desync, Value::AutoTtl),
    opt("orig-autottl6", Arg::Optional, Kind::Desync, Value::AutoTtl),
    opt(
        "orig-tcp-flags-set",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "orig-tcp-flags-unset",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "orig-mod-start",
        Arg::Required,
        Kind::Desync,
        Value::PacketLimit,
    ),
    opt(
        "orig-mod-cutoff",
        Arg::Required,
        Kind::Desync,
        Value::PacketLimit,
    ),
    opt("dpi-desync-ttl", Arg::Required, Kind::Desync, Value::Ttl),
    opt("dpi-desync-ttl6", Arg::Required, Kind::Desync, Value::Ttl),
    opt(
        "dpi-desync-autottl",
        Arg::Optional,
        Kind::Desync,
        Value::AutoTtl,
    ),
    opt(
        "dpi-desync-autottl6",
        Arg::Optional,
        Kind::Desync,
        Value::AutoTtl,
    ),
    opt(
        "dpi-desync-tcp-flags-set",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-tcp-flags-unset",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-fooling",
        Arg::Required,
        Kind::Desync,
        Value::Fooling,
    ),
    opt(
        "dpi-desync-repeats",
        Arg::Required,
        Kind::Desync,
        Value::Count,
    ),
    opt(
        "dpi-desync-skip-nosni",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-split-pos",
        Arg::Required,
        Kind::Desync,
        Value::Positions,
    ),
    opt(
        "dpi-desync-split-http-req",
        Arg::Required,
        Kind::Desync,
        Value::Positions,
    ),
    opt(
        "dpi-desync-split-tls",
        Arg::Required,
        Kind::Desync,
        Value::Positions,
    ),
    opt(
        "dpi-desync-split-seqovl",
        Arg::Required,
        Kind::Desync,
        Value::Position,
    ),
    opt(
        "dpi-desync-split-seqovl-pattern",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fakedsplit-pattern",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fakedsplit-mod",
        Arg::Required,
        Kind::Desync,
        Value::Mods,
    ),
    opt(
        "dpi-desync-hostfakesplit-midhost",
        Arg::Required,
        Kind::Desync,
        Value::Position,
    ),
    opt(
        "dpi-desync-hostfakesplit-mod",
        Arg::Required,
        Kind::Desync,
        Value::Mods,
    ),
    opt(
        "dpi-desync-ipfrag-pos-tcp",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-ipfrag-pos-udp",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-ts-increment",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-badseq-increment",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-badack-increment",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-any-protocol",
        Arg::Optional,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-fake-tcp-mod",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-fake-http",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-tls",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-tls-mod",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-fake-unknown",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-syndata",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-quic",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-wireguard",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-dht",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-discord",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-stun",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-fake-unknown-udp",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-udplen-increment",
        Arg::Required,
        Kind::Desync,
        Value::Text,
    ),
    opt(
        "dpi-desync-udplen-pattern",
        Arg::Required,
        Kind::Desync,
        Value::Payload,
    ),
    opt(
        "dpi-desync-cutoff",
        Arg::Required,
        Kind::Desync,
        Value::PacketLimit,
    ),
    opt(
        "dpi-desync-start",
        Arg::Required,
        Kind::Desync,
        Value::PacketLimit,
    ),
    opt("hostlist", Arg::Required, Kind::File, Value::Text),
    opt(
        "hostlist-domains",
        Arg::Required,
        Kind::Filter,
        Value::Domains,
    ),
    opt("hostlist-exclude", Arg::Required, Kind::File, Value::Text),
    opt(
        "hostlist-exclude-domains",
        Arg::Required,
        Kind::Filter,
        Value::Domains,
    ),
    opt("hostlist-auto", Arg::Required, Kind::File, Value::Text),
    opt(
        "hostlist-auto-fail-threshold",
        Arg::Required,
        Kind::Filter,
        Value::Text,
    ),
    opt(
        "hostlist-auto-fail-time",
        Arg::Required,
        Kind::Filter,
        Value::Text,
    ),
    opt(
        "hostlist-auto-retrans-threshold",
        Arg::Required,
        Kind::Filter,
        Value::Text,
    ),
    opt(
        "hostlist-auto-debug",
        Arg::Required,
        Kind::File,
        Value::Text,
    ),
    opt("new", Arg::None, Kind::Filter, Value::Text),
    opt("skip", Arg::None, Kind::Filter, Value::Text),
    opt("filter-l3", Arg::Required, Kind::Filter, Value::L3),
    opt("filter-tcp", Arg::Required, Kind::Filter, Value::Ports),
    opt("filter-udp", Arg::Required, Kind::Filter, Value::Ports),
    opt("filter-l7", Arg::Required, Kind::Filter, Value::L7),
    opt("filter-ssid", Arg::Required, Kind::Filter, Value::Text),
    opt("ipset", Arg::Required, Kind::File, Value::Text),
    opt("ipset-ip", Arg::Required, Kind::Filter, Value::Text),
    opt("ipset-exclude", Arg::Required, Kind::File, Value::Text),
    opt("ipset-exclude-ip", Arg::Required, Kind::Filter, Value::Text),
    opt("bind-fix4", Arg::None, Kind::Daemon, Value::Text),
    opt("bind-fix6", Arg::None, Kind::Daemon, Value::Text),
    opt("wf-iface", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-l3", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-tcp", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-udp", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-raw", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-raw-part", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-filter-lan", Arg::Required, Kind::Windows, Value::Text),
    opt("wf-save", Arg::Required, Kind::Windows, Value::Text),
    opt("ssid-filter", Arg::Required, Kind::Windows, Value::Text),
    opt("nlm-filter", Arg::Required, Kind::Windows, Value::Text),
    opt("nlm-list", Arg::Optional, Kind::Windows, Value::Text),
];

pub fn lookup(name: &str) -> Option<&'static OptionSpec> {
    OPTIONS.iter().find(|o| o.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        for (i, a) in OPTIONS.iter().enumerate() {
            assert!(
                OPTIONS[i + 1..].iter().all(|b| b.name != a.name),
                "{} twice",
                a.name
            );
        }
    }

    #[test]
    fn helper_owned_options_are_not_allowed() {
        for name in [
            "user", "uid", "qnum", "daemon", "pidfile", "debug", "hostlist", "ipset", "wf-tcp",
        ] {
            assert!(!lookup(name).unwrap().allowed_in_strategy(), "{name}");
        }
    }
}
