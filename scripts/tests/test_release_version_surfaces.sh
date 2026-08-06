#!/usr/bin/env bash
# scripts/tests/test_release_version_surfaces.sh — release-cut manifest contract.
#
# `set-version.sh --check-files` owns Axis W's Cargo, Python project/runtime,
# npm manifest/lockfile, and Cargo.lock internal-package surfaces. Assert the
# shipped files cannot silently lag a release cut.
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

lock_package_versions() {
  awk -v target="$1" '
    BEGIN { RS = ""; FS = "\n" }
    {
      name = ""
      version = ""
      for (i = 1; i <= NF; i++) {
        if ($i ~ /^name = "/) {
          name = $i
          sub(/^name = "/, "", name)
          sub(/"$/, "", name)
        }
        if ($i ~ /^version = "/) {
          version = $i
          sub(/^version = "/, "", version)
          sub(/"$/, "", version)
        }
      }
      if (name == target) { print version }
    }
  ' "$REPO_ROOT/Cargo.lock"
}

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

axis_w_packages=(
  fathomdb
  fathomdb-cli
  fathomdb-embedder
  fathomdb-engine
  fathomdb-napi
  fathomdb-py
  fathomdb-query
  fathomdb-schema
)

for package in "${axis_w_packages[@]}"; do
  mapfile -t versions < <(lock_package_versions "$package")
  if [ "${#versions[@]}" -eq 1 ] && [ "${versions[0]}" = "$workspace_version" ]; then
    printf 'PASS  Cargo.lock %s version matches workspace (%s)\n' "$package" "$workspace_version"
  else
    printf 'FAIL  Cargo.lock %s version(s) %s do not match workspace %s\n' \
      "$package" "${versions[*]:-<missing>}" "$workspace_version" >&2
    failed=$((failed + 1))
  fi
done

mapfile -t axis_e_versions < <(lock_package_versions fathomdb-embedder-api)
if [ "${#axis_e_versions[@]}" -eq 1 ] && [ "${axis_e_versions[0]}" = '0.6.1' ]; then
  printf 'PASS  Cargo.lock fathomdb-embedder-api remains Axis E 0.6.1\n'
else
  printf 'FAIL  Cargo.lock fathomdb-embedder-api version(s) %s must remain Axis E 0.6.1\n' \
    "${axis_e_versions[*]:-<missing>}" >&2
  failed=$((failed + 1))
fi

[ "$failed" -eq 0 ] || exit 1
printf 'All release version surfaces match workspace %s\n' "$workspace_version"
