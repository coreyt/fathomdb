#!/usr/bin/env bash
# Cross-language SDK consistency test orchestrator.
#
# Runs deterministic scenarios through both the Python and TypeScript SDKs,
# then diffs normalized JSON manifests to prove data consistency.
#
# See tests/cross-language/README.md for the full design, scenario format,
# and instructions for adding new scenarios.
#
# Prerequisites:
#   - Python SDK installed (pip install -e python/ --no-build-isolation)
#   - Rust native binding built (cargo build -p fathomdb --features node)
#   - TypeScript workspace installed (cd typescript && npm install)
#   - Cross-language TS driver deps installed (cd tests/cross-language/typescript && npm install)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TMP=$(mktemp -d)
# Route all child processes through the same per-session temp root so
# cleanup is a single rm -rf (GH #40). Rust tempfile, Python pytest,
# and TypeScript os.tmpdir() all honor $TMPDIR.
export TMPDIR="$TMP"
trap 'rm -rf "$TMP"' EXIT

# Resolve native binding path for TypeScript.
# cargo build produces a .so/.dylib; Node require() needs a .node extension.
NATIVE_SO=""
for candidate in \
    "$REPO_ROOT/target/debug/libfathomdb.so" \
    "$REPO_ROOT/target/debug/libfathomdb.dylib" \
    "$REPO_ROOT/target/release/libfathomdb.so" \
    "$REPO_ROOT/target/release/libfathomdb.dylib"; do
    if [ -f "$candidate" ]; then
        NATIVE_SO="$candidate"
        break
    fi
done

if [ -z "$NATIVE_SO" ]; then
    echo "ERROR: Could not find native binding. Run: cargo build -p fathomdb --features node"
    exit 1
fi

# Copy to .node extension so Node's require() loads it as a native addon
NATIVE_NODE="$TMP/fathomdb.node"
cp "$NATIVE_SO" "$NATIVE_NODE"
export FATHOMDB_NATIVE_BINDING="$NATIVE_NODE"
echo "Using native binding: $NATIVE_SO -> $NATIVE_NODE"

VITE_NODE="$REPO_ROOT/typescript/node_modules/.bin/vite-node"
TS_DRIVER="$SCRIPT_DIR/typescript/driver.ts"
PY_DRIVER="$SCRIPT_DIR/python/driver.py"

passed=0
failed=0

compare() {
    local label="$1" file_a="$2" file_b="$3"
    if diff -u "$file_a" "$file_b" > "$TMP/diff-output.txt" 2>&1; then
        echo "  PASS: $label"
        passed=$((passed + 1))
    else
        echo "  FAIL: $label"
        head -40 "$TMP/diff-output.txt"
        failed=$((failed + 1))
    fi
}

# Suppress engine tracing warnings (e.g. vec_nodes_active on non-vector builds)
echo "=== Phase 1: Python writes + reads ==="
python "$PY_DRIVER" --db "$TMP/py.db" --mode write > "$TMP/py-wrote.json" 2>/dev/null

echo "=== Phase 2: TypeScript writes + reads ==="
"$VITE_NODE" "$TS_DRIVER" -- --db "$TMP/ts.db" --mode write > "$TMP/ts-wrote.json" 2>/dev/null

echo "=== Phase 3: Cross-read — TypeScript reads Python DB ==="
"$VITE_NODE" "$TS_DRIVER" -- --db "$TMP/py.db" --mode read > "$TMP/ts-read-py.json" 2>/dev/null

echo "=== Phase 4: Cross-read — Python reads TypeScript DB ==="
python "$PY_DRIVER" --db "$TMP/ts.db" --mode read > "$TMP/py-read-ts.json" 2>/dev/null

echo ""
echo "=== Comparing manifests ==="

# Same input → same state: both SDKs should produce identical query results
compare "Python vs TypeScript produce identical state" "$TMP/py-wrote.json" "$TMP/ts-wrote.json"

# Cross-read: TypeScript can correctly read what Python wrote
compare "TypeScript reads Python DB correctly" "$TMP/py-wrote.json" "$TMP/ts-read-py.json"

# Cross-read: Python can correctly read what TypeScript wrote
compare "Python reads TypeScript DB correctly" "$TMP/ts-wrote.json" "$TMP/py-read-ts.json"

echo ""
if [ "$failed" -gt 0 ]; then
    echo "RESULT: $passed passed, $failed failed"
    exit 1
else
    echo "RESULT: All $passed cross-language consistency checks passed."
fi
