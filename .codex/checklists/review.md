# Pre-PR review

- Read the affected code, project guide, and current open PRs.
- Keep changes focused and add regressions for changed behavior.
- Check untrusted process/DNS data, FFI assumptions, CSV export, and PID
  handling when touching those boundaries.
- Run `./scripts/check.sh` on macOS; validate bundle changes with
  `./scripts/make-app.sh`, codesign, architecture inspection, and CLI smoke.
- Inspect `git diff --check`, `git diff --stat`, and the full staged diff.
- Confirm no private endpoint data, keys, generated bundles, or agent state
  are staged. Keep Cargo.lock changes intentional.
- Fill out the PR template, including exact validation and unresolved risks.
- Push the feature branch, open a draft against the default branch
  (`master` today, `main` after the Settings rename), and inspect CI.
