#!/usr/bin/env bash
# scripts/tests/test_lint_plan_anchors.sh — T1d recurrence guard (DOC-HYGIENE-2).
#
# Proves scripts/lint-plan-anchors.sh:
#
#   RULE 1 — bare line-anchor ban
#     1.  FAILS an ACTIVE plan carrying a bare `<name>:<line>` anchor.
#     1b. Is GENERIC, not an enumerated crate-prefix list: a name nobody would
#         have thought to enumerate (`fathomdb-cli:389`, `wibble-frobnicator:4711`)
#         fails just the same. This is the arm that matters — an enumeration is
#         defeated by the next name anyone invents.
#     1c. Accepts a range (`lib.rs:876-887`) and an open end (`engine:12166+`).
#     1d. Does NOT fire on a colon inside ordinary code/prose
#         (`SHA256("{kind}:{name}")`) or on a sub-100 `:NN` section reference —
#         the documented floor.
#
#   RULE 2 — MANDATORY symbol-existence check (the crux of the tranche)
#     2.  FAILS a citation whose symbol does NOT occur in the file it names.
#         Without this arm the lint would only swap an unverified NUMBER for an
#         unverified SYMBOL — the same broken pointer with better cosmetics.
#     3.  PASSES a citation whose symbol DOES occur.
#     3b. FAILS a citation naming a file that does not exist.
#     3c. FAILS a citation whose path is AMBIGUOUS (resolves to >1 file).
#     3c1. Ignores an untracked nested worktree when resolving a citation.
#     3d. FAILS a citation containing an elision placeholder (`…`/`...`) — an
#         unverifiable citation is failed, never skipped.
#     3e. Is WRAP-AWARE: a bad citation split across a line break (with or
#         without a `>` blockquote marker) still FAILS. A checker a soft wrap
#         can defeat is a checker whose green means nothing.
#
#   SCOPE FENCE
#     4.  PASSES a NON-ACTIVE plan carrying a bare anchor (a COMPLETE plan is a
#         historical record; rewriting its anchors would falsify the record).
#     4b. Does not scan dev/plans/runs/** or dev/plans/prompts/** — immutable
#         run artifacts.
#
#   VACUOUS-PASS GUARD (TC-37)
#     5.  FAILS when ZERO ACTIVE plans are discovered.
#     5b. FAILS when ZERO citations are verified across the ACTIVE plans.
#
# Builds a throwaway fixture repo under mktemp -d; never touches this checkout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LINT="${LINT_UNDER_TEST:-$REPO_ROOT/scripts/lint-plan-anchors.sh}"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Every fixture gets: a real source file to cite, and one ACTIVE plan holding a
# valid citation — so the vacuity guards do not fire on arms that are testing
# something else.
setup_fixture() {
  rm -rf "$FIX/repo"
  mkdir -p "$FIX/repo/scripts" "$FIX/repo/dev/plans/runs" "$FIX/repo/dev/plans/prompts" \
           "$FIX/repo/src/rust/crates/widget/src"
  cp "$LINT" "$FIX/repo/scripts/lint-plan-anchors.sh"
  chmod +x "$FIX/repo/scripts/lint-plan-anchors.sh"
  cat >"$FIX/repo/src/rust/crates/widget/src/lib.rs" <<'EOF'
pub fn real_symbol() -> u32 { 7 }
fn another_real_symbol() {}
EOF
  (cd "$FIX/repo" && git init -q && git config user.email t@example.com && git config user.name t)
}

baseline_plan() {
  cat >"$FIX/repo/dev/plans/baseline.md" <<'EOF'
---
status: ACTIVE
---

# Baseline

The write path is `pub fn real_symbol` in `widget/src/lib.rs`.
EOF
}

run_lint() {
  set +e
  OUT="$(cd "$FIX/repo" && bash scripts/lint-plan-anchors.sh 2>&1)"
  RC=$?
  set -e
}

# --- Arm 1: bare anchor in an ACTIVE plan -> FAIL ------------------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/offender.md" <<'EOF'
---
status: ACTIVE
---

The dispatcher lives at `engine:11152` today.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q 'offender.md:5' <<<"$OUT" && grep -q 'engine:11152' <<<"$OUT"; then
  pass "bare anchor in an ACTIVE plan -> non-zero exit, names file:line and the anchor"
else
  fail "arm 1 (bare anchor): rc=$RC out=$OUT"
fi

# --- Arm 1b: the rule is a SHAPE, not an enumerated prefix list ----------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/novel.md" <<'EOF'
---
status: ACTIVE
---

