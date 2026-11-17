use std::collections::HashMap;
use std::io::{self, BufWriter};
use std::net::IpAddr;
use std::ops::RangeInclusive;
use std::time::Duration;

use crate::dns::{lookup_ip, Resolved};
use crate::procinfo::{self, ProcIdentity};
use crate::proctree::Tracker;
use crate::snapshot::{
    csv_escape, fmt_addr, sanitize_clipboard_text, snapshot, Endpoint, TcpState,
};
use crate::watch;

pub struct CliArgs {
    pub all: bool,
    pub csv: bool,
    pub numeric: bool,
    pub filter: Option<String>,
    pub watch: bool,
    pub interval_ms: Option<u64>,
    pub duration_s: Option<u64>,
    /// Root PIDs: output is narrowed to these processes and their descendants.
    pub tree: Vec<u32>,
}

/// The optional `process|pid` argument: all digits selects one PID,
/// anything else is a case-insensitive substring of the process name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Filter {
    Pid(u32),
    Name(String),
}

impl Filter {
    pub fn parse(text: &str) -> Self {
        match text.parse::<u32>() {
            Ok(pid) => Filter::Pid(pid),
            Err(_) => Filter::Name(text.to_ascii_lowercase()),
        }
    }

    pub fn matches(&self, e: &Endpoint) -> bool {
        match self {
            Filter::Pid(pid) => e.key.pid == *pid,
            Filter::Name(name) => e.process.to_ascii_lowercase().contains(name),
        }
    }
}

/// Returns Ok(true) if CLI handled the process (including --help in CLI mode).
pub fn dispatch(args: &[String]) -> Result<bool, String> {
    // `--watch` has no GUI meaning, so it selects the CLI by itself.
    if !args.iter().any(|a| a == "--cli" || a == "--watch") {
        if args.iter().any(|a| a == "-h" || a == "--help") {
            print_help();
            return Ok(true);
        }
        return Ok(false);
    }
    let parsed = parse(args)?;
    run(&parsed)
}

fn parse(args: &[String]) -> Result<CliArgs, String> {
    let mut parsed = CliArgs {
        all: false,
        csv: false,
        numeric: false,
        filter: None,
        watch: false,
        interval_ms: None,
        duration_s: None,
        tree: Vec::new(),
    };
    let mut rest = args.iter().skip(1);
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--cli" => {}
            "-a" => parsed.all = true,
            "-c" => parsed.csv = true,
            "-n" => parsed.numeric = true,
            "--watch" => parsed.watch = true,
            "--interval" => {
                let range = watch::MIN_INTERVAL_MS..=watch::MAX_INTERVAL_MS;
                parsed.interval_ms = Some(number(a, rest.next(), "milliseconds", range)?);
            }
            "--for" => {
                let range = 1..=watch::MAX_DURATION_S;
                parsed.duration_s = Some(number(a, rest.next(), "seconds", range)?);
            }
            "--tree" => {
                // Darwin PIDs are positive signed 32-bit values.
                let pid = number(a, rest.next(), "a PID", 1..=i32::MAX as u64)? as u32;
                if !parsed.tree.contains(&pid) {
                    parsed.tree.push(pid);
                }
                if parsed.tree.len() > watch::MAX_ROOTS {
                    return Err(format!("at most {} --tree roots", watch::MAX_ROOTS));
                }
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            s if s.starts_with('-') => return Err(format!("unknown flag: {s}")),
            s => {
                if parsed.filter.is_some() {
                    return Err("only one process/PID filter allowed".into());
                }
                parsed.filter = Some(s.to_string());
            }
        }
    }
    if !parsed.watch && (parsed.interval_ms.is_some() || parsed.duration_s.is_some()) {
        return Err("--interval and --for need --watch".into());
    }
    if parsed.watch && parsed.csv {
        return Err("--watch writes JSON Lines; -c applies to snapshots only".into());
    }
    Ok(parsed)
}

