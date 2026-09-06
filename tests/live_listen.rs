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

#[test]
fn bound_and_connected_udp_have_unknown_direction() {
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind receiver");
    let client = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind client");
    client
        .connect(receiver.local_addr().unwrap())
        .expect("connect UDP");
    let ports = [
        receiver.local_addr().unwrap().port(),
        client.local_addr().unwrap().port(),
    ];
    let pid = std::process::id();
    for attempt in 0..5 {
        let endpoints = snapshot().expect("snapshot");
        if ports.iter().all(|port| {
            endpoints.iter().any(|e| {
                e.key.proto == Proto::Udp
                    && e.key.pid == pid
                    && e.key.local.port() == *port
                    && e.dir == netboard::Dir::Unknown
                    && e.key.remote.ip().is_unspecified()
            })
        }) {
            return;
        }
        assert!(
            attempt < 4,
            "UDP endpoints missing or incorrectly classified"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
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
