#!/usr/bin/env bash
# scripts/tests/test_release_version_surfaces.sh — release-cut manifest contract.
#
# `set-version.sh --check-files` owns Axis W's Cargo, Python project, and
# package.json surfaces.  The npm lockfile root metadata and Python runtime
# __version__ are intentionally maintained separately, so assert that they
# cannot silently lag a release cut.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

workspace_version="$(awk '
  /^\[workspace\.package\]/ { in_block = 1; next }
  /^\[/ { in_block = 0 }
  in_block && /^version[[:space:]]*=/ {
    n = split($0, fields, "\"")
    if (n >= 3) { print fields[2] }
    exit
  }
' "$REPO_ROOT/Cargo.toml")"

if [ -z "$workspace_version" ]; then
  printf 'FAIL  cannot read workspace package version\n' >&2
  exit 1
fi

lockfile="$REPO_ROOT/src/ts/package-lock.json"
lock_top="$(jq -r '.version // empty' "$lockfile")"
lock_root="$(jq -r '.packages[""].version // empty' "$lockfile")"
runtime_version="$(sed -n 's/^__version__[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' \
  "$REPO_ROOT/src/python/fathomdb/__init__.py")"

failed=0
for label in 'package-lock top-level' 'package-lock root package' 'python runtime'; do
  case "$label" in
    'package-lock top-level') observed="$lock_top" ;;
    'package-lock root package') observed="$lock_root" ;;
    'python runtime') observed="$runtime_version" ;;
  esac
  if [ "$observed" = "$workspace_version" ]; then
    printf 'PASS  %s version matches workspace (%s)\n' "$label" "$workspace_version"
  else
    printf 'FAIL  %s version %s does not match workspace %s\n' \
      "$label" "${observed:-<missing>}" "$workspace_version" >&2
    failed=$((failed + 1))
  fi
done

[ "$failed" -eq 0 ] || exit 1
printf 'All release version surfaces match workspace %s\n' "$workspace_version"
