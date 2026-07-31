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
# THIRD PREDICATE (codex §9 [P2], arms 5h/5i/9e): the publish gate's
# `pre_sign.pinned_to` is a REGISTERED CITATION, not printed text. It names the
# file whose content the HITL pre-sign is bound to, and it was emitted raw — so
# a mistyped or renamed pin path was still printed while `--verify` reported 0
# dead citations and CI stayed green. That is CHECK 1's guarantee not holding
# over the one path it matters most for. Arm 5h asserts the failure direction, 5i
# asserts the verified-path COUNT moves (a bolted-on existence check beside the
# citation list would satisfy 5h alone), and 9e asserts it on the live state file.
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

  # The publish-gate fact set. THREE DISTINCT FACTS — pre-sign, minting, and who
  # actually gates publish — never one collapsed word. The predecessor model
  # carried `state_word: "unsigned"` and this manifest printed it, so a Slice-20
  # brief told the next orchestrator that publish awaited an AC-079 signature the
  # HITL had ALREADY GIVEN (pre-signed 2026-07-25, master F-34). Briefing an
  # orchestrator with a settled call restated as open is how it gets re-decided.
  case "${1:-presigned}" in
    presigned)
      GATE_JSON='"ac": "AC-999",
      "covers": "the accumulated governed-surface delta",
      "pre_sign_state": "PRE_SIGNED",
      "pre_sign": {"on": "2026-01-02", "by": "HITL", "source": "master F-99",
                   "pinned_to": "src/conformance/governed-surface-allowlist.json",
                   "reopens_if": "any diff to that file re-opens it (the pin)"},
      "minted": false, "minted_as": "SIGNED", "sign_off_slice": 30,
      "publish_gated_by": "the separate HITL publish gate",
      "board_ref": "§4 #1"'
      ;;
    notpresigned)
      GATE_JSON='"ac": "AC-999",
      "covers": "the accumulated governed-surface delta",
      "pre_sign_state": "NOT_PRE_SIGNED",
      "minted": false, "minted_as": "SIGNED", "sign_off_slice": 30,
      "publish_gated_by": "the separate HITL publish gate",
      "board_ref": "§4 #1"'
      ;;
    *) printf 'setup_fixture: unknown gate mode %q\n' "$1" >&2; exit 2 ;;
  esac

  cat >"$FIX/dev/plans/release-state-9.9.9.json" <<EOF
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
      ${GATE_JSON}
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

# --- Arm 5e: the PUBLISH-GATE line must not brief a stale claim ------------
# The measured defect: this line printed a single `state_word` ("unsigned"), so a
# Slice-20 manifest told the next orchestrator that publish awaited an AC-079
# signature the HITL had ALREADY GIVEN (pre-signed 2026-07-25, master F-34). A
# brief that restates a settled call as open invites re-deciding it.
setup_fixture presigned
run_gen 9.9.9 10
GATELINES="$(sed -n '/^  publish gate/,/^  re-verified green\|^  Mint no AC/p' <<<"$OUT")"
MISSING=""
grep -q 'PRE-SIGNED'                            <<<"$GATELINES" || MISSING="$MISSING pre-signed"
grep -q 'NOT YET MINTED'                        <<<"$GATELINES" || MISSING="$MISSING not-minted"
grep -q 'mints at'                              <<<"$GATELINES" || MISSING="$MISSING mints-at"
grep -q 'Slice 30'                              <<<"$GATELINES" || MISSING="$MISSING sign-off-slice"
grep -q 'governed-surface-allowlist.json'       <<<"$GATELINES" || MISSING="$MISSING pin"
grep -q 'separate HITL publish gate'            <<<"$GATELINES" || MISSING="$MISSING separate-gate"
grep -q 'never re-decide'                       <<<"$GATELINES" || MISSING="$MISSING do-not-re-decide"
grep -q 'None'                                  <<<"$GATELINES" && MISSING="$MISSING rendered-None"
grep -qi 'unsigned'                             <<<"$GATELINES" && MISSING="$MISSING claims-unsigned"
if [ "$RC" -eq 0 ] && [ -z "$MISSING" ]; then
  pass "publish gate — a PRE-SIGNED gate briefs pre-sign + pin + minting + the separate gate"
else
  fail "arm 5e (pre-signed publish-gate line): rc=$RC missing:$MISSING lines:$GATELINES"
fi

# --- Arm 5f: the NOT-pre-signed direction still briefs it as BLOCKING ------
# Without this the fix would be indistinguishable from hardcoding the happy path.
setup_fixture notpresigned
run_gen 9.9.9 10
GATELINES="$(sed -n '/^  publish gate/,/^  re-verified green\|^  Mint no AC/p' <<<"$OUT")"
if [ "$RC" -eq 0 ] \
   && grep -q 'NOT PRE-SIGNED' <<<"$GATELINES" \
   && grep -q 'awaiting HITL sign-off' <<<"$GATELINES" \
   && ! grep -q 'None' <<<"$GATELINES"; then
  pass "publish gate — a gate that is genuinely NOT pre-signed briefs as awaiting sign-off"
else
  fail "arm 5f (not-pre-signed publish-gate line): rc=$RC lines:$GATELINES"
fi

# --- Arm 5g: the RETIRED `state_word` cannot be read again ----------------
# A consumer falling back to the collapsed word — or `.get()` rendering `None`
# from a field that no longer exists — is exactly the recurrence.
setup_fixture presigned
perl -0777 -pi -e 's/"pre_sign_state": "PRE_SIGNED",/"state_word": "unsigned",/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'state_word' <<<"$OUT"; then
  pass "a state file still carrying the retired \`state_word\` HARD-fails the manifest"
else
  fail "arm 5g (retired state_word): rc=$RC out=$OUT"
fi

# --- Arm 5h: THE PIN PATH IS A VERIFIED CITATION, not raw text -------------
# codex §9 [P2]. `pre_sign.pinned_to` names the file whose CONTENT the HITL
# pre-sign is bound to — the most load-bearing path in the whole manifest, since
# the pin is what makes the pre-sign citable rather than a bare assertion. It was
# emitted with `m.out(... % pre["pinned_to"])`, i.e. as RAW TEXT, so CHECK 1
# never saw it. MEASURED RED against the pre-fix generator (GATE_UNDER_TEST):
# with `pinned_to` pointing at src/conformance/DOES-NOT-EXIST.json the manifest
# was still EMITTED, still exited 0, and still printed
#   `PATH VERIFICATION: 24 distinct path(s) exist, 24 anchor(s) verified, 0 dead.`
# — a dead pin path briefing an orchestrator while CI stayed green, which is
# exactly the "a gate that appears to cover a citation but does not" class.
setup_fixture presigned
mutate_state "s['acceptance']['publish_gate']['pre_sign']['pinned_to'] = 'src/conformance/DOES-NOT-EXIST.json'"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'DOES-NOT-EXIST.json' <<<"$OUT" \
   && grep -qi 'does NOT exist' <<<"$OUT" \
   && ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "pre-sign pin — a \`pinned_to\` that does not resolve HARD-fails and names the dead path"
else
  fail "arm 5h (dead pin path): rc=$RC out=$OUT"
fi

# --- Arm 5i: ...and the pin is COUNTED, so the registration is proven -------
# Asserting only the failure direction would be satisfied by a special-case
# `os.path.exists` check bolted on beside the citation list. This asserts the
# VERIFIED-PATH COUNT moves: pointing `pinned_to` at an existing path that
# nothing else cites raises the count by exactly one, which can only happen if
# the pin went through the same citation machinery as every other path.
# The default fixture pins to src/conformance/governed-surface-allowlist.json,
# which §4 ALREADY cites, so its count is deliberately unchanged (paths are
# counted distinct) — that is why this arm re-pins to a fresh file.
# Pre-fix the count does NOT move: measured RED.
setup_fixture presigned
run_gen 9.9.9 10 --verify
PIN_BASE_N="$(grep -oE '[0-9]+ path\(s\)' <<<"$OUT" | head -1 | grep -oE '[0-9]+' || true)"
setup_fixture presigned
printf '{"delta": []}\n' >"$FIX/src/conformance/pinned-delta.json"
mutate_state "s['acceptance']['publish_gate']['pre_sign']['pinned_to'] = 'src/conformance/pinned-delta.json'"
run_gen 9.9.9 10 --verify
PIN_NEW_N="$(grep -oE '[0-9]+ path\(s\)' <<<"$OUT" | head -1 | grep -oE '[0-9]+' || true)"
if [ "$RC" -eq 0 ] && [ -n "$PIN_BASE_N" ] && [ -n "$PIN_NEW_N" ] \
   && [ "$PIN_NEW_N" -eq "$((PIN_BASE_N + 1))" ]; then
  pass "pre-sign pin — a distinct \`pinned_to\` raises the verified-path count by exactly 1"
else
  fail "arm 5i (pin counted): rc=$RC base=$PIN_BASE_N new=$PIN_NEW_N (expected $((PIN_BASE_N + 1)))"
fi

# Restore the default fixture for the arms that follow.
setup_fixture
run_gen 9.9.9 10

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
   && grep -qi 'first slice' <<<"$BASE_BLOCK" \
   && grep -q 'branch from `git rev-parse origin/main`' <<<"$BASE_BLOCK"; then
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

# --- Arm 8i2: ...and the warning is not contradicted by an instruction ------
# codex §9 [P2]. The warning alone was not enough: the no-predecessor branch ALSO
# printed "branch from \`git rev-parse origin/main\`", so the same brief warned the
# tip was ahead and then told the operator to branch from it anyway. An operator
# follows the instruction, not the caveat beside it, so the two cases must be
# mutually exclusive. Reuses arm 8i's run (same fixture, same $OUT).
# NOTE: the generic "re-verify \`git rev-parse origin/main\`" line is a different
# statement and stays; the assertion targets the BRANCH instruction only.
if ! grep -q 'branch from `git rev-parse origin/main`' <<<"$OUT" \
   && grep -q 'do NOT branch from the tip' <<<"$OUT" \
   && grep -q 'NO correct automatic' <<<"$OUT" \
   && grep -q 'git log' <<<"$OUT"; then
  pass "a historical first-slice brief is never ALSO told to branch from the tip"
else
  fail "arm 8i2 (contradictory branch-from-tip instruction): rc=$RC out=$OUT"
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

