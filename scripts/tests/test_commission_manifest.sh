#!/usr/bin/env bash
# scripts/tests/test_commission_manifest.sh — T3b recurrence guard
# (DOC-HYGIENE-2): the GENERATED COMMISSION MANIFEST.
#
# The incident this closes: briefing an orchestrator for a slice meant
# hand-assembling the same citation list every time — design-of-record paths,
# contract paths, plan section anchors, the base SHA, the worktree rules, the
# stop conditions — out of a 5-12 file fan-out, with NOTHING checking that the
# cited paths still resolved. T1d already measured what that costs: the TC-45
# line anchors in plan-0.8.20.md were ~2,100 lines off and had never been
# correct. A citation nobody checks is a citation that rots silently.
#
# Predicate under test (see scripts/commission-manifest.sh for the statement):
# for a given release + slice, the emitted manifest cites only paths that EXIST
# and only anchors that actually OCCUR in the file they name — and it hard-fails
# rather than emitting anything if either is false, or if it would emit nothing.
#
# RED-first: the predicate passes on the real repo today (arm 8), so asserting
# against the real checkout alone would prove nothing — a `true` script would
# pass it. Every failure arm below runs against a purpose-built fixture repo in
# which exactly one fault is planted, so an arm can only go green because the
# generator's own check fired.
#
# NON-VACUITY: the suite honours $GATE_UNDER_TEST, so a MUTANT generator — one
# whose path-existence test always answers "exists" — can be pointed at it. That
# mutant turns arms 1/1b RED, which is the evidence that a green here is
# load-bearing rather than a script that merely exits 0.
#
# SECOND PREDICATE (codex §9 [P2], arms 8e-8j): the emitted base SHA is the
# TARGET SLICE'S PREDECESSOR — the greatest LANDED slice strictly below it —
# never max(landed). Measured RED against the pre-fix generator: with Slice 10
# in `landed`, the manifest for Slice 10 printed
#   `base sha  dddd5555  (Slice 10, the newest LANDED slice's landing merge)`
# i.e. branch from the very work you are being commissioned to do; and with a
# later slice landed it printed Slice 30's merge as the base for Slice 10. That
# is the agent-worktree-stale-base trap in printed form, and it becomes live the
# moment 0.8.20 Slice 20 lands. These arms assert the base LINE specifically,
# because every landing SHA also appears in the `landed so far` roll-up and a
# whole-output grep therefore cannot distinguish a right base from a wrong one.
#
# Isolation: fixtures are throwaway git repos under mktemp -d (the generator
# does `cd "$(git rev-parse --show-toplevel)"`, so each fixture must BE a repo).
# Nothing here writes into the real checkout — arm 9 asserts that mechanically.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GEN="${GATE_UNDER_TEST:-$REPO_ROOT/scripts/commission-manifest.sh}"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"
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

