# NetBoard

Live TCP/UDP endpoint viewer for macOS. TCPView-class table: process, PID, protocol, direction, local/remote, state. Not affiliated with Microsoft or Sysinternals.

**Colors:** orange = TCP/UDP to an off-box remote (stays lit). Green = new outgoing, blue = new incoming/listen, yellow = state changed, red = just gone (2 ticks). ICMP/ping does not appear. **Off-box only** hides listeners and loopback.

**Close Connection** on Darwin cannot delete another process's TCB (Windows `SetTcpEntry` has no public equivalent). The menu terminates the owning process after confirm.

```
export PATH="$HOME/.cargo/bin:$PATH"   # rustc 1.85.0 via rustup
cargo run
cargo run -- --cli -n -a
./scripts/make-app.sh                  # dist/NetBoard.app
```

See `docs/BUILD.md`. First launch of the unsigned app: Right-click → Open.
