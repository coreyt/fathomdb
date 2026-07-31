#!/usr/bin/env bash
# Fails when the release dispatch confirmation can be auto-populated.
set -euo pipefail

if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
  printf 'usage: %s <release-workflow.yml>\n' "$0" >&2
  exit 2
fi

confirmation_block="$(awk '
  $0 == "      confirm_release_version:" { found = 1; in_block = 1; next }
  in_block && /^      [[:alnum:]_]+:$/ { exit }
  in_block { print }
  END { exit !found }
' "$1")" || {
  printf 'FAIL  release workflow is missing the confirm_release_version input\n' >&2
  exit 1
}

if grep -qE '^[[:space:]]+default:' <<<"$confirmation_block"; then
  printf 'FAIL  release workflow confirmation input must not have a default\n' >&2
  exit 1
fi
