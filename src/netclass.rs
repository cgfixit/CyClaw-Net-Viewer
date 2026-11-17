//! Where an address reaches: this machine, the local network, or beyond.
//!
//! "Off-box" is one bit: not loopback. An egress question needs more than
//! that. A connection to a LAN printer and one to a public host are both
//! off-box, and a listener on `0.0.0.0` is reachable from the network even
//! though it has no peer at all. This module names the class of an address
//! and says when a socket that waits for peers can be reached from off the
//! machine.
//!
//! The classes describe address ranges, not routes. `lan` means a private,
//! link-local or unique-local range, which is usually the local network but
//! can be a VPN. `public` means globally routable unicast.

use std::net::IpAddr;

use crate::snapshot::{Endpoint, Proto, TcpState};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Class {
    /// `0.0.0.0` or `::`. As a local address: every interface.
    Wildcard,
    /// `127.0.0.0/8` and `::1`.
    Loopback,
    /// RFC 1918, IPv4 link-local, IPv6 link-local and unique-local.
    Lan,
    /// IPv4 and IPv6 multicast, and the IPv4 limited broadcast.
    Multicast,
    /// Globally routable unicast.
    Public,
    /// Shared address space (CGNAT, many VPN overlays), documentation,
    /// benchmarking, and reserved ranges.
    Other,
}

impl Class {
    pub const ALL: [Class; 6] = [
        Class::Wildcard,
        Class::Loopback,
        Class::Lan,
        Class::Multicast,
        Class::Public,
        Class::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Wildcard => "wildcard",
            Class::Loopback => "loopback",
            Class::Lan => "lan",
            Class::Multicast => "multicast",
            Class::Public => "public",
            Class::Other => "other",
        }
    }

    pub fn from_word(word: &str) -> Option<Class> {
        Class::ALL.into_iter().find(|class| class.as_str() == word)
    }
}

/// IPv4-mapped IPv6 is the IPv4 address it carries.
pub fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

pub fn class(ip: IpAddr) -> Class {
    match canonical(ip) {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            if v4.is_unspecified() {
                Class::Wildcard
            } else if v4.is_loopback() {
                Class::Loopback
            } else if v4.is_private() || v4.is_link_local() {
                Class::Lan
            } else if v4.is_multicast() || v4.is_broadcast() {
                Class::Multicast
            } else if a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b)) // shared address space
                || (a == 198 && (b == 18 || b == 19)) // benchmarking
                || (a, b, c) == (192, 0, 0) // protocol assignments
                || v4.is_documentation()
            {
                Class::Other
            } else {
                Class::Public
            }
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            if v6.is_unspecified() {
                Class::Wildcard
            } else if v6.is_loopback() {
                Class::Loopback
            } else if s[0] & 0xffc0 == 0xfe80 || s[0] & 0xfe00 == 0xfc00 {
                // Link-local fe80::/10 and unique-local fc00::/7.
                Class::Lan
            } else if s[0] & 0xff00 == 0xff00 {
                Class::Multicast
            } else if s[0] & 0xe000 == 0x2000 && !(s[0] == 0x2001 && s[1] == 0x0db8) {
                // Global unicast 2000::/3, less documentation 2001:db8::/32.
                Class::Public
            } else if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                // NAT64 well-known prefix: an IPv4 host beyond a translator.
                Class::Public
            } else {
                Class::Other
            }
        }
    }
}

/// True when the endpoint has a peer: a connected socket.
pub fn has_peer(e: &Endpoint) -> bool {
    !e.key.remote.ip().is_unspecified() && e.key.remote.port() != 0
}

/// True for a socket that waits for peers: a TCP listener, or an
/// unconnected UDP socket bound to a port. An unconnected UDP socket
/// receives from any sender, even when its owner only uses it to send.
pub fn is_receiver(e: &Endpoint) -> bool {
    if has_peer(e) || e.key.local.port() == 0 {
        return false;
    }
    match e.key.proto {
        Proto::Tcp => e.state == Some(TcpState::Listen),
        Proto::Udp => true,
    }
}