# A minimal but REAL fixture release: a state file with a landed prefix and one
# UNBLOCKED slice whose short/title carry the tokens the design scan keys on, a
# plan carrying every required role heading, and the fixed infrastructure the
# manifest cites (orchestration doc, release hand-off, preflight, AC register,
# governed-surface pin, schema source). Every arm mutates ONE thing from this
# baseline, so a red arm isolates exactly one fault.
setup_fixture() {
  rm -rf "$FIX"
  mkdir -p "$FIX/dev/plans/runs/codex" "$FIX/dev/plans/prompts" "$FIX/dev/design/sub" \
           "$FIX/scripts" "$FIX/src/conformance" "$FIX/src/rust/crates/fathomdb-schema/src"
  cp "$GEN" "$FIX/scripts/commission-manifest.sh"
  chmod +x "$FIX/scripts/commission-manifest.sh"
  (cd "$FIX" && git init -q && git config user.email t@example.com && git config user.name t)

  cat >"$FIX/dev/plans/release-state-9.9.9.json" <<'EOF'
{
  "release": "9.9.9",
  "release_kind": "even, publish",
  "board": "dev/plans/runs/board.md",
  "plan": "dev/plans/plan-9.9.9.md",
  "master": "dev/plans/master.md",
  "schema_version": 42,
  "schema_version_source": "pub const SCHEMA_VERSION in src/rust/crates/fathomdb-schema/src/lib.rs",
  "ladder": [
    {"slice": 0,  "short": "X0",    "title": "X0 design gate",                        "depends_on": [],      "status": "LANDED",      "sha": "aaaa1111"},
    {"slice": 5,  "short": "R-9-A", "title": "the keystone",                          "depends_on": [0],     "status": "LANDED",      "sha": "bbbb2222"},
    {"slice": 10, "short": "R-9-B", "title": "widget_readiness + the TC-99 terminal fix", "depends_on": [5], "status": "UNBLOCKED",   "sha": null},
    {"slice": 30, "short": "H7",    "title": "can-i-deploy contract gate",            "depends_on": [10],    "status": "NOT_STARTED", "sha": null, "publish_precondition": true}
  ],
  "landed": [0, 5],
  "next_slice": 10,
  "remaining_ladder": [10, 30],
  "unblocked": [10],
  "publish_precondition_slice": 30,
  "acceptance": {
    "highest_defined_non_reserved": "AC-900",
    "publish_gate": {
      "ac": "AC-999", "minted": false, "signed": false, "state_word": "unsigned",
      "sign_off_slice": 30, "board_ref": "§4 #1"
    }
  },
  "decisions": {
    "unruled": [
      {"id": "publish", "title": "PUBLISH the pair", "gated_at": "Slice 30", "halts_run": true,
       "source": "plan-9.9.9.md §11 item 3"},
      {"id": "batched-surface", "title": "the batched governed-surface delta", "gated_at": "the 30 boundary",
       "halts_run": false, "source": "plan-9.9.9.md §11 item 2"}
    ],
    "ruled": [
      {"id": "already-settled", "ruling": "CLOSED BY DECISION", "ruled_on": "2026-01-01",
       "source": "plan-9.9.9.md §10"}
    ]
  },
  "generated_views": []
}
EOF

  cat >"$FIX/dev/plans/plan-9.9.9.md" <<'EOF'
---
status: ACTIVE
---

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
---
status: locked
---

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

  # The two templates the emitted section set is DERIVED from. The stand-ins
  # carry exactly the headings the manifest cites, so a rename of any of them in
  # the real repo is caught by arm 9 rather than by an agent following a dead
  # pointer.
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

  # Design tier: one ACTIVE doc and one UNREVIEWED doc, both carrying the slice
  # tokens, plus a doc that carries NO token (it must NOT be cited).
  cat >"$FIX/dev/design/widget-design.md" <<'EOF'
---
status: ACTIVE
---

# Widget design

widget_readiness is defined here, and R-9-B is its requirement id.
EOF

  cat >"$FIX/dev/design/sub/widget-contract.md" <<'EOF'
---
status: UNREVIEWED
---

# Widget contract

The TC-99 terminal fix attaches to widget_readiness.
EOF

  # The slice's OWN design memo, found by filename (release + slice), carrying
  # none of the tokens — and its 0.8.0-era namesake, which must NOT be pulled in
  # just because the slice numbers collide.
  cat >"$FIX/dev/design/9.9.9-slice-10-design.md" <<'EOF'
---
status: ACTIVE
---

# The slice memo

No token here at all.
EOF

  cat >"$FIX/dev/design/slice-10-design.md" <<'EOF'
---
status: UNREVIEWED
---

# An OLD release's slice 10

Also no token, and a different release.
EOF

  cat >"$FIX/dev/design/unrelated.md" <<'EOF'
---
status: UNREVIEWED
---

# Unrelated

Nothing to do with this slice.
EOF
}

