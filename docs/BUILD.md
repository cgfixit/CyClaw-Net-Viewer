# Build

Requires macOS 13+, Apple Silicon, **Rust 1.85.0**.

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

`.app` / DMG (ad-hoc signed, not notarized):

```
chmod +x scripts/make-app.sh
./scripts/make-app.sh          # dist/NetBoard.app
./scripts/make-app.sh --dmg    # dist/NetBoard.dmg
```

First open: Finder → Right-click `NetBoard.app` → Open. Gatekeeper blocks unsigned double-click until then.

Do not enable App Sandbox; it hides other processes' sockets.
