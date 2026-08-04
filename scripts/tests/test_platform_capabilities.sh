#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

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
echo 'PASS test-platform-capabilities'
