#!/usr/bin/env bash
# scripts/release/smoke/smoke-pypi-wheel.sh — AC-056 PyPI smoke.
#
#   $1 = version (e.g. 0.6.0)
#
# Installs fathomdb==$1 from PyPI into a fresh venv, exercises the SDK
# end-to-end (open + write empty batch + search + close), and asserts
# the python process exits cleanly. Per `feedback_release_verification`:
# the wheel-on-disk lock cleanup + process exit are the bug signal that
# only fires under real install-from-registry conditions.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  printf 'usage: %s <version>\n' "$0" >&2
  exit 2
fi
VERSION="$1"
if ! printf '%s' "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  printf 'smoke-pypi-wheel: invalid version "%s" — expected semver MAJOR.MINOR.PATCH\n' \
    "$VERSION" >&2
  exit 2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

python3 -m venv "$WORK/venv"
# shellcheck source=/dev/null
. "$WORK/venv/bin/activate"
pip install --quiet --upgrade pip
pip install --quiet "fathomdb==${VERSION}"

DB="$WORK/smoke.fdb"
python3 - "$DB" <<'PY'
import sys
from fathomdb import Engine
db_path = sys.argv[1]
e = Engine.open(db_path)
e.write([])
e.search("smoke")
e.close()
print("ok")
PY

printf 'smoke-pypi-wheel: ok — fathomdb %s installed + open/write/search/close + process exit clean\n' \
  "$VERSION"
