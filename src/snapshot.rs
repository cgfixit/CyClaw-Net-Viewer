use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
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

/// Direction for one endpoint. `listen_ports` is `(pid, local_port)` of TCP LISTEN sockets.
pub fn direction(
    proto: Proto,
    state: Option<TcpState>,
    pid: u32,
    local_port: u16,
    remote: SocketAddr,
    listen_ports: &HashSet<(u32, u16)>,
) -> Dir {
    match proto {
        Proto::Tcp => match state {
            Some(TcpState::Listen) => Dir::Listen,
            Some(TcpState::SynSent) => Dir::Out,
            _ if listen_ports.contains(&(pid, local_port)) => Dir::In,
            _ => Dir::Out,
        },
        Proto::Udp => {
            if remote.ip().is_unspecified() && remote.port() == 0 {
                Dir::Listen
            } else {
                Dir::Out
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
        let dir = direction(proto, state, pid, local.port(), remote, &listen_ports);
        let (process, path) = if pid == 0 {
            ("?".into(), String::new())
        } else {
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
    let mut name = [0u8; 64];
    let mut path = [0u8; 4096];
    let n = unsafe { libc::proc_name(pid as i32, name.as_mut_ptr() as *mut _, name.len() as u32) };
    let p =
        unsafe { libc::proc_pidpath(pid as i32, path.as_mut_ptr() as *mut _, path.len() as u32) };
    (buf_to_string(&mut name, n), buf_to_string(&mut path, p))
}

fn buf_to_string(buf: &mut [u8], n: i32) -> String {
    if n <= 0 {
        return String::new();
    }
    let end = (n as usize).min(buf.len().saturating_sub(1));
    buf[end] = 0;
    unsafe { CStr::from_ptr(buf.as_ptr() as *const _) }
        .to_string_lossy()
        .into_owned()
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

pub fn fmt_addr(addr: SocketAddr, name: Option<&str>) -> String {
    if addr.ip().is_unspecified() {
        return format!("*:{}", addr.port());
    }
    if let Some(n) = name {
        if !n.is_empty() {
            return format!("{n}:{}", addr.port());
        }
    }
    if addr.is_ipv6() {
        format!("[{}]:{}", addr.ip(), addr.port())
    } else {
        format!("{}:{}", addr.ip(), addr.port())
    }
}

pub fn csv_escape(s: &str) -> String {
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

    fn sa(port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
    }

    fn unspecified() -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
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
    fn direction_table() {
        let mut listen = HashSet::new();
        listen.insert((10, 443));
        assert_eq!(
            direction(
                Proto::Tcp,
                Some(TcpState::Listen),
                10,
                443,
                unspecified(),
                &listen
            ),
            Dir::Listen
        );
        assert_eq!(
            direction(
                Proto::Tcp,
                Some(TcpState::SynSent),
                11,
                50000,
                sa(443),
                &listen
            ),
            Dir::Out
        );
        assert_eq!(
            direction(
                Proto::Tcp,
                Some(TcpState::Established),
                10,
                443,
                sa(50000),
                &listen
            ),
            Dir::In
        );
        assert_eq!(
            direction(
                Proto::Tcp,
                Some(TcpState::Established),
                12,
                50001,
                sa(443),
                &listen
            ),
            Dir::Out
        );
        assert_eq!(
            direction(Proto::Udp, None, 1, 53, unspecified(), &listen),
            Dir::Listen
        );
        assert_eq!(
            direction(Proto::Udp, None, 1, 53, sa(53), &listen),
            Dir::Out
        );
    }
}
