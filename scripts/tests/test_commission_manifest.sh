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
PRE_GEN="$TMPROOT/commission-manifest-pre.sh"
PRE_SHA=""
while read -r c; do
  [ -n "$c" ] || continue
  if ! (cd "$REPO_ROOT" && git show "$c:scripts/commission-manifest.sh" 2>/dev/null) \
       | grep -q 'design_refs'; then PRE_SHA="$c"; break; fi
done < <(cd "$REPO_ROOT" && git log --format=%H -- scripts/commission-manifest.sh)
if [ -z "$PRE_SHA" ]; then
  fail "arm 11d (pre-change generator): no revision of scripts/commission-manifest.sh predates design_refs — cannot prove additivity; do NOT skip this"
else
  (cd "$REPO_ROOT" && git show "$PRE_SHA:scripts/commission-manifest.sh") >"$PRE_GEN"
  chmod +x "$PRE_GEN"
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
    [ "$PRE_OUT" = "$POST_OUT" ] \
      || D11D_DRIFT="$D11D_DRIFT slice-$d11d_slice:$(diff <(printf '%s\n' "$PRE_OUT") <(printf '%s\n' "$POST_OUT") | head -20 | tr '\n' '~')"
  done
  if [ -z "$D11D_DRIFT" ]; then
    pass "design_refs is ADDITIVE — with the key absent the manifest is byte-identical to the pre-change generator ($PRE_SHA)"
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
D9D_STATE_FILE="$REPO_ROOT/dev/plans/release-state-0.8.20.json"
set +e
D9D_DERIVED="$(python3 -c '
import json, sys
try:
    s = json.load(open(sys.argv[1]))
except Exception as e:
    print("ERR unreadable state file: %s" % e); raise SystemExit(0)
t = s.get("next_slice")
if isinstance(t, bool) or not isinstance(t, int):
    print("ERR next_slice is %r, not an int — end-of-ladder or malformed. "
          "Re-point this arm at a live release; do NOT skip it." % (t,))
    raise SystemExit(0)
landed = sorted(n for n in (s.get("landed") or []) if isinstance(n, int))
prior = [n for n in landed if n < t]
if not prior:
    print("ERR no landed slice strictly below next_slice %d" % t); raise SystemExit(0)
b = prior[-1]
sha = next((e.get("sha") for e in (s.get("ladder") or []) if e.get("slice") == b), None)
if not sha:
    print("ERR predecessor Slice %d records no landing sha" % b); raise SystemExit(0)
print("OK %d %d %s %s" % (t, b, sha, "LANDED" if t in landed else "PENDING"))
' "$D9D_STATE_FILE" 2>&1)"
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
  if grep -qiE "HISTORICAL.*Slice $D9D_TARGET is ITSELF LANDED" <<<"$D9D_OUT"; then
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
  done <<<"$CURATED_PAIRS"
  if [ -z "$C9F_BAD" ] && [ "$C9F_SEEN" -gt 0 ]; then
    pass "real repo — every ladder entry carrying \`design_refs\` generates and cites all $C9F_SEEN curated doc(s)"
  else
    fail "arm 9f (live curated citations): seen=$C9F_SEEN bad:$C9F_BAD"
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

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll commission-manifest tests passed\n'
