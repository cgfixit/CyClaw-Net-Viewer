# Agent guide

## Project and scope

CyClaw-Net-Viewer is a standalone macOS TCP/UDP endpoint viewer. The Cargo
crate and executable remain `netboard` internally. The GitHub repository is
still `cgfixit/Mac-NetViewer-EZview` until the owner renames it. It is not
the CyClaw AI repository and does not import that project's runtime or
policies.

Read `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, and the relevant source
before editing. The default branch is currently `master`; a rename to `main`
is deferred. Work on a `codex/<topic>` branch and open draft PRs against the
repository default; do not merge or push directly to the default branch
unless the user explicitly requests it. Check open PRs for overlapping work
first and use `.github/PULL_REQUEST_TEMPLATE.md`.

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

The app requires macOS 12+, Xcode Command Line Tools, and Rust 1.85.0.
Keep `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, CI, and build docs
consistent. Compatibility pins are deliberate; do not run a blanket
`cargo update` or raise the compiler requirement as incidental cleanup.

Run `./scripts/check.sh` on macOS for formatting, Clippy, and all tests.
For packaging changes also run `./scripts/make-app.sh` and verify the
signature and CLI as described in `CONTRIBUTING.md`. Use `--locked` for
builds and tests. Add focused regression tests for behavior/security fixes.
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