run_gen() {
  set +e
  OUT="$(cd "$FIX" && ./scripts/commission-manifest.sh "$@" 2>&1)"
  RC=$?
  set -e
}

# Structural edits to the fixture's state file (landed set, ladder statuses,
# landing SHAs) for the base-SHA arms. python3 is already a hard precondition of
# the generator, so using it here adds no dependency; hand-rolled perl over JSON
# would be the fragile choice. `s` is the parsed state, `L` maps slice -> entry.
mutate_state() {
  python3 -c "
import json
p = '$FIX/dev/plans/release-state-9.9.9.json'
s = json.load(open(p))
L = {e['slice']: e for e in s['ladder']}
$1
json.dump(s, open(p, 'w'), indent=2)
"
}

# The `base sha` line as the operator reads it. Asserting on THIS line, not on
# the whole manifest, is the point: every landing SHA also appears in the
# `landed so far` roll-up, so a whole-output grep cannot tell a correct base
# from a wrong one.
base_line() { grep -m1 '^  base sha' <<<"$OUT" || true; }

# --- Arm 0: the BASELINE fixture is GREEN and NON-EMPTY --------------------
# Without this, every RED arm below could be passing for an unrelated reason.
setup_fixture
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && [ "$(printf '%s\n' "$OUT" | grep -c .)" -gt 20 ]; then
  pass "baseline fixture — a known slice emits a non-empty manifest (exit 0)"
else
  fail "arm 0 (baseline green): rc=$RC out=$OUT"
fi

# --- Arm 0b: every SECTION the two templates require is present ------------
# The manifest REPLACES a hand-maintained brief template, so it has to carry the
# section set those templates actually contain — not a parallel invention.
MISSING=""
for section in "ASSIGNMENT" "BASE SHA" "WORKTREE RULES" "CONTRACT PATHS" \
               "PLAN SECTION ANCHORS" "DESIGN DOCS" "ACCEPTANCE" "STOP CONDITIONS" "CLOSURE"; do
  grep -q "$section" <<<"$OUT" || MISSING="$MISSING $section"
done
if [ -z "$MISSING" ]; then
  pass "the manifest carries every template-derived section"
else
  fail "arm 0b (section set): missing:$MISSING"
fi

# --- Arm 1: A CITED PATH DOES NOT EXIST -> hard FAIL, naming it ------------
# THE core guard. A rotted citation must fail loudly instead of being emitted.
setup_fixture
rm -f "$FIX/dev/acceptance.md"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'dev/acceptance.md' <<<"$OUT"; then
  pass "missing cited path — HARD fails and NAMES the path"
else
  fail "arm 1 (missing path): rc=$RC out=$OUT"
fi

# --- Arm 1a: the failing run emits NO manifest ------------------------------
# "Fails loudly rather than being emitted": a manifest that is printed anyway,
# with one dead citation in it, is exactly the artifact this tranche removes.
if ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "a failing verification suppresses the manifest entirely"
else
  fail "arm 1a (no partial emit): the manifest was printed despite a dead citation"
fi

# --- Arm 1b: a cited ANCHOR that no longer occurs -> hard FAIL --------------
# Swapping an unverified line number for an unverified SYMBOL would launder a bad
# pointer as a good one (T1d's crux). Anchors are existence-checked too.
setup_fixture
perl -0777 -pi -e 's/## 9\. Immediate next slice/## 9. Next up/' "$FIX/dev/plans/plan-9.9.9.md"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -qi 'immediate next slice\|no heading' <<<"$OUT"; then
  pass "rotted plan anchor — a role heading that no longer exists HARD-fails"
else
  fail "arm 1b (rotted anchor): rc=$RC out=$OUT"
fi

# --- Arm 1c: an infrastructure anchor that rots ----------------------------
setup_fixture
perl -0777 -pi -e 's/## 10\. Hard rules summary/## 10. Rules/' "$FIX/dev/design/orchestration.md"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -qi 'hard rules' <<<"$OUT"; then
  pass "rotted orchestration anchor — the worktree-rules citation HARD-fails"
