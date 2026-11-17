use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::sockets;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Proto {
    Tcp,
    Udp,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IpVer {
    V4,
    V6,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    In,
    Out,
    Listen,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
    Unknown,
}

impl TcpState {
    pub fn as_str(self) -> &'static str {
        match self {
            TcpState::Closed => "CLOSED",
            TcpState::Listen => "LISTEN",
            TcpState::SynSent => "SYN_SENT",
            TcpState::SynReceived => "SYN_RECEIVED",
            TcpState::Established => "ESTABLISHED",
            TcpState::FinWait1 => "FIN_WAIT_1",
            TcpState::FinWait2 => "FIN_WAIT_2",
            TcpState::CloseWait => "CLOSE_WAIT",
            TcpState::Closing => "CLOSING",
            TcpState::LastAck => "LAST_ACK",
            TcpState::TimeWait => "TIME_WAIT",
            TcpState::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EndpointKey {
    pub pid: u32,
    pub proto: Proto,
    pub ip_ver: IpVer,
    pub local: SocketAddr,
    pub remote: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub key: EndpointKey,
    pub state: Option<TcpState>,
    pub dir: Dir,
    pub process: String,
    pub path: String,
}

impl Endpoint {
    pub fn proto_label(&self) -> &'static str {
        match (self.key.proto, self.key.ip_ver) {
            (Proto::Tcp, IpVer::V4) => "TCP4",
            (Proto::Tcp, IpVer::V6) => "TCP6",
            (Proto::Udp, IpVer::V4) => "UDP4",
            (Proto::Udp, IpVer::V6) => "UDP6",
        }
    }

    pub fn dir_label(&self) -> &'static str {
        match self.dir {
            Dir::In => "In",
            Dir::Out => "Out",
            Dir::Listen => "Listen",
            Dir::Unknown => "Unknown",
        }
    }

    pub fn state_label(&self) -> &'static str {
        self.state.map(TcpState::as_str).unwrap_or("")
    }
}

#[derive(Debug)]
pub struct SnapshotError(pub String);

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for SnapshotError {}

/// TCP uses the existing per-PID listening-port heuristic.
/// UDP has no listening state: a concrete peer plus a matching local binding
/// suggests In (a receiver), never proves packet direction. Match PID, family,
/// port and address; a wildcard binding covers that family's local addresses.
/// Missing peers, port zero, unknown owners or unmatched bindings mean Unknown,
/// including client-style endpoints: a peer alone is not evidence of Out.
/// `udp_bindings` holds unconnected sockets only, so a connected socket is
/// never its own receiver: an ordinary UDP client stays Unknown.
pub fn direction(
    proto: Proto,
    state: Option<TcpState>,
    pid: u32,
    local: SocketAddr,
    remote: SocketAddr,
    listen_ports: &HashSet<(u32, u16)>,
    udp_bindings: &HashSet<(u32, SocketAddr)>,
) -> Dir {
    match proto {
        Proto::Tcp => match state {
            Some(TcpState::Listen) => Dir::Listen,
            Some(TcpState::SynSent) => Dir::Out,
            _ if listen_ports.contains(&(pid, local.port())) => Dir::In,
            _ => Dir::Out,
        },
        Proto::Udp => {
            if pid == 0
                || local.port() == 0
                || remote.ip().is_unspecified()
                || remote.port() == 0
                || local.is_ipv4() != remote.is_ipv4()
            {
                return Dir::Unknown;
            }
            let wildcard = SocketAddr::new(unspecified_remote(local.ip()).ip(), local.port());
            if udp_bindings.contains(&(pid, local)) || udp_bindings.contains(&(pid, wildcard)) {
                Dir::In
            } else {
                Dir::Unknown
            }
        }
    }
}

pub fn snapshot() -> Result<Vec<Endpoint>, SnapshotError> {
    let socks = sockets::scan().map_err(|e| SnapshotError(format!("libproc sockets: {e}")))?;
    Ok(endpoints(socks, proc_names))
}

