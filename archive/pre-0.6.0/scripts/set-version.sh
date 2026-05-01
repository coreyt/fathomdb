#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

CARGO_TOML="$REPO_ROOT/Cargo.toml"
PYPROJECT_TOML="$REPO_ROOT/python/pyproject.toml"
PACKAGE_JSON="$REPO_ROOT/typescript/packages/fathomdb/package.json"

# Version files: Cargo.toml has the workspace version plus workspace dependency
# versions; pyproject.toml and package.json each have a single version field.

get_cargo_version() {
    sed -n 's/^version = "\([^"]*\)"/\1/p' "$CARGO_TOML" | head -1
}

get_python_version() {
    sed -n 's/^version = "\([^"]*\)"/\1/p' "$PYPROJECT_TOML" | head -1
}

get_npm_version() {
    sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "$PACKAGE_JSON" | head -1
}

set_all_versions() {
    local version="$1"

    # Cargo.toml: workspace version and workspace dependency versions
    sed -i "s/^version = \"[^\"]*\"/version = \"$version\"/" "$CARGO_TOML"
    sed -i "s/\(fathomdb-engine = { path = \"crates\/fathomdb-engine\", version = \"\)[^\"]*/\1$version/" "$CARGO_TOML"
    sed -i "s/\(fathomdb-query = { path = \"crates\/fathomdb-query\", version = \"\)[^\"]*/\1$version/" "$CARGO_TOML"
    sed -i "s/\(fathomdb-schema = { path = \"crates\/fathomdb-schema\", version = \"\)[^\"]*/\1$version/" "$CARGO_TOML"

    # python/pyproject.toml
    sed -i "s/^version = \"[^\"]*\"/version = \"$version\"/" "$PYPROJECT_TOML"

    # typescript/packages/fathomdb/package.json
    sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" "$PACKAGE_JSON"
}

check_files() {
    local cargo_v python_v npm_v
    cargo_v="$(get_cargo_version)"
    python_v="$(get_python_version)"
    npm_v="$(get_npm_version)"

    local rc=0
    if [ "$cargo_v" != "$python_v" ]; then
        echo "MISMATCH: Cargo.toml=$cargo_v  python/pyproject.toml=$python_v" >&2
        rc=1
    fi
    if [ "$cargo_v" != "$npm_v" ]; then
        echo "MISMATCH: Cargo.toml=$cargo_v  typescript/package.json=$npm_v" >&2
        rc=1
    fi
    if [ "$rc" -eq 0 ]; then
        echo "OK: all versions are $cargo_v"
    fi
    return $rc
}

parse_version() {
    local v="$1"
    IFS='.' read -r MAJOR MINOR MICRO <<< "$v"
}

usage() {
    cat <<'USAGE'
Usage: set-version.sh [OPTIONS]

Options:
  --set-version VERSION   Set all version files to VERSION (e.g. 1.2.3)
  --increment-major       Bump major, reset minor and micro to 0
  --increment-minor       Bump minor, reset micro to 0
  --increment-micro       Bump micro (patch) version
  --check-files           Check all version files are in sync (exit 1 if not)
  -h, --help              Show this help message

Exactly one action must be specified.
USAGE
}

ACTION=""
SET_VERSION=""

while [ $# -gt 0 ]; do
    case "$1" in
        --set-version)
            ACTION="set"
            SET_VERSION="$2"
            shift 2
            ;;
        --increment-major)
            ACTION="inc-major"
            shift
            ;;
        --increment-minor)
            ACTION="inc-minor"
            shift
            ;;
        --increment-micro)
            ACTION="inc-micro"
            shift
            ;;
        --check-files)
            ACTION="check"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$ACTION" ]; then
    usage >&2
    exit 2
fi

case "$ACTION" in
    check)
        check_files
        ;;
    set)
        if [ -z "$SET_VERSION" ]; then
            echo "error: --set-version requires a VERSION argument" >&2
            exit 2
        fi
        set_all_versions "$SET_VERSION"
        check_files
        ;;
    inc-major)
        current="$(get_cargo_version)"
        parse_version "$current"
        new="$((MAJOR + 1)).0.0"
        echo "$current -> $new"
        set_all_versions "$new"
        check_files
        ;;
    inc-minor)
        current="$(get_cargo_version)"
        parse_version "$current"
        new="$MAJOR.$((MINOR + 1)).0"
        echo "$current -> $new"
        set_all_versions "$new"
        check_files
        ;;
    inc-micro)
        current="$(get_cargo_version)"
        parse_version "$current"
        new="$MAJOR.$MINOR.$((MICRO + 1))"
        echo "$current -> $new"
        set_all_versions "$new"
        check_files
        ;;
esac
