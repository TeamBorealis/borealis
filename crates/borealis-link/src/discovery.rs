//! mDNS discovery of an open Send/Receive window by its nameplate (feature `mdns`, desktop).
//!
//! The listener advertises `_borealis-link._tcp.local.` with TXT `v=1`, `np=<nameplate>` only
//! while its window is open (the [`Advertisement`] unregisters when dropped). The typing device
//! browses for that nameplate to learn `ip:port`. Discovery is a convenience and never trusted:
//! the advert carries no name or fingerprint, and a squatter advertising the same nameplate only
//! gets one failed SPAKE2 attempt, which kills the code.
//!
//! Mobile: raw multicast on iOS needs Apple's restricted multicast entitlement, and Android needs
//! a `MulticastLock`; neither is in place, so phones rely on the QR (which carries `ip:port`) or
//! a typed `host:port` next to the code (spike X0.1). The app enables this feature on desktop only.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::error::LinkError;
use crate::random::{hex, random_bytes};

/// DNS-SD service type of the device link.
pub const SERVICE_TYPE: &str = "_borealis-link._tcp.local.";
const TXT_VERSION: &str = "1";

fn mdns_err(e: impl std::fmt::Display) -> LinkError {
    LinkError::Io(format!("mdns: {e}"))
}

/// A running advertisement; dropping it withdraws the service.
pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
}

impl std::fmt::Debug for Advertisement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Advertisement")
            .field("fullname", &self.fullname)
            .finish_non_exhaustive()
    }
}

impl Advertisement {
    /// Advertise `nameplate` at `ip:port`.
    ///
    /// # Errors
    /// [`LinkError::Io`] when the mDNS daemon cannot start (no multicast-capable interface).
    pub fn start(nameplate: u16, ip: IpAddr, port: u16) -> Result<Self, LinkError> {
        let daemon = ServiceDaemon::new().map_err(mdns_err)?;
        let tag = hex(&random_bytes::<4>()?);
        let instance = format!("borealis-{nameplate}-{tag}");
        let host = format!("borealis-{tag}.local.");
        let np = nameplate.to_string();
        let props = [("v", TXT_VERSION), ("np", np.as_str())];
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, ip, port, &props[..])
            .map_err(mdns_err)?;
        let fullname = info.get_fullname().to_string();
        daemon.register(info).map_err(mdns_err)?;
        Ok(Self { daemon, fullname })
    }
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Browse for `nameplate` for up to `timeout`; the first resolved listener's address.
///
/// # Errors
/// [`LinkError::Io`] when browsing cannot start.
pub fn find(nameplate: u16, timeout: Duration) -> Result<Option<SocketAddr>, LinkError> {
    let daemon = ServiceDaemon::new().map_err(mdns_err)?;
    let rx = daemon.browse(SERVICE_TYPE).map_err(mdns_err)?;
    let deadline = Instant::now() + timeout;
    let want = nameplate.to_string();
    let mut found = None;
    while found.is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(event) = rx.recv_timeout(left) else {
            break;
        };
        let ServiceEvent::ServiceResolved(svc) = event else {
            continue;
        };
        let matches = svc.get_property_val_str("v") == Some(TXT_VERSION)
            && svc.get_property_val_str("np") == Some(want.as_str());
        if matches {
            found = svc
                .get_addresses_v4()
                .into_iter()
                .min()
                .map(|ip| SocketAddr::new(IpAddr::V4(ip), svc.get_port()));
        }
    }
    let _ = daemon.stop_browse(SERVICE_TYPE);
    let _ = daemon.shutdown();
    Ok(found)
}
