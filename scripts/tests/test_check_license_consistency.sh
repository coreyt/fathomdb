#!/usr/bin/env bash
# scripts/tests/test_check_license_consistency.sh — coverage for the license
# type + license-SHIPPING gate (scripts/check-license-consistency.sh),
# 0.8.20 Slice 39 (R-20-DOC, HITL license ruling seq-193).
#
# The incident this closes: through the whole 0.8.x line the repo-root LICENSE
# said MIT while all four publishable manifests said Apache-2.0, and NO
# published artifact carried a license file at all — measured with
# `cargo package --list` (7 crates, zero license files) and `npm pack
# --dry-run` (83 entries, no LICENSE). crates.io versions are IMMUTABLE, so
# publishing that would have been unfixable rather than merely wrong.
#
# RED-FIRST, because the real tree is GREEN now. Asserting only against the
# real repo would prove nothing — `exit 0` would pass it. Every failure arm
# below therefore runs against a purpose-built BROKEN fixture root, so an arm
# can only go green because the predicate actually fired, and each arm greps
# for the SPECIFIC message so a different failure cannot be mistaken for the
# one under test.
#
# THE PACKAGING HALF IS PROVEN SEPARATELY (arms 16-18). The declaration half
# and the packaging half are deliberately coupled in the real tree — breaking
# the mechanism trips both — so a fixture is the only way to show the packaging
# comparison is reading real tool output rather than restating the config:
#   * arm 16 builds an npm package with NO license file and asserts the
#     packaging-specific message fires off `npm pack --dry-run --json`;
#   * arm 17 asserts the real-repo run actually EMITS an `ok crate ... ships
#     LICENSE` line for every publishable crate (i.e. cargo really ran);
#   * arm 18 asserts `--skip-packaging` suppresses exactly those lines, so the
#     flag is real and arm 17's evidence is not printed unconditionally.
#
# Isolation: fixtures are plain directories under mktemp -d (the checker takes
# --root for exactly this reason). No real manifest, no real LICENSE and no
# real lockfile is ever written by this suite.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-license-consistency.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

run_checker() {
  set +e
  OUT="$(bash "$CHECKER" "$@" 2>&1)"
  RC=$?
  set -e
}

# expect <rc> <needle> <label> -- asserts BOTH the exit code and that the
# specific diagnostic fired. Exit code alone is not enough: an arm that breaks
# two things at once would pass on the wrong one.
expect() {
  local want_rc="$1" needle="$2" label="$3"
  if [ "$RC" != "$want_rc" ]; then
    fail "$label — expected rc=$want_rc, got rc=$RC. Output: $OUT"
    return
  fi
  if [ -n "$needle" ] && ! printf '%s' "$OUT" | grep -qF -- "$needle"; then
    fail "$label — rc=$want_rc as expected but the diagnostic %$needle% never fired. Output: $OUT"
    return
  fi
  pass "$label (rc=$RC)"
}

# ------------------------------------------------------------- fixtures ------
# mkfix <name> -> prints a fixture root that the checker passes cleanly.
# Everything is minimal-but-real: real MIT text, a real one-member cargo
# workspace, a real pyproject, real package.json/package-lock.json.
mkfix() {
  local d="$TMPROOT/$1"
  mkdir -p "$d/crates/foo/src" "$d/src/python" "$d/src/ts/npm/plat"

  cp "$REPO_ROOT/LICENSE" "$d/LICENSE"

  cat >"$d/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/foo"]
resolver = "2"

[workspace.package]
version = "0.0.1"
edition = "2021"
license = "MIT"
license-file = "LICENSE"
EOF

  cat >"$d/crates/foo/Cargo.toml" <<'EOF'
[package]
name = "foo"
version.workspace = true
edition.workspace = true
license.workspace = true
license-file.workspace = true
EOF
  printf 'pub fn foo() {}\n' >"$d/crates/foo/src/lib.rs"

  cat >"$d/src/python/pyproject.toml" <<'EOF'
[project]
name = "foo"
version = "0.0.1"
license = "MIT"
license-files = ["LICENSE"]
EOF
  cp "$REPO_ROOT/LICENSE" "$d/src/python/LICENSE"

  cat >"$d/src/ts/package.json" <<'EOF'
{
  "name": "foo",
  "version": "0.0.1",
  "license": "MIT",
  "files": ["dist"]
}
EOF
  cp "$REPO_ROOT/LICENSE" "$d/src/ts/LICENSE"

  cat >"$d/src/ts/package-lock.json" <<'EOF'
{
  "name": "foo",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "foo", "version": "0.0.1", "license": "MIT" },
    "node_modules/typescript": { "version": "6.0.3", "license": "Apache-2.0" }
  }
}
EOF

  cat >"$d/src/ts/npm/plat/package.json" <<'EOF'
{
  "name": "@foo/foo-plat",
  "version": "0.0.1",
  "license": "MIT",
  "files": ["foo.node"]
}
EOF
  cp "$REPO_ROOT/LICENSE" "$d/src/ts/npm/plat/LICENSE"

  printf '%s' "$d"
}

