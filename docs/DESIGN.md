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

`proc_listpids` → `proc_pidinfo(PROC_PIDLISTFDS)` → `proc_pidfdinfo(PROC_PIDFDSOCKETINFO)`, via `netstat2` (same path as `lsof` / Apple DTS). Process name/path from `proc_name` / `proc_pidpath`. Reverse DNS is a background `getnameinfo` cache.

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
  cannot prove Out. `netstat2` currently omits every UDP peer, so all actual UDP
  rows are Unknown and show `*:0` for the remote. No packet capture is added.
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
| `endpoint` | A difference between two samples | `event` `existing`, `new`, `changed` or `closed`; `process` with `pid`, `start` or null, `name`, `path`; `proto` `tcp`/`udp`; `ip_version` 4/6; `dir` `in`/`out`/`listen`/`unknown`; `local` and `remote` as `{"ip","port"}`; `state` or null; `prev_state` on `changed`; `off_box` |
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
  or a UDP socket shows remote `0.0.0.0:0`. IPv4-mapped IPv6 unwraps as it
  does elsewhere.
- **Process identity** is the PID plus `start`, the kernel's creation time
  to the microsecond. The same PID with another start time is another
  process: its old endpoints are `closed` and the new ones are `new`. An
  endpoint whose owner exits between the socket sample and the identity
  lookup keeps the start time it had in the previous sample.
- **Names and paths** are read fresh each sample for endpoints, and once
  when a process is first tracked. They are attacker-chosen text. JSON
  strings escape controls, bidi and zero-width formatting, and U+2028/2029
  as `\uXXXX`: a parser reads the exact value back, the line stays one line,
  and a terminal shows inert ASCII.
- **Cadence** is fixed from the start time. A sample that overruns its slot
  skips to the next future slot instead of sampling back to back. With
  `--for`, the last sample is taken at the limit. `max_gap_ms` is the
  longest time between two successful samples, failed ones included in the
  gap; it is null with fewer than two.
- **Failures.** A failed or panicking sample writes `error`, changes no
  state, and closes nothing; the next good sample is compared with the last
  good one. Any failed sample makes the exit status 2.
- **Stopping.** SIGINT and SIGTERM set a flag that the wait loop checks
  every 50 ms, so the `end` record is still written. A closed reader is
  noticed on the next write and ends the watch quietly with status 0.
- **No DNS.** Watch mode has no name-resolution path at all; `-n` and `-a`
  are implied and `-c` is refused.

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
