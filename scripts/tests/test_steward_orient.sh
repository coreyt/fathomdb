#!/usr/bin/env bash
# scripts/tests/test_steward_orient.sh — recurrence guard for the stateless
# cold-start briefing (DOC-HYGIENE-2 T3a, scripts/steward-orient.sh).
#
# The incident this closes: every Steward cold start re-paid for the same
# orientation, because live state was narrated across a 5-12 file fan-out. The
# briefing collapses that into ONE stateless read. What makes it trustworthy is
# not that it prints something — it is that it CANNOT print a silently partial
# picture, and that it cannot be quietly pinned to a release that has closed.
#
# Predicate under test (see scripts/steward-orient.sh's header for the full
# statement):
#   * output is <= 4096 bytes and NO file is written anywhere — INCLUDING
#     `.git/index`, which `git status` rewrites when it refreshes a stale stat
#     cache (arms 2b-2d);
#   * every load-bearing section is non-empty, or the run HARD-fails naming the
#     section (the documented exemptions are the sections that have a legitimate
#     empty state: open-PR count, orphan checkout dirs, dirty-file count);
#   * the release is derived from the LIVE BOARD FILENAME, never hardcoded;
#   * `<repo-root>-worktrees/` is resolved as a SIBLING of the repo root;
#   * the board-CLOSED predicate is the SHARED one (head -n 15, not head -n 5).
#
# RED-first: the happy path passes on the real repo today, so asserting against
# the real checkout alone would prove nothing (a `true` script would pass it).
# Every failure arm below runs against a purpose-built fixture repo in which the
# fault is deliberately planted, so an arm can only go green because the guard
# actually fired. The real-repo arms are the regression half of the pair.
#
# NON-VACUITY: the suite honours $GATE_UNDER_TEST, so a MUTANT briefing (one
# whose zero-result guard never fires) can be pointed at it. That mutant turns
# arms 3/4/5/8 RED — which is the evidence that a green here is load-bearing
# rather than a script that merely exits 0.
#
# Isolation: fixtures are throwaway git repos under mktemp -d (the briefing does
# `cd "$(git rev-parse --show-toplevel)"`, so each fixture needs to BE a repo),
# and each fixture is nested one directory deep so its SIBLING worktrees dir is
# a real, writable path. Nothing here writes into the real checkout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="${GATE_UNDER_TEST:-$REPO_ROOT/scripts/steward-orient.sh}"
BOARD_CURRENCY="$REPO_ROOT/scripts/check-board-currency.sh"
CLOSED_LIB="$REPO_ROOT/scripts/lib/board-closed.sh"
LEDGERWATCH="$REPO_ROOT/dev/agent-tools/ledgerwatch/ledgerwatch.py"
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

# One directory deep so "$FIX"-worktrees is a sibling INSIDE $TMPROOT.
FIX="$TMPROOT/parent/proj"
WTDIR="$TMPROOT/parent/proj-worktrees"
OUT=""
ERR=""
RC=0
BYTES=0

