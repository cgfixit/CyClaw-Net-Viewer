# Security

Report vulnerabilities privately via GitHub Security Advisories on [cgfixit/Mac-NetViewer-EZview](https://github.com/cgfixit/Mac-NetViewer-EZview/security/advisories/new). Do not file a public issue for a still-unfixed hole.

This tool lists other processes' sockets on purpose. Do not enable App Sandbox. There is no bounty and no SLA.

`cargo audit` runs weekly on `Cargo.lock`. Rustc is pinned at 1.85.0. Do not float `eframe` without checking that pin still builds.

## Runtime boundaries

- Run as a normal user. SIP/TCC and OS permissions can hide processes; do
  not disable them to fill the table. Elevated execution increases the
  consequences of process termination and file exports.
- Close Connection sends SIGTERM to the whole owning process after
  confirmation. Zero, launchd, self, and out-of-range PIDs are refused.
  PID reuse between snapshot and confirmation remains a limitation; a PID
  is not a durable process identity. Avoid terminating stale selections.
- DNS resolution is on by default and can send PTR/A queries through the
  system resolver. Numeric CLI mode avoids lookups; disabling GUI name
  resolution stops new requests, but queued requests may complete. Cached
  names are untrusted labels, not verified peer identities.
- GUI CSV exports are created exclusively with mode 0600, without following
  existing symlinks. Existing files are never overwritten. A failed write
  may leave a partial private file and is reported as a save failure.
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
