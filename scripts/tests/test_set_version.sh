#!/usr/bin/env bash
# scripts/tests/test_set_version.sh — coverage for set-version.sh
# two-axis enforcement per dev/design/release.md § Version axes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SV="$REPO_ROOT/scripts/set-version.sh"

CARGO="$REPO_ROOT/Cargo.toml"
PYPROJ="$REPO_ROOT/src/python/pyproject.toml"
NPMPKG="$REPO_ROOT/src/ts/package.json"
NPMLOCK="$REPO_ROOT/src/ts/package-lock.json"
PYRUNTIME="$REPO_ROOT/src/python/fathomdb/__init__.py"
CARGOLOCK="$REPO_ROOT/Cargo.lock"
EMBAPI="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api/Cargo.toml"
# napi per-platform binary packages (R-REL-4f) — set-version.sh keeps their
# `version` in lockstep with Axis W, so the test must snapshot/restore them too.
NPM_PLATFORM_DIR="$REPO_ROOT/src/ts/npm"

FAILED=0
TMPDIR_ROOT="$(mktemp -d)"
trap 'restore; rm -rf "$TMPDIR_ROOT"' EXIT

# Snapshot the four manifests so the test never leaves the tree dirty.
SNAP="$TMPDIR_ROOT/snap"
mkdir -p "$SNAP"
cp "$CARGO" "$SNAP/Cargo.toml"
cp "$PYPROJ" "$SNAP/pyproject.toml"
cp "$NPMPKG" "$SNAP/package.json"
cp "$NPMLOCK" "$SNAP/package-lock.json"
cp "$PYRUNTIME" "$SNAP/python-init.py"
cp "$CARGOLOCK" "$SNAP/Cargo.lock"
cp "$EMBAPI" "$SNAP/embedder-api.toml"
if [ -d "$NPM_PLATFORM_DIR" ]; then
  cp -r "$NPM_PLATFORM_DIR" "$SNAP/npm"
fi

restore() {
  cp "$SNAP/Cargo.toml" "$CARGO" 2>/dev/null || true
  cp "$SNAP/pyproject.toml" "$PYPROJ" 2>/dev/null || true
  cp "$SNAP/package.json" "$NPMPKG" 2>/dev/null || true
  cp "$SNAP/package-lock.json" "$NPMLOCK" 2>/dev/null || true
  cp "$SNAP/python-init.py" "$PYRUNTIME" 2>/dev/null || true
  cp "$SNAP/Cargo.lock" "$CARGOLOCK" 2>/dev/null || true
  cp "$SNAP/embedder-api.toml" "$EMBAPI" 2>/dev/null || true
  if [ -d "$SNAP/npm" ]; then
    cp -r "$SNAP/npm/." "$NPM_PLATFORM_DIR/" 2>/dev/null || true
  fi
}

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Extract Axis W workspace version from Cargo.toml [workspace.package].
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

# Extract Axis E embedder-api version from its [package] block.
emb_version() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$EMBAPI"
}

