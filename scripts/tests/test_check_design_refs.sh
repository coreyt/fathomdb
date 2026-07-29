#!/usr/bin/env bash
# scripts/tests/test_check_design_refs.sh — TC-92 recurrence guard
# (DOC-HYGIENE-3): A REQUIREMENT ID MUST HAVE DESIGN COVERAGE AT THE MOMENT IT
# IS MINTED.
#
# ---------------------------------------------------------------------------
# THE INCIDENT (measured, at the Slice 22 commission)
# ---------------------------------------------------------------------------
# `scripts/commission-manifest.sh 0.8.20 22` exited 1 with
#     ZERO design docs matched 0.8.20 Slice 22 (tokens: TC-67, TC-68, R-20-VC)
# — its TC-37 vacuous-pass guard, working exactly as designed. THE ALARM WAS
# REAL AND THE DIAGNOSIS WAS NOT "the design does not exist": the design of
# record existed for every leg and was found in minutes. The ids were minted
# 2026-07-27, long after those documents were written, so a LITERAL TOKEN SCAN
# CANNOT SEE A DEPENDENCY THAT RUNS BACKWARD IN TIME.
#
# The consequence is bimodal. Zero matches hard-fails and cannot be commissioned
# — loud, recoverable, and what happened to Slice 22. ONE weak incidental match
# emits a brief whose required reading is thin but LOOKS complete — quiet, and
# worse. Slice 21 was the second shape: it matched exactly one doc, on TC-71,
# and the manifest honestly printed `NO design doc mentions: ac_002, TC-57,
# R-20-CR`. That line was correct and was easy to read past.
#
# ---------------------------------------------------------------------------
# WHAT IS ALREADY FIXED, AND WHAT IS NOT
# ---------------------------------------------------------------------------
# Slice 22 was unblocked by hand-annotating five docs with a "Requirement
# traceability" blockquote. That approach is SUPERSEDED by `design_refs` (a
# curated per-ladder-entry citation list, d30ef52f) and is not the remedy here.
#
# THE UNFIXED HALF IS THE DISCIPLINE GAP: nothing requires a back-link at the
# moment a requirement id is minted. That is remedy (a) in TC-92, and this suite
# gates the mechanical version of it — `scripts/check-design-refs.sh`, wired into
# the pre-commit hook so it fires exactly when the state file or the design tier
# is staged.
#
# ---------------------------------------------------------------------------
# PREDICATE UNDER TEST
# ---------------------------------------------------------------------------
# For every ladder entry in every `dev/plans/release-state-*.json`, every token
# the entry declares must have design coverage: a design doc that whole-token
# matches it, or a curated `design_refs` document that does. Anything else must
# appear in the check's own FROZEN BASELINE EXEMPTION table, and anything not in
# that table is a hard failure — on a LANDED slice or a future one alike.
#
# IT IS A RATCHET, NOT A SNAPSHOT. Arm 11 pins that it is NOT implemented as
# "skip LANDED slices": that would let a new landed gap through, which is the
# exact quiet failure mode TC-92 is about.
#
# NO TOKEN DERIVATION OF ITS OWN (arm 6). The check DRIVES
# `scripts/commission-manifest.sh` and reads its coverage report, so the two can
# never disagree about what a slice's tokens are. Arm 7 pins the manifest's
# stdout byte-identical to the base commit for all 14 real ladder slices, because
# the `design_refs` mechanism's whole safety argument rests on that byte-identity.
#
# LOCAL-ONLY BY DESIGN (arm 9). CI does not invoke `scripts/hooks/pre-commit` and
# does not auto-discover `scripts/tests/*`. Every check in
# `scripts/preflight.sh --landing` is deliberately paired with a mirrored CI job;
# this one is not, so it must stay out of preflight and out of agent-test.sh.
#
# Isolation: the failing arms run in a throwaway git repo under mktemp -d, built
# around `scripts/tests/fixtures/release-state-9.9.9-design-refs.json`. Nothing
# here writes into the real checkout, and the real `release-state-0.8.20.json` is
# never mutated (arm 12).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="${GATE_UNDER_TEST:-$REPO_ROOT/scripts/check-design-refs.sh}"
GEN="$REPO_ROOT/scripts/commission-manifest.sh"
PRE_COMMIT="$REPO_ROOT/scripts/hooks/pre-commit"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"
FIXTURE_STATE="$SCRIPT_DIR/fixtures/release-state-9.9.9-design-refs.json"

