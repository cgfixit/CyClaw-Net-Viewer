# CyClaw-Net-Viewer design

TCPView-class viewer for Darwin (window title **CyClaw-Net-Viewer**). Official event colors from [Microsoft Learn — TCPView](https://learn.microsoft.com/en-us/sysinternals/downloads/tcpview): new = green, state change = yellow, deleted = red. Splits new into **outgoing** (green) and **incoming/listen** (blue). Runs on macOS 12+ Intel and Apple Silicon (`MACOSX_DEPLOYMENT_TARGET=12.0`).

**Off-box remotes** (any TCP/UDP peer that is not unspecified and not loopback) stay **orange** while they exist, so a telemetry-kill watch can see phone-home without waiting for a new/delete flash. Event colors still win. ICMP/ping is not in the socket table.

Address cells: reverse DNS plus a forward A lookup so a v6 socket can still show `hostname (1.2.3.4):port`. IPv4-mapped IPv6 is printed as IPv4. Dual-stack remains two rows.

## Snapshot

`proc_listpids` → `proc_pidinfo(PROC_PIDLISTFDS)` → `proc_pidfdinfo(PROC_PIDFDSOCKETINFO)`, via `netstat2` (same path as `lsof` / Apple DTS). Process name/path from `proc_name` / `proc_pidpath`. Reverse DNS is a background `getnameinfo` cache.

## Close connection

Windows TCPView calls `SetTcpEntry(MIB_TCP_STATE_DELETE_TCB)`. Darwin has no public TCB-delete for another process. The app offers SIGTERM of the owning PID after a confirm dialog.

## Direction

- TCP LISTEN → Listen
- TCP SYN_SENT → Out
- other TCP: In if that PID also has LISTEN on the local port, else Out
- UDP with unspecified remote → Listen, else Out
