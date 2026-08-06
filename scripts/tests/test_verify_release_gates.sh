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

FAILED=0
TMPDIR_ROOT="$(mktemp -d)"
SNAP="$TMPDIR_ROOT/snap"
mkdir -p "$SNAP"
TEMP_TAGS=()

cleanup_temp_tags() {
  for tag in "${TEMP_TAGS[@]}"; do
    git -C "$REPO_ROOT" tag -d "$tag" >/dev/null 2>&1 || true
  done
}

trap 'cleanup_temp_tags; restore; rm -rf "$TMPDIR_ROOT"' EXIT

# set-version.sh owns all Axis-W manifests, runtimes, lockfiles, and platform
# packages.  This fixture deliberately drives synthetic versions through that
# script, so snapshot the full owned surface rather than restoring a stale
# subset and corrupting later release-gate cases.
SNAP_VERSIONED=(
  Cargo.toml
  Cargo.lock
  src/python/pyproject.toml
  src/python/fathomdb/__init__.py
  src/ts/package.json
  src/ts/package-lock.json
  src/ts/npm
  src/rust/crates/fathomdb-engine/Cargo.toml
)
for rel in "${SNAP_VERSIONED[@]}"; do
  if [ -e "$REPO_ROOT/$rel" ]; then
    mkdir -p "$SNAP/$(dirname "$rel")"
    cp -R "$REPO_ROOT/$rel" "$SNAP/$rel"
  fi
done

restore() {
  for rel in "${SNAP_VERSIONED[@]}"; do
    if [ -e "$SNAP/$rel" ]; then
      if [ -d "$SNAP/$rel" ]; then
        cp -R "$SNAP/$rel/." "$REPO_ROOT/$rel/" 2>/dev/null || true
      else
        cp "$SNAP/$rel" "$REPO_ROOT/$rel" 2>/dev/null || true
      fi
    fi
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
CANDIDATE_SHA="$(git -C "$REPO_ROOT" rev-parse HEAD)"

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
  restore
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
  restore
  fail "missing description should fail crate-metadata check"
else
  restore
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

# 9. workflow_dispatch + dry_run=true accepts an immutable full candidate SHA.
#    The prospective tag still has to match Axis W, but the tag need not exist.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
  RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
  RELEASE_GATES_CANDIDATE_COMMIT="$CANDIDATE_SHA" \
  GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" >/dev/null 2>&1 \
  && pass "dispatch+dry_run=true validates its immutable candidate and prospective tag" \
  || fail "dispatch+dry_run=true should pass with a matching candidate SHA"

# 9a. A dry run must not silently fall back to a branch tip or an uncreated tag.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
    RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  fail "dispatch+dry_run=true without an immutable candidate SHA should fail"
else
  printf '%s' "$out" | grep -qi 'candidate' \
    && pass "dry-run dispatch requires an immutable candidate SHA" \
    || fail "wrong diagnostic for missing dry-run candidate; got: $out"
fi

# 9b. The full SHA must resolve and be the checked-out commit, not merely look
# like one. Forty zeroes has the right shape but cannot be selected safely.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
    RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
    RELEASE_GATES_CANDIDATE_COMMIT="0000000000000000000000000000000000000000" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  fail "dispatch+dry_run=true with an unresolved candidate SHA should fail"
else
  printf '%s' "$out" | grep -qi 'candidate' \
    && pass "dry-run dispatch rejects an unresolved candidate SHA" \
    || fail "wrong diagnostic for unresolved dry-run candidate; got: $out"
fi

# 10. workflow_dispatch rejects a non-semver release input before a dry run
#     can exercise any publisher.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
    RELEASE_DISPATCH_VERSION="not-semver" RELEASE_GATES_TAG="vnot-semver" \
    RELEASE_GATES_CANDIDATE_COMMIT="$CANDIDATE_SHA" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  fail "dispatch with a non-semver release version should fail; got: $out"
else
  printf '%s' "$out" | grep -qi 'semver' \
    && pass "dispatch rejects a non-semver release version" \
    || fail "wrong diagnostic for a non-semver dispatch version; got: $out"
fi

# 11. workflow_dispatch rejects a tag that is not exactly v<release_version>.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
    RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v9.9.9" \
    RELEASE_GATES_CANDIDATE_COMMIT="$CANDIDATE_SHA" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  fail "dispatch with a mismatched derived tag should fail; got: $out"
else
  printf '%s' "$out" | grep -qiE 'tag.*mismatch|canonical' \
    && pass "dispatch rejects a mismatched derived tag" \
    || fail "wrong diagnostic for a mismatched dispatch tag; got: $out"
fi

# 12. workflow_dispatch + dry_run=false without the typed confirmation must
#     fail before the emergency-republish path can proceed.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="false" \
    RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  fail "dispatch+dry_run=false without confirmation should fail; got: $out"
else
  printf '%s' "$out" | grep -qi 'confirm' \
    && pass "dispatch+dry_run=false without confirmation rejected" \
    || fail "wrong diagnostic for missing dispatch confirmation; got: $out"
fi

# 13. workflow_dispatch + dry_run=false with a mismatched typed confirmation
#     must fail rather than accepting any non-empty second factor.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="false" \
    RELEASE_CONFIRM_VERSION="9.9.9" RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  fail "dispatch+dry_run=false with mismatched confirmation should fail; got: $out"
else
  printf '%s' "$out" | grep -qi 'confirm' \
    && pass "dispatch+dry_run=false with mismatched confirmation rejected" \
    || fail "wrong diagnostic for mismatched dispatch confirmation; got: $out"
fi

# 14. workflow_dispatch + dry_run=false with matching typed confirmation
#     proceeds and retains its emergency-republish warning.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="false" \
    RELEASE_CONFIRM_VERSION="$WS" RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  printf '%s' "$out" | grep -qi 'emergency-republish' \
    && pass "dispatch+dry_run=false with matching confirmation emits emergency-republish warning" \
    || fail "dispatch+dry_run=false missing emergency-republish warning; got: $out"
else
  fail "dispatch+dry_run=false with matching confirmation should pass; got: $out"
fi

# 15. workflow_dispatch + dry_run=true + crate metadata broken: still fails.
#     Non-tag gates must keep running on dispatch.
restore
printf '# Changelog\n\n## %s\n' "$WS" > "$CL_PATH"
ENG="$REPO_ROOT/src/rust/crates/fathomdb-engine/Cargo.toml"
sed -i '/^description[[:space:]]*=/d' "$ENG"
if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="true" \
    RELEASE_DISPATCH_VERSION="$WS" RELEASE_GATES_TAG="v${WS}" \
    RELEASE_GATES_CANDIDATE_COMMIT="$CANDIDATE_SHA" \
    GITHUB_REF_NAME="phase-11d-release-workflow" "$VRG" 2>&1)"; then
  restore
  fail "dispatch+broken metadata should still fail crate-metadata check"