else
  fail "arm 1c (rotted infra anchor): rc=$RC out=$OUT"
fi

# --- Arm 1d: a cited SYMBOL that no longer occurs -> hard FAIL -------------
# The anchors in arms 1b/1c are resolved by reading the file, so they can only
# rot at RESOLUTION time. These two citations are hand-registered symbols
# (`--landing` in the preflight, `pub const SCHEMA_VERSION` in the schema source)
# and they are what makes the occurrence check load-bearing: without an arm here,
# disabling that check leaves the suite green (measured — mutant M2).
setup_fixture
perl -0777 -pi -e 's/--landing/--publish/' "$FIX/scripts/preflight.sh"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q -- '--landing' <<<"$OUT" && grep -q 'preflight.sh' <<<"$OUT"; then
  pass "rotted symbol — the TC-RUBRIC-5 \`--landing\` citation HARD-fails when it stops occurring"
else
  fail "arm 1d (rotted symbol): rc=$RC out=$OUT"
fi

# --- Arm 1e: the SCHEMA-source symbol rots ---------------------------------
setup_fixture
perl -0777 -pi -e 's/pub const SCHEMA_VERSION/pub const SCHEMA_REV/' \
  "$FIX/src/rust/crates/fathomdb-schema/src/lib.rs"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'SCHEMA_VERSION' <<<"$OUT"; then
  pass "rotted symbol — the state file's SCHEMA source citation HARD-fails"
else
  fail "arm 1e (schema symbol): rc=$RC out=$OUT"
fi

# --- Arm 2: ZERO citations for the slice -> hard FAIL (TC-37) --------------
# The vacuous-pass class this repo is named for: a generator that exits 0 with
# an empty required-reading list has briefed nobody while looking green.
setup_fixture
perl -0777 -pi -e 's/"short": "R-9-B", "title": "widget_readiness \+ the TC-99 terminal fix"/"short": "R-0-ZZ", "title": "nothing matches this"/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
# ...and no slice memo for it either, so BOTH selectors come up empty.
rm -f "$FIX/dev/design/9.9.9-slice-10-design.md"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'TC-37' <<<"$OUT" && grep -qi 'zero\|no design' <<<"$OUT"; then
  pass "zero design citations — an empty manifest HARD-fails (TC-37), never exit 0"
else
  fail "arm 2 (zero citations): rc=$RC out=$OUT"
fi

# --- Arm 3: UNKNOWN RELEASE -> hard FAIL, listing what exists --------------
setup_fixture
run_gen 1.2.3 10
if [ "$RC" -ne 0 ] && grep -qi 'no release-state file\|unknown release' <<<"$OUT" \
   && grep -q '9.9.9' <<<"$OUT"; then
  pass "unknown release — HARD fails and names the releases that DO have a state file"
else
  fail "arm 3 (unknown release): rc=$RC out=$OUT"
fi

# --- Arm 4: UNKNOWN SLICE -> hard FAIL, listing the ladder -----------------
setup_fixture
run_gen 9.9.9 99
if [ "$RC" -ne 0 ] && grep -qi 'not in the .*ladder\|unknown slice' <<<"$OUT" \
   && grep -q '30' <<<"$OUT"; then
  pass "unknown slice — HARD fails and names the ladder it is not in"
else
  fail "arm 4 (unknown slice): rc=$RC out=$OUT"
fi

