//! Which network the computer is on, so each network can keep the method
//! that works there (docs/PLAN.md §2.5). A network is its router: the gateway's
//! hardware address, read from /proc. The name people know it by comes
//! from NetworkManager or iwd when either runs.

use std::collections::HashMap;
use std::net::Ipv4Addr;

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub id: String,
    pub name: String,
}

/// The default route's interface and gateway, from /proc/net/route.
fn gateway(routes: &str) -> Option<(String, Ipv4Addr)> {
    routes.lines().skip(1).find_map(|line| {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 || f[1] != "00000000" {
            return None;
        }
        // little-endian hex
        let raw = u32::from_str_radix(f[2], 16).ok()?;
        Some((f[0].to_owned(), Ipv4Addr::from(raw.swap_bytes())))
    })
}

/// The hardware address of `ip`, from /proc/net/arp.
fn hardware_address(table: &str, ip: Ipv4Addr) -> Option<String> {
    table.lines().skip(1).find_map(|line| {
        let f: Vec<&str> = line.split_whitespace().collect();
        (f.len() >= 4 && f[0] == ip.to_string() && f[3] != "00:00:00:00:00:00")
            .then(|| f[3].to_owned())
    })
}

async fn property<T: TryFrom<OwnedValue>>(
    conn: &zbus::Connection,
    service: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<T>
where
    T::Error: Into<zbus::Error>,
{
    zbus::Proxy::new(conn, service, path, interface)
        .await
        .ok()?
        .get_property(name)
        .await
        .ok()
}

/// The active connection's name in NetworkManager.
async fn network_manager_name(conn: &zbus::Connection) -> Option<String> {
    const NM: &str = "org.freedesktop.NetworkManager";
    let active: OwnedObjectPath = property(
        conn,
        NM,
        "/org/freedesktop/NetworkManager",
        NM,
        "PrimaryConnection",
    )
    .await?;
    if active.as_str() == "/" {
        return None;
    }
    property(
        conn,
        NM,
        active.as_str(),
        "org.freedesktop.NetworkManager.Connection.Active",
        "Id",
    )
    .await
}

/// The Wi-Fi network iwd is connected to.
async fn iwd_name(conn: &zbus::Connection) -> Option<String> {
    let manager = zbus::fdo::ObjectManagerProxy::builder(conn)
        .destination("net.connman.iwd")
        .ok()?
        .path("/")
        .ok()?
        .build()
        .await
        .ok()?;
    let objects = manager.get_managed_objects().await.ok()?;
    let get = |props: &HashMap<_, OwnedValue>, key: &str| props.get(key).cloned();
    let network = objects.values().find_map(|interfaces| {
        let station = interfaces
            .iter()
            .find(|(i, _)| i.as_str() == "net.connman.iwd.Station")?
            .1;
        let state: String = get(station, "State")?.try_into().ok()?;
        if state != "connected" {
            return None;
        }
        OwnedObjectPath::try_from(get(station, "ConnectedNetwork")?).ok()
    })?;
    let interfaces = objects.get(&network)?;
    let props = interfaces
        .iter()
        .find(|(i, _)| i.as_str() == "net.connman.iwd.Network")?
        .1;
    get(props, "Name")?.try_into().ok()
}

/// The network the default route goes through, `None` when offline.
pub async fn current() -> Option<Network> {
    let routes = std::fs::read_to_string("/proc/net/route").ok()?;
    let (interface, gateway) = gateway(&routes)?;
    let arp = std::fs::read_to_string("/proc/net/arp").unwrap_or_default();
    let id = match hardware_address(&arp, gateway) {
        Some(mac) => format!("router:{mac}"),
        // a tunnel or point-to-point link has no router address
        None => format!("route:{interface}:{gateway}"),
    };
    let name = match zbus::Connection::system().await {
        Ok(conn) => match network_manager_name(&conn).await {
            Some(name) => Some(name),
            None => iwd_name(&conn).await,
        },
        Err(_) => None,
    };
    let wireless = std::path::Path::new("/sys/class/net")
        .join(&interface)
        .join("wireless")
        .exists();
    let name = name.unwrap_or_else(|| {
        if wireless {
            gettextrs::gettext("Wi‑Fi")
        } else {
            gettextrs::gettext("Wired connection")
        }
    });
    Some(Network { id, name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_router() {
        let routes = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\n\
                      wlan0\t0001A8C0\t00000000\t0001\t0\t0\t303\t00FFFFFF\n\
                      wlan0\t00000000\t0101A8C0\t0003\t0\t0\t303\t00000000\n";
        let (interface, ip) = gateway(routes).unwrap();
        assert_eq!(
            (interface.as_str(), ip),
            ("wlan0", Ipv4Addr::new(192, 168, 1, 1))
        );
        let arp = "IP address       HW type     Flags       HW address            Mask     Device\n\
                   192.168.1.46     0x1         0x2         84:a4:66:f5:1f:7f     *        wlan0\n\
                   192.168.1.1      0x1         0x2         d8:42:f7:6a:f9:3b     *        wlan0\n";
        assert_eq!(
            hardware_address(arp, ip).as_deref(),
            Some("d8:42:f7:6a:f9:3b")
        );
        assert_eq!(hardware_address(arp, Ipv4Addr::new(10, 0, 0, 1)), None);
        assert_eq!(gateway("Iface\tDestination\n"), None);
    }
}
