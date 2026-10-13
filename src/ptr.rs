//! PTR fallbacks for when the system resolver has no answer: one legacy
//! unicast mDNS query on the local link (RFC 6762 section 6.7) for LAN
//! peers, and an optional user-selected public resolver for globally
//! routable peers. Callers must still validate the returned hostname.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const MDNS_GROUP: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)), 5353);
const MDNS_WAIT: Duration = Duration::from_millis(800);
const UNICAST_WAIT: Duration = Duration::from_millis(1200);
const UNICAST_ATTEMPTS: usize = 2;
const TYPE_CNAME: u16 = 5;
const TYPE_PTR: u16 = 12;
const CLASS_IN: u16 = 1;

/// Optional public resolver for global peers. Off by default: enabling it
/// sends each unresolved public peer IP to that provider in cleartext.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Fallback {
    Off = 0,
    Quad9 = 1,
    Cloudflare = 2,
}

impl Fallback {
    pub const ALL: [Fallback; 3] = [Fallback::Off, Fallback::Quad9, Fallback::Cloudflare];

    pub fn label(self) -> &'static str {
        match self {
            Fallback::Off => "Off (system resolver + LAN mDNS)",
            Fallback::Quad9 => "Quad9 (9.9.9.9)",
            Fallback::Cloudflare => "Cloudflare (1.1.1.1)",
        }
    }

    pub fn server(self) -> Option<SocketAddr> {
        let ip = match self {
            Fallback::Off => return None,
            Fallback::Quad9 => Ipv4Addr::new(9, 9, 9, 9),
            Fallback::Cloudflare => Ipv4Addr::new(1, 1, 1, 1),
        };
        Some(SocketAddr::new(IpAddr::V4(ip), 53))
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Fallback::Quad9,
            2 => Fallback::Cloudflare,
            _ => Fallback::Off,
        }
    }
}

/// Which fallback, if any, may be asked about an address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// RFC 1918 or IPv4 link-local: ask on the local link via mDNS.
    Lan,
    /// Globally routable unicast: the opt-in public resolver may be asked.
    Global,
    /// Loopback, multicast, documentation, CGNAT, reserved, IPv6 LAN:
    /// never sent anywhere beyond the system resolver.
    Other,
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v) => v.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

pub fn scope(ip: IpAddr) -> Scope {
    match canonical(ip) {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            if v.is_private() || v.is_link_local() {
                Scope::Lan
            } else if v.is_unspecified()
                || v.is_loopback()
                || v.is_multicast()
                || v.is_broadcast()
                || v.is_documentation()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b)) // shared address space
                || (a == 198 && (b == 18 || b == 19)) // benchmarking
                || (a, b, c) == (192, 0, 0)
            {
                Scope::Other
            } else {
                Scope::Global
            }
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            let global_unicast = s[0] & 0xe000 == 0x2000;
            let documentation = s[0] == 0x2001 && s[1] == 0x0db8;
            if global_unicast && !documentation {
                Scope::Global
            } else {
                Scope::Other
            }
        }
    }
}

/// `d.c.b.a.in-addr.arpa` or the 32-nibble `ip6.arpa` name.
pub fn reverse_name(ip: IpAddr) -> String {
    match canonical(ip) {
        IpAddr::V4(v) => {
            let [a, b, c, d] = v.octets();
            format!("{d}.{c}.{b}.{a}.in-addr.arpa")
        }
        IpAddr::V6(v) => {
            let mut name = String::with_capacity(72);
            for byte in v.octets().iter().rev() {
                name.push_str(&format!("{:x}.{:x}.", byte & 0x0f, byte >> 4));
            }
            name.push_str("ip6.arpa");
            name
        }
    }
}

