#!/bin/bash
# Print the next automatic release version.
#
# The version already in Cargo.toml counts as used, as do git tags and
# GitHub releases (drafts included). The next release is one patch past
# the highest of those. CFBundleVersion is one past the highest integer
# seen on the tree or recorded for an earlier release.
#
#   ./scripts/next-release-version.sh --cargo-version 0.2.0 --bundle-version 4
#   ./scripts/next-release-version.sh --read-tree
#   ./scripts/next-release-version.sh --github
#   ./scripts/next-release-version.sh --self-test
#
# Stdout is exactly:
#   version=X.Y.Z
#   bundle_version=N
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TREE="$ROOT"
cargo_version=""
bundle_version=""
read_tree=0
use_github=0
occupied=""
seen_bundles=""

usage() {
  echo "Usage: $0 [--cargo-version X.Y.Z] [--bundle-version N] [--occupied VER]... [--seen-bundle N]... [--read-tree] [--root DIR] [--github] [--self-test]" >&2
}

normalize_version() {
  local value="$1"
  case "$value" in
    refs/tags/*) value="${value#refs/tags/}" ;;
  esac
  case "$value" in
    v*) value="${value#v}" ;;
  esac
  printf '%s' "$value"
}

add_occupied() {
  local value
  value="$(normalize_version "$1")"
  if printf '%s' "$value" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    occupied="${occupied}${value}"$'\n'
  elif [ -n "$value" ]; then
    echo "note: ignoring non-release version '${1}'" >&2
  fi
}

add_seen_bundle() {
  local value="$1"
  if printf '%s' "$value" | grep -Eq '^[1-9][0-9]*$'; then
    seen_bundles="${seen_bundles}${value}"$'\n'
  else
    echo "::error::CFBundleVersion '${value}' is not a positive integer" >&2
    exit 1
  fi
}

read_tree_versions() {
  local parsed
  parsed="$(python3 - "$TREE" <<'PY'
import pathlib, re, sys
root = pathlib.Path(sys.argv[1])
cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
match = re.search(r"(?ms)^\[package\]\n(.*?)(?=^\[|\Z)", cargo)
if not match:
    raise SystemExit("Cargo.toml has no [package] table")
version = re.search(r'(?m)^version = "([^"]+)"', match.group(1))
if not version:
    raise SystemExit("Cargo.toml [package] has no version")
plist = (root / "Info.plist").read_text(encoding="utf-8")
bundle = re.search(r"<key>CFBundleVersion</key>\s*<string>([^<]*)</string>", plist)
if not bundle:
    raise SystemExit("Info.plist has no CFBundleVersion")
print(version.group(1))
print(bundle.group(1))
PY
)"
  cargo_version="$(printf '%s\n' "$parsed" | awk 'NR==1{print}')"
  bundle_version="$(printf '%s\n' "$parsed" | awk 'NR==2{print}')"
}

bundle_at_git_ref() {
  local ref="$1" encoded version
  encoded="$(gh api "repos/${GITHUB_REPOSITORY}/contents/Info.plist?ref=${ref}" --jq .content)"
  version="$(printf '%s' "$encoded" | python3 -c 'import base64, re, sys
text = base64.b64decode(sys.stdin.read()).decode("utf-8")
match = re.search(r"<key>CFBundleVersion</key>\s*<string>([^<]*)</string>", text)
if not match:
    raise SystemExit("missing CFBundleVersion")
print(match.group(1))')"
  add_seen_bundle "$version"
}

collect_from_github() {
  local repo tag sha body_versions ref seen_refs=""
  repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
  read_tree_versions
  add_seen_bundle "$bundle_version"

  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    add_occupied "$ref"
    case "$seen_refs" in
      *"|${ref}|"*) continue ;;
    esac
    seen_refs="${seen_refs}|${ref}|"
    bundle_at_git_ref "${ref#refs/tags/}"
  done < <(gh api --paginate "repos/${repo}/git/matching-refs/tags/v" --jq '.[].ref')

  while IFS=$'\t' read -r tag sha; do
    [ -n "$tag" ] || continue
    add_occupied "$tag"
    if ! printf '%s' "$sha" | grep -Eq '^[0-9a-f]{40}$'; then
      echo "note: release ${tag} target '${sha}' is not a commit SHA; using its notes only" >&2
      continue
    fi
    case "$seen_refs" in
      *"|${sha}|"*) continue ;;
    esac
    seen_refs="${seen_refs}|${sha}|"
    bundle_at_git_ref "$sha"
  done < <(gh api --paginate "repos/${repo}/releases" --jq '.[] | [.tag_name, .target_commitish] | @tsv')

  body_versions="$(gh api --paginate "repos/${repo}/releases" --jq '.[] | .body // ""' \
    | sed -n 's/^netviewer-cfbundle-version: \([1-9][0-9]*\)$/\1/p')"
  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    add_seen_bundle "$ref"
  done <<EOF
${body_versions}
EOF
}

compute() {
  if ! printf '%s' "$cargo_version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "::error::automatic releases need a Cargo.toml version like 0.2.0, got '${cargo_version}'" >&2
    exit 1
  fi
  add_seen_bundle "$bundle_version"
  awk -v cargo="$cargo_version" -v bundle="$bundle_version" -v occupied="$occupied" -v seen="$seen_bundles" '
    function compare(left, right,    i, a, b) {
      split(left, a, ".")
      split(right, b, ".")
      for (i = 1; i <= 3; i++) {
        if ((a[i] + 0) > (b[i] + 0)) return 1
        if ((a[i] + 0) < (b[i] + 0)) return -1
      }
      return 0
    }
    function bump(version,    a) {
      split(version, a, ".")
      return a[1] "." a[2] "." (a[3] + 1)
    }
    BEGIN {
      highest = cargo
      used[cargo] = 1
      count = split(occupied, items, "\n")
      for (i = 1; i <= count; i++) {
        if (items[i] == "") continue
        used[items[i]] = 1
        if (compare(items[i], highest) > 0) highest = items[i]
      }
      next_version = bump(highest)
      for (tries = 0; tries < 50 && (next_version in used); tries++) {
        next_version = bump(next_version)
      }
      if (next_version in used) {
        print "no free patch within 50 of " highest > "/dev/stderr"
        exit 1
      }
      highest_bundle = bundle + 0
      count = split(seen, bundles, "\n")
      for (i = 1; i <= count; i++) {
        if (bundles[i] == "") continue
        if ((bundles[i] + 0) > highest_bundle) highest_bundle = bundles[i] + 0
      }
      print "version=" next_version
      print "bundle_version=" (highest_bundle + 1)
    }
  '
}

self_test() {
  local failed=0
  expect() {
    local name="$1" expected_version="$2" expected_bundle="$3" out version bundle
    shift 3
    if ! out="$("$0" "$@")"; then
      echo "FAIL ${name}: command failed" >&2
      failed=1
      return
    fi
    version="$(printf '%s\n' "$out" | awk -F= '$1=="version"{print $2}')"
    bundle="$(printf '%s\n' "$out" | awk -F= '$1=="bundle_version"{print $2}')"
    if [ "$version" = "$expected_version" ] && [ "$bundle" = "$expected_bundle" ]; then
      echo "PASS ${name}: ${version} CFBundleVersion ${bundle}"
    else
      echo "FAIL ${name}: got ${version} / ${bundle}, expected ${expected_version} / ${expected_bundle}" >&2
      failed=1
    fi
  }
  expect_fail() {
    local name="$1"
    shift
    if "$0" "$@" >/dev/null 2>&1; then
      echo "FAIL ${name}: expected a non-zero exit" >&2
      failed=1
    else
      echo "PASS ${name}: rejected"
    fi
  }

  expect "no tags" 0.2.1 5 \
    --cargo-version 0.2.0 --bundle-version 4
  expect "v0.2.0 draft only" 0.2.1 5 \
    --cargo-version 0.2.0 --bundle-version 4 --occupied v0.2.0 --seen-bundle 4
  expect "v0.2.0 draft and stale v0.1.0 draft" 0.2.1 5 \
    --cargo-version 0.2.0 --bundle-version 4 \
    --occupied v0.2.0 --occupied v0.1.0 --seen-bundle 4 --seen-bundle 1
  expect "v0.2.0 published" 0.2.1 5 \
    --cargo-version 0.2.0 --bundle-version 4 \
    --occupied refs/tags/v0.2.0 --seen-bundle 4
  expect "v0.2.1 already exists" 0.2.2 6 \
    --cargo-version 0.2.0 --bundle-version 4 \
    --occupied v0.2.0 --occupied v0.2.1 --seen-bundle 4 --seen-bundle 5
  expect "numeric patch order" 0.2.11 5 \
    --cargo-version 0.2.0 --bundle-version 4 \
    --occupied v0.2.9 --occupied 0.2.10
  expect "recorded bundle wins over the tree" 0.2.1 10 \
    --cargo-version 0.2.0 --bundle-version 4 --seen-bundle 9
  expect "prerelease tag does not occupy the stable patch" 0.2.1 5 \
    --cargo-version 0.2.0 --bundle-version 4 --occupied v0.2.1-rc.1
  expect_fail "prerelease Cargo.toml version" \
    --cargo-version 0.2.0-rc.1 --bundle-version 4
  expect_fail "CFBundleVersion zero" \
    --cargo-version 0.2.0 --bundle-version 0

  local fixture parsed version bundle
  fixture="$(mktemp -d)"
  cat > "${fixture}/Cargo.toml" <<'EOF'
[workspace]

[package]
name = "netboard"
version = "1.4.5"
edition = "2021"

[dependencies]
demo = { version = "9.9.9" }
EOF
  cat > "${fixture}/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
	<key>CFBundleShortVersionString</key>
	<string>1.4.5</string>
	<key>CFBundleGetInfoString</key>
	<string>0.2.0</string>
	<key>CFBundleVersion</key>
	<string>8</string>
</dict></plist>
EOF
  expect "reads the package version, not a dependency" 1.4.6 9 \
    --read-tree --root "$fixture"
  rm -rf "$fixture"

  if [ "$failed" -ne 0 ]; then
    exit 1
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --cargo-version)
      cargo_version="$2"
      shift 2
      ;;
    --bundle-version)
      bundle_version="$2"
      shift 2
      ;;
    --occupied)
      add_occupied "$2"
      shift 2
      ;;
    --seen-bundle)
      add_seen_bundle "$2"
      shift 2
      ;;
    --read-tree)
      read_tree=1
      shift
      ;;
    --root)
      TREE="$2"
      shift 2
      ;;
    --github)
      use_github=1
      shift
      ;;
    --self-test)
      self_test
      exit 0
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [ "$use_github" -eq 1 ]; then
  collect_from_github
elif [ "$read_tree" -eq 1 ]; then
  read_tree_versions
fi

if [ -z "$cargo_version" ] || [ -z "$bundle_version" ]; then
  echo "::error::cargo version and CFBundleVersion are required" >&2
  usage
  exit 2
fi

compute
