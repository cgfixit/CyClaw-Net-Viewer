# Build

Requires **macOS 12+** (Monterey or newer), Intel or Apple Silicon, **Rust 1.85.0 or newer**.

Rust 1.85.0 remains the repository default and minimum. CI also runs the
current stable toolchain on both Mac architectures, including Clippy, tests,
and release builds, with `--locked`. Use rustup to select the compiler:

```
export PATH="$HOME/.cargo/bin:$PATH"
git clone https://github.com/cgfixit/CyClaw-Net-Viewer.git
rustup toolchain install 1.85.0
cd CyClaw-Net-Viewer
rustc -V    # rustc 1.85.0
cargo run --locked
./scripts/check.sh
cargo run --locked -- --cli -n -a
```

For current stable, run `rustup update stable`, then
`RUSTUP_TOOLCHAIN=stable ./scripts/check.sh` and
`cargo +stable build --release --locked`. Installing a newer compiler alone
does not override `rust-toolchain.toml`. The CI job logs print the actual
compiler and architecture; weekly runs check new stable releases. Intermediate
Rust releases are not individually tested. CI runners validate their installed
macOS versions, not every macOS version back to the deployment target of 12.
The GUI is compiled, while runtime checks cover the CLI and Darwin collector;
interactive window behavior still needs a manual Mac session.

The [egress harness](EGRESS_SANDBOX.md) is part of the test suite and can also
be run with `./scripts/emulate-egress-sandbox.sh`.

## Verified compiler baseline

On 2026-09-07 UTC, revision `f9d16b95e8b63b91cb487843c08dd4b7f6f51ba9`
passed [all four native CI jobs](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/runs/34078050328):

| Compiler | Apple Silicon (`macos-14`) | Intel (`macos-15-intel`) |
| --- | --- | --- |
| Rust 1.85.0 | PASS | PASS |
| Rust 1.98.1 (current stable at verification) | PASS | PASS |

Each job passed formatting, Clippy with warnings denied, all 50 tests,
the locked release build, and bundle metadata validation. The tests include
real Darwin TCP/UDP sockets, native Bash traffic, numeric CLI attribution,
descriptor closure, exports, and diff behavior. The separately verified
[bundle round trip](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/runs/34077803168)
at `9039b96` checked universal architecture, signature, checksum, extracted
metadata, and extracted CLI execution. Subsequent code changes in `f9d16b9`
only adjusted test references and compiler installation in CI.
Interactive GUI operation, optional LAN egress, and every supported macOS
release were not exercised by these runs.

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
CLI execution, and ZIP extraction. Public downloads use a
[GitHub Release](https://github.com/cgfixit/CyClaw-Net-Viewer/releases/latest).
See [CONTRIBUTING.md](../CONTRIBUTING.md) for validation commands and
[dist/README.md](../dist/README.md) for artifact layout.
