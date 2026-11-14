//! mDNS advertisement: `_amremote._tcp.local.` on the configured port,
//! TXT `version=1`, `name=<hostname>` and `ip=<ipv4>` (contract C-002).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};

pub const SERVICE_TYPE: &str = "_amremote._tcp.local.";
/// Contract version advertised in the TXT record.
pub const TXT_VERSION: &str = "1";

/// An address nobody can route to (TEST-NET-1, RFC 5737), used only to ask the OS which interface
/// the default route leaves by. Nothing is ever sent to it.
const ROUTE_PROBE: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);

/// An address a phone on the LAN could actually use.
fn is_lan_usable(ip: Ipv4Addr) -> bool {
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast())
}

/// The laptop's primary LAN IPv4 address: the one the OS would use to reach the outside, so a VPN
/// or virtual adapter that is not on the default route is not picked. `None` when there is no
/// default route (clients then use the address the mDNS resolver gave them).
pub fn primary_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // Connecting a UDP socket sends no packet; it only makes the OS choose a local address.
    socket.connect((ROUTE_PROBE, 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if is_lan_usable(ip) => Some(ip),
        _ => None,
    }
}

/// Builds the service record. Addresses are filled in automatically from the machine's interfaces;
/// `ip` also goes into the TXT record, because the agent listens on IPv4 only and clients must not
/// pick an IPv6 address.
pub fn service_info(hostname: &str, port: u16, ip: Option<Ipv4Addr>) -> Result<ServiceInfo> {
    let mut properties = HashMap::from([
        ("version".to_owned(), TXT_VERSION.to_owned()),
        ("name".to_owned(), hostname.to_owned()),
    ]);
    if let Some(ip) = ip {
        properties.insert("ip".to_owned(), ip.to_string());
    }
    let info = ServiceInfo::new(
        SERVICE_TYPE,
        hostname,
        &format!("{hostname}.local."),
        "",
        port,
        properties,
    )
    .context("cannot build the mDNS service record")?;
    Ok(info.enable_addr_auto())
}

/// How often the LAN address is looked at again (contract C-004: the TXT `ip` must follow a DHCP
/// renewal or a Wi-Fi switch).
pub const IP_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Remembers which `ip` the TXT record carries and says when it has to be published again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IpTracker {
    published: Option<Ipv4Addr>,
}

impl IpTracker {
    fn new(published: Option<Ipv4Addr>) -> Self {
        Self { published }
    }

    /// The address to publish now, or `None` if nothing has to change. A moment without any
    /// address (`None`: the network is down) keeps the last one, so `ip` does not flicker away.
    fn changed(&self, current: Option<Ipv4Addr>) -> Option<Ipv4Addr> {
        current.filter(|ip| Some(*ip) != self.published)
    }
}

/// One look at the LAN address. `publish` re-announces the service with the new `ip`; only when it
/// succeeds is the new address remembered, so a failure is retried at the next look.
/// Returns the address that was published, if any.
fn refresh_ip(
    tracker: &mut IpTracker,
    current: Option<Ipv4Addr>,
    publish: impl FnOnce(Ipv4Addr) -> Result<()>,
) -> Option<Ipv4Addr> {
    let ip = tracker.changed(current)?;
    match publish(ip) {
        Ok(()) => {
            tracker.published = Some(ip);
            Some(ip)
        }
        Err(e) => {
            tracing::warn!("cannot re-publish the mDNS record with the new address: {e:#}");
            None
        }
    }
}

/// Keeps the advertisement alive; it stops when this is dropped or [`Advertisement::stop`] is called.
pub struct Advertisement {
    daemon: ServiceDaemon,
    /// Dropping this wakes the refresh thread and ends it.
    stop_refresh: Option<mpsc::Sender<()>>,
    refresh: Option<JoinHandle<()>>,
}

impl Advertisement {
    pub fn stop(mut self) {
        drop(self.stop_refresh.take());
        if let Some(handle) = self.refresh.take() {
            let _ = handle.join();
        }
        if let Err(e) = self.daemon.shutdown() {
            tracing::warn!("mDNS shutdown failed: {e}");
        }
    }
}

/// Looks at the LAN address every [`IP_CHECK_INTERVAL`] and re-announces the service when it changed.
fn spawn_ip_refresh(
    daemon: ServiceDaemon,
    hostname: String,
    port: u16,
    mut tracker: IpTracker,
) -> Result<(mpsc::Sender<()>, JoinHandle<()>)> {
    let (stop, stopped) = mpsc::channel::<()>();
    let handle = std::thread::Builder::new()
        .name("mdns-ip-refresh".into())
        .spawn(move || {
            // `recv_timeout` is the sleep: it ends at once when the sender is dropped.
            while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(IP_CHECK_INTERVAL)
            {
                let before = tracker.published;
                let published = refresh_ip(&mut tracker, primary_ipv4(), |ip| {
                    let info = service_info(&hostname, port, Some(ip))?;
                    daemon
                        .register(info)
                        .context("cannot register the mDNS service")
                });
                if let Some(ip) = published {
                    tracing::info!(?before, now = %ip, "LAN address changed: mDNS `ip` updated");
                }
            }
        })
        .context("cannot start the mDNS refresh thread")?;
    Ok((stop, handle))
}

