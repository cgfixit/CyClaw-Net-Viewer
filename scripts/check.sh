#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
if [ "$(uname -s)" != Darwin ]; then
  echo "The application checks require macOS (Darwin FFI and a live socket test)." >&2
  exit 1
fi

cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