/// mDNS PTR for a LAN peer. Bonjour/Avahi hosts publish reverse records for
/// their own addresses even when the router's DNS has none. Only replies
/// from LAN-scoped sources are considered.
pub fn mdns_ptr(ip: IpAddr) -> Option<String> {
    match exchange(MDNS_GROUP, false, ip, MDNS_WAIT, |src| {
        scope(src.ip()) == Scope::Lan
    }) {
        Outcome::Host(host) => Some(host),
        Outcome::Negative | Outcome::Timeout => None,
    }
}

/// Recursive PTR query to `server`. Replies must come from that exact
/// address and port; a timeout is retried once with a fresh ID and port.
pub fn unicast_ptr(ip: IpAddr, server: SocketAddr) -> Option<String> {
    for _ in 0..UNICAST_ATTEMPTS {
        match exchange(server, true, ip, UNICAST_WAIT, |src| src == server) {
            Outcome::Host(host) => return Some(host),
            Outcome::Negative => return None,
            Outcome::Timeout => {}
        }
    }
    None
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Host(String),
    Negative,
    Timeout,
}

fn exchange(
    dest: SocketAddr,
    recursion: bool,
    ip: IpAddr,
    wait: Duration,
    accept_from: impl Fn(SocketAddr) -> bool,
) -> Outcome {
    let qname = reverse_name(ip);
    let id = random_id();
    let multicast = dest.ip().is_multicast();
    let bind = SocketAddr::new(
        if dest.is_ipv4() {
            Ipv4Addr::UNSPECIFIED.into()
        } else {
            std::net::Ipv6Addr::UNSPECIFIED.into()
        },
        0,
    );
    let Ok(sock) = UdpSocket::bind(bind) else {
        return Outcome::Timeout;
    };
    if multicast {
        // RFC 6762: mDNS packets use IP TTL 255; routers never forward them.
        let _ = sock.set_multicast_ttl_v4(255);
    }
    if sock.send_to(&query(id, recursion, &qname), dest).is_err() {
        return Outcome::Timeout;
    }
    let deadline = Instant::now() + wait;
    let mut buf = [0u8; 1500];
    loop {
        let Some(left) = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
        else {
            return Outcome::Timeout;
        };
        if sock.set_read_timeout(Some(left)).is_err() {
            return Outcome::Timeout;
        }
        let (n, src) = match sock.recv_from(&mut buf) {
            Ok(got) => got,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Outcome::Timeout,
        };
        if !accept_from(src) {
            continue;
        }
        match parse_reply(&buf[..n], id, &qname) {
            Reply::Ignore => continue,
            Reply::Host(host) => return Outcome::Host(host),
            // Several hosts may hear a multicast query; keep listening.
            Reply::Negative if multicast => continue,
            Reply::Negative => return Outcome::Negative,
        }
    }
}

fn random_id() -> u16 {
    // RandomState keys come from OS randomness; the hash of the clock is an
    // unpredictable 16-bit ID without adding an RNG dependency.
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish() as u16
}

fn query(id: u16, recursion: bool, qname: &str) -> Vec<u8> {
    let mut m = Vec::with_capacity(16 + qname.len() + 2);
    m.extend_from_slice(&id.to_be_bytes());
    m.extend_from_slice(&(if recursion { 0x0100u16 } else { 0 }).to_be_bytes());
    m.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // QD=1, AN=NS=AR=0
    for label in qname.split('.') {
        m.push(label.len() as u8);
        m.extend_from_slice(label.as_bytes());
    }
    m.push(0);
    m.extend_from_slice(&TYPE_PTR.to_be_bytes());
    m.extend_from_slice(&CLASS_IN.to_be_bytes());
    m
}

#[derive(Debug, PartialEq, Eq)]
enum Reply {
    /// Not a reply to this query (wrong ID, not a response, malformed).
    Ignore,
    /// A reply that answers with no usable PTR (NXDOMAIN, truncated, ...).
    Negative,
    Host(String),
}

fn be16(msg: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *msg.get(at)?,
        *msg.get(at.checked_add(1)?)?,
    ]))
}

