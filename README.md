# CyClaw-Net-Viewer

Live TCP/UDP endpoint viewer for macOS (crate name `netboard`). TCPView-class table for CyClaw telemetry-kill watching: process, PID, protocol, direction, local/remote, state. Not affiliated with Microsoft or Sysinternals.

**Repo:** [github.com/cgfixit/Mac-NetViewer-EZview](https://github.com/cgfixit/Mac-NetViewer-EZview)

**Colors:** orange = TCP/UDP to an off-box remote (stays lit). Green = new outgoing, blue = new incoming/listen, yellow = state changed, red = just gone (2 ticks). ICMP/ping does not appear. **Off-box only** hides listeners and loopback.

**Resolve names** (default on) shows `hostname (IPv4):port` when PTR/A exist. IPv6 sockets stay listed; IPv4-mapped v6 displays as dotted IPv4.

**Close Connection** on Darwin cannot delete another process's TCB (Windows `SetTcpEntry` has no public equivalent). The menu terminates the owning process after confirm.

Requires **macOS 12+** (Monterey), Intel or Apple Silicon, **Rust 1.85.0** to rebuild.

## Run the bundled app (no Rust required)

```
git clone https://github.com/cgfixit/Mac-NetViewer-EZview.git
open Mac-NetViewer-EZview/dist/CyClaw-Net-Viewer.app
```

The binary is ad-hoc signed, not notarized. First launch: Finder → Right-click `CyClaw-Net-Viewer.app` → Open.

## Build from source

```
export PATH="$HOME/.cargo/bin:$PATH"   # rustc 1.85.0 via rustup
cargo run
cargo run -- --cli -n -a
./scripts/make-app.sh                  # dist/CyClaw-Net-Viewer.app
```

See `docs/BUILD.md`. Guides: `docs/CyClaw-Net-Viewer-User-Guide.pdf`, `docs/CyClaw-Net-Viewer-How-It-Works.pdf`.
