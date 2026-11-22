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
  Immediately before signaling, the PID's current process name is re-read
  and must equal the confirmed name, otherwise nothing is sent. That narrows
  but does not close PID reuse (a reused PID with the same short name still
  matches); a PID is not a durable process identity. Avoid terminating
  stale selections. Keyboard shortcuts are suspended while the dialog is
  open.
- GUI name resolution is off by default. Enabling it can send PTR
  queries through the system resolver. Numeric CLI mode (`-n`) avoids
  lookups; disabling GUI name resolution stops new requests, but queued
  requests may complete. PTR answers are kept only when they are a plain
  ASCII hostname (letters, digits, `-`, `_`, dots; at most 253 bytes;
  non-numeric last label), so an answer cannot imitate an IP address or
  carry spaces, controls, or bidi marks. When the system resolver has no
  answer, a LAN peer (RFC 1918 / link-local IPv4) gets one mDNS query on
  the local link. **Options → Fallback DNS** is Off by default; choosing
  Quad9 or Cloudflare sends globally routable peer IPs that the system
  cannot name to that provider in cleartext over UDP/53. Private, loopback,
  documentation, and other special-purpose addresses are never sent to it.
  Fallback replies must match the query ID, source, and question name.
  Misses are retried after two minutes. Cached names are still untrusted
  labels, not verified peer identities. The GUI resolver keeps at most 4096 completed
  addresses; evicted names may be looked up again.
- Watch mode (`--watch`) only observes. It never resolves a name, opens no
  socket of its own, and writes to stdout alone. Its stream holds process
  names, executable paths, PIDs and peer addresses: redirect it under
  `umask 077`, because stdout does not get the GUI's private-file policy.
  Names and paths, of a process and of its local peers, are attacker-chosen
  and are declared in the `start` record's `untrusted_fields`; JSON strings
  escape controls, bidi and
  zero-width characters so a hostile name cannot forge a record or reorder
  a terminal line, but the text itself is unchanged for a parser. Do not
  hand a capture to a language model as trusted input. A watch is a series
  of samples: sockets shorter than the gap between two samples are not
  seen, a failed sample observes nothing, and an empty stream does not
  establish that nothing was sent. Exit 0 means every sample succeeded,
  not that the watched processes were quiet.
- `--policy FILE` reads one operator-written file: a regular file of at
  most 64 KiB of UTF-8, refused before sampling if it does not parse, with
  an error that names the line and never echoes its text. No rule ships in
  the binary; `docs/examples/` holds starting points. A policy only
  classifies what a watch saw. It blocks nothing and signals nothing, and
  its `pass` verdict (exit 0) means no violation was seen, with the
  sampling gap above. A process the watch cannot read makes the verdict
  `incomplete` (exit 2) instead of passing silently.
- `--tree PID` follows observed parentage, keyed on PID plus kernel start
  time. It is not containment: a process whose parent exits before a scan
  sees both, or that runs as another user, is not followed (one running as
  another user is counted in `unreadable_processes`). A root that
  cannot be identified is refused (exit 2) instead of printing nothing.
  SIGINT and SIGTERM end a watch after its `end` record; the handler only
  sets a flag.
- Sockets are read with first-party libproc calls in `src/sockets.rs`. The
  kernel record layout is asserted at compile time and only a full-length
  answer is read, so a descriptor that closes mid-scan is skipped rather
  than read from stale memory. A local peer (`--peers`, watch `peer`) is a
  match within one snapshot: it names who held the other end then, it is
  not found when that process cannot be read, and a null peer does not
  mean a connection stayed on this Mac.
- GUI CSV exports go to `~/Downloads` (or `~` when that folder is missing)
  and are created exclusively with mode 0600, without following
  existing symlinks. Existing files are never overwritten. A failed write
  may leave a partial private file and is reported as a save failure.
- Reveal in Finder uses `open -R -- <path>` so a dash-leading `proc_pidpath`
  cannot become an `open` flag. Spawn errors may be shown in the status bar.
- Process names and other CSV text can contain spreadsheet formulas. The
  exporter quotes dangerous text and prefixes an apostrophe. Spreadsheet
  import/re-save behavior varies; import columns as text and do not enable
  formulas or external links in untrusted captures. CLI stdout redirection
  does not inherit the GUI's private-file creation policy.
