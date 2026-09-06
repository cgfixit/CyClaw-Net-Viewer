# Build

Requires **macOS 12+** (Monterey or newer), Intel or Apple Silicon, **Rust 1.85.0**.

Homebrew `rust` is newer and is not the pin. Use rustup:

```
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.85.0
cd Mac-NetViewer-EZview
rustc -V    # rustc 1.85.0
cargo run --locked
./scripts/check.sh
cargo run --locked -- --cli -n -a
```

`.app` / DMG (ad-hoc signed, not notarized). `MACOSX_DEPLOYMENT_TARGET=12.0`. The script builds a universal `arm64+x86_64` binary when the other target/SDK is available; otherwise it ships the host arch.

```
./scripts/make-app.sh          # dist/CyClaw-Net-Viewer.app
./scripts/make-app.sh --dmg    # dist/CyClaw-Net-Viewer.dmg
```

The bundle is ad-hoc signed, not notarized. Use the per-app approval offered
by your macOS version if Gatekeeper blocks it; do not disable system protections.

Do not enable App Sandbox: cross-process socket visibility is essential.
Hardened Runtime and notarization are separate release-engineering work and
have not been validated for this app.

Generated bundles stay out of Git. CI verifies both architectures, signature,
CLI execution, and ZIP extraction. See [CONTRIBUTING.md](../CONTRIBUTING.md)
for validation commands and [dist/README.md](../dist/README.md) for artifacts.