See `fathomdb-cli:389` and `wibble_frobnicator-v2.rs:4711` and `Z9:100`.
EOF
run_lint
n_hits=$(grep -c '^FAIL .*bare line anchor' <<<"$OUT" || true)
if [ "$RC" -ne 0 ] && [ "$n_hits" -eq 3 ] \
   && grep -q 'fathomdb-cli:389' <<<"$OUT" \
   && grep -q 'wibble_frobnicator-v2.rs:4711' <<<"$OUT" \
   && grep -q 'Z9:100' <<<"$OUT"; then
  pass "generic shape rule — unenumerable prefixes all caught (3/3), not a whitelist"
else
  fail "arm 1b (generic rule): rc=$RC hits=$n_hits out=$OUT"
fi

# --- Arm 1c: ranges and open ends are anchors too ------------------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/ranges.md" <<'EOF'
---
status: ACTIVE
---

Worker at `lib.rs:876-887`; edges inlined from `engine:12166+`.
EOF
run_lint
n_hits=$(grep -c '^FAIL .*bare line anchor' <<<"$OUT" || true)
if [ "$RC" -ne 0 ] && [ "$n_hits" -eq 2 ]; then
  pass "ranges (\`lib.rs:876-887\`) and open ends (\`engine:12166+\`) are caught"
else
  fail "arm 1c (ranges/open ends): rc=$RC hits=$n_hits out=$OUT"
fi

# --- Arm 1d: no false positives on colons in code / sub-100 refs ---------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/notanchors.md" <<'EOF'
---
status: ACTIVE
---

`derive_logical_id = SHA256("{kind}:{name}")` is hashed; see `§2:98` and
`bm25(search_index_v2, 1.0, 3.0)` and `h:<content-hash>` and `12:345`.
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "no false positive on embedded colons, sub-100 \`:NN\` refs, or leading-digit tokens"
else
  fail "arm 1d (false positives): rc=$RC out=$OUT"
fi

# --- Arm 2: cited symbol DOES NOT EXIST -> FAIL (the crux) --------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/ghost.md" <<'EOF'
---
status: ACTIVE
---

The projector is `fn symbol_that_never_existed` in `widget/src/lib.rs`.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q 'does NOT occur' <<<"$OUT" \
   && grep -q 'symbol_that_never_existed' <<<"$OUT"; then
  pass "MANDATORY existence check: a cited symbol absent from the named file -> FAIL"
else
  fail "arm 2 (existence check) — THIS IS THE CRUX, the lint is laundering: rc=$RC out=$OUT"
fi

# --- Arm 3: cited symbol EXISTS -> PASS ---------------------------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/good.md" <<'EOF'
---
status: ACTIVE
---

`pub fn real_symbol` / `fn another_real_symbol` in `widget/src/lib.rs` both exist.
EOF
run_lint
if [ "$RC" -eq 0 ] && grep -q 'citation(s) verified' <<<"$OUT"; then
  pass "citations whose symbols exist (incl. a \`/\`-chain) -> exit 0"
else
  fail "arm 3 (valid citation): rc=$RC out=$OUT"
fi

# --- Arm 3b: cited file does not exist -> FAIL --------------------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/nofile.md" <<'EOF'
---
status: ACTIVE
---

See `pub fn real_symbol` in `gadget/src/nowhere.rs`.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q 'matches no file' <<<"$OUT"; then
  pass "citation naming a nonexistent file -> FAIL"
else
  fail "arm 3b (missing file): rc=$RC out=$OUT"
fi

# --- Arm 3c: ambiguous cited path -> FAIL -------------------------------
setup_fixture; baseline_plan
mkdir -p "$FIX/repo/src/rust/crates/gadget/src"
cp "$FIX/repo/src/rust/crates/widget/src/lib.rs" "$FIX/repo/src/rust/crates/gadget/src/lib.rs"
cat >"$FIX/repo/dev/plans/ambiguous.md" <<'EOF'
---
status: ACTIVE
---

See `pub fn real_symbol` in `src/lib.rs`.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -qi 'AMBIGUOUS' <<<"$OUT"; then
  pass "citation path resolving to >1 file -> FAIL as ambiguous (not silently picked)"
else
  fail "arm 3c (ambiguous path): rc=$RC out=$OUT"
fi

# --- Arm 3c1: untracked nested worktrees are not part of this checkout ----
setup_fixture; baseline_plan
mkdir -p "$FIX/repo/.claude/worktrees/stale/src/rust/crates/widget/src"
cp "$FIX/repo/src/rust/crates/widget/src/lib.rs" \
  "$FIX/repo/.claude/worktrees/stale/src/rust/crates/widget/src/lib.rs"
run_lint
if [ "$RC" -eq 0 ]; then
  pass "untracked nested worktree copy does not make a current-checkout citation ambiguous"
else
  fail "arm 3c1 (nested worktree exclusion): rc=$RC out=$OUT"
fi

# --- Arm 3d: elision placeholder -> FAIL, never skip --------------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/elided.md" <<'EOF'
---
status: ACTIVE
---