/// Direction and process metadata for one scan. `names` is asked once per PID.
fn endpoints(
    socks: Vec<sockets::RawSocket>,
    mut names_of: impl FnMut(u32) -> (String, String),
) -> Vec<Endpoint> {
    let mut names: HashMap<u32, (String, String)> = HashMap::new();
    let mut listen_ports: HashSet<(u32, u16)> = HashSet::new();
    let mut udp_bindings = HashSet::new();
    let mut raw: Vec<(u32, Proto, SocketAddr, SocketAddr, Option<TcpState>)> =
        Vec::with_capacity(socks.len());

    for s in socks {
        match s.proto {
            Proto::Tcp => {
                if s.state == Some(TcpState::Listen) {
                    listen_ports.insert((s.pid, s.local.port()));
                }
            }
            Proto::Udp => {
                // A binding is a socket waiting for anyone, not proof of a
                // listener. One that is connected to a peer is not a
                // receiver for its own traffic.
                let connected = !s.remote.ip().is_unspecified() && s.remote.port() != 0;
                if s.pid != 0 && s.local.port() != 0 && !connected {
                    udp_bindings.insert((s.pid, s.local));
                }
            }
        }
        raw.push((s.pid, s.proto, s.local, s.remote, s.state));
    }

    let mut out = Vec::with_capacity(raw.len());
    for (pid, proto, local, remote, state) in raw {
        let ip_ver = if local.is_ipv6() {
            IpVer::V6
        } else {
            IpVer::V4
        };
        let dir = direction(
            proto,
            state,
            pid,
            local,
            remote,
            &listen_ports,
            &udp_bindings,
        );
        let (process, path) = if pid == 0 {
            ("?".into(), String::new())
        } else {
            // Resolve each PID once per snapshot. Each Endpoint owns its strings
            // after this cache is dropped; these per-row clones are intentional.
            names.entry(pid).or_insert_with(|| names_of(pid)).clone()
        };
        let process = if process.is_empty() {
            "?".into()
        } else {
            process
        };
        out.push(Endpoint {
            key: EndpointKey {
                pid,
                proto,
                ip_ver,
                local,
                remote,
            },
            state,
            dir,
            process,
            path,
        });
    }
    out
}

fn unspecified_remote(local: IpAddr) -> SocketAddr {
    match local {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}

/// Fresh process name for `pid`, using the same "?" placeholder as snapshots.
/// Lets termination re-check that a PID still names the confirmed process.
pub fn process_name(pid: u32) -> String {
    process_names(pid).0
}

/// Fresh process name and executable path for `pid`, with the snapshot's
/// "?" placeholder for a name that cannot be read. The path may be empty.
pub fn process_names(pid: u32) -> (String, String) {
    let (name, path) = if pid == 0 {
        (String::new(), String::new())
    } else {
        proc_names(pid)
    };
    if name.is_empty() {
        ("?".into(), path)
    } else {
        (name, path)
    }
}

fn proc_names(pid: u32) -> (String, String) {
    let Ok(pid) = i32::try_from(pid) else {
        return (String::new(), String::new());
    };
    // proc_name uses proc_bsdinfo.pbi_name (2 * MAXCOMLEN bytes), falling
    // back to pbi_comm. This is a possibly truncated name, not a bundle ID.
    // Keep a trailing zero even if the fixed-width copy fills pbi_name:
    // libproc calls strlen on that copy before returning its byte count.
    let mut name = [0u8; 2 * libc::MAXCOMLEN + 1];
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: writable buffers have exactly the advertised byte capacities;
    // pid fits c_int, and neither API retains the pointer after returning.
    let n = unsafe { libc::proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
    // SAFETY: same buffer/lifetime contract, with the platform path capacity.
    let p = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    (buf_to_string(&name, n), buf_to_string(&path, p))
}

fn buf_to_string(buf: &[u8], n: i32) -> String {
    if n <= 0 {
        return String::new();
    }
    let bytes = &buf[..(n as usize).min(buf.len())];
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    // Bounded decoding also handles truncation without NUL and partial UTF-8.
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// True when the peer is off this machine (not unspecified, not loopback).
/// Used to color local→remote rows for telemetry-kill watching.
pub fn is_offbox(addr: SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(v) => !v.is_unspecified() && !v.is_loopback(),
        IpAddr::V6(v) => {
            if v.is_unspecified() || v.is_loopback() {
                return false;
            }
            if let Some(v4) = v.to_ipv4_mapped() {
                return !v4.is_unspecified() && !v4.is_loopback();
            }
            true
        }
    }
}

pub fn numeric_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v) => v.to_string(),
        IpAddr::V6(v) => v
            .to_ipv4_mapped()
            .map(|x| x.to_string())
            .unwrap_or_else(|| v.to_string()),
    }
}

