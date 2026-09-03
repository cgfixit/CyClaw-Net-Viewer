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

fn print_row(e: &Endpoint, csv: bool, name: &dyn Fn(std::net::IpAddr) -> Option<String>) {
    let local = fmt_addr(e.key.local, name(e.key.local.ip()).as_deref());
    let remote = fmt_addr(e.key.remote, name(e.key.remote.ip()).as_deref());
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
            "{:<18} {:>6} {:<5} {:<6} {:<28} {:<28} {:<13} {}",
            trunc(&e.process, 18),
            e.key.pid,
            e.proto_label(),
            e.dir_label(),
            trunc(&local, 28),
            trunc(&remote, 28),
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