# ---------------------------------------------------------------- fixture ---
# A minimal but REAL fixture repo carrying one of everything the briefing reads:
# a live board with a next-action row, its release-state file, both ledgers, and
# a dated Steward hand-off. Every arm mutates exactly ONE thing from this
# baseline, so a red arm isolates a single fault.
setup_fixture() {
  rm -rf "$TMPROOT/parent"
  mkdir -p "$FIX/dev/plans/runs" "$FIX/dev/steward" \
           "$FIX/dev/agent-tools/ledgerwatch" "$WTDIR"
  cp "$LEDGERWATCH" "$FIX/dev/agent-tools/ledgerwatch/ledgerwatch.py"

  # The live board is 0.9.0 — deliberately NOT the release the real repo is on,
  # so a hardcoded "0.8.20" implementation cannot pass any fixture arm.
  cat >"$FIX/dev/plans/runs/STATUS-0.9.0.md" <<'EOF'
# 0.9.0 — status board

## 1. Current state

| | |
|---|---|
| **Slice in flight** | NONE. |
| **Immediate next action** | **Commission Slice 20** — the fixture's next action, recorded verbatim. |
EOF

  # A CLOSED board for a DIFFERENT release, with the banner behind YAML
  # frontmatter so it sits at line 6-15 (the shared-predicate window arm).
  cat >"$FIX/dev/plans/runs/STATUS-0.8.20.md" <<'EOF'
---
title: 0.8.20
status: CLOSED
target_release: 0.8.20
---

# 0.8.20 — status board

> **CLOSED — historical record.** Nothing lands into this board again.

| | |
|---|---|
| **Immediate next action** | Nothing — this release is closed. |
EOF

  cat >"$FIX/dev/plans/release-state-0.9.0.json" <<'EOF'
{
  "release": "0.9.0",
  "board": "dev/plans/runs/STATUS-0.9.0.md",
  "schema_version": 42,
  "ladder": [
    {"slice": 0,  "short": "X0",  "title": "design gate", "status": "LANDED",      "sha": "aaaa1111"},
    {"slice": 5,  "short": "R-A", "title": "the first",   "status": "LANDED",      "sha": "bbbb2222"},
    {"slice": 20, "short": "R-B", "title": "the next",    "status": "UNBLOCKED",   "sha": null},
    {"slice": 30, "short": "PUB", "title": "publish",     "status": "NOT_STARTED", "sha": null}
  ],
  "landed": [0, 5],
  "next_slice": 20,
  "remaining_ladder": [20, 30]
}
EOF

  # A stale state file for the CLOSED release: a hardcoded implementation that
  # reads release-state-0.8.20.json would report THIS, which is the bug.
  cat >"$FIX/dev/plans/release-state-0.8.20.json" <<'EOF'
{
  "release": "0.8.20",
  "board": "dev/plans/runs/STATUS-0.8.20.md",
  "schema_version": 24,
  "ladder": [{"slice": 0, "short": "X0", "title": "old", "status": "LANDED", "sha": "cccc3333"}],
  "landed": [0],
  "next_slice": null,
  "remaining_ladder": []
}
EOF

  cat >"$FIX/dev/steward/steward-ledger.jsonl" <<'EOF'
{"ts": "2026-01-01T00:00:00.000Z", "seq": 1, "kind": "decision", "summary": "fixture decision one"}
{"ts": "2026-01-02T00:00:00.000Z", "seq": 2, "kind": "decision", "summary": "fixture decision two"}
{"ts": "2026-01-03T00:00:00.000Z", "seq": 3, "kind": "note", "summary": "fixture note three"}
{"ts": "2026-01-04T00:00:00.000Z", "seq": 4, "kind": "note", "summary": "fixture note four"}
{"ts": "2026-01-05T00:00:00.000Z", "seq": 5, "kind": "note", "summary": "fixture note five"}
{"ts": "2026-01-06T00:00:00.000Z", "seq": 6, "kind": "ruling", "summary": "fixture ruling six"}
EOF

  cat >"$FIX/dev/todos-and-considerations-ledger.jsonl" <<'EOF'
{"ts": "2026-01-01T00:00:00.000Z", "seq": 1, "id": "TC-1", "kind": "todo", "status": "open", "summary": "one"}
{"ts": "2026-01-02T00:00:00.000Z", "seq": 2, "id": "TC-2", "kind": "todo", "status": "open", "summary": "two"}
{"ts": "2026-01-03T00:00:00.000Z", "seq": 3, "id": "TC-1", "kind": "todo", "status": "closed", "summary": "one, closed"}
{"ts": "2026-01-04T00:00:00.000Z", "seq": 4, "kind": "observation", "summary": "no id here"}
EOF

  printf '# Steward hand-off\n\nFixture hand-off body.\n' \
    >"$FIX/dev/plans/runs/STEWARD-SESSION-HANDOFF-2026-01-05-A.md"
  printf '# Steward hand-off\n\nOlder fixture hand-off body.\n' \
    >"$FIX/dev/plans/runs/STEWARD-SESSION-HANDOFF-2026-01-04-A.md"

  (cd "$FIX" \
     && git init -q \
     && git config user.email t@example.com \
     && git config user.name t \
     && git config commit.gpgsign false \
     && git add -A \
     && git commit -qm 'fixture: baseline briefing inputs')
}

