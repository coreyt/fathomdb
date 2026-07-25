#!/usr/bin/env bash
# scripts/tests/test_check_ledgers.sh — coverage for the shared ledger-integrity
# predicate (scripts/check-ledgers.sh) AND for its two wirings: `preflight.sh
# --landing` (PREVENT) and the always-on CI job (DETECT).
#
# The incident this closes: 19 consecutive commits (f22e4947 -> 3264114a, four
# days) shipped `dev/steward/steward-ledger.jsonl.seq` frozen at 80 while
# max(seq) inside the ledger had reached 98. The sidecar is a TRACKED file, so
# every clone taken in that window would have minted seq numbers starting at 81
# — silently colliding with 18 already-recorded entries. Nothing in the repo
# checked that the sidecar agreed with the file it summarizes.
#
# Predicate under test (see scripts/check-ledgers.sh header for the full
# statement) — EXACTLY TWO checks, per ledger:
#   (a) sidecar agreement: <ledger>.jsonl.seq == max(seq) over the entries.
#   (b) contiguity:        the seq values have no gaps and no duplicates.
# Plus the TC-37 vacuous-pass guard: discovering ZERO ledgers is itself a HARD
# failure, never a silent exit 0.
#
# RED-first: both checks pass today on all three real ledgers, so asserting
# against the real repo alone would prove nothing (a `true` script would pass
# it). Every failure arm below therefore runs against a purpose-built CORRUPT
# fixture root, so each arm can only go green because the predicate actually
# fires. The real-repo arm is the regression half of the same pair.
#
# Isolation: fixtures are plain directories under mktemp -d (the checker takes
# --root for exactly this reason); the preflight arms build throwaway git repos
# + linked worktrees. Nothing here writes into the real checkout, and no real
# .jsonl / .jsonl.seq is ever touched.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-ledgers.sh"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"

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

# mkroot <name> -> prints the fixture root's path (a plain dir, no git needed)
mkroot() {
  local d="$TMPROOT/$1"
  mkdir -p "$d"
  printf '%s' "$d"
}

# write_ledger <root> <relpath-without-ext> <sidecar-text> <seq...>
# Writes <relpath>.jsonl with one entry per given seq, and the sidecar verbatim
# (verbatim so a garbage / trailing-newline / stale sidecar can be expressed).
write_ledger() {
  local root="$1" rel="$2" sidecar="$3"; shift 3
  mkdir -p "$(dirname "$root/$rel")"
  : >"$root/$rel.jsonl"
  local s
  for s in "$@"; do
    printf '{"seq":%s,"note":"entry %s"}\n' "$s" "$s" >>"$root/$rel.jsonl"
  done
  printf '%s' "$sidecar" >"$root/$rel.jsonl.seq"
}

run_checker() {
  set +e
  OUT="$(bash "$CHECKER" "$@" 2>&1)"
  RC=$?
  set -e
}

run_preflight() {
  local cwd="$1"; shift
  set +e
  OUT="$(cd "$cwd" && bash "$PREFLIGHT" "$@" 2>&1)"
  RC=$?
  set -e
}

# expect_rc <expected-rc> <description>  (reads RC/OUT set by run_*)
expect_rc() {
  local want="$1" desc="$2"
  if [ "$RC" -eq "$want" ]; then
    pass "$desc"
  else
    fail "$desc — expected rc=$want, got rc=$RC; out: $OUT"
  fi
}

# ============================== check-ledgers.sh ==============================

# --- Arm 1: a clean fixture root (two ledgers, one nested) passes -------------
R="$(mkroot clean)"
write_ledger "$R" dev/steward/steward-ledger 3 1 2 3
write_ledger "$R" dev/design/sub/OPP-12-sub-ledger 2 1 2
run_checker --root "$R"
expect_rc 0 "clean fixture (2 ledgers, one nested) exits 0"

