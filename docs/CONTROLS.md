# Controls

| Key / click | Action |
|---|---|
| Filter box | substring on process, PID, addrs, state, path |
| Color remotes | orange fill on off-box peers (default on) |
| Off-box only | hide listeners / loopback / `*:*` |
| Space | pause / resume |
| R | refresh now |
| N | toggle reverse DNS |
| Delete / Backspace | terminate selected process (confirm) |
| Click header | sort (click again reverses) |
| Click row | select |
| Right-click row | copy line, copy remote, terminate, reveal in Finder |
| File → Save CSV | write `cyclaw-net-viewer-YYYYMMDD-HHMMSS.csv` to `~/Downloads` (else `~`); the status bar shows the path |
| File → Terminate process | same as terminate; enabled only for ESTABLISHED TCP |

Single-key shortcuts are ignored while any control has keyboard focus
(typing in the filter, or a Tab-focused button), while the terminate
confirmation is open, and when Cmd, Ctrl, or Option is held.