/// Keep the observed socket address, with an optional untrusted DNS label.
/// Only IPv4-mapped IPv6 addresses unwrap; DNS answers and other endpoints
/// cannot establish that a native IPv6 peer is an IPv4 peer.
pub fn fmt_addr(addr: SocketAddr, host: Option<&str>) -> String {
    if addr.ip().is_unspecified() {
        return format!("*:{}", addr.port());
    }
    let port = addr.port();
    let shown = match addr.ip() {
        IpAddr::V6(v) if v.to_ipv4_mapped().is_none() => format!("[{}]", numeric_ip(addr.ip())),
        _ => numeric_ip(addr.ip()),
    };
    if let Some(n) = host.filter(|n| !n.is_empty()) {
        return format!("{n} ({shown}):{port}");
    }
    format!("{shown}:{port}")
}

/// Characters that can forge rows or reorder text in a terminal, table cell,
/// or pasteboard: Unicode controls plus bidi overrides/isolates/marks and
/// zero-width joiners. Process names and paths are attacker-chosen.
pub(crate) fn is_spoofing_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// Replace controls and bidi/zero-width formatting characters with spaces.
/// Used for the pasteboard, GUI cells, and CLI text output.
pub fn sanitize_clipboard_text(s: &str) -> String {
    s.chars()
        .map(|c| if is_spoofing_char(c) { ' ' } else { c })
        .collect()
}

