#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
if [ "$(uname -s)" != Darwin ]; then
  echo "FAIL: egress observation requires macOS and the Darwin collector." >&2
  exit 1
fi

# RUSTUP_TOOLCHAIN may select stable; the repository pin remains the default.
cargo test --locked --test egress_sandbox -- --nocapture