/// The class of the local address when a receiver can be reached from off
/// this machine: bound to a wildcard or to any non-loopback address.
pub fn exposure(e: &Endpoint) -> Option<Class> {
    if !is_receiver(e) {
        return None;
    }
    let bound = class(e.key.local.ip());
    (bound != Class::Loopback).then_some(bound)
}

/// The class of the peer, or `None` when there is no peer.
pub fn remote_class(e: &Endpoint) -> Option<Class> {
    has_peer(e).then(|| class(e.key.remote.ip()))
}

/// One word for the CLI `Class` column: the peer's class for a connection,
/// `exposed-<bind>` for a receiver reachable from off the machine,
/// `local-only` for one bound to loopback, empty for anything else.
pub fn label(e: &Endpoint) -> String {
    if let Some(peer) = remote_class(e) {
        return peer.as_str().into();
    }
    match exposure(e) {
        Some(bound) => format!("exposed-{}", bound.as_str()),
        None if is_receiver(e) => "local-only".into(),
        None => String::new(),
    }
}

/// True when `addr` falls in `net/prefix`. Families must match after
/// IPv4-mapped addresses are unwrapped.
pub fn in_network(addr: IpAddr, net: IpAddr, prefix: u8) -> bool {
    match (canonical(addr), canonical(net)) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let mask = u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0);
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Dir, EndpointKey, IpVer};
    use std::net::SocketAddr;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn ipv4_ranges_fall_in_their_class() {
        for (addr, want) in [
            ("0.0.0.0", Class::Wildcard),
            ("127.0.0.1", Class::Loopback),
            ("127.255.0.9", Class::Loopback),
            ("10.1.2.3", Class::Lan),
            ("172.16.0.1", Class::Lan),
            ("172.31.255.254", Class::Lan),
            ("192.168.1.20", Class::Lan),
            ("169.254.10.10", Class::Lan),
            ("224.0.0.251", Class::Multicast),
            ("239.255.255.250", Class::Multicast),
            ("255.255.255.255", Class::Multicast),
            ("100.64.0.1", Class::Other),
            ("100.127.255.254", Class::Other),
            ("192.0.2.7", Class::Other),
            ("198.18.0.1", Class::Other),
            ("203.0.113.9", Class::Other),
            ("240.0.0.1", Class::Other),
            ("0.1.2.3", Class::Other),
            ("192.0.0.8", Class::Other),
            ("8.8.8.8", Class::Public),
            ("172.32.0.1", Class::Public),
            ("100.128.0.1", Class::Public),
            ("1.1.1.1", Class::Public),
        ] {
            assert_eq!(class(ip(addr)), want, "{addr}");
        }
    }

    #[test]
    fn ipv6_ranges_fall_in_their_class_and_mapped_ipv4_is_ipv4() {
        for (addr, want) in [
            ("::", Class::Wildcard),
            ("::1", Class::Loopback),
            ("fe80::1", Class::Lan),
            ("febf::1", Class::Lan),
            ("fd12:3456::1", Class::Lan),
            ("fc00::1", Class::Lan),
            ("ff02::fb", Class::Multicast),
            ("2606:4700::1111", Class::Public),
            ("2001:db8::1", Class::Other),
            ("64:ff9b::808:808", Class::Public),
            ("fec0::1", Class::Other),
            ("::ffff:127.0.0.1", Class::Loopback),
            ("::ffff:192.168.0.2", Class::Lan),
            ("::ffff:8.8.4.4", Class::Public),
            ("::ffff:0.0.0.0", Class::Wildcard),
        ] {
            assert_eq!(class(ip(addr)), want, "{addr}");
        }
        for class in Class::ALL {
            assert_eq!(Class::from_word(class.as_str()), Some(class));
        }
        assert_eq!(Class::from_word("LAN"), None);
    }

    fn ep(proto: Proto, local: &str, remote: &str, state: Option<TcpState>) -> Endpoint {
        let local: SocketAddr = local.parse().unwrap();
        Endpoint {
            key: EndpointKey {
                pid: 1,
                proto,
                ip_ver: if local.is_ipv4() {
                    IpVer::V4
                } else {
                    IpVer::V6
                },
                local,
                remote: remote.parse().unwrap(),
            },
            state,
            dir: Dir::Unknown,
            process: "p".into(),
            path: String::new(),
        }
    }

    #[test]
    fn listeners_and_bound_udp_are_exposed_unless_bound_to_loopback() {
        let listen = Some(TcpState::Listen);
        let cases = [
            (
                ep(Proto::Tcp, "0.0.0.0:22", "0.0.0.0:0", listen),
                "exposed-wildcard",
            ),
            (
                ep(Proto::Tcp, "[::]:8080", "[::]:0", listen),
                "exposed-wildcard",
            ),
            (
                ep(Proto::Tcp, "192.168.1.5:8080", "0.0.0.0:0", listen),
                "exposed-lan",
            ),
            (
                ep(Proto::Tcp, "127.0.0.1:11434", "0.0.0.0:0", listen),
                "local-only",
            ),
            (
                ep(Proto::Tcp, "[::1]:11434", "[::]:0", listen),
                "local-only",
            ),
            (
                ep(Proto::Udp, "0.0.0.0:5353", "0.0.0.0:0", None),
                "exposed-wildcard",
            ),
            (
                ep(Proto::Udp, "127.0.0.1:53", "0.0.0.0:0", None),
                "local-only",
            ),
            // Connections are classed by their peer, never as exposed.
            (
                ep(
                    Proto::Tcp,
                    "192.168.1.5:50000",
                    "8.8.8.8:443",
                    Some(TcpState::Established),
                ),
                "public",
            ),
            (
                ep(Proto::Udp, "0.0.0.0:50001", "192.168.1.1:53", None),
                "lan",
            ),
            (
                ep(
                    Proto::Tcp,
                    "127.0.0.1:50002",
                    "127.0.0.1:8080",
                    Some(TcpState::Established),
                ),
                "loopback",
            ),
            // Unbound UDP and a TCP socket that is neither listening nor
            // connected wait for nobody.
            (ep(Proto::Udp, "0.0.0.0:0", "0.0.0.0:0", None), ""),
            (
                ep(
                    Proto::Tcp,
                    "0.0.0.0:7000",
                    "0.0.0.0:0",
                    Some(TcpState::Closed),
                ),
                "",
            ),
        ];
        for (e, want) in cases {
            assert_eq!(label(&e), want, "{:?} {:?}", e.key.local, e.key.remote);
            assert_eq!(exposure(&e).is_some(), want.starts_with("exposed-"));
        }
    }

    #[test]
    fn networks_match_by_prefix_within_one_family() {
        assert!(in_network(ip("10.9.8.7"), ip("10.0.0.0"), 8));
        assert!(!in_network(ip("11.0.0.1"), ip("10.0.0.0"), 8));
        assert!(in_network(ip("::ffff:10.0.0.1"), ip("10.0.0.0"), 8));
        assert!(in_network(ip("1.2.3.4"), ip("0.0.0.0"), 0));
        assert!(in_network(ip("192.0.2.7"), ip("192.0.2.7"), 32));
        assert!(!in_network(ip("192.0.2.8"), ip("192.0.2.7"), 32));
        assert!(in_network(ip("2001:db8::5"), ip("2001:db8::"), 32));
        assert!(!in_network(ip("2001:db9::5"), ip("2001:db8::"), 32));
        assert!(in_network(ip("::1"), ip("::1"), 128));
        assert!(!in_network(ip("10.0.0.1"), ip("::"), 0));
    }
}
