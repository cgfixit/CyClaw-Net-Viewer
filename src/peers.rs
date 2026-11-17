//! Which local process is on the other end of a connection.
//!
//! When both ends of a connection are sockets on this machine, both are in
//! the same snapshot. Matching them names the process a client is really
//! talking to, which matters when that process is a relay: a local proxy,
//! a model server, a helper daemon. Traffic to `127.0.0.1` stays on the
//! machine only as far as that peer.
//!
//! This is a join over one snapshot, not a trace. It says who held the
//! other end when the snapshot was taken.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use crate::snapshot::{Endpoint, EndpointKey, Proto, TcpState};

/// At most this many processes are named for one endpoint.
pub const MAX_PEER_PROCESSES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Via {
    /// A socket with the mirrored address pair: the same connection.
    Connection,
    /// TCP to a loopback port with no accepted socket visible yet: the
    /// process listening on that port.
    Listener,
    /// UDP to a loopback port: the process with an unconnected socket bound
    /// there. Any number of senders can reach such a socket.
    Binding,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Via::Connection => "connection",
            Via::Listener => "listener",
            Via::Binding => "binding",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LocalPeer {
    pub via: Via,
    /// Distinct owners in ascending order, at most `MAX_PEER_PROCESSES`.
    /// More than one means the socket is shared, as with forked workers.
    pub pids: Vec<u32>,
    /// How many distinct owners there are in all.
    pub total: usize,
}

/// IPv4-mapped IPv6 and plain IPv4 are the same peer.
fn canonical(addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(addr.ip().to_canonical(), addr.port())
}

fn has_peer(e: &Endpoint) -> bool {
    !e.key.remote.ip().is_unspecified() && e.key.remote.port() != 0
}

fn owners(mut pids: Vec<u32>, via: Via) -> Option<LocalPeer> {
    pids.sort_unstable();
    pids.dedup();
    let total = pids.len();
    pids.truncate(MAX_PEER_PROCESSES);
    (total > 0).then_some(LocalPeer { via, pids, total })
}

/// The local peer of every endpoint that has one, keyed by endpoint.
///
/// A mirrored address pair is exact for any address. The listener and
/// binding fallbacks apply to loopback destinations only: only there is
/// the destination certain to be this machine.
pub fn local_peers(endpoints: &[Endpoint]) -> HashMap<EndpointKey, LocalPeer> {
    let mut connected: HashMap<(Proto, SocketAddr, SocketAddr), Vec<u32>> = HashMap::new();
    let mut waiting: HashMap<(Proto, u16), Vec<(IpAddr, u32)>> = HashMap::new();
    for e in endpoints {
        let local = canonical(e.key.local);
        if has_peer(e) {
            connected
                .entry((e.key.proto, local, canonical(e.key.remote)))
                .or_default()
                .push(e.key.pid);
        } else if e.key.local.port() != 0
            && (e.key.proto == Proto::Udp || e.state == Some(TcpState::Listen))
        {
            waiting
                .entry((e.key.proto, local.port()))
                .or_default()
                .push((local.ip(), e.key.pid));
        }
    }

    let mut peers = HashMap::new();
    for e in endpoints.iter().filter(|e| has_peer(e)) {
        let (local, remote) = (canonical(e.key.local), canonical(e.key.remote));
        let mirrored = connected
            .get(&(e.key.proto, remote, local))
            .cloned()
            .and_then(|pids| owners(pids, Via::Connection));
        let peer = mirrored.or_else(|| {
            if !remote.ip().is_loopback() {
                return None;
            }
            let bound = waiting.get(&(e.key.proto, remote.port()))?;
            let at = |wanted: &dyn Fn(IpAddr) -> bool| -> Vec<u32> {
                bound
                    .iter()
                    .filter(|(ip, _)| wanted(*ip))
                    .map(|(_, pid)| *pid)
                    .collect()
            };
            // A socket bound to the exact address wins over a wildcard. An
            // IPv6 wildcard also takes IPv4 unless the socket opted out,
            // which a snapshot cannot see.
            let mut pids = at(&|ip| ip == remote.ip());
            if pids.is_empty() {
                pids = at(&|ip| match ip {
                    IpAddr::V4(v4) => v4.is_unspecified() && remote.is_ipv4(),
                    IpAddr::V6(v6) => v6.is_unspecified(),
                });
            }
            let via = match e.key.proto {
                Proto::Tcp => Via::Listener,
                Proto::Udp => Via::Binding,
            };
            owners(pids, via)
        });
        if let Some(peer) = peer {
            peers.insert(e.key.clone(), peer);
        }
    }
    peers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Dir, IpVer};

    fn ep(pid: u32, proto: Proto, local: &str, remote: &str, state: Option<TcpState>) -> Endpoint {
        let local: SocketAddr = local.parse().unwrap();
        Endpoint {
            key: EndpointKey {
                pid,
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
            process: format!("p{pid}"),
            path: String::new(),
        }
    }

    fn tcp(pid: u32, local: &str, remote: &str) -> Endpoint {
        ep(pid, Proto::Tcp, local, remote, Some(TcpState::Established))
    }

    fn listen(pid: u32, local: &str) -> Endpoint {
        let any = if local.starts_with('[') {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        ep(pid, Proto::Tcp, local, any, Some(TcpState::Listen))
    }

    fn peer_of(endpoints: &[Endpoint], index: usize) -> Option<(Via, Vec<u32>, usize)> {
        local_peers(endpoints)
            .get(&endpoints[index].key)
            .map(|p| (p.via, p.pids.clone(), p.total))
    }

    #[test]
    fn both_ends_of_a_loopback_connection_name_each_other() {
        let eps = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:11434"),
            tcp(20, "127.0.0.1:11434", "127.0.0.1:50000"),
            listen(20, "127.0.0.1:11434"),
            tcp(30, "192.0.2.10:50001", "198.51.100.7:443"),
        ];
        assert_eq!(peer_of(&eps, 0), Some((Via::Connection, vec![20], 1)));
        assert_eq!(peer_of(&eps, 1), Some((Via::Connection, vec![10], 1)));
        // A listener has no peer, and neither has a connection that leaves.
        assert_eq!(peer_of(&eps, 2), None);
        assert_eq!(peer_of(&eps, 3), None);
    }

    #[test]
    fn a_mirrored_pair_is_matched_on_any_local_address_and_across_mapped_ipv4() {
        // The machine talking to itself on its LAN address.
        let lan = [
            tcp(10, "192.0.2.10:50000", "192.0.2.10:8080"),
            tcp(20, "192.0.2.10:8080", "192.0.2.10:50000"),
        ];
        assert_eq!(peer_of(&lan, 0), Some((Via::Connection, vec![20], 1)));
        // An IPv6 listener that accepted an IPv4 client.
        let mapped = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:8080"),
            tcp(20, "[::ffff:127.0.0.1]:8080", "[::ffff:127.0.0.1]:50000"),
        ];
        assert_eq!(peer_of(&mapped, 0), Some((Via::Connection, vec![20], 1)));
        assert_eq!(peer_of(&mapped, 1), Some((Via::Connection, vec![10], 1)));
        // Same ports, different protocol: not the same conversation.
        let crossed = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:8080"),
            ep(20, Proto::Udp, "127.0.0.1:8080", "127.0.0.1:50000", None),
        ];
        assert_eq!(peer_of(&crossed, 0), None);
    }

    #[test]
    fn an_unaccepted_loopback_connection_falls_back_to_the_listener() {
        let eps = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:8080"),
            listen(20, "0.0.0.0:8080"),
            tcp(11, "[::1]:50001", "[::1]:9090"),
            listen(21, "[::]:9090"),
            // An IPv6 wildcard listener also takes IPv4.
            tcp(12, "127.0.0.1:50002", "127.0.0.1:9090"),
            // An IPv4 wildcard listener does not take IPv6.
            tcp(13, "[::1]:50003", "[::1]:8080"),
        ];
        assert_eq!(peer_of(&eps, 0), Some((Via::Listener, vec![20], 1)));
        assert_eq!(peer_of(&eps, 2), Some((Via::Listener, vec![21], 1)));
        assert_eq!(peer_of(&eps, 4), Some((Via::Listener, vec![21], 1)));
        assert_eq!(peer_of(&eps, 5), None);
    }

    #[test]
    fn an_exact_binding_wins_over_a_wildcard_and_udp_is_a_binding() {
        let eps = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:8080"),
            listen(20, "0.0.0.0:8080"),
            listen(21, "127.0.0.1:8080"),
            ep(12, Proto::Udp, "127.0.0.1:50001", "127.0.0.1:5353", None),
            ep(22, Proto::Udp, "0.0.0.0:5353", "0.0.0.0:0", None),
        ];
        assert_eq!(peer_of(&eps, 0), Some((Via::Listener, vec![21], 1)));
        assert_eq!(peer_of(&eps, 3), Some((Via::Binding, vec![22], 1)));
        // The unconnected UDP socket itself names nobody.
        assert_eq!(peer_of(&eps, 4), None);
    }

    #[test]
    fn a_listener_is_never_assumed_for_a_destination_off_this_machine() {
        // Something here listens on 443; the client is talking to another host.
        let eps = [
            tcp(10, "192.0.2.10:50000", "198.51.100.7:443"),
            listen(20, "0.0.0.0:443"),
            ep(11, Proto::Udp, "192.0.2.10:50001", "198.51.100.7:443", None),
            ep(21, Proto::Udp, "0.0.0.0:443", "0.0.0.0:0", None),
        ];
        assert_eq!(peer_of(&eps, 0), None);
        assert_eq!(peer_of(&eps, 2), None);
    }

    #[test]
    fn shared_sockets_list_every_owner_up_to_the_cap() {
        let mut eps = vec![tcp(10, "127.0.0.1:50000", "127.0.0.1:8080")];
        // Twelve forked workers share one listening socket; one holds two
        // descriptors for it.
        for pid in (100..112).rev() {
            eps.push(listen(pid, "127.0.0.1:8080"));
        }
        eps.push(listen(100, "127.0.0.1:8080"));
        assert_eq!(
            peer_of(&eps, 0),
            Some((Via::Listener, (100..108).collect(), 12))
        );
        // Parent and child both hold the accepted socket.
        let shared = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:8080"),
            tcp(31, "127.0.0.1:8080", "127.0.0.1:50000"),
            tcp(30, "127.0.0.1:8080", "127.0.0.1:50000"),
        ];
        assert_eq!(
            peer_of(&shared, 0),
            Some((Via::Connection, vec![30, 31], 2))
        );
    }

    #[test]
    fn a_process_talking_to_itself_is_its_own_peer() {
        let eps = [
            tcp(10, "127.0.0.1:50000", "127.0.0.1:8080"),
            tcp(10, "127.0.0.1:8080", "127.0.0.1:50000"),
        ];
        assert_eq!(peer_of(&eps, 0), Some((Via::Connection, vec![10], 1)));
        assert_eq!(peer_of(&eps, 1), Some((Via::Connection, vec![10], 1)));
    }
}