# --- Arm 2 (RED case "sidecar-mismatch"): the measured incident ---------------
# Entries reach seq 3; the tracked sidecar still says 2 — exactly the shape of
# the 80-vs-98 freeze. Check (a) alone must catch this (contiguity is fine).
R="$(mkroot sidecar-mismatch)"
write_ledger "$R" dev/steward/steward-ledger 2 1 2 3
run_checker --root "$R"
expect_rc 1 "sidecar disagreeing with max(seq) HARD-fails"
if printf '%s' "$OUT" | grep -q 'BROKEN.*steward-ledger.jsonl.seq'; then
  pass "sidecar-mismatch output names the offending sidecar"
else
  fail "expected a BROKEN line naming the sidecar; got: $OUT"
fi
if printf '%s' "$OUT" | grep -qE 'sidecar (says|reads) 2.*max\(seq\) (is )?3|2.*!=.*3'; then
  pass "sidecar-mismatch output names both the sidecar value and max(seq)"
else
  fail "expected the message to name 2 and 3; got: $OUT"
fi

# --- Arm 3 (RED case "seq-gap") ----------------------------------------------
# seq 1,2,4 — sidecar agrees with max(seq)=4, so ONLY check (b) can catch it.
R="$(mkroot seq-gap)"
write_ledger "$R" dev/steward/steward-ledger 4 1 2 4
run_checker --root "$R"
expect_rc 1 "a gap in seq HARD-fails even when the sidecar agrees with max(seq)"
if printf '%s' "$OUT" | grep -q 'BROKEN.*[Mm]issing'; then
  pass "seq-gap output names the missing seq"
else
  fail "expected a BROKEN line naming the missing seq; got: $OUT"
fi

# --- Arm 4 (RED case "seq-duplicate") ----------------------------------------
# seq 1,2,2 — max(seq)=2 matches the sidecar, so again only check (b) fires.
# This is the collision shape the frozen sidecar would have produced.
R="$(mkroot seq-duplicate)"
write_ledger "$R" dev/steward/steward-ledger 2 1 2 2
run_checker --root "$R"
expect_rc 1 "a duplicate seq HARD-fails even when the sidecar agrees with max(seq)"
if printf '%s' "$OUT" | grep -q 'BROKEN.*[Dd]uplicate'; then
  pass "seq-duplicate output names the duplicated seq"
else
  fail "expected a BROKEN line naming the duplicate; got: $OUT"
fi

# --- Arm 5 (RED case "zero-ledgers"): TC-37 vacuous-pass guard ----------------
# A gate that discovers nothing and reports ok is worse than no gate: it is an
# active false assurance. Zero discovered ledgers must be LOUD, never exit 0.
R="$(mkroot zero-ledgers)"
run_checker --root "$R"
expect_rc 1 "discovering ZERO ledgers HARD-fails (TC-37 vacuous-pass guard)"
if printf '%s' "$OUT" | grep -q 'BROKEN.*no .*ledger'; then
  pass "zero-ledgers failure says loudly that it could not vouch for anything"
else
  fail "expected a BROKEN line about discovering no ledgers; got: $OUT"
fi

# --- Arm 6: malformed line — deterministic HARD fail, not a crash -------------
R="$(mkroot malformed)"
write_ledger "$R" dev/steward/steward-ledger 3 1 2 3
printf '{"seq":4,"note":"torn\n' >>"$R/dev/steward/steward-ledger.jsonl"
printf '%s' 4 >"$R/dev/steward/steward-ledger.jsonl.seq"
run_checker --root "$R"
expect_rc 1 "a malformed (non-JSON) line HARD-fails with a line number, not a crash"
if printf '%s' "$OUT" | grep -q 'BROKEN.*:4:'; then
  pass "malformed-line failure names the offending line number"
else
  fail "expected a BROKEN line naming line 4; got: $OUT"
fi

# --- Arm 7: blank lines are ignored (documented behaviour) -------------------
R="$(mkroot blank-line)"
write_ledger "$R" dev/steward/steward-ledger 3 1 2 3
# Re-write with a blank line in the middle and a whitespace-only line at the end.
printf '{"seq":1}\n\n{"seq":2}\n   \n{"seq":3}\n' >"$R/dev/steward/steward-ledger.jsonl"
run_checker --root "$R"
expect_rc 0 "blank / whitespace-only lines are ignored, not treated as corruption"

