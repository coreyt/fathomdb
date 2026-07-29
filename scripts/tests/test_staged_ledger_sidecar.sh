#!/usr/bin/env bash
# scripts/tests/test_staged_ledger_sidecar.sh — TC-88 recurrence guard
# (DOC-HYGIENE-3): A LEDGER AND ITS `.seq` SIDECAR CANNOT BE COMMITTED APART.
#
# ---------------------------------------------------------------------------
# THE INCIDENT (measured, twice in the same session, by the same actor)
# ---------------------------------------------------------------------------
# `dev/agent-tools/ledgerwrite/ledgerwrite.py` appends to `<ledger>.jsonl` AND
# writes the new high-water mark to a sibling `<ledger>.jsonl.seq`. The natural
# git incantation after an append is `git add dev/steward/steward-ledger.jsonl`
# — the sidecar is a SEPARATE PATH, is not implied by it, and nothing warned.
# Commits 41a81c17 (steward seq-131) and 3e660f95 (todos TC-87) each staged the
# `.jsonl` and left the `.seq` behind. Twenty minutes apart. Discipline did not
# prevent the second occurrence, which is the whole point.
#
# BLAST RADIUS, and why this is worse than a private mistake. The sidecar is
# TRACKED and is the only thing an appender reads to pick the next seq, so a
# stranded sidecar mints COLLIDING seq numbers in every clone taken afterwards.
# `scripts/preflight.sh --landing` §8 hard-fails when a sidecar disagrees with
# max(seq) — so the actor who caused it sees nothing wrong while THE LANDING GATE
# ON MAIN IS BROKEN FOR EVERY SUBSEQUENT AGENT. It surfaced only because the
# Slice 21 orchestrator hit exit 1 at its push and correctly refused to clear a
# ledger it had been told was not its own. A well-behaved halt is the ONLY reason
# it was caught rather than compounding.
#
# ---------------------------------------------------------------------------
# WHY DETECTION ALREADY EXISTED AND WAS NOT ENOUGH
# ---------------------------------------------------------------------------
# `scripts/check-ledgers.sh` reads the WORKING TREE. On the machine that made the
# mistake the working tree is CONSISTENT — both files are correct on disk; only
# the COMMIT is torn. So preflight and the CI job both went green for the author
# and both went red for everyone downstream. The missing predicate is over the
# INDEX, at commit time, and that is what `scripts/check-staged-ledger-sidecars.sh`
# adds.
#
# THE FIX IS TOOLING, NOT A NOTE. TC-88 says so explicitly ("DO NOT fix this by
# telling agents to remember the sidecar; that is the failure mode this repo has
# already ruled against"), and it is the repo's standing rule.
#
# ---------------------------------------------------------------------------
# PREDICATE UNDER TEST
# ---------------------------------------------------------------------------
# A commit that stages a ledger `.jsonl` or its `.seq` sidecar must leave the
# PAIR consistent AS COMMITTED: the integer in `<ledger>.jsonl.seq` equals
# max(seq) over the entries of `<ledger>.jsonl`, read from the index (falling
# back to HEAD for whichever half is not staged). Anything else is refused, with
# the exact `git add` that fixes it.
#
# REUSE, NOT REIMPLEMENTATION: the gate materialises the index into a temp tree
# and runs `scripts/check-ledgers.sh --root` over it, so the commit-time
# predicate is the SAME CODE as the landing-time and CI ones and cannot diverge
# from them. Arm 6 asserts that mechanically.
#
# Isolation: every arm runs in a throwaway git repo under mktemp -d. Nothing here
# writes into the real checkout — arm 7 asserts that.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="${GATE_UNDER_TEST:-$REPO_ROOT/scripts/check-staged-ledger-sidecars.sh}"
PRE_COMMIT="$REPO_ROOT/scripts/hooks/pre-commit"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"

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

FIX="$TMPROOT/repo"
OUT=""
RC=0

# A fixture repo with ONE real ledger pair, committed and consistent, plus an
# unrelated tracked file and a sidecar-less `.jsonl` (run output / fixture data —
# out of scope by construction, exactly as in the real repo).
setup_fixture() {
  rm -rf "$FIX"
  mkdir -p "$FIX/dev/steward" "$FIX/dev/experiments/out" "$FIX/scripts"
  cp "$GATE" "$FIX/scripts/check-staged-ledger-sidecars.sh" 2>/dev/null || true
  chmod +x "$FIX/scripts/check-staged-ledger-sidecars.sh" 2>/dev/null || true
  cp "$REPO_ROOT/scripts/check-ledgers.sh" "$FIX/scripts/check-ledgers.sh"
  chmod +x "$FIX/scripts/check-ledgers.sh"
  (cd "$FIX" && git init -q && git config user.email t@example.com && git config user.name t)

  {
    printf '{"seq": 1, "note": "first"}\n'
    printf '{"seq": 2, "note": "second"}\n'
  } >"$FIX/dev/steward/steward-ledger.jsonl"
  printf '2' >"$FIX/dev/steward/steward-ledger.jsonl.seq"

  printf 'unrelated tracked content\n' >"$FIX/README.md"
  # A `.jsonl` with NO sidecar: run output, not a ledger. Must never be in scope.
  printf '{"run": 1}\n' >"$FIX/dev/experiments/out/results.jsonl"

  (cd "$FIX" && git add -A && git commit -qm base)
}

