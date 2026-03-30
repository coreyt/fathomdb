#!/usr/bin/env bash
# Installs the project-local SQLite version by sourcing tooling/sqlite.env
# and the helper functions from developer-setup.sh.
#
# Usage (CI or local):
#   INSTALL_PREFIX=/path/to/.local bash scripts/install-project-sqlite.sh
#
# If INSTALL_PREFIX is not set, defaults to $REPO_ROOT/.local/sqlite-$SQLITE_VERSION.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Source the version policy.
SQLITE_POLICY_FILE="$REPO_ROOT/tooling/sqlite.env"
if [[ ! -f "$SQLITE_POLICY_FILE" ]]; then
  echo "error: sqlite.env not found at $SQLITE_POLICY_FILE" >&2
  exit 1
fi
# shellcheck disable=SC1090
source "$SQLITE_POLICY_FILE"

SQLITE_VERSION="${SQLITE_VERSION:?SQLITE_VERSION must be set in sqlite.env}"

# Derive numeric version and download URL using the same logic as developer-setup.sh.
sqlite_numeric_version() {
  local version="$1"
  local major minor patch
  IFS=. read -r major minor patch <<<"$version"
  printf '%d%02d%02d00\n' "$major" "$minor" "$patch"
}

sqlite_release_year() {
  local version="$1"
  case "$version" in
    3.41.*|3.42.*|3.43.*|3.44.*) printf '2023\n' ;;
    3.45.*|3.46.*) printf '2024\n' ;;
    *) echo "error: unsupported SQLite release year for $version" >&2; exit 1 ;;
  esac
}

NUMERIC_VERSION="$(sqlite_numeric_version "$SQLITE_VERSION")"
YEAR="$(sqlite_release_year "$SQLITE_VERSION")"
ARCHIVE_URL="https://sqlite.org/${YEAR}/sqlite-autoconf-${NUMERIC_VERSION}.tar.gz"

INSTALL_DIR="${INSTALL_PREFIX:-$REPO_ROOT/.local/sqlite-$SQLITE_VERSION}"

# Skip if already installed at the correct version.
if [[ -x "$INSTALL_DIR/bin/sqlite3" ]]; then
  installed="$("$INSTALL_DIR/bin/sqlite3" --version | awk '{print $1}')"
  if [[ "$installed" == "$SQLITE_VERSION" ]]; then
    echo "SQLite $SQLITE_VERSION already installed at $INSTALL_DIR"
    exit 0
  fi
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Installing SQLite $SQLITE_VERSION to $INSTALL_DIR"
curl -fsSL "$ARCHIVE_URL" -o "$TMP_DIR/sqlite.tar.gz"
tar xzf "$TMP_DIR/sqlite.tar.gz" -C "$TMP_DIR"
cd "$TMP_DIR/sqlite-autoconf-${NUMERIC_VERSION}"
./configure --prefix="$INSTALL_DIR" --disable-shared --quiet
make -j"$(nproc)" --quiet
make install --quiet
echo "SQLite $SQLITE_VERSION installed successfully"
