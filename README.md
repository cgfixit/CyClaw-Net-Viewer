# NetBoard

Live TCP/UDP endpoint viewer for macOS. TCPView-class table: process, PID, protocol, direction, local/remote, state. Not affiliated with Microsoft or Sysinternals.

**Colors:** green = new outgoing, blue = new incoming (including new listeners), yellow = TCP state changed, red = just gone (2 ticks).

**Close Connection** on Darwin cannot delete another process's TCB (Windows `SetTcpEntry` has no public equivalent). The menu terminates the owning process after confirm.

```
export PATH="$HOME/.cargo/bin:$PATH"   # rustc 1.85.0 via rustup
cargo run
cargo run -- --cli -n -a
./scripts/make-app.sh                  # dist/NetBoard.app
```

See `docs/BUILD.md`. First launch of the unsigned app: Right-click → Open.
