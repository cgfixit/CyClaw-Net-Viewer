# Known issues

- SIP / TCC-protected processes often omit socket FDs unless you run as root. Viewing never requires root; the table is just incomplete.
- No TCB close. Terminate process is SIGTERM of the owner after confirm.
- Red deleted rows linger two refresh ticks (~2s at the default 1s rate), then drop.
- ICMP (`ping`) is not TCP/UDP and never appears. Watch `curl`/`nc` or the process under test.
- A UDP socket shows a peer only if the process called `connect` on it (QUIC and most DNS clients do). One that uses `sendto` for each datagram has no peer in the kernel's socket record and shows `*:0`. UDP direction is Unknown either way: local bindings do not establish packet direction.
- Local peers are a match within one snapshot. A peer in a process this user cannot read is not found, a TCP client names the listener until the server accepts, and the GUI does not show peers yet (`--peers` and watch mode do).
- Watch mode samples; it does not trace. A connection that opens and closes inside one `--interval` is never reported, and `max_gap_ms` in the `end` record is the longest such blind interval. Use an enforcing control (a sandbox profile or a firewall rule) when traffic must be impossible, and the watch as the independent check.
- `--tree` misses a child whose parent exits before a sample sees them together (a double fork), without a count. A descendant running as another user is not followed either, but it is counted in `unreadable_processes` and makes a policy verdict `incomplete`. A one-shot `--cli --tree` snapshot only knows who descends from the root at that instant.
- A snapshot fails only if processes cannot be listed at all. The GUI then keeps its last good rows; a watch writes an `error` record, counts the sample as failed, and exits 2 at the end. A process whose descriptors cannot be listed is skipped. A watch counts those in scope (`unreadable_processes`); the snapshot CLI and the GUI do not show the count yet.
- Without root, an unscoped watch can never return a `pass` verdict: it cannot read other users' processes, so it is always `incomplete`. Scope it with `--tree`, or run as root.
- An unconnected UDP socket bound to a port counts as an exposed receiver even when its owner only sends with it, because the kernel records no peer that would show otherwise. A `sendto` client on `0.0.0.0` is therefore a `listener` breach under a policy that has no `listen` rule for it.
- Exposed listeners take the orange off-box fill in the GUI and stay visible under **Off-box only**. That GUI change is covered by unit tests of the filter and color rule; the README screenshots predate it.
- A watch whose reader has gone away notices on its next write, which for a quiet scope can be its `end` record.
- Unsigned ad-hoc `.app`. Notarization is out of scope.
- First snapshot can take a beat while libproc walks every PID.
- macOS 15+ may ask to allow CyClaw-Net-Viewer to find devices on your local network the first time Resolve names sends an mDNS query for a LAN peer. Allowing it lets LAN names resolve; denying it just leaves those peers numeric. The public DNS fallback (Options) needs outbound UDP/53; networks that block it leave names numeric.
