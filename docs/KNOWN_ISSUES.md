# Known issues

- SIP / TCC-protected processes often omit socket FDs unless you run as root. Viewing never requires root; the table is just incomplete.
- No TCB close. Close Connection is SIGTERM of the owner after confirm.
- Red deleted rows linger two refresh ticks (~2s at the default 1s rate), then drop.
- `netstat2` UDP sockets have no remote address; connected UDP shows as `*:0` / Listen-shaped.
- Unsigned ad-hoc `.app`. Notarization is out of scope.
- First snapshot can take a beat while libproc walks every PID.
