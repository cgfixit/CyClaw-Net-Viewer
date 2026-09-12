---
name: refactor-netviewer
description: Iterative architecture refactor for CyClaw-Net-Viewer (crate netboard). Use when asked to refactor, clean up structure, split modules, or improve organization, or when the user runs /refactor-netviewer.
---

# Refactor NetViewer

Locate the `netboard` git root. Read `AGENTS.md` and the modules you would
touch. This is not CyClaw: do not call pytest, soul mutation, or
invariant-guard.

Loop until the requested structure is clean, one concern per step:

1. **Assess** the current split in `src/` (`lib`, `main`/`cli`, `snapshot`,
   `diff`, `dns`, `app`, `kill`, `export`). Prefer extracting a bounded
   helper over a rewrite.
2. **Execute** the single highest-leverage step. Keep behavior unless the
   task is a deliberate change.
3. **Live-test** with `$netviewer-invariants` then `$netviewer-sandbox`.
   A failed check means revert or rescope, not a loosened test.
4. **Commit** that step only. Tracker is untracked
   (`.git/refactor-netboard.md` or `%TEMP%\refactor-netboard.md`); never
   commit it.

Preserve unless the task says otherwise: `EndpointKey`, two-tick delete,
event-color over Off-box orange, SIGTERM of the owning PID (not TCB-delete),
CSV `create_new` + `0o600` + formula prefix, numeric `-n` skips DNS, rustc
1.85.0, eframe/egui_extras `=0.31.1`, image `=0.25.6`. No tokio, reqwest,
clap, or anyhow. `unsafe` stays `cfg(target_os = "macos")` with SAFETY
comments. Re-read live `Cargo.toml` rather than freezing pins here.

Windows/Linux: format and invariants only; say Darwin compile/Clippy/tests
are unverified. Never delete `compile_error!` to green a refactor step.
