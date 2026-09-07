# Security

Report vulnerabilities privately via GitHub Security Advisories on [cgfixit/CyClaw-Net-Viewer](https://github.com/cgfixit/CyClaw-Net-Viewer/security/advisories/new). Do not file a public issue for a still-unfixed hole.

This tool lists other processes' sockets on purpose. Do not enable App Sandbox. There is no bounty and no SLA.

`cargo audit` runs weekly on `Cargo.lock`, on relevant PRs, and on relevant
`main` pushes. Rustc is pinned at 1.85.0. Do not float `eframe` without
checking that pin still builds. The audit runs directly in CI with read-only
permissions and preserves advisory failures; it does not publish a separate
check run requiring write access.

## Runtime boundaries

- Run as a normal user. SIP/TCC and OS permissions can hide processes; do
  not disable them to fill the table. Elevated execution increases the
  consequences of process termination and file exports.
- Terminate process sends SIGTERM to the whole owning process after
  confirmation. Zero, launchd, self, and out-of-range PIDs are refused.
  PID reuse between snapshot and confirmation remains a limitation; a PID
  is not a durable process identity. Avoid terminating stale selections.
- GUI name resolution is off by default. Enabling it can send PTR
  queries through the system resolver. Numeric CLI mode (`-n`) avoids
  lookups; disabling GUI name resolution stops new requests, but queued
  requests may complete. Cached names are untrusted labels, not verified
  peer identities.
- GUI CSV exports are created exclusively with mode 0600, without following
  existing symlinks. Existing files are never overwritten. A failed write
  may leave a partial private file and is reported as a save failure.
- Reveal in Finder uses `open -R -- <path>` so a dash-leading `proc_pidpath`
  cannot become an `open` flag. Spawn errors may be shown in the status bar.
- Process names and other CSV text can contain spreadsheet formulas. The
  exporter quotes dangerous text and prefixes an apostrophe. Spreadsheet
  import/re-save behavior varies; import columns as text and do not enable
  formulas or external links in untrusted captures. CLI stdout redirection
  does not inherit the GUI's private-file creation policy.

## Build and distribution

Only source and the application lockfile are maintained in Git; generated
app bundles come from builds. CI uses read-only repository permissions,
checkout without persisted credentials, full action commit pins, and
locked Cargo builds. Dependabot proposes Action and Cargo updates for review.
The toolchain action is intentionally excluded because the compiler is pinned.

Bundles use an ad-hoc signature, not Developer ID signing or notarization.
Verify artifacts against the intended source revision; a checksum alone
does not authenticate a publisher. Do not turn off Gatekeeper globally.

See [the initialization review](docs/INITIAL_REVIEW.md) for the scope and
remaining limitations of the repository review.

## Known dependency advisories (2026-09-06)

The full lockfile audit reports `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`
against `quick-xml 0.30.0`, pulled through the Linux/BSD accessibility stack.
It is absent from both supported macOS target dependency trees. Those two
IDs are ignored in `.cargo/audit.toml` as Darwin-unreachable DoS findings
so the Audit job can pass without changing the macOS binary, AccessKit, or
the rustc 1.85.0 pin. Drop the ignores when `zbus_xml` 4.x leaves the
lockfile. See the review for evidence and follow-up. The audit also reports
unmaintained `paste` and `ttf-parser` packages; those are not ignored.
