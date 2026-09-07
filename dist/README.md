# Generated application bundles

Public downloads use the latest
[GitHub Release](https://github.com/cgfixit/CyClaw-Net-Viewer/releases/latest)
(`CyClaw-Net-Viewer.zip` plus `CyClaw-Net-Viewer.zip.sha256`). Build
locally with `./scripts/make-app.sh` when you want a private copy.

Maintainers may also download a 14-day
[Bundle workflow](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/bundle.yml)
artifact for a specific commit. Prefer the reviewed run for the exact commit
you need. Pull-request artifacts contain proposed changes. Actions zips expire
and require GitHub sign-in; they are not the public distribution
channel.

Release and Bundle archives contain `CyClaw-Net-Viewer.zip` and its
SHA-256 checksum. Verify the checksum, then open the ZIP on macOS.
The archive preserves the app's executable bit and signature
resources. Generated bundles are ignored by Git.

The old checked-in app has been removed to avoid shipping an executable
that silently falls behind source fixes. Git history still contains it;
this change does not rewrite history. See [build instructions](../docs/BUILD.md).
