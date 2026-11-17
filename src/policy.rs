//! An operator-written network policy, checked against observed endpoints.
//!
//! The policy is default-deny over two things a watched process can do:
//! connect to a peer, and wait for peers (listen on TCP, or bind an
//! unconnected UDP socket to a port). Each needs a rule. Nothing ships
//! with a rule in it: what a process may reach is the operator's decision,
//! and `docs/examples/` only holds starting points.
//!
//! ```text
//! netboard-policy 1
//! # A local model server, and nothing else.
//! allow loopback proto tcp port 11434
//! listen loopback
//! ```
//!
//! A rule is a verb, a target, and optional `proto` and `port` qualifiers:
//!
//! - `allow <target>` permits connections whose peer matches. The target is
//!   `any`, an address class (`loopback`, `lan`, `public`, `multicast`,
//!   `other`), an address, or a network in CIDR form. `port` is the peer's.
//! - `listen <target>` permits receivers whose local address matches. The
//!   target may also be `wildcard` (`0.0.0.0` or `::`). `port` is local.
//!
//! A policy describes what was seen, not what was prevented. A watch
//! samples, so a connection shorter than the interval can pass unseen; the
//! verdict says "no violation seen", never "no violation".

use std::io::Read;
use std::net::IpAddr;
use std::path::Path;

use crate::netclass::{self, Class};
use crate::snapshot::{Endpoint, Proto};

/// The file must start with this line, so a stray file is not a policy.
pub const HEADER: &str = "netboard-policy 1";
pub const MAX_BYTES: u64 = 64 * 1024;
pub const MAX_RULES: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Breach {
    /// A connection to a peer no `allow` rule permits.
    Connection,
    /// A receiver whose local address no `listen` rule permits.
    Listener,
}

