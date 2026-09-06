---
name: optimize-netviewer
description: Find and implement evidence-backed correctness, performance, safety, and maintainability improvements in CyClaw-Net-Viewer (GitHub: cgfixit/Mac-NetViewer-EZview; crate/binary netboard). Use for optimize-netviewer, NetViewer optimization, targeted bottleneck investigations, or a repository improvement pass. Respect audit-only scope, deduplicate current PRs, preserve macOS and Rust compatibility, and publish focused draft PRs when requested or already authorized. Do not apply to the CyClaw AI repository or unrelated network tools.
---

# Optimize NetViewer

Run one bounded improvement pass in CyClaw-Net-Viewer
(`cgfixit/Mac-NetViewer-EZview`). Start with a
reproducible defect, measured cost, or concrete validation gap. Implement the
smallest useful change within the user's scope; do not invent a finding quota.
For an audit-only request, return evidence and recommendations without edits.
For an implementation request, finish authorized work without asking again.
Carry existing publication authorization forward; do not infer remote-write
permission solely from the skill name. Never merge or push the default branch
(`master` today, `main` after the Settings rename) unless the user explicitly
requests it.

## Establish the live baseline

1. Locate the real Git root with `git rev-parse --show-toplevel`; a parent folder
   may contain a separate, empty repository. Confirm the origin repository and
   the `netboard` package before operating. Run commands from that checkout,
   never from the installed skill folder. If invoked from another repository,
   locate an already-known matching checkout or ask for its location; do not
   modify the unrelated repository.
2. Inspect `git status --short --branch`, upstream, recent commits and local
   changes. Fetch origin and record the base SHA and current head. Use the
   current remote default (`origin/master` today) for independent work.
   Fast-forward only when resyncing a clean tracking branch; never reset,
   discard, or stash unrelated work silently.
   Use a separate worktree when an active or dirty branch must be preserved.
   If network access is unavailable, label the baseline stale and continue
   independent inspection without claiming synchronization or PR deduplication.
3. Read `AGENTS.md`, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, the relevant
   `docs/` pages, and `.codex/checklists/review.md`. Inspect hidden files with
   `rg --files --hidden` before declaring repository guidance missing. Prefer
   the repository copy of this skill when installed and shared copies differ.
