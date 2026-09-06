# CyClaw-Net-Viewer design

TCPView-class viewer for Darwin (window title **CyClaw-Net-Viewer**). Official event colors from [Microsoft Learn — TCPView](https://learn.microsoft.com/en-us/sysinternals/downloads/tcpview): new = green, state change = yellow, deleted = red. Splits new into **outgoing** (green) and **incoming/listen** (blue). Runs on macOS 12+ Intel and Apple Silicon (`MACOSX_DEPLOYMENT_TARGET=12.0`).

**Off-box remotes** (any TCP/UDP peer that is not unspecified and not loopback) stay **orange** while they exist, so a telemetry-kill watch can see phone-home without waiting for a new/delete flash. Event colors still win. ICMP/ping is not in the socket table.

Address cells prefer, in order: `hostname (IPv4):port` when Resolve names is on; a non-unspecified IPv4 (DNS A, IPv4-mapped unwrapping, or a unique current-row twin sharing pid/proto/remote port); otherwise `[v6]:port`. Two or more distinct IPv4 remotes on that key keep the native v6 form. Dual-stack remains two rows. The twin is display-only and does not change `EndpointKey`.

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
Duplicate pending/completed lookups stay suppressed, including negative answers;
a full queue leaves the address retryable on a later frame. Shutdown disconnects
the sender, drains accepted requests and joins workers without a cache lock.
System DNS calls cannot be cancelled, so shutdown can wait for resolver timeouts.

CLI output resolves synchronously after filtering endpoints. Each unique address
is looked up at most once per CLI snapshot, with both answers and misses cached
across local and remote cells. Numeric mode and unspecified addresses skip DNS.
This cache lasts only for that snapshot; subsequent invocations resolve afresh.

`socket2::SockAddr` handles Darwin IPv4/IPv6 storage, lengths and byte order for
`getnameinfo`. Host output must contain a NUL within `NI_MAXHOST` bytes. This
preserves PTR lookups; forward A resolution remains separate. These dependencies
require Rust 1.60 and 1.63 respectively, below the unchanged Rust 1.85 pin.

`proc_name` returns a short, possibly truncated process name (not a bundle ID).
Its buffer follows `proc_bsdinfo.pbi_name` (`2 * MAXCOMLEN`, plus a trailing zero for libproc's `strlen`); the executable path
uses `PROC_PIDPATHINFO_MAXSIZE`. Decoding is bounded and replaces partial UTF-8.