# The commit this unit was cut from. The manifest's stdout for every real ladder
# slice must still be byte-identical to it (arm 7). If the manifest is ever
# changed deliberately, this pin moves in the same commit as the change.
BASE_SHA="2671346dff75c92c4486e674886fc2c61cfb096b"
REAL_SLICES=(0 5 10 15 20 21 22 23 25 30 31 32 33 40)

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# TC-128 CANARY, captured BEFORE anything runs and asserted by arm 18 at the end.
# The shared common dir is where `core.bare` lives and it is shared by every
# linked worktree, so a stray write from anything in this suite shows up here.
# This exists because a `git init` in the gate reached the PRIMARY CHECKOUT twice
# on 2026-07-29 and set `bare = true`, and no test noticed.
REAL_COMMON_DIR="$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null)"
case "$REAL_COMMON_DIR" in
  /*) : ;;
  *)  REAL_COMMON_DIR="$REPO_ROOT/$REAL_COMMON_DIR" ;;
esac
# `cksum` is POSIX; `md5sum` is GNU and would hash nothing on BSD, making this
# canary compare an empty string against an empty string and pass regardless.
# The raw bytes are kept so arm 18 can assert they were actually read.
COMMON_CFG_BEFORE="$(cat "$REAL_COMMON_DIR/config" 2>/dev/null || true)"
COMMON_BEFORE="$(printf '%s' "$COMMON_CFG_BEFORE" | cksum)"
BARE_BEFORE="$(git -C "$REPO_ROOT" config --get core.bare 2>/dev/null || echo unset)"

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

# A minimal but REAL fixture release. It carries exactly the infrastructure
# `commission-manifest.sh` cites (every path it names is existence-checked, so a
# missing one fails CHECK 1 rather than the arm under test) plus a three-slice
# ladder that spans the three coverage outcomes:
#   Slice 0  — every token whole-token matched by a design doc          (COVERED)
#   Slice 5  — LANDED, and TC-999 is matched by nothing                 (GAP)
#   Slice 10 — TC-998/R-9-C reached only by a curated doc OUTSIDE the
#              scanned roots                                            (CURATED)
setup_fixture() {
  rm -rf "$FIX"
  mkdir -p "$FIX/dev/plans/runs/codex" "$FIX/dev/plans/prompts" "$FIX/dev/design" \
           "$FIX/dev/adr" "$FIX/dev/interfaces" "$FIX/scripts" "$FIX/src/conformance" \
           "$FIX/src/rust/crates/fathomdb-schema/src"
  cp "$GEN" "$FIX/scripts/commission-manifest.sh"
  chmod +x "$FIX/scripts/commission-manifest.sh"
  cp "$GATE" "$FIX/scripts/check-design-refs.sh" 2>/dev/null || true
  chmod +x "$FIX/scripts/check-design-refs.sh" 2>/dev/null || true
  (cd "$FIX" && git init -q && git config user.email t@example.com && git config user.name t)

  cp "$FIXTURE_STATE" "$FIX/dev/plans/release-state-9.9.9.json"

  cat >"$FIX/dev/plans/plan-9.9.9.md" <<'EOF'
# 9.9.9 — Plan

## 1. Goal & scope

## 2. Decisions already taken (do NOT re-litigate)

## 3. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

## 4. Slice ladder (mod-5)

## 5. Reserved-gap policy

## 6. Cross-cutting DoD (X0/X1/X2/X3 — bind EVERY slice)

## 7. Prerequisites

## 9. Immediate next slice

## 10. Decisions taken (recorded)

## 11. Open HITL decision queue
EOF

  printf '# Board\n' >"$FIX/dev/plans/runs/board.md"
  printf '# Master\n' >"$FIX/dev/plans/master.md"
  printf 'transcripts land here\n' >"$FIX/dev/plans/runs/codex/README.md"

  cat >"$FIX/dev/design/orchestration.md" <<'EOF'
# Orchestration

## 1.6 Preflight gate (run before every worktree spawn)

## 10. Hard rules summary

## 11. Worktree cleanup (after phase family closes)

## 8. Closure output.json schema (per slice)
EOF

  cat >"$FIX/dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md" <<'EOF'
# Release Orchestrator hand-off

## 0. Hard preflight checks (CURRENT — apply every session)

## 6. Orchestration mechanics
EOF

  cat >"$FIX/dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md" <<'EOF'
# LBO template

## STEP 0 — Isolate (fail-fast)

## Escalate to LBS (`SendMessage`) when

## Closure output
EOF

  cat >"$FIX/dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md" <<'EOF'
# Slice template

## 6. Scope discipline — stay in-slice

## 7. When something goes wrong — detect, log, recover (do not hide)

## 9. Closure — write `output.json` LAST (orchestration §8 schema)
EOF

  cat >"$FIX/scripts/preflight.sh" <<'EOF'
#!/usr/bin/env bash
# --landing hard-fails on the primary checkout
EOF
  chmod +x "$FIX/scripts/preflight.sh"

  printf '# Acceptance\n\nAC-900\n' >"$FIX/dev/acceptance.md"
  printf '# AGENTS\n\nTDD is mandatory.\n' >"$FIX/AGENTS.md"
  printf '{"allow": []}\n' >"$FIX/src/conformance/governed-surface-allowlist.json"
  printf '{"pin": "x"}\n' >"$FIX/scripts/governed-surface-pin.json"
  printf 'pub const SCHEMA_VERSION: i64 = 42;\n' \
    >"$FIX/src/rust/crates/fathomdb-schema/src/lib.rs"

  # Slice 0's design of record: both of its tokens appear, whole-token.
  cat >"$FIX/dev/design/widget-design.md" <<'EOF'
---
status: ACTIVE
---

# Widget design

widget_readiness is defined here, and R-9-A is its requirement id.
EOF

  # Slice 5's design of record covers R-9-B and says NOTHING about TC-999 —
  # the reserved-gap shape: an id minted after the document was written.
  cat >"$FIX/dev/design/gap-leg-design.md" <<'EOF'
---
status: ACTIVE
---

# The gap leg

R-9-B is designed here.
EOF

  # OUTSIDE the three scanned roots, so only the curated citation can reach it.
  cat >"$FIX/dev/plans/prompts/CURATED-OUT-OF-ROOT.md" <<'EOF'
# A curated document the scan cannot reach

R-9-C and TC-998 are both designed here.
EOF

  # A base commit, so `git diff --cached` and the index/worktree arms below have
  # a HEAD to differ from. Without it every path reads as freshly added and
  # "staged vs unstaged" cannot be posed at all.
  (cd "$FIX" && git add -A && git commit -qm base)
}

run_gate() {
  set +e
  OUT="$(cd "$FIX" && bash ./scripts/check-design-refs.sh "$@" 2>&1)"
  RC=$?
  set -e
}

# --- Arm 0: THE REAL REPO IS GREEN TODAY -----------------------------------
# The ratchet's floor. If this ever goes red without a deliberate change, a new
# requirement id was minted with no design of record and nobody back-linked it.
set +e
REAL_OUT="$(cd "$REPO_ROOT" && bash scripts/check-design-refs.sh 2>&1)"
REAL_RC=$?
set -e
if [ "$REAL_RC" -eq 0 ]; then
  pass "the real repo is GREEN — every declared token has design coverage or a frozen exemption"
else
  fail "arm 0 (real repo): rc=$REAL_RC out=$REAL_OUT"
fi

# --- Arm 1: THE TWO FROZEN EXEMPTIONS ARE LIVE ------------------------------
# Measured at the base commit: exactly two ladder entries carry a token no design
# doc mentions — Slice 20 / TC-45 and Slice 21 / ac_002. Both are LANDED, the
# state file is the Steward's to edit and not this unit's, and adding a
# "Requirement traceability" blockquote is the superseded anti-pattern. So both
# are enumerated, and the check must SAY it is exempting them rather than pass
# silently: a silent pass is indistinguishable from no coverage gap at all.
if grep -q 'TC-45' <<<"$REAL_OUT" && grep -q 'ac_002' <<<"$REAL_OUT" \
   && grep -qiE 'exempt' <<<"$REAL_OUT"; then
  pass "the report NAMES both frozen exemptions (Slice 20 TC-45, Slice 21 ac_002)"
else
  fail "arm 1 (exemptions reported): out=$REAL_OUT"
fi

# --- Arm 1b: each exemption carries a justification and a TC-92 pointer -----
# An exemption with no reason recorded is indistinguishable from a bug, and the
# next reader has no way to judge whether it may be removed.
if grep -q 'TC-92' "$GATE" \
   && awk '/TC-45/{found=1} END{exit !found}' "$GATE" \
   && awk '/ac_002/{found=1} END{exit !found}' "$GATE"; then
  pass "the exemption table lives in the check itself and points at TC-92"
else
  fail "arm 1b (exemption provenance): $GATE does not enumerate them with a TC-92 pointer"
fi

# --- Arm 2: NON-VACUITY — a fully covered slice passes ----------------------
# Without this every RED arm below could be passing because the check refuses
# everything it is shown.
setup_fixture
run_gate --release 9.9.9 --slice 0
if [ "$RC" -eq 0 ]; then
  pass "non-vacuity — a slice whose every token is whole-token matched passes"
else
  fail "arm 2 (covered slice): rc=$RC out=$OUT"
fi

# --- Arm 3: THE SYNTHETIC THIRD GAP FAILS ----------------------------------
# A token in the ladder that no design doc mentions and no curation reaches. This
# is the case the frozen table must NOT silently absorb.
setup_fixture
run_gate --release 9.9.9 --slice 5
if [ "$RC" -eq 1 ] && grep -q 'TC-999' <<<"$OUT" && grep -qE 'Slice 5|slice 5' <<<"$OUT"; then
  pass "a NEW uncovered token fails, naming the slice and the token"
else
  fail "arm 3 (synthetic gap): rc=$RC out=$OUT"
fi

# --- Arm 3b: the failure names BOTH ways to fix it -------------------------
# A gate that says "wrong" without saying "do this" makes the fix a guess, and a
# guess is how the superseded hand-annotation gets reached for again.
if grep -q 'design_refs' <<<"$OUT" && grep -qiE 'back-link' <<<"$OUT"; then
  pass "the failure names both remedies — back-link the id, or curate design_refs"
else
  fail "arm 3b (actionable remedy): out=$OUT"
fi

# --- Arm 4: A CURATED DOC OUTSIDE THE SCANNED ROOTS PROVIDES COVERAGE ------
# `design_refs` reaches anything in the checkout, including tiers the manifest's
# walker does not cover. If curation could not satisfy the check, remedy (b)
# would be a remedy in name only and the check would false-positive on exactly
# the slices the design_refs mechanism was built for.
setup_fixture
run_gate --release 9.9.9 --slice 10
if [ "$RC" -eq 0 ] && grep -q 'CURATED-OUT-OF-ROOT.md' <<<"$OUT"; then
  pass "a curated design_ref outside the scanned roots covers its tokens, and is named"
else
  fail "arm 4 (curated coverage): rc=$RC out=$OUT"
fi

# --- Arm 5: THE DEFAULT IS A SWEEP, and one gap fails the sweep ------------
# The pre-commit wiring passes no slice, so the sweep is the shape that actually
# runs. A sweep that reported the gap but exited 0 would be the TC-37 class.
setup_fixture
run_gate
if [ "$RC" -eq 1 ] && grep -q 'TC-999' <<<"$OUT"; then
  pass "the default sweep covers the whole ladder and fails on the one gap in it"
else
  fail "arm 5 (sweep): rc=$RC out=$OUT"
fi

# --- Arm 6: NO TOKEN DERIVATION OF ITS OWN ---------------------------------
# TC-100 is the standing proof that how a token is matched is subtle (`"C-1" in
# "TC-15"` is True). A second implementation of token derivation would drift from
# the manifest's, and the two disagreeing about what a slice's design coverage is
# would be worse than either alone. The check must DRIVE the manifest, and it
# must refuse to run if the manifest's whole-token boundary construction has
# changed under it.
if grep -q 'commission-manifest.sh' "$GATE"; then
  pass "the check drives commission-manifest.sh rather than re-deriving tokens"
else
  fail "arm 6 (no drift): $GATE does not invoke scripts/commission-manifest.sh"
fi

setup_fixture
# Break the manifest's boundary construction. A check that kept going here would
# be silently using a different notion of "the token occurs" from the tool whose
# report it consumes.
python3 - "$FIX/scripts/commission-manifest.sh" <<'PY'
import sys
p = sys.argv[1]
with open(p, encoding="utf-8") as fh:
    text = fh.read()
text = text.replace('(?<![0-9A-Za-z_])', '(?<![0-9A-Za-z])')
with open(p, "w", encoding="utf-8") as fh:
    fh.write(text)
PY
run_gate --release 9.9.9 --slice 0
if [ "$RC" -eq 2 ] && grep -qiE 'boundary|drift' <<<"$OUT"; then
  pass "the check REFUSES TO RUN (exit 2) when the manifest's token boundary changes"
else
  fail "arm 6b (drift detector): rc=$RC out=$OUT"
fi

# --- Arm 7: THE MANIFEST'S OUTPUT IS BYTE-IDENTICAL TO THE BASE COMMIT -----
# `design_refs` is safely additive only because the eight uncurated slices render
# exactly as they did before it existed. This unit adds a CONSUMER of the
# manifest; it must not become an editor of it.
if git -C "$REPO_ROOT" cat-file -e "$BASE_SHA:scripts/commission-manifest.sh" 2>/dev/null; then
  BASE_GEN="$TMPROOT/commission-manifest.base.sh"
  git -C "$REPO_ROOT" show "$BASE_SHA:scripts/commission-manifest.sh" >"$BASE_GEN"
  chmod +x "$BASE_GEN"
  DIFFS=""
  for s in "${REAL_SLICES[@]}"; do
    set +e
    A="$(cd "$REPO_ROOT" && bash "$BASE_GEN" 0.8.20 "$s" 2>/dev/null)"
    B="$(cd "$REPO_ROOT" && bash "$GEN" 0.8.20 "$s" 2>/dev/null)"
    set -e
    [ "$A" = "$B" ] || DIFFS="$DIFFS $s"
  done
  if [ -z "$DIFFS" ]; then
    pass "commission-manifest stdout is byte-identical to $BASE_SHA for all 14 ladder slices"
  else
    fail "arm 7 (byte-identity): slices differ:$DIFFS"
  fi
else
  fail "arm 7 (byte-identity): base commit $BASE_SHA is unreachable; the pin cannot be checked"
fi

# --- Arm 8: WIRED INTO THE TRACKED pre-commit HOOK -------------------------
# Remedy (a) — "back-link at mint time" — made mechanical. The hook is the only
# moment at which the minting agent still has the knowledge in hand.
if grep -q 'check-design-refs.sh' "$PRE_COMMIT"; then
  pass "scripts/hooks/pre-commit runs the design-refs gate"
else
  fail "arm 8 (hook wiring): scripts/hooks/pre-commit does not run the gate"
fi

# --- Arm 8b: the hook gates it on the staged path set ----------------------
# It must fire when the commit stages a release-state file or anything under the
# design tier, and cost unrelated commits nothing. A gate that taxes every commit
# in the repo gets turned off, and a gate that is turned off is worse than none.
#
# The trigger set lives in ONE place — the gate's own `--staged-only` branch —
# rather than being restated in the hook, so the two can never disagree about
# when it fires. The hook is asserted to pass the flag; the gate is asserted to
# own the pattern.
if grep -q 'check-design-refs.sh --staged-only' "$PRE_COMMIT" \
   && grep -qE 'release-state.*json.*\|.*dev/design|dev/design.*\|.*release-state' "$GATE"; then
  pass "the hook passes --staged-only and the gate owns the release-state/dev/design trigger set"
else
  fail "arm 8b (path gating): the hook does not gate the gate on a staged path set"
fi

# --- Arm 9: LOCAL-ONLY — it is NOT wired into CI or the landing gate -------
# Every check in `preflight.sh --landing` is deliberately paired with a mirrored
# CI job. This one has no mirror, so putting it there would make `--landing` a
# claim CI does not back. `.github/**` is Slice 40's, and this unit touches none
# of it.
CI_HITS="$(grep -rl 'check-design-refs' "$REPO_ROOT/.github" 2>/dev/null || true)"
# NON-VACUITY: with no gate in the tree the absence is trivially true.
if [ ! -f "$GATE" ]; then
  fail "arm 9 (CI-inert): $GATE does not exist, so the assertion is vacuous"
elif [ -z "$CI_HITS" ] \
   && ! grep -q 'check-design-refs' "$PREFLIGHT" \
   && ! grep -q 'check-design-refs' "$AGENT_TEST"; then
  pass "CI-inert — not in .github/**, not in preflight --landing, not in agent-test.sh"
else
  fail "arm 9 (CI-inert): reached CI or the landing gate: [$CI_HITS]"
fi

# --- Arm 10: THE HOOK EARLY-EXITS AT ~ZERO COST ON UNRELATED WORK ----------
# Measured against the real hook with one unrelated file staged. The budget is
# generous on purpose — it is here to catch the gate running unconditionally, not
# to police milliseconds.
setup_fixture
cp "$PRE_COMMIT" "$FIX/pre-commit-under-test"
printf 'unrelated\n' >"$FIX/README.md"
(cd "$FIX" && git add README.md)
START="$(date +%s%N)"
set +e
(cd "$FIX" && bash ./scripts/check-design-refs.sh --staged-only >/dev/null 2>&1)
RC=$?
set -e
END="$(date +%s%N)"
ELAPSED_MS=$(( (END - START) / 1000000 ))
if [ "$RC" -eq 0 ] && [ "$ELAPSED_MS" -lt 1500 ]; then
  pass "--staged-only exits 0 in ${ELAPSED_MS}ms when no relevant path is staged"
else
  fail "arm 10 (early exit): rc=$RC elapsed=${ELAPSED_MS}ms"
fi

# --- Arm 11: NOT IMPLEMENTED AS "SKIP LANDED SLICES" -----------------------
# The obvious cheap way to make the baseline green is to exempt everything
# already landed. That would let a NEW landed gap through — the quiet failure
# mode TC-92 is actually about. Slice 5 in the fixture is LANDED and must still
# fail (arm 3 proved it does); this arm pins that the source contains no
# status-based bypass.
# Two assertions, one behavioural and one structural, because a prose grep for
# the word LANDED would fire on the comment that EXPLAINS the ban.
#
# Behavioural: the fixture's gap slice is LANDED, and arm 3 already showed it
# fails. Restate the premise here so the arm cannot pass because the fixture
# drifted to a non-landed status.
setup_fixture
FIX_STATUS="$(python3 -c 'import json,sys
s=json.load(open(sys.argv[1]))
print([e["status"] for e in s["ladder"] if e["slice"]==5][0])' "$FIX/dev/plans/release-state-9.9.9.json")"
run_gate --release 9.9.9 --slice 5
# Structural: the check never reads a ladder entry's `status` at all, so it
# CANNOT branch on LANDED — a stronger statement than "no skip appears".
if [ ! -f "$GATE" ]; then
  fail "arm 11 (no LANDED bypass): $GATE does not exist, so the assertion is vacuous"
elif [ "$FIX_STATUS" = "LANDED" ] && [ "$RC" -eq 1 ] \
     && ! grep -qE '"status"|get\("status"\)|\.status' "$GATE"; then
  pass "no LANDED bypass — the check never reads a ladder status, and a LANDED gap still fails"
else
  fail "arm 11 (no LANDED bypass): fixture status=$FIX_STATUS rc=$RC; \
$GATE may branch on ladder status"
fi

# --- Arm 12: READ-ONLY — the real state file is never touched --------------
REAL_STATE="$REPO_ROOT/dev/plans/release-state-0.8.20.json"
BEFORE="$(git -C "$REPO_ROOT" hash-object "$REAL_STATE")"
set +e
(cd "$REPO_ROOT" && bash scripts/check-design-refs.sh >/dev/null 2>&1)
RC=$?
set -e
AFTER="$(git -C "$REPO_ROOT" hash-object "$REAL_STATE")"
# NON-VACUITY: a gate that does not exist also changes nothing. rc=127 fails.
if [ "$RC" -ne 127 ] && [ "$BEFORE" = "$AFTER" ]; then
  pass "read-only — dev/plans/release-state-0.8.20.json is byte-identical after a run"
else
  fail "arm 12 (read-only): rc=$RC before=$BEFORE after=$AFTER"
fi

# ===========================================================================
# codex §9 fix round 1 — `--staged-only` must judge the INDEX, and must not let
# a DELETION slip past its trigger.
# ===========================================================================

# --- Arm 13: A STAGED DELETION TRIGGERS THE SWEEP ---------------------------
# [P2] The trigger scan used `--diff-filter=ACMR`, which EXCLUDES deletions, so a
# commit that removed the only design document naming a token exited at the
# early-out and the coverage was deleted straight through the gate. This arm
# removes the sole design of record for Slice 0's tokens and stages the removal:
# the gate must both FIRE and FAIL. It exercises the snapshot too — the deleted
# file is absent from the index, so the sweep sees the loss.
setup_fixture
(cd "$FIX" && git rm -q dev/design/widget-design.md)
run_gate --staged-only --release 9.9.9 --slice 0
if [ "$RC" -eq 1 ] && grep -qE 'R-9-A|widget_readiness' <<<"$OUT"; then
  pass "a staged DELETION of the only design doc triggers the sweep and fails"
else
  fail "arm 13 (deletion triggers): rc=$RC out=$OUT"
fi

# --- Arm 14: NO FALSE PASS — an UNSTAGED back-link must not rescue a commit -
# [P2] The sweep read the WORKING TREE. Under a partial commit that means a
# staged ladder entry minting a new id passes because the back-link exists only
# as an unstaged edit — the commit lands with no design of record and the gate
# says nothing. That is the quiet mode TC-92 exists to stop, arriving through the
# gate built to stop it.
setup_fixture
python3 - "$FIX/dev/plans/release-state-9.9.9.json" <<'PY'
import json, sys
p = sys.argv[1]
s = json.load(open(p))
for e in s["ladder"]:
    if e["slice"] == 0:
        e["title"] = e["title"] + " and the TC-997 leg"
json.dump(s, open(p, "w"), indent=2)
PY
(cd "$FIX" && git add dev/plans/release-state-9.9.9.json)
# The back-link exists ONLY in the working tree. It is never staged.
printf '\nTC-997 is designed here too.\n' >>"$FIX/dev/design/widget-design.md"
run_gate --staged-only --release 9.9.9 --slice 0
if [ "$RC" -eq 1 ] && grep -q 'TC-997' <<<"$OUT"; then
  pass "no false PASS — an UNSTAGED back-link does not cover a staged new id"
else
  fail "arm 14 (index not worktree, false pass): rc=$RC out=$OUT"
fi

# --- Arm 14b: NO FALSE BLOCK — an UNSTAGED loss must not refuse a clean commit
# The other direction, and the one that decides whether the gate survives
# contact with agents. An unrelated unstaged edit that removes coverage must not
# refuse a staged commit that is itself clean: a false BLOCK on the active commit
# path is how seat-path-guard.sh (TC-121) taught two independent agents to route
# around a guard, and the fallback from a blocked commit is a hand-append.
setup_fixture
printf '\ntrivially edited to trigger the gate\n' >>"$FIX/dev/design/gap-leg-design.md"
(cd "$FIX" && git add dev/design/gap-leg-design.md)
# Coverage for Slice 0 is destroyed in the WORKING TREE only.
: >"$FIX/dev/design/widget-design.md"
run_gate --staged-only --release 9.9.9 --slice 0
if [ "$RC" -eq 0 ]; then
  pass "no false BLOCK — an UNSTAGED loss of coverage does not refuse a clean staged commit"
else
  fail "arm 14b (index not worktree, false block): rc=$RC out=$OUT"
fi

# --- Arm 15: THE SNAPSHOT IS TORN DOWN AND THE REPO IS UNTOUCHED -----------
# The gate materialises the index under mktemp -d. It must leave nothing behind
# inside the repository and must not disturb the index or the worktree it is
# gating.
setup_fixture
printf '\nedit\n' >>"$FIX/dev/design/gap-leg-design.md"
(cd "$FIX" && git add dev/design/gap-leg-design.md)
STATUS_BEFORE="$(cd "$FIX" && git status --porcelain --untracked-files=all)"
run_gate --staged-only --release 9.9.9 --slice 0
STATUS_AFTER="$(cd "$FIX" && git status --porcelain --untracked-files=all)"
# NON-VACUITY: rc=127 (gate absent) also leaves everything alone.
if [ "$RC" -ne 127 ] && [ "$STATUS_BEFORE" = "$STATUS_AFTER" ]; then
  pass "the index snapshot leaves the gated repo's index and worktree exactly as found"
else
  fail "arm 15 (snapshot hygiene): rc=$RC before=[$STATUS_BEFORE] after=[$STATUS_AFTER]"
fi

# --- Arm 16: AN UNMERGED INDEX IS ANNOUNCED, NOT REFUSED -------------------
# Cannot-certify is not the same as a coverage gap. There is no stage 0 to
# snapshot, so the gate says so LOUDLY and gets out of the way — a merge commit
# touching dev/design/** must not be blocked by a gate that could not look. This
# deliberately diverges from check-staged-ledger-sidecars.sh, which exits 2:
# that gate protects an invariant conflict resolution actively breaks.
setup_fixture
(
  cd "$FIX"
  git checkout -q -b other
  printf '\nbranch B version\n' >>dev/design/gap-leg-design.md
  git commit -qam "B"
  git checkout -q master 2>/dev/null || git checkout -q main
  printf '\nbranch A version\n' >>dev/design/gap-leg-design.md
  git commit -qam "A"
  git merge other >/dev/null 2>&1 || true
)
run_gate --staged-only --release 9.9.9 --slice 0
UNMERGED="$(cd "$FIX" && git ls-files --unmerged | wc -l)"
if [ "$UNMERGED" -gt 0 ] && [ "$RC" -eq 0 ] && grep -q 'UNMERGED' <<<"$OUT"; then
  pass "an UNMERGED index is announced as NOT CHECKED and let through, never refused"
elif [ "$UNMERGED" -eq 0 ]; then
  fail "arm 16 (unmerged): the fixture did not reach a conflicted state; arm is vacuous"
else
  fail "arm 16 (unmerged): rc=$RC out=$OUT"
fi

# --- Arm 17: A REAL LINKED-WORKTREE PRE-COMMIT HOOK, END TO END ------------
# THE PREDECESSOR OF THIS ARM WENT GREEN OVER A LIVE DEFECT, which is the reason
# it was rebuilt. It hand-made an approximation of the hook environment
# (`GIT_INDEX_FILE=.git/index GIT_DIR=.git`, both RELATIVE) and asserted the
# verdict. A real LINKED-WORKTREE hook — how every orchestrated commit in this
# repo is made — exports both as ABSOLUTE paths, and it is the absolute `GIT_DIR`
# that breaks `git init`. A guard-test that reproduces the wrong environment is
# the vacuous-green class this repo has named twice, so this arm now builds a
# genuine worktree, installs a genuine hook and makes a genuine commit, and takes
# the environment from git rather than from a guess.
#
# `git worktree add` here is entirely inside the throwaway fixture repo under
# mktemp -d; it never goes near the real checkout.
setup_fixture
LINKED="$TMPROOT/linked"
ENVCAP="$TMPROOT/hookenv.txt"
rm -rf "$LINKED" "$ENVCAP"
(cd "$FIX" && git worktree add -q "$LINKED" -b hookarm)
cat >"$FIX/.git/hooks/pre-commit" <<EOF
#!/usr/bin/env bash
env | grep '^GIT_' | sort >"$ENVCAP"
exec ./scripts/check-design-refs.sh --staged-only --release 9.9.9 --slice 0
EOF
chmod +x "$FIX/.git/hooks/pre-commit"
set +e
OUT="$(cd "$LINKED" && git rm -q dev/design/widget-design.md && git commit -m 'drop the design of record' 2>&1)"
RC=$?
set -e
HOOK_GIT_DIR="$(sed -n 's/^GIT_DIR=//p' "$ENVCAP" 2>/dev/null || true)"
# NON-VACUITY, and it is the whole lesson of this round: if the hook did not
# actually export an ABSOLUTE GIT_DIR then this arm is testing the same fiction
# the old one did, and it must say so rather than pass.
case "$HOOK_GIT_DIR" in
  /*) ENV_OK=1 ;;
  *)  ENV_OK=0 ;;
esac
if [ "$ENV_OK" -ne 1 ]; then
  fail "arm 17 (linked-worktree hook): the hook exported GIT_DIR=[$HOOK_GIT_DIR], not an \
absolute path — the arm did not reproduce the environment it claims to test"
elif [ "$RC" -ne 0 ] && grep -qE 'R-9-A|widget_readiness' <<<"$OUT"; then
  pass "a real linked-worktree hook (absolute GIT_DIR) refuses the commit that deletes coverage"
else
  fail "arm 17 (linked-worktree hook): rc=$RC out=$OUT"
fi

# --- Arm 17b: THE GATE MUST NOT WRITE INTO THE REPOSITORY IT IS GATING -----
# THE [P1], stated as the property it actually violated. With `GIT_DIR` still
# exported, `git init -q --template= "$SNAPSHOT"` does not create
# `$SNAPSHOT/.git` at all — GIT_DIR wins, so git REINITIALISES THE GATED
# REPOSITORY instead. MEASURED: `git init` under an exported GIT_DIR rewrites
# that repository's `.git/config`; the gate's three real-side reads
# (`diff --cached`, `ls-files --unmerged`, `checkout-index`) leave the admin
# directory byte-for-byte untouched, so this signature is a stable assertion and
# not a flaky one.
#
# A gate whose header says READ-ONLY must be read-only. This is also the honest
# statement of the defect: the verdict happened to stay correct (the manifest's
# `cd "$(git rev-parse --show-toplevel)"` degrades to `cd ""`, a bash no-op), so
# a verdict-only assertion CANNOT see this — which is exactly how the old arm 17
# passed over it.
# `cksum` (POSIX) rather than `md5sum` (GNU). And the raw listing is emitted
# BEFORE hashing so the caller can assert it is NON-EMPTY: `find -printf` is
# GNU-only, and on BSD it would fail, hash nothing, and yield the SAME constant
# both times — an arm about fail-open that itself fails open.
admin_listing() { find "$1" -printf '%P %T@\n' 2>/dev/null | LC_ALL=C sort; }
setup_fixture
(cd "$FIX" && git rm -q dev/design/widget-design.md)
LIST_BEFORE="$(admin_listing "$FIX/.git")"
SIG_BEFORE="$(printf '%s' "$LIST_BEFORE" | cksum)"
set +e
OUT="$(cd "$FIX" && GIT_DIR="$FIX/.git" GIT_INDEX_FILE="$FIX/.git/index" \
        bash ./scripts/check-design-refs.sh --staged-only --release 9.9.9 --slice 0 2>&1)"
RC=$?
set -e
SIG_AFTER="$(printf '%s' "$(admin_listing "$FIX/.git")" | cksum)"
if [ -z "$LIST_BEFORE" ]; then
  fail "arm 17b (no write into the gated repo): the admin listing came back EMPTY, so the \
comparison would pass no matter what the gate did (find -printf unsupported?)"
elif [ "$RC" -eq 1 ] && [ "$SIG_BEFORE" = "$SIG_AFTER" ]; then
  pass "read-only under an exported absolute GIT_DIR — the gated repo's .git is untouched"
else
  fail "arm 17b (no write into the gated repo): rc=$RC signature changed=$( \
[ "$SIG_BEFORE" = "$SIG_AFTER" ] && echo no || echo YES)"
fi

# --- Arm 17c: THE GATE CONTAINS NO GIT WRITE OF ANY KIND (TC-128) ----------
# THE ARM THAT MAKES THE SAFETY STRUCTURAL. An earlier version called
# `git init "$SNAPSHOT"`; with an ABSOLUTE `GIT_DIR` exported — which every real
# linked-worktree pre-commit hook does — GIT_DIR wins and git reinitialises the
# repository GIT_DIR names instead. TWICE on 2026-07-29 (18:14:51 and 18:26:28)
# that was the PRIMARY CHECKOUT, which gained `bare = true` and refused every
# work-tree operation while `git rev-parse HEAD` still resolved.
#
# The remedy is not a better-ordered environment scrub. A guard on the commit
# path must not be ABLE to damage what it guards, so the single writing git call
# was REMOVED: the snapshot is made a repository with `mkdir` and `printf`, no
# git binary involved. With no write in the path, a leaked GIT_* can at worst
# produce a wrong answer from a docs-hygiene gate.
#
# This arm is a pure grep — it never executes the gate — so it is safe to run
# against any revision, including the ones that caused the incidents.
git_write_verbs() {
  # Strip whole-line comments first: the header discusses `git init` at length
  # and matching prose would report a defect that is not in the code (and, worse,
  # could be "fixed" by deleting the explanation).
  local src="$1" v
  sed 's/^[[:space:]]*#.*$//' "$src" >"$TMPROOT/code-only.sh"
  for v in init config add commit reset rm mv stash clean gc prune repack push \
           fetch pull merge rebase apply am cherry-pick revert update-ref \
           symbolic-ref write-tree commit-tree hash-object tag branch worktree \
           notes replace; do
    if grep -qE "(^|[^-[:alnum:]_])git( --[a-z-]+)* $v( |\$)" "$TMPROOT/code-only.sh"; then
      printf '%s ' "$v"
    fi
  done
}
if [ ! -f "$GATE" ]; then
  fail "arm 17c (no git write): $GATE does not exist, so the assertion is vacuous"
else
  WRITES="$(git_write_verbs "$GATE")"
  # `checkout-index` is the one permitted writer and is excluded from the list
  # above by construction: it writes ONLY inside the mktemp -d snapshot.
  if [ -z "$WRITES" ]; then
    pass "the gate contains NO git write verb — write-safety is structural, not a promise"
  else
    fail "arm 17c (no git write): $GATE invokes git write verb(s): $WRITES"
  fi
fi

# --- Arm 17d: THE SCRUB STILL FOLLOWS THE LAST REAL-REPO READ --------------
# Safety no longer depends on this, but CORRECTNESS does. `checkout-index` must
# see the hook's `GIT_INDEX_FILE`, because a PARTIAL commit
# (`git commit -- <paths>`) puts the content that will be committed into a
# TEMPORARY index and names it there (measured: `.git/next-index-*.lock`).
# Scrubbing before it would snapshot the repository's ordinary index — the wrong
# tree — and silently reintroduce the worktree-vs-index defect.
if [ ! -f "$GATE" ]; then
  fail "arm 17d (scrub ordering): $GATE does not exist, so the assertion is vacuous"
else
  CO_LINE="$(grep -n '^[[:space:]]*if ! git checkout-index' "$GATE" | head -1 | cut -d: -f1)"
  UNSET_LINE="$(grep -n '^[[:space:]]*unset GIT_DIR' "$GATE" | head -1 | cut -d: -f1)"
  if [ -n "$CO_LINE" ] && [ -n "$UNSET_LINE" ] && [ "$CO_LINE" -lt "$UNSET_LINE" ]; then
    pass "the GIT_* scrub follows the last real-repo read (checkout-index=$CO_LINE, unset=$UNSET_LINE)"
  else
    fail "arm 17d (scrub ordering): checkout-index=$CO_LINE unset=$UNSET_LINE — the scrub \
must follow checkout-index so a partial commit's temporary index is the one snapshotted"
  fi
fi

# --- Arm 17e: THE SNAPSHOT MUST RESOLVE TO ITSELF --------------------------
# Belt and braces. Without it, a snapshot whose toplevel resolved elsewhere would
# let the sweep answer about the wrong tree, and a snapshot with no `.git` would
# limp along on `cd ""` — a bash no-op that POSIX leaves unspecified, and exactly
# the accidental correctness that hid the mis-ordering.
if grep -q 'show-toplevel' "$GATE" && grep -qE 'NOT CHECKED.*resolve|resolve to itself' "$GATE"; then
  pass "the gate proves the snapshot resolves to itself before sweeping, or reports NOT CHECKED"
else
  fail "arm 17e (self-resolution): $GATE does not verify the snapshot's own toplevel"
fi

# --- Arm 17f: THE SELF-CHECK STILL REFUSES UNDER A BSD `readlink` ----------
# A PURE-LINUX ARM CANNOT SEE THIS DEFECT, which is exactly how it got here. The
# self-check used `readlink -f`, which BSD/macOS `readlink` does not support:
# both sides of the comparison degrade to the empty string, `"" != ""` is false,
# and the guard whose whole job is to prove the toplevel resolved inside the
# snapshot SILENTLY PASSES. So this arm shadows `readlink` on PATH with a stub
# that behaves like BSD's — rejecting `-f` — and requires the gate to still
# reach a correct verdict rather than sail through.
setup_fixture
mkdir -p "$TMPROOT/bsdbin"
cat >"$TMPROOT/bsdbin/readlink" <<'EOF'
#!/usr/bin/env bash
# BSD/macOS readlink: no -f. Behave exactly as it does — complain and fail.
for a in "$@"; do
  case "$a" in
    -f|-f*) printf 'readlink: illegal option -- f\n' >&2; exit 1 ;;
  esac
done
exec /usr/bin/readlink "$@"
EOF
chmod +x "$TMPROOT/bsdbin/readlink"
(cd "$FIX" && git rm -q dev/design/widget-design.md)
set +e
OUT="$(cd "$FIX" && PATH="$TMPROOT/bsdbin:$PATH" \
        bash ./scripts/check-design-refs.sh --staged-only --release 9.9.9 --slice 0 2>&1)"
RC=$?
set -e
# NON-VACUITY: prove the stub really is on PATH and really refuses -f, otherwise
# this arm is testing a GNU readlink under a different name.
STUB_RC=0
PATH="$TMPROOT/bsdbin:$PATH" readlink -f / >/dev/null 2>&1 || STUB_RC=1
if [ "$STUB_RC" -ne 1 ]; then
  fail "arm 17f (BSD readlink): the stub did not reject -f, so the arm is vacuous"
elif [ "$RC" -eq 1 ] && grep -qE 'R-9-A|widget_readiness' <<<"$OUT" \
     && ! grep -qi 'readlink' <<<"$OUT"; then
  pass "the self-check works under a BSD readlink (no -f) — correct verdict, no readlink errors"
else
  fail "arm 17f (BSD readlink): rc=$RC out=$OUT"
fi

# --- Arm 17g: NO GNU-ONLY CONSTRUCT IN THE GATE ----------------------------
# The gate runs on the commit path on whatever machine an agent is using. Its
# only GNU dependency was `readlink -f`, and that one failed OPEN. Keep the list
# empty rather than re-auditing by hand each round.
GNUISMS="$(grep -nE '(^|[^-[:alnum:]_])(readlink[[:space:]]+-f|grep[[:space:]]+-P|sed[[:space:]]+-i[[:space:]]|stat[[:space:]]+-c|date[[:space:]]+-d|md5sum)' \
             <(sed 's/^[[:space:]]*#.*$//' "$GATE") || true)"
if [ -z "$GNUISMS" ]; then
  pass "the gate uses no GNU-only construct (readlink -f, grep -P, sed -i, stat -c, date -d, md5sum)"
else
  fail "arm 17g (portability): GNU-only construct(s) in $GATE: $GNUISMS"
fi

# --- Arm 18: THE PRIMARY CHECKOUT'S CONFIG IS UNTOUCHED BY THIS WHOLE SUITE -
# THE ARM THAT WOULD HAVE CAUGHT TC-128, and it is deliberately scoped to the
# ENTIRE suite rather than to one gate invocation: the damage was done by a
# `git init` reaching a repository nobody in the test was thinking about. The
# common dir is where `core.bare` lives, and it is shared by every linked
# worktree — so this is the canary for the primary checkout, from a test that
# only ever means to touch a fixture under mktemp -d.
COMMON_AFTER="$(printf '%s' "$(cat "$REAL_COMMON_DIR/config" 2>/dev/null || true)" | cksum)"
BARE_AFTER="$(git -C "$REPO_ROOT" config --get core.bare 2>/dev/null || echo unset)"
if [ -z "$COMMON_CFG_BEFORE" ]; then
  fail "arm 18 (primary config canary): the shared config at $REAL_COMMON_DIR/config read \
back EMPTY, so this canary would pass no matter what happened to it"
elif [ "$COMMON_BEFORE" = "$COMMON_AFTER" ] && [ "$BARE_BEFORE" = "$BARE_AFTER" ]; then
  pass "the primary checkout's git config is byte-identical after the suite (core.bare=$BARE_AFTER)"
else
  fail "arm 18 (primary config canary): the shared git config CHANGED during this suite. \
core.bare before=[$BARE_BEFORE] after=[$BARE_AFTER] (TC-128)"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll design-refs tests passed\n'
