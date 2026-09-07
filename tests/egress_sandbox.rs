//! Native Bash traffic observed through both the Darwin collector and public CLI.
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use netboard::{snapshot, Dir, Proto, TcpState};

const LIMIT: Duration = Duration::from_secs(15);

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Only children created by this test are ever signalled.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_until(label: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + LIMIT;
    while !condition() {
        assert!(Instant::now() < deadline, "FAIL: {label} timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn numeric_csv(pid: u32, all: bool) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_netboard"));
    command.args(["--cli", "-n", "-c"]);
    if all {
        command.arg("-a");
    }
    let mut child = OwnedChild(
        command
            .arg(pid.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start numeric NetViewer CLI"),
    );
    wait_until("CLI completion", || {
        child.0.try_wait().expect("poll CLI").is_some()
    });
    assert!(child.0.wait().unwrap().success(), "FAIL: CLI exit status");
    let mut output = String::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    assert!(output.starts_with("Process,PID,Proto,Dir,Local,Remote,State,Path\n"));
    output
}

#[test]
fn bash_egress_is_visible_and_disappears_after_close() {
    // By default nothing leaves this machine. An explicit numeric peer enables
    // a separate operator-controlled LAN exercise, never a public test service.
    let peer = std::env::var("NETVIEWER_EGRESS_PEER").ok();
    let listener = peer.is_none().then(|| {
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind private loopback fixture")
    });
    let remote: SocketAddr = match peer {
        Some(value) => value
            .parse()
            .expect("NETVIEWER_EGRESS_PEER must be numeric IPv4:port"),
        None => listener.as_ref().unwrap().local_addr().unwrap(),
    };
    assert!(remote.is_ipv4() && remote.port() != 0 && !remote.ip().is_unspecified());
    assert!(!remote.ip().is_multicast());

    let mut child = OwnedChild(
        Command::new("/bin/bash")
            .args([
                "--noprofile", "--norc", "-c",
                "set -eu; read -r start; exec 3<>/dev/tcp/\"$1\"/\"$2\"; printf 'netviewer-egress-test\\n' >&3; read -r close; exec 3>&-; read -r finish",
                "netviewer-egress", &remote.ip().to_string(), &remote.port().to_string(),
            ])
            .env_remove("BASH_ENV")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start native Bash fixture"),
    );
    let pid = child.0.id();
    let mut input = child.0.stdin.take().unwrap();
    assert_eq!(
        numeric_csv(pid, true).lines().count(),
        1,
        "FAIL: idle baseline"
    );
    input.write_all(b"start\n").unwrap();

    let mut observed = None;
    wait_until("Bash ESTABLISHED endpoint", || {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "FAIL: Bash exited before observation"
        );
        observed = snapshot()
            .expect("collect live sockets")
            .into_iter()
            .find(|e| {
                e.key.pid == pid
                    && e.key.proto == Proto::Tcp
                    && e.key.remote == remote
                    && e.state == Some(TcpState::Established)
                    && e.dir == Dir::Out
            });
        observed.is_some()
    });
    let endpoint = observed.unwrap();
    let _accepted = listener.map(|listener| {
        listener.set_nonblocking(true).unwrap();
        let (mut stream, _) = listener.accept().expect("accept established fixture");
        stream.set_read_timeout(Some(LIMIT)).unwrap();
        let mut payload = [0; 22];
        stream
            .read_exact(&mut payload)
            .expect("read synthetic traffic");
        assert_eq!(&payload, b"netviewer-egress-test\n");
        stream
    });
    let expected = format!(
        ",{pid},TCP4,Out,{},{remote},ESTABLISHED,",
        endpoint.key.local
    );
    for all in [false, true] {
        assert!(
            numeric_csv(pid, all)
                .lines()
                .any(|row| row.contains(&expected)),
            "FAIL: CLI omitted or misreported the Bash connection"
        );
    }
    println!("PASS: idle baseline, native Bash traffic, collector and numeric CLI attribution");

    input.write_all(b"close\n").unwrap();
    wait_until("closed descriptor removal", || {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "FAIL: Bash exited before close check"
        );
        !snapshot()
            .unwrap()
            .iter()
            .any(|e| e.key.pid == pid && e.key.remote == remote)
    });
    assert_eq!(
        numeric_csv(pid, true).lines().count(),
        1,
        "FAIL: stale CLI endpoint"
    );
    input.write_all(b"finish\n").unwrap();
    wait_until("Bash exit", || child.0.try_wait().unwrap().is_some());
    assert!(child.0.wait().unwrap().success());
    println!("PASS: endpoint removal while owner remains alive, fixture cleanup");
}
