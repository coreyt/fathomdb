#!/usr/bin/env bash
# scripts/tests/test_verify_release_gates.sh — coverage for the release-gate
# preflight script. Owner contract: dev/design/release.md § Tiered publish
# order — every check that gates entry into T1 is exercised here at least
# once positive + once negative.
#
# Tag/branch context for the script is injected via GITHUB_REF_NAME +
# RELEASE_GATES_HEAD_REF env so the test does not depend on git state.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VRG="$REPO_ROOT/scripts/verify-release-gates.sh"

CARGO="$REPO_ROOT/Cargo.toml"
EMBAPI="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api/Cargo.toml"
PYPROJ="$REPO_ROOT/src/python/pyproject.toml"
NPMPKG="$REPO_ROOT/src/ts/package.json"

FAILED=0
TMPDIR_ROOT="$(mktemp -d)"
SNAP="$TMPDIR_ROOT/snap"
mkdir -p "$SNAP"
trap 'restore; rm -rf "$TMPDIR_ROOT"' EXIT

cp "$CARGO" "$SNAP/Cargo.toml"
cp "$EMBAPI" "$SNAP/embedder-api.toml"
cp "$PYPROJ" "$SNAP/pyproject.toml"
cp "$NPMPKG" "$SNAP/package.json"
SNAP_CRATES="$SNAP/crates"
mkdir -p "$SNAP_CRATES"
for c in "$REPO_ROOT"/src/rust/crates/*/Cargo.toml; do
  rel="$(basename "$(dirname "$c")")"
  cp "$c" "$SNAP_CRATES/$rel.toml"
done

restore() {
  cp "$SNAP/Cargo.toml" "$CARGO" 2>/dev/null || true
  cp "$SNAP/embedder-api.toml" "$EMBAPI" 2>/dev/null || true
  cp "$SNAP/pyproject.toml" "$PYPROJ" 2>/dev/null || true
  cp "$SNAP/package.json" "$NPMPKG" 2>/dev/null || true
  for f in "$SNAP_CRATES"/*.toml; do
    base="$(basename "$f" .toml)"
    cp "$f" "$REPO_ROOT/src/rust/crates/$base/Cargo.toml" 2>/dev/null || true
  done
}

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

ws_version() {
  awk '
    /^\[workspace\.package\]/ { in_block = 1; next }
    /^\[/                     { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$CARGO"
}

# Stub HEAD-on-main check by exporting RELEASE_GATES_SKIP_GIT_REACH=1; the
# script honors the override for tests so the suite does not rely on the
# actual git topology of the worktree under test.
export RELEASE_GATES_SKIP_GIT_REACH=1
# Provide a CHANGELOG override path the test controls.
CL_PATH="$TMPDIR_ROOT/CHANGELOG.md"
export RELEASE_GATES_CHANGELOG="$CL_PATH"

WS="$(ws_version)"

# 1. Happy path: tag matches workspace version, CHANGELOG has matching
#    heading, --check-files passes, every crate has description.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
GITHUB_REF_NAME="v${WS}" "$VRG" >/dev/null 2>&1 \
  && pass "happy path passes" \
  || fail "happy path should pass"

# 2. Tag prefix wrong (no leading v) → fail with structured tag-mismatch line.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_REF_NAME="${WS}" "$VRG" 2>&1)"; then
  fail "tag without v prefix should fail"
else
  printf '%s' "$out" | grep -qi 'tag' \
    && pass "tag-without-v prefix rejected with tag-related diagnostic" \
    || fail "wrong diagnostic for missing v prefix; got: $out"
fi

# 3. Tag version drifts from workspace → fail with structured tag-mismatch.
restore
printf '# Changelog\n\n## 9.9.9\n' > "$CL_PATH"
if out="$(GITHUB_REF_NAME="v9.9.9" "$VRG" 2>&1)"; then
  fail "tag drift should fail"
else
  printf '%s' "$out" | grep -qiE 'tag.*(drift|mismatch|does not match)' \
    && pass "tag/workspace drift rejected" \
    || fail "wrong diagnostic for tag drift; got: $out"
fi

# 4. set-version.sh --check-files failure surfaces via this gate.
restore
sed -i 's/^version = "[^"]*"/version = "7.7.7"/' "$REPO_ROOT/src/python/pyproject.toml"
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_REF_NAME="v${WS}" "$VRG" 2>&1)"; then
  cp "$SNAP/Cargo.toml" "$CARGO"
  fail "drift in --check-files should fail the gate"
else
  printf '%s' "$out" | grep -qi 'check-files\|version drift' \
    && pass "set-version --check-files drift surfaces in gate" \
    || fail "wrong diagnostic for --check-files drift; got: $out"
fi

# 5. CHANGELOG missing the tag section → fail.
restore
printf '# Changelog\n\n## 0.0.1\n' > "$CL_PATH"
if out="$(GITHUB_REF_NAME="v${WS}" "$VRG" 2>&1)"; then
  fail "missing CHANGELOG section should fail"
else
  printf '%s' "$out" | grep -qi 'changelog' \
    && pass "missing CHANGELOG section rejected" \
    || fail "wrong diagnostic for missing CHANGELOG section; got: $out"
fi

# 6. Crate metadata: drop description on fathomdb-engine → fail with
#    a diagnostic naming the crate manifest.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
ENG="$REPO_ROOT/src/rust/crates/fathomdb-engine/Cargo.toml"
sed -i '/^description[[:space:]]*=/d' "$ENG"
if out="$(GITHUB_REF_NAME="v${WS}" "$VRG" 2>&1)"; then
  cp "$SNAP_CRATES/fathomdb-engine.toml" "$ENG"
  fail "missing description should fail crate-metadata check"
else
  cp "$SNAP_CRATES/fathomdb-engine.toml" "$ENG"
  printf '%s' "$out" | grep -qi 'description' \
    && printf '%s' "$out" | grep -q 'fathomdb-engine' \
    && pass "missing description on fathomdb-engine flagged" \
    || fail "wrong diagnostic for missing description; got: $out"
fi

# 7. Missing GITHUB_REF_NAME (no tag context) → fail with usage-ish diagnostic.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(unset GITHUB_REF_NAME; "$VRG" 2>&1)"; then
  fail "missing GITHUB_REF_NAME should fail"
else
  printf '%s' "$out" | grep -qi 'ref_name\|tag' \
    && pass "missing GITHUB_REF_NAME rejected" \
    || fail "wrong diagnostic for missing tag context; got: $out"
fi

# 8. RELEASE_GATES_SKIP_GIT_REACH=0 + bogus ref → fail (head-on-main check
#    enforced when not skipped, exercised via a non-existent ref). MUST use
#    a GA-shape version (no hyphen) so the RC-skip path (per HITL
#    2026-05-17) doesn't bypass the check. Bumps workspace to a synthetic
#    GA version, then restore() resets state.
restore
GA_VERSION="0.999.0"
bash "$REPO_ROOT/scripts/set-version.sh" --workspace "$GA_VERSION" >/dev/null
printf '# Changelog\n\n## %s\n' "$GA_VERSION" > "$CL_PATH"
if out="$(RELEASE_GATES_SKIP_GIT_REACH=0 RELEASE_GATES_HEAD_REF="refs/heads/__nonexistent__" \
    GITHUB_REF_NAME="v${GA_VERSION}" "$VRG" 2>&1)"; then
  fail "head-not-on-main should fail when reach check enabled (GA shape)"
else
  printf '%s' "$out" | grep -qiE 'main|reach' \
    && pass "head-not-reachable-from-main rejected (GA shape)" \
    || fail "wrong diagnostic for unreachable HEAD; got: $out"
fi
restore

# 9. workflow_dispatch + dry_run=true: tag-format check skipped, other
#    gates still run. GITHUB_REF_NAME is a branch name, no v-prefix.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
  GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" >/dev/null 2>&1 \
  && pass "dispatch+dry_run=true skips tag check and passes" \
  || fail "dispatch+dry_run=true should pass on otherwise-clean state"

# 10. workflow_dispatch + dry_run=false: emergency-republish path emits
#     an explicit warning to stderr but does not fail on clean state.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="false" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  printf '%s' "$out" | grep -qi 'emergency-republish' \
    && pass "dispatch+dry_run=false emits emergency-republish warning" \
    || fail "dispatch+dry_run=false missing emergency-republish warning; got: $out"
else
  fail "dispatch+dry_run=false should not fail on otherwise-clean state; got: $out"
fi

# 11. workflow_dispatch + dry_run=true + crate metadata broken: still fails.
#     Non-tag gates must keep running on dispatch.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
ENG="$REPO_ROOT/src/rust/crates/fathomdb-engine/Cargo.toml"
sed -i '/^description[[:space:]]*=/d' "$ENG"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  cp "$SNAP_CRATES/fathomdb-engine.toml" "$ENG"
  fail "dispatch+broken metadata should still fail crate-metadata check"
else
  cp "$SNAP_CRATES/fathomdb-engine.toml" "$ENG"
  printf '%s' "$out" | grep -qi 'description' \
    && pass "dispatch keeps non-tag gates enforced" \
    || fail "wrong diagnostic on dispatch metadata break; got: $out"
fi

# 12. RC version (hyphen in WS_VERSION) + non-existent main ref:
#     HEAD-on-main check is SKIPPED per HITL 2026-05-17. Gate emits a
#     NOTE to stderr but does not die. GA tags still enforce (covered by
#     test #8).
restore
# Strip any existing pre-release suffix from $WS before appending -rc.1 so
# the synthetic version stays well-formed even when run against a tree
# already on a hyphenated version (e.g. 0.6.0-rc.1).
WS_BASE="${WS%%-*}"
RC_VERSION="${WS_BASE}-rc.1"
bash "$REPO_ROOT/scripts/set-version.sh" --workspace "$RC_VERSION"
printf '# Changelog\n\n## %s\n' "$RC_VERSION" > "$CL_PATH"
if out="$(RELEASE_GATES_SKIP_GIT_REACH=0 RELEASE_GATES_HEAD_REF="refs/heads/__nonexistent__" \
    GITHUB_REF_NAME="v${RC_VERSION}" "$VRG" 2>&1)"; then
  printf '%s' "$out" | grep -qiE 'release candidate|RC' \
    && pass "RC version (${RC_VERSION}) skips HEAD-on-main with NOTE" \
    || fail "RC version should emit RC-skip NOTE; got: $out"
else
  fail "RC version should pass gates with bogus main ref; got: $out"
fi
restore

restore

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll verify-release-gates tests passed\n'