# ===========================================================================
# FOURTH PREDICATE (TC-92 / TC-94), arms 11a-11h: a slice may CITE its design
# docs by hand, and a cited doc is checked exactly like every other citation.
# ===========================================================================
# THE INCIDENT, MEASURED. The design tier has two DISCOVERY selectors — a
# filename selector (`<release>-slice-NN-*.md`) and a token selector over
# `dev/design/**` content — and an entire class of slice defeats both BY
# CONSTRUCTION. `plan-<release>.md` §3 is frozen at Slice 0, so a RESERVED-GAP
# slice minted mid-release by HITL ruling carries requirement ids (R-20-CR,
# R-20-VC, R-20-SV and their TC-* carries) that appear NOWHERE in dev/design/**,
# and it never got a §3.0 memo of its own either. The filename selector hit
# exactly 1 of the 12 slices in the 0.8.20 ladder. The TC-37 vacuous-pass guard
# therefore hard-failed (exit 1) on a perfectly well-designed slice — three in a
# row: Slices 21, 22 and 23.
#
# The fix is an OPTIONAL `design_refs` list on the ladder entry. It is CITATION,
# not discovery, and three properties are the whole point:
#   * ABSENT MEANS ABSENT (arm 11d) — every byte of the manifest for a slice
#     without `design_refs` is what it was before the feature existed, asserted
#     against the PRE-CHANGE generator recovered from git, not against a
#     hand-copied golden.
#   * A CURATED PATH IS A CHECKED PATH (arm 11b) — CHECK 1 applies with no
#     exemption, because curation nobody checks rots exactly like a scan hit.
#   * IT REACHES OUTSIDE `dev/design/` (arms 11c/11c2) — dev/adr/**,
#     dev/interfaces/** — which the scan's walker can never do.
# And the guard it unblocks must not be weakened by it (arms 11e/11e2).

# --- Arm 11a: a `design_refs` doc reaches §6, marked CURATED ----------------
# The cited doc carries NO slice token and is NOT named for this release+slice,
# so NEITHER discovery selector can reach it: if it appears, it appears because
# it was curated. Pre-change the key is ignored entirely — measured RED.
setup_fixture
cat >"$FIX/dev/design/hand-picked.md" <<'EOF'
---
status: locked
---

# Hand-picked

Nothing in here names this slice, its requirement or its carries.
EOF
mutate_state "L[10]['design_refs'] = ['dev/design/hand-picked.md']"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && grep -qE 'CURATED.*dev/design/hand-picked\.md' <<<"$OUT" \
   && grep -qE 'CURATED.*\[locked\]|\[locked\].*CURATED' <<<"$OUT" \
   && ! grep -qE 'CURATED.*widget-design\.md' <<<"$OUT"; then
  pass "design_refs — a hand-cited doc neither selector can find is emitted, marked CURATED"
else
  fail "arm 11a (design_refs emitted + marked): rc=$RC out=$OUT"
fi

# --- Arm 11b: a `design_refs` path that does NOT resolve -> hard FAIL -------
# CHECK 1 with no exemption. A curated citation is the one a human chose, so a
# dead one is MORE misleading than a dead scan hit, not less: it carries the
# Steward's authority. Pre-change: the key was ignored, so the manifest was
# emitted and exited 0 — measured RED.
setup_fixture
mutate_state "L[10]['design_refs'] = ['dev/design/NO-SUCH-DESIGN.md']"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] \
   && grep -q 'NO-SUCH-DESIGN.md' <<<"$OUT" \
   && grep -qi 'does NOT exist' <<<"$OUT" \
   && ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "design_refs — a curated path that does not resolve HARD-fails and emits no manifest"
else
  fail "arm 11b (dead design_refs path): rc=$RC out=$OUT"
fi

# --- Arm 11c: a doc OUTSIDE `dev/design/` is reachable, status told honestly -
# DESIGN_ROOT is a real boundary and the walker is NOT widened (that would drag
# every unrelated doc in); `design_refs` is the mechanism for dev/adr/** and
# dev/interfaces/**. This fixture ADR has no frontmatter at all, so the manifest
# must SAY there is no recorded status rather than invent one.
setup_fixture
mkdir -p "$FIX/dev/adr"
printf '# ADR-9.9.9 — error taxonomy\n\nNo frontmatter at all.\n' \
  >"$FIX/dev/adr/ADR-9.9.9-taxonomy.md"
mutate_state "L[10]['design_refs'] = ['dev/adr/ADR-9.9.9-taxonomy.md']"
run_gen 9.9.9 10
ADR_ROW="$(grep -m1 'ADR-9.9.9-taxonomy.md' <<<"$OUT" || true)"
if [ "$RC" -eq 0 ] \
   && grep -q 'CURATED' <<<"$ADR_ROW" \
   && grep -qiE 'no .?status:' <<<"$ADR_ROW" \
   && ! grep -qE '\[(ACTIVE|locked|accepted|UNREVIEWED)\]' <<<"$ADR_ROW"; then
  pass "design_refs — a dev/adr doc the scan can never reach is cited, and its MISSING status is said so"
else
  fail "arm 11c (out-of-tree design_ref): rc=$RC row=[$ADR_ROW]"
fi

# --- Arm 11c2: ...and an out-of-tree doc that DOES record a status shows it --
# Without this, arm 11c would be satisfied by a generator that simply never
# reads frontmatter outside DESIGN_ROOT.
setup_fixture
mkdir -p "$FIX/dev/adr"
printf -- '---\nstatus: accepted\n---\n\n# ADR-9.9.9\n' >"$FIX/dev/adr/ADR-9.9.9-signed.md"
mutate_state "L[10]['design_refs'] = ['dev/adr/ADR-9.9.9-signed.md']"
run_gen 9.9.9 10
ADR_ROW="$(grep -m1 'ADR-9.9.9-signed.md' <<<"$OUT" || true)"
if [ "$RC" -eq 0 ] && grep -q 'CURATED' <<<"$ADR_ROW" && grep -q 'accepted' <<<"$ADR_ROW"; then
  pass "design_refs — an out-of-tree doc's RECORDED status is read and reported"
else
  fail "arm 11c2 (out-of-tree status read): rc=$RC row=[$ADR_ROW]"
fi

# --- Arm 11d: ABSENT `design_refs` -> byte-identical to the PRE-CHANGE tool --
# The additivity claim, asserted rather than asserted-about. The comparison
# runs the generator as it stood BEFORE `design_refs` existed — recovered from
# git by walking this file's history back to the newest revision that does not
# mention `design_refs` — against the current one, over the SAME fixture, with
# no `design_refs` anywhere in the state file. Any drift at all is a fail.
#
# NON-VACUITY: every way the recovery can come up empty (squashed history, a
# shallow clone, the file renamed) routes to fail(), never to a silent pass. A
# comparison that quietly compares nothing is worse than no comparison.
#
# ---------------------------------------------------------------------------
# TWO CHANGES MADE HERE BY DOC-HYGIENE-3, both loud on purpose.
# ---------------------------------------------------------------------------
# (1) IT USED TO ABORT THE WHOLE SUITE ON ITS FIRST REAL DRIFT. The drift capture
#     ran `$(diff … | head -20 | tr …)` under `set -euo pipefail`; `diff` exits 1
#     when the files differ, `pipefail` propagates that through the pipeline, the
#     assignment inherits it, and the `||` compound therefore returned 1 — so the
#     first genuine difference killed the script instead of reporting it. MEASURED
#     here: the suite exited 1 after arm 11c2 with NO `FAIL` line printed and 25
#     later arms never run. A test that dies rather than reports is worse than a
#     test that is wrong, because its silence reads like the end of a clean run.
#     Fixed by capturing the diff with `|| true`.
#
# (2) ONE ENUMERATED, JUSTIFIED ALLOWANCE. TC-94 defect 1 widened the design scan
#     from `dev/design` alone to `dev/design` + `dev/adr` + `dev/interfaces`, and
#     the manifest DISCLOSES the roots it scanned in its own header. That header
#     line therefore differs from the pre-change generator's, for a reason that
#     has nothing to do with `design_refs`. Freezing the whole output against a
#     historical revision forever is the TC-81 time-bomb class this suite already
#     rejected once (see arm 9d's history): any correct improvement to the tool
#     turns it red against a tool that is right.
#
#     The allowance is NOT "ignore differences". Exactly one line is exempt, it is
#     named by its literal text, and BOTH sides must still carry it — so a second
#     drifting line, or the disappearance of the header line altogether, is still
#     a hard failure. Everything else stays pinned byte-for-byte, which is the
#     property `design_refs` actually needs.
# (3) THE BASELINE RECOVERY WAS NON-DETERMINISTIC, and it silently chose the
#     WRONG revision. The test read `! (git show …) | grep -q design_refs`: under
#     `set -o pipefail` the PIPELINE's status is what `!` inverts, and `grep -q`
#     exits the moment it matches, which SIGPIPEs `git show` (exit 141). A
#     non-zero pipeline then reads as "this revision does not mention
#     design_refs" and the loop breaks on it. MEASURED here across three
#     consecutive runs of this suite on the same history: two picked 220347b4
#     (correct — the last revision before the feature) and one picked 0eb588ba,
#     which is THE COMMIT THAT ADDED `design_refs` and contains the string 21
#     times. An additivity arm comparing the tool against a baseline that already
#     has the feature proves nothing while printing a green line naming a SHA.
#     Fixed by materialising the blob first and grepping the variable, and by
#     treating an empty blob as "not a candidate" rather than as a match.
PRE_GEN="$TMPROOT/commission-manifest-pre.sh"
PRE_SHA=""
while read -r c; do
  [ -n "$c" ] || continue
  PRE_BLOB="$(cd "$REPO_ROOT" && git show "$c:scripts/commission-manifest.sh" 2>/dev/null || true)"
  [ -n "$PRE_BLOB" ] || continue
  if ! grep -q 'design_refs' <<<"$PRE_BLOB"; then PRE_SHA="$c"; break; fi
done < <(cd "$REPO_ROOT" && git log --format=%H -- scripts/commission-manifest.sh)
if [ -z "$PRE_SHA" ]; then
  fail "arm 11d (pre-change generator): no revision of scripts/commission-manifest.sh predates design_refs — cannot prove additivity; do NOT skip this"
else
  (cd "$REPO_ROOT" && git show "$PRE_SHA:scripts/commission-manifest.sh") >"$PRE_GEN"
  chmod +x "$PRE_GEN"
  # The single exempt line, named by the literal text both sides share. Anything
  # that does not contain this needle is compared byte-for-byte.
  ROOTS_LINE_NEEDLE='frontmatter under'
  D11D_DRIFT=""
  for d11d_slice in 10 30; do
    setup_fixture
    printf -- '---\nstatus: ACTIVE\n---\n\n# Slice 30 memo\n' >"$FIX/dev/design/9.9.9-slice-30-design.md"
    set +e
    PRE_OUT="$(cd "$FIX" && bash "$PRE_GEN" 9.9.9 "$d11d_slice" 2>&1)"
    PRE_RC=$?
    POST_OUT="$(cd "$FIX" && ./scripts/commission-manifest.sh 9.9.9 "$d11d_slice" 2>&1)"
    POST_RC=$?
    set -e
    [ "$PRE_RC" -eq 0 ] && [ "$POST_RC" -eq 0 ] \
      || D11D_DRIFT="$D11D_DRIFT slice-$d11d_slice:rc($PRE_RC/$POST_RC)"
    # THE ALLOWANCE MUST HAVE SOMETHING TO ALLOW, on both sides. If either
    # generator stops emitting the roots-disclosure line, the exemption would be
    # silently exempting a line that no longer exists — so that is a failure, not
    # a pass.
    PRE_ROOTS_N="$(grep -c "$ROOTS_LINE_NEEDLE" <<<"$PRE_OUT" || true)"
    POST_ROOTS_N="$(grep -c "$ROOTS_LINE_NEEDLE" <<<"$POST_OUT" || true)"
    [ "$PRE_ROOTS_N" -eq 1 ] && [ "$POST_ROOTS_N" -eq 1 ] \
      || D11D_DRIFT="$D11D_DRIFT slice-$d11d_slice:roots-line-count($PRE_ROOTS_N/$POST_ROOTS_N)"
    # EVERY OTHER LINE stays pinned byte-for-byte.
    PRE_REST="$(grep -v "$ROOTS_LINE_NEEDLE" <<<"$PRE_OUT" || true)"
    POST_REST="$(grep -v "$ROOTS_LINE_NEEDLE" <<<"$POST_OUT" || true)"
    if [ "$PRE_REST" != "$POST_REST" ]; then
      D11D_DIFF="$(diff <(printf '%s\n' "$PRE_REST") <(printf '%s\n' "$POST_REST") | head -20 | tr '\n' '~' || true)"
      D11D_DRIFT="$D11D_DRIFT slice-$d11d_slice:$D11D_DIFF"
    fi
  done
  if [ -z "$D11D_DRIFT" ]; then
    pass "design_refs is ADDITIVE — with the key absent every line except the enumerated roots-disclosure line is byte-identical to the pre-change generator ($PRE_SHA)"
  else
    fail "arm 11d (additivity): drift:$D11D_DRIFT"
  fi
