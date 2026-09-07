---
name: verify-netviewer-bundle
description: Validate a CyClaw-Net-Viewer macOS app bundle and its archived round trip, including architecture, signature, checksum, and CLI launch. Use for packaging changes or release candidate verification.
---

# Verify the NetViewer bundle

Locate the `netboard` repository; inspect AGENTS.md, scripts/make-app.sh,
Info.plist, .github/workflows/bundle.yml, and docs/BUILD.md. Work on the
requested revision and record it. Use macOS; Windows inspection cannot
validate Darwin binaries or codesign. Build `./scripts/make-app.sh` when
validating source. Inspect an existing supplied bundle without replacing it.

For a universal release candidate require both arm64 and x86_64 with
`lipo <app>/Contents/MacOS/netboard -verify_arch arm64 x86_64`, plus
`codesign --verify --deep --strict <app>` and `plutil -lint <app>/Contents/Info.plist`.
Verify the executable's `--cli -n -a 4294967295` exits successfully; the
impossible PID avoids publishing real socket data. A host-only local build
can be useful but must not be labeled universal.

Follow the Bundle workflow's ditto ZIP creation/extraction sequence in a
fresh temporary directory. Verify the SHA-256 checksum before extraction,
then repeat executable, architecture, signature, and numeric CLI checks on
the extracted bundle. Preserve executable permissions and bundle metadata.
Inspect the exact revision's Bundle CI artifact when macOS is unavailable.

Report each result and whether an interactive GUI launch was actually tested.
Ad-hoc signing and a checksum do not establish publisher identity or
notarization. Keep generated bundles and private captures out of Git.
Building/verifying does not authorize publishing a release or changing OS
protections; follow the user's existing publication scope.
