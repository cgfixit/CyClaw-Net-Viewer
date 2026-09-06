use crate::dns::lookup_ip;
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

    let name = |ip| {
        if args.numeric {
            None
        } else {
            lookup_ip(ip)
        }
    };

    if args.csv {
        println!("Process,PID,Proto,Dir,Local,Remote,State,Path");
    }
    for e in &eps {
        print_row(e, args.csv, &name);
    }
    Ok(true)
}

fn print_row(
    e: &Endpoint,
    csv: bool,
    name: &dyn Fn(std::net::IpAddr) -> Option<crate::dns::Resolved>,
) {
    let fmt = |addr: std::net::SocketAddr| {
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
         GUI:  netboard\n\
         CLI:  netboard --cli [-a] [-c] [-n] [process|pid]\n\
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
        let parsed = parse(&args(&["netboard", "--cli"])).unwrap();
        assert!(!parsed.all);
        assert!(!parsed.csv);
        assert!(!parsed.numeric);
        assert_eq!(parsed.filter, None);
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
            vec!["netboard", "--cli", "-anc"],
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
}