fi

# --- Arm 11e: the TC-37 vacuous guard is NOT weakened by the feature --------
# `design_refs: []` is not a design of record. With both selectors empty AND an
# empty curated list, the manifest still briefs nobody, so it must still die.
setup_fixture
perl -0777 -pi -e 's/"short": "R-9-B", "title": "widget_readiness \+ the TC-99 terminal fix"/"short": "R-0-ZZ", "title": "nothing matches this"/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
rm -f "$FIX/dev/design/9.9.9-slice-10-design.md"
mutate_state "L[10]['design_refs'] = []"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'TC-37' <<<"$OUT" && ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "vacuity guard — an EMPTY \`design_refs\` plus an empty scan still HARD-fails (TC-37)"
else
  fail "arm 11e (empty design_refs still vacuous): rc=$RC out=$OUT"
fi

# --- Arm 11e2: ...but a NON-empty one legitimately unblocks the same slice ---
# The direction the feature exists for, and the proof that arm 11e's green is
# about emptiness rather than about the generator refusing to look. Same
# otherwise-vacuous slice, one curated citation added. Pre-change: RED (dies).
setup_fixture
cat >"$FIX/dev/design/hand-picked.md" <<'EOF'
---
status: locked
---

# Hand-picked

Nothing in here names this slice, its requirement or its carries.
EOF
perl -0777 -pi -e 's/"short": "R-9-B", "title": "widget_readiness \+ the TC-99 terminal fix"/"short": "R-0-ZZ", "title": "nothing matches this"/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
rm -f "$FIX/dev/design/9.9.9-slice-10-design.md"
mutate_state "L[10]['design_refs'] = ['dev/design/hand-picked.md']"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && grep -qE 'CURATED.*hand-picked\.md' <<<"$OUT" \
   && grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "design_refs — a reserved-gap slice both selectors miss is commissionable via curation"
else
  fail "arm 11e2 (curation unblocks the guard): rc=$RC out=$OUT"
fi

# --- Arm 11f: the coverage report names unmatched tokens even when others hit
# TC-94 defect 3. A slice with ONE weak incidental match must not read like a
# slice with full coverage: the report is per-TOKEN, so a token nothing mentions
# is named even while its neighbours matched. Kept a REPORT, never a failure —
# the run still exits 0.
#
# HONEST LABEL: this arm is GREEN at baseline. The brief that commissioned it
# stated the report fired "only when every token misses"; that premise is
# measurably false (0.8.20 Slices 15/20/21/31 all emit it with other tokens
# matched, and `git log -S` shows the block has never changed since 1b9cf9a3).
# It is therefore a LOCK on existing behaviour, not a RED-first test.
setup_fixture
mutate_state "L[10]['title'] = L[10]['title'] + ' and the TC-404 carry'"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && grep -q 'NO design doc mentions' <<<"$OUT" \
   && grep -qE 'NO design doc mentions:[^.]*TC-404' <<<"$OUT" \
   && grep -q 'dev/design/widget-design.md' <<<"$OUT" \
   && ! grep -qE 'NO design doc mentions:[^.]*(TC-99|widget_readiness)' <<<"$OUT"; then
  pass "coverage report — an unmatched token is named even though the slice's other tokens matched"
else
  fail "arm 11f (partial-coverage report): rc=$RC out=$OUT"
fi

# --- Arm 11g: a malformed `design_refs` HARD-fails rather than being ignored -
# Silently dropping a mistyped curation would put the slice straight back into
# the vacuous-guard failure it was added to fix, with no explanation.
setup_fixture
mutate_state "L[10]['design_refs'] = 'dev/design/widget-design.md'"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'design_refs' <<<"$OUT" && ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "design_refs — a non-list value HARD-fails instead of being silently ignored"
else
  fail "arm 11g (malformed design_refs): rc=$RC out=$OUT"
fi

# --- Arm 11h: an ABSOLUTE or escaping `design_refs` path HARD-fails ---------
# Every other cited path in the manifest is repo-relative; a curated one that
# escapes the checkout would resolve on the author's machine and nowhere else.
setup_fixture
mutate_state "L[10]['design_refs'] = ['../outside.md']"
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'design_refs' <<<"$OUT" && ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "design_refs — a path escaping the repo root HARD-fails"
else
  fail "arm 11h (escaping design_refs path): rc=$RC out=$OUT"
fi

# --- Arm 11i: a curated CONTRACT reaches §4, not only §6 -------------------
# §4 is where an orchestrator looks for the document that WINS on conflict, and
# the motivating case is exactly a contract: 0.8.20 Slice 22 cites
# `OPP-12-C1-converged-contract.md`, byte-pinned by scripts/c1-conformance-pin.json
# (sha256 AND git blob sha1), the ratified contract whose Q4/Q6(a) clause a TC-67
# implementation can turn RED. A pinned file can never be back-linked — any edit
# breaks the pin — so citation is the ONLY mechanism that can put it in front of
# the orchestrator, and §6 alone is the wrong place for it.
#
# The fixture doc carries no slice token and is not named for this release+slice,
# so neither selector can reach it. The default fixture ALSO has a scan-reached
# contract (`sub/widget-contract.md`), and this arm asserts the two render
# DIFFERENTLY: curated is marked, scanned is not. Pre-change: RED (§4 iterates
# scan hits only, so the curated contract never appears).
setup_fixture
cat >"$FIX/dev/design/hand-picked-contract.md" <<'EOF'
---
status: ratified
---

# Hand-picked contract

Nothing in here names this slice, its requirement or its carries.
EOF
mutate_state "L[10]['design_refs'] = ['dev/design/hand-picked-contract.md']"
run_gen 9.9.9 10
S4="$(sed -n '/^## 4\. CONTRACT PATHS/,/^## 5\./p' <<<"$OUT")"
if [ "$RC" -eq 0 ] \
   && grep -qE 'design contract.*CURATED.*hand-picked-contract\.md' <<<"$S4" \
   && grep -q 'ratified' <<<"$S4" \
   && grep -q 'sub/widget-contract.md' <<<"$S4" \
   && ! grep -qE 'CURATED.*sub/widget-contract\.md' <<<"$S4"; then
  pass "design_refs — a curated CONTRACT reaches §4 marked CURATED, and a scanned one is not marked"
else
  fail "arm 11i (curated contract in §4): rc=$RC s4=[$S4]"
fi

# --- Arm 11i2: a curated NON-contract does NOT leak into §4 ----------------
# Without this, "feed design_refs into §4" degenerates into pasting §6 into §4
# and the CONTRACT PATHS section stops meaning anything. §4 selects on the same
# predicate it always did — the basename — applied to the curated set as well.
setup_fixture
cat >"$FIX/dev/design/hand-picked.md" <<'EOF'
---
status: locked
---

# Hand-picked

Nothing in here names this slice, its requirement or its carries.
EOF
mutate_state "L[10]['design_refs'] = ['dev/design/hand-picked.md']"
run_gen 9.9.9 10
S4="$(sed -n '/^## 4\. CONTRACT PATHS/,/^## 5\./p' <<<"$OUT")"
if [ "$RC" -eq 0 ] \
   && ! grep -q 'hand-picked.md' <<<"$S4" \
   && grep -qE 'CURATED.*hand-picked\.md' <<<"$OUT"; then
  pass "design_refs — a curated doc that is NOT a contract stays in §6 and out of §4"
else
  fail "arm 11i3 (curated non-contract leaks into §4): rc=$RC s4=[$S4]"
fi

# --- Arm 11i3: §4 is byte-identical when `design_refs` is absent ------------
# Asserted, not assumed. Arm 11d compares the WHOLE manifest, which subsumes
# this; §4 is called out separately because it is the section this change
# reaches into, and a section-scoped failure message is what a future reader
# needs. Reuses the pre-change generator arm 11d recovered from git.
if [ -z "${PRE_SHA:-}" ]; then
  fail "arm 11i4 (§4 additivity): no pre-change generator recovered (see arm 11d) — do NOT skip this"
else
  S4_DRIFT=""
  for d11i_slice in 10 30; do
    setup_fixture
    printf -- '---\nstatus: ACTIVE\n---\n\n# Slice 30 memo\n' >"$FIX/dev/design/9.9.9-slice-30-design.md"
    set +e
    S4_PRE="$(cd "$FIX" && bash "$PRE_GEN" 9.9.9 "$d11i_slice" 2>&1 \
              | sed -n '/^## 4\. CONTRACT PATHS/,/^## 5\./p')"
    S4_POST="$(cd "$FIX" && ./scripts/commission-manifest.sh 9.9.9 "$d11i_slice" 2>&1 \
               | sed -n '/^## 4\. CONTRACT PATHS/,/^## 5\./p')"
    set -e
    [ -n "$S4_PRE" ] || S4_DRIFT="$S4_DRIFT slice-$d11i_slice:empty-pre-section"
    [ "$S4_PRE" = "$S4_POST" ] || S4_DRIFT="$S4_DRIFT slice-$d11i_slice:changed"
  done
  if [ -z "$S4_DRIFT" ]; then
    pass "§4 CONTRACT PATHS is byte-identical to the pre-change generator when \`design_refs\` is absent"
  else
    fail "arm 11i4 (§4 additivity): drift:$S4_DRIFT"
  fi
fi

