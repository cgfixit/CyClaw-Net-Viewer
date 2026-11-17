//! TCP and UDP sockets of every readable process, through public libproc:
//! `proc_listallpids`, `proc_pidinfo(PROC_PIDLISTFDS)`, then
//! `proc_pidfdinfo(PROC_PIDFDSOCKETINFO)` per socket descriptor. This is
//! the path `lsof` takes. It needs no privilege and sees only what the
//! kernel lets this user read.
//!
//! The records mirror `struct socket_fdinfo` from `<sys/proc_info.h>`.
//! Only the fields read here are named; the rest is opaque padding of the
//! same size and alignment, and the layout is asserted at compile time.

use std::mem::{offset_of, size_of, MaybeUninit};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use libc::c_int;

use crate::procinfo;
use crate::snapshot::{Proto, TcpState};

/// `proc_pidfdinfo` flavor for `struct socket_fdinfo`.
const PROC_PIDFDSOCKETINFO: c_int = 3;
/// `soi_kind` values: which member of `soi_proto` the kernel filled.
const SOCKINFO_IN: c_int = 1;
const SOCKINFO_TCP: c_int = 2;
/// `insi_vflag` bits: which address slot holds the addresses.
const INI_IPV4: u8 = 0x1;
const INI_IPV6: u8 = 0x2;

/// `struct in4in6_addr`: an IPv4 address in the last four bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct In4In6Addr {
    pad: [u32; 3],
    addr4: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
union InAddr {
    v46: In4In6Addr,
    v6: [u8; 16],
}

/// `struct in_sockinfo`. Ports are network-order 16-bit values in an `int`.
#[repr(C)]
#[derive(Clone, Copy)]
struct InSockInfo {
    insi_fport: c_int,
    insi_lport: c_int,
    insi_gencnt: u64,
    insi_flags: u32,
    insi_flow: u32,
    insi_vflag: u8,
    insi_ip_ttl: u8,
    rfu_1: u32,
    insi_faddr: InAddr,
    insi_laddr: InAddr,
    /// `insi_v4` and `insi_v6`, unread.
    tail: [u32; 4],
}

/// `struct tcp_sockinfo`.
#[repr(C)]
#[derive(Clone, Copy)]
struct TcpSockInfo {
    tcpsi_ini: InSockInfo,
    tcpsi_state: c_int,
    /// Timers, MSS, flags, reserved and the TCP control block address.
    tail: [u32; 9],
}

#[repr(C)]
#[derive(Clone, Copy)]
union ProtoInfo {
    pri_in: InSockInfo,
    pri_tcp: TcpSockInfo,
    /// The largest member, `struct un_sockinfo`: 528 bytes, 8-byte aligned.
    largest: [u64; 66],
}

/// `struct socket_info`.
#[repr(C)]
#[derive(Clone, Copy)]
struct SocketInfo {
    /// `struct vinfo_stat`, then the socket and PCB addresses.
    head: [u64; 19],
    soi_type: c_int,
    soi_protocol: c_int,
    soi_family: c_int,
    /// Options, queue lengths, error, out-of-band mark and both buffers.
    middle: [u32; 17],
    soi_kind: c_int,
    rfu_1: u32,
    soi_proto: ProtoInfo,
}

/// `struct socket_fdinfo`: a `struct proc_fileinfo`, then the socket.
#[repr(C)]
#[derive(Clone, Copy)]
struct SocketFdInfo {
    pfi: [u64; 3],
    psi: SocketInfo,
}

