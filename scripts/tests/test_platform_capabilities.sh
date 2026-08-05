#!/usr/bin/env bash
set -euo pipefail
# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1

scripts/check-platform-capabilities.sh

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
cp dev/platform-capabilities.json "$tmpdir/manifest.json"
python3 - "$tmpdir/manifest.json" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path))
data['platforms'][0]['npm_package'] = 'wrong'
json.dump(data, open(path, 'w'))
PY
if MANIFEST_PATH="$tmpdir/manifest.json" scripts/check-platform-capabilities.sh >/dev/null 2>&1; then
  echo 'FAIL test-platform-capabilities: mismatched package metadata passed' >&2
  exit 1
fi

cp dev/platform-capabilities.json "$tmpdir/manifest.json"
python3 - "$tmpdir/manifest.json" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path))
data['platforms'][1]['status'] = 'release-ready'
json.dump(data, open(path, 'w'))
PY
if MANIFEST_PATH="$tmpdir/manifest.json" scripts/check-platform-capabilities.sh >/dev/null 2>&1; then
  echo 'FAIL test-platform-capabilities: unadvertised ARM64 publication passed' >&2
  exit 1
fi
echo 'PASS test-platform-capabilities'
