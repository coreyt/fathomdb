#!/usr/bin/env bash
# scripts/set-version.sh — two-axis version single source of truth.
#
# Axis W (workspace lockstep): Cargo.toml [workspace.package].version,
# src/python/pyproject.toml [project].version, src/ts/package.json
# top-level "version". Inherited by every workspace crate via
# `version.workspace = true`.
#
# Axis E (embedder-api independent semver):
# src/rust/crates/fathomdb-embedder-api/Cargo.toml [package].version.
#
# Owner: dev/design/release.md § Version axes.
set -euo pipefail

usage() {
  cat <<'USAGE' >&2
Usage: scripts/set-version.sh <mode> [args]

Modes:
  --workspace <new-w-version>     Set Axis W (Cargo workspace + python + ts).
  --embedder-api <new-e-version>  Set Axis E (fathomdb-embedder-api only).
  --check-files                   Verify both axes are internally consistent.

Examples:
  scripts/set-version.sh --workspace 0.6.1
  scripts/set-version.sh --embedder-api 0.7.0
  scripts/set-version.sh --check-files
USAGE
}

die_usage() {
  if [ -n "${1:-}" ]; then
    printf 'error: %s\n' "$1" >&2
  fi
  usage
  exit 2
}

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO="$REPO_ROOT/Cargo.toml"
PYPROJ="$REPO_ROOT/src/python/pyproject.toml"
NPMPKG="$REPO_ROOT/src/ts/package.json"
EMB_API_DIR="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api"
EMB_API="$EMB_API_DIR/Cargo.toml"
CRATES_DIR="$REPO_ROOT/src/rust/crates"
NPM_PLATFORM_DIR="$REPO_ROOT/src/ts/npm"

# Atomic write: edits "$1" with awk program "$2" via a tmpfile mv.
awk_inplace() {
  local target="$1"
  local prog="$2"
  local tmp
  tmp="$(mktemp "${target}.XXXXXX")"
  awk "$prog" "$target" >"$tmp"
  mv "$tmp" "$target"
}

