#!/usr/bin/env bash
# scripts/verify-release-gates.sh — release-time preflight.
#
# Asserts (in order, fail-fast):
#   1. GITHUB_REF_NAME is set and looks like `v<axis-W-version>`, and the
#      bare version matches Axis W (Cargo.toml [workspace.package].version).
#   2. set-version.sh --check-files passes (two-axis lockstep + inheritance).
#   3. HEAD commit is reachable from `main` (overridable via env for tests).
#   4. CHANGELOG.md has a section heading matching the tag's Axis-W version.
#   5. Every publishable workspace crate has `description`, `license`, and
#      `repository` fields populated (cargo publish requires these; failing
#      early beats failing at T4 after T1+T2+T3 succeed).
#
# Test seams (env overrides; do not use in CI):
#   RELEASE_GATES_SKIP_GIT_REACH=1  Skip check 3 entirely.
#   RELEASE_GATES_HEAD_REF=<ref>    Compare against this ref instead of main.
#   RELEASE_GATES_CHANGELOG=<path>  Use this CHANGELOG file instead of repo root.
#
# Owner: dev/design/release.md § Tiered publish order (entry gate to T1).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO="$REPO_ROOT/Cargo.toml"
CRATES_DIR="$REPO_ROOT/src/rust/crates"
SET_VERSION="$REPO_ROOT/scripts/set-version.sh"

die() {
  printf 'release-gate: %s\n' "$*" >&2
  exit 1
}

# Mirror of set-version.sh's reader; duplicated here to avoid sourcing a
# script that has top-level dispatch logic.
read_workspace_version() {
  awk '
    /^\[workspace\.package\]/ { in_block = 1; next }
    /^\[/                     { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$CARGO"
}

# --- Check 1: tag presence + Axis W match -----------------------------------

if [ -z "${GITHUB_REF_NAME:-}" ]; then
  die "GITHUB_REF_NAME is not set; this script must run under a tag push (refs/tags/v*)"
fi

TAG="$GITHUB_REF_NAME"
case "$TAG" in
  v*) ;;
  *) die "tag '$TAG' does not start with 'v'; release tags must look like v<axis-W-version>" ;;
esac

TAG_VERSION="${TAG#v}"
WS_VERSION="$(read_workspace_version)"
if [ -z "$WS_VERSION" ]; then
  die "Cargo.toml: no [workspace.package] version found"
fi
if [ "$TAG_VERSION" != "$WS_VERSION" ]; then
  die "tag/workspace version mismatch — tag is '$TAG_VERSION', Cargo.toml [workspace.package].version is '$WS_VERSION'"
fi

# --- Check 2: --check-files lockstep ----------------------------------------

if ! cf_out="$(bash "$SET_VERSION" --check-files 2>&1)"; then
  printf '%s\n' "$cf_out" >&2
  die "set-version.sh --check-files failed; resolve the version drift above before tagging"
fi

# --- Check 3: HEAD reachable from main --------------------------------------

if [ "${RELEASE_GATES_SKIP_GIT_REACH:-0}" != "1" ]; then
  main_ref="${RELEASE_GATES_HEAD_REF:-refs/heads/main}"
  if ! git -C "$REPO_ROOT" rev-parse --verify "$main_ref" >/dev/null 2>&1; then
    die "cannot resolve $main_ref; release tags must be cut from a commit on main"
  fi
  head_sha="$(git -C "$REPO_ROOT" rev-parse HEAD)"
  if ! git -C "$REPO_ROOT" merge-base --is-ancestor "$head_sha" "$main_ref" 2>/dev/null; then
    die "HEAD ($head_sha) is not reachable from $main_ref; release tags must be cut from main"
  fi
fi

# --- Check 4: CHANGELOG heading for this version ----------------------------

CHANGELOG="${RELEASE_GATES_CHANGELOG:-$REPO_ROOT/CHANGELOG.md}"
if [ ! -f "$CHANGELOG" ]; then
  die "CHANGELOG.md not found at $CHANGELOG; create one with a '## $WS_VERSION' section before tagging"
fi
if ! grep -qE "^##[[:space:]]+v?${WS_VERSION//./\\.}([[:space:]].*)?\$" "$CHANGELOG"; then
  die "CHANGELOG.md has no '## $WS_VERSION' (or '## v$WS_VERSION') section heading"
fi

# --- Check 5: cargo publish required fields ---------------------------------
# `license.workspace = true` + `repository.workspace = true` are inherited
# from [workspace.package]; we only need to check `description` is present
# per-crate (cargo emits a warning but accepts publish without it — we
# treat it as required for first-class packages).

PUBLISHABLE_CRATES=(
  fathomdb
  fathomdb-cli
  fathomdb-embedder
  fathomdb-embedder-api
  fathomdb-engine
  fathomdb-query
  fathomdb-schema
)

# Read the [package] block until next [section] and check field presence.
has_package_field() {
  local manifest="$1" field="$2"
  awk -v field="$field" '
    /^\[package\]/ { in_block = 1; next }
    /^\[/ && in_block { exit }
    in_block && $0 ~ ("^" field "[[:space:]]*[=.]") { found = 1; exit }
    END { exit (found ? 0 : 1) }
  ' "$manifest"
}

missing=0
for crate in "${PUBLISHABLE_CRATES[@]}"; do
  manifest="$CRATES_DIR/$crate/Cargo.toml"
  if [ ! -f "$manifest" ]; then
    printf 'release-gate: %s: manifest missing\n' "$manifest" >&2
    missing=1
    continue
  fi
  for field in description license repository; do
    if ! has_package_field "$manifest" "$field"; then
      printf 'release-gate: %s: missing required field "%s" in [package]\n' \
        "$manifest" "$field" >&2
      missing=1
    fi
  done
done
if [ "$missing" -ne 0 ]; then
  die "one or more crates are missing cargo publish metadata; fix and retag"
fi

printf 'release-gate: ok — tag=%s, axis-W=%s\n' "$TAG" "$WS_VERSION"