pub fn csv_escape(s: &str) -> String {
    // Process names and DNS data are untrusted spreadsheet input. Quoting
    // alone does not stop formulas; preserve the value as text on import.
    let trimmed = s.trim_start_matches(|c: char| c.is_whitespace() || c.is_control());
    let formula = trimmed.starts_with(['=', '+', '-', '@', '＝', '＋', '－', '＠'])
        || s.starts_with(['\t', '\r', '\n']);
    if formula {
        return format!("\"'{}\"", s.replace('"', "\"\""));
    }
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4};

    #[test]
    fn clipboard_text_replaces_controls_and_preserves_ordinary_utf8() {
        assert_eq!(sanitize_clipboard_text(""), "");
        assert_eq!(sanitize_clipboard_text("café"), "café");
        assert_eq!(sanitize_clipboard_text("ok 📡"), "ok 📡");
        assert_eq!(
            sanitize_clipboard_text("a\u{0000}b\nc\rd\u{007f}e\u{0085}f"),
            "a b c d e f"
        );
        assert_eq!(sanitize_clipboard_text("proc\tpid\tTCP4"), "proc pid TCP4");
        // Bidi overrides/isolates and zero-width characters cannot reorder
        // or hide text; ordinary RTL letters stay intact.
        assert_eq!(
            sanitize_clipboard_text("evil\u{202E}gpj.exe\u{2066}x\u{2069}\u{200B}\u{FEFF}"),
            "evil gpj.exe x   "
        );
        assert_eq!(sanitize_clipboard_text("שלום"), "שלום");
    }

    #[test]
    fn process_name_uses_placeholder_for_missing_owner() {
        assert_eq!(process_name(0), "?");
        assert_ne!(process_name(std::process::id()), "?");
    }

    #[test]
    fn csv_quotes_delimiters_and_preserves_ordinary_text() {
        assert_eq!(csv_escape(""), "");
        assert_eq!(csv_escape("café"), "café");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn csv_marks_formula_like_values_as_text() {
        for value in [
            "=1+1", "+cmd", "-1", "@SUM(A1)", "  =1", "\tfoo", "\rfoo", "\nfoo", "＝1", "＋1",
            "－1", "＠foo",
        ] {
            assert_eq!(csv_escape(value), format!("\"'{value}\""));
        }
        assert_eq!(csv_escape("=\"x,y\""), "\"'=\"\"x,y\"\"\"");
    }

    fn sa(port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
    }

    fn unspecified() -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
    }

    #[test]
    fn address_labels_preserve_the_observed_peer() {
        for (address, numeric) in [
            ("192.0.2.1:443", "192.0.2.1:443"),
            ("[2001:db8::1]:443", "[2001:db8::1]:443"),
            ("[::1]:80", "[::1]:80"),
            ("[::ffff:192.0.2.1]:443", "192.0.2.1:443"),
        ] {
            let addr = address.parse().unwrap();
            assert_eq!(fmt_addr(addr, None), numeric);
            assert_eq!(fmt_addr(addr, Some("")), numeric);
            let (ip, port) = numeric.rsplit_once(':').unwrap();
            assert_eq!(
                fmt_addr(addr, Some("untrusted.example")),
                format!("untrusted.example ({ip}):{port}")
            );
        }
        assert_eq!(fmt_addr(unspecified(), Some("ignored.example")), "*:0");
    }

    #[test]
    fn offbox_is_remote_not_loopback() {
        assert!(!is_offbox(unspecified()));
        assert!(!is_offbox(sa(443)));
        assert!(is_offbox(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(8, 8, 8, 8),
            443
        ))));
        assert!(is_offbox(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(10, 0, 0, 1),
            443
        ))));
        let mapped_loop = SocketAddr::new(
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x7f00, 1)),
            1,
        );
        assert!(!is_offbox(mapped_loop));
    }

    #[test]
    fn tcp_direction_is_unchanged() {
        let listen = HashSet::from([(10, 443)]);
        let udp = HashSet::new();
        for (state, pid, port, remote, expected) in [
            (TcpState::Listen, 10, 443, unspecified(), Dir::Listen),
            (TcpState::SynSent, 10, 443, sa(50000), Dir::Out),
            (TcpState::Established, 10, 443, sa(50000), Dir::In),
            (TcpState::Established, 12, 50001, sa(443), Dir::Out),
        ] {
            assert_eq!(
                direction(
                    Proto::Tcp,
                    Some(state),
                    pid,
                    sa(port),
                    remote,
                    &listen,
                    &udp
                ),
                expected
            );
        }
    }

    #[test]
    fn udp_direction_requires_peer_and_matching_binding() {
        let tcp = HashSet::from([(10, 53)]);
        let wildcard = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 53);
        let udp = HashSet::from([(10, wildcard), (11, sa(5353))]);
        for (pid, local, remote, expected) in [
            (10, sa(53), sa(50000), Dir::In),          // wildcard server binding
            (11, sa(5353), sa(50000), Dir::In),        // exact server binding
            (10, sa(50000), sa(53), Dir::Unknown),     // client-shaped, no evidence
            (10, sa(53), unspecified(), Dir::Unknown), // never connected
            (10, wildcard, sa(50000), Dir::In),
            (
                10,
                sa(53),
                SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 123),
                Dir::Unknown,
            ),
            (10, sa(53), sa(0), Dir::Unknown),
            (10, sa(0), sa(53), Dir::Unknown),
            (0, sa(53), sa(50000), Dir::Unknown),
            (12, sa(53), sa(50000), Dir::Unknown), // different PID
            (
                11,
                SocketAddr::new(Ipv4Addr::new(127, 0, 0, 2).into(), 5353),
                sa(50000),
                Dir::Unknown,
            ),
        ] {
            assert_eq!(
                direction(Proto::Udp, None, pid, local, remote, &tcp, &udp),
                expected
            );
        }
        assert_eq!(
            direction(
                Proto::Udp,
                None,
                10,
                sa(53),
                sa(50000),
                &tcp,
                &HashSet::new()
            ),
            Dir::Unknown
        );
    }

    #[test]
    fn a_connected_udp_socket_keeps_its_peer_and_is_not_its_own_receiver() {
        let raw = |pid, proto, local: &str, remote: &str, state| sockets::RawSocket {
            pid,
            proto,
            local: local.parse().unwrap(),
            remote: remote.parse().unwrap(),
            state,
        };
        let mut asked = Vec::new();
        let eps = endpoints(
            vec![
                // A client: connected, with no unconnected sibling on its port.
                raw(10, Proto::Udp, "192.0.2.10:50000", "203.0.113.9:443", None),
                // A server: one unconnected socket and one connected to a client.
                raw(20, Proto::Udp, "0.0.0.0:4433", "0.0.0.0:0", None),
                raw(
                    20,
                    Proto::Udp,
                    "192.0.2.10:4433",
                    "198.51.100.7:61000",
                    None,
                ),
                raw(
                    30,
                    Proto::Tcp,
                    "127.0.0.1:8080",
                    "0.0.0.0:0",
                    Some(TcpState::Listen),
                ),
                raw(
                    30,
                    Proto::Tcp,
                    "127.0.0.1:8080",
                    "127.0.0.1:51000",
                    Some(TcpState::Established),
                ),
            ],
            |pid| {
                asked.push(pid);
                (format!("p{pid}"), String::new())
            },
        );
        let seen: Vec<(u32, String, Dir)> = eps
            .iter()
            .map(|e| (e.key.pid, e.key.remote.to_string(), e.dir))
            .collect();
        assert_eq!(
            seen,
            [
                (10, "203.0.113.9:443".to_string(), Dir::Unknown),
                (20, "0.0.0.0:0".to_string(), Dir::Unknown),
                (20, "198.51.100.7:61000".to_string(), Dir::In),
                (30, "0.0.0.0:0".to_string(), Dir::Listen),
                (30, "127.0.0.1:51000".to_string(), Dir::In),
            ]
        );
        assert!(is_offbox(eps[0].key.remote), "a UDP peer can be off-box");
        assert_eq!(eps[0].process, "p10");
        asked.sort_unstable();
        assert_eq!(asked, [10, 20, 30], "names are read once per PID");
    }

    #[test]
    fn udp_bindings_do_not_cross_address_families() {
        let tcp = HashSet::new();
        let local = SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 53);
        let peer = SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 50000);
        let v4 = HashSet::from([(10, SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 53))]);
        assert_eq!(
            direction(Proto::Udp, None, 10, local, peer, &tcp, &v4),
            Dir::Unknown
        );
        let v6 = HashSet::from([(10, SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 53))]);
        assert_eq!(
            direction(Proto::Udp, None, 10, local, peer, &tcp, &v6),
            Dir::In
        );
        assert_eq!(
            direction(Proto::Udp, None, 10, local, sa(50000), &tcp, &v6),
            Dir::Unknown
        );
        assert_eq!(
            direction(
                Proto::Udp,
                None,
                10,
                local,
                unspecified_remote(local.ip()),
                &tcp,
                &v6
            ),
            Dir::Unknown
        );
    }

    #[test]
    fn process_buffers_decode_within_reported_bounds() {
        assert_eq!(buf_to_string(b"name\0junk", 9), "name");
        assert_eq!(buf_to_string(b"longname", 4), "long");
        assert_eq!(buf_to_string(b"full", 99), "full");
        assert_eq!(buf_to_string(b"name", -1), "");
        assert_eq!(buf_to_string(b"name", 0), "");
        assert_eq!(buf_to_string(b"", 1), "");
        assert_eq!(buf_to_string(&[b'a', 0xc3], 2), "a\u{fffd}");
    }
}