// A wrong size makes every `proc_pidfdinfo` call fail its length check; a
// wrong offset would read the wrong bytes. Both are pinned to the values
// of the Darwin header.
const _: () = {
    assert!(size_of::<InSockInfo>() == 80);
    assert!(offset_of!(InSockInfo, insi_vflag) == 24);
    assert!(offset_of!(InSockInfo, insi_faddr) == 32);
    assert!(offset_of!(InSockInfo, insi_laddr) == 48);
    assert!(size_of::<TcpSockInfo>() == 120);
    assert!(offset_of!(TcpSockInfo, tcpsi_state) == 80);
    assert!(size_of::<ProtoInfo>() == 528);
    assert!(offset_of!(SocketInfo, soi_protocol) == 156);
    assert!(offset_of!(SocketInfo, soi_family) == 160);
    assert!(offset_of!(SocketInfo, soi_kind) == 232);
    assert!(offset_of!(SocketInfo, soi_proto) == 240);
    assert!(size_of::<SocketInfo>() == 768);
    assert!(size_of::<SocketFdInfo>() == 792);
};

/// One TCP or UDP socket descriptor of one process.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RawSocket {
    pub pid: u32,
    pub proto: Proto,
    pub local: SocketAddr,
    /// The connected peer. Unspecified with port 0 when there is none:
    /// a listener, or a UDP socket that was never connected.
    pub remote: SocketAddr,
    pub state: Option<TcpState>,
}

fn tcp_state(state: c_int) -> TcpState {
    match state {
        0 => TcpState::Closed,
        1 => TcpState::Listen,
        2 => TcpState::SynSent,
        3 => TcpState::SynReceived,
        4 => TcpState::Established,
        5 => TcpState::CloseWait,
        6 => TcpState::FinWait1,
        7 => TcpState::Closing,
        8 => TcpState::LastAck,
        9 => TcpState::FinWait2,
        10 => TcpState::TimeWait,
        _ => TcpState::Unknown,
    }
}

fn port(raw: c_int) -> u16 {
    let bytes = raw.to_ne_bytes();
    u16::from_be_bytes([bytes[0], bytes[1]])
}

/// The address in one slot. An IPv6 socket talking to an IPv4 peer has only
/// the IPv4 slot filled; it is returned IPv4-mapped so it stays an IPv6
/// socket address that the rest of the viewer unwraps for display.
fn address(slot: &InAddr, v6_socket: bool, vflag: u8) -> IpAddr {
    // SAFETY: both members are plain bytes of the same 16-byte storage.
    let (v4, v6) = unsafe { (Ipv4Addr::from(slot.v46.addr4), Ipv6Addr::from(slot.v6)) };
    if !v6_socket {
        IpAddr::V4(v4)
    } else if vflag & INI_IPV4 != 0 && vflag & INI_IPV6 == 0 {
        IpAddr::V6(v4.to_ipv6_mapped())
    } else {
        IpAddr::V6(v6)
    }
}

/// The TCP or UDP endpoint in a socket record, or `None` for any other
/// socket (Unix, raw, kernel control) or a record that does not add up.
fn parse(pid: u32, info: &SocketFdInfo) -> Option<RawSocket> {
    let socket = &info.psi;
    let v6_socket = match socket.soi_family {
        family if family == libc::AF_INET => false,
        family if family == libc::AF_INET6 => true,
        _ => return None,
    };
    let (proto, ini, state) = match (socket.soi_kind, socket.soi_protocol) {
        (SOCKINFO_TCP, libc::IPPROTO_TCP) => {
            // SAFETY: `soi_kind` says the kernel filled the TCP member.
            let tcp = unsafe { socket.soi_proto.pri_tcp };
            (Proto::Tcp, tcp.tcpsi_ini, Some(tcp_state(tcp.tcpsi_state)))
        }
        (SOCKINFO_IN, libc::IPPROTO_UDP) => {
            // SAFETY: `soi_kind` says the kernel filled the Internet member.
            (Proto::Udp, unsafe { socket.soi_proto.pri_in }, None)
        }
        // A TCP socket whose control block is already gone reports another
        // kind; it has no addresses to show.
        _ => return None,
    };
    let at = |slot: &InAddr, raw_port: c_int| {
        SocketAddr::new(address(slot, v6_socket, ini.insi_vflag), port(raw_port))
    };
    Some(RawSocket {
        pid,
        proto,
        local: at(&ini.insi_laddr, ini.insi_lport),
        remote: at(&ini.insi_faddr, ini.insi_fport),
        state,
    })
}

