#!/usr/bin/env bash
# AC-051b: pip version-skew detected at resolve time.
#
# Builds four synthetic wheels (api-v1, api-v2, probe-a, probe-b) where
# probe-a pins mock-skew-api==0.6.0 and probe-b pins
# mock-skew-api==99.99.99. Installing both into one environment forces
# the pip resolver to fail with a conflict naming mock-skew-api. The
# stand-in names map back to the real packages once REQ-048 publishing
# lands (see fixtures/pip-skew/constraints.txt).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE="$SCRIPT_DIR/../fixtures/pip-skew"

if ! command -v python3 >/dev/null 2>&1; then
  echo "skip: python3 not on PATH" >&2
  exit 0
fi

if ! python3 -c 'import setuptools, wheel' >/dev/null 2>&1; then
  echo "skip: setuptools/wheel not importable; install python build deps" >&2
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

VENV="$WORK/venv"
WHEELS="$WORK/wheels"
SRC="$WORK/src"
mkdir -p "$WHEELS"

# Copy the fixture into a writable workdir so the wheel build does not
# leak build/ or *.egg-info under the tracked fixture path.
cp -r "$FIXTURE" "$SRC"

python3 -m venv "$VENV"
PIP="$VENV/bin/pip"

# Build wheels for the two api versions + two probes into a local
# find-links directory. --no-build-isolation avoids a network fetch of
# the build backend; setuptools+wheel are already in the venv.
"$PIP" install --quiet --upgrade pip setuptools wheel >/dev/null
for pkg in api-v1 api-v2 probe-a probe-b; do
  "$PIP" wheel --quiet --no-deps --no-build-isolation \
    --wheel-dir "$WHEELS" "$SRC/$pkg" >/dev/null
done

# Resolve both probes against the local index only. Expect failure.
if out="$("$PIP" install --dry-run --no-index \
  --find-links "$WHEELS" \
  mock-fathomdb==0.6.0 mock-fathomdb-embedder==0.6.0 2>&1)"; then
  printf 'FAIL: pip resolved unexpectedly; expected conflict\n%s\n' "$out" >&2
  exit 1
fi

if ! printf '%s' "$out" | grep -q 'mock-skew-api'; then
  printf 'FAIL: pip error did not name mock-skew-api\n%s\n' "$out" >&2
  exit 1
fi

printf 'PASS: AC-051b — pip resolver detected mock-skew-api skew\n'