# ===========================================================================
# FIFTH PREDICATE (TC-100 + TC-94 defects 1 and 2), arms 12a-12k: the DISCOVERY
# half selects the RIGHT documents — matched on WHOLE tokens, over the three
# roots that actually hold design authority, for EVERY leg the ladder entry
# names including the bare-number ones.
# ===========================================================================
# Three measured defects in one selector, all of which make the required-reading
# list wrong rather than merely short:
#
#  TC-100 — SUBSTRING MATCHING. `matched = [t for t in tokens if t in text]` has
#    no word boundary, so the token `C-1` matches inside `TC-15`, `TC-100`,
#    `TC-19`… MEASURED on the live repo before this change: 0.8.20 Slice 15 cited
#    EIGHTEEN documents on that one token, among them
#    `dev/design/0.8.4-graphrag-sensemaking.md` and four
#    `fathomdb-memex-overall-roadmap/*` drafts that cannot inform a projection
#    registry. A brief is a REQUIRED-READING list: padding it spends the
#    orchestrator's context on documents that cannot help, and — worse — makes a
#    slice with genuinely thin design coverage look thoroughly supported, which
#    is the exact condition the TC-37 vacuous-pass guard exists to surface. Short
#    ids are the hazard: `C-1`, `C-2`, and any `AC-NN`/`TC-NN` that is a prefix
#    of a longer id.
#
#  TC-94 (1) — UNREACHABLE TIERS. `scan_design` walked `dev/design` alone, so
#    `dev/adr/**` and `dev/interfaces/**` could never appear in a brief AT ANY
#    STATUS, no matter what was back-linked into them. An ADR is a HIGHER tier of
#    authority than a design memo (`ADR-0.6.0-error-taxonomy.md` is the ruling
#    document for decision #18; `ADR-0.8.18-vector-equivalence-self-check.md` is
#    HITL-SIGNED), and `dev/interfaces/*.md` is the surface AGENTS.md §25 obliges
#    an error-surface change to update — so the scan was blind to precisely the
#    strongest evidence. `design_refs` is NOT a fix for this: it is the CITATION
#    half, it requires a human to have known to curate, and the eight ladder
#    entries with no curation got nothing.
#
#  TC-94 (2) — BARE-NUMBER LEGS ARE INVISIBLE. `slice_tokens` derives tokens by
#    regex from the ladder entry's own `short`/`title`, and the shapes `#18` and
#    `#99` match none of its patterns. Two of 0.8.20 Slice 22's four legs
#    therefore contributed NOTHING to selection and no document could ever be
#    matched for them.
#
# (TC-94 defect (3) as originally filed — "a full token match SUPPRESSES the
# manifest's own NO-design-doc-mentions warning" — was WITHDRAWN as FALSE at
# steward `seq-146`, measured: the report is per-token and always has been. Arm
# 11f already locks that behaviour and is labelled as a lock, not a RED-first
# test. Nothing here re-opens it.)
#
# THE FIXES ARE COUPLED AND THEIR EVIDENCE IS A DIFF. Word-boundary matching
# SHRINKS the cited set, widening the roots GROWS it, and bare-number tokens grow
# it again — so "it still exits 0" proves nothing. The before/after citation-set
# diff across every landed 0.8.20 slice is recorded at
# `dev/plans/runs/DOC-HYGIENE-3-citation-set-diff.md`; these arms pin the
# MECHANISM the diff is evidence for.

# --- Arm 12a: a SHORT token must not match inside a LONGER id (TC-100) ------
# The fixture form of the live Slice-15 defect. `longer-id-only.md` says `TC-15`
# and `TC-100` and never `C-1` on its own; `genuine-c1.md` states the token. Both
# were cited before this change — measured RED, and the pair is what makes the
# arm about BOUNDARIES rather than about matching less.
setup_fixture
mutate_state "L[10]['title'] = L[10]['title'] + ' plus the C-1 projection registry'"
cat >"$FIX/dev/design/longer-id-only.md" <<'EOF'
---
status: ACTIVE
---

# Only ever the longer ids

TC-15 and TC-100 are discussed here. The contract id never appears on its own.
EOF
cat >"$FIX/dev/design/genuine-c1.md" <<'EOF'
---
status: ACTIVE
---

# The contract

C-1 is the converged contract id, stated as a whole token.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && grep -q 'dev/design/genuine-c1.md' <<<"$OUT" \
   && grep -qE 'matched:.*C-1' <<<"$OUT" \
   && ! grep -q 'dev/design/longer-id-only.md' <<<"$OUT"; then
  pass "token matching is WORD-BOUNDED — \`C-1\` matches \`C-1\` and NOT \`TC-15\`/\`TC-100\` (TC-100)"
else
  fail "arm 12a (word-boundary token match): rc=$RC out=$OUT"
fi

# --- Arm 12b: the boundary holds on the trailing side too ------------------
# `TC-99` must not match inside `TC-990`. Without this, a fix that only guarded
# the LEADING edge (e.g. prefixing `\b` and stopping) would pass arm 12a.
setup_fixture
cat >"$FIX/dev/design/trailing-run-on.md" <<'EOF'
---
status: ACTIVE
---

# A longer id that starts with a shorter one

TC-990 is a different carry entirely, and R-9-B7 is a different requirement.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && ! grep -q 'dev/design/trailing-run-on.md' <<<"$OUT"; then
  pass "token matching is bounded on BOTH sides — \`TC-99\` does not match inside \`TC-990\`"
else
  fail "arm 12b (trailing boundary): rc=$RC out=$OUT"
fi

# --- Arm 12c: dev/adr/** and dev/interfaces/** are REACHABLE BY THE SCAN ----
# TC-94 (1). Neither document is curated — there is no `design_refs` here — so
# they can only appear if the walker reaches them. Measured RED: the walker
# covered `dev/design` alone, so an ADR could never be cited at any status.
setup_fixture
mkdir -p "$FIX/dev/adr" "$FIX/dev/interfaces"
cat >"$FIX/dev/adr/ADR-9.9.9-widget-authority.md" <<'EOF'
---
status: accepted
---

# ADR-9.9.9 — the ruling document

widget_readiness is DECIDED here; this ADR outranks any design memo.
EOF
cat >"$FIX/dev/interfaces/rust.md" <<'EOF'
# Rust surface

The TC-99 terminal fix changes this surface and must be recorded here.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && grep -q 'dev/adr/ADR-9.9.9-widget-authority.md' <<<"$OUT" \
   && grep -q 'dev/interfaces/rust.md' <<<"$OUT" \
   && ! grep -qE 'CURATED.*(ADR-9.9.9-widget-authority|interfaces/rust)' <<<"$OUT"; then
  pass "scan roots — an ADR and an interface doc are DISCOVERED (not curated) by the token scan (TC-94 (1))"
else
  fail "arm 12c (adr/interfaces reachable): rc=$RC out=$OUT"
fi

# --- Arm 12c2: ...and the ADR's RECORDED status is reported, not invented ---
# The scan must read frontmatter outside `dev/design/` exactly as it does inside
# it, and the interface doc — which carries no frontmatter at all — must say so
# rather than be laundered into a classification it never made.
ADR_SCAN_ROW="$(grep -m1 'dev/adr/ADR-9.9.9-widget-authority.md' <<<"$OUT" || true)"
IFACE_SCAN_ROW="$(grep -m1 'dev/interfaces/rust.md' <<<"$OUT" || true)"
if grep -q '\[accepted\]' <<<"$ADR_SCAN_ROW" \
   && grep -qE '\[\(no status:\)\]' <<<"$IFACE_SCAN_ROW"; then
  pass "scan roots — an out-of-tree doc's recorded status is read, and a missing one is SAID to be missing"
else
  fail "arm 12c2 (out-of-tree status honesty): adr=[$ADR_SCAN_ROW] iface=[$IFACE_SCAN_ROW]"
fi

# --- Arm 12c3: the roots are EXACTLY THREE, not "all of dev/" --------------
# Widening the walker to the whole tree would drag every unrelated document into
# every brief — the reason the original author declined to widen it at all. This
# arm is what keeps the widening bounded: a `dev/plans/` note carrying a live
# token must NOT be cited.
setup_fixture
cat >"$FIX/dev/plans/stray-note.md" <<'EOF'
# A planning note, not design authority

widget_readiness gets a mention here, in passing.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && ! grep -q 'dev/plans/stray-note.md' <<<"$OUT"; then
  pass "scan roots are BOUNDED — a token-carrying doc outside the three roots is not cited"
else
  fail "arm 12c3 (roots bounded): rc=$RC out=$OUT"
fi

# --- Arm 12c4: a CURATED out-of-tree doc the scan ALSO reaches prints ONCE --
# `curated_design_refs` is a separate, hand-cited half and must keep working
# exactly as it does. The one thing that legitimately changes: a curated ADR the
# scan can now reach is reported ONCE, in the curated block, with its scan hits
# folded in — never twice, and never with the now-false claim that the scan
# cannot reach it.
setup_fixture
mkdir -p "$FIX/dev/adr"
cat >"$FIX/dev/adr/ADR-9.9.9-widget-authority.md" <<'EOF'
---
status: accepted
---

# ADR-9.9.9 — the ruling document

widget_readiness is DECIDED here; this ADR outranks any design memo.
EOF
mutate_state "L[10]['design_refs'] = ['dev/adr/ADR-9.9.9-widget-authority.md']"
run_gen 9.9.9 10
ADR_ROWS="$(grep -c 'dev/adr/ADR-9.9.9-widget-authority.md' <<<"$OUT" || true)"
if [ "$RC" -eq 0 ] && [ "$ADR_ROWS" -eq 1 ] \
   && grep -qE 'CURATED.*ADR-9.9.9-widget-authority' <<<"$OUT" \
   && grep -q 'the scan reached it too, on: widget_readiness' <<<"$OUT" \
   && ! grep -q 'cited BECAUSE the scan cannot reach it' <<<"$OUT"; then
  pass "curation is unchanged — a curated ADR the scan now reaches prints once, with its scan hits folded in"
else
  fail "arm 12c4 (curated + scanned ADR): rc=$RC rows=$ADR_ROWS out=$OUT"
fi

# --- Arm 12d: BARE-NUMBER legs become tokens (TC-94 (2)) -------------------
# `#18` and `#99` matched none of the token patterns, so two of 0.8.20 Slice 22's
# four legs contributed nothing to selection at all. Measured RED: the token line
# never carried them and no document could be matched for them.
setup_fixture
mutate_state "L[10]['title'] = L[10]['title'] + ' plus decision #18 and sqlite-vec #99'"
cat >"$FIX/dev/design/bare-number-leg.md" <<'EOF'
---
status: ACTIVE
---

# The decision the leg depends on

Decision #18 is settled here, and sqlite-vec #99 is the upstream issue.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && grep -qE "OWN tokens from the state file:.*#18" <<<"$OUT" \
   && grep -qE "OWN tokens from the state file:.*#99" <<<"$OUT" \
   && grep -q 'dev/design/bare-number-leg.md' <<<"$OUT" \
   && grep -qE 'matched:.*#18' <<<"$OUT"; then
  pass "bare-number legs — \`#18\`/\`#99\` are derived as tokens and select a document (TC-94 (2))"
else
  fail "arm 12d (bare-number tokens): rc=$RC out=$OUT"
fi

# --- Arm 12d2: ...and a bare-number token is bounded too -------------------
# `#18` must not match inside `#180`. Without this the new pattern would
# reintroduce TC-100 in a new shape the moment it shipped.
setup_fixture
mutate_state "L[10]['title'] = L[10]['title'] + ' plus decision #18'"
cat >"$FIX/dev/design/bare-number-near-miss.md" <<'EOF'
---
status: ACTIVE
---

# A different issue