/// Interpret `msg` as the response to `query(id, _, qname)`. Follows an
/// in-message CNAME chain (RFC 2317 classless delegation) and only accepts
/// a PTR whose owner is the queried name or a CNAME target reached from it.
fn parse_reply(msg: &[u8], id: u16, qname: &str) -> Reply {
    if be16(msg, 0) != Some(id) {
        return Reply::Ignore;
    }
    let Some(flags) = be16(msg, 2) else {
        return Reply::Ignore;
    };
    if flags & 0x8000 == 0 {
        return Reply::Ignore; // a query, not a response
    }
    if flags & 0x0200 != 0 || flags & 0x000f != 0 {
        return Reply::Negative; // truncated or RCODE != NOERROR
    }
    match ptr_records(msg) {
        None => Reply::Ignore,
        Some(records) => follow(&records, qname).map_or(Reply::Negative, Reply::Host),
    }
}

/// (type, owner, target) for IN-class PTR and CNAME answers.
fn ptr_records(msg: &[u8]) -> Option<Vec<(u16, String, String)>> {
    let qd = be16(msg, 4)?;
    let an = be16(msg, 6)?;
    let mut pos = 12usize;
    for _ in 0..qd {
        let (_, next) = read_name(msg, pos)?;
        pos = next.checked_add(4)?;
    }
    if pos > msg.len() {
        return None;
    }
    let mut records = Vec::new();
    for _ in 0..an {
        let (owner, next) = read_name(msg, pos)?;
        let rtype = be16(msg, next)?;
        // Mask the mDNS cache-flush bit from the class field.
        let class = be16(msg, next.checked_add(2)?)? & 0x7fff;
        let rdlen = usize::from(be16(msg, next.checked_add(8)?)?);
        let rdata = next.checked_add(10)?;
        let end = rdata.checked_add(rdlen)?;
        if end > msg.len() {
            return None;
        }
        if class == CLASS_IN && (rtype == TYPE_PTR || rtype == TYPE_CNAME) {
            let (target, after) = read_name(msg, rdata)?;
            if after > end {
                return None;
            }
            records.push((rtype, owner, target));
        }
        pos = end;
    }
    Some(records)
}

fn follow(records: &[(u16, String, String)], qname: &str) -> Option<String> {
    let mut wanted = qname;
    for _ in 0..8 {
        let owned_by = |t: u16| {
            records
                .iter()
                .find(|(rt, owner, _)| *rt == t && owner.eq_ignore_ascii_case(wanted))
        };
        if let Some((_, _, host)) = owned_by(TYPE_PTR) {
            return Some(host.clone());
        }
        wanted = &owned_by(TYPE_CNAME)?.2;
    }
    None
}

