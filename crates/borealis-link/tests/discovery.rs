#![cfg(feature = "mdns")]
//! mDNS advertise + browse by nameplate on this machine. Needs a multicast-capable interface,
//! so it is opt-in (`cargo test -p borealis-link --test discovery -- --ignored`); CI sandboxes
//! often have none.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use borealis_link::discovery::{find, Advertisement};

#[test]
#[ignore = "needs a multicast-capable network interface"]
fn a_window_is_found_by_its_nameplate() {
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let ad = Advertisement::start(77, ip, 45_123).unwrap();
    let found = find(77, Duration::from_secs(5)).unwrap();
    assert_eq!(found.map(|a| a.port()), Some(45_123));
    assert_eq!(find(78, Duration::from_millis(800)).unwrap(), None);
    drop(ad);
}