Issue #180 is a wholly different thing and must not be dragged in.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && ! grep -q 'dev/design/bare-number-near-miss.md' <<<"$OUT"; then
  pass "bare-number tokens are bounded — \`#18\` does not match inside \`#180\`"
else
  fail "arm 12d2 (bare-number boundary): rc=$RC out=$OUT"
fi

# --- Arm 12d3: a SINGLE-DIGIT `#N` is NOT a token, and that is measured -----
# The bare-number pattern requires TWO OR MORE digits, and this arm is the reason
# why. MEASURED across `dev/design/**`, `dev/adr/**` and `dev/interfaces/**` at
# the time of the change: word-bounded `#18` occurs in 6 documents (errors.md,
# the sqlite-vec #99 probe memo, ADR-0.6.0-error-taxonomy.md and all three
# `dev/interfaces/*.md` surfaces) and `#99` in 3 — every one of them genuinely
# about that decision or that upstream issue. `#3` occurs in 11 documents and
# `#1` in 16, none of them about a leg: they are prose ordinals ("item #3").
#
# THE COST IS NOT NOISE, IT IS THE GUARD. 0.8.20 Slices 31/32/33 are titled
# "Library Sweep #3, leg N of 3" — a sweep ordinal, not a design id. Slices 32
# and 33 have NO design of record yet and correctly HARD-FAIL the TC-37
# vacuous-pass guard today. A one-digit token would have matched them 11
# incidental documents apiece and turned that honest hard failure into a brief
# that looks supported — TC-100's disease, reintroduced by TC-94 (2)'s cure. So
# the floor of two digits is not a tidiness rule; it is what stops this change
# from defeating the guard the whole file exists to serve.
setup_fixture
mutate_state "L[10]['title'] = L[10]['title'] + ' — Library Sweep #3, leg 1 of 3'"
cat >"$FIX/dev/design/single-digit-ordinal.md" <<'EOF'
---
status: ACTIVE
---

# A doc with a prose ordinal in it

Item #3 in the list below is unrelated to any slice.
EOF
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && ! grep -qE "OWN tokens from the state file:.*#3" <<<"$OUT" \
   && ! grep -q 'dev/design/single-digit-ordinal.md' <<<"$OUT"; then
  pass "bare-number tokens need TWO digits — a one-digit \`#3\` ordinal is not a selector"
else
  fail "arm 12d3 (single-digit ordinal): rc=$RC out=$OUT"
fi

# --- Arm 12d4: ...and the TC-37 guard still fires for an undesigned slice ----
# The direct consequence asserted rather than argued. A slice whose tokens match
# nothing must still HARD-FAIL even though its title carries a `#N` ordinal that
# a laxer pattern would have "covered".
setup_fixture
perl -0777 -pi -e 's/"short": "R-9-B", "title": "widget_readiness \+ the TC-99 terminal fix"/"short": "SWEEP", "title": "Library Sweep #3, leg 2 of 3"/' \
  "$FIX/dev/plans/release-state-9.9.9.json"
rm -f "$FIX/dev/design/9.9.9-slice-10-design.md"
cat >"$FIX/dev/design/single-digit-ordinal.md" <<'EOF'
---
status: ACTIVE
---

# A doc with a prose ordinal in it

Item #3 in the list below is unrelated to any slice.
EOF
run_gen 9.9.9 10
if [ "$RC" -ne 0 ] && grep -q 'TC-37' <<<"$OUT" && ! grep -q 'COMMISSION MANIFEST' <<<"$OUT"; then
  pass "vacuity guard — a slice whose only bare number is a one-digit ordinal still HARD-fails (TC-37)"
else
  fail "arm 12d4 (ordinal does not defeat the guard): rc=$RC out=$OUT"
fi

# --- Arm 12e: the LIVE repo — EVERY reported match is a whole-token match ---
# TC-100 asserted against the shipped state file rather than a fixture, and
# DERIVED rather than literal (TC-81: a hardcoded "Slice 15 must not cite the
# GraphRAG memo" is a time bomb that goes red the day the ladder moves). For
# every slice in the live 0.8.20 ladder, every token the manifest REPORTS in a
# `matched:` list must occur in that document as a whole token. Measured RED:
# Slice 15 alone reported 18 documents matched on `C-1` where the file only ever
# says `TC-15`/`TC-19`/`TC-100`.
#
# NON-VACUITY: the check FAILS if it inspected fewer than 20 (doc, token) pairs.
# A parser that silently matched nothing would otherwise pass green.
set +e
BOUND_OUT="$(cd "$REPO_ROOT" && python3 - <<'PYEOF' 2>&1
import json, re, subprocess, sys

state = json.load(open("dev/plans/release-state-0.8.20.json"))
# The LANDED slices plus the next one: exactly the set whose manifest is required
# to generate. A NOT_STARTED slice with no design of record yet HARD-FAILS the
# TC-37 guard by design (0.8.20 Slices 32/33 do today), and folding that expected
# hard failure in here would make this arm red for a reason that is not its own.
slices = sorted({s for s in state["landed"] if isinstance(s, int)}
                | ({state["next_slice"]} if isinstance(state["next_slice"], int) else set()))
if len(slices) < 3:
    print("ERR only %d live slice(s) to check — too few to vouch for anything" % len(slices))
    raise SystemExit(0)

bad, pairs = [], 0
for sl in slices:
    p = subprocess.run(["bash", "scripts/commission-manifest.sh", "0.8.20", str(sl)],
                       capture_output=True, text=True)
    if p.returncode != 0:
        print("ERR slice %s: manifest exited %d" % (sl, p.returncode)); raise SystemExit(0)
    sec, last = None, None
    for ln in p.stdout.split("\n"):
        if ln.startswith("## "):
            sec = ln[3:].split(".")[0].strip()
            continue
        if sec != "6":
            continue
        m = re.search(r"`([A-Za-z0-9_./+-]+\.md)`", ln)
        if m and (ln.startswith("  [") or ln.startswith("  CURATED [")):
            last = m.group(1)
            continue
        if last and ln.strip().startswith("matched:"):
            toks = [t.strip() for t in ln.split("matched:", 1)[1].split(",") if t.strip()]
            try:
                text = open(last, encoding="utf-8", errors="replace").read()
            except OSError as exc:
                print("ERR cannot read cited %s: %s" % (last, exc)); raise SystemExit(0)
            for t in toks:
                if t.startswith("filename:"):
                    continue
                pairs += 1
                rx = r"(?<![0-9A-Za-z_])" + re.escape(t) + r"(?![0-9A-Za-z_])"
                if not re.search(rx, text):
                    bad.append("slice %s: %s reported matched on %r, which occurs "
                               "only inside a longer id" % (sl, last, t))
            last = None
if pairs < 20:
    print("ERR only %d (doc, token) pair(s) inspected — too few to vouch for anything" % pairs)
    raise SystemExit(0)
print("OK %d pair(s) checked, %d spurious" % (pairs, len(bad)))
for b in bad[:12]:
    print("  " + b)
PYEOF
)"
BOUND_RC=$?
set -e
if [ "$BOUND_RC" -eq 0 ] && grep -q '^OK ' <<<"$BOUND_OUT" && grep -q ', 0 spurious' <<<"$BOUND_OUT"; then
  pass "real repo — every reported \`matched:\` token occurs as a WHOLE token in the doc it cites ($BOUND_OUT)"
else
  fail "arm 12e (live substring matches): rc=$BOUND_RC out=$BOUND_OUT"
fi

# --- Arm 12f: the LIVE repo — every bare-number leg becomes a token --------
# TC-94 (2) against the shipped state file, derived from the ladder itself so it
# follows whatever the ladder says next. Non-vacuity: if NO live ladder entry
# carries a `#NN` leg the arm FAILS — 0.8.20 Slice 22's `decision #18` and
# `sqlite-vec #99` are exactly why this exists.
set +e
BARE_OUT="$(cd "$REPO_ROOT" && python3 - <<'PYEOF' 2>&1
import json, re, subprocess

state = json.load(open("dev/plans/release-state-0.8.20.json"))
live = {s for s in state["landed"] if isinstance(s, int)}
if isinstance(state["next_slice"], int):
    live.add(state["next_slice"])
seen, bad = 0, []
for e in state["ladder"]:
    if e["slice"] not in live:
        continue
    blob = "%s %s" % (e.get("short") or "", e.get("title") or "")
    # TWO OR MORE digits: `#3` in "Library Sweep #3" is a prose ordinal, not a
    # leg, and is deliberately not a token (arms 12d3/12d4).
    legs = re.findall(r"(?<![0-9A-Za-z_])#[0-9]{2,}(?![0-9A-Za-z_])", blob)
    if not legs:
        continue
    p = subprocess.run(["bash", "scripts/commission-manifest.sh", "0.8.20", str(e["slice"])],
                       capture_output=True, text=True)
    if p.returncode != 0:
        bad.append("slice %s: manifest exited %d" % (e["slice"], p.returncode)); continue
    line = ""
    for ln in p.stdout.split("\n"):
        if "OWN tokens from the state file:" in ln:
            line = ln; break
    for leg in legs:
        seen += 1
        if leg not in line:
            bad.append("slice %s: leg %s is not in the token line %r" % (e["slice"], leg, line))
if seen == 0:
    print("ERR no live ladder entry carries a bare-number leg — nothing asserted")
else:
    print("OK %d bare-number leg(s) checked, %d missing" % (seen, len(bad)))
for b in bad[:12]:
    print("  " + b)
PYEOF
)"
BARE_RC=$?
set -e
if [ "$BARE_RC" -eq 0 ] && grep -q '^OK ' <<<"$BARE_OUT" && grep -q ', 0 missing' <<<"$BARE_OUT"; then
  pass "real repo — every bare-number leg in the live ladder reaches the token line ($BARE_OUT)"
else
  fail "arm 12f (live bare-number legs): rc=$BARE_RC out=$BARE_OUT"
fi

# --- Arm 12g: the LIVE repo — the higher-authority tiers are now REACHED ----
# TC-94 (1) asserted end-to-end. Across the live 0.8.20 ladder, at least one
# document under `dev/adr/` or `dev/interfaces/` must be reached by the SCAN
# (not by curation) — otherwise the widening shipped without ever having done
# anything, which is the shape of change that quietly regresses.
set +e
TIER_OUT="$(cd "$REPO_ROOT" && python3 - <<'PYEOF' 2>&1
import json, re, subprocess

state = json.load(open("dev/plans/release-state-0.8.20.json"))
live = {s for s in state["landed"] if isinstance(s, int)}
if isinstance(state["next_slice"], int):
    live.add(state["next_slice"])
hits = []
for e in state["ladder"]:
    sl = e["slice"]
    if sl not in live:
        continue
    curated = set(e.get("design_refs") or [])
    p = subprocess.run(["bash", "scripts/commission-manifest.sh", "0.8.20", str(sl)],
                       capture_output=True, text=True)
    if p.returncode != 0:
        print("ERR slice %s: manifest exited %d" % (sl, p.returncode)); raise SystemExit(0)
    sec = None
    for ln in p.stdout.split("\n"):
        if ln.startswith("## "):
            sec = ln[3:].split(".")[0].strip(); continue
        if sec != "6" or not ln.startswith("  ["):
            continue
        m = re.search(r"`([A-Za-z0-9_./+-]+\.md)`", ln)
        if not m:
            continue
        path = m.group(1)
        if path.startswith(("dev/adr/", "dev/interfaces/")) and path not in curated:
            hits.append("slice %s -> %s" % (sl, path))
