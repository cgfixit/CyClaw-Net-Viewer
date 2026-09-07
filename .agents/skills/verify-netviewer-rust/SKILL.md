---
name: verify-netviewer-rust
description: Verify CyClaw-Net-Viewer against its minimum Rust toolchain and current stable on Intel and Apple Silicon macOS. Use for compiler compatibility, dependency changes, or Rust CI failures.
---

# Verify NetViewer Rust compatibility

Find the `netboard` checkout and read AGENTS.md, Cargo.toml, Cargo.lock,
rust-toolchain.toml, scripts/check.sh, and .github/workflows/ci.yml. Record
HEAD and actual compiler versions; derive the minimum version from the files.
Preserve the lockfile and intentional pins unless dependency changes are
part of the request. Never use a blanket `cargo update` to fix compiler CI.

On macOS, run the pinned `./scripts/check.sh`, then
`RUSTUP_TOOLCHAIN=stable ./scripts/check.sh` and
`cargo +stable build --release --locked`. Install/update stable with rustup
when necessary and authorized by the verification task. The environment
override is essential: installing stable alone leaves the repository pin active.
The full test suite includes the Bash egress observer and real CLI subprocesses.

For a PR, inspect the exact head's CI matrix: pinned and stable compilers on
both macos-14 (Apple Silicon) and macos-15-intel. Verify all matrix results,
including release builds; a successful Intel compile is not an ARM runtime
test. If stable Clippy introduces new warnings, prefer MSRV-compatible fixes
or narrowly explained lint handling over global warning suppression.

On Windows/Linux, run formatting and workflow lint locally, then use native
macOS CI for the compiler/runtime evidence. Do not remove the Darwin guard,
mock the collector, or report compilation as GUI interaction verification.
Report exact versions, architectures, revision, check URLs, failures and
unverified OS/GUI coverage. Update docs/BUILD.md and README compatibility
claims only to the level supported by those results.
