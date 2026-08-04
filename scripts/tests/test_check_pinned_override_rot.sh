#!/usr/bin/env bash
# RED-first recurrence coverage for the offline pinned-override rot gate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-pinned-override-rot.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; exit 1; }

make_fixture() {
  local name="$1"
  local mode="$2"
  local fixture="$WORK/$name"
  mkdir -p "$fixture/scripts"
  cp "$REPO_ROOT/scripts/pinned-override-rot.json" "$fixture/scripts/pinned-override-rot.json"
  python3 - "$fixture" "$mode" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
mode = sys.argv[2]
metadata = root / "scripts/pinned-override-rot.json"
data = json.loads(metadata.read_text())
package = "js-yaml" if mode == "vulnerable" else "example-pin"
version = "4.2.0" if mode == "vulnerable" else "2.0.0"
data["npm_overrides"] = [{
    "package": package,
    "version": version,
    "rationale": "fixture security override",
    "unpin_evidence": {
        "resolved_version": "3.0.0" if mode == "vulnerable" else "2.0.0",
        "dependent_ranges": [">=2.0.0, <4.0.0"] if mode == "vulnerable" else [">=2.0.0, <3.0.0"],
        "provenance": "fixture no-override resolution"
    }
}]
if mode == "missing-rationale":
    data["npm_overrides"][0].pop("rationale")
if mode == "malformed-advisories":
    data.pop("advisories")
metadata.write_text(json.dumps(data), encoding="utf-8")
(root / "package.json").write_text(json.dumps({"overrides": {package: version}}), encoding="utf-8")
(root / "package-lock.json").write_text(json.dumps({
    "lockfileVersion": 3,
    "packages": {"node_modules/parent": {"dependencies": {
        package: ">=2.0.0, <4.0.0" if mode == "vulnerable" else ">=2.0.0, <3.0.0"
    }}}
}), encoding="utf-8")
(root / "Cargo.toml").write_text("[workspace]\nresolver = '2'\n", encoding="utf-8")
PY
  printf '%s' "$fixture"
}

run_fixture() {
  local fixture="$1"
  set +e
  OUT="$(bash "$CHECKER" --root "$fixture" 2>&1)"
  RC=$?
  set -e
}

expect_failure() {
  local expected="$1" description="$2"
  if [ "$RC" -ne 1 ] && [ "$RC" -ne 2 ]; then
    fail "$description — expected a hard failure, got rc=$RC output=$OUT"
  fi
  if ! grep -Fq "$expected" <<<"$OUT"; then
    fail "$description — output did not name $expected: $OUT"
  fi
  pass "$description"
}

# R1 exact historical regression: 4.2.0 was the old js-yaml override and is
# inside GHSA-52cp-r559-cp3m's >=4.0.0,<4.3.0 range.
run_fixture "$(make_fixture vulnerable vulnerable)"
expect_failure 'R1 npm override js-yaml@4.2.0 is vulnerable to GHSA-52cp-r559-cp3m' \
  'R1 rejects the historical vulnerable js-yaml 4.2.0 override'

# R2: the fixture's documented no-override resolution is safe and satisfies
# every recorded dependent constraint, so retaining the override is stale.
run_fixture "$(make_fixture obsolete obsolete)"
expect_failure 'R2 npm override example-pin@2.0.0 is obsolete' \
  'R2 hard-fails an override whose no-override resolution meets its reason'

# R3: an override cannot rely on an unstructured package.json comment.
run_fixture "$(make_fixture missing-rationale missing-rationale)"
expect_failure 'R3 npm override example-pin@2.0.0 has no recorded rationale' \
  'R3 rejects an override without a recorded rationale'

# Advisory input unavailable/malformed is unverified, not clean.
run_fixture "$(make_fixture malformed-advisories malformed-advisories)"
if [ "$RC" -ne 2 ]; then
  fail "malformed advisory input must exit 2 (unverified), got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: metadata has no advisories list' \
  'malformed advisory input loudly refuses a clean verdict'

# A missing checked-in advisory source is also an unverified failure, never a
# network fallback or an implicit pass.
fixture="$(make_fixture missing-source obsolete)"
set +e
OUT="$(bash "$CHECKER" --root "$fixture" --metadata "$fixture/scripts/does-not-exist.json" 2>&1)"
RC=$?
set -e
if [ "$RC" -ne 2 ]; then
  fail "missing advisory source must exit 2 (unverified), got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: cannot read pinned-override metadata' \
  'missing advisory input loudly refuses a clean verdict'

# The real tree is the regression half: after removing obsolete root overrides,
# the snapshot remains parseable and the gate stays clean without a network.
run_fixture "$REPO_ROOT"
if [ "$RC" -ne 0 ]; then
  fail "real repository must pass the offline pin-rot gate: $OUT"
fi
pass 'real repository has no unrecorded or stale governed npm override'
