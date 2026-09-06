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

## Packaging

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

Use focused feature branches and draft PRs against `master`. Fill in the
PR template, include regression coverage for changed behavior, and record
what was and was not tested. Do not merge your own automated changes.

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
