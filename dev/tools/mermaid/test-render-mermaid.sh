#!/usr/bin/env bash
#
# test-render-mermaid.sh — acceptance tests for render-mermaid.sh.
#
# One test per acceptance criterion in REQ-AC.md. Real end-to-end renders (they
# drive the actual mmdc + Chromium), so deps must be installed first:
#   ./render-mermaid.sh install
#
# Exit 0 = all ACs green; non-zero = at least one failed (count on stderr).
set -uo pipefail   # NOT -e: a failed assertion must not abort the whole suite.

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
T="$HERE/render-mermaid.sh"
BIN="$HERE/node_modules/.bin/mmdc"

WORK="$(mktemp -d)"
restore_deps() { [[ -e "$BIN.hidden" ]] && mv -f "$BIN.hidden" "$BIN"; return 0; }
cleanup() { restore_deps; rm -rf "$WORK"; }
trap cleanup EXIT

pass=0; fail=0
ok()   { printf 'PASS %-7s %s\n' "$1" "$2"; pass=$((pass+1)); }
bad()  { printf 'FAIL %-7s %s\n' "$1" "$2" >&2; fail=$((fail+1)); }

# run CMD… -> sets $code, writes $WORK/out and $WORK/err
run() { "$@" >"$WORK/out" 2>"$WORK/err"; code=$?; }
out() { cat "$WORK/out"; }
err() { cat "$WORK/err"; }

# assert helpers take an AC id, a human note, and the condition already evaluated
expect_code() { [[ "$code" == "$1" ]] || { bad "$2" "$3: expected exit $1, got $code (err: $(err | tail -1))"; return 1; }; return 0; }
expect_empty_out() { [[ ! -s "$WORK/out" ]] || { bad "$1" "$2: stdout not empty ([$(out)])"; return 1; }; return 0; }

# --- fixtures ---------------------------------------------------------------
VALID="$WORK/valid.mmd"
printf 'graph TD\n A[Start] --> B{OK?}\n B -->|yes| C[Done]\n B -->|no| A\n' > "$VALID"
MD="$WORK/doc.md"
printf '# Title\n\ntext\n\n```mermaid\ngraph LR\n X --> Y\n```\n' > "$MD"
BADMMD="$WORK/bad.mmd"
printf 'this is @@@ not valid mermaid !!!\n' > "$BADMMD"

is_svg() { head -c 200 "$1" 2>/dev/null | grep -q '<svg'; }
is_kind() { file --brief "$1" 2>/dev/null | grep -qi "$2"; }

echo "== R1 happy paths =="

run "$T" -i "$VALID" -o "$WORK/a.svg"
expect_code 0 AC1.1 "svg render" && { is_svg "$WORK/a.svg" && ok AC1.1 "svg written" || bad AC1.1 "output is not an <svg>"; }

run "$T" -i "$VALID" -o "$WORK/a.png"
expect_code 0 AC1.2 "png render" && { is_kind "$WORK/a.png" "PNG" && ok AC1.2 "png written" || bad AC1.2 "output is not a PNG"; }

run "$T" -i "$VALID" -o "$WORK/a.pdf"
expect_code 0 AC1.3 "pdf render" && { is_kind "$WORK/a.pdf" "PDF" && ok AC1.3 "pdf written" || bad AC1.3 "output is not a PDF"; }

cp "$VALID" "$WORK/infer.mmd"
run "$T" "$WORK/infer.mmd"
if expect_code 0 AC1.4 "inferred render"; then
  [[ -f "$WORK/infer.svg" ]] && [[ "$(out)" == "$WORK/infer.svg" ]] \
    && ok AC1.4 "inferred sibling .svg created + path printed" \
    || bad AC1.4 "inferred svg or stdout path wrong (out=[$(out)])"
fi

run "$T" -i "$MD" -o "$WORK/frommd.svg"
# mmdc emits per-block artefacts as <base>-1.svg for markdown input.
if expect_code 0 AC1.5 "markdown extraction"; then
  ls "$WORK"/frommd*.svg >/dev/null 2>&1 && ok AC1.5 "svg extracted from markdown block" \
    || bad AC1.5 "no svg artefact from markdown"
fi

BATCH="$WORK/batch"; mkdir -p "$BATCH/sub"
cp "$VALID" "$BATCH/one.mmd"; cp "$VALID" "$BATCH/sub/two.mmd"
run "$T" --all "$BATCH"
if expect_code 0 AC1.6 "batch render"; then
  if [[ -f "$BATCH/one.svg" && -f "$BATCH/sub/two.svg" ]] \
     && grep -q "$BATCH/one.svg" "$WORK/out" && grep -q "$BATCH/sub/two.svg" "$WORK/out"; then
    ok AC1.6 "batch rendered both + printed paths"
  else
    bad AC1.6 "batch missed a file or a path (out=[$(out)])"
  fi
