#!/usr/bin/env bash
# scripts/release/smoke/smoke-npm-package.sh — AC-056 npm smoke.
#
#   $1 = version (e.g. 0.6.0)
#
# Installs fathomdb@$1 from npm into a fresh workspace, exercises the
# napi binding end-to-end (open + write minimal record + search + close),
# and asserts the node process exits cleanly. Same `feedback_release_
# verification` rationale as the PyPI smoke — locks + process exit are
# the bug signal that only fires under real install-from-registry.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  printf 'usage: %s <version>\n' "$0" >&2
  exit 2
fi
VERSION="$1"
if ! printf '%s' "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
  printf 'smoke-npm-package: invalid version "%s" — expected semver MAJOR.MINOR.PATCH[-PRERELEASE]\n' \
    "$VERSION" >&2
  exit 2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

cd "$WORK"
npm init -y >/dev/null
npm install --silent "fathomdb@${VERSION}"

DB="$WORK/smoke.fdb"
cat > smoke.mjs <<'JS'
import { Engine } from "fathomdb";
const dbPath = process.argv[2];
const e = await Engine.open(dbPath);
await e.write([{ kind: "doc", body: "{}" }]);
await e.search("smoke");
await e.close();
console.log("ok");
JS
node smoke.mjs "$DB"

printf 'smoke-npm-package: ok — fathomdb %s installed + open/write/search/close + process exit clean\n' \
  "$VERSION"
