# Contributing

## Development setup

Use macOS 12+ with Xcode Command Line Tools (`xcode-select --install`) and
rustup. Clone the repository and run:

```sh
cd Mac-NetViewer-EZview
rustup toolchain install 1.85.0 --component rustfmt --component clippy
./scripts/check.sh
cargo run --locked -- --cli -n -a
cargo run --locked
```

`scripts/check.sh` runs the same Rust checks as CI: formatting, Clippy with
warnings denied, and all tests, including a real loopback socket snapshot.
The GUI and Darwin FFI cannot be built or tested natively on Windows/Linux.
Those hosts can review source and run `cargo fmt --all -- --check`; use
macOS CI for compilation and runtime validation.

## CI tools and test coverage

CI runs Rust 1.85.0 formatting, Clippy, and tests natively on Apple Silicon
(`macos-14`) and Intel (`macos-15-intel`). It also validates `Info.plist`.
The separate Bundle workflow verifies the universal binary and signature.

Additional checks use open-source tools with no paid license key or hosted
reporting account:

- [Gitleaks CLI](https://github.com/gitleaks/gitleaks) scans full reachable
  Git history with redacted output on PRs, default-branch (`master` or
  `main`) pushes, and weekly runs.
- [Actionlint](https://github.com/rhysd/actionlint) validates all workflows
  and their embedded shell; [ShellCheck](https://github.com/koalaman/shellcheck)
  also checks `scripts/*.sh`. Linux tool downloads have pinned SHA-256 hashes.
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) runs macOS tests
  and uploads HTML and LCOV coverage as a 14-day Actions artifact. There is
  no percentage gate yet; use the report to identify meaningful coverage gaps.
  Ordinary GitHub Actions runner/storage allowances still apply.

Reproduce coverage on macOS:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked --version 0.6.16
cargo llvm-cov --locked --all-features --lcov --output-path lcov.info
cargo llvm-cov report --html
```

Unit tests live beside private helpers in `src/`. Integration tests in
`tests/` cover diff identity/lifecycle, private CSV exports, CLI subprocess
exit codes/output, and live loopback sockets. Run an individual suite with
`cargo test --locked --test cli` or `cargo test --locked --test export`.
Keep fixtures synthetic, avoid public DNS, and test PID validation without
sending signals. CLI snapshot tests filter an impossible PID to avoid
printing host process data. Update tool versions and hashes together;
Dependabot updates action references, but not versions inside shell steps.

## Packaging validation

```sh
./scripts/make-app.sh
codesign --verify --deep --strict dist/CyClaw-Net-Viewer.app
lipo -info dist/CyClaw-Net-Viewer.app/Contents/MacOS/netboard
dist/CyClaw-Net-Viewer.app/Contents/MacOS/netboard --cli -n -a
./scripts/make-app.sh --dmg
```

The script requires macOS and builds from `Cargo.lock`. It attempts both
architectures and reports when only the host architecture is available.
CI requires both architectures, checks the signature, and archives the app
with `ditto` to preserve executable permissions and bundle metadata. Bundles
are ad-hoc signed, not Developer ID signed or notarized.

Do not commit `target/` or generated `dist/` artifacts. The Bundle workflow
provides a ZIP and SHA-256 checksum tied to the workflow's source revision.
Checksums detect corruption; they do not establish a trusted publisher.

## Changes and review

The GitHub default branch is still `master`. A rename to `main` is deferred
until a repository admin flips it in Settings; do not delete `master` or
rewrite shared history. Open draft PRs against the current default (`master`
today, `main` after that flip). Workflows listen to both names so CI keeps
running across the rename.

Administrator rename (Christopher), in this order:

1. Open [Branches](https://github.com/cgfixit/Mac-NetViewer-EZview/branches).
2. Next to `master`, click the pencil and rename the branch to `main`.
3. Confirm GitHub's rename dialog. History stays intact, open PRs retarget,
   and old `master` URLs redirect. This is reversible by renaming `main`
   back to `master`.
4. Confirm [Settings → General](https://github.com/cgfixit/Mac-NetViewer-EZview/settings)
   shows Default branch `main`.
5. Do not delete `master` if a leftover pointer remains, and do not
   force-push shared history. After clones and bookmarks are confirmed, a
   follow-up can drop `master` from workflow `branches:` filters.

GitHub topics (`macos`, `rust`, `networking`, `egui`, `sysadmin`, `cyclaw`)
and the About description are already set on the remote. Do not clear or
replace them as incidental cleanup.

Use focused feature branches and draft PRs against the default branch. Fill
in the PR template, include regression coverage for changed behavior, and
record what was and was not tested. Do not merge your own automated changes.

Keep Rust 1.85.0 compatibility pins and `Cargo.lock` together. Dependency
updates need macOS build/test validation and the Audit check. Weekly
Dependabot proposals are reviewed rather than merged automatically.

Run `git diff --check` before committing. GitHub CI also checks spelling.
Do not attach real socket exports, process paths, or credentials to public
issues or tests. Use loopback addresses and synthetic names in fixtures.

## Documentation

Markdown in `docs/` describes current behavior. The checked-in PDFs are
snapshot guides; optional `tools/write_*.py` scripts use ReportLab to
regenerate them. Python is not needed to build or run the Rust app. Review
the generators' output paths before running them. Keep PDF regeneration
separate from unrelated code fixes.

For agent-assisted work, start with [AGENTS.md](AGENTS.md) and
[.codex/README.md](.codex/README.md).