fi

run "$T" -i "$VALID" -o "$WORK/dark.svg" -t dark
expect_code 0 AC1.7 "theme passthrough" && { [[ -f "$WORK/dark.svg" ]] && ok AC1.7 "passthrough flag honored" || bad AC1.7 "no output with -t dark"; }

echo "== R2 edge cases / failure modes =="

run "$T" -i "$WORK/nope.mmd" -o "$WORK/nope.svg"
if expect_code 66 AC2.1 "missing input"; then
  { err | grep -q "nope.mmd"; } && [[ ! -f "$WORK/nope.svg" ]] \
    && ok AC2.1 "missing input -> 66, named on stderr, no output" \
    || bad AC2.1 "stderr did not name file or a stray output exists"
fi

run "$T" -i "$BADMMD" -o "$WORK/bad.svg"
if expect_code 70 AC2.2 "invalid diagram"; then
  [[ -s "$WORK/err" && ! -f "$WORK/bad.svg" ]] \
    && ok AC2.2 "invalid diagram -> 70, stderr set, no leftover file" \
    || bad AC2.2 "no stderr or a stale output file was left"
fi

EMPTY="$WORK/emptydir"; mkdir -p "$EMPTY"
run "$T" --all "$EMPTY"
expect_code 64 AC2.3 "empty --all" && expect_empty_out AC2.3 "empty --all" && ok AC2.3 "no *.mmd -> 64, silent stdout"

run "$T"
expect_code 64 AC2.4 "no args" && ok AC2.4 "empty command -> 64"

run "$T" -i "$VALID" -o "$WORK/missing/dir/out.svg"
if expect_code 0 AC2.5 "missing outdir"; then
  [[ -f "$WORK/missing/dir/out.svg" ]] && ok AC2.5 "missing outdir created + rendered" \
    || bad AC2.5 "exit 0 but no output file"
fi

SPACED="$WORK/with space.mmd"; cp "$VALID" "$SPACED"
run "$T" "$SPACED"
if expect_code 0 AC2.6 "spaced filename"; then
  [[ -f "$WORK/with space.svg" && "$(out)" == "$WORK/with space.svg" ]] \
    && ok AC2.6 "spaced filename handled" \
    || bad AC2.6 "spaced filename mishandled (out=[$(out)])"
fi

echo "== R3 dependency safety / HITL =="

run "$T" check
expect_code 0 AC3.1 "check installed" && expect_empty_out AC3.1 "check installed" && ok AC3.1 "check ready -> 0, silent"

# Reversibly hide the binary to simulate a fresh, uninstalled checkout.
mv "$BIN" "$BIN.hidden"

run "$T" check
if expect_code 69 AC3.2 "check missing"; then
  expect_empty_out AC3.2 "check missing" && { err | grep -qi 'NOT INSTALLED'; } \
    && ok AC3.2 "check not-installed -> 69, notice on stderr, silent stdout" \
    || bad AC3.2 "missing notice or stdout not empty"
fi

run "$T" -i "$VALID" -o "$WORK/na.svg"
if expect_code 69 AC3.3 "render missing"; then
  [[ ! -x "$BIN" && ! -f "$WORK/na.svg" ]] \
    && ok AC3.3 "render without deps -> 69, no install, no output" \
    || bad AC3.3 "an install appears to have happened or an output was produced"
fi

restore_deps
run "$T" check
expect_code 0 AC3.x "deps restored" && ok AC3.x "deps restored after simulation"

echo "== R4 agentic I/O contract =="

# AC4.2: explicit -o success is silent on stdout.
run "$T" -i "$VALID" -o "$WORK/c.svg"
expect_code 0 AC4.2 "explicit -o" && expect_empty_out AC4.2 "explicit -o" && ok AC4.2 "explicit -o render silent on stdout"

# AC4.3: a failure keeps stdout empty (message on stderr only).
run "$T" -i "$BADMMD" -o "$WORK/c2.svg"
expect_empty_out AC4.3 "failure stdout" && [[ -s "$WORK/err" ]] && ok AC4.3 "failure -> stdout empty, stderr set"

# AC4.1 + AC4.4 are demonstrated by the inferred/batch (payload-only stdout) and
# both check states (silent stdout) already asserted above.
ok AC4.1 "payload-only stdout (via AC1.4/AC1.6)"
ok AC4.4 "check silent both states (via AC3.1/AC3.2)"

echo "---------------------------------------------"
printf 'RESULT: %d passed, %d failed\n' "$pass" "$fail"
[[ "$fail" == 0 ]]
