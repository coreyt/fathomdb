#!/usr/bin/env bash
# Self-test for AC-050a AST no-shim scanner (scripts/security/ast_scan.py).
# Drives clean + dirty fixtures per language.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SCAN="$REPO_ROOT/scripts/security/ast_scan.py"
FIX="$SCRIPT_DIR/fixtures/ast-shim"

fail() { echo "FAIL: $*" >&2; exit 1; }

for lang in rust python ts; do
    # Clean fixture must pass.
    if ! python3 "$SCAN" --language "$lang" --path "$FIX/$lang/clean" --repo-root "$REPO_ROOT" >/dev/null; then
        fail "$lang clean fixture must exit 0"
    fi
    echo "OK $lang clean"

    # Dirty fixture must fail with non-zero exit and a recognizable diagnostic.
    set +e
    python3 "$SCAN" --language "$lang" --path "$FIX/$lang/dirty" --repo-root "$REPO_ROOT" \
        >/dev/null 2>/tmp/ast_scan_dirty_$lang.err
    rc=$?
    set -e
    if [ "$rc" -ne 1 ]; then
        fail "$lang dirty fixture must exit 1, got $rc"
    fi
    if ! grep -qE "(legacy_|compat_v0_5|allow\(deprecated\))" /tmp/ast_scan_dirty_$lang.err; then
        fail "$lang dirty fixture: diagnostic must name a banned pattern"
    fi
    echo "OK $lang dirty"
done

echo "test_ast_scan.sh: all language fixtures pass"
