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
# shellcheck source=lib/governed-surface-fixture.sh
. "$SCRIPT_DIR/lib/governed-surface-fixture.sh"
# shellcheck source=lib/c1-conformance-fixture.sh
. "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"

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

# --- Arm 3b (RED case "seq-gap-huge"): the report must be BOUNDED ------------
# A ledger with a LARGE accidental gap — seq 1 then 1000000000, with a sidecar
# that agrees with max(seq) — is exactly the corruption check (b) exists to
# report, and it was the input that made the gate fall over. The pre-fix code
# computed `sorted(set(range(lo, hi + 1)) - set(counts))`, i.e. it materialized
# every missing value BEFORE it could print a bounded report.
#
# MEASURED RED WITNESS on this fixture against the pre-fix checker:
#   `( ulimit -v 4000000; timeout 20 bash check-ledgers.sh --root <fixture> )`
#   -> rc=1 after 2.5 s with a Python `MemoryError` traceback and NO BROKEN
#      line at all. The 4 GiB address-space cap is a host guard, not the real
#      ceiling: set(range()) costs ~60 B/element and ~0.05 s/1e6 (measured at
#      1e6 and 1e7), so the uncapped run needs ~60 GiB and ~50 s — an OOM or a
#      hang on the very defect the gate is meant to name.
# GREEN: the same fixture must report BROKEN promptly, inside a bounded memory
# cap, with a CAPPED list of missing values and a truthful total.
# Which assertions below actually carried the RED (run against the pre-fix
# checker, 3 FAIL / suite rc=1): the three that require a BROKEN missing-seq
# line, its ascending enumeration, and its capped tail. The rc and elapsed
# assertions passed VACUOUSLY there — under the address-space cap the old code
# died fast and with rc=1 — so they are regression guards, not the RED signal.
R="$(mkroot seq-gap-huge)"
write_ledger "$R" x 1000000000 1 1000000000
# `ulimit -v` is GNU/Linux; where the shell will not take it (macOS) the
# timeout alone still bounds the arm.
if ( ulimit -v 4000000 ) 2>/dev/null; then HUGE_LIMIT='ulimit -v 4000000'; else HUGE_LIMIT=':'; fi
if command -v timeout >/dev/null 2>&1; then HUGE_TIMEOUT=(timeout 20); else HUGE_TIMEOUT=(); fi
HUGE_START=$SECONDS
set +e
OUT="$( ( eval "$HUGE_LIMIT"; "${HUGE_TIMEOUT[@]}" bash "$CHECKER" --root "$R" ) 2>&1 )"
RC=$?
set -e
HUGE_ELAPSED=$((SECONDS - HUGE_START))
expect_rc 1 "a HUGE seq gap HARD-fails with the integrity exit code (not a timeout/OOM crash)"
if [ "$HUGE_ELAPSED" -lt 10 ]; then
  pass "a HUGE seq gap is reported promptly (${HUGE_ELAPSED}s, well inside the 20s timeout)"
else
  fail "a HUGE seq gap took ${HUGE_ELAPSED}s — the report is not bounded in time"
fi
HUGE_LINE="$(printf '%s\n' "$OUT" | grep 'BROKEN.*missing seq' || true)"
if [ -n "$HUGE_LINE" ]; then
  pass "a HUGE seq gap still produces the BROKEN missing-seq line"
else
  fail "expected a BROKEN line naming the missing seq; got: $OUT"
fi
if printf '%s' "$HUGE_LINE" | grep -q 'missing seq: 2, 3'; then
  pass "the HUGE-gap report enumerates from the first missing value, ascending"
else
  fail "expected the enumeration to start at 2, 3; got: $HUGE_LINE"
fi
if printf '%s' "$HUGE_LINE" | grep -qE '\.\.\.and 999999993 more$'; then
  pass "the HUGE-gap report caps the list and states the true remaining count"
else
  fail "expected the capped '...and N more' tail with the true total; got: $HUGE_LINE"
fi
if [ -n "$HUGE_LINE" ] && [ "${#HUGE_LINE}" -lt 300 ]; then
  pass "the HUGE-gap report is bounded in size (${#HUGE_LINE} chars, not a billion values)"
else
  fail "the missing-seq line is ${#HUGE_LINE} chars — the report is not bounded"
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
  # `--landing` also runs the governed-surface pin gate (DOC-HYGIENE-2 T1e),
  # which HARD-fails a tree whose pin it cannot read — the same TC-37 stance this
  # suite's own gate takes, and equally correct. The fixture therefore carries a
  # minimal, self-consistent surface + pin, so the only thing these arms can fail
  # on is the LEDGER state they deliberately plant.
  seed_governed_surface_fixture "$primary"
  seed_c1_conformance_fixture "$primary"
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