else
  restore
  printf '%s' "$out" | grep -qi 'description' \
    && pass "dispatch keeps non-tag gates enforced" \
    || fail "wrong diagnostic on dispatch metadata break; got: $out"
fi

# 16. A non-dry-run workflow_dispatch must be checked out at its canonical tag,
#     not the branch/ref selected in the GitHub UI. Exercise real git resolution:
#     a tag at HEAD passes, while a tag at HEAD^ fails before publishing. The
#     opt-out is only for restricted local worktrees whose shared git-dir is
#     deliberately read-only; CI always runs these controls.
if [ "${RELEASE_GATES_SKIP_TAG_CHECKOUT_CONTROL:-0}" = "1" ]; then
  pass "canonical-tag checkout controls skipped by explicit local harness override"
else
  CHECKOUT_TEST_ID="${BASHPID:-$$}"
  CHECKOUT_OK_VERSION="0.997.${CHECKOUT_TEST_ID}"
  CHECKOUT_OK_TAG="v${CHECKOUT_OK_VERSION}"
  restore
  bash "$REPO_ROOT/scripts/set-version.sh" --workspace "$CHECKOUT_OK_VERSION" >/dev/null
  printf '# Changelog\n\n## %s\n' "$CHECKOUT_OK_VERSION" > "$CL_PATH"
  git -C "$REPO_ROOT" tag "$CHECKOUT_OK_TAG" HEAD
  TEMP_TAGS+=("$CHECKOUT_OK_TAG")
  if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="false" \
      RELEASE_GATES_REQUIRE_TAG_CHECKOUT=1 \
      RELEASE_CONFIRM_VERSION="$CHECKOUT_OK_VERSION" \
      RELEASE_DISPATCH_VERSION="$CHECKOUT_OK_VERSION" RELEASE_GATES_TAG="$CHECKOUT_OK_TAG" \
      GITHUB_REF_NAME="main" "$VRG" 2>&1)"; then
    pass "dispatch accepts a checkout whose HEAD is the canonical tag"
  else
    fail "dispatch should accept a canonical-tag checkout; got: $out"
  fi

  CHECKOUT_BAD_VERSION="0.996.${CHECKOUT_TEST_ID}"
  CHECKOUT_BAD_TAG="v${CHECKOUT_BAD_VERSION}"
  restore
  bash "$REPO_ROOT/scripts/set-version.sh" --workspace "$CHECKOUT_BAD_VERSION" >/dev/null
  printf '# Changelog\n\n## %s\n' "$CHECKOUT_BAD_VERSION" > "$CL_PATH"
  git -C "$REPO_ROOT" tag "$CHECKOUT_BAD_TAG" HEAD^
  TEMP_TAGS+=("$CHECKOUT_BAD_TAG")
  if out="$(GITHUB_EVENT_NAME="workflow_dispatch" DRY_RUN="false" \
      RELEASE_GATES_REQUIRE_TAG_CHECKOUT=1 \
      RELEASE_CONFIRM_VERSION="$CHECKOUT_BAD_VERSION" \
      RELEASE_DISPATCH_VERSION="$CHECKOUT_BAD_VERSION" RELEASE_GATES_TAG="$CHECKOUT_BAD_TAG" \
      GITHUB_REF_NAME="main" "$VRG" 2>&1)"; then
    fail "dispatch must reject a UI-selected checkout that differs from its canonical tag; got: $out"
  else
    printf '%s' "$out" | grep -qi 'checkout mismatch' \
      && pass "dispatch rejects a checkout whose HEAD differs from the canonical tag" \
      || fail "wrong diagnostic for a dispatch checkout mismatch; got: $out"
  fi
  restore
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