# --- Arm 5: a KNOWN slice emits the load-bearing facts ---------------------
# Non-emptiness is not the bar: the manifest has to be usable to brief a slice.
setup_fixture
run_gen 9.9.9 10
MISSING=""
grep -q 'bbbb2222'                              <<<"$OUT" || MISSING="$MISSING base-sha"
grep -q 'dev/plans/plan-9.9.9.md'               <<<"$OUT" || MISSING="$MISSING plan"
grep -q 'dev/design/widget-design.md'           <<<"$OUT" || MISSING="$MISSING design-of-record"
grep -q 'dev/design/sub/widget-contract.md'     <<<"$OUT" || MISSING="$MISSING nested-design"
grep -q 'preflight.sh'                          <<<"$OUT" || MISSING="$MISSING preflight"
grep -q 'PUBLISH the pair'                      <<<"$OUT" || MISSING="$MISSING halting-decision"
grep -q 'AC-999'                                <<<"$OUT" || MISSING="$MISSING publish-gate-ac"
grep -q 'UNREVIEWED'                            <<<"$OUT" || MISSING="$MISSING unreviewed-surfaced"
if [ "$RC" -eq 0 ] && [ -z "$MISSING" ]; then
  pass "known slice — base SHA, contract, design, worktree rules and stop conditions all present"
else
  fail "arm 5 (known slice content): rc=$RC missing:$MISSING"
fi

# --- Arm 5b: a doc carrying NO slice token is NOT cited --------------------
# Otherwise "design of record" degenerates into "every design doc in the repo".
if ! grep -q 'dev/design/unrelated.md' <<<"$OUT"; then
  pass "an unrelated design doc is not cited (the scan is token-driven, not a dump)"
else
  fail "arm 5b (token selectivity): unrelated.md was cited"
fi

# --- Arm 5d: the slice's OWN memo is found by filename, and only ITS ---------
# A design doc named for this release AND this slice is that slice's memo even
# when it carries no token. Its same-numbered namesake from ANOTHER release must
# not be cited: a wrong citation that still resolves is the worst kind.
if grep -q 'dev/design/9.9.9-slice-10-design.md' <<<"$OUT" \
   && ! grep -q 'dev/design/slice-10-design.md' <<<"$OUT"; then
  pass "filename selector — this release's slice memo is cited, another release's is not"
else
  fail "arm 5d (filename selector): out=$OUT"
fi

# --- Arm 5c: UNREVIEWED is surfaced as unclassified, not laundered ---------
# T2c's backfill defaulted to UNREVIEWED = "nobody has classified this yet". A
# manifest that presents those as authoritative would launder the gate's silence.
if grep -q 'TC-50' <<<"$OUT" && grep -qi 'not verified\|unclassified\|not a currency claim' <<<"$OUT"; then
  pass "UNREVIEWED docs are surfaced with their status + the TC-50 caveat"
else
  fail "arm 5c (unreviewed honesty): out=$OUT"
fi

# --- Arm 6: STATE-FILE-DRIVEN — the manifest follows T2a's file ------------
# Proves the generator READS the single-writer state file rather than
# re-scraping the prose (which is the whole point of T2a owning these facts).
setup_fixture
perl -0777 -pi -e 's/bbbb2222/cafe4444/; s/"next_slice": 10/"next_slice": 30/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && grep -q 'cafe4444' <<<"$OUT" && ! grep -q 'bbbb2222' <<<"$OUT" \
   && grep -qi 'not the next slice' <<<"$OUT"; then
  pass "state-file-driven — a changed base SHA and next slice both move the manifest"
else
  fail "arm 6 (state-file-driven): rc=$RC out=$OUT"
fi

# --- Arm 7: NO BARE LINE ANCHORS are ever emitted (T1d's ban) --------------
setup_fixture
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && ! grep -qE '`[A-Za-z_][A-Za-z0-9_.+-]*:[0-9]{3,}(-[0-9]+)?\+?`' <<<"$OUT"; then
  pass "no bare \`name:line\` anchors in the emitted manifest (T1d shape)"
else
  fail "arm 7 (line-anchor ban): rc=$RC"
fi