# Runs the briefing with cwd inside $1. stdout and stderr are captured
# SEPARATELY so the byte cap is measured on the payload alone. TMPDIR is
# redirected to a per-run sandbox so "writes no file" can be asserted.
run_orient() {
  local dir="$1"
  local sandbox="$TMPROOT/sandbox"
  rm -rf "$sandbox"
  mkdir -p "$sandbox"
  set +e
  (cd "$dir" && TMPDIR="$sandbox" bash "$GATE") \
    >"$TMPROOT/out.txt" 2>"$TMPROOT/err.txt"
  RC=$?
  set -e
  OUT="$(cat "$TMPROOT/out.txt")"
  ERR="$(cat "$TMPROOT/err.txt")"
  BYTES="$(wc -c <"$TMPROOT/out.txt" | tr -d ' ')"
  SANDBOX_RESIDUE="$(find "$sandbox" -mindepth 1 | head -5)"
}

tree_snapshot() { find "$1" -path '*/.git' -prune -o -printf '%p %s\n' | sort; }

# --- Arm 0: BASELINE fixture is GREEN, under budget, every section present ---
# Without this, every RED arm below could be passing for an unrelated reason.
setup_fixture
run_orient "$FIX"
if [ "$RC" -eq 0 ]; then
  pass "baseline fixture — complete briefing (exit 0)"
else
  fail "arm 0 (baseline green): rc=$RC err=$ERR out=$OUT"
fi
if [ "$BYTES" -le 4096 ] && [ "$BYTES" -gt 0 ]; then
  pass "baseline fixture — payload is 1..4096 bytes ($BYTES)"
else
  fail "arm 0 (byte cap): $BYTES bytes"
fi
for needle in 'REPO ' 'WORKTREES' 'RELEASE 0.9.0' 'NEXT ACTION' 'LEDGER' 'TODOS' 'PR' 'HANDOFF'; do
  if grep -qF "$needle" <<<"$OUT"; then
    pass "baseline fixture — section present: $needle"
  else
    fail "arm 0: section missing: $needle; out=$OUT"
  fi
done

# --- Arm 9a: the next action is reproduced VERBATIM -------------------------
# A briefing that paraphrases the board's next action is a second narration of
# the fact the whole effort exists to de-duplicate.
if grep -qF '**Commission Slice 20** — the fixture'"'"'s next action, recorded verbatim.' <<<"$OUT"; then
  pass "the board's next-action cell is reproduced verbatim (markup included)"
else
  fail "arm 9a (verbatim next action): out=$OUT"
fi

# --- Arm 9b: landed slices AND their SHAs come from the state file ----------
if grep -qF 'aaaa1111' <<<"$OUT" && grep -qF 'bbbb2222' <<<"$OUT" \
   && grep -qE 'SCHEMA[^0-9]*42' <<<"$OUT"; then
  pass "landed slices + SHAs + SCHEMA are read from the release-state file"
else
  fail "arm 9b (state-file facts): out=$OUT"
fi
if grep -qF 'cccc3333' <<<"$OUT"; then
  fail "arm 9b: the CLOSED release's state file was read (hardcoded-release bug)"
else
  pass "the CLOSED release's stale state file is NOT read"
fi

# --- Arm 6: HARDCODED-RELEASE regression (correction 1) --------------------
# The named failure: a `0.8.20` literal keeps working right up until 0.8.20
# closes, then silently prints "nothing landed" forever. This repo's TC-37
# vacuous-pass class. The fixture's live board is 0.9.0 and its CLOSED board is
# 0.8.20, so only a filename-derived implementation can report 0.9.0.
if grep -qF 'RELEASE 0.9.0' <<<"$OUT" && ! grep -qF 'RELEASE 0.8.20' <<<"$OUT"; then
  pass "release is derived from the LIVE board filename (0.9.0), not hardcoded"
else
  fail "arm 6 (release from board filename): out=$OUT"
fi
if grep -qF 'STATUS-0.9.0.md' <<<"$OUT"; then
  pass "the briefing names the live board it read"
else
  fail "arm 6b (board named): out=$OUT"
fi