4. Read `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `scripts/check.sh` and
   the active workflows. Re-derive versions, dependencies and validation from
   source. The baseline targets macOS 12+, Intel and Apple Silicon, Rust 1.85.0,
   eframe/egui, netstat2 and Darwin libproc; do not freeze these as eternal pins.
5. List open PRs using available GitHub tools or a verified GitHub CLI. Inspect
   the diffs and current review/check state of plausible overlaps. On Windows,
   verify `gh` resolves to the GitHub CLI, not an unrelated executable. Never
   print credentials or expand tokens into command output to diagnose access.

## Select an improvement

Inspect callers and tests, not just isolated lines. For each surviving candidate,
record the trigger, file/line evidence, user impact, smallest fix and meaningful
verification. Reproduce reported bugs before accepting their premise; a large
file, a newer dependency, or a suspected clone is not enough. Recheck prior
accepted risks against the current code and platform instead of maintaining a
permanent skip list. Announce the selected scope before editing.

Use this map to choose a bounded path through the repository:

| Area | Inspect and preserve |
| --- | --- |
| `src/snapshot.rs` | PID attribution, IPv4/IPv6 and wildcard endpoints, process name/path decoding, available netstat2 fields, direction evidence. A local UDP binding does not prove traffic direction; do not invent missing remote peers or call proc_name a bundle-ID API. |
| `src/diff.rs`, `tests/diff.rs` | Endpoint identity `(pid, proto, ip_ver, local, remote)`, state transitions, returning keys, exact deleted-row visibility. Trace every tick before alleging an off-by-one error. |
| `src/app.rs` | Refresh ownership, lock duration, frame-stable rows, filtering/sorting, selection, repaint/pause, event-color precedence, CSV and termination flows. Measure allocations or work at representative synthetic row counts; do not trade stable snapshots for a cosmetic speedup. |
| `src/dns.rs` | Queue capacity, receive contention, duplicate and negative caching, overload retry, worker lifetime/shutdown, PTR versus forward A lookup, Darwin sockaddr family/length/byte order. Keep DNS outside UI/publication locks. |
| `src/cli.rs`, `src/main.rs` | GUI/CLI dispatch, numeric `-n` avoiding DNS, all-endpoint `-a`, filters, labels and CSV consistency. |
| `src/export.rs`, `src/kill.rs` | Exclusive owner-only file creation, symlink/collision refusal, formula mitigation, positive PID checks and explicit process-termination confirmation. Test validators without signaling real processes. |
| Build/docs | Deliberate compatibility pins, locked dependencies, macOS CI, universal bundles, ad-hoc signatures, accurate Markdown and tracked documentation assets. Keep PDF regeneration separate unless requested. |

Treat old reviews as hypotheses. Changes may already exist in an open PR or
merged code: for example, shared row Arcs, an MPMC DNS queue, a socket-address
abstraction or an Unknown direction. Verify the actual head rather than
reimplementing them. Describe inferred direction and off-box status according
to the data available; neither establishes a packet's route or intent.

## Implement a focused change

- Map affected files to the proposed review boundary. Keep independent changes
  on `codex/<topic>` branches from the default branch; reuse an authorized matching PR when
  appropriate. Stack only real dependencies with the parent as the child base,
  and describe merge order. Do not duplicate another PR's fix.
- Preserve live behavior unless fixing an evidenced defect or the user requests
  a behavior change. Check model additions through diff/highlight, filtering,
  sorting, rendering, CLI, exports and tests. Prove deleted rows are visible on
  exactly the documented refreshes; do not age them per repaint.
- Keep changes small. Retain required ownership clones; avoid full row copies on
  hot paths when immutable ownership suffices. Do not hold locks while rendering,
  resolving names or exporting. Preserve queue backpressure and account for
  shutdown latency when system DNS cannot be cancelled.
- Keep unsafe boundaries minimal and document pointer validity, storage lifetime,
  initialization, NUL bounds and OS-specific lengths. Use macOS constants or
  existing compatible abstractions. Never bypass platform guards to get a pass.
- Preserve numeric mode's privacy, CSV protections and termination confirmation.
  Do not add analytics, packet capture, elevated privileges, or disable SIP, TCC,
  Gatekeeper or other OS protections as an optimization.
- Justify dependencies with a concrete benefit and verify their MSRV and macOS
  support. Update the lockfile narrowly; no blanket `cargo update`, incidental
  compiler bump, blanket warning suppression or speculative dependency churn.
  Inspect `.cargo/audit.toml` and target dependency trees for audit findings;
  do not add advisory ignores merely to make CI green.

## Verify on the supported platform

Add focused regressions for changed behavior and deterministic seams for DNS or
queue tests. Use synthetic endpoints and loopback sockets; do not rely on public
DNS or expose real socket captures. For a performance claim, record comparable
before/after workload and measurements; otherwise label it a structural
improvement without inventing timings.

On macOS, run the repository's `./scripts/check.sh` and ensure the requested
scope includes these commands with the declared toolchain:

```sh
cargo fmt --all
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
git diff --check
```

Inspect the script before assuming it includes all-feature Clippy. For packaging
changes also follow `CONTRIBUTING.md` and `scripts/make-app.sh`: verify arm64 and
x86_64 when universal output is required, signature, archive permissions and a
numeric CLI smoke test. Do not claim notarization from an ad-hoc signature.

On Windows/Linux, run available source/format/syntax checks and report the exact
platform/toolchain limitation. Do not install an unrelated platform stack or
remove Darwin-only code to manufacture a test pass. Use current-head macOS CI
when publication is authorized; otherwise provide a concrete local patch with
validation limits. A different revision's CI result is not evidence for this head.
For skill/docs-only work, validate metadata, referenced paths and diff hygiene;
compiling the unchanged application locally is not required.

## Publish and report

Inspect the full staged diff and keep commits logically separable. Exclude real
endpoint exports, credentials, signing keys, caches, sessions, generated bundles
and machine-specific configuration. Keep documentation images tracked. Before
publishing, refresh the base, assess divergence, and validate any resulting
changes. Do not rewrite a published branch without authorization; when a rewrite
is authorized, use an exact expected-SHA lease.

When a push and draft PR are requested or already authorized, complete them on
the feature branch using `.github/PULL_REQUEST_TEMPLATE.md`. Do not ask again
merely because this skill mentions authorization. Honor actual tool approval
boundaries; if an action is rejected, report the action and reason instead of
bypassing it. Inspect remote state before retrying an uncertain push or PR create.
Watch CI for the current head and distinguish introduced failures, inherited
failures and unavailable platform evidence.

Finish with the selected finding and fix, changed files, evidence of benefit,
exact test/lint results, branch/commit and draft PR link when published, and any
intentional deferral with a technical reason. Return no-change findings when
nothing survives validation instead of manufacturing code churn.

## Optional local slash launcher

Use `assets/optimize-netviewer.md` as the thin custom-prompt launcher when the
user requests slash-command installation. Install it as
`<Codex home>/prompts/optimize-netviewer.md`; keep this skill as the workflow's
source. Codex's legacy custom-prompt syntax is `/prompts:optimize-netviewer`;
the skill itself can be invoked as `$optimize-netviewer` or selected via `/skills`.
Do not claim a bare `/optimize-netviewer` alias is registered without UI evidence.