print("OK %d scanned higher-tier citation(s)" % len(hits))
for h in hits[:12]:
    print("  " + h)
PYEOF
)"
TIER_RC=$?
set -e
TIER_N="$(grep -oE '^OK [0-9]+' <<<"$TIER_OUT" | grep -oE '[0-9]+' || true)"
if [ "$TIER_RC" -eq 0 ] && [ -n "$TIER_N" ] && [ "$TIER_N" -gt 0 ]; then
  pass "real repo — the scan now reaches $TIER_N dev/adr or dev/interfaces doc(s) it could never reach before"
else
  fail "arm 12g (live higher-tier reach): rc=$TIER_RC out=$TIER_OUT"
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

# --- Arm 9d: the real repo's base line, STATE-DERIVED (TC-79 / TC-81) -------
# The live regression half of the predecessor fix. It asserts the base LINE and
# not the whole manifest, because every landing SHA also appears in the `landed
# so far` roll-up, so a whole-manifest grep cannot tell a correct base from a
# wrong one. That intent is unchanged.
#
# WHY IT IS DERIVED RATHER THAN LITERAL. The previous form hardcoded Slice 20 as
# its "currently in flight" example, hardcoded Slice 15 / a2022957 as the
# expected base, and asserted UNCONDITIONALLY that the real manifest carried no
# HISTORICAL banner. Its own comment said the predecessor and max(landed) agree
# "TODAY" — it documented its own expiry date and shipped anyway. Slice 20 then
# landed (841c307b), the generator CORRECTLY began emitting
# `⚠ HISTORICAL  Slice 20 is ITSELF LANDED`, and this arm went permanently RED
# against a tool that was right: a time-bombed assertion, the TC-81 hazard class
# (first instance efa8d584). Re-pointing the literal at a later slice is the
# same bomb with a later fuse, so it was rejected.
#
# The form below reads the target (`next_slice`, unlanded by definition), its
# predecessor (the greatest `landed` entry STRICTLY below the target) and that
# predecessor's landing SHA out of the single-writer state file, and asserts the
# HISTORICAL banner as an EQUIVALENCE: present if and only if the target is
# itself in `landed`. That is strictly stronger than the old one-directional
# prohibition — it now also catches a banner that fails to appear — and it stays
# true whatever the ladder does next.
#
# NON-VACUITY IS THE POINT: there is no skip path. Every way the derivation can
# come up empty (end-of-ladder `next_slice: null`, no landed predecessor, a
# predecessor with no recorded SHA, an unreadable state file) routes to fail(),
# never to a silent pass. A vacuously-green gate is worse than the bug it was
# meant to catch, and this repo has been bitten by exactly that before.
#
# THE ORACLE IS A FUNCTION, not an inline heredoc, because arm 13g grades the
# SAME oracle against a fractional-id state file. An oracle that only ever ran
# against the all-integer live state could never be shown to be wrong about a
# fractional one — which is exactly how this arm came to REPLICATE the
# generator's own int-only assumptions (brief §1c) instead of falsifying them.
#
# ⛔ AND IT MUST NOT REPLICATE THE GENERATOR'S ASSUMPTIONS. An oracle that copies
# the implementation cannot falsify it. This one did, in TWO places (brief §1c):
#
#   * `isinstance(t, int)` on `next_slice` routed a FRACTIONAL target to
#     `ERR … not an int` -> fail(), so the arm REJECTED the very input this
#     unit exists to support. That is what made this suite rc=1 outright while
#     `Slice 39.5` sat in the ladder.
#   * `isinstance(n, int)` on `landed` was semantically identical to the
#     generator's own defect at commission-manifest.sh:701 — filtering a whole
#     landed unit out of the predecessor set, in the check written to catch
#     exactly that.
#
# Repairing only one of them leaves the other live, so both are stated here, and
# arm 13h grades this function against a fractional state file where each is
# independently observable (a fractional target AND a fractional predecessor).
derive_base_from_state() {     # $1 = release-state json path
  python3 -c '
import json, sys


def sid(n):
    """Render a slice id. NEVER %d: `"%d" % 39.5` is `"39"`, so an oracle that
    printed its own finding with %d would report the WRONG SLICE while looking
    right — the fabricated-pointer failure this suite exists to catch, in the
    catcher."""
    return "%g" % n if isinstance(n, float) else str(n)


try:
    s = json.load(open(sys.argv[1]))
except Exception as e:
    print("ERR unreadable state file: %s" % e); raise SystemExit(0)
t = s.get("next_slice")
# int OR float — a fractional slice is a legitimate target. `bool` stays
# excluded because True is an int in Python and would pass as Slice 1.
if isinstance(t, bool) or not isinstance(t, (int, float)):
    print("ERR next_slice is %r, not a slice number — end-of-ladder or malformed. "
          "Re-point this arm at a live release; do NOT skip it." % (t,))
    raise SystemExit(0)
landed = sorted(n for n in (s.get("landed") or [])
                if not isinstance(n, bool) and isinstance(n, (int, float)))
prior = [n for n in landed if n < t]
if not prior:
    print("ERR no landed slice strictly below next_slice %s" % sid(t)); raise SystemExit(0)
b = prior[-1]
sha = next((e.get("sha") for e in (s.get("ladder") or []) if e.get("slice") == b), None)
if not sha:
    print("ERR predecessor Slice %s records no landing sha" % sid(b)); raise SystemExit(0)
print("OK %s %s %s %s" % (sid(t), sid(b), sha, "LANDED" if t in landed else "PENDING"))
' "$1" 2>&1
}

D9D_STATE_FILE="$REPO_ROOT/dev/plans/release-state-0.8.20.json"
set +e
D9D_DERIVED="$(derive_base_from_state "$D9D_STATE_FILE")"
D9D_DERIVE_RC=$?
set -e
read -r D9D_OK D9D_TARGET D9D_BASE D9D_SHA D9D_TARGET_STATE <<<"$D9D_DERIVED" || true
if [ "$D9D_DERIVE_RC" -ne 0 ] || [ "${D9D_OK:-}" != "OK" ]; then
  fail "arm 9d (state derivation): rc=$D9D_DERIVE_RC out=[$D9D_DERIVED]"
else
  # The banner MUST track the target's landed state in both directions.
  if [ "$D9D_TARGET_STATE" = "LANDED" ]; then D9D_WANT_BANNER=present
  else D9D_WANT_BANNER=absent; fi
  set +e
  D9D_OUT="$("$REPO_ROOT/scripts/commission-manifest.sh" 0.8.20 "$D9D_TARGET" 2>&1)"
  D9D_RC=$?
  set -e
  D9D_BASE_LINE="$(grep -m1 '^  base sha' <<<"$D9D_OUT" || true)"
  # Match the banner by its SEMANTIC content — the word plus the slice-specific
  # claim — not by its presentation. An earlier draft grepped the literal `⚠`
  # glyph, which would have gone RED on a cosmetic ASCII-ification of a banner
  # whose behaviour was intact: the same time-bombed-assertion class (TC-81)
  # this arm exists to stop. Codex §9 [low], accepted and fixed rather than
  # carried.
  # The `.` of a fractional target is a REGEX WILDCARD in an ERE, so a bare
  # interpolation would let `Slice 39x5 is ITSELF LANDED` satisfy an assertion
  # about Slice 39.5 — the arm carrying the same defect it grades.
  D9D_TARGET_RE="${D9D_TARGET//./\\.}"
  if grep -qiE "HISTORICAL.*Slice $D9D_TARGET_RE is ITSELF LANDED" <<<"$D9D_OUT"; then
    D9D_GOT_BANNER=present
  else D9D_GOT_BANNER=absent; fi
  if [ "$D9D_RC" -eq 0 ] \
     && grep -qF "$D9D_SHA" <<<"$D9D_BASE_LINE" \
     && grep -qF "(Slice $D9D_BASE " <<<"$D9D_BASE_LINE" \
     && [ "$D9D_GOT_BANNER" = "$D9D_WANT_BANNER" ]; then
    pass "real repo — Slice $D9D_TARGET's base is Slice $D9D_BASE's landing merge ($D9D_SHA) on the base line, and the HISTORICAL banner is $D9D_GOT_BANNER as the state file's landed set requires ($D9D_TARGET_STATE)"
  else
    fail "arm 9d (real base line, derived target=Slice $D9D_TARGET): rc=$D9D_RC want base=[Slice $D9D_BASE / $D9D_SHA] banner=$D9D_WANT_BANNER; got banner=$D9D_GOT_BANNER line=[$D9D_BASE_LINE]"
  fi
fi

# --- Arm 9e: the REAL manifest's pin path is emitted as a CITATION ----------
# The live half of arms 5h/5i. The pin line must render the path the way every
# other cited path renders (through `m.cite`, i.e. backticked and therefore
# existence-checked), and the path it names must resolve in this checkout. The
# assertion is value-agnostic — it reads whatever the state file pins to — so it
# keeps holding when the pin moves. Pre-fix the line was raw text: measured RED.
REAL_PIN_LINE="$(grep -m1 '^      pinned to' <<<"$REAL_OUT" || true)"
REAL_PIN="$(sed 's/^ *pinned to *//; s/`//g' <<<"$REAL_PIN_LINE")"
if grep -qE '^ +pinned to +`[^`]+`$' <<<"$REAL_PIN_LINE" \
   && [ -n "$REAL_PIN" ] && [ -e "$REPO_ROOT/$REAL_PIN" ]; then
  pass "real repo — the publish-gate pin is emitted as a citation and resolves ($REAL_PIN)"
else
  fail "arm 9e (real pin citation): line=[$REAL_PIN_LINE] pin=[$REAL_PIN]"
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

# --- Arm 9f: the REAL repo's curated slices generate, and cite what they name
# The live half of arms 11a-11c. DERIVED from the state file, never literal: it
# reads every ladder entry that carries `design_refs`, generates that slice's
# manifest, and asserts exit 0 plus a CURATED row for each named path. Adding,
# removing or re-pointing a curation moves the assertion with it, so this cannot
# become the time-bombed literal that TC-81 named (arm 9d's history).
#
# NON-VACUITY: if NO entry in the live release-state files carries `design_refs`
# the arm FAILS. The three reserved-gap slices (21/22/23) are exactly why the
# feature exists; a repo where the curation silently vanished must go red here,
# not quietly pass with an empty loop.
set +e
CURATED_PAIRS="$(cd "$REPO_ROOT" && python3 -c '
import glob, json, sys
rows = []
for p in sorted(glob.glob("dev/plans/release-state-*.json")):
    rel = p[len("dev/plans/release-state-"):-len(".json")]
    try:
        s = json.load(open(p))
    except Exception as e:
        print("ERR %s unreadable: %s" % (p, e)); raise SystemExit(0)
    for e in s.get("ladder") or []:
        for ref in e.get("design_refs") or []:
            rows.append("%s %s %s" % (rel, e.get("slice"), ref))