# --- Arm 8: an entry with no integer seq HARD-fails --------------------------
R="$(mkroot no-seq-field)"
printf '{"seq":1}\n{"note":"no seq here"}\n' >"$R/x.jsonl"
mkdir -p "$R" && printf '%s' 1 >"$R/x.jsonl.seq"
run_checker --root "$R"
expect_rc 1 "an entry with no integer seq HARD-fails (cannot be checked silently)"

# --- Arm 9: empty ledger — sidecar must read 0 -------------------------------
R="$(mkroot empty-ledger-zero)"
mkdir -p "$R"
: >"$R/x.jsonl"
printf '%s' 0 >"$R/x.jsonl.seq"
run_checker --root "$R"
expect_rc 0 "an empty ledger whose sidecar reads 0 is consistent (max(seq) := 0)"

R="$(mkroot empty-ledger-nonzero)"
mkdir -p "$R"
: >"$R/x.jsonl"
printf '%s' 5 >"$R/x.jsonl.seq"
run_checker --root "$R"
expect_rc 1 "an empty ledger whose sidecar claims 5 HARD-fails (unbacked high-water mark)"

# --- Arm 10: dangling sidecar (no .jsonl beside it) --------------------------
R="$(mkroot dangling-sidecar)"
mkdir -p "$R"
printf '%s' 7 >"$R/x.jsonl.seq"
run_checker --root "$R"
expect_rc 1 "a sidecar with no .jsonl beside it HARD-fails"

# --- Arm 11: sidecar formatting — trailing newline ok, garbage is not --------
R="$(mkroot sidecar-trailing-newline)"
write_ledger "$R" x $'3\n' 1 2 3
run_checker --root "$R"
expect_rc 0 "a sidecar with a trailing newline is accepted (real ones have none)"

R="$(mkroot sidecar-garbage)"
write_ledger "$R" x 'eighty' 1 2 3
run_checker --root "$R"
expect_rc 1 "a non-integer sidecar HARD-fails"

R="$(mkroot sidecar-blank)"
write_ledger "$R" x '' 1 2 3
run_checker --root "$R"
expect_rc 1 "an empty sidecar HARD-fails"

# --- Arm 12: contiguity does not pin min(seq) to 1 (documented) --------------
# A ledger whose head was archived legitimately starts above 1; the predicate
# is "no gaps, no duplicates", not "starts at 1". Pinning min would be a THIRD
# check, and this tranche ships exactly two.
R="$(mkroot min-not-one)"
write_ledger "$R" x 7 5 6 7
run_checker --root "$R"
expect_rc 0 "contiguity deliberately does not require min(seq)==1"

# --- Arm 13: out-of-scope jsonl (no sidecar) is not checked ------------------
# dev/experiments/** output jsonl have no sidecar and no seq; discovery is
# driven by the SIDECAR, so they must be invisible to this gate.
R="$(mkroot no-sidecar)"
mkdir -p "$R/dev/experiments/out"
printf '{"anything":"at all"}\n{"not":"a ledger"}\n' >"$R/dev/experiments/out/drift.jsonl"
write_ledger "$R" dev/steward/steward-ledger 1 1
run_checker --root "$R"
expect_rc 0 "a .jsonl with no .seq sidecar is out of scope (never checked)"

# --- Arm 14: usage errors are a DISTINCT exit code (2), never 0 --------------
run_checker --not-a-flag
expect_rc 2 "an unknown flag exits 2 (usage), distinct from an integrity failure"

R="$(mkroot missing-root)"
run_checker --root "$R/does-not-exist"
expect_rc 2 "a --root that does not exist exits 2, never a silent 0"

# ============================ the three REAL ledgers ==========================
# The regression half: the predicate must be satisfiable, and it must discover
# every sidecar-bearing ledger in this repo (3 today), not zero and not one.
run_checker
expect_rc 0 "the real repo's ledgers pass both checks"
for real in dev/steward/steward-ledger.jsonl \
            dev/todos-and-considerations-ledger.jsonl \
            dev/design/record-lifecycle-protocol/OPP-12-sub-ledger.jsonl; do
  if printf '%s' "$OUT" | grep -qF "$real"; then
    pass "real-repo run reports $real"
  else
    fail "real-repo run never mentions $real (discovery hole?); out: $OUT"
  fi
