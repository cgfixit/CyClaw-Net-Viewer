use std::collections::HashMap;
use std::net::IpAddr;

use crate::dns::{lookup_ip, Resolved};
use crate::snapshot::{csv_escape, fmt_addr, snapshot, Endpoint, TcpState};

pub struct CliArgs {
    pub all: bool,
    pub csv: bool,
    pub numeric: bool,
    pub filter: Option<String>,
}

/// Returns Ok(true) if CLI handled the process (including --help in CLI mode).
pub fn dispatch(args: &[String]) -> Result<bool, String> {
    if !args.iter().any(|a| a == "--cli") {
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
    let mut all = false;
    let mut csv = false;
    let mut numeric = false;
    let mut filter = None;
    let mut skip_bin = true;
    for a in args {
        if skip_bin {
            skip_bin = false;
            continue;
        }
        match a.as_str() {
            "--cli" => {}
            "-a" => all = true,
            "-c" => csv = true,
            "-n" => numeric = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            s if s.starts_with('-') => return Err(format!("unknown flag: {s}")),
            s => {
                if filter.is_some() {
                    return Err("only one process/PID filter allowed".into());
                }
                filter = Some(s.to_string());
            }
        }
    }
    Ok(CliArgs {
        all,
        csv,
        numeric,
        filter,
    })
}

fn run(args: &CliArgs) -> Result<bool, String> {
    let mut eps = snapshot().map_err(|e| e.0)?;
    if !args.all {
        eps.retain(|e| e.state == Some(TcpState::Established));
    }
    if let Some(f) = &args.filter {
        if let Ok(pid) = f.parse::<u32>() {
            eps.retain(|e| e.key.pid == pid);
        } else {
            let f = f.to_ascii_lowercase();
            eps.retain(|e| e.process.to_ascii_lowercase().contains(&f));
        }
    }

    print_rows(&eps, args.csv, args.numeric, lookup_ip);
    Ok(true)
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
            // endpoints must not repeat synchronous PTR/forward lookups.
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
    let mut fmt = |addr: std::net::SocketAddr| {
        let r = name(addr.ip());
        fmt_addr(
            addr,
            r.as_ref().map(|x| x.host.as_str()),
            r.as_ref().and_then(|x| x.ipv4),
        )
    };
    let local = fmt(e.key.local);
    let remote = fmt(e.key.remote);
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
        println!(
            "{:<18} {:>6} {:<5} {:<6} {:<40} {:<40} {:<13} {}",
            trunc(&e.process, 18),
            e.key.pid,
            e.proto_label(),
            e.dir_label(),
            trunc(&local, 40),
            trunc(&remote, 40),
            e.state_label(),
            e.path
        );
    }
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
         GUI:  cyclaw-net-viewer\n\
         CLI:  cyclaw-net-viewer --cli [-a] [-c] [-n] [process|pid]\n\
         \n\
           -a    all endpoints (default: ESTABLISHED TCP)\n\
           -c    CSV\n\
           -n    numeric addresses (no reverse DNS)\n"
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
        let parsed = parse(&args(&["cyclaw-net-viewer", "--cli"])).unwrap();
        assert!(!parsed.all);
        assert!(!parsed.csv);
        assert!(!parsed.numeric);
        assert_eq!(parsed.filter, None);
    }

    #[test]
    fn flags_can_surround_a_single_process_or_pid_filter() {
        for filter in ["Example Process", "12345"] {
            let parsed = parse(&args(&[
                "cyclaw-net-viewer",
                "--cli",
                "-n",
                filter,
                "-c",
                "-a",
            ]))
            .unwrap();
            assert!(parsed.all && parsed.csv && parsed.numeric);
            assert_eq!(parsed.filter.as_deref(), Some(filter));
        }
    }

    #[test]
    fn unknown_flags_and_multiple_filters_are_rejected() {
        for values in [
            vec!["cyclaw-net-viewer", "--cli", "--bogus"],
            vec!["cyclaw-net-viewer", "--cli", "-can"],
            vec!["cyclaw-net-viewer", "--cli", "first", "second"],
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
                ip_ver: crate::IpVer::V4,
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
                    ipv4: None,
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
    fn numeric_and_unspecified_addresses_never_call_lookup() {
        for csv in [false, true] {
            print_rows(
                &[endpoint("127.0.0.1:50000", "192.0.2.1:443")],
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
