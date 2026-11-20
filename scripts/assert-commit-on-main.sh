#!/bin/bash
# Exit 0 when SHA is main or an ancestor of main.
# A release build must not leave that history.
set -euo pipefail

sha="${1:?usage: assert-commit-on-main.sh SHA}"
repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

if ! printf '%s' "$sha" | grep -Eq '^[0-9a-f]{40}$'; then
  echo "::error::ref must be a 40-character lowercase SHA" >&2
  exit 1
fi

if ! line="$(gh api "repos/${repo}/compare/${sha}...main" --jq '[.status, (.behind_by|tostring)] | @tsv')"; then
  echo "::error::could not compare ${sha} with main" >&2
  exit 1
fi
status="${line%%$'\t'*}"
behind="${line#*$'\t'}"

if [ "$status" = "identical" ]; then
  exit 0
fi
if [ "$status" = "ahead" ] && [ "$behind" = "0" ]; then
  exit 0
fi

echo "::error::${sha} is not contained in main (status ${status}, behind ${behind})" >&2
exit 1
