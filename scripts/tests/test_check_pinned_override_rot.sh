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
  cp "$REPO_ROOT/scripts/pinned-override-advisories.json" "$fixture/scripts/pinned-override-advisories.json"
  python3 - "$fixture" "$mode" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
mode = sys.argv[2]
metadata = root / "scripts/pinned-override-rot.json"
data = json.loads(metadata.read_text())
package = "js-yaml"
version = "4.2.0" if mode == "vulnerable" else "4.3.0"
if mode == "prerelease":
    version = "4.3.0-rc.1"
data["npm_overrides"] = [{
    "package": package,
    "version": version,
    "rationale": "fixture security override",
    "advisory_ids": ["GHSA-h67p-54hq-rp68", "GHSA-52cp-r559-cp3m"],
    "unpin_evidence": {
        "resolved_version": "4.3.0",
        "dependent_ranges": [">=4.0.0, <5.0.0"],
        "provenance": "fixture no-override resolution"
    }
}]
if mode == "obsolete":
    data["npm_overrides"][0].pop("unpin_evidence")
if mode == "falsified-r2":
    data["npm_overrides"][0]["unpin_evidence"]["resolved_version"] = "999.0.0"
if mode in {"snapshot-digest", "missing-advisory-mapping"}:
    data["npm_overrides"][0]["unpin_evidence"]["resolved_version"] = "1.0.0"
if mode == "missing-advisory-mapping":
    data["npm_overrides"][0].pop("advisory_ids", None)
if mode == "snapshot-digest":
    data["advisory_snapshot"]["sha256"] = "0" * 64
if mode == "missing-rationale":
    data["npm_overrides"][0].pop("rationale")
if mode == "malformed-advisories":
    source = root / "scripts/pinned-override-advisories.json"
    snapshot = json.loads(source.read_text())
    snapshot["advisories"] = []
    source.write_text(json.dumps(snapshot), encoding="utf-8")
