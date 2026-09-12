---
name: netviewer-invariants
description: Fail-closed contract checks for CyClaw-Net-Viewer (crate netboard). Use before editing src, Cargo.toml, lockfile, scripts, or workflows, before sandbox or refactor sign-off, or when the user runs /netviewer-invariants.
---

# NetViewer invariants

Locate the `netboard` git root (`Cargo.toml` package `netboard`, origin
`cgfixit/CyClaw-Net-Viewer`). Read `AGENTS.md` and `SECURITY.md`. Run
commands from that checkout, including when this skill is installed
elsewhere. This is not CyClaw; do not load CyClaw invariant-guard.

From the repo root:

```
python .agents/skills/netviewer-invariants/scripts/check_invariants.py
```

Use `python3` when that is the interpreter. A non-zero exit is a broken
step: fix or revert; do not weaken an assertion to go green. The script
reads the live tree (no frozen SHAs). Re-read `SECURITY.md` for behavior
the grep cannot see: confirmed SIGTERM of the owning PID, Off-box UDP
remotes as `*:0`, numeric CLI `-n` skipping DNS, no analytics.

Do not treat a Windows/Linux pass of this script as a Darwin runtime pass.
Windows and Linux still cannot compile the crate; never remove
`compile_error!` to fake one.