# Append an entry to the fixture ledger and update the sidecar ON DISK, exactly
# as ledgerwrite.py does. What each arm then STAGES is the variable under test.
append_entry() {
  printf '{"seq": %s, "note": "appended"}\n' "$1" >>"$FIX/dev/steward/steward-ledger.jsonl"
  printf '%s' "$1" >"$FIX/dev/steward/steward-ledger.jsonl.seq"
}

run_gate() {
  set +e
  OUT="$(cd "$FIX" && bash ./scripts/check-staged-ledger-sidecars.sh "$@" 2>&1)"
  RC=$?
  set -e
}

# --- Arm 0: a consistent, fully-staged pair is CLEARED ---------------------
# Without this every RED arm below could be passing because the gate refuses
# everything.
setup_fixture
append_entry 3
(cd "$FIX" && git add dev/steward/steward-ledger.jsonl dev/steward/steward-ledger.jsonl.seq)
run_gate
if [ "$RC" -eq 0 ]; then
  pass "baseline — a ledger staged TOGETHER with its sidecar is cleared (exit 0)"
else
  fail "arm 0 (consistent pair): rc=$RC out=$OUT"
fi

# --- Arm 1: THE INCIDENT — the `.jsonl` staged ALONE is REFUSED ------------
# `git add dev/steward/steward-ledger.jsonl` and nothing else. The working tree
# is perfectly consistent, which is exactly why check-ledgers.sh over the
# worktree cannot see this; the COMMIT is what would be torn.
setup_fixture
append_entry 3
(cd "$FIX" && git add dev/steward/steward-ledger.jsonl)
run_gate
if [ "$RC" -ne 0 ] && grep -q 'steward-ledger.jsonl.seq' <<<"$OUT"; then
  pass "the measured incident — staging the ledger ALONE is refused, and the sidecar is named"
else
  fail "arm 1 (jsonl staged alone): rc=$RC out=$OUT"
fi

# --- Arm 1b: ...and the refusal states the EXACT command that fixes it ------
# A gate that says "wrong" without saying "run this" makes the fix a guess, and a
# guess at commit time is how `--no-verify` gets reached for.
if grep -q 'git add' <<<"$OUT" && grep -q 'dev/steward/steward-ledger.jsonl.seq' <<<"$OUT"; then
  pass "the refusal prints the exact \`git add\` that repairs the commit"
else
  fail "arm 1b (actionable remedy): out=$OUT"
fi

# --- Arm 2: THE OPPOSITE DIRECTION — the sidecar staged alone -------------
# Staging only the `.seq` publishes a high-water mark for entries that are not in
# the commit. Same tear, other half; without this arm the gate could be a
# one-directional "did you also add the .seq" check.
setup_fixture
append_entry 3
(cd "$FIX" && git add dev/steward/steward-ledger.jsonl.seq)
run_gate
if [ "$RC" -ne 0 ] && grep -q 'steward-ledger.jsonl' <<<"$OUT"; then
  pass "the other direction — staging the SIDECAR alone is refused too"
else
  fail "arm 2 (seq staged alone): rc=$RC out=$OUT"
fi

# --- Arm 3: A COMMIT THAT TOUCHES NO LEDGER IS NOT DELAYED OR BLOCKED ------
# The gate runs in a pre-commit hook on every commit in the repo. If it fired on
# unrelated work it would be turned off, and a gate that gets turned off is worse
# than no gate. Note the fixture ALSO carries a modified-but-unstaged ledger
# here: an unrelated commit made while a ledger append sits in the working tree
# must still go through.
setup_fixture
append_entry 3
printf 'a change with nothing to do with ledgers\n' >>"$FIX/README.md"
(cd "$FIX" && git add README.md)
run_gate
if [ "$RC" -eq 0 ]; then
  pass "a commit staging no ledger path is cleared, even with an unstaged ledger append on disk"
else
  fail "arm 3 (unrelated commit): rc=$RC out=$OUT"
fi

# --- Arm 4: A SIDECAR-LESS `.jsonl` IS OUT OF SCOPE BY CONSTRUCTION --------
# `dev/experiments/**` run output, test fixtures and golden corpora are `.jsonl`
# files that are NOT ledgers. Discovery is driven by the SIDECAR, exactly as
# check-ledgers.sh's is; committing one of these must not be refused.
setup_fixture
printf '{"run": 2}\n' >>"$FIX/dev/experiments/out/results.jsonl"
(cd "$FIX" && git add dev/experiments/out/results.jsonl)
run_gate
if [ "$RC" -eq 0 ]; then
  pass "a \`.jsonl\` with no sidecar (run output, fixtures) is out of scope and is cleared"
