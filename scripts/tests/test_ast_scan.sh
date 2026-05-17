#!/usr/bin/env bash
# Self-test for AC-050a AST no-shim scanner (scripts/security/ast_scan.py).
# Drives clean + dirty fixtures per language, plus targeted sub-fixtures
# for the verb-reroute, crate-root, and block-comment rules.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SCAN="$REPO_ROOT/scripts/security/ast_scan.py"
FIX="$SCRIPT_DIR/fixtures/ast-shim"

fail() { echo "FAIL: $*" >&2; exit 1; }

run_clean() {
    local lang="$1" path="$2"
    if ! python3 "$SCAN" --language "$lang" --path "$path" --repo-root "$REPO_ROOT" >/dev/null 2>&1; then
        fail "$lang clean fixture ($path) must exit 0"
    fi
}

run_dirty() {
    local lang="$1" path="$2" expect_pattern="$3"
    local errf
    errf=$(mktemp)
    set +e
    python3 "$SCAN" --language "$lang" --path "$path" --repo-root "$REPO_ROOT" >/dev/null 2>"$errf"
    local rc=$?
    set -e
    if [ "$rc" -ne 1 ]; then
        cat "$errf" >&2
        rm -f "$errf"
        fail "$lang dirty fixture ($path) must exit 1, got $rc"
    fi
    if ! grep -qE "$expect_pattern" "$errf"; then
        cat "$errf" >&2
        rm -f "$errf"
        fail "$lang dirty fixture ($path): expected pattern '$expect_pattern' missing"
    fi
    rm -f "$errf"
}

# V05_VERBS must be non-empty — sentinel that the rule is enforced.
verb_count=$(python3 -c 'import sys; sys.path.insert(0, "'"$REPO_ROOT"'/scripts/security"); import ast_scan; print(len(ast_scan.V05_VERBS))')
if [ "$verb_count" -lt 1 ]; then
    fail "V05_VERBS is empty — verb-reroute rule is unenforced"
fi
echo "OK V05_VERBS populated ($verb_count entries)"

for lang in rust python ts; do
    run_clean "$lang" "$FIX/$lang/clean"
    echo "OK $lang clean"
    run_dirty "$lang" "$FIX/$lang/dirty" "(legacy_|compat_v0_5|allow\\(deprecated\\)|verb re-route)"
    echo "OK $lang dirty"
done

# Targeted sub-rules: each fixture lives in its own directory so the
# scanner's rglob finds exactly one file and the assertion is isolated.
run_dirty rust "$FIX/rust/crate-root" "rust-crate-root-allow-deprecated"
echo "OK rust crate-root rule (lib.rs isolated)"

run_dirty rust "$FIX/rust/verb-reroute" "rust-05x-verb-reroute"
echo "OK rust v05 verb-reroute"

run_dirty python "$FIX/python/verb-reroute" "python-05x-verb-reroute"
echo "OK python v05 verb-reroute"

run_dirty ts "$FIX/ts/verb-reroute" "ts-05x-verb-reroute"
echo "OK ts v05 verb-reroute"

# Block-comment stripping: banned names inside /* */ must NOT flag
# (asserted by the rust/clean directory passing — block_comment_safe.rs
# embeds banned names in /* */, /*! */, and /** */ doc comments).
# Real code OUTSIDE a block comment in a file that ALSO contains a
# block comment must still flag.
run_dirty rust "$FIX/rust/block-comment-real" "(legacy_|rust-public-symbol)"
echo "OK rust block-comment does not swallow real code"

# Nested block comments (Rust edition 2018+ allows /* /* */ */).
# Clean nested fixture rides in rust/clean/. Dirty nested fixture
# lives in its own dir so the assertion proves the depth-counter
# (vs boolean) state machine: outer closes only after BOTH */ tokens,
# leaving `pub fn legacy_admin` as live code that MUST flag.
run_dirty rust "$FIX/rust/nested-block-real" "(legacy_admin|rust-public-symbol)"
echo "OK rust nested block-comment (depth counter)"

echo "test_ast_scan.sh: all language fixtures pass"