# --- Arm 8: board-CLOSED detector parity (correction 4) --------------------
# The shared predicate's window is `head -n 15`, because YAML frontmatter pushes
# the banner down (measured at line 10 on STATUS-0.8.9.1.md). Here the ONLY
# board is CLOSED with its banner at line 9. A `head -n 5` reimplementation
# would call it LIVE and the briefing would exit 0; the shared 15-line window
# sees the banner, finds ZERO live boards, and HARD-fails.
setup_fixture
rm -f "$FIX/dev/plans/runs/STATUS-0.9.0.md" "$FIX/dev/plans/release-state-0.9.0.json"
CLOSED_BANNER_LINE="$(grep -n 'CLOSED — historical record' "$FIX/dev/plans/runs/STATUS-0.8.20.md" | cut -d: -f1)"
if [ "$CLOSED_BANNER_LINE" -gt 5 ] && [ "$CLOSED_BANNER_LINE" -le 15 ]; then
  pass "arm 8 fixture really places the CLOSED banner at line 6-15 (line $CLOSED_BANNER_LINE)"
else
  fail "arm 8 fixture invalid: banner at line $CLOSED_BANNER_LINE"
fi
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qiE 'live (release )?board' <<<"$ERR"; then
  pass "a CLOSED board (banner at line 6-15) is skipped -> zero live boards HARD-fails"
else
  fail "arm 8 (closed-detector parity): rc=$RC err=$ERR"
fi

# --- Arm 8b: the predicate is SHARED, not reimplemented --------------------
# Structural, because behaviour parity alone would not stop the two windows
# drifting apart later.
if grep -qF 'head -n 15' "$CLOSED_LIB" \
   && grep -qF 'CLOSED — historical record' "$CLOSED_LIB"; then
  pass "scripts/lib/board-closed.sh holds the head -n 15 predicate"
else
  fail "arm 8b: the shared predicate lib is missing or has the wrong window"
fi
for caller in "$GATE" "$BOARD_CURRENCY"; do
  if grep -qF 'lib/board-closed.sh' "$caller"; then
    pass "$(basename "$caller") sources the shared board-CLOSED predicate"
  else
    fail "arm 8b: $(basename "$caller") does not use lib/board-closed.sh"
  fi
  # Guarded on the file existing, so a MISSING caller cannot pass this by
  # vacuously failing to match.
  if [ -f "$caller" ] && ! grep -qE 'head -n (5|15) "\$board"' "$caller"; then
    pass "$(basename "$caller") has no inline CLOSED-window reimplementation"
  else
    fail "arm 8b: $(basename "$caller") is missing or still inlines its own CLOSED window"
  fi
done

# --- Arm 7: worktrees dir is a SIBLING of the repo root (correction 2) -----
# `<root>-worktrees/` sits BESIDE the repo, not inside it. Resolving it as a
# child yields a silently empty section — the exact class of failure this
# briefing exists to make impossible. The fixture plants two ORPHAN checkout
# dirs (present on disk, absent from `git worktree list`) in the sibling dir;
# only a sibling-resolving implementation can name them.
setup_fixture
mkdir -p "$WTDIR/orphan-alpha" "$WTDIR/orphan-beta"
mkdir -p "$FIX/proj-worktrees/decoy-child"   # the wrong (child) location
run_orient "$FIX"
if [ "$RC" -eq 0 ] && grep -qF 'orphan-alpha' <<<"$OUT" && grep -qF 'orphan-beta' <<<"$OUT"; then
  pass "orphan checkout dirs in the SIBLING <root>-worktrees/ are found and named"
else
  fail "arm 7 (sibling worktrees dir): rc=$RC out=$OUT"
fi
# Requires a non-empty payload, so a script that produced NOTHING cannot pass
# this arm by simply failing to mention the decoy.
if [ -n "$OUT" ] && ! grep -qF 'decoy-child' <<<"$OUT"; then
  pass "the child-path decoy is NOT enumerated (resolution is a sibling, not a child)"
else
  fail "arm 7: empty payload, or the child-path decoy was enumerated (resolved as a child)"
fi
if grep -qF "$WTDIR" <<<"$OUT"; then
  pass "the briefing prints the resolved worktrees dir so a wrong path is visible"
else
  fail "arm 7: resolved worktrees dir not printed; out=$OUT"
fi

