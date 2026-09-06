# Initialization review

Reviewed 2026-09-06, starting from `162db5dee341f507ae35fdca031a808ad889e625`.
Scope: first-party Rust modules and tests, build script, Cargo manifests,
GitHub workflows, repository hygiene, and user/contributor documentation.
This was a source review and targeted hardening pass, not a penetration
test or an audit of every transitive dependency or historical binary.

## Changes

- Added project-specific `AGENTS.md`, minimal Codex configuration, a review
  checklist, contributor instructions, and a draft PR template.
- Added consistent text endings and ignored credentials, captures, local
  agent state, and generated application output while retaining Cargo.lock.
- Replaced the checked-in executable with source-revision CI artifacts.
  Packaging now validates signatures, architectures, CLI execution, and
  permission-preserving archive extraction.
- Pinned existing Actions to full commits, disabled persisted checkout
  credentials, bounded job duration, and enforced locked builds/tests.
- Prevented unsigned PID conversion from reaching negative process/group
  signal semantics. This is defensive API hardening: the source review did
  not establish that normal kernel snapshots supply out-of-range PIDs.
- Added spreadsheet formula mitigation for CSV text from process metadata
  and DNS, with quoting and prefix regression cases.
- Replaced truncating CSV writes with exclusive private creation and tests
  for collisions, existing symlinks, dangling symlinks, and permissions.
- Replaced the yanked `url 2.5.3` pin with `2.5.8` (declared MSRV 1.63),
  updating only that package entry in Cargo.lock. The compiler pin stays 1.85.0.
- Ran cargo-audit directly with a pinned tool version to avoid the former
  audit action's attempt to publish check runs with a read-only token.

## Dependency audit follow-up

The audit found [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html)
and [RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html)
in `quick-xml 0.30.0`. Both require `quick-xml >= 0.41.0` for a fix.
The lockfile chain is `accesskit_winit -> accesskit_unix -> atspi ->
atspi-common -> zbus-lockstep / zbus-lockstep-macros -> zbus_xml 4.0.0 ->
quick-xml 0.30.0`. The separate Wayland chain already uses quick-xml 0.41.0.

`accesskit_winit 0.23.1` declares `accesskit_unix` only for Linux and BSD
targets. These commands both report `nothing to print`:

```sh
cargo tree --locked --target aarch64-apple-darwin -i quick-xml@0.30.0
cargo tree --locked --target x86_64-apple-darwin -i quick-xml@0.30.0
```

This is evidence of no build dependency path for the vulnerable version in
the supported macOS targets, not a fix to the full lockfile. The old
`zbus_xml` major cannot accept quick-xml 0.41 as a lockfile-only update.
`zbus_xml` 5.2.x dropped quick-xml but declares MSRV 1.87, above this
app's rustc 1.85.0 pin. Bumping `eframe` / AccessKit would change Darwin
accessibility, so those advisories are ignored in `.cargo/audit.toml`
as Darwin-unreachable DoS findings. Drop the ignores when `zbus_xml` 4.x
leaves the lockfile without changing Darwin AccessKit or rustc 1.85.0.
Do not disable accessibility just to remove an advisory. Unmaintained
`paste` and `ttf-parser` also need upstream tracking and are not ignored.
The yanked URL warning is addressed by the patch update above.

## Remaining limits

PID reuse can race confirmation; stronger process identity tracking would
need dedicated Darwin design and tests. DNS has no application-level expiry
or cache-size limit and can retain stale names for the session. Name
resolution is enabled by default and is not a zero-network mode. CLI text
output and clipboard rows remain raw diagnostic data; use care with control
characters from untrusted process metadata. IPv6 display may use an IPv4
address from DNS and must not be treated as a verified socket peer.

Dependency advisory results come from the Audit workflow; passing first-party
tests does not establish dependency security. Manual GUI behavior, macOS 12
runtime compatibility, and execution of the Intel slice require appropriate
Mac hardware; a universal build alone does not prove all three. Markdown
describes current behavior; existing PDF guides remain historical snapshots.

## Reference guidance

- [GitHub Actions secure use](https://docs.github.com/en/actions/reference/security/secure-use)
- [OWASP CSV injection](https://owasp.org/www-community/attacks/CSV_Injection)
- [Codex project configuration](https://learn.chatgpt.com/docs/config-file/config-basic)