/// What listing one process's descriptors found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Listing {
    Listed,
    /// The process exited, or is a zombie that holds no descriptors.
    Gone,
    /// The process exists and this user may not read it.
    Unreadable,
}

fn errno() -> c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn clear_errno() {
    // SAFETY: `__error` returns this thread's errno slot, which is always valid.
    unsafe { *libc::__error() = 0 };
}

/// Why a listing call returned nothing: an exited process (ESRCH, or a
/// zombie) is not a gap in what was seen; anything else is.
fn failed(pid: libc::pid_t) -> Listing {
    let code = errno();
    if code == libc::ESRCH || procinfo::is_zombie(pid as u32) {
        Listing::Gone
    } else {
        Listing::Unreadable
    }
}

/// Descriptors of `pid` into `fds`. A process another user owns, or one
/// SIP or TCC protects, is `Unreadable`; one that exited is `Gone`.
fn list_fds(pid: libc::pid_t, fds: &mut Vec<libc::proc_fdinfo>) -> Listing {
    let entry = size_of::<libc::proc_fdinfo>();
    fds.clear();
    clear_errno();
    // SAFETY: a null buffer of size zero only asks for the byte count.
    let need =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if need < 0 || (need == 0 && errno() != 0) {
        return failed(pid);
    }
    if need == 0 {
        // A readable process with no descriptors at all.
        return Listing::Listed;
    }
    // Room for descriptors opened between the two calls.
    let capacity = need as usize / entry + 16;
    let Ok(bytes) = c_int::try_from(capacity * entry) else {
        return Listing::Unreadable;
    };
    fds.resize(
        capacity,
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0,
        },
    );
    clear_errno();
    // SAFETY: the buffer is writable for `bytes` bytes and outlives the call.
    let got = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            bytes,
        )
    };
    if got <= 0 {
        fds.clear();
        return if got == 0 && errno() == 0 {
            Listing::Listed
        } else {
            failed(pid)
        };
    }
    fds.truncate((got as usize / entry).min(capacity));
    Listing::Listed
}

/// The socket record of one descriptor. `proc_pidfdinfo` returns 0, not a
/// negative number, when the descriptor closed after it was listed; only a
/// full-length answer is read.
fn socket_info(pid: libc::pid_t, fd: i32) -> Option<SocketFdInfo> {
    let mut info = MaybeUninit::<SocketFdInfo>::zeroed();
    let size = size_of::<SocketFdInfo>() as c_int;
    // SAFETY: the buffer is writable for exactly `size` bytes, the flavor
    // matches the struct, and libproc does not keep the pointer.
    let got = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    // SAFETY: the storage was zeroed, and every field is plain data.
    (got == size).then(|| unsafe { info.assume_init() })
}

/// Every TCP and UDP socket this user can see. A process that exits, or a
/// descriptor that closes, during the walk is skipped. Only a failure to
/// list processes at all is an error.
/// Every socket this user may read, and the processes whose descriptors
/// could not be read. Those are gaps, not quiet processes: anything they
/// hold is missing from `sockets`.
pub struct Scan {
    pub sockets: Vec<RawSocket>,
    pub unreadable: Vec<u32>,
}