# sub <file> <from> <to>
sub() { python3 - "$1" "$2" "$3" <<'PY'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as fh:
    t = fh.read()
assert old in t, "fixture edit target %r not found in %s" % (old, path)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(t.replace(old, new, 1))
PY
}

# ===================== arm 1 — the fixture itself is GREEN ====================
# Without this the RED arms below prove nothing: they could all be failing for
# a reason baked into the fixture rather than the mutation under test.
FIX="$(mkfix green)"
run_checker --root "$FIX" --skip-packaging
expect 0 "check-license-consistency: OK (MIT)" "arm 1: an in-order fixture passes the declaration half"

# ===================== arms 2-11 — every declaration RED ======================
FIX="$(mkfix ws-license)"
sub "$FIX/Cargo.toml" 'license = "MIT"' 'license = "Apache-2.0"'
run_checker --root "$FIX" --skip-packaging
expect 1 "[workspace.package].license is 'Apache-2.0'" "arm 2: workspace license disagreeing with LICENSE fails"

FIX="$(mkfix ws-no-license-file)"
sub "$FIX/Cargo.toml" 'license-file = "LICENSE"' ''
run_checker --root "$FIX" --skip-packaging
expect 1 "has no \`license-file\`" "arm 3: a workspace with no license-file fails (no crate would ship the text)"

FIX="$(mkfix crate-no-license-file)"
sub "$FIX/crates/foo/Cargo.toml" 'license-file.workspace = true' ''
run_checker --root "$FIX" --skip-packaging
expect 1 "is PUBLISHABLE but declares no \`license-file\`" "arm 4: a publishable crate not opting in fails"

FIX="$(mkfix py-legacy-table)"
sub "$FIX/src/python/pyproject.toml" 'license = "MIT"' 'license = { text = "MIT" }'
run_checker --root "$FIX" --skip-packaging
expect 1 "legacy table form" "arm 5: the legacy pyproject license table (which ships no license text) fails"

FIX="$(mkfix py-no-license-files)"
sub "$FIX/src/python/pyproject.toml" 'license-files = ["LICENSE"]' ''
run_checker --root "$FIX" --skip-packaging
expect 1 "has no \`license-files\`" "arm 6: a pyproject with no license-files fails"

FIX="$(mkfix py-escaping-glob)"
sub "$FIX/src/python/pyproject.toml" 'license-files = ["LICENSE"]' 'license-files = ["../../LICENSE"]'
run_checker --root "$FIX" --skip-packaging
expect 1 "escapes the project root" "arm 7: a PEP-639-illegal ../ glob fails instead of quietly matching nothing"

FIX="$(mkfix ts-license)"
sub "$FIX/src/ts/package.json" '"license": "MIT"' '"license": "Apache-2.0"'
run_checker --root "$FIX" --skip-packaging
expect 1 "src/ts/package.json: .license is 'Apache-2.0'" "arm 8: the npm main package's license field is checked"

FIX="$(mkfix ts-license-symlink)"
rm "$FIX/src/ts/LICENSE"
ln -s ../../LICENSE "$FIX/src/ts/LICENSE"
run_checker --root "$FIX" --skip-packaging
expect 1 "is a SYMLINK" "arm 9: a SYMLINKED npm LICENSE fails (measured: npm pack silently ships nothing)"

FIX="$(mkfix ts-license-drift)"
printf 'MIT License\n\nsomething else entirely\n' >"$FIX/src/ts/LICENSE"
run_checker --root "$FIX" --skip-packaging
expect 1 "has DRIFTED from the authoritative" "arm 10: a drifted LICENSE copy fails (the cost of copies, and its mitigation)"

FIX="$(mkfix lock-root)"
sub "$FIX/src/ts/package-lock.json" '"name": "foo", "version": "0.0.1", "license": "MIT"' '"name": "foo", "version": "0.0.1", "license": "Apache-2.0"'
run_checker --root "$FIX" --skip-packaging
expect 1 "the ROOT package entry, NOT a dependency" "arm 11: the lockfile ROOT entry is checked"

