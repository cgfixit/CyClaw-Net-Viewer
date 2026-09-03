#!/bin/sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ROOT"

cargo build --release
BIN="$ROOT/target/release/netboard"
test -x "$BIN"

DIST="$ROOT/dist"
APP="$DIST/NetBoard.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/netboard"
cp "$ROOT/Info.plist" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
echo "built $APP"

if [ "${1:-}" = "--dmg" ]; then
  DMG="$DIST/NetBoard.dmg"
  hdiutil create -volname NetBoard -srcfolder "$APP" -ov -format UDZO "$DMG"
  echo "built $DMG"
fi
