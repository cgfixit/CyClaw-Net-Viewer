use std::net::{TcpListener, TcpStream};

use netboard::{snapshot, Proto};

#[test]
fn connected_tcp_pair_has_both_established_endpoints_and_directions() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let server_addr = listener.local_addr().unwrap();
    let client = TcpStream::connect_timeout(&server_addr, std::time::Duration::from_secs(5))
        .expect("connect loopback");
    let client_addr = client.local_addr().unwrap();
    // A connected client is sufficient: the established server socket is in the
    // accept queue. Accept nonblocking so a regression cannot hang the test.
    listener.set_nonblocking(true).unwrap();
    let (server, _) = listener
        .accept()
        .expect("accept queued loopback connection");
    let pid = std::process::id();
    for attempt in 0..20 {
        let endpoints = snapshot().expect("snapshot");
        let found = [
            (client_addr, server_addr, netboard::Dir::Out),
            (server_addr, client_addr, netboard::Dir::In),
        ]
        .iter()
        .all(|(local, remote, direction)| {
            endpoints.iter().any(|e| {
                e.key.pid == pid
                    && e.key.proto == Proto::Tcp
                    && e.key.local == *local
                    && e.key.remote == *remote
                    && e.state == Some(netboard::TcpState::Established)
                    && e.dir == *direction
            })
        });
        if found {
            drop((client, server, listener));
            return;
        }
        assert!(
            attempt < 19,
            "loopback TCP pair missing or incorrectly classified"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Poll the live snapshot until `found` accepts it, a second at most.
fn eventually(label: &str, mut found: impl FnMut(&[netboard::Endpoint]) -> bool) {
    for attempt in 0..20 {
        if found(&snapshot().expect("snapshot")) {
            return;
        }
        assert!(attempt < 19, "{label}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn connected_udp_reports_its_peer_and_direction_stays_unknown() {
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind receiver");
    let client = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind client");
    let receiver_addr = receiver.local_addr().unwrap();
    client.connect(receiver_addr).expect("connect UDP");
    let client_addr = client.local_addr().unwrap();
    let pid = std::process::id();
    eventually("UDP endpoints missing or incorrectly reported", |eps| {
        let udp = |local| {
            eps.iter()
                .find(|e| e.key.proto == Proto::Udp && e.key.pid == pid && e.key.local == local)
        };
        // The client connected, so the kernel knows its peer. Nothing says
        // which way packets flow: direction stays Unknown for both.
        let connected = udp(client_addr)
            .is_some_and(|e| e.key.remote == receiver_addr && e.dir == netboard::Dir::Unknown);
        // The receiver never connected: it has no peer.
        let bound = udp(receiver_addr).is_some_and(|e| {
            e.key.remote.ip().is_unspecified()
                && e.key.remote.port() == 0
                && e.dir == netboard::Dir::Unknown
        });
        connected && bound
    });
}

#[test]
fn ipv6_loopback_tcp_and_udp_report_native_addresses() {
    let listener = TcpListener::bind("[::1]:0").expect("bind IPv6 loopback");
    let server_addr = listener.local_addr().unwrap();
    let client = TcpStream::connect_timeout(&server_addr, std::time::Duration::from_secs(5))
        .expect("connect IPv6 loopback");
    let client_addr = client.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let (_server, _) = listener.accept().expect("accept queued connection");
    let receiver = std::net::UdpSocket::bind("[::1]:0").expect("bind IPv6 receiver");
    let sender = std::net::UdpSocket::bind("[::1]:0").expect("bind IPv6 sender");
    let receiver_addr = receiver.local_addr().unwrap();
    sender
        .connect(receiver_addr)
        .expect("connect UDP over IPv6");
    let sender_addr = sender.local_addr().unwrap();
    let pid = std::process::id();
    eventually("IPv6 loopback endpoints missing or misread", |eps| {
        let has = |proto, local, remote| {
            eps.iter().any(|e| {
                e.key.pid == pid
                    && e.key.proto == proto
                    && e.key.ip_ver == netboard::IpVer::V6
                    && e.key.local == local
                    && e.key.remote == remote
            })
        };
        has(Proto::Tcp, client_addr, server_addr)
            && has(Proto::Tcp, server_addr, client_addr)
            && has(Proto::Udp, sender_addr, receiver_addr)
    });
}

#[test]
fn an_ipv6_listener_reports_its_ipv4_client_as_that_ipv4_address() {
    // A wildcard IPv6 listener also accepts IPv4 on macOS. The accepted
    // socket is an IPv6 socket whose peer is an IPv4 address.
    let listener = TcpListener::bind("[::]:0").expect("bind IPv6 wildcard");
    let port = listener.local_addr().unwrap().port();
    let target = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let client = TcpStream::connect_timeout(&target, std::time::Duration::from_secs(5))
        .expect("connect over IPv4 to the IPv6 listener");
    let client_addr = client.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let (_server, _) = listener.accept().expect("accept queued connection");
    let pid = std::process::id();
    eventually("accepted IPv4-on-IPv6 socket missing or misread", |eps| {
        eps.iter().any(|e| {
            e.key.pid == pid
                && e.key.proto == Proto::Tcp
                && e.key.ip_ver == netboard::IpVer::V6
                && e.key.local.port() == port
                && e.key.remote.port() == client_addr.port()
                && e.key.remote.ip().to_canonical() == client_addr.ip()
                && e.key.local.ip().to_canonical() == target.ip()
                && e.state == Some(netboard::TcpState::Established)
                && !netboard::is_offbox(e.key.remote)
        })
    });
}

#[test]
fn snapshot_sees_bound_listener() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let pid = std::process::id();

    let mut found = false;
    for _ in 0..5 {
        let eps = snapshot().expect("snapshot");
        found = eps
            .iter()
            .any(|e| e.key.proto == Proto::Tcp && e.key.local.port() == port && e.key.pid == pid);
        if found {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        found,
        "bound 127.0.0.1:{port} pid {pid} missing from snapshot"
    );
    drop(listener);
}
