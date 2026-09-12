---
name: netviewer-sandbox
description: Sandbox verification for CyClaw-Net-Viewer (crate netboard) on the current checkout. Use when asked to verify, smoke-test, validate, or sandbox this repo, or when the user runs /netviewer-sandbox.
---

# NetViewer sandbox

Locate the `netboard` git root and confirm origin `cgfixit/CyClaw-Net-Viewer`.
Read `AGENTS.md`, `scripts/check.sh`, and `CONTRIBUTING.md`. Record HEAD,
OS, and `rustc --version`. Run from that checkout. This is not CyClaw;
do not run CyClaw sandbox, pytest, or `verify_ci_emulation.py`.

Run `$netviewer-invariants` first. A failed invariant check ends the
sandbox; do not skip it.

**Windows/Linux:** `git diff --check`. If rustc exists, `cargo fmt --all -- --check`.
That is not a runtime pass. Use this SHA's macOS CI
(`.github/workflows/ci.yml`: Rust 1.85.0 and stable on macos-14 and
macos-15-intel). Do not remove `compile_error!`, mock the collector, or
claim GUI interaction.

**macOS:** `./scripts/check.sh` (fmt, Clippy `-D warnings`, `cargo test --locked`,
including the Bash egress fixture). Focused observation:
`./scripts/emulate-egress-sandbox.sh` via `$emulate-egress-sandbox`.
Packaging in scope: `$verify-netviewer-bundle`. Compiler pin or
Dependabot: `$verify-netviewer-rust`.

Never disable SIP/TCC/Gatekeeper, send real credentials, rely on public
DNS, or commit captures, bundles, or `target/`.

```
NetViewer sandbox
HEAD: <sha>
OS / rustc: <uname> / <version>
invariants: PASS/FAIL
check.sh: PASS/FAIL/SKIP
egress: PASS/FAIL/SKIP
bundle: PASS/FAIL/SKIP
CI URLs: <this SHA or none>
GUI launched: yes/no
unverified: <items>
```
