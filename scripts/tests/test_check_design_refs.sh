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

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll design-refs tests passed\n'