# --- Arm 8: READ-ONLY — generating writes nothing into the repo ------------
setup_fixture
(cd "$FIX" && git add -A && git commit -qm base)
run_gen 9.9.9 10
DIRT="$(cd "$FIX" && git status --porcelain --untracked-files=all)"
if [ "$RC" -eq 0 ] && [ -z "$DIRT" ]; then
  pass "read-only — a generate leaves the worktree byte-clean"
else
  fail "arm 8 (read-only): rc=$RC dirt=$DIRT"
fi

# --- Arm 8b: --verify prints the counts and no manifest --------------------
run_gen 9.9.9 10 --verify
if [ "$RC" -eq 0 ] && grep -qi 'verified' <<<"$OUT" && ! grep -q 'STOP CONDITIONS' <<<"$OUT"; then
  pass "--verify checks the citations without emitting the brief"
else
  fail "arm 8c (--verify): rc=$RC out=$OUT"
fi

# --- Arm 8c: --verify-all sweeps every state file's NEXT slice -------------
run_gen --verify-all
if [ "$RC" -eq 0 ] && grep -q '9.9.9' <<<"$OUT"; then
  pass "--verify-all verifies the next slice of every release-state file"
else
  fail "arm 8d (--verify-all): rc=$RC out=$OUT"
fi

# --- Arm 8d: --verify-all with ZERO state files -> hard FAIL (TC-37) -------
setup_fixture
rm -f "$FIX/dev/plans/release-state-9.9.9.json"
run_gen --verify-all
if [ "$RC" -ne 0 ] && grep -q 'TC-37' <<<"$OUT"; then
  pass "vacuity guard — a sweep that discovers zero state files HARD-fails"
else
  fail "arm 8e (zero state files): rc=$RC out=$OUT"
fi

# --- Arm 8e: THE TARGET SLICE IS ITSELF LANDED -> base is its PREDECESSOR ---
# codex §9 [P2]. The base used to be max(landed), which for a slice that is not
# the next one selects the target's OWN landing merge (or a later one) — a brief
# telling an operator to branch from the very work being commissioned. This is
# the fixture form of the live case: it arrives the moment 0.8.20 Slice 20 lands
# and someone regenerates its manifest. Slice 10 is landed here; the base MUST
# be Slice 5's merge (bbbb2222) and MUST NOT be Slice 10's own (dddd5555).
setup_fixture
mutate_state "L[10]['status'] = 'LANDED'; L[10]['sha'] = 'dddd5555'; s['landed'] = [0, 5, 10]; s['next_slice'] = 30"
run_gen 9.9.9 10
BASE_LINE="$(base_line)"
if [ "$RC" -eq 0 ] && grep -q 'bbbb2222' <<<"$BASE_LINE" && ! grep -q 'dddd5555' <<<"$BASE_LINE"; then
  pass "target slice already landed — the base is its PREDECESSOR, never its own landing merge"
else
  fail "arm 8e (predecessor base): rc=$RC base=[$BASE_LINE]"
fi

# --- Arm 8f: and the regeneration is LABELLED, not silently reinterpreted ----
# The reader has to know they asked for a slice that is already in the past;
# a correct base printed without that context still misleads.
if grep -qi 'HISTORICAL' <<<"$OUT"; then
  pass "an already-landed target is marked HISTORICAL rather than briefed as if pending"
else
  fail "arm 8f (historical marker): out=$OUT"
fi

# --- Arm 8g: LATER slices landed too -> still the IMMEDIATE predecessor ------
# max(landed) here is Slice 30, two slices past the target. The base must not
# drift forward just because the release moved on.
setup_fixture
mutate_state "L[10]['status'] = 'LANDED'; L[10]['sha'] = 'dddd5555'
L[30]['status'] = 'LANDED'; L[30]['sha'] = 'eeee6666'
s['landed'] = [0, 5, 10, 30]; s['next_slice'] = None"
run_gen 9.9.9 10
BASE_LINE="$(base_line)"
if [ "$RC" -eq 0 ] && grep -q 'bbbb2222' <<<"$BASE_LINE" \
   && ! grep -q 'dddd5555' <<<"$BASE_LINE" && ! grep -q 'eeee6666' <<<"$BASE_LINE"; then
  pass "later slices landed — the base stays the target's immediate predecessor"
