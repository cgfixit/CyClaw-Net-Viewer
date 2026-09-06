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
