//! Headless watch: sample sockets on an interval and write JSON Lines events.
//!
//! Events are differences between consecutive samples. A socket that opens
//! and closes between two samples is never reported, so the stream can show
//! that something happened but cannot show that nothing did. The closing
//! record states the longest gap between samples for that reason.
//!
//! Watch output is always numeric: this module has no name-resolution path.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::cli::Filter;
use crate::netclass::{self, Class};
use crate::peers::{local_peers, Via};
use crate::policy::{Breach, Policy};
use crate::procinfo::{self, ProcIdentity, ProcStart};
use crate::proctree::{Change, Tracker};
use crate::snapshot::{
    is_offbox, is_spoofing_char, numeric_ip, process_names, Dir, Endpoint, EndpointKey, IpVer,
    Proto, Sample, TcpState,
};

/// Bumped when a record gains, loses or changes the meaning of a field.
pub const SCHEMA: &str = "netboard.watch/1";
pub const DEFAULT_INTERVAL_MS: u64 = 1000;
pub const MIN_INTERVAL_MS: u64 = 50;
pub const MAX_INTERVAL_MS: u64 = 60_000;
pub const MAX_DURATION_S: u64 = 86_400;
pub const MAX_ROOTS: usize = 16;

pub struct Config {
    pub interval: Duration,
    pub duration: Option<Duration>,
    /// Empty means every process; otherwise these and their descendants.
    pub roots: Vec<ProcIdentity>,
    pub filter: Option<Filter>,
    /// Checked against every endpoint in scope; the end record gives a verdict.
    pub policy: Option<Policy>,
}