# --- Arm 3: ZERO landed slices -> HARD fail naming the section -------------
setup_fixture
perl -0777 -pi -e 's/"LANDED"/"NOT_STARTED"/g; s/"landed": \[0, 5\]/"landed": []/' \
  "$FIX/dev/plans/release-state-0.9.0.json"
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qi 'landed' <<<"$ERR"; then
  pass "zero landed slices -> HARD fail, and the message names the section"
else
  fail "arm 3 (zero landed slices): rc=$RC err=$ERR"
fi

# --- Arm 4: ZERO ledger entries -> HARD fail ------------------------------
setup_fixture
: >"$FIX/dev/steward/steward-ledger.jsonl"
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qi 'ledger' <<<"$ERR"; then
  pass "zero steward-ledger entries -> HARD fail, and the message names the section"
else
  fail "arm 4 (zero ledger entries): rc=$RC err=$ERR"
fi

# --- Arm 4b: ZERO foldable todos -> HARD fail -----------------------------
setup_fixture
cat >"$FIX/dev/todos-and-considerations-ledger.jsonl" <<'EOF'
{"ts": "2026-01-04T00:00:00.000Z", "seq": 1, "kind": "observation", "summary": "no id at all"}
EOF
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qi 'todo' <<<"$ERR"; then
  pass "a todos ledger that folds to zero ids -> HARD fail"
else
  fail "arm 4b (zero folded todos): rc=$RC err=$ERR"
fi

# --- Arm 5: NO STEWARD-SESSION-HANDOFF-* -> HARD fail ---------------------
setup_fixture
rm -f "$FIX"/dev/plans/runs/STEWARD-SESSION-HANDOFF-*.md
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qi 'hand-off\|handoff' <<<"$ERR"; then
  pass "no STEWARD-SESSION-HANDOFF-* found -> HARD fail, and the message names it"
else
  fail "arm 5 (missing hand-off): rc=$RC err=$ERR"
fi

# --- Arm 5b: the NEWEST hand-off is the one reported ----------------------
setup_fixture
run_orient "$FIX"
if grep -qF 'STEWARD-SESSION-HANDOFF-2026-01-05-A.md' <<<"$OUT" \
   && ! grep -qF 'STEWARD-SESSION-HANDOFF-2026-01-04-A.md' <<<"$OUT"; then
  pass "the NEWEST hand-off is reported (dated filename order), not an older one"
else
  fail "arm 5b (newest hand-off): out=$OUT"
fi

# --- Arm 5c: a board with no next-action row -> HARD fail -----------------
setup_fixture
perl -0777 -pi -e 's/\*\*Immediate next action\*\*/**Something else entirely**/' \
  "$FIX/dev/plans/runs/STATUS-0.9.0.md"
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qi 'next action' <<<"$ERR"; then
  pass "a board whose next-action row moved -> HARD fail, never a silent omission"
else
  fail "arm 5c (missing next-action row): rc=$RC err=$ERR"
fi

# --- Arm 5d: a missing release-state file -> HARD fail --------------------
# The other half of correction 1: when the live board rolls to a release that
# has no state file yet, the briefing must SAY SO, not print "nothing landed".
setup_fixture
rm -f "$FIX/dev/plans/release-state-0.9.0.json"
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qF 'release-state-0.9.0.json' <<<"$ERR"; then
  pass "a live board with no matching release-state file -> HARD fail naming the path"
else
  fail "arm 5d (missing state file): rc=$RC err=$ERR"
fi

# --- Arm 1: OUTPUT OVER 4 KB -> HARD fail (the cap is mechanical) ---------
# Enforced by the script itself, not only by this test: the cap is a promise the
# tool makes to the reader's context budget, so it has to be self-checking.
setup_fixture
BIG="$(head -c 6000 /dev/zero | tr '\0' 'x')"
perl -0777 -pi -e "s/the fixture's next action, recorded verbatim\\./$BIG/" \
  "$FIX/dev/plans/runs/STATUS-0.9.0.md"
run_orient "$FIX"
if [ "$RC" -ne 0 ] && grep -qiE 'budget|4096|bytes' <<<"$ERR"; then
  pass "a briefing over the 4096-byte cap HARD-fails and names the budget"
else
  fail "arm 1 (byte cap): rc=$RC bytes=$BYTES err=$ERR"