# Set [workspace.package].version in Cargo.toml.
set_workspace_version() {
  local new="$1"
  awk_inplace "$CARGO" '
    BEGIN { in_block = 0 }
    /^\[workspace\.package\]/ { in_block = 1; print; next }
    /^\[/ { in_block = 0; print; next }
    {
      if (in_block && $0 ~ /^version[[:space:]]*=/) {
        print "version = \"'"$new"'\""
      } else {
        print
      }
    }
  '
}

# Update every Axis-W entry in [workspace.dependencies]. fathomdb-embedder-api
# is Axis E and is skipped here — its version is set separately via
# set_workspace_dep_embedder_api_version().
#
# Each line we touch is of the shape:
#   fathomdb-<name> = { path = "...", version = "X.Y.Z" }
# We rewrite only the version="X.Y.Z" segment; path is preserved.
set_workspace_dep_axis_w_versions() {
  local new="$1"
  awk_inplace "$CARGO" '
    BEGIN { in_block = 0 }
    /^\[workspace\.dependencies\]/ { in_block = 1; print; next }
    /^\[/ { in_block = 0; print; next }
    {
      if (in_block && $0 ~ /^fathomdb(-[a-z]+)*[[:space:]]*=/) {
        # Skip Axis E (fathomdb-embedder-api).
        if ($0 ~ /^fathomdb-embedder-api[[:space:]]*=/) {
          print; next
        }
        # Rewrite version="..." in-place, preserve everything else.
        if (sub(/version[[:space:]]*=[[:space:]]*"[^"]*"/,
                "version = \"'"$new"'\"")) {
          print; next
        }
      }
      print
    }
  '
}

# Update the Axis-E entry in [workspace.dependencies] (fathomdb-embedder-api).
set_workspace_dep_embedder_api_version() {
  local new="$1"
  awk_inplace "$CARGO" '
    BEGIN { in_block = 0 }
    /^\[workspace\.dependencies\]/ { in_block = 1; print; next }
    /^\[/ { in_block = 0; print; next }
    {
      if (in_block && $0 ~ /^fathomdb-embedder-api[[:space:]]*=/) {
        if (sub(/version[[:space:]]*=[[:space:]]*"[^"]*"/,
                "version = \"'"$new"'\"")) {
          print; next
        }
      }
      print
    }
  '
}

# Set [project].version in pyproject.toml.
set_pyproject_version() {
  local new="$1"
  awk_inplace "$PYPROJ" '
    BEGIN { in_block = 0 }
    /^\[project\]/ { in_block = 1; print; next }
    /^\[/ { in_block = 0; print; next }
    {
      if (in_block && $0 ~ /^version[[:space:]]*=/) {
        print "version = \"'"$new"'\""
      } else {
        print
      }
    }
  '
}

# Set the top-level "version" field in package.json.
# Constraint: only rewrite the first occurrence so a nested "version" inside
# devDependencies cannot be touched.
set_npm_version() {
  local new="$1"
  local tmp
  tmp="$(mktemp "${NPMPKG}.XXXXXX")"
  awk -v new="$new" '
    BEGIN { done = 0 }
    {
      if (!done && match($0, /"version"[[:space:]]*:[[:space:]]*"[^"]*"/)) {
        sub(/"version"[[:space:]]*:[[:space:]]*"[^"]*"/, "\"version\": \"" new "\"")
        done = 1
      }
      print
    }
  ' "$NPMPKG" >"$tmp"
  mv "$tmp" "$NPMPKG"
}

# Set the top-level "version" in each src/ts/npm/<triple>/package.json (the
# napi per-platform binary packages). They are versioned in lockstep with
# Axis W, same as the main package.
set_npm_platform_pkg_versions() {
  local new="$1" pkg tmp
  [ -d "$NPM_PLATFORM_DIR" ] || return 0
  for pkg in "$NPM_PLATFORM_DIR"/*/package.json; do
    [ -f "$pkg" ] || continue
    tmp="$(mktemp "${pkg}.XXXXXX")"
    awk -v new="$new" '
      BEGIN { done = 0 }
      {
        if (!done && match($0, /"version"[[:space:]]*:[[:space:]]*"[^"]*"/)) {
          sub(/"version"[[:space:]]*:[[:space:]]*"[^"]*"/, "\"version\": \"" new "\"")
          done = 1
        }
        print
      }
    ' "$pkg" >"$tmp"
    mv "$tmp" "$pkg"
  done
}

# Set [package].version in fathomdb-embedder-api/Cargo.toml. Preserves any
# leading comment lines inside the [package] block (so the Axis-E why-comment
# survives re-runs).
set_embedder_api_version() {
  local new="$1"
  awk_inplace "$EMB_API" '
    BEGIN { in_block = 0 }
    /^\[package\]/ { in_block = 1; print; next }
    /^\[/ { in_block = 0; print; next }
    {
      if (in_block && $0 ~ /^version[[:space:]]*=/) {
        print "version = \"'"$new"'\""
      } else {
        print
      }
    }
  '
}

# --- readers ---------------------------------------------------------------

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

read_pyproject_version() {
  awk '
    /^\[project\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$PYPROJ"
}

read_npm_version() {
  sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$NPMPKG" | head -1
}

# Read the first "version" from an arbitrary package.json (platform pkgs).
read_npm_version_file() {
  sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -1
}

# Returns the literal text of the version line inside [package] of the
# embedder-api crate (e.g. `version = "0.6.0"` or `version.workspace = true`).
read_embedder_api_version_line() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*[=.]/ {
      print
      exit
    }
  ' "$EMB_API"
}

# Parse out the value portion: returns "0.6.0" for `version = "0.6.0"`, or
# the literal "workspace" sentinel for `version.workspace = true`.
embedder_api_version_value() {
  local line
  line="$(read_embedder_api_version_line)"
  case "$line" in
    version.workspace*) printf 'workspace' ;;
    version[[:space:]]*=*\"*) printf '%s' "$line" | sed -n 's/.*"\([^"]*\)".*/\1/p' ;;
    *) printf 'unknown' ;;
  esac
}

# --- check-files -----------------------------------------------------------

