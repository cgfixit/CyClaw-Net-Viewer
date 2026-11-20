#!/bin/bash
# Print the newest main commit that can be auto-released.
# Stdout is one full SHA. Progress goes to stderr.
#
# A commit qualifies when it is a pull request merged into main, it is not
# authored by github-actions[bot], this repo's auto-release scripts are in
# the tree, and both the Bundle and CI push runs succeeded. The newest
# merged commit whose bundle succeeded may be polled until CI finishes.
# Older commits are not polled.
set -euo pipefail

repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
max_commits="${MAX_COMMITS:-40}"
ci_wait="${CI_WAIT_SECONDS:-900}"

if ! printf '%s' "$max_commits" | grep -Eq '^[1-9][0-9]*$'; then
  echo "::error::MAX_COMMITS must be a positive integer" >&2
  exit 1
fi
if ! printf '%s' "$ci_wait" | grep -Eq '^[0-9]+$'; then
  echo "::error::CI_WAIT_SECONDS must be a non-negative integer" >&2
  exit 1
fi

latest_run() {
  local workflow="$1" sha="$2"
  gh api "repos/${repo}/actions/workflows/${workflow}/runs?head_sha=${sha}&event=push&branch=main&per_page=1" \
    --jq 'if (.workflow_runs | length) == 0 then "\t" else [.workflow_runs[0].status // "", .workflow_runs[0].conclusion // ""] | @tsv end'
}

has_script() {
  local sha="$1" err
  if err="$(gh api "repos/${repo}/contents/scripts/apply-release-version.sh?ref=${sha}" --jq .path 2>&1 >/dev/null)"; then
    return 0
  fi
  # Redirect order above keeps stderr. A missing file is a skip; anything else is fatal.
  if printf '%s\n' "$err" | grep -Eq 'HTTP 404|Not Found'; then
    return 1
  fi
  printf '%s\n' "$err" >&2
  return 2
}

is_release_commit() {
  local sha="$1" login count
  if ! login="$(gh api "repos/${repo}/commits/${sha}" --jq '.author.login // ""')"; then
    echo "::error::could not read commit ${sha}" >&2
    return 2
  fi
  if [ "$login" = "github-actions[bot]" ]; then
    echo "skip ${sha}: authored by github-actions[bot]" >&2
    return 1
  fi
  if ! count="$(gh api -H "Accept: application/vnd.github+json" \
    "repos/${repo}/commits/${sha}/pulls" \
    --jq '[.[] | select(.merged_at != null and .base.ref == "main")] | length')"; then
    echo "::error::could not list pull requests for ${sha}" >&2
    return 2
  fi
  if [ "$count" = "0" ]; then
    echo "skip ${sha}: not a pull request merged into main" >&2
    return 1
  fi
  return 0
}

wait_for_ci() {
  local sha="$1" deadline line status conclusion
  deadline="$((SECONDS + ci_wait))"
  while true; do
    line="$(latest_run ci.yml "$sha")"
    status="${line%%$'\t'*}"
    conclusion="${line#*$'\t'}"
    if [ "$conclusion" = "success" ]; then
      return 0
    fi
    case "$conclusion" in
      failure|cancelled|timed_out|startup_failure)
        echo "ci ${sha} concluded ${conclusion}" >&2
        return 1
        ;;
    esac
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "ci ${sha} not successful after ${ci_wait}s (status ${status:-missing})" >&2
      return 1
    fi
    echo "waiting for ci on ${sha} (status ${status:-missing})" >&2
    sleep 60
  done
}

commits="$(gh api "repos/${repo}/commits?sha=main&per_page=${max_commits}" --jq '.[].sha')"
if [ -z "$commits" ]; then
  echo "::error::main has no commits" >&2
  exit 1
fi

chosen=""
polled=0
while IFS= read -r sha; do
  [ -n "$sha" ] || continue
  if ! printf '%s' "$sha" | grep -Eq '^[0-9a-f]{40}$'; then
    echo "::error::unexpected commit id '${sha}'" >&2
    exit 1
  fi
  rc=0
  has_script "$sha" || rc=$?
  if [ "$rc" -eq 2 ]; then
    exit 1
  fi
  if [ "$rc" -ne 0 ]; then
    echo "skip ${sha}: auto-release scripts are not on this commit" >&2
    continue
  fi
  rc=0
  is_release_commit "$sha" || rc=$?
  if [ "$rc" -eq 2 ]; then
    exit 1
  fi
  if [ "$rc" -ne 0 ]; then
    continue
  fi
  line="$(latest_run bundle.yml "$sha")"
  conclusion="${line#*$'\t'}"
  if [ "$conclusion" != "success" ]; then
    echo "skip ${sha}: bundle ${conclusion:-not successful}" >&2
    continue
  fi
  line="$(latest_run ci.yml "$sha")"
  status="${line%%$'\t'*}"
  conclusion="${line#*$'\t'}"
  if [ "$conclusion" = "success" ]; then
    chosen="$sha"
    break
  fi
  case "$conclusion" in
    failure|cancelled|timed_out|startup_failure)
      echo "skip ${sha}: ci ${conclusion}" >&2
      continue
      ;;
  esac
  if [ "$polled" -eq 1 ]; then
    echo "skip ${sha}: ci still ${status:-missing}" >&2
    continue
  fi
  polled=1
  if wait_for_ci "$sha"; then
    chosen="$sha"
    break
  fi
done <<EOF
${commits}
EOF

if [ -z "$chosen" ]; then
  echo "::error::no eligible main commit in the last ${max_commits} (merged pull request, auto-release scripts present, Bundle and CI succeeded)" >&2
  exit 1
fi

printf '%s\n' "$chosen"