The `wire_recover(…, "truncate-wal", …)` in `widget/src/lib.rs` arm.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q 'elision placeholder' <<<"$OUT"; then
  pass "unverifiable (elided) citation -> FAIL, not skipped"
else
  fail "arm 3d (elision): rc=$RC out=$OUT"
fi

# --- Arm 3e: a soft wrap must not defeat the existence check ------------
setup_fixture; baseline_plan
printf -- '---\nstatus: ACTIVE\n---\n\n' >"$FIX/repo/dev/plans/wrapped.md"
{
  printf 'Plain wrap: the projector is `fn ghost_alpha` in\n'
  printf '`widget/src/lib.rs` — and it is not there.\n\n'
  printf '> Blockquote wrap: `fn ghost_beta` in\n'
  printf '>   `widget/src/lib.rs` — also not there.\n\n'
  printf 'Split after the symbol: `fn ghost_gamma`\nin `widget/src/lib.rs`.\n'
} >>"$FIX/repo/dev/plans/wrapped.md"
run_lint
n_hits=$(grep -c 'does NOT occur' <<<"$OUT" || true)
if [ "$RC" -ne 0 ] && [ "$n_hits" -eq 3 ] \
   && grep -q 'ghost_alpha' <<<"$OUT" && grep -q 'ghost_beta' <<<"$OUT" \
   && grep -q 'ghost_gamma' <<<"$OUT"; then
  pass "wrap-aware: line-break-split bad citations (plain, blockquote, post-symbol) all caught (3/3)"
else
  fail "arm 3e (wrap-aware) — a soft wrap defeats the check: rc=$RC hits=$n_hits out=$OUT"
fi

# --- Arm 3f: a BLANK line terminates a citation (no runaway match) ------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/paragraphs.md" <<'EOF'
---
status: ACTIVE
---

A paragraph ending in `fn ghost_delta`

in `widget/src/lib.rs` is a new paragraph, not a citation.
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "a blank line terminates a citation — paragraph boundaries are not wraps"
else
  fail "arm 3f (paragraph boundary): rc=$RC out=$OUT"
fi

# --- Arm 4: NON-ACTIVE plan with a bare anchor -> PASS (scope fence) ----
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/historical.md" <<'EOF'
---
status: COMPLETE
---

Shipped against `engine:11152`, with `fn no_such_symbol` in `widget/src/lib.rs`.
EOF
cat >"$FIX/repo/dev/plans/proposed.md" <<'EOF'
---
status: PROPOSED
---

Also `napi:1102`.
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "scope fence — COMPLETE/PROPOSED plans are historical record, not scanned"
else
  fail "arm 4 (non-ACTIVE scope fence): rc=$RC out=$OUT"
fi

# --- Arm 4b: runs/ and prompts/ are never scanned -----------------------
setup_fixture; baseline_plan
cat >"$FIX/repo/dev/plans/runs/STATUS-0.8.20.md" <<'EOF'
---
status: ACTIVE
---

Run artifact citing `engine:11152` and `fn no_such_symbol` in `widget/src/lib.rs`.
EOF
cat >"$FIX/repo/dev/plans/prompts/SOME-PROMPT.md" <<'EOF'
---
status: ACTIVE
---

Prompt citing `napi:1102`.
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "runs/ and prompts/ are immutable run artifacts — never scanned"
else
  fail "arm 4b (runs/prompts scope fence): rc=$RC out=$OUT"
fi

# --- Arm 5: ZERO ACTIVE plans -> FAIL (vacuous-pass guard, TC-37) -------
setup_fixture
cat >"$FIX/repo/dev/plans/only-complete.md" <<'EOF'
---
status: COMPLETE
---

Nothing active here.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q 'ZERO ACTIVE plans' <<<"$OUT"; then
  pass "vacuity guard — zero ACTIVE plans discovered -> hard FAIL, not a silent exit 0"
else
  fail "arm 5 (zero ACTIVE plans): rc=$RC out=$OUT"
fi

# --- Arm 5b: ZERO citations verified -> FAIL (vacuous-pass guard) -------
setup_fixture
cat >"$FIX/repo/dev/plans/no-citations.md" <<'EOF'
---
status: ACTIVE
---

An ACTIVE plan that cites nothing at all, so the existence check never runs.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q 'ZERO citations verified' <<<"$OUT"; then
  pass "vacuity guard — zero citations verified -> hard FAIL (the check did not execute)"
else
  fail "arm 5b (zero citations): rc=$RC out=$OUT"
fi

if [ "$FAILED" -ne 0 ]; then
  printf '%d arm(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf 'PASS  all arms — scripts/lint-plan-anchors.sh bans bare line anchors generically, VERIFIES every cited symbol against the file it names (incl. across soft wraps), stays scoped to ACTIVE dev/plans/*.md, and refuses to pass vacuously\n'