# Line number of the first `version =` / `version.workspace` declaration
# inside the given TOML [table] of the given file. Empty if not present.
_toml_version_line() {
  local file="$1" table="$2"
  awk -v table="$table" '
    $0 == table { in_block = 1; next }
    /^\[/ && in_block { exit }
    in_block && /^version[[:space:]]*[=.]/ { print NR; exit }
  ' "$file"
}

# Line number of the first "version" key in a JSON file.
_json_version_line() {
  grep -n '"version"' "$1" | head -1 | cut -d: -f1
}

# Emit a structured drift diagnostic: `<file>:<line>: version drift —
# observed "X", expected "Y"`. Centralized so every drift site has the
# same shape (parseable by tooling, asserted in test_set_version.sh).
_drift() {
  local file="$1" line="$2" observed="$3" expected="$4"
  printf '%s:%s: version drift — observed "%s", expected "%s"\n' \
    "$file" "${line:-?}" "$observed" "$expected" >&2
}

check_files() {
  local ws py npm emb
  ws="$(read_workspace_version)"
  py="$(read_pyproject_version)"
  npm="$(read_npm_version)"
  emb="$(embedder_api_version_value)"

  local rc=0

  if [ -z "$ws" ]; then
    printf 'error: %s: no [workspace.package] version found\n' "$CARGO" >&2
    rc=1
  fi

  if [ "$py" != "$ws" ]; then
    _drift "$PYPROJ" "$(_toml_version_line "$PYPROJ" '[project]')" "$py" "$ws"
    rc=1
  fi

  if [ "$npm" != "$ws" ]; then
    _drift "$NPMPKG" "$(_json_version_line "$NPMPKG")" "$npm" "$ws"
    rc=1
  fi

  # Each src/ts/npm/<triple>/package.json (platform binary package) must be at
  # Axis W too — they publish in lockstep with the main package.
  local plat_pkg plat_pkg_ver plat_pkg_no
  if [ -d "$NPM_PLATFORM_DIR" ]; then
    for plat_pkg in "$NPM_PLATFORM_DIR"/*/package.json; do
      [ -f "$plat_pkg" ] || continue
      plat_pkg_ver="$(read_npm_version_file "$plat_pkg")"
      if [ "$plat_pkg_ver" != "$ws" ]; then
        plat_pkg_no="$(_json_version_line "$plat_pkg")"
        _drift "$plat_pkg" "${plat_pkg_no:-1}" "$plat_pkg_ver" "$ws"
        rc=1
      fi
    done
  fi

  case "$emb" in
    workspace)
      _drift "$EMB_API" "$(_toml_version_line "$EMB_API" '[package]')" \
        "version.workspace = true" "explicit [package] version"
      rc=1
      ;;
    unknown|'')
      _drift "$EMB_API" "$(_toml_version_line "$EMB_API" '[package]')" \
        "unparseable" "explicit [package] version"
      rc=1
      ;;
  esac

  # Every workspace crate except fathomdb-embedder-api must inherit Axis W
  # via `version.workspace = true`. Catch silent decoupling regressions.
  local crate manifest line line_no pkg_line
  for crate in "$CRATES_DIR"/*; do
    [ -d "$crate" ] || continue
    manifest="$crate/Cargo.toml"
    [ -f "$manifest" ] || continue
    if [ "$crate" = "$EMB_API_DIR" ]; then
      continue
    fi
    line="$(awk '
      /^\[package\]/ { in_block = 1; next }
      /^\[/          { in_block = 0 }
      in_block && /^version[[:space:]]*[=.]/ {
        print
        exit
      }
    ' "$manifest")"
    case "$line" in
      version.workspace[[:space:]]*=[[:space:]]*true) ;;
      '')
        pkg_line="$(grep -n '^\[package\]' "$manifest" | head -1 | cut -d: -f1)"
        _drift "$manifest" "${pkg_line:-1}" "<missing>" "version.workspace = true"
        rc=1
        ;;
      *)
        line_no="$(_toml_version_line "$manifest" '[package]')"
        _drift "$manifest" "${line_no:-1}" "$line" "version.workspace = true"
        rc=1
        ;;
    esac
  done

  # [workspace.dependencies] version pins must match Axis W (or Axis E for
  # fathomdb-embedder-api). `cargo publish` requires each in-workspace dep
  # to carry a version requirement — if the pins drift from the axis
  # version, the published manifest will fail registry resolution at the
  # next tier.
  while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    local dep_name dep_ver expected line_no
    dep_name="$(printf '%s' "$entry" | awk -F'[[:space:]]*=' '{print $1}')"
    dep_ver="$(printf '%s' "$entry" | sed -nE 's/.*version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p')"
    case "$dep_name" in
      fathomdb-embedder-api) expected="$emb" ;;
      *)                     expected="$ws"  ;;
    esac
    if [ -z "$dep_ver" ]; then
      line_no="$(grep -nE "^${dep_name}[[:space:]]*=" "$CARGO" | head -1 | cut -d: -f1)"
      _drift "$CARGO" "${line_no:-1}" "<missing version>" "$expected"
      rc=1
    elif [ "$dep_ver" != "$expected" ]; then
      line_no="$(grep -nE "^${dep_name}[[:space:]]*=" "$CARGO" | head -1 | cut -d: -f1)"
      _drift "$CARGO" "${line_no:-1}" "$dep_ver" "$expected"
      rc=1
    fi
  done <<EOF
$(awk '
  /^\[workspace\.dependencies\]/ { in_block = 1; next }
  /^\[/ && in_block { exit }
  in_block && /^fathomdb/ { print }
' "$CARGO")
EOF

  if [ $rc -eq 0 ]; then
    printf 'ok: Axis W = %s; Axis E = %s\n' "$ws" "$emb"
  fi
  return $rc
}

# --- dispatch --------------------------------------------------------------

if [ $# -eq 0 ]; then
  die_usage "no mode given"
fi

mode="$1"
shift || true

case "$mode" in
  --workspace)
    if [ $# -ne 1 ] || [ -z "${1:-}" ]; then
      die_usage "--workspace requires <new-w-version>"
    fi
    new="$1"
    set_workspace_version "$new"
    set_workspace_dep_axis_w_versions "$new"
    set_pyproject_version "$new"
    set_npm_version "$new"
    set_npm_platform_pkg_versions "$new"
    check_files
    ;;
  --embedder-api)
    if [ $# -ne 1 ] || [ -z "${1:-}" ]; then
      die_usage "--embedder-api requires <new-e-version>"
    fi
    new="$1"
    set_embedder_api_version "$new"
    set_workspace_dep_embedder_api_version "$new"
    check_files
    ;;
  --check-files)
    if [ $# -ne 0 ]; then
      die_usage "--check-files takes no arguments"
    fi
    check_files
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    die_usage "unknown mode: $mode"
    ;;
esac
