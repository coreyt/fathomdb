#!/usr/bin/env bash
# scripts/tests/test_npm_inject_optional_deps.sh — R-REL-4f publish-time
# optionalDependencies injection. The main `fathomdb` package.json is committed
# WITHOUT optionalDependencies (so `npm ci` stays in sync); this script writes
# them in just before publish, one entry per published platform package under
# src/ts/npm/<triple>/, pinned to the main version.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INJECT="$REPO_ROOT/scripts/release/npm-inject-optional-deps.sh"

FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Fixture: a main package + two platform packages.
mkdir -p "$WORK/main" "$WORK/npm/linux-x64-gnu" "$WORK/npm/darwin-arm64"
printf '{\n  "name": "fathomdb",\n  "version": "1.2.3",\n  "devDependencies": {"typescript": "^6.0.3"}\n}\n' >"$WORK/main/package.json"
printf '{"name":"@fathomdb/fathomdb-linux-x64-gnu","version":"1.2.3"}' >"$WORK/npm/linux-x64-gnu/package.json"
printf '{"name":"@fathomdb/fathomdb-darwin-arm64","version":"1.2.3"}' >"$WORK/npm/darwin-arm64/package.json"

# 1) Injects one entry per platform dir, pinned to the main version, sorted.
if out="$(bash "$INJECT" "$WORK/main" "$WORK/npm" 2>&1)"; then
  deps="$(node -e "process.stdout.write(JSON.stringify(require('$WORK/main/package.json').optionalDependencies))")"
  if [ "$deps" = '{"@fathomdb/fathomdb-darwin-arm64":"1.2.3","@fathomdb/fathomdb-linux-x64-gnu":"1.2.3"}' ]; then
    pass "injects one pinned optionalDependency per platform dir (sorted)"
  else
    fail "wrong optionalDependencies: $deps (log: $out)"
  fi
else
  fail "inject exited non-zero: $out"
fi

# 2) devDependencies are preserved (not clobbered).
dev="$(node -e "process.stdout.write(JSON.stringify(require('$WORK/main/package.json').devDependencies))")"
[ "$dev" = '{"typescript":"^6.0.3"}' ] \
  && pass "preserves existing devDependencies" \
  || fail "devDependencies clobbered: $dev"

# 3) Idempotent re-run yields identical file.
h1="$(sha256sum "$WORK/main/package.json" | cut -d' ' -f1)"
bash "$INJECT" "$WORK/main" "$WORK/npm" >/dev/null 2>&1
h2="$(sha256sum "$WORK/main/package.json" | cut -d' ' -f1)"
[ "$h1" = "$h2" ] && pass "re-run is idempotent" || fail "re-run changed the file"

# 4) No platform dirs -> error (never publish a main package with no binaries).
mkdir -p "$WORK/main2" "$WORK/empty"
printf '{"name":"fathomdb","version":"1.0.0"}' >"$WORK/main2/package.json"
if bash "$INJECT" "$WORK/main2" "$WORK/empty" >/dev/null 2>&1; then
  fail "empty platform dir should error"
else
  pass "no platform packages -> error"
fi

# 5) The real repo fixture publishes ONLY linux-x64-gnu for 0.8.18.
present="$(ls -d "$REPO_ROOT"/src/ts/npm/*/ 2>/dev/null | xargs -n1 basename | sort | tr '\n' ',' )"
[ "$present" = "linux-x64-gnu," ] \
  && pass "0.8.18 committed platform set is linux-x64-gnu only (D5)" \
  || fail "unexpected committed platform set: $present"

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll npm-inject-optional-deps tests passed\n'