else
  fail "arm 4 (sidecar-less jsonl): rc=$RC out=$OUT"
fi

# --- Arm 5: A PAIR STAGED TOGETHER BUT INCONSISTENT IS STILL REFUSED -------
# The predicate is CONSISTENCY AS COMMITTED, not "both paths appear in the
# index". A gate that only counted paths would clear this — a sidecar reading 9
# over a ledger whose max(seq) is 3 — which is the same unbacked high-water mark
# the original incident produced, reached by a different route.
setup_fixture
append_entry 3
printf '9' >"$FIX/dev/steward/steward-ledger.jsonl.seq"
(cd "$FIX" && git add dev/steward/steward-ledger.jsonl dev/steward/steward-ledger.jsonl.seq)
run_gate
if [ "$RC" -ne 0 ] && grep -qE 'BROKEN|9' <<<"$OUT"; then
  pass "both staged but DISAGREEING is refused — the predicate is consistency, not path-counting"
else
  fail "arm 5 (staged but inconsistent): rc=$RC out=$OUT"
fi

# --- Arm 5b: a torn seq RUN is refused too (contiguity) --------------------
# Free, and load-bearing, because it comes from reusing check-ledgers.sh rather
# than writing a second predicate: skipping a seq is the collision the stranded
# sidecar was going to cause, arriving directly.
setup_fixture
append_entry 7
(cd "$FIX" && git add dev/steward/steward-ledger.jsonl dev/steward/steward-ledger.jsonl.seq)
run_gate
# NON-VACUITY: `rc != 0` alone is satisfied by rc=127 (gate absent), which is
# exactly the state this arm was first measured in. It must also NAME the ledger.
if [ "$RC" -ne 0 ] && grep -q 'steward-ledger' <<<"$OUT"; then
  pass "a gap in the seq run is refused (contiguity comes free from reusing check-ledgers.sh)"
else
  fail "arm 5b (seq gap): rc=$RC out=$OUT"
fi

# --- Arm 6: REUSE — the gate INVOKES scripts/check-ledgers.sh --------------
# The repo's stated pattern for every shared gate (board-currency, ledger
# integrity, governed-surface pin, C-1 conformance, transcript hygiene): one
# predicate, several callers. A second copy of "sidecar == max(seq)" would drift
# from the one preflight --landing enforces, and the two disagreeing is a worse
# failure than either alone.
if grep -q 'check-ledgers.sh' "$GATE"; then
  pass "the gate reuses scripts/check-ledgers.sh rather than reimplementing the predicate"
else
  fail "arm 6 (reuse): $GATE does not invoke scripts/check-ledgers.sh"
fi

# --- Arm 7: READ-ONLY — checking changes neither the worktree nor the index -
# A commit-time gate that mutated the index would be changing what is about to be
# committed out from under the author.
setup_fixture
append_entry 3
(cd "$FIX" && git add dev/steward/steward-ledger.jsonl)
STATUS_BEFORE="$(cd "$FIX" && git status --porcelain --untracked-files=all)"
run_gate
STATUS_AFTER="$(cd "$FIX" && git status --porcelain --untracked-files=all)"
# NON-VACUITY: a gate that does not exist also changes nothing. rc=127 fails.
if [ "$RC" -ne 127 ] && [ "$STATUS_BEFORE" = "$STATUS_AFTER" ]; then
  pass "read-only — the gate leaves the index and the worktree exactly as it found them"
else
  fail "arm 7 (read-only): before=[$STATUS_BEFORE] after=[$STATUS_AFTER]"
fi

# --- Arm 8: WIRED INTO THE TRACKED pre-commit HOOK ------------------------
# The gate has to run at the moment of the mistake. TC-88 named a pre-commit hook
# as the cheapest remedy precisely because it "catches it at the moment of the
# mistake".
if grep -q 'check-staged-ledger-sidecars.sh' "$PRE_COMMIT"; then
  pass "scripts/hooks/pre-commit runs the staged-ledger-sidecar gate"
else
  fail "arm 8 (hook wiring): scripts/hooks/pre-commit does not run the gate"
fi

# --- Arm 9: THE REAL REPO'S OWN STATE IS CLEAN ----------------------------
# The regression half. Run against this checkout's actual index: whatever is
# staged right now must leave every ledger pair consistent.
set +e
REAL_OUT="$(cd "$REPO_ROOT" && bash scripts/check-staged-ledger-sidecars.sh 2>&1)"
REAL_RC=$?
set -e
if [ "$REAL_RC" -eq 0 ]; then
  pass "real repo — the current index leaves every ledger pair consistent"
else
  fail "arm 9 (real repo): rc=$REAL_RC out=$REAL_OUT"
fi

# --- Arm 10: wired into agent-test.sh -------------------------------------
if grep -q 'scripts/tests/test_staged_ledger_sidecar.sh' "$AGENT_TEST"; then
  pass "agent-test.sh runs this suite"
else
  fail "agent-test.sh does not run scripts/tests/test_staged_ledger_sidecar.sh"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll staged-ledger-sidecar tests passed\n'