pub fn scan() -> Result<Scan, String> {
    let mut sockets = Vec::new();
    let mut unreadable = Vec::new();
    let mut fds = Vec::new();
    for pid in procinfo::pids()? {
        let Ok(target) = libc::pid_t::try_from(pid) else {
            continue;
        };
        match list_fds(target, &mut fds) {
            Listing::Listed => {}
            Listing::Gone => continue,
            Listing::Unreadable => {
                unreadable.push(pid);
                continue;
            }
        }
        for fd in &fds {
            if fd.proc_fdtype != libc::PROX_FDTYPE_SOCKET as u32 {
                continue;
            }
            if let Some(socket) = socket_info(target, fd.proc_fd).and_then(|i| parse(pid, &i)) {
                sockets.push(socket);
            }
        }
    }
    Ok(Scan {
        sockets,
        unreadable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(family: c_int, kind: c_int, protocol: c_int) -> SocketFdInfo {
        // SAFETY: every field is plain data, so all-zero is a valid value.
        let mut info: SocketFdInfo = unsafe { std::mem::zeroed() };
        info.psi.soi_family = family;
        info.psi.soi_kind = kind;
        info.psi.soi_protocol = protocol;
        info
    }

    fn wire_port(port: u16) -> c_int {
        let be = port.to_be_bytes();
        c_int::from_ne_bytes([be[0], be[1], 0, 0])
    }

    fn v4_slot(ip: [u8; 4]) -> InAddr {
        InAddr {
            v46: In4In6Addr {
                pad: [0; 3],
                addr4: ip,
            },
        }
    }

    fn endpoints(
        ini: InSockInfo,
        local: InAddr,
        lport: u16,
        remote: InAddr,
        fport: u16,
    ) -> InSockInfo {
        InSockInfo {
            insi_lport: wire_port(lport),
            insi_fport: wire_port(fport),
            insi_laddr: local,
            insi_faddr: remote,
            ..ini
        }
    }

    fn blank() -> InSockInfo {
        // SAFETY: plain data.
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn tcp_over_ipv4_reads_both_addresses_ports_and_state() {
        let mut info = record(libc::AF_INET, SOCKINFO_TCP, libc::IPPROTO_TCP);
        let mut ini = endpoints(
            blank(),
            v4_slot([192, 0, 2, 10]),
            54321,
            v4_slot([198, 51, 100, 7]),
            443,
        );
        ini.insi_vflag = INI_IPV4;
        info.psi.soi_proto.pri_tcp.tcpsi_ini = ini;
        info.psi.soi_proto.pri_tcp.tcpsi_state = 4;
        assert_eq!(
            parse(42, &info),
            Some(RawSocket {
                pid: 42,
                proto: Proto::Tcp,
                local: "192.0.2.10:54321".parse().unwrap(),
                remote: "198.51.100.7:443".parse().unwrap(),
                state: Some(TcpState::Established),
            })
        );
        for (raw, state) in [
            (0, TcpState::Closed),
            (1, TcpState::Listen),
            (2, TcpState::SynSent),
            (3, TcpState::SynReceived),
            (5, TcpState::CloseWait),
            (6, TcpState::FinWait1),
            (7, TcpState::Closing),
            (8, TcpState::LastAck),
            (9, TcpState::FinWait2),
            (10, TcpState::TimeWait),
            (11, TcpState::Unknown),
            (-1, TcpState::Unknown),
        ] {
            info.psi.soi_proto.pri_tcp.tcpsi_state = raw;
            assert_eq!(parse(42, &info).unwrap().state, Some(state));
        }
    }

    #[test]
    fn connected_udp_reports_its_peer_and_an_unconnected_one_does_not() {
        let mut info = record(libc::AF_INET, SOCKINFO_IN, libc::IPPROTO_UDP);
        let mut ini = endpoints(
            blank(),
            v4_slot([192, 0, 2, 10]),
            50000,
            v4_slot([203, 0, 113, 9]),
            443,
        );
        ini.insi_vflag = INI_IPV4;
        info.psi.soi_proto.pri_in = ini;
        let connected = parse(7, &info).unwrap();
        assert_eq!(connected.proto, Proto::Udp);
        assert_eq!(connected.state, None);
        assert_eq!(connected.local, "192.0.2.10:50000".parse().unwrap());
        assert_eq!(connected.remote, "203.0.113.9:443".parse().unwrap());

        info.psi.soi_proto.pri_in = endpoints(ini, v4_slot([0; 4]), 5353, v4_slot([0; 4]), 0);
        let bound = parse(7, &info).unwrap();
        assert_eq!(bound.local, "0.0.0.0:5353".parse().unwrap());
        assert_eq!(bound.remote, "0.0.0.0:0".parse().unwrap());
    }

    #[test]
    fn ipv6_sockets_read_native_addresses_and_map_an_ipv4_peer() {
        let v6 = |last: u8| {
            let mut bytes = [0u8; 16];
            bytes[0] = 0x20;
            bytes[1] = 0x01;
            bytes[2] = 0x0d;
            bytes[3] = 0xb8;
            bytes[15] = last;
            InAddr { v6: bytes }
        };
        let mut info = record(libc::AF_INET6, SOCKINFO_IN, libc::IPPROTO_UDP);
        let mut ini = endpoints(blank(), v6(1), 50000, v6(2), 443);
        ini.insi_vflag = INI_IPV6;
        info.psi.soi_proto.pri_in = ini;
        let native = parse(7, &info).unwrap();
        assert_eq!(native.local, "[2001:db8::1]:50000".parse().unwrap());
        assert_eq!(native.remote, "[2001:db8::2]:443".parse().unwrap());

        // A dual-stack socket with an IPv4 peer: only the IPv4 slot is set.
        let mut mapped = endpoints(
            blank(),
            v4_slot([127, 0, 0, 1]),
            50000,
            v4_slot([192, 0, 2, 1]),
            443,
        );
        mapped.insi_vflag = INI_IPV4;
        info.psi.soi_proto.pri_in = mapped;
        let socket = parse(7, &info).unwrap();
        assert_eq!(socket.local, "[::ffff:127.0.0.1]:50000".parse().unwrap());
        assert_eq!(socket.remote, "[::ffff:192.0.2.1]:443".parse().unwrap());
        assert!(socket.local.is_ipv6(), "still an IPv6 socket");

        // An unbound IPv6 socket is the IPv6 wildcard, not an IPv4 one.
        let mut unbound = blank();
        unbound.insi_vflag = INI_IPV4 | INI_IPV6;
        info.psi.soi_proto.pri_in = unbound;
        assert_eq!(
            parse(7, &info).unwrap().local,
            "[::]:0".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn other_sockets_and_inconsistent_records_are_skipped() {
        for (family, kind, protocol) in [
            (libc::AF_UNIX, 3, 0),
            (libc::AF_INET, SOCKINFO_IN, libc::IPPROTO_ICMP),
            (libc::AF_INET, 0, libc::IPPROTO_TCP),
            (libc::AF_INET, SOCKINFO_IN, libc::IPPROTO_TCP),
            (libc::AF_INET, SOCKINFO_TCP, libc::IPPROTO_UDP),
            (99, SOCKINFO_TCP, libc::IPPROTO_TCP),
        ] {
            assert_eq!(parse(1, &record(family, kind, protocol)), None);
        }
    }

    #[test]
    fn ports_are_network_order_in_the_first_two_bytes() {
        assert_eq!(port(wire_port(443)), 443);
        assert_eq!(port(wire_port(0)), 0);
        assert_eq!(port(wire_port(65535)), 65535);
        assert_eq!(port(c_int::from_ne_bytes([0x01, 0xbb, 0x7f, 0x7f])), 443);
    }

    #[test]
    fn an_unreaped_child_is_gone_not_unreadable() {
        let mut child = std::process::Command::new("/usr/bin/true")
            .spawn()
            .expect("start a child that exits at once");
        let pid = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while procinfo::identity(pid).is_some() {
            assert!(std::time::Instant::now() < deadline, "child never exited");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Exited but not waited for: a zombie. It must not count as a gap,
        // or every watch with a short-lived child would end incomplete.
        let scan = scan().expect("scan");
        let unreadable = scan.unreadable.contains(&pid);
        assert!(child.wait().unwrap().success());
        assert!(!unreadable, "zombie {pid} was reported unreadable");
    }

    #[test]
    fn scan_sees_a_listener_this_process_holds() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let local = listener.local_addr().unwrap();
        let me = std::process::id();
        let scan = scan().expect("scan");
        // This process can always read itself.
        assert!(!scan.unreadable.contains(&me));
        assert!(scan.sockets.iter().any(|s| {
            s.pid == me
                && s.proto == Proto::Tcp
                && s.local == local
                && s.remote.port() == 0
                && s.state == Some(TcpState::Listen)
        }));
    }
}