print("\n".join(rows))
' 2>&1)"
CURATED_RC=$?
set -e
if [ "$CURATED_RC" -ne 0 ] || grep -q '^ERR' <<<"$CURATED_PAIRS" || [ -z "$CURATED_PAIRS" ]; then
  fail "arm 9f (live design_refs): rc=$CURATED_RC no curated ladder entry found — out=[$CURATED_PAIRS]"
else
  C9F_BAD=""
  C9F_SEEN=0
  C9F_LAST=""
  C9F_OUT=""
  while read -r c9f_rel c9f_slice c9f_ref; do
    [ -n "$c9f_ref" ] || continue
    C9F_SEEN=$((C9F_SEEN + 1))
    if [ "$c9f_rel/$c9f_slice" != "$C9F_LAST" ]; then
      C9F_LAST="$c9f_rel/$c9f_slice"
      set +e
      C9F_OUT="$("$REPO_ROOT/scripts/commission-manifest.sh" "$c9f_rel" "$c9f_slice" 2>&1)"
      C9F_RC=$?
      set -e
      [ "$C9F_RC" -eq 0 ] || C9F_BAD="$C9F_BAD $c9f_rel/$c9f_slice:rc=$C9F_RC"
    fi
    grep -F "$c9f_ref" <<<"$C9F_OUT" | grep -q 'CURATED' \
      || C9F_BAD="$C9F_BAD $c9f_rel/$c9f_slice:$c9f_ref-not-curated"
    # A curated CONTRACT must also reach §4, where an orchestrator looks for the
    # document that WINS on conflict. The live instance is 0.8.20 Slice 22's
    # byte-pinned `OPP-12-C1-converged-contract.md`, which cannot be back-linked
    # and so can only ever get there by citation. Derived from the basename, so
    # it follows whatever the state file curates next.
    case "$(basename "$c9f_ref")" in
      *contract*|*CONTRACT*)
        sed -n '/^## 4\. CONTRACT PATHS/,/^## 5\./p' <<<"$C9F_OUT" \
          | grep -F "$c9f_ref" | grep -q 'CURATED' \
          || C9F_BAD="$C9F_BAD $c9f_rel/$c9f_slice:$c9f_ref-not-in-section-4"
        C9F_CONTRACTS=$((${C9F_CONTRACTS:-0} + 1))
        ;;
    esac
  done <<<"$CURATED_PAIRS"
  # NON-VACUITY, second half: at least one live curation must be a CONTRACT, or
  # the §4 half of this arm asserted nothing. 0.8.20 Slice 22's byte-pinned C-1
  # contract is that case and is the reason the §4 reach exists at all.
  if [ -z "$C9F_BAD" ] && [ "$C9F_SEEN" -gt 0 ] && [ "${C9F_CONTRACTS:-0}" -gt 0 ]; then
    pass "real repo — every ladder entry carrying \`design_refs\` generates and cites all $C9F_SEEN curated doc(s), ${C9F_CONTRACTS:-0} of them into §4 as contracts"
  else
    fail "arm 9f (live curated citations): seen=$C9F_SEEN contracts=${C9F_CONTRACTS:-0} bad:$C9F_BAD"
  fi
fi

# ===========================================================================
# Arms 13a-13h — SLICE-ID-HARDENING (0.8.20 cross-cutting unit, brief §1a
# sites 2 and 3, and §4 [DETERMINE] duty 2).
#
# WHY A FIXTURE AND NOT THE REAL STATE FILE. `release-state-0.8.20.json` carries
# NO fractional slice id anywhere — `landed` is all ints, `next_slice` is the int
# 40, every `ladder[].slice` is an int. Against that state :701's `isinstance`
# filter is a NO-OP and :497's `%d` truncation NEVER FIRES, because `slice_no` is
# always an int. An arm pointed at the real checkout is therefore green before
# the fix and green after it: vacuous by construction. Fractional ids belong in
# throwaway fixtures, and nowhere near the real ladder or board.
#
# ⚠ ONE EXCEPTION, and it is not hypothetical: site 3's neighbour bleed IS LIVE
# in this checkout. `dev/design/0.8.20-slice-39.5-collect-all-test-harness.md`
# and `dev/design/0.8.20-slice-39-publish-facing-documentation.md` both exist, and
# the shipped pattern `slice[-_ ]?0*39(?![0-9])` matches BOTH — so Slice 39's
# commission cites Slice 39.5's design memo as its own. Arm 13i asserts that on
# the real checkout.
# ===========================================================================

# Adds a fractional slice to the fixture ladder, lands it, and plants the design
# memos the filename selector has to tell apart.
frac_manifest_fixture() {
  mutate_state '
s["ladder"].append({"slice": 10.5, "short": "R-B5",
                    "title": "widget_readiness fractional leg",
                    "depends_on": [5], "status": "LANDED", "sha": "cccc3333"})
s["landed"] = [0, 5, 10.5]
'
  # Slice 30 has no token-bearing title, so without a memo of its own it would
  # hard-fail the TC-37 guard before ever reaching the base-SHA assertion.
  cat >"$FIX/dev/design/9.9.9-slice-30-design.md" <<'EOF'
---
status: ACTIVE
---

# Slice 30's memo

No token here at all.
EOF

  # The filename-selector matrix. EVERY ONE of these is deliberately TOKEN-FREE,
  # so the ONLY route by which any of them can reach a manifest is the
  # release+slice filename match at :497 — which is what makes each arm below a
  # measurement of that pattern and of nothing else.
  for frac_memo in \
      "9.9.9-slice-10.5-design.md:the fractional slice's OWN memo" \
      "9.9.9-slice-10x5-design.md:the WILDCARD impostor — an unescaped . matches the x" \
      "9.9.9-slice-10.5.1-design.md:a LONGER dotted id, a different unit again" \
      "9.9.9-slice-10.md:slice 10's memo with the id flush against the EXTENSION dot"
  do
    cat >"$FIX/dev/design/${frac_memo%%:*}" <<EOF
---
status: ACTIVE
---

# ${frac_memo#*:}

No token here at all.
EOF
  done
}

# --- Arm 13a: site 2 — a fractional LANDED slice is the predecessor --------
# `landed_nums = sorted({s for s in landed if isinstance(s, int)})` drops a float
# OUT of the predecessor set, so the base SHA silently skips a whole unit's work
# and the operator is told to branch from a commit that predates it. That is this
# repo's named agent-worktree-stale-base trap, printed with the manifest's
# authority behind it — produced by the tool built to prevent it.
setup_fixture
frac_manifest_fixture
run_gen 9.9.9 30
if [ "$RC" -eq 0 ] \
   && grep -qF 'cccc3333' <<<"$(base_line)" \
   && grep -qF '(Slice 10.5 ' <<<"$(base_line)" \
   && ! grep -qF 'bbbb2222' <<<"$(base_line)"; then
  pass "site 2 — Slice 30's base is the FRACTIONAL predecessor Slice 10.5 (cccc3333), not the integer Slice 5 it would fall back to"
else
  fail "arm 13a (site 2 base sha): rc=$RC base=[$(base_line)]"
fi

# --- Arm 13b: site 2 — a fractional slice is recognised as ITSELF LANDED ---
# The other consequence of the same filter: `slice_no in landed_nums` is False
# for a landed fractional slice, so the HISTORICAL banner never prints and a
# regeneration reads as a fresh commission.
run_gen 9.9.9 10.5
if [ "$RC" -eq 0 ] && grep -qiE 'HISTORICAL.*Slice 10\.5 is ITSELF LANDED' <<<"$OUT"; then
  pass "site 2 — regenerating a LANDED fractional slice raises the HISTORICAL banner"
else
  fail "arm 13b (site 2 historical banner): rc=$RC out=$(grep -c . <<<"$OUT") lines; banner=[$(grep -i HISTORICAL <<<"$OUT" || true)]"
fi

# --- Arm 13c: site 2 — the `landed so far` roll-up flags a fractional land --
# `isinstance(s, int) and s >= slice_no` at the roll-up is the same filter in its
# third costume: a fractional slice landed at or after the target carries no
# `⚠ at/after` mark, so the one line that would have warned the reader is silent.
run_gen 9.9.9 10
LANDED_LINE="$(grep -m1 '^  landed so far' <<<"$OUT" || true)"
if [ "$RC" -eq 0 ] && grep -qF '10.5 (cccc3333) ⚠ at/after Slice 10' <<<"$LANDED_LINE"; then
  pass "site 2 — the landed roll-up marks the fractional Slice 10.5 as at/after the target"
else
  fail "arm 13c (site 2 landed roll-up): rc=$RC line=[$LANDED_LINE]"
fi

# --- Arm 13d: site 3 — an integer slice must not claim its .5 neighbour ----
# `"%d" % 39.5` and `"%d" % 39` produce the BYTE-IDENTICAL pattern, so the
# filename selector cannot tell Slice N's memo from Slice N.5's. The manifest
# then cites another unit's design of record as this slice's required reading —
# and the TC-37 vacuous-pass guard, which fires only when NOTHING matches, is
# satisfied by the wrong document.
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] \
   && grep -qF '9.9.9-slice-10-design.md' <<<"$OUT" \
   && ! grep -qF '9.9.9-slice-10.5-design.md' <<<"$OUT" \
   && ! grep -qF '9.9.9-slice-10.5.1-design.md' <<<"$OUT"; then
  pass "site 3 — Slice 10's manifest cites its OWN memo and neither Slice 10.5's nor Slice 10.5.1's"
else
  fail "arm 13d (site 3 neighbour bleed): rc=$RC cited 10.5=$(grep -c '9.9.9-slice-10.5-design.md' <<<"$OUT") 10.5.1=$(grep -c '9.9.9-slice-10.5.1-design.md' <<<"$OUT")"
fi

# --- Arm 13e: [DETERMINE] duty 2 — the WILDCARD case ----------------------
# The arm the brief says will not exist unless it is deliberately written. The
# obvious `%d` -> `%s` swap yields `slice[-_ ]?0*10.5(?![0-9])`, in which `.`
# matches ANY character — so `slice-10x5-design.md` resolves as Slice 10.5's
# memo. Escaping is what closes it, and escaping alone is NOT sufficient: arm
# 13f is the case that survives escape-only.
run_gen 9.9.9 10.5
if [ "$RC" -eq 0 ] && ! grep -qF '9.9.9-slice-10x5-design.md' <<<"$OUT"; then
  pass "duty 2 — the slice id is a LITERAL, not a pattern: slice-10x5 is not Slice 10.5's memo"
else
  fail "arm 13e (duty 2 wildcard): rc=$RC cited the impostor=$(grep -c '9.9.9-slice-10x5-design.md' <<<"$OUT")"
fi

