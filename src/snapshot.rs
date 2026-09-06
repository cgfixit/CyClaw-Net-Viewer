use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use netstat2::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState as NsTcp,
};

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

    fn from_ns(s: NsTcp) -> Self {
        match s {
            NsTcp::Closed => TcpState::Closed,
            NsTcp::Listen => TcpState::Listen,
            NsTcp::SynSent => TcpState::SynSent,
            NsTcp::SynReceived => TcpState::SynReceived,
            NsTcp::Established => TcpState::Established,
            NsTcp::FinWait1 => TcpState::FinWait1,
            NsTcp::FinWait2 => TcpState::FinWait2,
            NsTcp::CloseWait => TcpState::CloseWait,
            NsTcp::Closing => TcpState::Closing,
            NsTcp::LastAck => TcpState::LastAck,
            NsTcp::TimeWait => TcpState::TimeWait,
            NsTcp::DeleteTcb | NsTcp::Unknown => TcpState::Unknown,
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
/// netstat2 currently omits UDP peers, so real UDP snapshots remain Unknown.
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
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto = ProtocolFlags::TCP | ProtocolFlags::UDP;
    let socks =
        get_sockets_info(af, proto).map_err(|e| SnapshotError(format!("libproc sockets: {e}")))?;

    let mut names: HashMap<u32, (String, String)> = HashMap::new();
    let mut listen_ports: HashSet<(u32, u16)> = HashSet::new();
    let mut udp_bindings = HashSet::new();
    let mut raw: Vec<(u32, Proto, SocketAddr, SocketAddr, Option<TcpState>)> =
        Vec::with_capacity(socks.len());

    for si in socks {
        let pid = si.associated_pids.first().copied().unwrap_or(0);
        match si.protocol_socket_info {
            ProtocolSocketInfo::Tcp(t) => {
                let local = SocketAddr::new(t.local_addr, t.local_port);
                let remote = SocketAddr::new(t.remote_addr, t.remote_port);
                let state = TcpState::from_ns(t.state);
                if state == TcpState::Listen {
                    listen_ports.insert((pid, t.local_port));
                }
                raw.push((pid, Proto::Tcp, local, remote, Some(state)));
            }
            ProtocolSocketInfo::Udp(u) => {
                let local = SocketAddr::new(u.local_addr, u.local_port);
                // This is a binding, not proof of a UDP listener. The source
                // does not expose peers, even for connected UDP sockets.
                if pid != 0 && local.port() != 0 {
                    udp_bindings.insert((pid, local));
                }
                let remote = unspecified_remote(u.local_addr);
                raw.push((pid, Proto::Udp, local, remote, None));
            }
        }
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
            names.entry(pid).or_insert_with(|| proc_names(pid)).clone()
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
    Ok(out)
}

fn unspecified_remote(local: IpAddr) -> SocketAddr {
    match local {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
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

/// `host (1.2.3.4):443` when DNS gave a name and an IPv4; else hostname+literal or numeric.
pub fn fmt_addr(addr: SocketAddr, host: Option<&str>, ipv4: Option<Ipv4Addr>) -> String {
    if addr.ip().is_unspecified() {
        return format!("*:{}", addr.port());
    }
    let port = addr.port();
    let ip_txt = numeric_ip(addr.ip());
    if let Some(n) = host {
        if !n.is_empty() {
            let shown = ipv4.map(|v| v.to_string()).unwrap_or(ip_txt);
            return format!("{n} ({shown}):{port}");
        }
    }
    match addr.ip() {
        IpAddr::V6(v) if v.to_ipv4_mapped().is_none() => format!("[{ip_txt}]:{port}"),
        _ => format!("{ip_txt}:{port}"),
    }
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
    fn fmt_addr_hostname_and_ipv4() {
        let v4 = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(8, 8, 8, 8), 443));
        assert_eq!(fmt_addr(v4, None, None), "8.8.8.8:443");
        assert_eq!(
            fmt_addr(v4, Some("dns.google"), Some(Ipv4Addr::new(8, 8, 8, 8))),
            "dns.google (8.8.8.8):443"
        );
        assert_eq!(fmt_addr(unspecified(), None, None), "*:0");
        let mapped = SocketAddr::new(
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x0808, 0x0808)),
            53,
        );
        assert_eq!(fmt_addr(mapped, None, None), "8.8.8.8:53");
        let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 80);
        assert_eq!(
            fmt_addr(v6, Some("localhost"), Some(Ipv4Addr::LOCALHOST)),
            "localhost (127.0.0.1):80"
        );
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
            (10, sa(53), unspecified(), Dir::Unknown), // current netstat2 data
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
