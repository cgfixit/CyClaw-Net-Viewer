# Claude Code

This repository is **CyClaw-Net-Viewer** (crate `netboard`), a macOS TCP/UDP
endpoint viewer. It is not the CyClaw AI backend. Do not import CyClaw
policies, pytest, or `verify_ci_emulation.py`.

Follow [AGENTS.md](AGENTS.md). Project Agent Skills live in
`.agents/skills/` (Codex, Cursor, Grok, and the Agent Skills spec).
Claude Code also has thin stubs under `.claude/skills/` that only redirect
to those canonical `SKILL.md` files. Load the matching skill before coding.

| Skill | Load when |
| --- | --- |
| `emulate-egress-sandbox` | Bash TCP fixture / Off-box observation |
| `verify-netviewer-rust` | Compiler pin, lockfile, Rust CI |
| `verify-netviewer-bundle` | App bundle, lipo, codesign, ZIP round-trip |
| `netviewer-sandbox` | Verify / smoke / sandbox this checkout |
| `refactor-netviewer` | Architecture or module-structure cleanup |
| `netviewer-invariants` | Before `src/` / Cargo / scripts / workflow edits |

Windows and Linux cannot compile this Darwin crate. Never remove
`compile_error!` to fake a pass.
