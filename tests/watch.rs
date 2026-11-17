//! The real binary in watch mode and with `--tree`, observed through its output.
//! Fixtures are native Bash on loopback; nothing leaves this machine.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(15);

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Only children created by this test are ever signalled.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn netboard(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_netboard"));
    command.args(args);
    command
}

fn run(args: &[&str]) -> Output {
    netboard(args).output().expect("run netboard CLI")
}

fn wait_until(label: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + LIMIT;
    while !condition() {
        assert!(Instant::now() < deadline, "FAIL: {label} timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let mut accepted = None;
    wait_until("fixture connection", || {
        accepted = listener.accept().ok().map(|(stream, _)| stream);
        accepted.is_some()
    });
    let stream = accepted.unwrap();
    stream.set_nonblocking(false).unwrap();
    stream
}

/// Value of the first `"name":` in a record. Enough for the flat fields
/// these tests read; nested matches use the encoder's exact text instead.
fn field<'a>(line: &'a str, name: &str) -> &'a str {
    let key = format!("\"{name}\":");
    let Some(at) = line.find(&key) else {
        return "";
    };
    line[at + key.len()..]
        .split([',', '}'])
        .next()
        .unwrap_or("")
        .trim_matches('"')
}

/// A running watch whose stdout lines arrive on a channel.
struct Watcher {
    child: OwnedChild,
    lines: Receiver<String>,
    seen: Vec<String>,
}

impl Watcher {
    fn start(args: &[&str]) -> Self {
        let mut child = OwnedChild(
            netboard(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start NetViewer watch"),
        );
        let stdout = child.0.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            seen: Vec::new(),
        }
    }

    /// First record, already seen or still to come, that satisfies `wanted`.
    fn wait_for(&mut self, label: &str, wanted: impl Fn(&str) -> bool) -> String {
        if let Some(line) = self.seen.iter().find(|line| wanted(line)) {
            return line.clone();
        }
        let deadline = Instant::now() + LIMIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let hit = wanted(&line);
                    self.seen.push(line.clone());
                    if hit {
                        return line;
                    }
                }
                // Timed out, or the watch exited without writing it.
                Err(_) => panic!("FAIL: {label} not seen in {:#?}", self.seen),
            }
        }
    }

    /// Wait for the process to end; returns its exit code and stderr text.
    fn finish(mut self) -> (Option<i32>, String, Vec<String>) {
        wait_until("watch exit", || {
            self.child.0.try_wait().expect("poll watch").is_some()
        });
        let status = self.child.0.wait().unwrap();
        let mut stderr = String::new();
        self.child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(2)) {
            self.seen.push(line);
        }
        (status.code(), stderr, std::mem::take(&mut self.seen))
    }
}

fn is(line: &str, kind: &str, event: &str) -> bool {
    field(line, "type") == kind && field(line, "event") == event
}

/// The `"local":{...}` object of an endpoint record, as written.
fn local(line: &str) -> &str {
    let from = line.find("\"local\":").expect("local address");
    let to = line.find(",\"remote\":").expect("remote address");
    &line[from..to]
}