fi
if [ "$BYTES" -gt 4096 ]; then
  pass "arm 1 fixture really produced an over-cap payload ($BYTES bytes)"
else
  fail "arm 1 fixture invalid: payload was only $BYTES bytes"
fi

# --- Arm 2: WRITES NO FILE ------------------------------------------------
# Three independent proofs: the fixture tree is byte-identical before/after, the
# real checkout's porcelain status is unchanged, and the process's entire TMPDIR
# is empty afterwards (so the ledgerwatch cursor sandbox is not left behind).
setup_fixture
BEFORE="$(tree_snapshot "$TMPROOT/parent")"
run_orient "$FIX"
AFTER="$(tree_snapshot "$TMPROOT/parent")"
# Guarded on a run having actually HAPPENED: a script that cannot start writes
# no file either, and must not be able to green this arm.
if [ "$RC" -ne 0 ] || [ -z "$OUT" ]; then
  fail "arm 2 precondition: the briefing did not run (rc=$RC, ${BYTES} bytes) — 'writes nothing' is vacuous"
else
  pass "arm 2 precondition: the briefing really ran (rc=0, $BYTES bytes)"
fi
if [ "$BEFORE" = "$AFTER" ]; then
  pass "writes no file — the fixture tree is byte-identical before/after"
else
  fail "arm 2 (fixture tree mutated): $(diff <(printf '%s' "$BEFORE") <(printf '%s' "$AFTER") | head -10)"
fi
if [ -z "$SANDBOX_RESIDUE" ]; then
  pass "writes no file — the run's entire TMPDIR is empty afterwards"
else
  fail "arm 2 (TMPDIR residue): $SANDBOX_RESIDUE"
fi
if [ ! -d "$FIX/.ledgerwatch" ]; then
  pass "writes no file — no default .ledgerwatch cursor dir is created in the repo"
else
  fail "arm 2: the briefing created $FIX/.ledgerwatch (a real cursor would be disturbed)"
fi

# --- Arm 2b: .git/index IS BYTE-UNCHANGED, EVEN WITH A STALE STAT CACHE ------
# Arm 2's tree_snapshot PRUNES */.git, so it could never see this: `git status`
# refreshes a stale stat cache and REWRITES .git/index under .git/index.lock.
# That is a repo-metadata write by a script whose stated contract is that it
# writes nothing at all, and it can race a concurrent git command for the lock.
#
# Staling technique: rewrite every tracked file's mtime to a FIXED PAST instant.
# A fixed timestamp (rather than `touch` + sleep) puts the mtime well outside
# git's racily-clean one-second window with no wall-clock cost, so every cache
# entry is unambiguously stat-dirty.
#
# NON-VACUITY, and the RED witness carried inside the suite: a fixture where git
# had nothing to refresh would green this arm no matter what the briefing does.
# So the same staled fixture is COPIED (cp -a preserves the mtimes, so the copy
# is staled identically) and probed with a PLAIN, lock-taking `git status` — the
# exact call the briefing used to make. That copy's index MUST change. Only then
# does "the briefing left the index alone" mean anything.
setup_fixture
find "$FIX" -path '*/.git' -prune -o -type f -print0 \
  | xargs -0 touch -d '2026-01-01 00:00:00'

WITNESS="$TMPROOT/parent/witness"
cp -a "$FIX" "$WITNESS"
WITNESS_BEFORE="$(sha256sum "$WITNESS/.git/index" | cut -d' ' -f1)"
git -C "$WITNESS" status --porcelain --untracked-files=all >/dev/null 2>&1 || true
WITNESS_AFTER="$(sha256sum "$WITNESS/.git/index" | cut -d' ' -f1)"
if [ "$WITNESS_BEFORE" != "$WITNESS_AFTER" ]; then
  pass "arm 2b witness: the stat cache really is stale — a plain \`git status\` rewrites .git/index"
else
  fail "arm 2b fixture invalid: a plain lock-taking \`git status\` did NOT rewrite .git/index, so this arm proves nothing"
fi
rm -rf "$WITNESS"