/// The value after a flag. The message names the flag and the accepted
/// range, never the rejected text.
fn number(
    flag: &str,
    raw: Option<&String>,
    unit: &str,
    range: RangeInclusive<u64>,
) -> Result<u64, String> {
    raw.and_then(|text| text.parse::<u64>().ok())
        .filter(|value| range.contains(value))
        .ok_or_else(|| {
            format!(
                "{flag} needs {unit} from {} to {}",
                range.start(),
                range.end()
            )
        })
}

/// A root must be identified before anything is scoped to it: narrowing to
/// a process that cannot be found would otherwise print nothing and look
/// like a quiet process.
fn resolve_roots(
    pids: &[u32],
    identify: impl Fn(u32) -> Option<ProcIdentity>,
) -> Result<Vec<ProcIdentity>, String> {
    pids.iter()
        .map(|&pid| {
            identify(pid).ok_or_else(|| {
                format!("--tree {pid}: no such process, or it belongs to another user")
            })
        })
        .collect()
}

fn run(args: &CliArgs) -> Result<bool, String> {
    let roots = resolve_roots(&args.tree, procinfo::identity)?;
    let filter = args.filter.as_deref().map(Filter::parse);
    if args.watch {
        return run_watch(args, roots, filter);
    }

    let mut eps = snapshot().map_err(|e| e.0)?;
    if !roots.is_empty() {
        // One scan: members are the roots and whatever descends from them
        // right now. Only a watch can keep a child that is re-parented later.
        let mut tree = Tracker::new(&roots);
        tree.update(&procinfo::table()?);
        eps.retain(|e| tree.contains(e.key.pid));
    }
    if !args.all {
        eps.retain(|e| e.state == Some(TcpState::Established));
    }
    if let Some(filter) = &filter {
        eps.retain(|e| filter.matches(e));
    }

    print_rows(&eps, args.csv, args.numeric, lookup_ip);
    Ok(true)
}

/// Watch output is every endpoint, numeric, as JSON Lines: `-a` and `-n`
/// are already implied. Exit 0 means every sample succeeded.
fn run_watch(
    args: &CliArgs,
    roots: Vec<ProcIdentity>,
    filter: Option<Filter>,
) -> Result<bool, String> {
    let cfg = watch::Config {
        interval: Duration::from_millis(args.interval_ms.unwrap_or(watch::DEFAULT_INTERVAL_MS)),
        duration: args.duration_s.map(Duration::from_secs),
        roots,
        filter,
    };
    let mut source = watch::Live::new();
    let mut out = BufWriter::new(io::stdout().lock());
    match watch::run(&cfg, &mut source, &mut out) {
        Ok(summary) if summary.failed == 0 => Ok(true),
        Ok(summary) => Err(format!(
            "watch incomplete: {} of {} samples failed",
            summary.failed,
            summary.samples + summary.failed
        )),
        // The reader closed the pipe: nothing left to report to.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(true),
        Err(e) => Err(format!("watch output failed: {e}")),
    }
}

fn print_rows(
    eps: &[Endpoint],
    csv: bool,
    numeric: bool,
    mut lookup: impl FnMut(IpAddr) -> Option<Resolved>,
) {
    let mut cache = HashMap::new();
    let mut name = |ip: IpAddr| {
        if numeric || ip.is_unspecified() {
            None
        } else {
            // Cache misses as well as answers for this snapshot only. Repeated
            // endpoints must not repeat synchronous PTR lookups.
            cache.entry(ip).or_insert_with(|| lookup(ip)).clone()
        }
    };

    if csv {
        println!("Process,PID,Proto,Dir,Local,Remote,State,Path");
    }
    for e in eps {
        print_row(e, csv, &mut name);
    }
}

fn print_row(e: &Endpoint, csv: bool, name: &mut dyn FnMut(IpAddr) -> Option<Resolved>) {
    let (local, remote) = format_endpoints(e, name);
    if csv {
        println!(
            "{},{},{},{},{},{},{},{}",
            csv_escape(&e.process),
            e.key.pid,
            e.proto_label(),
            e.dir_label(),
            csv_escape(&local),
            csv_escape(&remote),
            e.state_label(),
            csv_escape(&e.path)
        );
    } else {
        println!("{}", text_row(e, &local, &remote));
    }
}