/// Decode a possibly compressed name starting at `start`. Returns the
/// dotted name and the offset just past it in the original record. Labels
/// must be printable ASCII without dots, so a single label cannot pose as
/// several; pointers must point backwards and the walk is bounded.
fn read_name(msg: &[u8], start: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut pos = start;
    let mut after = None;
    for _ in 0..128 {
        let len = usize::from(*msg.get(pos)?);
        match len & 0xc0 {
            0x00 if len == 0 => return Some((name, after.unwrap_or(pos + 1))),
            0x00 => {
                let label = msg.get(pos + 1..pos + 1 + len)?;
                if !label.iter().all(|b| b.is_ascii_graphic() && *b != b'.') {
                    return None;
                }
                if !name.is_empty() {
                    name.push('.');
                }
                name.extend(label.iter().map(|&b| char::from(b)));
                if name.len() > 255 {
                    return None;
                }
                pos += 1 + len;
            }
            0xc0 => {
                let target = ((len & 0x3f) << 8) | usize::from(*msg.get(pos + 1)?);
                after.get_or_insert(pos + 2);
                if target >= pos {
                    return None;
                }
                pos = target;
            }
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn reverse_names_cover_v4_v6_and_mapped() {
        assert_eq!(reverse_name(v4(192, 0, 2, 1)), "1.2.0.192.in-addr.arpa");
        let mapped = IpAddr::V6(Ipv4Addr::new(192, 0, 2, 1).to_ipv6_mapped());
        assert_eq!(reverse_name(mapped), "1.2.0.192.in-addr.arpa");
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        assert_eq!(
            reverse_name(v6),
            "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.ip6.arpa"
        );
    }

    #[test]
    fn only_lan_goes_to_mdns_and_only_global_may_go_public() {
        for ip in [
            v4(10, 0, 0, 5),
            v4(172, 16, 1, 1),
            v4(192, 168, 1, 20),
            v4(169, 254, 3, 4),
        ] {
            assert_eq!(scope(ip), Scope::Lan, "{ip}");
        }
        let mapped = IpAddr::V6(Ipv4Addr::new(192, 168, 1, 20).to_ipv6_mapped());
        assert_eq!(scope(mapped), Scope::Lan);
        for ip in [v4(8, 8, 8, 8), v4(17, 253, 144, 10)] {
            assert_eq!(scope(ip), Scope::Global, "{ip}");
        }
        assert_eq!(scope("2606:4700::1111".parse().unwrap()), Scope::Global);
        for ip in [
            v4(127, 0, 0, 1),
            v4(0, 0, 0, 0),
            v4(224, 0, 0, 251),
            v4(255, 255, 255, 255),
            v4(192, 0, 2, 1),
            v4(198, 51, 100, 7),
            v4(203, 0, 113, 10),
            v4(100, 64, 0, 1),
            v4(198, 18, 0, 1),
            v4(192, 0, 0, 9),
            v4(240, 0, 0, 1),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            "fe80::1".parse().unwrap(),
            "fd00::1".parse().unwrap(),
            "2001:db8::1".parse().unwrap(),
            "ff02::fb".parse().unwrap(),
        ] {
            assert_eq!(scope(ip), Scope::Other, "{ip}");
        }
    }

    #[test]
    fn query_wire_format_is_a_single_ptr_question() {
        let q = query(0xabcd, true, "1.2.0.192.in-addr.arpa");
        assert_eq!(&q[..12], &[0xab, 0xcd, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            &q[12..],
            b"\x011\x012\x010\x03192\x07in-addr\x04arpa\x00\x00\x0c\x00\x01"
        );
        assert_eq!(&query(7, false, "a")[2..4], &[0, 0]); // mDNS: no RD bit
    }

    /// Reply to `query(id, _, qname)` with the given answers, using a
    /// compression pointer back to the question name where `owner` is None.
    fn reply(
        id: u16,
        flags: u16,
        qname: &str,
        answers: &[(Option<&str>, u16, u16, &str)],
    ) -> Vec<u8> {
        let mut m = query(id, false, qname);
        m[2..4].copy_from_slice(&flags.to_be_bytes());
        m[6..8].copy_from_slice(&(answers.len() as u16).to_be_bytes());
        for (owner, rtype, class, target) in answers {
            match owner {
                None => m.extend_from_slice(&[0xc0, 12]),
                Some(o) => m.extend_from_slice(&wire_name(o)),
            }
            m.extend_from_slice(&rtype.to_be_bytes());
            m.extend_from_slice(&class.to_be_bytes());
            m.extend_from_slice(&[0, 0, 0, 60]);
            let rdata = wire_name(target);
            m.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
            m.extend_from_slice(&rdata);
        }
        m
    }

    /// Uncompressed wire-format name (labels plus the root terminator).
    fn wire_name(name: &str) -> Vec<u8> {
        let q = query(0, false, name);
        q[12..q.len() - 4].to_vec()
    }

    const Q: &str = "20.1.168.192.in-addr.arpa";

    #[test]
    fn parses_direct_cnamed_and_mdns_answers() {
        let ok = reply(9, 0x8180, Q, &[(None, TYPE_PTR, CLASS_IN, "printer.local")]);
        assert_eq!(parse_reply(&ok, 9, Q), Reply::Host("printer.local".into()));
        // mDNS sets the cache-flush bit on the class.
        let mdns = reply(9, 0x8400, Q, &[(None, TYPE_PTR, 0x8001, "nas.local")]);
        assert_eq!(parse_reply(&mdns, 9, Q), Reply::Host("nas.local".into()));
        let chained = reply(
            9,
            0x8180,
            Q,
            &[
                (None, TYPE_CNAME, CLASS_IN, "20.0-63.1.168.192.in-addr.arpa"),
                (
                    Some("20.0-63.1.168.192.in-addr.arpa"),
                    TYPE_PTR,
                    CLASS_IN,
                    "host.example",
                ),
            ],
        );
        assert_eq!(
            parse_reply(&chained, 9, Q),
            Reply::Host("host.example".into())
        );
    }

    #[test]
    fn rejects_foreign_negative_and_malformed_replies() {
        let ok = reply(9, 0x8180, Q, &[(None, TYPE_PTR, CLASS_IN, "printer.local")]);
        assert_eq!(parse_reply(&ok, 10, Q), Reply::Ignore); // wrong ID
        let mut as_query = ok.clone();
        as_query[2] &= 0x7f;
        assert_eq!(parse_reply(&as_query, 9, Q), Reply::Ignore);
        assert_eq!(parse_reply(&ok[..ok.len() - 3], 9, Q), Reply::Ignore); // cut short
        assert_eq!(
            parse_reply(&reply(9, 0x8183, Q, &[]), 9, Q),
            Reply::Negative
        ); // NXDOMAIN
        assert_eq!(
            parse_reply(
                &reply(9, 0x8380, Q, &[(None, TYPE_PTR, CLASS_IN, "x.example")]),
                9,
                Q
            ),
            Reply::Negative // truncated
        );
        // A PTR for some other name is not an answer to this question.
        let other = reply(
            9,
            0x8180,
            Q,
            &[(
                Some("1.1.1.1.in-addr.arpa"),
                TYPE_PTR,
                CLASS_IN,
                "evil.example",
            )],
        );
        assert_eq!(parse_reply(&other, 9, Q), Reply::Negative);
        // Pointer loop / forward pointer.
        let mut looped = reply(9, 0x8180, Q, &[]);
        looped[6..8].copy_from_slice(&1u16.to_be_bytes());
        let at = looped.len();
        looped.extend_from_slice(&[0xc0, (at & 0xff) as u8]);
        assert_eq!(parse_reply(&looped, 9, Q), Reply::Ignore);
        // A label containing a dot cannot pose as two labels.
        let mut dotted = reply(9, 0x8180, Q, &[(None, TYPE_PTR, CLASS_IN, "aaaa.example")]);
        let i = dotted.len() - 13; // inside "aaaa"
        dotted[i] = b'.';
        assert_eq!(parse_reply(&dotted, 9, Q), Reply::Ignore);
    }

    #[test]
    fn exchange_accepts_only_the_expected_server() {
        use std::thread;
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let spoofer = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ip = v4(192, 168, 1, 20);
        let worker = thread::spawn(move || {
            for host in ["spoofed.example", "real.example"] {
                let mut buf = [0u8; 512];
                let (_, client) = server.recv_from(&mut buf).unwrap();
                let id = u16::from_be_bytes([buf[0], buf[1]]);
                let answer = reply(id, 0x8180, Q, &[(None, TYPE_PTR, CLASS_IN, host)]);
                if host == "spoofed.example" {
                    // Right ID, wrong source: must be ignored, then time out.
                    spoofer.send_to(&answer, client).unwrap();
                } else {
                    server.send_to(&answer, client).unwrap();
                }
            }
        });
        let wait = Duration::from_millis(400);
        assert_eq!(
            exchange(addr, true, ip, wait, |s| s == addr),
            Outcome::Timeout
        );
        assert_eq!(
            exchange(addr, true, ip, wait, |s| s == addr),
            Outcome::Host("real.example".into())
        );
        worker.join().unwrap();
    }
}