INDEX_BEFORE="$(sha256sum "$FIX/.git/index" | cut -d' ' -f1)"
INDEX_META_BEFORE="$(stat -c '%s %.9Y' "$FIX/.git/index")"
run_orient "$FIX"
INDEX_AFTER="$(sha256sum "$FIX/.git/index" | cut -d' ' -f1)"
INDEX_META_AFTER="$(stat -c '%s %.9Y' "$FIX/.git/index")"
# Guarded on the run having HAPPENED: a briefing that cannot start also writes
# no index, and must not be able to green this.
if [ "$RC" -eq 0 ] && [ -n "$OUT" ]; then
  pass "arm 2b precondition: the briefing still completes on a stat-stale checkout (rc=0)"
else
  fail "arm 2b precondition: the briefing did not run (rc=$RC, $BYTES bytes) — the index assertion would be vacuous; err=$ERR"
fi
if [ "$INDEX_BEFORE" = "$INDEX_AFTER" ]; then
  pass "writes no file — .git/index is BYTE-IDENTICAL across a run with a deliberately staled stat cache"
else
  fail "arm 2b: the briefing REWROTE .git/index ($INDEX_BEFORE -> $INDEX_AFTER) — it must run git with optional locks disabled"
fi
if [ "$INDEX_META_BEFORE" = "$INDEX_META_AFTER" ]; then
  pass "writes no file — .git/index size+mtime are unchanged too (NANOSECOND mtime: it was not even reopened)"
else
  fail "arm 2b: .git/index metadata moved ($INDEX_META_BEFORE -> $INDEX_META_AFTER)"
fi
if [ ! -e "$FIX/.git/index.lock" ]; then
  pass "writes no file — no .git/index.lock is left behind"
else
  fail "arm 2b: the briefing left a .git/index.lock behind"
fi

# --- Arm 2c: a HELD index lock does not perturb the briefing ----------------
# HONEST SCOPE: measured on git 2.43, a plain `git status` tolerates a held
# optional lock (it declines to write and still exits 0), so this arm is NOT a
# RED witness for the fix — it is a forward guard. It is worth its cost because
# the briefing runs under `set -euo pipefail`, where any future git probe that
# DOES insist on the lock (or a git version that starts erroring) would abort the
# whole cold-start briefing rather than degrade. With optional locks disabled the
# lock is never contended for at all.
setup_fixture
find "$FIX" -path '*/.git' -prune -o -type f -print0 \
  | xargs -0 touch -d '2026-01-01 00:00:00'
: >"$FIX/.git/index.lock"
LOCKED_INDEX_BEFORE="$(sha256sum "$FIX/.git/index" | cut -d' ' -f1)"
run_orient "$FIX"
LOCKED_INDEX_AFTER="$(sha256sum "$FIX/.git/index" | cut -d' ' -f1)"
if [ "$RC" -eq 0 ] && [ -n "$OUT" ]; then
  pass "a concurrently HELD .git/index.lock does not abort the briefing (rc=0)"
else
  fail "arm 2c: a held index lock broke the briefing (rc=$RC); err=$ERR"
fi
if [ "$LOCKED_INDEX_BEFORE" = "$LOCKED_INDEX_AFTER" ]; then
  pass "a held index lock run still leaves .git/index byte-identical"
else
  fail "arm 2c: .git/index changed while its lock was held by another process"
fi
if [ -e "$FIX/.git/index.lock" ]; then
  pass "the other process's .git/index.lock is still there — the briefing never touched it"
else
  fail "arm 2c: the briefing REMOVED another process's .git/index.lock"
fi
rm -f "$FIX/.git/index.lock"

# --- Arm 2d: structural — EVERY git probe disables optional locks -----------
# Behavioural arm 2b only covers the git calls on the paths it exercises. This
# arm is the drift guard: a new `$(git ...)` added later without the flag would
# silently reintroduce the index write. Counts every command substitution /
# process substitution that starts a git command and requires all of them to
# carry --no-optional-locks, plus the exported env var that covers git run by
# CHILD processes (gh, python3).
GIT_CALLS_ALL="$(grep -coE '(\$\(|<\()git ' "$GATE" || true)"
GIT_CALLS_SAFE="$(grep -coE '(\$\(|<\()git --no-optional-locks ' "$GATE" || true)"
if [ "$GIT_CALLS_ALL" -ge 8 ]; then
  pass "arm 2d precondition: the briefing really does shell out to git ($GIT_CALLS_ALL call sites)"