# The other half of arm 11: a DEPENDENCY's own recorded licence is a factual
# record. The fixture lockfile records node_modules/typescript as Apache-2.0
# (it really is). A gate that flagged that would push someone toward the global
# find-and-replace that corrupts the lockfile into lying about third parties.
FIX="$(mkfix lock-dep-untouched)"
run_checker --root "$FIX" --skip-packaging
expect 0 "" "arm 12: an Apache-2.0 DEPENDENCY entry in the lockfile is left alone (factual record)"

FIX="$(mkfix plat-license)"
sub "$FIX/src/ts/npm/plat/package.json" '"license": "MIT"' '"license": "Apache-2.0"'
run_checker --root "$FIX" --skip-packaging
expect 1 "npm/plat/package.json: .license is 'Apache-2.0'" "arm 13: every per-platform npm package is discovered and checked"

# ===================== arms 14-15 — the vacuous-pass guard ====================
# TC-37: a gate that cannot see its subject must NEVER report green.
FIX="$(mkfix no-license)"
rm "$FIX/LICENSE"
run_checker --root "$FIX" --skip-packaging
expect 2 "is missing or is not a regular file" "arm 14: a MISSING root LICENSE is exit 2, never a pass"

FIX="$(mkfix unknown-license)"
printf 'Some Bespoke Licence\n\nblah\n' >"$FIX/LICENSE"
run_checker --root "$FIX" --skip-packaging
expect 2 "cannot determine the license type" "arm 15: an unrecognised LICENSE type is exit 2, never a pass"

run_checker --root "$TMPROOT" --only bogus --skip-packaging
expect 2 "--only accepts cargo,python,npm" "arm 15b: a bad --only is a usage error, not a silent narrowing"

# ============ arm 16 — the PACKAGING half fires off REAL tool output ==========
# npm is the affordable half to prove this with: offline, sub-second, and it
# genuinely omits a license file when there is none to include. The fixture has
# a valid MIT package.json but NO license file at all, so `npm pack --dry-run
# --json` really does return a file list without one.
if command -v npm >/dev/null 2>&1; then
  FIX="$(mkfix npm-packaging-red)"
  rm "$FIX/src/ts/LICENSE" "$FIX/src/ts/npm/plat/LICENSE"
  printf '{}\n' >"$FIX/src/ts/index.js"
  run_checker --root "$FIX" --only npm
  expect 1 "file list does NOT contain LICENSE" "arm 16: the packaging assertion fires on a REAL npm pack file list"
else
  printf 'SKIP  arm 16: npm not on PATH\n'
fi

# ============ arms 17-18 — the packaging half really RUNS on the repo =========
# Arm 17 is the regression half: the real tree must pass, AND the run must
# visibly emit one `ok crate ... ships LICENSE` line per publishable crate. The
# count is derived from the workspace manifest, not hardcoded, so adding a
# publishable crate cannot quietly shrink the evidence.
EXPECTED_CRATES="$(python3 - "$REPO_ROOT" <<'PY'
import os, sys, tomllib
root = sys.argv[1]
with open(os.path.join(root, "Cargo.toml"), "rb") as fh:
    ws = tomllib.load(fh)
n = 0
for m in ws["workspace"]["members"]:
    with open(os.path.join(root, m, "Cargo.toml"), "rb") as fh:
        pkg = tomllib.load(fh).get("package", {})
    if pkg.get("publish") is not False:
        n += 1
print(n)
PY
)"

run_checker
if [ "$RC" != 0 ]; then
  fail "arm 17: the REAL repo must pass the full check (rc=$RC). Output: $OUT"
else
  GOT="$(printf '%s' "$OUT" | grep -c 'ok  crate .* ships ' || true)"
  if [ "$GOT" = "$EXPECTED_CRATES" ]; then
    pass "arm 17: the real repo passes AND cargo really ran for all $GOT publishable crates"
  else
    fail "arm 17: expected $EXPECTED_CRATES 'ok crate ... ships' lines (one per publishable crate), saw $GOT. Output: $OUT"
  fi
fi

run_checker --skip-packaging
if [ "$RC" != 0 ]; then
  fail "arm 18: --skip-packaging must still pass on the real repo (rc=$RC). Output: $OUT"
elif printf '%s' "$OUT" | grep -q 'ok  crate .* ships '; then
  fail "arm 18: --skip-packaging still emitted packaging evidence — the flag is not real, which would make arm 17's evidence unconditional. Output: $OUT"
else
  pass "arm 18: --skip-packaging really suppresses the packaging half (so arm 17's evidence is earned)"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-license-consistency tests passed\n'
