# Generated application bundles

Build locally with `./scripts/make-app.sh`, or download the artifact from a
successful [Bundle workflow](https://github.com/cgfixit/Mac-NetViewer-EZview/actions/workflows/bundle.yml)
run for the source revision you intend to use. Prefer reviewed `master`
builds; pull-request artifacts contain proposed changes.

The artifact contains `CyClaw-Net-Viewer.zip` and its SHA-256 checksum.
Extract the outer Actions download, verify the checksum, then open the
inner ZIP on macOS. The inner archive preserves the app's executable bit
and signature resources. Generated bundles are ignored by Git.

The old checked-in app has been removed to avoid shipping an executable
that silently falls behind source fixes. Git history still contains it;
this change does not rewrite history. See [build instructions](../docs/BUILD.md).