if mode == "empty-valid-advisory-snapshot":
    source = root / "scripts/pinned-override-advisories.json"
    snapshot = json.loads(source.read_text())
    snapshot["advisories"] = []
    source.write_text(json.dumps(snapshot), encoding="utf-8")
    data["advisory_snapshot"]["sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
if mode in {"forged-metadata-source", "forged-source", "malformed-advisory-id", "mismatched-advisory-url", "canonical-looking-forgery"}:
    source = root / "scripts/pinned-override-advisories.json"
    snapshot = json.loads(source.read_text())
    if mode == "forged-metadata-source":
        data["advisory_snapshot"]["source"] = "Invented advisory source"
    elif mode == "forged-source":
        snapshot["source"]["name"] = "Invented advisory source"
    elif mode == "malformed-advisory-id":
        snapshot["advisories"][0]["id"] = "NOT-A-GHSA"
    elif mode == "canonical-looking-forgery":
        snapshot["advisories"][0]["id"] = "GHSA-2345-2345-2345"
        snapshot["advisories"][0]["url"] = "https://github.com/advisories/GHSA-2345-2345-2345"
        snapshot["source"]["provenance"] = "Forged but canonical-looking GitHub advisory source"
    else:
        snapshot["advisories"][0]["url"] = "https://github.com/advisories/GHSA-0000-0000-0000"
    source.write_text(json.dumps(snapshot), encoding="utf-8")
    data["advisory_snapshot"]["sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
metadata.write_text(json.dumps(data), encoding="utf-8")
(root / "package.json").write_text(json.dumps({"overrides": {package: version}}), encoding="utf-8")
(root / "package-lock.json").write_text(json.dumps({
    "lockfileVersion": 3,
    "packages": {"node_modules/parent": {"dependencies": {
        package: ">=4.0.0, <5.0.0"
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

# Structural checks must still apply after an intentionally reviewed snapshot
# refresh. This copies the checker and replaces only its source-owned anchor
# with the fixture's digest; ordinary fixtures always use the real anchor.
run_fixture_with_reanchored_snapshot() {
  local fixture="$1"
  local fixture_checker="$fixture/check-pinned-override-rot.py"
  python3 - "$REPO_ROOT/scripts/check-pinned-override-rot.py" "$fixture" "$fixture_checker" <<'PY'
import hashlib
import re
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
root = Path(sys.argv[2])
target = Path(sys.argv[3])
digest = hashlib.sha256((root / "scripts/pinned-override-advisories.json").read_bytes()).hexdigest()
updated, count = re.subn(
    r'^PINNED_ADVISORY_SNAPSHOT_SHA256 = "[0-9a-f]{64}"$',
    f'PINNED_ADVISORY_SNAPSHOT_SHA256 = "{digest}"',
    source,
    flags=re.MULTILINE,
)
if count != 1:
    raise SystemExit("fixture could not replace exactly one checker advisory anchor")
target.write_text(updated, encoding="utf-8")
PY
  set +e
  OUT="$(python3 "$fixture_checker" --root "$fixture" 2>&1)"
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

# R2 deliberately fails closed: a lockfile produced while an override is live
# cannot prove what npm would resolve without that override.
run_fixture "$(make_fixture obsolete obsolete)"
if [ "$RC" -ne 2 ]; then
  fail "R2 must be unverified without reproducible evidence, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: R2 cannot derive a no-override resolution' \
  'R2 fails closed when npm cannot reproduce a no-override resolution'

# R3: an override cannot rely on an unstructured package.json comment.
run_fixture "$(make_fixture missing-rationale missing-rationale)"
expect_failure 'R3 npm override js-yaml@4.3.0 has no recorded rationale' \
  'R3 rejects an override without a recorded rationale'

# Advisory input unavailable/malformed is unverified, not clean.
run_fixture "$(make_fixture malformed-advisories malformed-advisories)"
if [ "$RC" -ne 2 ]; then
  fail "malformed advisory input must exit 2 (unverified), got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot sha256' \
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

# SemVer prereleases cannot be compared by the intentionally small stable-only
# range grammar. They must refuse a verdict, rather than flattening rc.1 to
# 2.0.0 and potentially reporting an R1 false green.
run_fixture "$(make_fixture prerelease prerelease)"
if [ "$RC" -ne 2 ]; then
  fail "prerelease override must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: prerelease version' \
  'R1 refuses prerelease pins that the stable-only comparator cannot order'

# The old R2 decision trusted a JSON field that anyone could edit. A falsified
# self-attestation cannot convert this gate into a clean (or obsolete) result.
run_fixture "$(make_fixture falsified-r2 falsified-r2)"
if [ "$RC" -ne 2 ]; then
  fail "self-attested R2 evidence must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: R2 cannot derive a no-override resolution' \
  'R2 refuses falsified self-attested no-override evidence'

# Advisory content is a separate checked-in snapshot. Its digest is pinned by
# governed metadata, so a valid-but-edited JSON source cannot silently erase an
# advisory.
run_fixture "$(make_fixture snapshot-digest snapshot-digest)"
if [ "$RC" -ne 2 ]; then
  fail "advisory digest mismatch must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot sha256' \
  'advisory snapshot content digest is enforced'

# Updating the digest cannot make an intentionally empty but valid JSON
# snapshot trustworthy: completeness is checked separately from integrity.
run_fixture_with_reanchored_snapshot "$(make_fixture empty-valid-advisory-snapshot empty-valid-advisory-snapshot)"
if [ "$RC" -ne 2 ]; then
  fail "empty advisory snapshot must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot advisories must be a non-empty list' \
  'empty valid advisory snapshot is not a clean result'

# Every governed override must name the advisories considered for that package.
# An empty or omitted mapping is not evidence that there are no advisories.
run_fixture "$(make_fixture missing-advisory-mapping missing-advisory-mapping)"
if [ "$RC" -ne 2 ]; then
  fail "missing per-pin advisory mapping must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: npm override js-yaml.advisory_ids' \
  'per-pin advisory mapping cannot be omitted'

# The digest binds a checked-in snapshot, but it must not let a rehashed
# arbitrary source, identifier, or URL impersonate the GitHub Advisory DB.
# Each arm deliberately recomputes the digest after mutating that field.
run_fixture "$(make_fixture forged-metadata-source forged-metadata-source)"
if [ "$RC" -ne 2 ]; then
  fail "forged metadata advisory source must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory_snapshot.source' \
  'metadata source must be the canonical GitHub Advisory Database'

run_fixture_with_reanchored_snapshot "$(make_fixture forged-source forged-source)"
if [ "$RC" -ne 2 ]; then
  fail "forged advisory source must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot source.name' \
  'snapshot source must be the canonical GitHub Advisory Database'

run_fixture_with_reanchored_snapshot "$(make_fixture malformed-advisory-id malformed-advisory-id)"
if [ "$RC" -ne 2 ]; then
  fail "malformed GitHub advisory id must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot advisories[0].id' \
  'snapshot advisory id must use the canonical GHSA form'

run_fixture_with_reanchored_snapshot "$(make_fixture mismatched-advisory-url mismatched-advisory-url)"
if [ "$RC" -ne 2 ]; then
  fail "mismatched GitHub advisory URL must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot advisories[0].url' \
  'snapshot advisory URL must exactly name its GHSA identifier'

# A checksum recorded only in mutable metadata cannot authenticate a forged
# but canonical-looking GitHub source. Recomputing that metadata checksum must
# still fail against the checker-source anchor.
run_fixture "$(make_fixture canonical-looking-forgery canonical-looking-forgery)"
if [ "$RC" -ne 2 ]; then
  fail "canonical-looking forged advisory snapshot must be unverified, got rc=$RC output=$OUT"
fi
expect_failure 'UNVERIFIED pinned-override-rot: advisory snapshot sha256 does not match independently pinned checker digest' \
  'canonical-looking forged advisory snapshot cannot bypass the checker anchor'

# Cargo git sources can appear in workspace and target-specific dependency
# tables, not just root dependency sections. Both must block a clean result.
cargo_fixture="$WORK/cargo-nested-git"
mkdir -p "$cargo_fixture/scripts" "$cargo_fixture/member"
cp "$REPO_ROOT/scripts/pinned-override-rot.json" "$cargo_fixture/scripts/pinned-override-rot.json"
cp "$REPO_ROOT/scripts/pinned-override-advisories.json" "$cargo_fixture/scripts/pinned-override-advisories.json"
printf '%s\n' '{"overrides": {}}' >"$cargo_fixture/package.json"
printf '%s\n' '{"lockfileVersion": 3, "packages": {}}' >"$cargo_fixture/package-lock.json"
cat >"$cargo_fixture/Cargo.toml" <<'EOF'
[workspace]
resolver = "2"

[workspace.dependencies]
workspace-git = { git = "https://example.invalid/workspace-git" }
EOF
cat >"$cargo_fixture/member/Cargo.toml" <<'EOF'
[package]
name = "member"
version = "0.1.0"

[target.'cfg(unix)'.dependencies]
target-git = { git = "https://example.invalid/target-git" }
EOF
run_fixture "$cargo_fixture"
if [ "$RC" -ne 1 ]; then
  fail "nested Cargo git dependencies must fail, got rc=$RC output=$OUT"
fi
expect_failure 'Cargo.toml:workspace.dependencies.workspace-git' \
  'Cargo workspace dependency git override is detected'
expect_failure 'member/Cargo.toml:target.cfg(unix).dependencies.target-git' \
  'Cargo target-specific dependency git override is detected'

# The real tree is the regression half: after removing obsolete root overrides,
# the snapshot remains parseable and the gate stays clean without a network.
run_fixture "$REPO_ROOT"
if [ "$RC" -ne 0 ]; then
  fail "real repository must pass the offline pin-rot gate: $OUT"
fi
pass 'real repository has no unrecorded or stale governed npm override'