done

# ============================ preflight.sh --landing ==========================
# These arms prove the PREVENT wiring. Pre-wiring they are the RED witness for
# the gap: a repo carrying a corrupt ledger cleared `--landing` with exit 0.

NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# make_repo <primary> <linked> <sidecar-text> <seq...> — a throwaway repo whose
# only ledger is built from the given sidecar/seqs, plus a linked worktree
# (TC-RUBRIC-5 forbids --landing in a primary checkout).
make_repo() {
  local primary="$1" linked="$2" sidecar="$3"; shift 3
  mkdir -p "$primary"
  git init -q -b main "$primary"
  git -C "$primary" config user.email ledger-test@example.invalid
  git -C "$primary" config user.name 'Ledger Test'
  git -C "$primary" config commit.gpgsign false
  git -C "$primary" config core.hooksPath "$NO_HOOKS"
  mkdir -p "$primary/src" "$primary/scripts"
  printf 'fixture\n' >"$primary/src/keep.txt"
  write_ledger "$primary" dev/steward/steward-ledger "$sidecar" "$@"
  git -C "$primary" add -A
  git -C "$primary" commit -q -m 'fixture: initial commit'
  git -C "$primary" worktree add -q -b landing-fixture "$linked" >/dev/null 2>&1
}

BROKEN_PRIMARY="$TMPROOT/repo-broken"
BROKEN_LINKED="$TMPROOT/repo-broken-wt"
make_repo "$BROKEN_PRIMARY" "$BROKEN_LINKED" 2 1 2 3

CLEAN_PRIMARY="$TMPROOT/repo-clean"
CLEAN_LINKED="$TMPROOT/repo-clean-wt"
make_repo "$CLEAN_PRIMARY" "$CLEAN_LINKED" 3 1 2 3

run_preflight "$BROKEN_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing HARD-fails in a worktree whose ledger sidecar is stale"
else
  fail "--landing MUST fail on a stale sidecar (this is the incident reproduced); out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'HARD.*ledger-integrity:'; then
  pass "--landing failure output names the ledger-integrity check"
else
  fail "expected a HARD line naming ledger-integrity; got: $OUT"
fi

run_preflight "$CLEAN_LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing still exits 0 in a worktree whose ledgers are consistent"
else
  fail "--landing must not regress a consistent tree; got rc=$RC, out: $OUT"
fi

# Mirrors the board-currency contract (test_check_board_currency.sh Arm 8):
# the check is --landing-only, so plain preflight stays lean and unchanged.
run_preflight "$BROKEN_LINKED"
if printf '%s' "$OUT" | grep -q 'ledger-integrity:'; then
  fail "ledger-integrity must be --landing-only; it ran without --landing: $OUT"
else
  pass "regression guard: ledger-integrity is inert without --landing"
fi

# ============================ CI wiring is ALWAYS-ON ==========================
# A docs_only-gated job never fires on a code push, and a code push can freeze a
# sidecar just as easily as a docs push (the measured incident spanned both).
# Assert statically that the job exists, runs the SHARED script, and carries no
# `if:` condition at all.
CI_JOB_BLOCK="$(awk '
  /^  ledger-integrity:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a ledger-integrity job"
else
  fail "ci.yml has no ledger-integrity job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/check-ledgers.sh'; then
  pass "the CI job runs the SHARED scripts/check-ledgers.sh (one predicate, two callers)"
else
  fail "the CI job must invoke scripts/check-ledgers.sh, not a reimplementation"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*if:'; then
  fail "the ledger-integrity job must be ALWAYS-ON (no if:/docs_only gate); block: $CI_JOB_BLOCK"
else
  pass "the ledger-integrity job is always-on (no if: condition, not docs_only-gated)"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-ledgers tests passed\n'