/// The answer to "did the watched scope stay inside the policy?".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Every sample succeeded, every process in scope was readable, and
    /// nothing seen broke the policy. Not proof: see the sampling gap.
    Pass,
    /// Something seen broke the policy. Takes precedence over Incomplete:
    /// what was missed can only add to what was seen.
    Violation,
    /// Nothing seen broke the policy, but part of the scope went unseen.
    Incomplete,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Violation => "violation",
            Verdict::Incomplete => "incomplete",
        }
    }

    /// The process exit code, matching yara-gate's: 0, 1, 2.
    pub fn exit_code(self) -> i32 {
        match self {
            Verdict::Pass => 0,
            Verdict::Violation => 1,
            Verdict::Incomplete => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndReason {
    Duration,
    Interrupted,
    ScopeExited,
}

impl EndReason {
    fn as_str(self) -> &'static str {
        match self {
            EndReason::Duration => "duration",
            EndReason::Interrupted => "interrupted",
            EndReason::ScopeExited => "scope_exited",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Summary {
    pub reason: EndReason,
    /// Samples that succeeded.
    pub samples: u64,
    /// Samples that failed. Nothing was observed for those intervals.
    pub failed: u64,
    /// Distinct processes in scope whose sockets could not be read.
    pub unreadable: usize,
    /// Endpoints that broke the policy, each counted once.
    pub violations: u64,
    /// `None` without a policy.
    pub verdict: Option<Verdict>,
}

/// Everything the loop takes from the machine, so tests can script it.
pub trait Source {
    /// Endpoints, and the processes whose sockets could not be read.
    fn endpoints(&mut self) -> Result<Sample, String>;
    fn processes(&mut self) -> Result<Vec<ProcIdentity>, String>;
    /// Child PIDs of `pid`, readable or not.
    fn children(&mut self, pid: u32) -> Vec<u32>;
    fn identity(&mut self, pid: u32) -> Option<ProcIdentity>;
    /// Current process name and executable path.
    fn names(&mut self, pid: u32) -> (String, String);
    fn wall(&mut self) -> SystemTime;
    /// Monotonic time since the source was created.
    fn elapsed(&mut self) -> Duration;
    /// Wait this long. `false` means a stop was requested.
    fn wait(&mut self, time: Duration) -> bool;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Existing,
    New,
    Changed(Option<TcpState>),
    Closed,
}

type Key = (EndpointKey, Option<ProcStart>);

/// A process holding the other end of a connection on this machine.
#[derive(Clone, PartialEq, Eq, Debug)]
struct PeerProcess {
    pid: u32,
    start: Option<ProcStart>,
    name: String,
    path: String,
    /// False when the watch is scoped to a tree and this process is outside it.
    in_scope: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Peer {
    via: Via,
    processes: Vec<PeerProcess>,
    total: usize,
}

/// An endpoint as one sample saw it.
struct Seen {
    endpoint: Endpoint,
    peer: Option<Peer>,
}

/// Run until the duration ends, a stop is requested, or the watched tree
/// has exited. A write error ends the watch and is returned to the caller.
pub fn run<S: Source, W: Write>(cfg: &Config, src: &mut S, out: &mut W) -> io::Result<Summary> {
    let began = src.elapsed();
    writeln!(out, "{}", start_line(cfg, &timestamp(src.wall())))?;
    out.flush()?;

    let mut tracker = (!cfg.roots.is_empty()).then(|| Tracker::new(&cfg.roots));
    // Name and path as first seen, kept so an exit can still name the process.
    let mut tracked: HashMap<u32, (String, String)> = HashMap::new();
    let mut prev: HashMap<Key, Seen> = HashMap::new();
    let mut baseline = true;
    let (mut samples, mut failed) = (0u64, 0u64);
    let mut last_ok: Option<Duration> = None;
    let mut max_gap: Option<Duration> = None;
    // Processes in scope that could not be read, each reported once.
    let mut unreadable: HashSet<u32> = HashSet::new();
    let scoped = tracker.is_some() || cfg.filter.is_some();
    // Endpoints already reported as violations.
    let mut violated: HashSet<Key> = HashSet::new();
    let mut violations = 0u64;

    let reason = loop {
        let at = src.elapsed();
        match sample(src, tracker.as_mut(), cfg, &prev) {
            Ok((changes, next, gaps)) => {
                samples += 1;
                if let Some(before) = last_ok {
                    let gap = at.saturating_sub(before);
                    max_gap = Some(max_gap.map_or(gap, |longest| longest.max(gap)));
                }
                last_ok = Some(at);
                let stamp = timestamp(src.wall());
                for change in &changes {
                    let (identity, reason) = match change {
                        Change::Root(identity) => (identity, "root"),
                        Change::Joined(identity) => (identity, "descendant"),
                        Change::Exited(_) => continue,
                    };
                    let names = src.names(identity.pid);
                    let line = process_line(&stamp, "tracked", Some(reason), identity, &names);
                    writeln!(out, "{line}")?;
                    tracked.insert(identity.pid, names);
                }
                for pid in gaps {
                    // An unscoped watch run without root cannot read most
                    // system processes; it counts them rather than list
                    // hundreds. A scoped watch names each one.
                    if unreadable.insert(pid) && scoped {
                        let names = src.names(pid);
                        writeln!(out, "{}", unreadable_line(&stamp, pid, &names))?;
                    }
                }
                for (kind, seen, start) in delta(&prev, &next, baseline) {
                    writeln!(out, "{}", endpoint_line(&stamp, kind, seen, start))?;
                    let Some(policy) = &cfg.policy else {
                        continue;
                    };
                    if kind == Kind::Closed {
                        continue;
                    }
                    let key = (seen.endpoint.key.clone(), start);
                    if violated.contains(&key) {
                        continue;
                    }
                    // A state change is checked too: a socket can start
                    // listening on a port it bound earlier.
                    if let Some(breach) = policy.check(&seen.endpoint) {
                        writeln!(out, "{}", violation_line(&stamp, breach, seen, start))?;
                        violated.insert(key);
                        violations += 1;
                    }
                }
                prev = next;
                for change in &changes {
                    if let Change::Exited(identity) = change {
                        let names = tracked.remove(&identity.pid).unwrap_or_default();
                        let line = process_line(&stamp, "exited", None, identity, &names);
                        writeln!(out, "{line}")?;
                    }
                }
                if baseline {
                    // Everything above this record was already there; every
                    // endpoint event below it is a change. A caller that wants
                    // to observe an action starts it after reading this.
                    baseline = false;
                    let processes = tracker.as_ref().map(Tracker::len);
                    let line = baseline_line(&stamp, prev.len(), processes, unreadable.len());
                    writeln!(out, "{line}")?;
                }
            }
            Err(message) => {
                failed += 1;
                writeln!(out, "{}", error_line(&timestamp(src.wall()), &message))?;
            }
        }
        out.flush()?;
        if tracker.as_ref().is_some_and(Tracker::is_empty) {
            break EndReason::ScopeExited;
        }

        let now = src.elapsed().saturating_sub(began);
        if cfg.duration.is_some_and(|limit| now >= limit) {
            break EndReason::Duration;
        }
        // Fixed cadence from the start time. A sample that overran its slot
        // skips to the next future slot instead of sampling back to back.
        let slot = now.as_nanos() / cfg.interval.as_nanos().max(1) + 1;
        let mut due = cfg
            .interval
            .saturating_mul(u32::try_from(slot).unwrap_or(u32::MAX));
        if let Some(limit) = cfg.duration {
            // The last sample is taken at the limit, not one interval short.
            due = due.min(limit);
        }
        if !src.wait(due.saturating_sub(now)) {
            break EndReason::Interrupted;
        }
    };

    let elapsed = src.elapsed().saturating_sub(began);
    let summary = Summary {
        reason,
        samples,
        failed,
        unreadable: unreadable.len(),
        violations,
        verdict: None,
    };
    let gaps = incomplete_reasons(cfg, &summary);
    let summary = Summary {
        verdict: cfg.policy.as_ref().map(|_| {
            if violations > 0 {
                Verdict::Violation
            } else if gaps.is_empty() {
                Verdict::Pass
            } else {
                Verdict::Incomplete
            }
        }),
        ..summary
    };
    let line = end_line(&timestamp(src.wall()), &summary, &gaps, max_gap, elapsed);
    writeln!(out, "{line}")?;
    out.flush()?;
    Ok(summary)
}

/// Why the watch did not see its whole scope for its whole duration.
pub fn incomplete_reasons(cfg: &Config, summary: &Summary) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if summary.failed > 0 {
        reasons.push("failed_samples");
    }
    if summary.unreadable > 0 {
        reasons.push("unreadable_processes");
    }
    // Stopped before the time it was asked to cover.
    if cfg.duration.is_some() && summary.reason == EndReason::Interrupted {
        reasons.push("interrupted");
    }
    reasons
}

/// Process changes, the endpoints in scope, and the unreadable processes
/// in scope, from one sample.
type Observed = (Vec<Change>, HashMap<Key, Seen>, Vec<u32>);

/// One sample: sockets first, then the process table, so a failure of
/// either leaves the tracker untouched and the sample counts as failed.
fn sample<S: Source>(
    src: &mut S,
    tracker: Option<&mut Tracker>,
    cfg: &Config,
    prev: &HashMap<Key, Seen>,
) -> Result<Observed, String> {
    let Sample {
        mut endpoints,
        unreadable,
    } = src.endpoints()?;
    // Peers are matched on the whole sample, before any narrowing: the
    // other end of a watched connection is usually outside the scope.
    let mut peers = local_peers(&endpoints);
    let mut owners: HashMap<u32, (String, String)> = peers
        .values()
        .flat_map(|peer| &peer.pids)
        .map(|pid| (*pid, Default::default()))
        .collect();
    for e in &endpoints {
        if let Some(names) = owners.get_mut(&e.key.pid).filter(|n| n.0.is_empty()) {
            *names = (e.process.clone(), e.path.clone());
        }
    }

    let (changes, tree) = match tracker {
        Some(tree) => {
            let table = src.processes()?;
            let changes = tree.update(&table);
            endpoints.retain(|e| tree.contains(e.key.pid));
            (changes, Some(&*tree))
        }
        None => (Vec::new(), None),
    };
    if let Some(filter) = &cfg.filter {
        endpoints.retain(|e| filter.matches(e));
    }
    let gaps = gaps(src, tree, cfg.filter.as_ref(), unreadable);

    // A process can exit between the socket sample and its identity lookup.
    // Its endpoint keeps the start time it had last sample rather than
    // turning into a second endpoint with no identity.
    let known: HashMap<&EndpointKey, ProcStart> = prev
        .keys()
        .filter_map(|(key, start)| start.map(|s| (key, s)))
        .collect();
    let mut looked_up: HashMap<u32, Option<ProcStart>> = HashMap::new();
    let mut start_of = |pid: u32| match tree.and_then(|tree| tree.start_of(pid)) {
        Some(start) => Some(start),
        None => *looked_up
            .entry(pid)
            .or_insert_with(|| src.identity(pid).map(|identity| identity.start)),
    };
    let mut next = HashMap::with_capacity(endpoints.len());
    for endpoint in endpoints {
        let start = start_of(endpoint.key.pid).or_else(|| known.get(&endpoint.key).copied());
        let peer = peers.remove(&endpoint.key).map(|found| Peer {
            via: found.via,
            total: found.total,
            processes: found
                .pids
                .iter()
                .map(|&pid| {
                    let (name, path) = owners.get(&pid).cloned().unwrap_or_default();
                    PeerProcess {
                        pid,
                        start: start_of(pid),
                        name,
                        path,
                        in_scope: tree.is_none_or(|tree| tree.contains(pid)),
                    }
                })
                .collect(),
        });
        // Duplicated descriptors of one socket are one endpoint.
        next.entry((endpoint.key.clone(), start))
            .or_insert(Seen { endpoint, peer });
    }
    Ok((changes, next, gaps))
}

/// The unreadable processes that fall inside the scope. In a tree, that
/// is any descendant this user cannot read (a setuid program, or one
/// started under another user), found by walking child lists from the
/// members. Those descendants are never members: membership needs a
/// readable start time. Their own children are walked too.
fn gaps<S: Source>(
    src: &mut S,
    tree: Option<&Tracker>,
    filter: Option<&Filter>,
    unreadable: Vec<u32>,
) -> Vec<u32> {
    let unreadable: HashSet<u32> = unreadable.into_iter().collect();
    let mut found: Vec<u32> = match tree {
        None => unreadable.iter().copied().collect(),
        Some(tree) => {
            let mut found = HashSet::new();
            let mut queue: Vec<u32> = tree.pids().collect();
            while let Some(parent) = queue.pop() {
                for child in src.children(parent) {
                    if !tree.contains(child) && unreadable.contains(&child) && found.insert(child) {
                        queue.push(child);
                    }
                }
            }
            found.into_iter().collect()
        }
    };
    if let Some(filter) = filter {
        found.retain(|&pid| match filter {
            Filter::Pid(wanted) => pid == *wanted,
            // The name of another user's process may not be readable; its
            // executable path usually is.
            Filter::Name(name) => {
                let (process, path) = src.names(pid);
                let base = path.rsplit('/').next().unwrap_or_default();
                process.to_ascii_lowercase().contains(name)
                    || base.to_ascii_lowercase().contains(name)
            }
        });
    }
    found.sort_unstable();
    found
}

/// Events between two samples, in a stable order: by PID, protocol and
/// addresses, with a close ahead of an open of the same tuple. A peer that
/// appears or changes on an endpoint already reported is not an event.
fn delta<'a>(
    prev: &'a HashMap<Key, Seen>,
    next: &'a HashMap<Key, Seen>,
    baseline: bool,
) -> Vec<(Kind, &'a Seen, Option<ProcStart>)> {
    let mut events = Vec::new();
    for (key, seen) in next {
        let kind = match prev.get(key) {
            None if baseline => Kind::Existing,
            None => Kind::New,
            Some(old) if old.endpoint.state != seen.endpoint.state => {
                Kind::Changed(old.endpoint.state)
            }
            Some(_) => continue,
        };
        events.push((kind, seen, key.1));
    }
    for (key, seen) in prev {
        if !next.contains_key(key) {
            events.push((Kind::Closed, seen, key.1));
        }
    }
    events.sort_by_key(|(kind, seen, _)| {
        let rank = match kind {
            Kind::Closed => 0u8,
            Kind::Changed(_) => 1,
            Kind::Existing => 2,
            Kind::New => 3,
        };
        let key = &seen.endpoint.key;
        (
            key.pid,
            key.proto == Proto::Udp,
            key.local,
            key.remote,
            rank,
        )
    });
    events
}

// --- records ---------------------------------------------------------------

fn start_line(cfg: &Config, stamp: &str) -> String {
    let roots: Vec<String> = cfg
        .roots
        .iter()
        .map(|root| {
            Json::object()
                .number("pid", u64::from(root.pid))
                .string("start", &start_stamp(root.start))
                .finish()
        })
        .collect();
    let filter = match &cfg.filter {
        None => "null".to_string(),
        Some(Filter::Pid(pid)) => Json::object().number("pid", u64::from(*pid)).finish(),
        Some(Filter::Name(name)) => Json::object().string("name", name).finish(),
    };
    let duration = match cfg.duration {
        Some(limit) => limit.as_secs().to_string(),
        None => "null".into(),
    };
    Json::object()
        .string("type", "start")
        .string("schema", SCHEMA)
        .string("version", env!("CARGO_PKG_VERSION"))
        .string("ts", stamp)
        .number("interval_ms", millis(cfg.interval))
        .raw("duration_s", &duration)
        .raw("roots", &format!("[{}]", roots.join(",")))
        .raw("filter", &filter)
        .raw(
            "policy",
            &match &cfg.policy {
                Some(policy) => Json::object().number("rules", policy.len() as u64).finish(),
                None => "null".into(),
            },
        )
        // Chosen by whoever started the process, not by this tool.
        .raw(
            "untrusted_fields",
            r#"["process.name","process.path","peer.processes.name","peer.processes.path"]"#,
        )
        .finish()
}

fn process_line(
    stamp: &str,
    event: &str,
    reason: Option<&str>,
    identity: &ProcIdentity,
    names: &(String, String),
) -> String {
    let process = Json::object()
        .number("pid", u64::from(identity.pid))
        .number("ppid", u64::from(identity.ppid))
        .string("start", &start_stamp(identity.start))
        .string("name", &names.0)
        .string("path", &names.1)
        .finish();
    let mut line = Json::object()
        .string("type", "process")
        .string("ts", stamp)
        .string("event", event);
    if let Some(reason) = reason {
        line = line.string("reason", reason);
    }
    line.raw("process", &process).finish()
}

fn peer_json(peer: Option<&Peer>) -> String {
    let Some(peer) = peer else {
        return "null".into();
    };
    let processes: Vec<String> = peer
        .processes
        .iter()
        .map(|process| {
            Json::object()
                .number("pid", u64::from(process.pid))
                .optional("start", process.start.map(start_stamp).as_deref())
                .string("name", &process.name)
                .string("path", &process.path)
                .boolean("in_scope", process.in_scope)
                .finish()
        })
        .collect();
    Json::object()
        .string("via", peer.via.as_str())
        // Every distinct owner of the other end; `processes` lists the first few.
        .number("process_count", peer.total as u64)
        .raw("processes", &format!("[{}]", processes.join(",")))
        .finish()
}

fn endpoint_line(stamp: &str, kind: Kind, seen: &Seen, start: Option<ProcStart>) -> String {
    let e = &seen.endpoint;
    let event = match kind {
        Kind::Existing => "existing",
        Kind::New => "new",
        Kind::Changed(_) => "changed",
        Kind::Closed => "closed",
    };
    let mut line = Json::object()
        .string("type", "endpoint")
        .string("ts", stamp)
        .string("event", event)
        .raw("process", &process_json(e, start))
        .string("proto", proto_str(e.key.proto))
        .number(
            "ip_version",
            match e.key.ip_ver {
                IpVer::V4 => 4,
                IpVer::V6 => 6,
            },
        )
        .string(
            "dir",
            match e.dir {
                Dir::In => "in",
                Dir::Out => "out",
                Dir::Listen => "listen",
                Dir::Unknown => "unknown",
            },
        )
        .raw("local", &address_json(e.key.local))
        .raw("remote", &address_json(e.key.remote))
        .optional("state", e.state.map(TcpState::as_str));
    if let Kind::Changed(before) = kind {
        line = line.optional("prev_state", before.map(TcpState::as_str));
    }
    line.boolean("off_box", is_offbox(e.key.remote))
        .optional("remote_class", netclass::remote_class(e).map(Class::as_str))
        .string("local_class", netclass::class(e.key.local.ip()).as_str())
        .boolean("exposed", netclass::exposure(e).is_some())
        .raw("peer", &peer_json(seen.peer.as_ref()))
        .finish()
}

fn process_json(e: &Endpoint, start: Option<ProcStart>) -> String {
    Json::object()
        .number("pid", u64::from(e.key.pid))
        .optional("start", start.map(start_stamp).as_deref())
        .string("name", &e.process)
        .string("path", &e.path)
        .finish()
}

fn address_json(addr: std::net::SocketAddr) -> String {
    Json::object()
        .string("ip", &numeric_ip(addr.ip()))
        .number("port", u64::from(addr.port()))
        .finish()
}

fn proto_str(proto: Proto) -> &'static str {
    match proto {
        Proto::Tcp => "tcp",
        Proto::Udp => "udp",
    }
}

/// An endpoint no policy rule permits, written once per endpoint, right
/// after the record that reported it.
fn violation_line(stamp: &str, breach: Breach, seen: &Seen, start: Option<ProcStart>) -> String {
    let e = &seen.endpoint;
    Json::object()
        .string("type", "violation")
        .string("ts", stamp)
        .string("breach", breach.as_str())
        .raw("process", &process_json(e, start))
        .string("proto", proto_str(e.key.proto))
        .raw("local", &address_json(e.key.local))
        .raw("remote", &address_json(e.key.remote))
        .optional("state", e.state.map(TcpState::as_str))
        .optional("remote_class", netclass::remote_class(e).map(Class::as_str))
        .string("local_class", netclass::class(e.key.local.ip()).as_str())
        .finish()
}

/// A process in scope whose sockets this user cannot read. Its endpoints
/// are missing from the stream from here on.
fn unreadable_line(stamp: &str, pid: u32, names: &(String, String)) -> String {
    let process = Json::object()
        .number("pid", u64::from(pid))
        .string("name", &names.0)
        .string("path", &names.1)
        .finish();
    Json::object()
        .string("type", "process")
        .string("ts", stamp)
        .string("event", "unreadable")
        .raw("process", &process)
        .finish()
}

fn baseline_line(
    stamp: &str,
    endpoints: usize,
    processes: Option<usize>,
    unreadable: usize,
) -> String {
    let processes = match processes {
        Some(count) => count.to_string(),
        None => "null".into(),
    };
    Json::object()
        .string("type", "baseline")
        .string("ts", stamp)
        .number("endpoints", endpoints as u64)
        // Tracked processes when the watch is scoped to a tree.
        .raw("processes", &processes)
        .number("unreadable_processes", unreadable as u64)
        .finish()
}

fn error_line(stamp: &str, message: &str) -> String {
    Json::object()
        .string("type", "error")
        .string("ts", stamp)
        .string("message", message)
        .finish()
}

fn end_line(
    stamp: &str,
    summary: &Summary,
    incomplete: &[&str],
    max_gap: Option<Duration>,
    elapsed: Duration,
) -> String {
    let gap = match max_gap {
        Some(gap) => millis(gap).to_string(),
        None => "null".into(),
    };
    let policy = match summary.verdict {
        None => "null".to_string(),
        Some(verdict) => {
            let reasons: Vec<String> = incomplete.iter().map(|r| json_string(r)).collect();
            Json::object()
                .string("verdict", verdict.as_str())
                .number("violations", summary.violations)
                // Every reason the scope went partly unseen, even when a
                // violation decided the verdict.
                .raw("incomplete", &format!("[{}]", reasons.join(",")))
                .finish()
        }
    };
    Json::object()
        .string("type", "end")
        .string("ts", stamp)
        .string("reason", summary.reason.as_str())
        .number("samples", summary.samples)
        .number("failed_samples", summary.failed)
        .raw("max_gap_ms", &gap)
        .number("elapsed_ms", millis(elapsed))
        .number("unreadable_processes", summary.unreadable as u64)
        .raw("policy", &policy)
        .finish()
}

fn millis(time: Duration) -> u64 {
    u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
}

/// A flat JSON object. Keys are literals from this file; values go through
/// `json_string` or are numbers, booleans and objects built the same way.
struct Json(String);

impl Json {
    fn object() -> Self {
        Json("{".into())
    }

    fn key(mut self, key: &str) -> Self {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push('"');
        self.0.push_str(key);
        self.0.push_str("\":");
        self
    }

    fn raw(self, key: &str, json: &str) -> Self {
        let mut object = self.key(key);
        object.0.push_str(json);
        object
    }

    fn string(self, key: &str, value: &str) -> Self {
        self.raw(key, &json_string(value))
    }

    fn optional(self, key: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.string(key, value),
            None => self.raw(key, "null"),
        }
    }

    fn number(self, key: &str, value: u64) -> Self {
        self.raw(key, &value.to_string())
    }

    fn boolean(self, key: &str, value: bool) -> Self {
        self.raw(key, if value { "true" } else { "false" })
    }

    fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

/// A JSON string literal. Controls, bidi and zero-width formatting, and the
/// Unicode line separators are written as `\uXXXX`: a JSON parser reads the
/// exact value back, while the line stays one line and cannot reorder or
/// hide text in a terminal. Process names and paths are attacker-chosen.
pub fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if is_spoofing_char(c) || matches!(c, '\u{2028}' | '\u{2029}') => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// --- time ------------------------------------------------------------------

/// `YYYY-MM-DDTHH:MM:SS` in UTC for seconds since the Unix epoch.
fn date_time(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX / 2);
    let rem = secs % 86_400;
    // Civil-from-days, proleptic Gregorian (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// RFC 3339 UTC with milliseconds, for when a record was written.
fn timestamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    format!(
        "{}.{:03}Z",
        date_time(since.as_secs()),
        since.subsec_millis()
    )
}

/// RFC 3339 UTC with microseconds: the kernel's exact process start time,
/// which together with the PID identifies one process.
fn start_stamp(start: ProcStart) -> String {
    format!("{}.{:06}Z", date_time(start.sec), start.usec.min(999_999))
}

// --- the live machine ------------------------------------------------------

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn request_stop(_signal: libc::c_int) {
    // Only an atomic store: safe to run inside a signal handler.
    STOP.store(true, Ordering::SeqCst);
}

/// Live sockets and processes, the system clocks, and a stop flag set by
/// SIGINT or SIGTERM so an interrupted watch still writes its end record.
pub struct Live {
    began: Instant,
}

impl Live {
    pub fn new() -> Self {
        let handler = request_stop as *const () as libc::sighandler_t;
        // SAFETY: the handler only stores to an atomic. Replacing the default
        // action for these two signals affects this process alone.
        unsafe {
            libc::signal(libc::SIGINT, handler);
            libc::signal(libc::SIGTERM, handler);
        }
        Self {
            began: Instant::now(),
        }
    }
}

impl Default for Live {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for Live {
    fn endpoints(&mut self) -> Result<Sample, String> {
        // A panic in the collector must not end a long watch; the sample
        // is reported as failed instead.
        match std::panic::catch_unwind(crate::snapshot::sample) {
            Ok(result) => result.map_err(|e| e.0),
            Err(_) => Err("socket snapshot panicked".into()),
        }
    }

    fn processes(&mut self) -> Result<Vec<ProcIdentity>, String> {
        procinfo::table()
    }

    fn children(&mut self, pid: u32) -> Vec<u32> {
        procinfo::children(pid)
    }

    fn identity(&mut self, pid: u32) -> Option<ProcIdentity> {
        procinfo::identity(pid)
    }

    fn names(&mut self, pid: u32) -> (String, String) {
        process_names(pid)
    }

    fn wall(&mut self) -> SystemTime {
        SystemTime::now()
    }

    fn elapsed(&mut self) -> Duration {
        self.began.elapsed()
    }

    fn wait(&mut self, time: Duration) -> bool {
        let until = Instant::now() + time;
        loop {
            if STOP.load(Ordering::SeqCst) {
                return false;
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            // Short slices keep a stop request responsive.
            std::thread::sleep(left.min(Duration::from_millis(50)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    const EPOCH: u64 = 1_700_000_000; // 2023-11-14T22:13:20Z

    fn start(sec: u64) -> ProcStart {
        ProcStart { sec, usec: 5 }
    }

    fn proc(pid: u32, ppid: u32) -> ProcIdentity {
        ProcIdentity {
            pid,
            ppid,
            start: start(EPOCH - 100 + u64::from(pid)),
        }
    }

    fn tcp(pid: u32, local: u16, remote: &str, state: TcpState) -> Endpoint {
        let local: SocketAddr = format!("127.0.0.1:{local}").parse().unwrap();
        Endpoint {
            key: EndpointKey {
                pid,
                proto: Proto::Tcp,
                ip_ver: IpVer::V4,
                local,
                remote: remote.parse().unwrap(),
            },
            state: Some(state),
            dir: Dir::Out,
            process: format!("proc{pid}"),
            path: format!("/bin/proc{pid}"),
        }
    }

    struct Tick {
        endpoints: Result<Vec<Endpoint>, String>,
        processes: Result<Vec<ProcIdentity>, String>,
        unreadable: Vec<u32>,
    }

    fn tick(endpoints: Vec<Endpoint>, processes: Vec<ProcIdentity>) -> Tick {
        Tick {
            endpoints: Ok(endpoints),
            processes: Ok(processes),
            unreadable: Vec::new(),
        }
    }

    /// Scripted samples on a virtual clock. The last tick repeats.
    struct Fake {
        ticks: Vec<Tick>,
        taken: usize,
        clock: Duration,
        sample_cost: Duration,
        stop_at: Option<Duration>,
        unknown: Vec<u32>,
        children: HashMap<u32, Vec<u32>>,
        waits: Vec<Duration>,
    }

    impl Fake {
        fn new(ticks: Vec<Tick>) -> Self {
            Self {
                ticks,
                taken: 0,
                clock: Duration::ZERO,
                sample_cost: Duration::ZERO,
                stop_at: None,
                unknown: Vec::new(),
                children: HashMap::new(),
                waits: Vec::new(),
            }
        }

        fn current(&self) -> &Tick {
            &self.ticks[self.taken.saturating_sub(1).min(self.ticks.len() - 1)]
        }
    }

    impl Source for Fake {
        fn endpoints(&mut self) -> Result<Sample, String> {
            self.taken += 1;
            self.clock += self.sample_cost;
            let tick = self.current();
            tick.endpoints.clone().map(|endpoints| Sample {
                endpoints,
                unreadable: tick.unreadable.clone(),
            })
        }

        fn processes(&mut self) -> Result<Vec<ProcIdentity>, String> {
            self.current().processes.clone()
        }

        fn children(&mut self, pid: u32) -> Vec<u32> {
            self.children.get(&pid).cloned().unwrap_or_default()
        }

        fn identity(&mut self, pid: u32) -> Option<ProcIdentity> {
            (!self.unknown.contains(&pid)).then(|| proc(pid, 1))
        }

        fn names(&mut self, pid: u32) -> (String, String) {
            (format!("proc{pid}"), format!("/bin/proc{pid}"))
        }

        fn wall(&mut self) -> SystemTime {
            UNIX_EPOCH + Duration::from_secs(EPOCH) + self.clock
        }

        fn elapsed(&mut self) -> Duration {
            self.clock
        }

        fn wait(&mut self, time: Duration) -> bool {
            self.waits.push(time);
            if self.stop_at.is_some_and(|stop| self.clock + time >= stop) {
                return false;
            }
            self.clock += time;
            true
        }
    }

    fn config(interval_ms: u64, duration_s: Option<u64>) -> Config {
        Config {
            interval: Duration::from_millis(interval_ms),
            duration: duration_s.map(Duration::from_secs),
            roots: Vec::new(),
            filter: None,
            policy: None,
        }
    }

    fn watch(cfg: &Config, src: &mut Fake) -> (Summary, Vec<String>) {
        let mut out = Vec::new();
        let summary = run(cfg, src, &mut out).expect("in-memory writes succeed");
        let text = String::from_utf8(out).unwrap();
        assert!(text.ends_with('\n'));
        (summary, text.lines().map(str::to_string).collect())
    }

    #[test]
    fn timestamps_are_rfc3339_utc_across_leap_years() {
        for (secs, expected) in [
            (0, "1970-01-01T00:00:00"),
            (86_399, "1970-01-01T23:59:59"),
            (951_782_400, "2000-02-29T00:00:00"),
            (1_709_251_199, "2024-02-29T23:59:59"),
            (1_709_251_200, "2024-03-01T00:00:00"),
            (EPOCH, "2023-11-14T22:13:20"),
            (4_102_444_800, "2100-01-01T00:00:00"),
            (4_107_542_400, "2100-03-01T00:00:00"),
        ] {
            assert_eq!(date_time(secs), expected);
        }
        let at = UNIX_EPOCH + Duration::new(EPOCH, 7_654_321);
        assert_eq!(timestamp(at), "2023-11-14T22:13:20.007Z");
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        let exact = ProcStart {
            sec: EPOCH,
            usec: 42,
        };
        assert_eq!(start_stamp(exact), "2023-11-14T22:13:20.000042Z");
    }

    #[test]
    fn json_strings_escape_quotes_controls_and_spoofing_characters() {
        assert_eq!(json_string(""), "\"\"");
        assert_eq!(json_string("café 📡 שלום"), "\"café 📡 שלום\"");
        assert_eq!(json_string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(json_string("a\nb\rc\td"), r#""a\nb\rc\td""#);
        assert_eq!(
            json_string("x\u{0000}\u{001b}\u{007f}\u{0085}"),
            r#""x\u0000\u001b\u007f\u0085""#
        );
        // Bidi override, zero-width space, BOM and line separators stay
        // recoverable but cannot act on a terminal or split the line.
        assert_eq!(
            json_string("evil\u{202E}gpj\u{200B}\u{FEFF}\u{2028}\u{2029}"),
            r#""evil\u202egpj\u200b\ufeff\u2028\u2029""#
        );
        let hostile = json_string("\",\"type\":\"end\"}\n{\"forged\":\"");
        assert!(!hostile.contains('\n'));
        assert_eq!(hostile, r#""\",\"type\":\"end\"}\n{\"forged\":\"""#);
    }

    #[test]
    fn start_and_end_records_frame_the_stream() {
        let mut src = Fake::new(vec![tick(vec![], vec![])]);
        let (summary, lines) = watch(&config(1000, Some(2)), &mut src);
        assert_eq!(
            lines,
            [
                format!(
                    "{{\"type\":\"start\",\"schema\":\"netboard.watch/1\",\"version\":\"{}\",\
                     \"ts\":\"2023-11-14T22:13:20.000Z\",\"interval_ms\":1000,\"duration_s\":2,\
                     \"roots\":[],\"filter\":null,\"policy\":null,\
                     \"untrusted_fields\":[\"process.name\",\"process.path\",\
                     \"peer.processes.name\",\"peer.processes.path\"]}}",
                    env!("CARGO_PKG_VERSION")
                ),
                "{\"type\":\"baseline\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"endpoints\":0,\"processes\":null,\"unreadable_processes\":0}"
                    .to_string(),
                "{\"type\":\"end\",\"ts\":\"2023-11-14T22:13:22.000Z\",\"reason\":\"duration\",\
                 \"samples\":3,\"failed_samples\":0,\"max_gap_ms\":1000,\"elapsed_ms\":2000,\"unreadable_processes\":0,\"policy\":null}"
                    .to_string(),
            ]
        );
        assert_eq!(
            summary,
            Summary {
                reason: EndReason::Duration,
                samples: 3,
                failed: 0,
                unreadable: 0,
                violations: 0,
                verdict: None,
            }
        );
        assert_eq!(src.waits, [Duration::from_secs(1); 2]);
    }

    #[test]
    fn endpoints_are_reported_as_existing_new_changed_and_closed() {
        let peer = "192.0.2.7:443";
        let mut src = Fake::new(vec![
            tick(
                vec![tcp(7, 5000, "127.0.0.1:80", TcpState::Established)],
                vec![],
            ),
            tick(
                vec![
                    tcp(7, 5000, "127.0.0.1:80", TcpState::Established),
                    tcp(9, 6000, peer, TcpState::SynSent),
                ],
                vec![],
            ),
            tick(
                vec![
                    tcp(7, 5000, "127.0.0.1:80", TcpState::Established),
                    tcp(9, 6000, peer, TcpState::Established),
                ],
                vec![],
            ),
            tick(vec![], vec![]),
        ]);
        let (summary, lines) = watch(&config(1000, Some(3)), &mut src);
        assert_eq!(summary.samples, 4);
        assert_eq!(
            lines[1..lines.len() - 1],
            [
                "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"event\":\"existing\",\
                 \"process\":{\"pid\":7,\"start\":\"2023-11-14T22:11:47.000005Z\",\"name\":\"proc7\",\
                 \"path\":\"/bin/proc7\"},\"proto\":\"tcp\",\"ip_version\":4,\"dir\":\"out\",\
                 \"local\":{\"ip\":\"127.0.0.1\",\"port\":5000},\
                 \"remote\":{\"ip\":\"127.0.0.1\",\"port\":80},\"state\":\"ESTABLISHED\",\
                 \"off_box\":false,\"remote_class\":\"loopback\",\"local_class\":\"loopback\",\"exposed\":false,\"peer\":null}",
                "{\"type\":\"baseline\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"endpoints\":1,\
                 \"processes\":null,\"unreadable_processes\":0}",
                "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:21.000Z\",\"event\":\"new\",\
                 \"process\":{\"pid\":9,\"start\":\"2023-11-14T22:11:49.000005Z\",\"name\":\"proc9\",\
                 \"path\":\"/bin/proc9\"},\"proto\":\"tcp\",\"ip_version\":4,\"dir\":\"out\",\
                 \"local\":{\"ip\":\"127.0.0.1\",\"port\":6000},\
                 \"remote\":{\"ip\":\"192.0.2.7\",\"port\":443},\"state\":\"SYN_SENT\",\
                 \"off_box\":true,\"remote_class\":\"other\",\"local_class\":\"loopback\",\"exposed\":false,\"peer\":null}",
                "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:22.000Z\",\"event\":\"changed\",\
                 \"process\":{\"pid\":9,\"start\":\"2023-11-14T22:11:49.000005Z\",\"name\":\"proc9\",\
                 \"path\":\"/bin/proc9\"},\"proto\":\"tcp\",\"ip_version\":4,\"dir\":\"out\",\
                 \"local\":{\"ip\":\"127.0.0.1\",\"port\":6000},\
                 \"remote\":{\"ip\":\"192.0.2.7\",\"port\":443},\"state\":\"ESTABLISHED\",\
                 \"prev_state\":\"SYN_SENT\",\"off_box\":true,\"remote_class\":\"other\",\"local_class\":\"loopback\",\"exposed\":false,\"peer\":null}",
                "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:23.000Z\",\"event\":\"closed\",\
                 \"process\":{\"pid\":7,\"start\":\"2023-11-14T22:11:47.000005Z\",\"name\":\"proc7\",\
                 \"path\":\"/bin/proc7\"},\"proto\":\"tcp\",\"ip_version\":4,\"dir\":\"out\",\
                 \"local\":{\"ip\":\"127.0.0.1\",\"port\":5000},\
                 \"remote\":{\"ip\":\"127.0.0.1\",\"port\":80},\"state\":\"ESTABLISHED\",\
                 \"off_box\":false,\"remote_class\":\"loopback\",\"local_class\":\"loopback\",\"exposed\":false,\"peer\":null}",
                "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:23.000Z\",\"event\":\"closed\",\
                 \"process\":{\"pid\":9,\"start\":\"2023-11-14T22:11:49.000005Z\",\"name\":\"proc9\",\
                 \"path\":\"/bin/proc9\"},\"proto\":\"tcp\",\"ip_version\":4,\"dir\":\"out\",\
                 \"local\":{\"ip\":\"127.0.0.1\",\"port\":6000},\
                 \"remote\":{\"ip\":\"192.0.2.7\",\"port\":443},\"state\":\"ESTABLISHED\",\
                 \"off_box\":true,\"remote_class\":\"other\",\"local_class\":\"loopback\",\"exposed\":false,\"peer\":null}",
            ]
        );
    }

    #[test]
    fn udp_and_listen_endpoints_have_null_state_and_real_unspecified_addresses() {
        let mut udp = tcp(3, 5353, "0.0.0.0:0", TcpState::Closed);
        udp.key.proto = Proto::Udp;
        udp.key.local = "[::]:5353".parse().unwrap();
        udp.key.remote = "[::]:0".parse().unwrap();
        udp.key.ip_ver = IpVer::V6;
        udp.state = None;
        udp.dir = Dir::Unknown;
        let mut src = Fake::new(vec![tick(vec![udp], vec![])]);
        src.unknown.push(3);
        let (_, lines) = watch(&config(1000, Some(1)), &mut src);
        assert_eq!(
            lines[1],
            "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"event\":\"existing\",\
             \"process\":{\"pid\":3,\"start\":null,\"name\":\"proc3\",\"path\":\"/bin/proc3\"},\
             \"proto\":\"udp\",\"ip_version\":6,\"dir\":\"unknown\",\
             \"local\":{\"ip\":\"::\",\"port\":5353},\"remote\":{\"ip\":\"::\",\"port\":0},\
             \"state\":null,\"off_box\":false,\"remote_class\":null,\"local_class\":\"wildcard\",\"exposed\":true,\"peer\":null}"
        );
    }

    #[test]
    fn duplicate_descriptors_are_one_endpoint_and_a_reused_pid_is_another() {
        let a = tcp(7, 5000, "192.0.2.7:443", TcpState::Established);
        let seen = || Seen {
            endpoint: a.clone(),
            peer: None,
        };
        let prev: HashMap<Key, Seen> = HashMap::from([((a.key.clone(), Some(start(1))), seen())]);
        // Same PID and tuple, different process start: close then open.
        let next: HashMap<Key, Seen> = HashMap::from([((a.key.clone(), Some(start(2))), seen())]);
        let kinds: Vec<(Kind, Option<ProcStart>)> = delta(&prev, &next, false)
            .into_iter()
            .map(|(kind, _, started)| (kind, started))
            .collect();
        assert_eq!(
            kinds,
            [(Kind::Closed, Some(start(1))), (Kind::New, Some(start(2)))]
        );
        assert!(delta(&prev, &prev, false).is_empty());

        let mut src = Fake::new(vec![tick(vec![a.clone(), a.clone()], vec![])]);
        let (_, lines) = watch(&config(1000, Some(1)), &mut src);
        assert_eq!(lines.len(), 4, "{lines:?}");
    }

    #[test]
    fn an_exiting_process_keeps_its_identity_until_its_endpoint_closes() {
        let a = tcp(7, 5000, "192.0.2.7:443", TcpState::Established);
        let mut src = Fake::new(vec![
            tick(vec![a.clone()], vec![]),
            tick(vec![a.clone()], vec![]),
            tick(vec![], vec![]),
        ]);
        let cfg = config(1000, Some(2));
        let mut out = Vec::new();
        // Identity is readable for the first sample only.
        struct Fading(Fake);
        impl Source for Fading {
            fn endpoints(&mut self) -> Result<Sample, String> {
                self.0.endpoints()
            }
            fn processes(&mut self) -> Result<Vec<ProcIdentity>, String> {
                self.0.processes()
            }
            fn children(&mut self, pid: u32) -> Vec<u32> {
                self.0.children(pid)
            }
            fn identity(&mut self, pid: u32) -> Option<ProcIdentity> {
                (self.0.taken == 1).then(|| proc(pid, 1))
            }
            fn names(&mut self, pid: u32) -> (String, String) {
                self.0.names(pid)
            }
            fn wall(&mut self) -> SystemTime {
                self.0.wall()
            }
            fn elapsed(&mut self) -> Duration {
                self.0.elapsed()
            }
            fn wait(&mut self, time: Duration) -> bool {
                self.0.wait(time)
            }
        }
        src.sample_cost = Duration::ZERO;
        run(&cfg, &mut Fading(src), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let events: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("\"type\":\"endpoint\""))
            .collect();
        assert_eq!(events.len(), 2, "{text}");
        assert!(events[0].contains("\"event\":\"existing\""));
        assert!(events[1].contains("\"event\":\"closed\""));
        assert!(events[1].contains("\"start\":\"2023-11-14T22:11:47.000005Z\""));
    }

    #[test]
    fn tree_scope_reports_processes_and_only_their_endpoints() {
        let (root, child, other) = (proc(100, 1), proc(200, 100), proc(900, 1));
        let adopted = ProcIdentity { ppid: 1, ..child };
        let mine = tcp(200, 5000, "192.0.2.7:443", TcpState::Established);
        let theirs = tcp(900, 7000, "192.0.2.9:443", TcpState::Established);
        let mut src = Fake::new(vec![
            tick(vec![theirs.clone()], vec![root, other]),
            tick(vec![mine.clone(), theirs.clone()], vec![root, child, other]),
            // The root exits; its child is adopted by launchd and keeps going.
            tick(vec![mine.clone(), theirs.clone()], vec![adopted, other]),
            tick(vec![theirs.clone()], vec![other]),
        ]);
        let mut cfg = config(1000, None);
        cfg.roots = vec![root];
        let (summary, lines) = watch(&cfg, &mut src);
        assert_eq!(summary.reason, EndReason::ScopeExited);
        assert_eq!(summary.samples, 4);
        let shape: Vec<String> = lines
            .iter()
            .map(|line| {
                let field = |name: &str| {
                    let tail = line.split_once(&format!("\"{name}\":")).map(|(_, t)| t);
                    tail.and_then(|t| t.split([',', '}']).next())
                        .unwrap_or("-")
                        .trim_matches('"')
                        .to_string()
                };
                format!("{} {} pid={}", field("type"), field("event"), field("pid"))
            })
            .collect();
        assert_eq!(
            shape,
            [
                "start - pid=100",
                "process tracked pid=100",
                "baseline - pid=-",
                "process tracked pid=200",
                "endpoint new pid=200",
                "process exited pid=100",
                "endpoint closed pid=200",
                "process exited pid=200",
                "end - pid=-",
            ],
            "{lines:#?}"
        );
        assert!(!lines.iter().any(|line| line.contains("\"pid\":900")));
        assert_eq!(
            lines[1],
            "{\"type\":\"process\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"event\":\"tracked\",\
             \"reason\":\"root\",\"process\":{\"pid\":100,\"ppid\":1,\
             \"start\":\"2023-11-14T22:13:20.000005Z\",\"name\":\"proc100\",\
             \"path\":\"/bin/proc100\"}}"
        );
        assert_eq!(
            lines[2],
            "{\"type\":\"baseline\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"endpoints\":0,\
             \"processes\":1,\"unreadable_processes\":0}"
        );
        assert!(lines[3].contains("\"reason\":\"descendant\""));
        assert!(lines[3].contains("\"ppid\":100"));
        assert!(lines[0]
            .contains("\"roots\":[{\"pid\":100,\"start\":\"2023-11-14T22:13:20.000005Z\"}]"));
        assert!(lines[8].contains("\"reason\":\"scope_exited\""));
    }

    #[test]
    fn a_local_peer_is_named_even_when_it_is_outside_the_watched_tree() {
        let (root, server) = (proc(100, 1), proc(900, 1));
        let client = tcp(100, 5000, "127.0.0.1:11434", TcpState::Established);
        let mut accepted = tcp(900, 11434, "127.0.0.1:5000", TcpState::Established);
        accepted.dir = Dir::In;
        let mut listener = tcp(900, 11434, "0.0.0.0:0", TcpState::Listen);
        listener.dir = Dir::Listen;
        let ticks = || {
            vec![
                tick(vec![listener.clone()], vec![root, server]),
                // Connected, not accepted yet: only the listener is visible.
                tick(vec![client.clone(), listener.clone()], vec![root, server]),
                tick(
                    vec![client.clone(), accepted.clone(), listener.clone()],
                    vec![root, server],
                ),
                tick(vec![listener.clone()], vec![server]),
            ]
        };
        let mut cfg = config(1000, None);
        cfg.roots = vec![root];
        let (summary, lines) = watch(&cfg, &mut Fake::new(ticks()));
        assert_eq!(summary.reason, EndReason::ScopeExited);
        let events: Vec<&String> = lines
            .iter()
            .filter(|line| line.contains("\"type\":\"endpoint\""))
            .collect();
        assert_eq!(events.len(), 2, "{lines:#?}");
        assert_eq!(
            events[0].as_str(),
            "{\"type\":\"endpoint\",\"ts\":\"2023-11-14T22:13:21.000Z\",\"event\":\"new\",\
             \"process\":{\"pid\":100,\"start\":\"2023-11-14T22:13:20.000005Z\",\
             \"name\":\"proc100\",\"path\":\"/bin/proc100\"},\"proto\":\"tcp\",\"ip_version\":4,\
             \"dir\":\"out\",\"local\":{\"ip\":\"127.0.0.1\",\"port\":5000},\
             \"remote\":{\"ip\":\"127.0.0.1\",\"port\":11434},\"state\":\"ESTABLISHED\",\
             \"off_box\":false,\"remote_class\":\"loopback\",\"local_class\":\"loopback\",\"exposed\":false,\"peer\":{\"via\":\"listener\",\"process_count\":1,\
             \"processes\":[{\"pid\":900,\"start\":\"2023-11-14T22:26:40.000005Z\",\
             \"name\":\"proc900\",\"path\":\"/bin/proc900\",\"in_scope\":false}]}}"
        );
        // The accept in the next sample is not an event; the close reports
        // the peer as it was last seen.
        assert!(events[1].contains("\"event\":\"closed\""));
        assert!(events[1].contains("\"peer\":{\"via\":\"connection\",\"process_count\":1,"));
        // The server's own sockets are outside the tree and are not reported.
        assert!(!lines
            .iter()
            .any(|line| line.contains("\"process\":{\"pid\":900")));

        // Without a tree every process is in scope, and each end names the other.
        let mut src = Fake::new(ticks());
        let (_, lines) = watch(&config(1000, Some(3)), &mut src);
        let of = |event: &str, pid: u32, remote_port: u16| {
            lines
                .iter()
                .find(|line| {
                    line.contains(&format!("\"event\":\"{event}\""))
                        && line.contains(&format!("\"process\":{{\"pid\":{pid},"))
                        && line.contains(&format!("\"port\":{remote_port}}},\"state\""))
                })
                .unwrap_or_else(|| panic!("{event} {pid} missing from {lines:#?}"))
        };
        assert!(of("new", 100, 11434).contains("\"in_scope\":true}]}}"));
        let served = of("new", 900, 5000);
        assert!(served.contains("\"peer\":{\"via\":\"connection\""));
        assert!(served.contains("\"processes\":[{\"pid\":100,"));
        assert!(of("existing", 900, 0).ends_with("\"peer\":null}"));
    }

    #[test]
    fn filter_narrows_endpoints_and_is_recorded() {
        let mut src = Fake::new(vec![tick(
            vec![
                tcp(7, 5000, "192.0.2.7:443", TcpState::Established),
                tcp(9, 6000, "192.0.2.7:443", TcpState::Established),
            ],
            vec![],
        )]);
        for (filter, record, pid) in [
            (Filter::parse("9"), "\"filter\":{\"pid\":9}", "\"pid\":9"),
            (
                Filter::parse("PROC7"),
                "\"filter\":{\"name\":\"proc7\"}",
                "\"pid\":7",
            ),
        ] {
            src.taken = 0;
            src.clock = Duration::ZERO;
            let mut cfg = config(1000, Some(1));
            cfg.filter = Some(filter);
            let (_, lines) = watch(&cfg, &mut src);
            assert!(lines[0].contains(record), "{}", lines[0]);
            assert_eq!(lines.len(), 4);
            assert!(lines[1].contains(pid), "{}", lines[1]);
        }
    }

    #[test]
    fn failed_samples_are_recorded_and_do_not_close_endpoints() {
        let a = tcp(7, 5000, "192.0.2.7:443", TcpState::Established);
        let mut src = Fake::new(vec![
            tick(vec![a.clone()], vec![]),
            Tick {
                endpoints: Err("libproc sockets: \"denied\"".into()),
                processes: Ok(vec![]),
                unreadable: vec![],
            },
            tick(vec![a.clone()], vec![]),
        ]);
        let (summary, lines) = watch(&config(1000, Some(2)), &mut src);
        assert_eq!((summary.samples, summary.failed), (2, 1));
        assert_eq!(lines.len(), 5, "{lines:#?}");
        assert!(lines[1].contains("\"event\":\"existing\""));
        assert!(lines[2].contains("\"type\":\"baseline\""));
        assert_eq!(
            lines[3],
            "{\"type\":\"error\",\"ts\":\"2023-11-14T22:13:21.000Z\",\
             \"message\":\"libproc sockets: \\\"denied\\\"\"}"
        );
        // The gap spans the failed sample: two seconds went unobserved.
        assert!(lines[4].contains("\"samples\":2,\"failed_samples\":1,\"max_gap_ms\":2000"));
    }

    #[test]
    fn a_failed_process_scan_fails_the_sample_without_ending_the_scope() {
        let root = proc(100, 1);
        let mut src = Fake::new(vec![
            Tick {
                endpoints: Ok(vec![]),
                processes: Err("libproc could not list processes".into()),
                unreadable: vec![],
            },
            tick(vec![], vec![root]),
            tick(vec![], vec![]),
        ]);
        let mut cfg = config(1000, None);
        cfg.roots = vec![root];
        let (summary, lines) = watch(&cfg, &mut src);
        assert_eq!((summary.samples, summary.failed), (2, 1));
        assert_eq!(summary.reason, EndReason::ScopeExited);
        // The baseline is the first sample that succeeded, not the first tried.
        assert!(lines[1].contains("\"type\":\"error\""));
        assert!(lines[2].contains("\"event\":\"tracked\""));
        assert!(lines[3].contains("\"type\":\"baseline\""));
        assert!(lines[3].contains("\"ts\":\"2023-11-14T22:13:21.000Z\""));
        assert!(lines[4].contains("\"event\":\"exited\""));
        // One successful pair of samples, one second apart.
        assert!(lines[5].contains("\"max_gap_ms\":1000"));
    }

    #[test]
    fn an_interrupt_ends_the_stream_with_its_reason_and_no_gap_for_one_sample() {
        let mut src = Fake::new(vec![tick(vec![], vec![])]);
        src.stop_at = Some(Duration::from_millis(500));
        let (summary, lines) = watch(&config(1000, None), &mut src);
        assert_eq!(summary.reason, EndReason::Interrupted);
        assert_eq!(
            lines[2],
            "{\"type\":\"end\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"reason\":\"interrupted\",\
             \"samples\":1,\"failed_samples\":0,\"max_gap_ms\":null,\"elapsed_ms\":0,\"unreadable_processes\":0,\"policy\":null}"
        );
    }

    #[test]
    fn the_last_sample_lands_on_the_limit_and_slow_samples_skip_slots() {
        // 700 ms cadence inside a 2 s window: 0, 700, 1400, then 2000.
        let mut src = Fake::new(vec![tick(vec![], vec![])]);
        let (summary, _) = watch(&config(700, Some(2)), &mut src);
        assert_eq!(summary.samples, 4);
        assert_eq!(
            src.waits,
            [
                Duration::from_millis(700),
                Duration::from_millis(700),
                Duration::from_millis(600)
            ]
        );

        // Each sample takes 2.5 s against a 1 s interval: samples start at
        // 0 and 3 s, never back to back, and the gap is reported as 3 s.
        let mut src = Fake::new(vec![tick(vec![], vec![])]);
        src.sample_cost = Duration::from_millis(2500);
        let (summary, lines) = watch(&config(1000, Some(5)), &mut src);
        assert_eq!(summary.samples, 2);
        assert_eq!(src.waits, [Duration::from_millis(500)]);
        assert!(lines[2].contains("\"max_gap_ms\":3000,\"elapsed_ms\":5500"));
    }

    #[test]
    fn a_write_error_ends_the_watch() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut src = Fake::new(vec![tick(vec![], vec![])]);
        let err = run(&config(1000, None), &mut src, &mut Closed).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(src.taken, 0, "no sample after the reader went away");
    }

    fn listen(pid: u32, local: &str) -> Endpoint {
        let mut e = tcp(pid, 0, "0.0.0.0:0", TcpState::Listen);
        e.key.local = local.parse().unwrap();
        e.dir = Dir::Listen;
        e
    }

    fn lines_of<'a>(lines: &'a [String], kind: &str) -> Vec<&'a String> {
        lines
            .iter()
            .filter(|line| line.starts_with(&format!("{{\"type\":\"{kind}\"")))
            .collect()
    }

    #[test]
    fn endpoint_records_carry_address_classes_and_exposure() {
        let mut udp = tcp(5, 0, "0.0.0.0:0", TcpState::Closed);
        udp.key.proto = Proto::Udp;
        udp.key.local = "0.0.0.0:5353".parse().unwrap();
        udp.state = None;
        let mut lan = tcp(6, 5001, "192.168.1.20:8080", TcpState::Established);
        lan.key.local = "192.168.1.5:5001".parse().unwrap();
        let eps = vec![
            listen(3, "127.0.0.1:11434"),
            listen(4, "0.0.0.0:22"),
            udp,
            lan,
            tcp(7, 5002, "8.8.8.8:443", TcpState::Established),
        ];
        let (_, lines) = watch(
            &config(1000, Some(1)),
            &mut Fake::new(vec![tick(eps, vec![])]),
        );
        let classes: Vec<String> = lines_of(&lines, "endpoint")
            .iter()
            .map(|line| {
                let (_, tail) = line.split_once("\"off_box\":").unwrap();
                tail.split(",\"peer\"").next().unwrap().to_string()
            })
            .collect();
        assert_eq!(
            classes,
            [
                "false,\"remote_class\":null,\"local_class\":\"loopback\",\"exposed\":false",
                "false,\"remote_class\":null,\"local_class\":\"wildcard\",\"exposed\":true",
                "false,\"remote_class\":null,\"local_class\":\"wildcard\",\"exposed\":true",
                "true,\"remote_class\":\"lan\",\"local_class\":\"lan\",\"exposed\":false",
                "true,\"remote_class\":\"public\",\"local_class\":\"loopback\",\"exposed\":false",
            ]
        );
    }

    #[test]
    fn unreadable_processes_are_counted_and_named_only_when_scoped() {
        let mut first = tick(vec![], vec![]);
        first.unreadable = vec![50, 60];
        let mut second = tick(vec![], vec![]);
        second.unreadable = vec![60, 70];
        // Unscoped: counted, not listed.
        let mut src = Fake::new(vec![first, second]);
        let (summary, lines) = watch(&config(1000, Some(1)), &mut src);
        assert_eq!(summary.unreadable, 3);
        assert!(lines_of(&lines, "process").is_empty(), "{lines:#?}");
        assert!(lines_of(&lines, "baseline")[0].ends_with("\"unreadable_processes\":2}"));
        assert!(lines_of(&lines, "end")[0].contains("\"unreadable_processes\":3,\"policy\":null}"));
        // No policy, no verdict: the counts are information only.
        assert_eq!(summary.verdict, None);

        // A PID filter names the one it is about.
        let mut gap = tick(vec![], vec![]);
        gap.unreadable = vec![50, 60];
        let mut cfg = config(1000, Some(1));
        cfg.filter = Some(Filter::Pid(60));
        let (summary, lines) = watch(&cfg, &mut Fake::new(vec![gap]));
        assert_eq!(summary.unreadable, 1);
        assert_eq!(
            lines_of(&lines, "process"),
            ["{\"type\":\"process\",\"ts\":\"2023-11-14T22:13:20.000Z\",\"event\":\"unreadable\",\
              \"process\":{\"pid\":60,\"name\":\"proc60\",\"path\":\"/bin/proc60\"}}"]
        );
    }

    #[test]
    fn a_tree_counts_descendants_it_cannot_read_through_child_lists() {
        let root = proc(100, 1);
        // 100 runs 300 (setuid, unreadable), which runs 301 (also unreadable).
        // 900 is unreadable but nobody in the tree started it.
        let mut sample = tick(vec![], vec![root]);
        sample.unreadable = vec![300, 301, 900];
        let mut src = Fake::new(vec![sample]);
        src.children = HashMap::from([(100, vec![300]), (300, vec![301]), (1, vec![100, 900])]);
        let mut cfg = config(1000, Some(1));
        cfg.roots = vec![root];
        let (summary, lines) = watch(&cfg, &mut src);
        assert_eq!(summary.unreadable, 2);
        let named: Vec<&str> = lines_of(&lines, "process")
            .iter()
            .filter(|line| line.contains("\"event\":\"unreadable\""))
            .map(|line| {
                line.split("\"pid\":")
                    .nth(1)
                    .unwrap()
                    .split(',')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(named, ["300", "301"]);
    }

    fn policy(rules: &str) -> Option<Policy> {
        Some(Policy::parse(&format!("netboard-policy 1\n{rules}")).unwrap())
    }

    #[test]
    fn a_policy_reports_each_violation_once_and_ends_with_a_verdict() {
        let ok = tcp(7, 5000, "127.0.0.1:11434", TcpState::Established);
        let bad = tcp(7, 5001, "8.8.8.8:443", TcpState::SynSent);
        let mut bad_up = bad.clone();
        bad_up.state = Some(TcpState::Established);
        let mut src = Fake::new(vec![
            tick(vec![ok.clone(), listen(7, "127.0.0.1:8080")], vec![]),
            tick(vec![ok.clone(), bad.clone()], vec![]),
            tick(vec![ok.clone(), bad_up], vec![]),
            tick(vec![ok.clone(), listen(7, "0.0.0.0:8081")], vec![]),
        ]);
        let mut cfg = config(1000, Some(3));
        cfg.policy = policy("allow loopback proto tcp port 11434\nlisten loopback\n");
        let (summary, lines) = watch(&cfg, &mut src);
        assert!(
            lines[0].contains("\"policy\":{\"rules\":2}"),
            "{}",
            lines[0]
        );
        let violations = lines_of(&lines, "violation");
        assert_eq!(
            violations,
            [
                "{\"type\":\"violation\",\"ts\":\"2023-11-14T22:13:21.000Z\",\"breach\":\"connection\",\
                 \"process\":{\"pid\":7,\"start\":\"2023-11-14T22:11:47.000005Z\",\"name\":\"proc7\",\
                 \"path\":\"/bin/proc7\"},\"proto\":\"tcp\",\
                 \"local\":{\"ip\":\"127.0.0.1\",\"port\":5001},\
                 \"remote\":{\"ip\":\"8.8.8.8\",\"port\":443},\"state\":\"SYN_SENT\",\
                 \"remote_class\":\"public\",\"local_class\":\"loopback\"}",
                "{\"type\":\"violation\",\"ts\":\"2023-11-14T22:13:23.000Z\",\"breach\":\"listener\",\
                 \"process\":{\"pid\":7,\"start\":\"2023-11-14T22:11:47.000005Z\",\"name\":\"proc7\",\
                 \"path\":\"/bin/proc7\"},\"proto\":\"tcp\",\
                 \"local\":{\"ip\":\"0.0.0.0\",\"port\":8081},\
                 \"remote\":{\"ip\":\"0.0.0.0\",\"port\":0},\"state\":\"LISTEN\",\
                 \"remote_class\":null,\"local_class\":\"wildcard\"}",
            ]
        );
        // Each violation follows the endpoint record that reported it.
        let at = lines.iter().position(|l| l == violations[0]).unwrap();
        assert!(lines[at - 1].contains("\"event\":\"new\""));
        assert_eq!(summary.violations, 2);
        assert_eq!(summary.verdict, Some(Verdict::Violation));
        assert!(lines.last().unwrap().ends_with(
            "\"policy\":{\"verdict\":\"violation\",\"violations\":2,\"incomplete\":[]}}"
        ));
    }

    #[test]
    fn a_verdict_is_pass_only_when_the_whole_scope_was_seen() {
        let ok = tcp(7, 5000, "127.0.0.1:11434", TcpState::Established);
        let rules = "allow loopback\n";
        let verdict = |src: &mut Fake, duration: Option<u64>| {
            let mut cfg = config(1000, duration);
            cfg.policy = policy(rules);
            let (summary, lines) = watch(&cfg, src);
            let end = lines.last().unwrap().clone();
            (summary.verdict.unwrap(), end)
        };

        let (seen, end) = verdict(
            &mut Fake::new(vec![tick(vec![ok.clone()], vec![])]),
            Some(1),
        );
        assert_eq!(seen, Verdict::Pass);
        assert!(
            end.ends_with("\"policy\":{\"verdict\":\"pass\",\"violations\":0,\"incomplete\":[]}}")
        );

        let mut gap = tick(vec![ok.clone()], vec![]);
        gap.unreadable = vec![40];
        let failed = Tick {
            endpoints: Err("libproc could not list processes".into()),
            processes: Ok(vec![]),
            unreadable: vec![],
        };
        let (seen, end) = verdict(&mut Fake::new(vec![gap, failed]), Some(1));
        assert_eq!(seen, Verdict::Incomplete);
        assert!(end.contains(
            "\"verdict\":\"incomplete\",\"violations\":0,\
             \"incomplete\":[\"failed_samples\",\"unreadable_processes\"]"
        ));

        // Stopped before the requested time: incomplete. Without a
        // requested time, a stop is how a watch normally ends.
        let mut src = Fake::new(vec![tick(vec![ok.clone()], vec![])]);
        src.stop_at = Some(Duration::from_millis(500));
        let (seen, end) = verdict(&mut src, Some(5));
        assert_eq!(seen, Verdict::Incomplete);
        assert!(end.contains("\"incomplete\":[\"interrupted\"]"));
        let mut src = Fake::new(vec![tick(vec![ok.clone()], vec![])]);
        src.stop_at = Some(Duration::from_millis(500));
        assert_eq!(verdict(&mut src, None).0, Verdict::Pass);

        // A violation outranks every gap, which is still listed.
        let mut both = tick(
            vec![tcp(7, 5001, "8.8.8.8:443", TcpState::Established)],
            vec![],
        );
        both.unreadable = vec![40];
        let (seen, end) = verdict(&mut Fake::new(vec![both]), Some(1));
        assert_eq!(seen, Verdict::Violation);
        assert!(end.contains("\"violations\":1,\"incomplete\":[\"unreadable_processes\"]"));
        assert_eq!(
            [Verdict::Pass, Verdict::Violation, Verdict::Incomplete].map(Verdict::exit_code),
            [0, 1, 2]
        );
    }
}