- Copy line and Copy remote replace Unicode control characters and bidi or
  zero-width formatting characters with spaces before writing the
  pasteboard. Copy line joins the cleaned fields with tabs. GUI cells, the
  termination dialog, and CLI text rows apply the same cleaning to process
  names and paths so a hostile name cannot forge rows or reorder columns.
  CSV output keeps the raw value inside RFC 4180 quoting.

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

## Known dependency advisories (rechecked 2026-11-22)

`cargo audit` 0.22.2 reads the whole lockfile, including crates that the
macOS binaries never compile.

`rustix` 1.1.6 replaces 1.1.5 (`RUSTSEC-2026-0486`: xattr helpers could
mark unwritten bytes initialized when given a zero-length buffer). The
0.38 line does not have that API and is not affected. Neither version is
in the `aarch64-apple-darwin` or `x86_64-apple-darwin` trees. 1.1.6 is
what the Linux and BSD AccessKit and Wayland crates resolve. Its
`rust-version` is 1.65.

`url` is pinned to exactly 2.5.9 (`RUSTSEC-2026-0592`: `Url::origin()`
recursed on nested `blob:` URLs and could overflow the stack). 2.5.9
declares `rust-version` 1.63. That release also bounds `..` segment
parsing and follows the URL Standard for a few edge cases: caret encoding
in paths, `file:` URLs with a drive letter, backslashes in non-special
ports, and a non-recursive origin for `blob:` URLs whose inner scheme is
not `http`, `https`, or `file`. Application code does not call `Url`.
`webbrowser` 1.2.4, pulled in by egui, does. `form_urlencoded` moves to
1.2.3 because 2.5.9 requires `^1.2.3` (`rust-version` 1.51).

`quick-xml` 0.30.0 remains next to 0.41.0. `RUSTSEC-2026-0194` and
`RUSTSEC-2026-0195` are high-severity denial-of-service findings fixed
only in `quick-xml >= 0.41.0`. The 0.30 copy is Linux and BSD only:

`eframe` 0.31.1 → `egui-winit` 0.31.1 → `accesskit_winit` 0.23.1 →
`accesskit_unix` 0.13.1 → `accesskit_atspi_common` 0.10.1 →
`atspi-common` 0.6.0 → `zbus-lockstep` 0.4.4 → `zbus_xml` 4.0.0 →
`quick-xml` 0.30.0.

`cargo update -p quick-xml@0.30.0 --precise 0.41.0` cannot select 0.41.0.
`zbus_xml` 4.0.0 requires `quick-xml ^0.30`, and 4.0.0 is the only 4.x
release. `zbus_xml` 5.0.2 and 5.1.0 still depend on `quick-xml` `^0.36`
and `^0.38`. `zbus_xml` 5.2 and later drop `quick-xml` but declare
`rust-version` 1.87, above this crate's 1.85.0 pin, and `zbus-lockstep`
0.6.0 (the release that requires `zbus_xml ^5.2`) is outside
`atspi-common`'s `^0.4.4`. `accesskit_winit` 0.23 cannot take
`accesskit_unix` 0.14 or newer, and `egui-winit` 0.31.1 cannot take
`accesskit_winit` past 0.23, so removing 0.30 means leaving the exact
`eframe` 0.31.1 pin and changing Darwin accessibility. Both macOS trees
still print nothing for `quick-xml@0.30.0`. The two IDs stay ignored in
`.cargo/audit.toml` as Darwin-unreachable DoS findings so the Audit job
can pass. Drop the ignores when `zbus_xml` 4.x leaves the lockfile
without changing Darwin AccessKit or rustc 1.85.0. Do not turn
accessibility off to clear the advisory.

Unmaintained warnings are not ignored. None of these has a patched
release, so a lockfile bump cannot clear them:

- `paste` 1.0.15 (`RUSTSEC-2024-0436`) via `accesskit_windows` on
  Windows. The suggested replacement is a different crate (`pastey`).
- `ttf-parser` 0.25.1 (`RUSTSEC-2026-0192`) via `epaint` → `ab_glyph` →
  `owned_ttf_parser` on Darwin. Upstream points at `skrifa`, which is an
  egui font-stack change.
- `zbus_names` 3.0.0, `zvariant` 4.2.0, and `zvariant_derive` 4.2.0
  (`RUSTSEC-2026-0371`, `RUSTSEC-2026-0370`, `RUSTSEC-2026-0372`) on the
  Linux/BSD zbus 4 stack. They were folded into zbus 6, which is outside
  this pin.

There is no `deny.toml`. The Audit workflow runs `cargo audit`.
