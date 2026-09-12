# Agent guide

## Project and scope

CyClaw-Net-Viewer is a standalone macOS TCP/UDP endpoint viewer. The Cargo
crate and executable remain `netboard` internally. The GitHub repository is
`cgfixit/CyClaw-Net-Viewer`. It is not
the CyClaw AI repository and does not import that project's runtime or
policies.

Read `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, and the relevant source
before editing. The default branch is `main`. Work on a `codex/<topic>` branch
and open draft PRs against the repository default; do not merge or push
directly to the default branch unless the user explicitly requests it. Check
open PRs for overlapping work first and use `.github/PULL_REQUEST_TEMPLATE.md`.

## Source map

- `src/main.rs`, `src/cli.rs`: GUI/CLI dispatch and snapshot output.
- `src/snapshot.rs`: Darwin socket enumeration, process metadata, formatting.
- `src/diff.rs`: endpoint identity and highlight lifecycle.
- `src/dns.rs`: background PTR/A resolution and cache.
- `src/app.rs`: egui table, controls, export, termination confirmation.
- `src/kill.rs`, `src/export.rs`: process signaling and exclusive CSV writes.
- `tests/`: diff regression cases and a live loopback listener test.
- `scripts/`: macOS validation and bundle generation.
- `tools/`: optional Python PDF generators, not application dependencies.

## Build and validation

The app requires macOS 12+, Xcode Command Line Tools, and Rust 1.85.0 or newer.
Rust 1.85.0 remains the pinned minimum; CI also validates current stable on
Intel and Apple Silicon. Use `RUSTUP_TOOLCHAIN=stable ./scripts/check.sh`
for the latter; installing stable alone does not override the pin.
Keep `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, CI, and build docs
consistent. Compatibility pins are deliberate; do not run a blanket
`cargo update` or raise the compiler requirement as incidental cleanup.

Run `./scripts/check.sh` on macOS for formatting, Clippy, and all tests.
For packaging changes also run `./scripts/make-app.sh` and verify the
signature and CLI as described in `CONTRIBUTING.md`. Use `--locked` for
builds and tests. Add focused regression tests for behavior/security fixes.
The native Bash egress fixture is part of `cargo test --locked`; run
`./scripts/emulate-egress-sandbox.sh` for a focused observation/cleanup check.
Shared Agent Skills live in `.agents/skills` (canonical `SKILL.md` plus
Codex `agents/openai.yaml`). Invoke `$skill-name` in Codex, or load the
matching folder. Claude Code stubs under `.claude/skills` only redirect
here. Copying a skill out of the repo does not copy the application.

| Skill | Load when |
| --- | --- |
| `emulate-egress-sandbox` | Bash TCP fixture / Off-box observation |
| `verify-netviewer-rust` | Compiler pin, lockfile, Rust CI |
| `verify-netviewer-bundle` | `make-app.sh`, lipo, codesign, ZIP round-trip |
| `netviewer-sandbox` | Verify / smoke / sandbox this checkout |
| `refactor-netviewer` | Architecture or module-structure cleanup |
| `netviewer-invariants` | Before `src/` / Cargo / scripts / workflow edits |

`optimize-netviewer` remains the installable copy under `.codex/skills`.
Windows and Linux cannot run this Darwin library: report that limitation
and use the macOS CI results, never remove the platform guard to fake a pass.

## Security and behavior boundaries

- Viewing should work without root, with incomplete visibility documented.
  Do not disable SIP, TCC, Gatekeeper, or other system protections.
- Keep process termination explicit and confirmed. SIGTERM ends the entire
  process; it does not close one socket. Reject zero, self, and invalid PIDs.
  Tests must not signal unrelated processes or process groups.
- Treat process names, paths, DNS answers, and captured socket data as
  untrusted. Preserve CSV formula mitigation and exclusive private saves.
- DNS resolution causes network traffic. Numeric CLI mode must avoid DNS.
  Do not add analytics, remote reporting, or a required network service.
- Preserve the existing endpoint key, two-tick deletion lifecycle, and
  event-color precedence unless the task calls for a behavior change.
- Keep FFI changes small, document pointer/length assumptions, and validate
  on macOS. Do not introduce shell interpolation for process metadata.
- Keep GitHub Actions permissions read-only, checkout credentials disabled,
  and action references pinned to reviewed full commit hashes.

## Repository hygiene

Do not commit credentials, signing keys, real socket captures, CSV exports,
agent sessions, caches, build output, or generated app bundles. Commit
`Cargo.lock`. Keep shared `.codex/` guidance small and portable; personal
model, authentication, trust, and machine paths belong in user settings.
Summarize what changed, what ran, the exact CI result, and remaining limits.