/// Starts advertising this machine on the LAN.
pub fn advertise(port: u16) -> Result<Advertisement> {
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let ip = primary_ipv4();
    if ip.is_none() {
        tracing::warn!("no LAN IPv4 address found (no default route?): advertising without `ip`");
    }
    let info = service_info(&hostname, port, ip)?;
    let daemon = ServiceDaemon::new().context("cannot start the mDNS daemon")?;
    daemon
        .register(info)
        .context("cannot register the mDNS service")?;
    tracing::info!(%hostname, port, ?ip, "advertising {SERVICE_TYPE}");
    let (stop_refresh, refresh) =
        spawn_ip_refresh(daemon.clone(), hostname, port, IpTracker::new(ip))?;
    Ok(Advertisement {
        daemon,
        stop_refresh: Some(stop_refresh),
        refresh: Some(refresh),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_matches_the_contract() {
        let info = service_info("MY-LAPTOP", 8765, Some(Ipv4Addr::new(192, 168, 1, 23))).unwrap();
        assert_eq!(info.get_type(), "_amremote._tcp.local.");
        assert_eq!(info.get_port(), 8765);
        assert_eq!(info.get_property_val_str("version"), Some("1"));
        assert_eq!(info.get_property_val_str("name"), Some("MY-LAPTOP"));
        assert_eq!(info.get_property_val_str("ip"), Some("192.168.1.23"));
    }

    #[test]
    fn without_an_address_the_ip_field_is_left_out() {
        let info = service_info("MY-LAPTOP", 8765, None).unwrap();
        assert_eq!(info.get_property_val_str("version"), Some("1"));
        assert_eq!(info.get_property_val_str("ip"), None);
    }

    #[test]
    fn only_addresses_a_phone_can_reach_are_advertised() {
        for good in [
            Ipv4Addr::new(192, 168, 1, 23),
            Ipv4Addr::new(10, 0, 0, 5),
            Ipv4Addr::new(172, 16, 4, 9),
        ] {
            assert!(is_lan_usable(good), "{good}");
        }
        for bad in [
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::new(169, 254, 10, 1),
            Ipv4Addr::new(224, 0, 0, 251),
            Ipv4Addr::BROADCAST,
        ] {
            assert!(!is_lan_usable(bad), "{bad}");
        }
    }

    const HOME: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 23);
    const CAFE: Ipv4Addr = Ipv4Addr::new(10, 20, 30, 40);

    #[test]
    fn an_unchanged_address_is_not_published_again() {
        let mut tracker = IpTracker::new(Some(HOME));
        let mut calls = 0;
        let published = refresh_ip(&mut tracker, Some(HOME), |_| {
            calls += 1;
            Ok(())
        });
        assert_eq!((published, calls), (None, 0));
    }

    #[test]
    fn a_new_address_is_published_once_and_remembered() {
        let mut tracker = IpTracker::new(Some(HOME));
        let mut sent = Vec::new();
        let published = refresh_ip(&mut tracker, Some(CAFE), |ip| {
            sent.push(ip);
            Ok(())
        });
        assert_eq!(published, Some(CAFE));
        assert_eq!(sent, vec![CAFE]);
        assert_eq!(tracker, IpTracker::new(Some(CAFE)));
        // The next look finds the same address: nothing more to do.
        assert_eq!(refresh_ip(&mut tracker, Some(CAFE), |_| Ok(())), None);
    }

    #[test]
    fn the_first_address_is_published_when_there_was_none_at_startup() {
        let mut tracker = IpTracker::new(None);
        assert_eq!(refresh_ip(&mut tracker, None, |_| Ok(())), None);
        assert_eq!(refresh_ip(&mut tracker, Some(HOME), |_| Ok(())), Some(HOME));
    }

    #[test]
    fn losing_the_network_keeps_the_last_address() {
        let mut tracker = IpTracker::new(Some(HOME));
        assert_eq!(refresh_ip(&mut tracker, None, |_| Ok(())), None);
        assert_eq!(tracker, IpTracker::new(Some(HOME)));
        // Back on the same network: still nothing to publish.
        assert_eq!(refresh_ip(&mut tracker, Some(HOME), |_| Ok(())), None);
    }

    #[test]
    fn a_failed_publish_is_retried_at_the_next_look() {
        let mut tracker = IpTracker::new(Some(HOME));
        let failed = refresh_ip(&mut tracker, Some(CAFE), |_| {
            Err(anyhow::anyhow!("daemon busy"))
        });
        assert_eq!(failed, None);
        assert_eq!(tracker, IpTracker::new(Some(HOME)), "not remembered");
        assert_eq!(refresh_ip(&mut tracker, Some(CAFE), |_| Ok(())), Some(CAFE));
    }

    #[test]
    fn the_republished_record_carries_the_new_ip() {
        let info = service_info("MY-LAPTOP", 8765, Some(CAFE)).unwrap();
        assert_eq!(info.get_property_val_str("ip"), Some("10.20.30.40"));
        assert_eq!(info.get_property_val_str("version"), Some("1"));
    }

    #[test]
    fn the_primary_address_is_a_usable_ipv4_when_there_is_one() {
        // Offline machines have no default route and give `None`; otherwise it must be usable.
        if let Some(ip) = primary_ipv4() {
            assert!(is_lan_usable(ip), "{ip}");
        }
    }
}