py_version() {
  awk '
    /^\[project\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$PYPROJ"
}

npm_version() {
  sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$NPMPKG" | head -1
}

npm_lock_top_version() {
  jq -r '.version // empty' "$NPMLOCK"
}

npm_lock_root_version() {
  jq -r '.packages[""].version // empty' "$NPMLOCK"
}

python_runtime_version() {
  sed -n 's/^__version__[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$PYRUNTIME"
}

cargo_lock_package_version() {
  local target="$1"
  awk -v target="$target" '
    BEGIN { RS = ""; FS = "\n" }
    {
      name = ""; version = ""
      for (i = 1; i <= NF; i++) {
        if ($i ~ /^name = "/) {
          name = $i
          sub(/^name = "/, "", name)
          sub(/"$/, "", name)
        }
        if ($i ~ /^version = "/) {
          version = $i
          sub(/^version = "/, "", version)
          sub(/"$/, "", version)
        }
      }
      if (name == target) { print version }
    }
  ' "$CARGOLOCK"
}

# 1. --check-files clean tree → 0.
restore
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean tree exits 0"
else
  fail "check-files clean tree should exit 0"
fi

# 2. --workspace 9.9.9 updates all Axis W manifests; Axis E untouched.
restore
ORIG_EMB="$(emb_version)"
"$SV" --workspace 9.9.9 >/dev/null
if [ "$(ws_version)" = "9.9.9" ] && [ "$(py_version)" = "9.9.9" ] && [ "$(npm_version)" = "9.9.9" ]; then
  pass "workspace 9.9.9 updates Cargo+pyproject+package.json"
else
  fail "workspace 9.9.9 did not propagate: ws=$(ws_version) py=$(py_version) npm=$(npm_version)"
fi
if [ "$(emb_version)" = "$ORIG_EMB" ]; then
  pass "workspace 9.9.9 leaves Axis E ($ORIG_EMB) untouched"
else
  fail "workspace 9.9.9 mutated Axis E: $ORIG_EMB → $(emb_version)"
fi
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean after --workspace 9.9.9"
else
  fail "check-files should be clean after --workspace 9.9.9"
fi

# 2b. A release cut owns the runtime and lockfile version surfaces consumed by
# package tooling, not just the source manifests. A partial bump used to leave
# all of these at the old release while --check-files still passed.
restore
"$SV" --workspace 9.9.8 >/dev/null
surface_bad=0
for observed in "$(python_runtime_version)" "$(npm_lock_top_version)" "$(npm_lock_root_version)"; do
  [ "$observed" = "9.9.8" ] || surface_bad=1
done
for package in fathomdb fathomdb-cli fathomdb-embedder fathomdb-engine fathomdb-napi fathomdb-py fathomdb-query fathomdb-schema; do
  [ "$(cargo_lock_package_version "$package")" = "9.9.8" ] || surface_bad=1
done
if [ "$surface_bad" -eq 0 ]; then
  pass "workspace 9.9.8 updates Python runtime, npm lockfile, and Axis-W Cargo.lock packages"
else
  fail "workspace 9.9.8 left a runtime or lockfile version surface stale"
fi
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean after workspace runtime and lockfile update"
else
  fail "check-files should cover and accept updated runtime and lockfile surfaces"
fi

# 3. --embedder-api 8.8.8 updates only Axis E; Axis W untouched.
restore
ORIG_WS="$(ws_version)"
ORIG_PY="$(py_version)"
ORIG_NPM="$(npm_version)"
"$SV" --embedder-api 8.8.8 >/dev/null
if [ "$(emb_version)" = "8.8.8" ]; then
  pass "embedder-api 8.8.8 updates Axis E"
else
  fail "embedder-api 8.8.8 did not update Axis E: $(emb_version)"
fi
if [ "$(ws_version)" = "$ORIG_WS" ] && [ "$(py_version)" = "$ORIG_PY" ] && [ "$(npm_version)" = "$ORIG_NPM" ]; then
  pass "embedder-api 8.8.8 leaves Axis W untouched"
else
  fail "embedder-api 8.8.8 leaked into Axis W"
fi
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean after --embedder-api 8.8.8"
else
  fail "check-files should be clean after --embedder-api 8.8.8"
fi

# Helper: assert that $out (a check-files stderr capture) contains a
# structured drift line for the given file naming observed/expected.
assert_drift_line() {
  local label="$1" out="$2" file_substr="$3" observed="$4" expected="$5"
  local missing=0
  printf '%s' "$out" | grep -qE "${file_substr}:[0-9]+:" || missing=1
  printf '%s' "$out" | grep -qF "observed \"${observed}\""        || missing=$((missing + 2))
  printf '%s' "$out" | grep -qF "expected \"${expected}\""        || missing=$((missing + 4))
  if [ "$missing" -eq 0 ]; then
    pass "$label"
  else
    fail "$label (missing bits=$missing); got: $out"
  fi
}

# 4a. Axis W drift on pyproject → structured diagnostic.
restore
WS_BEFORE="$(ws_version)"
sed -i 's/^version = "[^"]*"/version = "7.7.7"/' "$PYPROJ"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted pyproject"
else
  assert_drift_line "check-files: pyproject drift structured diagnostic" \
    "$out" "pyproject.toml" "7.7.7" "$WS_BEFORE"
fi

# 4b. Axis W drift on package.json → structured diagnostic.
restore
WS_BEFORE="$(ws_version)"
sed -i 's/"version"[[:space:]]*:[[:space:]]*"[^"]*"/"version": "7.7.7"/' "$NPMPKG"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted package.json"
else
  assert_drift_line "check-files: package.json drift structured diagnostic" \
    "$out" "package.json" "7.7.7" "$WS_BEFORE"
fi

# 4c. Runtime and lockfile surfaces are release contract, not manual post-cut
# chores. Each independently drifting surface must make --check-files red.
restore
WS_BEFORE="$(ws_version)"
sed -i 's/^__version__[[:space:]]*=[[:space:]]*"[^"]*"/__version__ = "7.7.7"/' "$PYRUNTIME"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted Python runtime version"
else
  assert_drift_line "check-files: Python runtime drift structured diagnostic" \
    "$out" "fathomdb/__init__.py" "7.7.7" "$WS_BEFORE"
fi

restore
WS_BEFORE="$(ws_version)"
node - "$NPMLOCK" <<'NODE'
const fs = require("fs");
const file = process.argv[2];
const lock = JSON.parse(fs.readFileSync(file, "utf8"));
lock.version = "7.7.7";
fs.writeFileSync(file, `${JSON.stringify(lock, null, 2)}\n`);
NODE
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted npm lockfile top-level version"
else
  assert_drift_line "check-files: npm lockfile drift structured diagnostic" \
    "$out" "package-lock.json" "7.7.7" "$WS_BEFORE"
fi

restore
WS_BEFORE="$(ws_version)"
python3 - "$CARGOLOCK" "$WS_BEFORE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old = f'name = "fathomdb"\nversion = "{sys.argv[2]}"'
new = 'name = "fathomdb"\nversion = "7.7.7"'
text = path.read_text()
if old not in text:
    raise SystemExit("fathomdb Cargo.lock package block not found")
path.write_text(text.replace(old, new, 1))
PY
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted Axis-W Cargo.lock package"
else
  assert_drift_line "check-files: Cargo.lock Axis-W drift structured diagnostic" \
    "$out" "Cargo.lock" "7.7.7" "$WS_BEFORE"
fi

# 5. Idempotent: --workspace <current> twice in a row → no second-pass change.
restore
CUR="$(ws_version)"
"$SV" --workspace "$CUR" >/dev/null
HASH1="$(sha256sum "$CARGO" "$PYPROJ" "$NPMPKG" | sha256sum)"
"$SV" --workspace "$CUR" >/dev/null
HASH2="$(sha256sum "$CARGO" "$PYPROJ" "$NPMPKG" | sha256sum)"
if [ "$HASH1" = "$HASH2" ]; then
  pass "workspace re-run is idempotent (hash stable)"
else
  fail "workspace re-run mutated content"
fi

# 6. Unknown flag → exit 2 with usage on stderr.
restore
if out="$("$SV" --bogus-flag 2>&1)"; then
  fail "unknown flag should exit non-zero"
else
  rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -q -i 'usage'; then
    pass "unknown flag → exit 2 + usage"
  else
    fail "unknown flag wrong exit ($rc) or no usage; got: $out"
  fi
fi

# 7. Missing argument to --workspace → exit 2 with usage.
restore
if out="$("$SV" --workspace 2>&1)"; then
  fail "missing arg should exit non-zero"
else
  rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -q -i 'usage'; then
    pass "missing arg → exit 2 + usage"
  else
    fail "missing arg wrong exit ($rc) or no usage; got: $out"
  fi
fi

# 8. Drift on Axis E (explicit version replaced with workspace inheritance) → fail.
# Pattern is version-agnostic so this test survives future Axis-E bumps without
# editing the sed literal each cut.
restore
sed -i -E 's/^version[[:space:]]*=[[:space:]]*"[^"]+"$/version.workspace = true/' "$EMBAPI"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail when Axis E inherits workspace"
else
  assert_drift_line "check-files: Axis E inheritance-regression structured diagnostic" \
    "$out" "fathomdb-embedder-api/Cargo.toml" \
    "version.workspace = true" "explicit [package] version"
fi

# 9. Drift on a non-embedder-api crate: replace `version.workspace = true`
#    with an explicit literal version → must be flagged with structured
#    diagnostic naming the crate manifest, observed=literal line,
#    expected=version.workspace = true.
restore
VICTIM=""
for c in "$REPO_ROOT"/src/rust/crates/*/Cargo.toml; do
  case "$c" in
    *fathomdb-embedder-api*) continue ;;
  esac
  if grep -q '^version\.workspace[[:space:]]*=[[:space:]]*true' "$c"; then
    VICTIM="$c"
    break
  fi
done
if [ -z "$VICTIM" ]; then
  fail "test 9 setup: no non-embedder-api crate uses version.workspace=true"
else
  cp "$VICTIM" "$TMPDIR_ROOT/victim.snap"
  sed -i 's/^version\.workspace[[:space:]]*=[[:space:]]*true/version = "9.9.9"/' "$VICTIM"
  if out="$("$SV" --check-files 2>&1)"; then
    cp "$TMPDIR_ROOT/victim.snap" "$VICTIM"
    fail "check-files should fail when a non-embedder-api crate hardcodes version"
  else
    cp "$TMPDIR_ROOT/victim.snap" "$VICTIM"
    assert_drift_line "check-files: non-emb-api crate hardcoded-version structured diagnostic" \
      "$out" "Cargo.toml" 'version = "9.9.9"' "version.workspace = true"
  fi
fi

# 10. Workspace.dependencies pin drift on an Axis W entry → structured
#     diagnostic. cargo publish requires every in-workspace dep to carry
#     a version requirement, and the pin must match the axis.
restore
sed -i 's/^fathomdb-engine[[:space:]]*=.*$/fathomdb-engine       = { path = "src\/rust\/crates\/fathomdb-engine",       version = "5.5.5" }/' "$CARGO"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted workspace.dependencies pin"
else
  assert_drift_line "check-files: workspace.dependencies pin drift structured diagnostic" \
    "$out" "Cargo.toml" "5.5.5" "$(ws_version)"
fi

# 11. --embedder-api updates the workspace.dependencies pin for
#     fathomdb-embedder-api, not just the [package] block.
restore
"$SV" --embedder-api 3.3.3 >/dev/null
if grep -E '^fathomdb-embedder-api[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"3\.3\.3"' "$CARGO" >/dev/null; then
  pass "embedder-api 3.3.3 propagates to workspace.dependencies pin"
else
  fail "embedder-api 3.3.3 did not update workspace.dependencies pin"
fi

# 12. --workspace updates every Axis-W pin in workspace.dependencies,
#     and leaves the Axis-E pin (fathomdb-embedder-api) alone.
restore
"$SV" --embedder-api 2.2.2 >/dev/null
"$SV" --workspace 4.4.4 >/dev/null
# Axis W pins must all be 4.4.4.
bad=0
for name in fathomdb fathomdb-embedder fathomdb-engine fathomdb-query fathomdb-schema; do
  if ! grep -E "^${name}[[:space:]]*=.*version[[:space:]]*=[[:space:]]*\"4\.4\.4\"" "$CARGO" >/dev/null; then
    bad=1
    fail "workspace 4.4.4 did not propagate to workspace.dependencies pin for $name"
  fi
done
[ "$bad" -eq 0 ] && pass "workspace 4.4.4 propagated to every Axis-W workspace.dependencies pin"
# Axis E pin must still be 2.2.2.
if grep -E '^fathomdb-embedder-api[[:space:]]*=.*version[[:space:]]*=[[:space:]]*"2\.2\.2"' "$CARGO" >/dev/null; then
  pass "workspace 4.4.4 left Axis-E workspace.dependencies pin (2.2.2) alone"
else
  fail "workspace 4.4.4 mutated Axis-E workspace.dependencies pin"
fi

# 13. --workspace 0.6.1 (real release bump) preserves axis-E pin at 0.6.0
#     when BOTH axes start at the same 0.6.0 value.
#
#     This is the key regression sentinel: if set_workspace_dep_axis_w_versions()
#     loses its embedder-api guard, it would silently bump the workspace.dependencies
#     pin from 0.6.0 to 0.6.1 — indistinguishable from a correct bump unless you
#     assert the exact retained value (test #12 can't catch this because it pre-sets
#     axis-E to a distinct version before running --workspace).
#
#     Per dev/design/release.md § Version axes and ADR-0.6.0-embedder-protocol,
#     axis-E is independent and MUST NOT be touched by --workspace.
restore
AXIS_E_BEFORE="$(emb_version)"  # 0.6.0 on this branch
"$SV" --workspace 0.6.1 >/dev/null

# (a) Workspace version flipped to 0.6.1.
if [ "$(ws_version)" = "0.6.1" ]; then
  pass "axis-E-pin(13a): workspace version flipped to 0.6.1"
else
  fail "axis-E-pin(13a): workspace version not 0.6.1, got $(ws_version)"
fi

# (b) fathomdb-embedder-api/Cargo.toml [package].version unchanged at axis-E value.
if [ "$(emb_version)" = "$AXIS_E_BEFORE" ]; then
  pass "axis-E-pin(13b): embedder-api [package].version preserved ($AXIS_E_BEFORE)"
else
  fail "axis-E-pin(13b): embedder-api [package].version changed: $AXIS_E_BEFORE → $(emb_version)"
fi

# (c) workspace.dependencies pin for fathomdb-embedder-api stays at axis-E value.
if grep -E "^fathomdb-embedder-api[[:space:]]*=.*version[[:space:]]*=[[:space:]]*\"${AXIS_E_BEFORE}\"" "$CARGO" >/dev/null; then
  pass "axis-E-pin(13c): workspace.dependencies pin for fathomdb-embedder-api preserved ($AXIS_E_BEFORE)"
else
  ACTUAL_PIN="$(grep -E '^fathomdb-embedder-api[[:space:]]*=' "$CARGO" | sed -n 's/.*version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p')"
  fail "axis-E-pin(13c): workspace.dependencies pin changed: expected $AXIS_E_BEFORE, got $ACTUAL_PIN"
fi

# (d) Every non-embedder-api workspace crate still uses version.workspace = true
#     (guards against silent drift introduced by the bump pass).
bad13=0
for c13 in "$REPO_ROOT"/src/rust/crates/*/Cargo.toml; do
  case "$c13" in *fathomdb-embedder-api*) continue ;; esac
  vline13="$(awk '/^\[package\]/ { in_block = 1; next } /^\[/ { in_block = 0 } in_block && /^version/ { print; exit }' "$c13")"
  case "$vline13" in
    version.workspace[[:space:]]*=[[:space:]]*true) ;;
    *)
      fail "axis-E-pin(13d): $(basename "$(dirname "$c13")") has unexpected version line: $vline13"
      bad13=$((bad13 + 1))
      ;;
  esac
done
[ "$bad13" -eq 0 ] && pass "axis-E-pin(13d): all non-embedder-api crates use version.workspace = true"

# (e) --check-files exits 0 after the bump.
if "$SV" --check-files >/dev/null 2>&1; then
  pass "axis-E-pin(13e): check-files exits 0 after --workspace 0.6.1"
else
  fail "axis-E-pin(13e): check-files failed after --workspace 0.6.1"
fi

restore

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll set-version.sh tests passed\n'
