# Build

Requires **macOS 12+** (Monterey or newer), Intel or Apple Silicon, **Rust 1.85.0**.

Homebrew `rust` is newer and is not the pin. Use rustup:

```
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.85.0
cd netboard
rustc -V    # rustc 1.85.0
cargo run
cargo test
cargo run -- --cli -n -a
```

`.app` / DMG (ad-hoc signed, not notarized). `MACOSX_DEPLOYMENT_TARGET=12.0`. The script builds a universal `arm64+x86_64` binary when the other target/SDK is available; otherwise it ships the host arch.

```
chmod +x scripts/make-app.sh
./scripts/make-app.sh          # dist/CyClaw-Net-Viewer.app
./scripts/make-app.sh --dmg    # dist/CyClaw-Net-Viewer.dmg
```

First open: Finder → Right-click `CyClaw-Net-Viewer.app` → Open. Gatekeeper blocks unsigned double-click until then.

Do not enable App Sandbox or Hardened Runtime; they hide other processes' sockets.
