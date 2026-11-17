# CyClaw-Net-Viewer design

TCPView-class viewer for Darwin (window title **CyClaw-Net-Viewer**). Official event colors from [Microsoft Learn — TCPView](https://learn.microsoft.com/en-us/sysinternals/downloads/tcpview): new = green, state change = yellow, deleted = red. Splits new into **outgoing** (green) and **incoming/listen** (blue). Runs on macOS 12+ Intel and Apple Silicon (`MACOSX_DEPLOYMENT_TARGET=12.0`).

**Off-box remotes** (any TCP/UDP peer that is not unspecified and not loopback) stay **orange** while they exist, so a telemetry-kill watch can see phone-home without waiting for a new/delete flash. Event colors still win. ICMP/ping is not in the socket table.

Address cells retain the observed socket IP and port in the GUI, filtering,
copy actions, CLI and CSV. Optional reverse DNS names are untrusted labels
alongside that IP. Native IPv6 stays bracketed; only IPv4-mapped IPv6 unwraps.
A shared PID/protocol/remote port does not establish that two peers are the
same host, and a DNS A record does not identify an IPv6 socket peer. Neither
can replace the observed address. Endpoint identity and sorting still use
the original socket tuple.

## Snapshot

`proc_listallpids` → `proc_pidinfo(PROC_PIDLISTFDS)` → `proc_pidfdinfo(PROC_PIDFDSOCKETINFO)`, called directly in `src/sockets.rs` (the same path as `lsof` / Apple DTS). Process name/path from `proc_name` / `proc_pidpath`. Reverse DNS is a background `getnameinfo` cache.

The collector is first-party because the socket record holds more than a
wrapper crate passed on. It mirrors `struct socket_fdinfo` from
`<sys/proc_info.h>`: the fields it reads are named, the rest is opaque
padding, and sizes and offsets are asserted at compile time (792 bytes for
the whole record). A wrong size makes every read fail its length check, so
a layout error shows as an empty table, never as plausible wrong data.

- A descriptor that closes between the listing and the query makes
  `proc_pidfdinfo` return 0, not a negative number. Only a full-length
  answer is read; anything else is skipped.
- A process that exits mid-walk, or whose descriptors this user may not
  list (another user's process, SIP, TCC), is skipped. Only a failure to
  list processes at all fails the snapshot.
- TCP and connected UDP sockets report the peer the kernel holds. A UDP
  socket that never called `connect` has none and shows `*:0`.
- An IPv6 socket whose peer is IPv4 (a dual-stack listener that accepted an
  IPv4 client) has only the record's IPv4 slot filled. It is reported
  IPv4-mapped, which the display unwraps, and it stays an IPv6 socket.
- No build-time code generation: the build needs no libclang.

## Close connection

Windows TCPView calls `SetTcpEntry(MIB_TCP_STATE_DELETE_TCB)`. Darwin has no public TCB-delete for another process. The app offers SIGTERM of the owning PID after a confirm dialog.

## Direction

- TCP LISTEN → Listen
- TCP SYN_SENT → Out
- other TCP: In if that PID also has LISTEN on the local port, else Out
- UDP: collect local bindings by PID, address family, address and port. If a
  concrete remote peer is supplied, a matching binding suggests In. A wildcard
  binding covers local addresses in the same family. Missing/unspecified peers,
  zero ports, unknown PIDs, mismatched families and unmatched bindings → Unknown.
- This is a receiver-role heuristic, not observed packet direction. Clients also
  bind local ports; a binding alone cannot prove a server role. A peer alone
  cannot prove Out. Only unconnected sockets count as bindings, so a connected
  socket is never its own receiver: an ordinary UDP client (QUIC, DNS) shows
  its real peer with direction Unknown, and is orange when that peer is
  off-box. A connected socket on a port where the same PID also holds an
  unconnected one is In. No packet capture is added.
- Unknown is used consistently in GUI filtering/sorting, CLI and CSV. Unknown
  rows have no new-in/out flash; deletion and state-change rules still apply.
  **Show listeners** hides Listen rows, not Unknown UDP rows.

## Watch stream

`netboard --cli --watch` samples on a fixed cadence and writes JSON Lines to
stdout, one object per line, flushed after every sample. It uses the same
collector as the table and the same endpoint identity, but not the GUI's
`diff`: a deleted row lingers for two repaints there, while the stream wants
exactly one `closed` record, and a new UDP socket has no flash there but is
still an event here. `src/watch.rs` holds the loop, the records, and a
`Source` trait so tests can script the machine and the clock.

Schema `netboard.watch/1`. The version changes when a record gains or loses
a field or a field changes meaning. Every record has `type` and `ts`
(RFC 3339 UTC, milliseconds).

| `type` | Written | Fields |
| --- | --- | --- |
| `start` | First line, before any sample | `schema`, `version`, `interval_ms`, `duration_s` or null, `roots` (`pid`, `start`), `filter` (`{"pid":n}`, `{"name":"…"}` or null), `untrusted_fields` |
| `process` | Tree scope only | `event` `tracked` or `exited`; `reason` `root` or `descendant` on `tracked`; `process` with `pid`, `ppid`, `start`, `name`, `path` |
| `endpoint` | A difference between two samples | `event` `existing`, `new`, `changed` or `closed`; `process` with `pid`, `start` or null, `name`, `path`; `proto` `tcp`/`udp`; `ip_version` 4/6; `dir` `in`/`out`/`listen`/`unknown`; `local` and `remote` as `{"ip","port"}`; `state` or null; `prev_state` on `changed`; `off_box`; `peer` or null ([local peers](#local-peers)) |
| `baseline` | Once, closing the first successful sample | `endpoints`, and `processes` (tracked count) or null without `--tree` |
| `error` | A sample failed | `message` |
| `end` | Last line | `reason` `duration`, `interrupted` or `scope_exited`; `samples`; `failed_samples`; `max_gap_ms` or null; `elapsed_ms` |

Within one sample the order is: `tracked` processes (parents before their
children), endpoint events sorted by PID, protocol and addresses (a `closed`
ahead of a `new` for the same tuple), then `exited` processes. The first
successful sample reports every endpoint as `existing` and ends with
`baseline`.

- **Addresses** are the observed socket values, never names. An unspecified
  address is written as `0.0.0.0` or `::` with its real port, so a listener
  or an unconnected UDP socket shows remote `0.0.0.0:0`. IPv4-mapped IPv6
  unwraps as it does elsewhere.
- **Process identity** is the PID plus `start`, the kernel's creation time
  to the microsecond. The same PID with another start time is another
  process: its old endpoints are `closed` and the new ones are `new`. An
  endpoint whose owner exits between the socket sample and the identity
  lookup keeps the start time it had in the previous sample.
- **Names and paths** are read fresh each sample for endpoints and their
  peers, and once when a process is first tracked. They are attacker-chosen
  text, listed in `untrusted_fields` as `process.name`, `process.path`,
  `peer.processes.name` and `peer.processes.path`. JSON
  strings escape controls, bidi and zero-width formatting, and U+2028/2029
  as `\uXXXX`: a parser reads the exact value back, the line stays one line,
  and a terminal shows inert ASCII.
- **Cadence** is fixed from the start time. A sample that overruns its slot
  skips to the next future slot instead of sampling back to back. With
  `--for`, the last sample is taken at the limit. `max_gap_ms` is the
  longest time between two successful samples, failed ones included in the
  gap; it is null with fewer than two.
- **Failures.** A sample fails only when processes cannot be listed at all
  (or the collector panics, which is caught). It writes `error`, changes no
  state, and closes nothing; the next good sample is compared with the last
  good one. Any failed sample makes the exit status 2.
- **Stopping.** SIGINT and SIGTERM set a flag that the wait loop checks
  every 50 ms, so the `end` record is still written. A closed reader is
  noticed on the next write and ends the watch quietly with status 0.
- **No DNS.** Watch mode has no name-resolution path at all; `-n` and `-a`
  are implied and `-c` is refused.

### Local peers

When both ends of a connection are sockets on this Mac, both are in the same
sample. `src/peers.rs` matches them, so a record can say which local process
a client is really talking to. That matters when the peer is a relay: a
local proxy, a model server, a helper daemon. A connection to `127.0.0.1`
stays on the machine only as far as that process.

```json
"peer":{"via":"connection","process_count":1,"processes":[{"pid":812,"start":"2026-11-17T02:58:11.004200Z","name":"ollama","path":"/usr/local/bin/ollama","in_scope":false}]}
```

| `via` | Match | Applies to |
| --- | --- | --- |
| `connection` | A socket with the mirrored address pair: the same connection | Any address, TCP and connected UDP |
| `listener` | No mirrored socket; the process listening on that port | TCP to a loopback address only |
| `binding` | The process with an unconnected UDP socket bound there | UDP to a loopback address only |

- A TCP client is `listener` until the server accepts, because a queued
  connection is not yet a descriptor of any process.
- The fallbacks apply to loopback destinations only. Elsewhere a local
  listener on the same port number says nothing about the remote host.
- An exact address beats a wildcard. An IPv6 wildcard is assumed to take
  IPv4 too; a snapshot cannot see a socket that opted out.
- IPv4-mapped and plain IPv4 addresses are the same peer.
- `processes` lists up to 8 distinct owners in PID order and
  `process_count` gives the total. More than one means the socket is
  shared, as with forked workers.
- `in_scope` is false when the watch has `--tree` and the peer is outside
  it. To follow what that process sends on, add it as another `--tree` root.
- Matching runs on the whole sample before any scope or filter, since the
  peer is usually outside both.
- The peer is what the sample showed when the event was written. A peer
  that appears or changes later on the same endpoint is not an event, and a
  `closed` record carries the peer last seen.
- A peer in a process this user cannot read is not found: `peer` is null,
  which does not mean nobody is there.

`netboard --cli --peers` adds the same match to a snapshot as a trailing
`Peer` column: `name[pid]`, several joined with `; `, and `(listener)` or
`(binding)` after a fallback match. Without the flag, snapshot output is
unchanged. The GUI does not show peers yet.

### Process tree

`--tree PID` narrows a snapshot or a watch to a process and its descendants.
`src/procinfo.rs` reads each process's parent and start time with the
public `proc_pidinfo(PROC_PIDTBSDINFO)`; `src/proctree.rs` is the pure
membership logic.

- A root must be identified before anything is scoped to it. A PID that is
  not running, or belongs to another user, is exit 2 rather than an empty
  result that looks like a quiet process.
- A process joins when a scan shows its parent is already a member. Once
  in, it stays until it exits, so a child adopted by launchd after its
  parent exits is still followed, along with anything it starts later. A
  one-shot snapshot has no history: it sees only who descends from the
  roots at that moment.
- Start times are wall-clock and are used for identity only, never to
  decide parentage: a clock stepped backwards can make a real child look
  older than its parent.
- A process that has exited but has not been reaped by its parent (a
  zombie) holds no sockets and counts as exited.
- A watch ends with `scope_exited` when no tracked process is left.

Limits, all inherent to sampling without privileges:

- A process that starts, opens a socket, and exits between two samples is
  not seen. Neither is a child whose parent exits before a scan sees them
  together (a double fork): it is never linked to the tree.
- `PROC_PIDTBSDINFO` is same-user unless root. A descendant running as
  another user cannot be identified or followed; its sockets are hidden
  from an unprivileged viewer anyway.
- A shorter `--interval` narrows these windows and costs more CPU: every
  sample walks every process's descriptors.

### Observing a command from its first socket

`my-agent & netboard --cli --watch --tree $!` is the short form, but the
agent runs for a moment before the first sample. To have the baseline
taken first, watch the launching shell and start the command afterwards:

```bash
umask 077
netboard --cli --watch --tree $$ > sockets.jsonl & watcher=$!
until grep -q '"type":"baseline"' sockets.jsonl; do sleep 0.1; done
my-agent
kill -TERM "$watcher"
```

The watcher is a child of the same shell, so it appears once as a tracked
process with no sockets, and a helper such as `sleep` can appear briefly.
Everything `my-agent` opens while a sample can see it is a `new` event.

## Refresh ownership and deleted rows

The refresh worker builds an immutable `Arc<Vec<Row>>`, then publishes it under a
short mutex lock. The UI retains one Arc throughout rendering and CSV export;
filtering and sorting use references into that snapshot. DNS and rendering run
outside the publication lock. Process metadata is resolved once per PID per
refresh, then cloned into owning endpoints; those strings outlive the PID cache.

A row's last live snapshot has `linger = 0`. The first missing refresh displays it
red with `linger = 2`; the second displays it red with `linger = 1`; the third
removes it. Countdown happens once per successful snapshot, never per repaint.
A returning endpoint is new again. Pausing or a failed refresh does not age rows.

## DNS workers and FFI

Eight workers receive from a bounded 256-request `crossbeam-channel` queue.
Duplicate pending/completed lookups stay suppressed. A negative answer is
suppressed for two minutes, then retried, so a transient router or resolver
failure does not hide a name for the whole session; changing the fallback
setting retries misses at once. A full queue leaves the address retryable
on a later frame. Shutdown disconnects
the sender, drains accepted requests and joins workers without a cache lock.
System DNS calls cannot be cancelled, so shutdown can wait for resolver timeouts.

### GUI fallback chain (`src/ptr.rs`)

1. `getnameinfo` (hosts file, router/ISP DNS, mDNSResponder).
2. No answer and the peer is RFC 1918 or IPv4 link-local: one legacy unicast
   mDNS PTR query (RFC 6762 section 6.7) to `224.0.0.251:5353` from an
   ephemeral port, 800 ms window. Bonjour/Avahi hosts publish reverse records
   for their own addresses even when the router has none. Only replies from
   LAN-scoped sources are read; the packet never leaves the link.
3. No answer, the peer is globally routable unicast, and the user picked
   Quad9 or Cloudflare in Options: a recursive PTR query over UDP/53 to that
   address, 1.2 s window, one retry on timeout, none on NXDOMAIN. Replies
   must come from the exact server address and port.

Loopback, multicast, documentation, CGNAT, benchmarking, reserved and IPv6
link-local/ULA addresses never go past step 1. Queries use a random ID and
OS-chosen port; replies must echo the ID, set QR, carry NOERROR without TC,
and answer the queried owner name (in-message CNAME chains for RFC 2317
delegation are followed up to 8 hops). Name decoding rejects forward or
looping compression pointers, labels containing dots or non-printable bytes,
and names over 255 bytes. Every answer, from any step, must then pass the
plain-hostname rule. The CLI stays on step 1 only.

CLI output resolves synchronously after filtering endpoints. Each unique address
is looked up at most once per CLI snapshot, with both answers and misses cached
across local and remote cells. Numeric mode and unspecified addresses skip DNS.
This cache lasts only for that snapshot; subsequent invocations resolve afresh.

`socket2::SockAddr` handles Darwin IPv4/IPv6 storage, lengths and byte order for
`getnameinfo`. Host output must contain a NUL within `NI_MAXHOST` bytes. This
preserves PTR lookups; the viewer does not perform a follow-up forward lookup
of the returned name. These dependencies
require Rust 1.60 and 1.63 respectively, below the unchanged Rust 1.85 pin.

`proc_name` returns a short, possibly truncated process name (not a bundle ID).
Its buffer follows `proc_bsdinfo.pbi_name` (`2 * MAXCOMLEN`, plus a trailing zero for libproc's `strlen`); the executable path
uses `PROC_PIDPATHINFO_MAXSIZE`. Decoding is bounded and replaces partial UTF-8.