impl Breach {
    pub fn as_str(self) -> &'static str {
        match self {
            Breach::Connection => "connection",
            Breach::Listener => "listener",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Allow,
    Listen,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Any,
    Class(Class),
    Network(IpAddr, u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Rule {
    verb: Verb,
    target: Target,
    proto: Option<Proto>,
    ports: Option<(u16, u16)>,
}

impl Rule {
    fn matches(&self, proto: Proto, addr: std::net::SocketAddr) -> bool {
        if self.proto.is_some_and(|wanted| wanted != proto) {
            return false;
        }
        if self
            .ports
            .is_some_and(|(low, high)| !(low..=high).contains(&addr.port()))
        {
            return false;
        }
        match self.target {
            Target::Any => true,
            Target::Class(class) => netclass::class(addr.ip()) == class,
            Target::Network(net, prefix) => netclass::in_network(addr.ip(), net, prefix),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Policy {
    rules: Vec<Rule>,
}

impl Policy {
    /// Read a policy file: a regular file of at most `MAX_BYTES` of UTF-8.
    /// Errors name the file's problem and the line, never echo its text.
    pub fn load(path: &Path) -> Result<Policy, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("--policy: {e}"))?;
        if !meta.is_file() {
            return Err("--policy: not a regular file".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|e| format!("--policy: {e}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(format!("--policy: larger than {MAX_BYTES} bytes"));
        }
        let text = String::from_utf8(bytes).map_err(|_| "--policy: not UTF-8".to_string())?;
        Policy::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Policy, String> {
        let mut rules = Vec::new();
        let mut header = false;
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            let words: Vec<&str> = line
                .split('#')
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .collect();
            if words.is_empty() {
                continue;
            }
            if !header {
                if words.join(" ") != HEADER {
                    return Err(format!(
                        "--policy line {number}: the first rule line must be `{HEADER}`"
                    ));
                }
                header = true;
                continue;
            }
            rules.push(rule(&words).map_err(|what| format!("--policy line {number}: {what}"))?);
            if rules.len() > MAX_RULES {
                return Err(format!("--policy: more than {MAX_RULES} rules"));
            }
        }
        if !header {
            return Err(format!("--policy: missing `{HEADER}` header"));
        }
        Ok(Policy { rules })
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// What this endpoint does that no rule permits, if anything. An
    /// endpoint that neither has a peer nor waits for one needs no rule.
    pub fn check(&self, e: &Endpoint) -> Option<Breach> {
        let (verb, addr, breach) = if netclass::has_peer(e) {
            (Verb::Allow, e.key.remote, Breach::Connection)
        } else if netclass::is_receiver(e) {
            (Verb::Listen, e.key.local, Breach::Listener)
        } else {
            return None;
        };
        let permitted = self
            .rules
            .iter()
            .any(|rule| rule.verb == verb && rule.matches(e.key.proto, addr));
        (!permitted).then_some(breach)
    }
}

fn rule(words: &[&str]) -> Result<Rule, String> {
    let verb = match words[0] {
        "allow" => Verb::Allow,
        "listen" => Verb::Listen,
        _ => return Err("a rule starts with `allow` or `listen`".into()),
    };
    let target = words
        .get(1)
        .ok_or_else(|| "the rule needs a target".to_string())
        .and_then(|word| target(verb, word))?;
    let mut proto = None;
    let mut ports = None;
    let mut rest = words[2..].iter();
    while let Some(word) = rest.next() {
        match *word {
            "proto" if proto.is_none() => {
                proto = Some(match rest.next().copied() {
                    Some("tcp") => Proto::Tcp,
                    Some("udp") => Proto::Udp,
                    _ => return Err("`proto` takes `tcp` or `udp`".into()),
                });
            }
            "port" if ports.is_none() => {
                ports = Some(
                    rest.next()
                        .and_then(|text| port_range(text))
                        .ok_or_else(|| {
                            "`port` takes a port or a range like 8000-8999, from 1 to 65535"
                                .to_string()
                        })?,
                );
            }
            "proto" | "port" => return Err("a qualifier appears twice".into()),
            _ => return Err("after the target only `proto` and `port` may follow".into()),
        }
    }
    Ok(Rule {
        verb,
        target,
        proto,
        ports,
    })
}

fn target(verb: Verb, word: &str) -> Result<Target, String> {
    if word == "any" {
        return Ok(Target::Any);
    }
    if let Some(class) = Class::from_word(word) {
        if class == Class::Wildcard && verb == Verb::Allow {
            return Err("`wildcard` is a local binding; it is a `listen` target only".into());
        }
        return Ok(Target::Class(class));
    }
    let (address, prefix) = match word.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (word, None),
    };
    let ip: IpAddr = address
        .parse()
        .map_err(|_| "the target is not `any`, a class, an address or a network".to_string())?;
    let ip = netclass::canonical(ip);
    let width = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix {
        None => width,
        Some(text) => text
            .parse::<u8>()
            .ok()
            .filter(|bits| *bits <= width)
            .ok_or_else(|| format!("the network prefix must be 0 to {width}"))?,
    };
    Ok(Target::Network(ip, prefix))
}

fn port_range(text: &str) -> Option<(u16, u16)> {
    let one = |part: &str| part.parse::<u16>().ok().filter(|port| *port != 0);
    match text.split_once('-') {
        Some((low, high)) => {
            let (low, high) = (one(low)?, one(high)?);
            (low <= high).then_some((low, high))
        }
        None => one(text).map(|port| (port, port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Dir, EndpointKey, IpVer, TcpState};
    use std::net::SocketAddr;

    fn ep(proto: Proto, local: &str, remote: &str, state: Option<TcpState>) -> Endpoint {
        let local: SocketAddr = local.parse().unwrap();
        Endpoint {
            key: EndpointKey {
                pid: 1,
                proto,
                ip_ver: if local.is_ipv4() {
                    IpVer::V4
                } else {
                    IpVer::V6
                },
                local,
                remote: remote.parse().unwrap(),
            },
            state,
            dir: Dir::Unknown,
            process: "p".into(),
            path: String::new(),
        }
    }

    fn connect(remote: &str) -> Endpoint {
        ep(
            Proto::Tcp,
            "192.168.1.5:50000",
            remote,
            Some(TcpState::Established),
        )
    }

    fn listen(local: &str) -> Endpoint {
        let any = if local.starts_with('[') {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        ep(Proto::Tcp, local, any, Some(TcpState::Listen))
    }

    fn policy(rules: &str) -> Policy {
        Policy::parse(&format!("{HEADER}\n{rules}")).unwrap()
    }

    #[test]
    fn an_empty_policy_denies_every_connection_and_receiver() {
        let p = policy("");
        assert!(p.is_empty());
        assert_eq!(p.check(&connect("127.0.0.1:80")), Some(Breach::Connection));
        assert_eq!(p.check(&listen("127.0.0.1:8080")), Some(Breach::Listener));
        let udp = ep(Proto::Udp, "0.0.0.0:5353", "0.0.0.0:0", None);
        assert_eq!(p.check(&udp), Some(Breach::Listener));
        // Waiting for nobody and talking to nobody needs no rule.
        let unbound = ep(Proto::Udp, "0.0.0.0:0", "0.0.0.0:0", None);
        assert_eq!(p.check(&unbound), None);
        let closed = ep(
            Proto::Tcp,
            "0.0.0.0:7000",
            "0.0.0.0:0",
            Some(TcpState::Closed),
        );
        assert_eq!(p.check(&closed), None);
    }

    #[test]
    fn allow_rules_match_class_address_network_proto_and_port() {
        let p = policy(
            "allow loopback proto tcp port 11434\n\
             allow 192.168.1.1 proto udp port 53\n\
             allow 10.0.0.0/8 port 8000-8999   # build cache\n\
             allow 2606:4700::/32\n",
        );
        assert_eq!(p.len(), 4);
        assert_eq!(p.check(&connect("127.0.0.1:11434")), None);
        assert_eq!(p.check(&connect("[::1]:11434")), None);
        assert_eq!(p.check(&connect("[::ffff:127.0.0.1]:11434")), None);
        assert_eq!(
            p.check(&connect("127.0.0.1:11435")),
            Some(Breach::Connection)
        );
        let dns = ep(Proto::Udp, "192.168.1.5:50001", "192.168.1.1:53", None);
        assert_eq!(p.check(&dns), None);
        // The same peer over TCP is a different rule.
        assert_eq!(
            p.check(&connect("192.168.1.1:53")),
            Some(Breach::Connection)
        );
        assert_eq!(p.check(&connect("10.20.30.40:8443")), None);
        assert_eq!(
            p.check(&connect("10.20.30.40:9000")),
            Some(Breach::Connection)
        );
        assert_eq!(p.check(&connect("11.0.0.1:8443")), Some(Breach::Connection));
        assert_eq!(p.check(&connect("[2606:4700::1111]:443")), None);
        assert_eq!(p.check(&connect("8.8.8.8:443")), Some(Breach::Connection));
        // An allow rule never covers a listener.
        assert_eq!(p.check(&listen("127.0.0.1:11434")), Some(Breach::Listener));
    }

    #[test]
    fn listen_rules_match_the_local_binding() {
        let p = policy("listen loopback\nlisten wildcard proto udp port 5353\n");
        assert_eq!(p.check(&listen("127.0.0.1:8080")), None);
        assert_eq!(p.check(&listen("[::1]:8080")), None);
        assert_eq!(p.check(&listen("0.0.0.0:8080")), Some(Breach::Listener));
        assert_eq!(p.check(&listen("[::]:8080")), Some(Breach::Listener));
        assert_eq!(p.check(&listen("192.168.1.5:8080")), Some(Breach::Listener));
        let mdns = ep(Proto::Udp, "0.0.0.0:5353", "0.0.0.0:0", None);
        assert_eq!(p.check(&mdns), None);
        let other = ep(Proto::Udp, "0.0.0.0:5354", "0.0.0.0:0", None);
        assert_eq!(p.check(&other), Some(Breach::Listener));
        // A listen rule never covers a connection.
        assert_eq!(p.check(&connect("127.0.0.1:80")), Some(Breach::Connection));
        let any = policy("allow any\nlisten any\n");
        assert_eq!(any.check(&connect("8.8.8.8:443")), None);
        assert_eq!(any.check(&listen("0.0.0.0:22")), None);
    }

    #[test]
    fn malformed_policies_are_refused_with_the_line_and_not_the_text() {
        for (text, want) in [
            ("", "missing `netboard-policy 1` header"),
            ("# only a comment\n", "missing `netboard-policy 1` header"),
            ("allow any\n", "line 1: the first rule line must be"),
            ("netboard-policy 2\n", "line 1: the first rule line must be"),
            (
                "netboard-policy 1\nallow\n",
                "line 2: the rule needs a target",
            ),
            (
                "netboard-policy 1\n\npermit any\n",
                "line 3: a rule starts with",
            ),
            (
                "netboard-policy 1\nallow wildcard\n",
                "line 2: `wildcard` is a local binding",
            ),
            (
                "netboard-policy 1\nallow evil.example\n",
                "line 2: the target is not",
            ),
            (
                "netboard-policy 1\nallow 10.0.0.0/33\n",
                "line 2: the network prefix must be 0 to 32",
            ),
            (
                "netboard-policy 1\nallow ::/129\n",
                "line 2: the network prefix must be 0 to 128",
            ),
            (
                "netboard-policy 1\nallow any proto icmp\n",
                "line 2: `proto` takes",
            ),
            (
                "netboard-policy 1\nallow any port 0\n",
                "line 2: `port` takes",
            ),
            (
                "netboard-policy 1\nallow any port 9-1\n",
                "line 2: `port` takes",
            ),
            (
                "netboard-policy 1\nallow any port 70000\n",
                "line 2: `port` takes",
            ),
            (
                "netboard-policy 1\nallow any port\n",
                "line 2: `port` takes",
            ),
            (
                "netboard-policy 1\nallow any port 1 port 2\n",
                "line 2: a qualifier appears twice",
            ),
            (
                "netboard-policy 1\nallow any via proxy\n",
                "line 2: after the target only",
            ),
        ] {
            let err = Policy::parse(text).unwrap_err();
            assert!(err.contains(want), "{text:?}: {err}");
            for secret in ["evil.example", "permit", "icmp", "proxy", "70000"] {
                assert!(!err.contains(secret), "{err} echoes input");
            }
        }
        let many = format!("{HEADER}\n{}", "allow any\n".repeat(MAX_RULES + 1));
        assert!(Policy::parse(&many)
            .unwrap_err()
            .contains("more than 512 rules"));
        assert_eq!(policy(&"allow any\n".repeat(MAX_RULES)).len(), MAX_RULES);
    }

    #[test]
    fn load_reads_a_bounded_regular_utf8_file() {
        let dir = std::env::temp_dir().join(format!("netboard-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.policy");
        std::fs::write(&good, format!("{HEADER}\nlisten loopback\n")).unwrap();
        assert_eq!(Policy::load(&good).unwrap().len(), 1);
        let big = dir.join("big.policy");
        std::fs::write(&big, vec![b'#'; MAX_BYTES as usize + 1]).unwrap();
        assert!(Policy::load(&big)
            .unwrap_err()
            .contains("larger than 65536 bytes"));
        let binary = dir.join("binary.policy");
        std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(Policy::load(&binary).unwrap_err(), "--policy: not UTF-8");
        assert_eq!(
            Policy::load(&dir).unwrap_err(),
            "--policy: not a regular file"
        );
        assert!(Policy::load(&dir.join("absent")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