#[test]
fn watch_arguments_are_checked_before_anything_is_sampled() {
    for (args, expected) in [
        (
            vec!["--cli", "--watch", "-c"],
            "--watch writes JSON Lines; -c applies to snapshots only",
        ),
        (
            vec!["--cli", "--interval", "500"],
            "--interval and --for need --watch",
        ),
        (
            vec!["--watch", "--interval", "5"],
            "--interval needs milliseconds from 50 to 60000",
        ),
        (
            vec!["--watch", "--for"],
            "--for needs seconds from 1 to 86400",
        ),
        (
            vec!["--cli", "--tree", "bash"],
            "--tree needs a PID from 1 to 2147483647",
        ),
        // Darwin never assigns a PID this high, so no identity can be read.
        (
            vec!["--watch", "--tree", "2147483646"],
            "--tree 2147483646: no such process, or it belongs to another user",
        ),
        (
            vec!["--cli", "-n", "--tree", "2147483646"],
            "--tree 2147483646: no such process, or it belongs to another user",
        ),
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim_end(),
            expected
        );
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn help_lists_the_watch_and_tree_flags() {
    let output = run(&["--watch", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stderr).unwrap();
    for text in [
        "netboard --cli --watch [--interval MS] [--for SECONDS] [process|pid]",
        "  --tree PID ",
        "  --peers ",
        "  --watch ",
        "  --interval MS ",
        "  --for SECONDS ",
    ] {
        assert!(help.contains(text), "{text:?} missing from {help}");
    }
}

#[test]
fn bounded_watch_of_nothing_is_a_start_and_an_end_record() {
    // Darwin PIDs are signed 32-bit; this filter cannot expose real process data.
    let output = run(&["--watch", "--interval", "100", "--for", "1", "4294967295"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].starts_with("{\"type\":\"start\",\"schema\":\"netboard.watch/1\","));
    assert!(lines[0].contains("\"interval_ms\":100,\"duration_s\":1,\"roots\":[],"));
    assert!(lines[0].contains("\"filter\":{\"pid\":4294967295}"));
    assert!(lines[0].ends_with(
        "\"untrusted_fields\":[\"process.name\",\"process.path\",\
         \"peer.processes.name\",\"peer.processes.path\"]}"
    ));
    assert!(lines[1].starts_with("{\"type\":\"baseline\","));
    assert!(lines[1].ends_with("\"endpoints\":0,\"processes\":null}"));
    assert!(lines[2].starts_with("{\"type\":\"end\","));
    assert_eq!(field(lines[2], "reason"), "duration");
    assert_eq!(field(lines[2], "failed_samples"), "0");
    let samples: u64 = field(lines[2], "samples").parse().unwrap();
    // First and last sample always happen; slow ones in between may be skipped.
    assert!((2..=11).contains(&samples), "{samples} samples");
    let elapsed: u64 = field(lines[2], "elapsed_ms").parse().unwrap();
    assert!((1000..15_000).contains(&elapsed), "{elapsed} ms");
}

#[test]
fn watch_reports_a_connection_opening_and_closing_then_stops_on_sigterm() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind loopback fixture");
    let remote = listener.local_addr().unwrap();
    let mut bash = OwnedChild(
        Command::new("/bin/bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "set -eu; read -r start; exec 3<>/dev/tcp/\"$1\"/\"$2\"; read -r close; exec 3>&-; read -r finish",
                "netviewer-watch",
                &remote.ip().to_string(),
                &remote.port().to_string(),
            ])
            .env_remove("BASH_ENV")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start native Bash fixture"),
    );
    let pid = bash.0.id();
    let mut input = bash.0.stdin.take().unwrap();

    // The positional filter selects the idle shell; nothing is open yet.
    let mut watch = Watcher::start(&["--cli", "--watch", "--interval", "100", &pid.to_string()]);
    let start = watch.wait_for("start record", |l| field(l, "type") == "start");
    assert!(start.contains(&format!("\"filter\":{{\"pid\":{pid}}}")));
    // Only what happens after the baseline record is reported as a change.
    let baseline = watch.wait_for("baseline record", |l| field(l, "type") == "baseline");
    assert_eq!(field(&baseline, "endpoints"), "0");

    input.write_all(b"start\n").unwrap();
    let peer = format!(
        "\"remote\":{{\"ip\":\"127.0.0.1\",\"port\":{}}}",
        remote.port()
    );
    let opened = watch.wait_for("new endpoint", |l| {
        is(l, "endpoint", "new") && l.contains(&peer)
    });
    assert_eq!(field(&opened, "pid"), pid.to_string());
    assert_eq!(field(&opened, "name"), "bash");
    assert_eq!(field(&opened, "proto"), "tcp");
    assert_eq!(field(&opened, "dir"), "out");
    assert!(opened.contains("\"off_box\":false"));
    // The kernel's start time for the shell: present, RFC 3339, microseconds.
    let started = field(&opened, "start");
    assert_eq!(
        started.len(),
        "2026-01-01T00:00:00.000000Z".len(),
        "{started}"
    );
    assert!(started.ends_with('Z') && started.contains('T'));
    // Not accepted yet, so no socket holds the other end: the peer is the
    // process listening on that loopback port, which is this test.
    let me = std::process::id();
    assert!(
        opened.contains(&format!(
            "\"peer\":{{\"via\":\"listener\",\"process_count\":1,\"processes\":[{{\"pid\":{me},"
        )),
        "{opened}"
    );
    assert!(opened.ends_with("\"in_scope\":true}]}}"), "{opened}");
    let _held = accept(&listener);

    input.write_all(b"close\n").unwrap();
    let closed = watch.wait_for("closed endpoint", |l| {
        is(l, "endpoint", "closed") && l.contains(&peer)
    });
    assert_eq!(field(&closed, "pid"), pid.to_string());
    assert_eq!(
        field(&closed, "start"),
        started,
        "one process, one identity"
    );
    assert_eq!(local(&closed), local(&opened), "the endpoint that opened");
    assert!(local(&opened).starts_with("\"local\":{\"ip\":\"127.0.0.1\",\"port\":"));

    // An unbounded watch ends on SIGTERM and still writes its end record.
    // SAFETY: the target is this test's own child, not yet reaped.
    let sent = unsafe { libc::kill(watch.child.0.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0);
    let end = watch.wait_for("end record", |l| field(l, "type") == "end");
    assert_eq!(field(&end, "reason"), "interrupted");
    assert_eq!(field(&end, "failed_samples"), "0");
    let (code, stderr, lines) = watch.finish();
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(lines.last().unwrap().starts_with("{\"type\":\"end\","));
    // Every endpoint event belongs to the filtered shell.
    for line in lines.iter().filter(|l| field(l, "type") == "endpoint") {
        assert_eq!(field(line, "pid"), pid.to_string(), "{line}");
    }

    input.write_all(b"finish\n").unwrap();
    wait_until("Bash exit", || bash.0.try_wait().unwrap().is_some());
}

#[test]
fn tree_watch_follows_a_reparented_child_until_the_whole_tree_exits() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind loopback fixture");
    let remote = listener.local_addr().unwrap();
    // The root starts one background child and waits. The child connects,
    // waits for a line on that connection, connects again, and exits when
    // the second connection closes. Closing the sockets releases it even
    // if this test fails first.
    let child = "exec 3<>/dev/tcp/\"$1\"/\"$2\"; read -r next <&3; \
                 exec 4<>/dev/tcp/\"$1\"/\"$2\"; read -r fin <&4";
    let root_script = format!(
        "/bin/bash --noprofile --norc -c '{child}' netviewer-child \"$1\" \"$2\" </dev/null & \
         read -r go; exit 0"
    );
    let mut root = OwnedChild(
        Command::new("/bin/bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                &root_script,
                "netviewer-root",
                &remote.ip().to_string(),
                &remote.port().to_string(),
            ])
            .env_remove("BASH_ENV")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start native Bash root"),
    );
    let root_pid = root.0.id();
    let mut go = root.0.stdin.take().unwrap();
    let mut first = accept(&listener);

    let mut watch = Watcher::start(&[
        "--cli",
        "--watch",
        "--interval",
        "100",
        "--for",
        "60",
        "--tree",
        &root_pid.to_string(),
    ]);
    let start = watch.wait_for("start record", |l| field(l, "type") == "start");
    assert!(start.contains(&format!("\"roots\":[{{\"pid\":{root_pid},\"start\":\"")));

    let peer = format!(
        "\"remote\":{{\"ip\":\"127.0.0.1\",\"port\":{}}}",
        remote.port()
    );
    // Open before the watch began, so it is part of the baseline.
    let held = watch.wait_for("child's connection", |l| {
        field(l, "type") == "endpoint" && l.contains(&peer)
    });
    assert_eq!(field(&held, "event"), "existing");
    assert_eq!(field(&held, "state"), "ESTABLISHED");
    // The other end is the socket this test accepted. This process is not
    // in the watched tree, and the record says so.
    let me = std::process::id();
    assert!(
        held.contains(&format!(
            "\"peer\":{{\"via\":\"connection\",\"process_count\":1,\"processes\":[{{\"pid\":{me},"
        )),
        "{held}"
    );
    assert!(held.ends_with("\"in_scope\":false}]}}"), "{held}");
    let child_pid = field(&held, "pid").to_string();
    assert_ne!(child_pid, root_pid.to_string(), "the child owns the socket");
    watch.wait_for("root tracked", |l| {
        is(l, "process", "tracked")
            && field(l, "reason") == "root"
            && field(l, "pid") == root_pid.to_string()
    });
    let joined = watch.wait_for("child tracked", |l| {
        is(l, "process", "tracked") && field(l, "pid") == child_pid
    });
    assert_eq!(field(&joined, "reason"), "descendant");
    assert_eq!(field(&joined, "ppid"), root_pid.to_string());
    assert_eq!(field(&joined, "start"), field(&held, "start"));
    let baseline = watch.wait_for("baseline record", |l| field(l, "type") == "baseline");
    assert!(
        baseline.ends_with("\"endpoints\":1,\"processes\":2}"),
        "{baseline}"
    );

    // The root exits; launchd adopts the child.
    go.write_all(b"go\n").unwrap();
    wait_until("root exit", || root.0.try_wait().unwrap().is_some());
    watch.wait_for("root exit record", |l| {
        is(l, "process", "exited") && field(l, "pid") == root_pid.to_string()
    });

    // The orphan opens a second connection. It is still attributed to the
    // tree, with the identity the child had before it was re-parented.
    first.write_all(b"next\n").unwrap();
    let second = accept(&listener);
    let opened = watch.wait_for("orphan's second connection", |l| {
        is(l, "endpoint", "new") && l.contains(&peer)
    });
    assert_eq!(field(&opened, "pid"), child_pid);
    assert_eq!(field(&opened, "start"), field(&held, "start"));
    assert_ne!(local(&opened), local(&held), "a second local port");
    // Accepted or still queued, the other end is this process either way.
    assert!(
        opened.contains(&format!("\"processes\":[{{\"pid\":{me},")),
        "{opened}"
    );

    // Closing both connections lets the orphan exit; the watch then ends by
    // itself, long before its 60 s limit.
    drop((first, second, listener));
    let end = watch.wait_for("end record", |l| field(l, "type") == "end");
    assert_eq!(field(&end, "reason"), "scope_exited");
    assert_eq!(field(&end, "failed_samples"), "0");
    let (code, stderr, lines) = watch.finish();
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let count = |kind: &str, event: &str| lines.iter().filter(|l| is(l, kind, event)).count();
    assert_eq!(count("process", "tracked"), 2, "{lines:#?}");
    assert_eq!(count("process", "exited"), 2, "{lines:#?}");
    assert_eq!(count("endpoint", "closed"), 2, "{lines:#?}");
    // Nothing outside the tree was reported.
    for line in lines.iter().filter(|l| field(l, "type") == "endpoint") {
        assert_eq!(field(line, "pid"), child_pid, "{line}");
    }
}

