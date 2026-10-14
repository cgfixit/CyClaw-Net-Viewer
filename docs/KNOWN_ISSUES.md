# Known issues

- SIP / TCC-protected processes often omit socket FDs unless you run as root. Viewing never requires root; the table is just incomplete.
- No TCB close. Terminate process is SIGTERM of the owner after confirm.
- Red deleted rows linger two refresh ticks (~2s at the default 1s rate), then drop.
- ICMP (`ping`) is not TCP/UDP and never appears. Watch `curl`/`nc` or the process under test.
- `netstat2` UDP sockets have no remote address; connected UDP shows as `*:0` / Unknown. Local bindings do not establish packet direction.
- Unsigned ad-hoc `.app`. Notarization is out of scope.
- First snapshot can take a beat while libproc walks every PID.
- macOS 15+ may ask to allow CyClaw-Net-Viewer to find devices on your local network the first time Resolve names sends an mDNS query for a LAN peer. Allowing it lets LAN names resolve; denying it just leaves those peers numeric. The public DNS fallback (Options) needs outbound UDP/53; networks that block it leave names numeric.