else
  fail "arm 8g (no forward drift): rc=$RC base=[$BASE_LINE]"
fi

# --- Arm 8h: THE FIRST SLICE — nothing precedes it --------------------------
# Documented behaviour: there is no predecessor merge, so the manifest says so
# in words and sends the operator to `git rev-parse origin/main`. It must never
# print an empty base, and it must never borrow a later slice's SHA.
setup_fixture
printf -- '---\nstatus: ACTIVE\n---\n\n# Slice 0 memo\n' >"$FIX/dev/design/9.9.9-slice-0-design.md"
mutate_state "L[0]['status'] = 'UNBLOCKED'; L[0]['sha'] = None
L[5]['status'] = 'NOT_STARTED'; L[5]['sha'] = None
s['landed'] = []; s['next_slice'] = 0"
run_gen 9.9.9 0
BASE_LINE="$(base_line)"
BASE_BLOCK="$(grep -m1 -A3 '^  base sha' <<<"$OUT" || true)"
if [ "$RC" -eq 0 ] && grep -qi 'no landed slice precedes' <<<"$BASE_LINE" \
   && ! grep -qE '[0-9a-f]{8}' <<<"$BASE_LINE" \
   && grep -qi 'first slice' <<<"$BASE_BLOCK" && grep -q 'origin/main' <<<"$BASE_BLOCK"; then
  pass "first slice — no predecessor is stated in words + branch-from-tip, not left blank"
else
  fail "arm 8h (first slice): rc=$RC base=[$BASE_LINE] block=[$BASE_BLOCK]"
fi

# --- Arm 8i: first slice, but LATER slices have landed -> the tip is AHEAD ---
# Regenerating a historical first-slice brief: branching from the tip would pick
# up work that slice never had, so the manifest has to say the tip has moved on
# instead of implying the tip IS the point of cut.
setup_fixture
printf -- '---\nstatus: ACTIVE\n---\n\n# Slice 0 memo\n' >"$FIX/dev/design/9.9.9-slice-0-design.md"
mutate_state "L[0]['status'] = 'UNBLOCKED'; L[0]['sha'] = None
s['landed'] = [5]; s['next_slice'] = 0"
run_gen 9.9.9 0
BASE_LINE="$(base_line)"
if [ "$RC" -eq 0 ] && ! grep -qE '[0-9a-f]{8}' <<<"$BASE_LINE" \
   && grep -q 'TIP IS AHEAD' <<<"$OUT" && grep -q 'bbbb2222' <<<"$OUT"; then
  pass "no predecessor but later landings exist — the manifest warns the tip is AHEAD"
else
  fail "arm 8i (tip ahead): rc=$RC base=[$BASE_LINE] out=$OUT"
fi

# --- Arm 8j: GAPS in `landed` -> the greatest landed slice BELOW the target --
# Slice 5 never landed; the predecessor is Slice 10, not "the target minus one"
# and not the oldest landing.
setup_fixture
printf -- '---\nstatus: ACTIVE\n---\n\n# Slice 30 memo\n' >"$FIX/dev/design/9.9.9-slice-30-design.md"
mutate_state "L[5]['status'] = 'NOT_STARTED'; L[5]['sha'] = None
L[10]['status'] = 'LANDED'; L[10]['sha'] = 'dddd5555'
s['landed'] = [0, 10]; s['next_slice'] = 30"
run_gen 9.9.9 30
BASE_LINE="$(base_line)"
if [ "$RC" -eq 0 ] && grep -q 'dddd5555' <<<"$BASE_LINE" \
   && ! grep -q 'aaaa1111' <<<"$BASE_LINE" && ! grep -q 'bbbb2222' <<<"$BASE_LINE"; then
  pass "gaps in \`landed\` — the base is the greatest landed slice strictly below the target"
