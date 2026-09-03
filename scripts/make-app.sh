#!/bin/sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=12.0
cd "$ROOT"

HOST="$(rustc -vV | awk '/^host:/{print $2}')"
ARM=aarch64-apple-darwin
INTEL=x86_64-apple-darwin

cargo build --release --target "$HOST"

ARM_BIN=""
INTEL_BIN=""
case "$HOST" in
  aarch64-apple-darwin) ARM_BIN="$ROOT/target/$ARM/release/netboard" ;;
  x86_64-apple-darwin) INTEL_BIN="$ROOT/target/$INTEL/release/netboard" ;;
esac

if [ "$HOST" != "$INTEL" ]; then
  if rustup target add "$INTEL" >/dev/null 2>&1 \
    && cargo build --release --target "$INTEL"; then
    INTEL_BIN="$ROOT/target/$INTEL/release/netboard"
  else
    echo "x86_64 build skipped (no target/SDK); shipping host arch only" >&2
  fi
fi
if [ "$HOST" != "$ARM" ]; then
  if rustup target add "$ARM" >/dev/null 2>&1 \
    && cargo build --release --target "$ARM"; then
    ARM_BIN="$ROOT/target/$ARM/release/netboard"
  else
    echo "arm64 build skipped (no target/SDK); shipping host arch only" >&2
  fi
fi

DIST="$ROOT/dist"
APP="$DIST/CyClaw-Net-Viewer.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
OUT="$APP/Contents/MacOS/netboard"

if [ -n "$ARM_BIN" ] && [ -n "$INTEL_BIN" ] && [ -x "$ARM_BIN" ] && [ -x "$INTEL_BIN" ]; then
  lipo -create -output "$OUT" "$ARM_BIN" "$INTEL_BIN"
elif [ -n "$ARM_BIN" ] && [ -x "$ARM_BIN" ]; then
  cp "$ARM_BIN" "$OUT"
elif [ -n "$INTEL_BIN" ] && [ -x "$INTEL_BIN" ]; then
  cp "$INTEL_BIN" "$OUT"
else
  cp "$ROOT/target/release/netboard" "$OUT"
fi
chmod +x "$OUT"
cp "$ROOT/Info.plist" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
echo "built $APP"
lipo -info "$OUT" || true

if [ "${1:-}" = "--dmg" ]; then
  DMG="$DIST/CyClaw-Net-Viewer.dmg"
  hdiutil create -volname CyClaw-Net-Viewer -srcfolder "$APP" -ov -format UDZO "$DMG"
  echo "built $DMG"
fi
