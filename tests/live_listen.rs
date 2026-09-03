use std::net::TcpListener;

use netboard::{snapshot, Proto};

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