# --- Arm 13f: [DETERMINE] duty 2 — the case that survives ESCAPE-ONLY ------
# `re.escape("10.5")` stops the wildcard but NOT the dotted continuation:
# `10\.5(?![0-9])` still matches `slice-10.5.1-...` because the character after
# `10.5` is `.`, which is not a digit. Slice 10.5 would claim Slice 10.5.1's
# memo. Graded here so a future "simplification" back to escape-only goes red.
if [ "$RC" -eq 0 ] \
   && grep -qF '9.9.9-slice-10.5-design.md' <<<"$OUT" \
   && ! grep -qF '9.9.9-slice-10.5.1-design.md' <<<"$OUT" \
   && ! grep -qF '9.9.9-slice-10-design.md' <<<"$OUT"; then
  pass "duty 2 — Slice 10.5 cites its own memo, and neither the longer dotted id 10.5.1 nor its integer neighbour 10"
else
  fail "arm 13f (duty 2 dotted continuation): rc=$RC 10.5.1=$(grep -c '9.9.9-slice-10.5.1-design.md' <<<"$OUT") 10=$(grep -c '9.9.9-slice-10-design.md' <<<"$OUT")"
fi

# --- Arm 13g: the CONTROL that rules out the OBVIOUS boundary fix ----------
# GREEN before the fix and GREEN after it, BY DESIGN — and RED against the
# natural first attempt, which is to tighten the trailing guard to `(?![0-9.])`.
# That candidate rejects `9.9.9-slice-10.md`, where the character after the id is
# the EXTENSION dot, and the slice loses its own memo: a NEW false negative in a
# selector whose failure mode is the TC-37 hard stop. Leg 1 measured the same
# trap on preflight.sh's ERE. What must be rejected is a following DIGIT, or a
# following `.` that begins a longer id — never a `.` on its own.
#
# Its non-vacuity is demonstrated against that candidate, not against the
# pre-fix tool; see the determination matrix quoted in the closure.
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && grep -qF '9.9.9-slice-10.md' <<<"$OUT"; then
  pass "duty 2 control — an id flush against the EXTENSION dot (slice-10.md) is still Slice 10's memo"
else
  fail "arm 13g (duty 2 extension-dot control): rc=$RC cited=$(grep -c '9.9.9-slice-10.md' <<<"$OUT")"
fi

# --- Arm 13h: the 9d ORACLE itself, against a FRACTIONAL state file --------
# Brief §1c. Arm 9d's derivation replicated the generator's own int-only
# assumptions in TWO places — `isinstance(t, int)` on `next_slice`, which routed
# a fractional target to ERR and made this suite rc=1 outright, and an
# `isinstance(n, int)` filter on `landed` semantically identical to :701. An
# oracle that copies the implementation cannot falsify it. Arm 9d runs against
# the live all-integer state and therefore cannot show the difference; this arm
# runs the SAME function against a fractional copy, which is what makes the
# repair measurable.
FRAC_STATE="$TMPROOT/frac-release-state.json"
python3 - "$REPO_ROOT/dev/plans/release-state-0.8.20.json" "$FRAC_STATE" <<'PY'
import json, sys
s = json.load(open(sys.argv[1]))
# A landed fractional unit BETWEEN two landed integers, so the correct
# predecessor of the fractional target is itself fractional: an oracle that
# filters floats out of `landed` answers 33, not 33.5.
s["ladder"].append({"slice": 33.5, "short": "R-20-FRAC", "title": "a landed fractional unit",
                    "depends_on": [33], "status": "LANDED", "sha": "ffff5555"})
s["ladder"].append({"slice": 39.5, "short": "R-20-HARNESS", "title": "the fractional target",
                    "depends_on": [39], "status": "UNBLOCKED", "sha": None})
s["landed"] = sorted(n for n in s["landed"] if n <= 33) + [33.5]
s["next_slice"] = 39.5
json.dump(s, open(sys.argv[2], "w"), indent=2)
PY
set +e
D13H="$(derive_base_from_state "$FRAC_STATE")"
D13H_RC=$?
set -e
if [ "$D13H_RC" -eq 0 ] && [ "$D13H" = "OK 39.5 33.5 ffff5555 PENDING" ]; then
  pass "arm 9d's oracle accepts a FRACTIONAL next_slice and a FRACTIONAL landed predecessor, and names both without truncating"
else
  fail "arm 13h (9d oracle on fractional state): rc=$D13H_RC want=[OK 39.5 33.5 ffff5555 PENDING] got=[$D13H]"
fi

# --- Arm 13i: site 3 is LIVE IN THIS CHECKOUT, not merely prospective ------
# Both memos are tracked files at HEAD:
#   dev/design/0.8.20-slice-39-publish-facing-documentation.md
#   dev/design/0.8.20-slice-39.5-collect-all-test-harness.md
# and the shipped pattern matches both for slice_no=39. So the 0.8.20 Slice 39
# commission cites another unit's design of record as its own required reading.
# Asserted on the REAL entry point — the generator itself — because five of six
# codex fix rounds across Slices 32/33 were defects in the verification
# apparatus rather than in the function under test.
#
# NON-VACUITY: if either memo stops existing the arm FAILS rather than passing
# on an empty premise.
REAL_39="$REPO_ROOT/dev/design/0.8.20-slice-39-publish-facing-documentation.md"
REAL_395="$REPO_ROOT/dev/design/0.8.20-slice-39.5-collect-all-test-harness.md"
if [ ! -f "$REAL_39" ] || [ ! -f "$REAL_395" ]; then
  fail "arm 13i: the live (39, 39.5) memo pair is gone — re-point this arm at the current pair, do NOT delete it"
else
  set +e
  A13I_OUT="$("$REPO_ROOT/scripts/commission-manifest.sh" 0.8.20 39 2>&1)"
  A13I_RC=$?
  set -e
  if [ "$A13I_RC" -eq 0 ] \
     && grep -qF '0.8.20-slice-39-publish-facing-documentation.md' <<<"$A13I_OUT" \
     && ! grep -qF '0.8.20-slice-39.5-collect-all-test-harness.md' <<<"$A13I_OUT"; then
    pass "real repo — the 0.8.20 Slice 39 manifest cites Slice 39's design memo and NOT Slice 39.5's"
  else
    fail "arm 13i (live neighbour bleed): rc=$A13I_RC cited 39.5's memo=$(grep -c '0.8.20-slice-39.5-collect-all-test-harness.md' <<<"$A13I_OUT")"
  fi
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

# --- RULED-WITH-WORK arms ---------------------------------------------------
# A decision can be CLOSED and still owe an ACTION. Ruling one moves it out of
# `decisions.unruled`, so it stops rendering as a named HALT/GATED row and
# collapses into the bare ruled COUNT -- i.e. ruling a decision that carries
# residual work makes that work LESS visible. `residual_work` on a `ruled` entry
# restores a named row. Added 2026-07-31 after review round 3 found the renderer
# had shipped with ZERO coverage: this suite was green before and after the
# behaviour change, so it vouched for nothing (the brief's own §7.14 -- a check
# must be proven non-vacuous by a control that FAILS).

# Arm RWW-a: the ABSENT case is the control. Without it, arm RWW-b could pass
# for an unrelated reason (e.g. the id appearing in some other section).
setup_fixture
mutate_state "s.setdefault('decisions', {}).setdefault('ruled', []).append(
    {'id': 'rww-probe', 'title': 'RWW PROBE TITLE', 'ruling': 'r', 'source': 'x'})"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && ! grep -q 'RULED-WITH-WORK' <<<"$OUT" && ! grep -q 'RWW PROBE TITLE' <<<"$OUT"; then
  pass "ruled decision WITHOUT residual_work is not named (the control: absent => silent)"
else
  fail "RWW-a: a ruled entry with no residual_work must not render; rc=$RC"
fi

# Arm RWW-b: the same entry, now carrying residual_work, IS named in full.
setup_fixture
mutate_state "s.setdefault('decisions', {}).setdefault('ruled', []).append(
    {'id': 'rww-probe', 'title': 'RWW PROBE TITLE', 'ruling': 'r', 'source': 'x',
     'residual_work': 'RWW PROBE OWED STEP'})"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && grep -q 'RULED-WITH-WORK' <<<"$OUT" \
   && grep -q 'RWW PROBE TITLE' <<<"$OUT" && grep -q 'RWW PROBE OWED STEP' <<<"$OUT"; then
  pass "ruled decision WITH residual_work is named in full (title + owed work)"
else
  fail "RWW-b: residual_work must render title and owed work; rc=$RC out=$OUT"
fi

# Arm RWW-c: empty / whitespace-only must behave as ABSENT, not as an empty row.
for probe in '' '   '; do
  setup_fixture
  mutate_state "s.setdefault('decisions', {}).setdefault('ruled', []).append(
      {'id': 'rww-probe', 'title': 'RWW PROBE TITLE', 'ruling': 'r', 'source': 'x',
       'residual_work': '$probe'})"
  run_gen 9.9.9 10
  if [ "$RC" -eq 0 ] && ! grep -q 'RULED-WITH-WORK' <<<"$OUT"; then
    pass "residual_work=$(printf '%q' "$probe") is treated as ABSENT (no empty row)"
  else
    fail "RWW-c: blank residual_work must not render a row; probe=$(printf '%q' "$probe") rc=$RC"
  fi
done

# Arm RWW-d: a WRONG-TYPE value must not crash the generator. `commission-manifest`
# is an ALWAYS-ON CI job (asserted below), so an AttributeError here reds CI for
# every release. A list is the natural shape a future writer would reach for when
# the obligation is a numbered sequence -- which is exactly how it is written today.
setup_fixture
mutate_state "s.setdefault('decisions', {}).setdefault('ruled', []).append(
    {'id': 'rww-probe', 'title': 'RWW PROBE TITLE', 'ruling': 'r', 'source': 'x',
     'residual_work': ['step one', 'step two']})"
run_gen 9.9.9 10
# ⚠ Assert the CONTENT survives, not merely that nothing raised. Proven necessary by a mutant:
# a `_rww_text` returning '' for every sequence passes an rc-and-no-traceback check green while
# SILENTLY DROPPING the obligation -- reproducing the very ruled-work-made-invisible defect this
# renderer exists to prevent. "It did not crash" is not "it worked".
if [ "$RC" -eq 0 ] && ! grep -qi 'traceback\|AttributeError' <<<"$OUT" \
   && grep -q 'step one step two' <<<"$OUT"; then
  pass "non-string residual_work renders its CONTENT and does not crash (always-on CI job)"
else
  fail "RWW-d: list residual_work must render 'step one step two' without raising; rc=$RC out=$OUT"
fi

# Arm RWW-e: a list of NULLs must behave as ABSENT, not render "owed: None" --
# which reads as "nothing owed", the exact inversion this renderer exists to
# prevent. Sibling of RWW-c; added because the [None] fix shipped without one.
setup_fixture
mutate_state "s.setdefault('decisions', {}).setdefault('ruled', []).append(
    {'id': 'rww-probe', 'title': 'RWW PROBE TITLE', 'ruling': 'r', 'source': 'x',
     'residual_work': [None, None]})"
run_gen 9.9.9 10
if [ "$RC" -eq 0 ] && ! grep -q 'RULED-WITH-WORK' <<<"$OUT" && ! grep -q 'owed: None' <<<"$OUT"; then
  pass "residual_work=[None,None] is treated as ABSENT (never renders 'owed: None')"
else
  fail "RWW-e: an all-null residual_work must not render a row; rc=$RC out=$OUT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll commission-manifest tests passed\n'