else
  fail "arm 8j (gapped landed): rc=$RC base=[$BASE_LINE]"
fi

# --- Arm 9: the REAL repo — Slice 20 of 0.8.20 (the acceptance criterion) --
# Every emitted path must resolve. This is the regression half of the pair and
# the tranche's stated bar.
set +e
REAL_OUT="$("$REPO_ROOT/scripts/commission-manifest.sh" 0.8.20 20 2>&1)"
REAL_RC=$?
set -e
if [ "$REAL_RC" -eq 0 ] && grep -q 'R-20-DR' <<<"$REAL_OUT" && grep -q 'a2022957' <<<"$REAL_OUT"; then
  pass "real repo — the 0.8.20 Slice 20 manifest generates with every path resolving"
else
  fail "arm 9 (real Slice 20): rc=$REAL_RC out=$REAL_OUT"
fi

# --- Arm 9b: the real manifest carries no bare line anchors ----------------
if ! grep -qE '`[A-Za-z_][A-Za-z0-9_.+-]*:[0-9]{3,}(-[0-9]+)?\+?`' <<<"$REAL_OUT"; then
  pass "the real manifest emits section anchors only — no \`name:line\` pointers"
else
  fail "arm 9b (real line-anchor ban): the manifest emitted a bare line anchor"
fi

# --- Arm 9d: the real repo's base SHA is Slice 15's merge, on the LINE ------
# The live regression half of the predecessor fix. Slice 20 is not yet landed,
# so predecessor and max(landed) agree TODAY — asserting the base LINE (not the
# whole manifest, where every landing SHA appears in the roll-up) is what keeps
# this honest the day Slice 20 lands.
REAL_BASE_LINE="$(grep -m1 '^  base sha' <<<"$REAL_OUT" || true)"
if grep -q 'a2022957' <<<"$REAL_BASE_LINE" && grep -q 'Slice 15' <<<"$REAL_BASE_LINE" \
   && ! grep -qi 'HISTORICAL' <<<"$REAL_OUT"; then
  pass "real repo — Slice 20's base is Slice 15's landing merge (a2022957), on the base line"
else
  fail "arm 9d (real base line): base=[$REAL_BASE_LINE]"
fi

# --- Arm 9c: the real repo's every-release sweep is green ------------------
set +e
SWEEP_OUT="$("$REPO_ROOT/scripts/commission-manifest.sh" --verify-all 2>&1)"
SWEEP_RC=$?
set -e
if [ "$SWEEP_RC" -eq 0 ]; then
  pass "real repo — --verify-all is green across every live release-state file"
else
  fail "arm 9c (real sweep): rc=$SWEEP_RC out=$SWEEP_OUT"
fi

# --- Arm 10: wired into agent-test.sh and into an ALWAYS-ON CI job ---------
if grep -q 'scripts/tests/test_commission_manifest.sh' "$AGENT_TEST"; then
  pass "agent-test.sh runs this fixture suite"
else
  fail "agent-test.sh does not run scripts/tests/test_commission_manifest.sh"
fi

CI_JOB_BLOCK="$(awk '
  /^  commission-manifest:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"
if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a commission-manifest job"
else
  fail "ci.yml has no commission-manifest job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/commission-manifest.sh'; then
  pass "the CI job runs the SHARED scripts/commission-manifest.sh"
else
  fail "the CI job must invoke scripts/commission-manifest.sh, not a reimplementation"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*(if|needs):'; then
  fail "the commission-manifest job must be ALWAYS-ON (no if:/needs:); block: $CI_JOB_BLOCK"
else
  pass "the commission-manifest job is always-on (no if:, no needs:, not docs_only-gated)"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll commission-manifest tests passed\n'