/// Fixed-width row. A newline or bidi override in an attacker-chosen process
/// name or path must not forge extra rows or reorder columns in a terminal.
fn text_row(e: &Endpoint, local: &str, remote: &str) -> String {
    format!(
        "{:<18} {:>6} {:<5} {:<6} {:<40} {:<40} {:<13} {}",
        trunc(&sanitize_clipboard_text(&e.process), 18),
        e.key.pid,
        e.proto_label(),
        e.dir_label(),
        trunc(local, 40),
        trunc(remote, 40),
        e.state_label(),
        sanitize_clipboard_text(&e.path)
    )
}

fn format_endpoints(
    e: &Endpoint,
    name: &mut dyn FnMut(IpAddr) -> Option<Resolved>,
) -> (String, String) {
    let mut fmt = |addr: std::net::SocketAddr| {
        let r = name(addr.ip());
        fmt_addr(addr, r.as_ref().map(|x| x.host.as_str()))
    };
    (fmt(e.key.local), fmt(e.key.remote))
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let take = s.chars().take(n.saturating_sub(1)).collect::<String>();
        format!("{take}…")
    }
}

fn print_help() {
    eprintln!(
        "CyClaw-Net-Viewer — Darwin TCP/UDP endpoint viewer\n\
         \n\
         GUI:    netboard\n\
         CLI:    netboard --cli [-a] [-c] [-n] [process|pid]\n\
         Watch:  netboard --cli --watch [--interval MS] [--for SECONDS] [process|pid]\n\
         \n\
         \x20 -a             all endpoints (default: ESTABLISHED TCP)\n\
         \x20 -c             CSV\n\
         \x20 -n             numeric addresses (no reverse DNS)\n\
         \x20 --tree PID     only this process and its descendants; repeat for more roots\n\
         \x20 --watch        JSON Lines events until interrupted: every endpoint, never DNS\n\
         \x20 --interval MS  time between watch samples, 50 to 60000 (default 1000)\n\
         \x20 --for SECONDS  end the watch after this long, 1 to 86400\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn defaults_keep_established_only_with_name_resolution() {
        let parsed = parse(&args(&["netboard", "--cli"])).unwrap();
        assert!(!parsed.all);
        assert!(!parsed.csv);
        assert!(!parsed.numeric);
        assert_eq!(parsed.filter, None);
        assert!(!parsed.watch && parsed.tree.is_empty());
        assert_eq!((parsed.interval_ms, parsed.duration_s), (None, None));
    }

    #[test]
    fn watch_flags_take_bounded_values_and_roots_repeat() {
        let parsed = parse(&args(&[
            "netboard",
            "--watch",
            "--interval",
            "250",
            "--tree",
            "42",
            "--for",
            "30",
            "--tree",
            "7",
            "--tree",
            "42",
            "curl",
        ]))
        .unwrap();
        assert!(parsed.watch);
        assert_eq!(parsed.interval_ms, Some(250));
        assert_eq!(parsed.duration_s, Some(30));
        assert_eq!(parsed.tree, [42, 7], "a repeated root is kept once");
        assert_eq!(parsed.filter.as_deref(), Some("curl"));
        // A snapshot can be narrowed to a tree as well.
        let snapshot = parse(&args(&["netboard", "--cli", "-a", "--tree", "42"])).unwrap();
        assert!(!snapshot.watch);
        assert_eq!(snapshot.tree, [42]);
        // Limits are inclusive.
        for (flag, low, high) in [("--interval", "50", "60000"), ("--for", "1", "86400")] {
            for value in [low, high] {
                assert!(parse(&args(&["netboard", "--watch", flag, value])).is_ok());
            }
        }
    }

    #[test]
    fn watch_flag_errors_name_the_flag_and_range_not_the_input() {
        for (values, expected) in [
            (
                vec!["--watch", "--interval"],
                "--interval needs milliseconds from 50 to 60000",
            ),
            (
                vec!["--watch", "--interval", "49"],
                "--interval needs milliseconds from 50 to 60000",
            ),
            (
                vec!["--watch", "--interval", "60001"],
                "--interval needs milliseconds from 50 to 60000",
            ),
            (
                vec!["--watch", "--interval", "1s"],
                "--interval needs milliseconds from 50 to 60000",
            ),
            (
                vec!["--watch", "--for", "0"],
                "--for needs seconds from 1 to 86400",
            ),
            (
                vec!["--watch", "--for", "-5"],
                "--for needs seconds from 1 to 86400",
            ),
            (
                vec!["--cli", "--tree", "0"],
                "--tree needs a PID from 1 to 2147483647",
            ),
            (
                vec!["--cli", "--tree", "2147483648"],
                "--tree needs a PID from 1 to 2147483647",
            ),
            (
                vec!["--cli", "--tree", "bash"],
                "--tree needs a PID from 1 to 2147483647",
            ),
            (
                vec!["--cli", "--interval", "500"],
                "--interval and --for need --watch",
            ),
            (
                vec!["--cli", "--for", "5"],
                "--interval and --for need --watch",
            ),
            (
                vec!["--watch", "-c"],
                "--watch writes JSON Lines; -c applies to snapshots only",
            ),
        ] {
            let mut full = vec!["netboard"];
            full.extend(&values);
            assert_eq!(parse(&args(&full)).err().as_deref(), Some(expected));
        }
        let mut many = vec!["netboard".to_string(), "--cli".to_string()];
        for pid in 1..=watch::MAX_ROOTS + 1 {
            many.extend(["--tree".to_string(), pid.to_string()]);
        }
        assert_eq!(
            parse(&many).err().as_deref(),
            Some("at most 16 --tree roots")
        );
    }

    #[test]
    fn watch_alone_selects_the_cli_and_rejects_bad_flags_before_sampling() {
        // Without --cli the GUI would open; --watch must never do that.
        assert_eq!(
            dispatch(&args(&["netboard", "--watch", "--bogus"])),
            Err("unknown flag: --bogus".into())
        );
        assert_eq!(dispatch(&args(&["netboard", "--bogus"])), Ok(false));
    }

    #[test]
    fn filter_is_a_pid_when_numeric_and_a_folded_name_otherwise() {
        assert_eq!(Filter::parse("42"), Filter::Pid(42));
        assert_eq!(Filter::parse("Example"), Filter::Name("example".into()));
        // Too large for a PID: treated as a name, which matches nothing real.
        assert_eq!(
            Filter::parse("99999999999"),
            Filter::Name("99999999999".into())
        );
        let e = endpoint("127.0.0.1:50000", "192.0.2.1:443");
        assert!(Filter::Pid(42).matches(&e) && !Filter::Pid(43).matches(&e));
        assert!(Filter::parse("XAMP").matches(&e) && !Filter::parse("curl").matches(&e));
    }

    #[test]
    fn every_tree_root_must_be_identified() {
        let known = |pid: u32| {
            (pid == 42).then_some(ProcIdentity {
                pid,
                ppid: 1,
                start: crate::procinfo::ProcStart { sec: 9, usec: 0 },
            })
        };
        assert_eq!(resolve_roots(&[], known), Ok(Vec::new()));
        assert_eq!(resolve_roots(&[42], known).unwrap()[0].pid, 42);
        assert_eq!(
            resolve_roots(&[42, 77], known),
            Err("--tree 77: no such process, or it belongs to another user".into())
        );
    }

    #[test]
    fn flags_can_surround_a_single_process_or_pid_filter() {
        for filter in ["Example Process", "12345"] {
            let parsed = parse(&args(&["netboard", "--cli", "-n", filter, "-c", "-a"])).unwrap();
            assert!(parsed.all && parsed.csv && parsed.numeric);
            assert_eq!(parsed.filter.as_deref(), Some(filter));
        }
    }

    #[test]
    fn unknown_flags_and_multiple_filters_are_rejected() {
        for values in [
            vec!["netboard", "--cli", "--bogus"],
            vec!["netboard", "--cli", "-can"],
            vec!["netboard", "--cli", "first", "second"],
        ] {
            assert!(parse(&args(&values)).is_err());
        }
    }

    #[test]
    fn text_truncation_counts_unicode_characters_without_splitting_utf8() {
        assert_eq!(trunc("", 18), "");
        assert_eq!(trunc("café", 4), "café");
        assert_eq!(trunc("🦀网络工具", 4), "🦀网络…");
        assert_eq!(trunc("ab", 1), "…");
    }

    fn endpoint(local: &str, remote: &str) -> Endpoint {
        Endpoint {
            key: crate::EndpointKey {
                pid: 42,
                proto: crate::Proto::Tcp,
                ip_ver: if local.parse::<std::net::SocketAddr>().unwrap().is_ipv4() {
                    crate::IpVer::V4
                } else {
                    crate::IpVer::V6
                },
                local: local.parse().unwrap(),
                remote: remote.parse().unwrap(),
            },
            state: Some(TcpState::Established),
            dir: crate::Dir::Out,
            process: "example".into(),
            path: String::new(),
        }
    }

    #[test]
    fn repeated_endpoints_resolve_each_ip_once_per_snapshot() {
        let mut eps = Vec::new();
        for port in 50000..51000 {
            eps.push(endpoint(&format!("127.0.0.1:{port}"), "192.0.2.1:443"));
        }
        // The same IP can also appear on opposite sides of different rows.
        eps.push(endpoint("192.0.2.1:50000", "127.0.0.1:443"));
        for csv in [false, true] {
            for answer in [
                None,
                Some(Resolved {
                    host: "example.test".into(),
                }),
            ] {
                let mut calls = Vec::new();
                print_rows(&eps, csv, false, |ip| {
                    calls.push(ip);
                    answer.clone()
                });
                assert_eq!(calls, [eps[0].key.local.ip(), eps[0].key.remote.ip()]);
            }
        }
    }

    #[test]
    fn text_rows_cannot_be_forged_by_process_names_or_paths() {
        let mut e = endpoint("127.0.0.1:50000", "192.0.2.1:443");
        e.process = "evil\nFAKE 1 TCP4".into();
        e.path = "/tmp/\u{202E}gpj.app\r\n".into();
        let row = text_row(&e, "127.0.0.1:50000", "192.0.2.1:443");
        assert!(!row.contains(['\n', '\r', '\u{202E}']), "{row:?}");
        assert!(row.starts_with("evil FAKE 1 TCP4"));
        assert!(row.ends_with("/tmp/ gpj.app  "));
    }

    #[test]
    fn same_process_same_port_peers_keep_distinct_addresses() {
        let eps = [
            endpoint("127.0.0.1:50000", "192.0.2.1:443"),
            endpoint("[::1]:50001", "[2001:db8::1]:443"),
        ];
        for host in [None, Some("untrusted.example")] {
            let mut name = |_| host.map(|host: &str| Resolved { host: host.into() });
            let v4 = format_endpoints(&eps[0], &mut name).1;
            let v6 = format_endpoints(&eps[1], &mut name).1;
            assert!(v4.contains("192.0.2.1"));
            assert!(v6.contains("[2001:db8::1]"));
            assert!(!v6.contains("192.0.2.1"));
        }
    }

    #[test]
    fn numeric_and_unspecified_addresses_never_call_lookup() {
        for csv in [false, true] {
            print_rows(
                &[
                    endpoint("127.0.0.1:50000", "192.0.2.1:443"),
                    endpoint("[::1]:50001", "[2001:db8::1]:443"),
                ],
                csv,
                true,
                |_| panic!("numeric output must not resolve names"),
            );
            print_rows(&[endpoint("0.0.0.0:0", "[::]:0")], csv, false, |_| {
                panic!("unspecified addresses must not resolve names")
            });
        }
    }
}