else
  fail "arm 2d precondition: found only $GIT_CALLS_ALL git call sites — the count regex has drifted, so this arm proves nothing"
fi
if [ "$GIT_CALLS_ALL" -eq "$GIT_CALLS_SAFE" ]; then
  pass "every git call site ($GIT_CALLS_SAFE/$GIT_CALLS_ALL) passes --no-optional-locks"
else
  fail "arm 2d: $((GIT_CALLS_ALL - GIT_CALLS_SAFE)) git call site(s) can still take .git/index.lock: $(grep -nE '(\$\(|<\()git ' "$GATE" | grep -v -- '--no-optional-locks')"
fi
if grep -qE '^export GIT_OPTIONAL_LOCKS=0$' "$GATE"; then
  pass "GIT_OPTIONAL_LOCKS=0 is exported, so git in CHILD processes (gh, python3) cannot lock either"
else
  fail "arm 2d: the briefing must export GIT_OPTIONAL_LOCKS=0"
fi

# --- Arm 10: the REAL repo is green (the regression half) ----------------
REAL_BEFORE="$(git -C "$REPO_ROOT" status --porcelain --untracked-files=all)"
run_orient "$REPO_ROOT"
REAL_AFTER="$(git -C "$REPO_ROOT" status --porcelain --untracked-files=all)"
if [ "$RC" -eq 0 ]; then
  pass "the real checkout produces a complete briefing (exit 0)"
else
  fail "arm 10 (real repo green): rc=$RC err=$ERR out=$OUT"
fi
if [ "$BYTES" -le 4096 ] && [ "$BYTES" -gt 0 ]; then
  pass "the real checkout's briefing is within the 4096-byte cap ($BYTES bytes)"
else
  fail "arm 10 (real byte cap): $BYTES bytes"
fi
if [ "$REAL_BEFORE" = "$REAL_AFTER" ]; then
  pass "the real checkout's porcelain status is unchanged by the briefing"
else
  fail "arm 10: the briefing mutated the real checkout"
fi
if [ -z "$SANDBOX_RESIDUE" ]; then
  pass "the real-repo run leaves no TMPDIR residue either"
else
  fail "arm 10 (TMPDIR residue): $SANDBOX_RESIDUE"
fi

# --- Arm 11: the CI job is ALWAYS-ON --------------------------------------
# Same reasoning as release-state-views: the inputs this briefing reads (the
# board, the state file, the ledgers, the hand-offs) are DOCS, and the
# `markdownlint` job's docs_only fast path excludes the heavy `verify` job that
# would otherwise run this suite. A guard that cannot run on the push that
# breaks it is decorative.
CI_JOB_BLOCK="$(awk '
  /^  steward-orient:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"
if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a steward-orient job"
else
  fail "ci.yml has no steward-orient job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/tests/test_steward_orient.sh'; then
  pass "the CI job runs THIS suite (not a reimplementation)"
else
  fail "the CI job must invoke scripts/tests/test_steward_orient.sh"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*(if|needs):'; then
  fail "the steward-orient job must be ALWAYS-ON (no if:/needs:); block: $CI_JOB_BLOCK"
else
  pass "the steward-orient job is always-on (no if:, no needs:, not docs_only-gated)"
fi

# --- Arm 12: the documented exemptions are documented ---------------------
# Correction 3 requires each legitimately-empty section to be an explicit
# decision on the record, not an accident of implementation.
if grep -qiE 'exempt' "$GATE"; then
  pass "the script header records the zero-result exemptions explicitly"
else
  fail "arm 12: the script does not document its zero-result exemptions"
fi
if grep -qF -- '--dry-run' "$GATE" && grep -qF -- '--state-dir' "$GATE"; then
  pass "ledger reads go through ledgerwatch with an explicit --state-dir + peek semantics"
else
  fail "arm 12: ledger reads must use ledgerwatch --state-dir with --dry-run peek semantics"
fi
if grep -qF -- '--project' "$GATE"; then
  pass "the todos fold uses ledgerwatch --project (not a hand-rolled fold)"
else
  fail "arm 12: the todos fold must use ledgerwatch --project"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll steward-orient tests passed\n'