#[test]
fn a_closed_reader_ends_the_watch_without_a_panic() {
    let mut child = OwnedChild(
        netboard(&["--watch", "--interval", "100", "--for", "2"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start NetViewer watch"),
    );
    let mut stdout = BufReader::new(child.0.stdout.take().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    assert!(first.starts_with("{\"type\":\"start\","));
    // The watch notices on its next write, at the latest its end record.
    drop(stdout);
    wait_until("watch exit", || child.0.try_wait().unwrap().is_some());
    assert_eq!(child.0.wait().unwrap().code(), Some(0));
    let mut stderr = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn snapshot_tree_includes_this_process_and_its_own_sockets() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind loopback fixture");
    let server = listener.local_addr().unwrap();
    let client = TcpStream::connect_timeout(&server, Duration::from_secs(5)).expect("connect");
    let local = client.local_addr().unwrap();
    let _accepted = accept(&listener);
    let me = std::process::id().to_string();
    let expected = format!(",{me},TCP4,Out,{local},{server},ESTABLISHED,");
    // The CLI is a child of this process, so it is inside the tree it prints.
    wait_until("own connection in the tree snapshot", || {
        let output = run(&["--cli", "-n", "-c", "--tree", &me]);
        assert!(output.status.success());
        let csv = String::from_utf8(output.stdout).unwrap();
        assert!(csv.starts_with("Process,PID,Proto,Dir,Local,Remote,State,Path\n"));
        csv.lines().any(|row| row.contains(&expected))
    });
    // With --peers the row names who holds the other end: this process.
    let with_peers = run(&["--cli", "-n", "-c", "--peers", "--tree", &me]);
    assert!(with_peers.status.success());
    let csv = String::from_utf8(with_peers.stdout).unwrap();
    assert!(csv.starts_with("Process,PID,Proto,Dir,Local,Remote,State,Path,Peer\n"));
    let row = csv
        .lines()
        .find(|row| row.contains(&expected))
        .expect("own connection with peers");
    assert!(row.ends_with(&format!("[{me}]")), "{row}");
    // The same snapshot filtered to a PID outside the tree is empty.
    let outside = run(&["--cli", "-n", "-a", "-c", "--tree", &me, "4294967295"]);
    assert_eq!(
        String::from_utf8(outside.stdout).unwrap(),
        "Process,PID,Proto,Dir,Local,Remote,State,Path\n"
    );
}
