#!/usr/bin/env bash
# scripts/tests/test_check_c1_conformance.sh — coverage for the RUBRIC-H7
# `can-i-deploy` contract-conformance gate (scripts/check-c1-conformance.sh) AND
# for its two wirings: `preflight.sh --landing` (PREVENT) and the always-on CI
# job (DETECT).
#
# WHAT IS BEING PROTECTED: R-20-H7 — as-built FathomDB code still satisfies the
# ratified cross-repo design contract
# dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md at the
# 0.8.20 co-land. The gate is mechanical on purpose ("not humans re-reading
# prose", plan-0.8.20.md §3), and an absent-or-failing gate HOLDS the breaking
# pair. So the gate itself needs a recurrence guard: these arms.
#
# RED-first: the real tree PASSES today, so asserting only against the real repo
# would prove nothing — a `true` script would pass it. Every failure arm below
# therefore runs against a purpose-built DIVERGENT FIXTURE (a mutated COPY of
# the contract, of the pin, or of the source root), so each arm can only go green
# because the predicate actually fired. The real-repo arm is the regression half
# of the same pair.
#
# THE FIXTURES ARE COPIES. Neither the real contract nor the real src/ tree is
# ever written by this suite — mutating them is the exact thing the gate exists
# to catch. Copies live under mktemp -d (the checker takes --contract/--pin/--root
# for exactly this reason); the preflight arms build throwaway git repos + linked
# worktrees.
#
# THE FIXTURE ROOTS ARE BUILT FROM `--list-sources`, not from a hand-maintained
# path list. If a future clause reads a new file, the fixture roots pick it up
# automatically and cannot silently go stale (a stale fixture root would turn
# every source arm into a TC-37 #4 evaporation and stop testing what it claims).
#
# NOTE ON THE WHITESPACE ARM: failing on a formatting-only change is a
# DELIBERATE, DOCUMENTED property of a content-hash pin, not an accident — see
# the gate's header. It is asserted here so the behaviour is a contract.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-c1-conformance.sh"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"
REAL_CONTRACT="$REPO_ROOT/dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md"
REAL_PIN="$REPO_ROOT/scripts/c1-conformance-pin.json"
# shellcheck source=lib/governed-surface-fixture.sh
. "$SCRIPT_DIR/lib/governed-surface-fixture.sh"
# shellcheck source=lib/c1-conformance-fixture.sh
. "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"

# The no-argument arm exercises the checker's REPO-RELATIVE defaults, which it
# resolves from `git rev-parse --show-toplevel` — i.e. from the cwd. Pin the cwd
# to this checkout so that arm tests THIS tree no matter where the suite is
# invoked from (mirrors the governed-surface-pin suite).
cd "$REPO_ROOT"

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

run_checker() {
  set +e
  OUT="$(bash "$CHECKER" "$@" 2>&1)"
  RC=$?
  set -e
}

expect_rc() {
  local want="$1" desc="$2"
  if [ "$RC" -eq "$want" ]; then
    pass "$desc"
  else
    fail "$desc — expected rc=$want, got rc=$RC; out: $OUT"
  fi
}

# expect_out <regex> <desc>
expect_out() {
  if printf '%s' "$OUT" | grep -qE "$1"; then
    pass "$2"
  else
    fail "$2 — expected output matching /$1/; got: $OUT"
  fi
}

# expect_no_out <regex> <desc>
expect_no_out() {
  if printf '%s' "$OUT" | grep -qE "$1"; then
    fail "$2 — output must NOT match /$1/; got: $OUT"
  else
    pass "$2"
  fi
}

# Every real DIVERGENCE (exit 1) must route the reader to the Steward/HITL and
# must forbid a silent re-pin: a failure the reader cannot act on is how a
# conformance gate gets quietly neutered.
expect_routes_to_steward() {
  local desc="$1" ok=1
  printf '%s' "$OUT" | grep -q 'DO NOT re-pin' || ok=0
  printf '%s' "$OUT" | grep -q 'Steward' || ok=0
  if [ "$ok" -eq 1 ]; then
    pass "$desc routes the reader to the Steward and forbids a silent re-pin"
  else
    fail "$desc did not print the full Steward-routing block; got: $OUT"
  fi
}

# ============================================================================
# Fixture-root construction, driven by the gate's own --list-sources output.
# ============================================================================
if ! SOURCE_MANIFEST="$(bash "$CHECKER" --list-sources 2>&1)"; then
  fail "--list-sources must succeed so the fixture roots can be built from it; got: $SOURCE_MANIFEST"
  SOURCE_MANIFEST=""
fi

# make_root <name> -> prints the path of a fresh fixture root carrying a COPY of
# every file/tree the clause assertions read.
make_root() {
  local d="$TMPROOT/root-$1" kind path
  mkdir -p "$d"
  while IFS=$'\t' read -r kind path; do
    [ -n "${kind:-}" ] || continue
    case "$kind" in
      file)
        mkdir -p "$d/$(dirname "$path")"
        cp "$REPO_ROOT/$path" "$d/$path"
        ;;
      tree)
        mkdir -p "$d/$path"
        ;;
    esac
  done <<<"$SOURCE_MANIFEST"
  printf '%s' "$d"
}

# copy_contract <name> -> a COPY of the real contract
copy_contract() {
  local d="$TMPROOT/contract-$1"
  mkdir -p "$d"
  cp "$REAL_CONTRACT" "$d/contract.md"
  printf '%s' "$d/contract.md"
}

# edit_pin <name> <python-body> -> a COPY of the pin, mutated. `pin` is the dict.
edit_pin() {
  local name="$1" body="$2" out="$TMPROOT/pin-$1.json"
  mkdir -p "$TMPROOT"
  python3 - "$REAL_PIN" "$out" <<PY
import json, sys
with open(sys.argv[1]) as fh:
    pin = json.load(fh)
$body
with open(sys.argv[2], "w") as fh:
    json.dump(pin, fh, indent=2)
    fh.write("\n")
PY
  printf '%s' "$out"
}

# ================== Arm 1: the real, unmodified tree, defaults ===============
# Regression half. Also a standing assertion that the pinned contract is itself
# byte-unmodified: if this suite ever "fixes" a red arm by editing the contract,
# this arm goes red.
run_checker
expect_rc 0 "the real repo conforms to the pinned C-1 contract (default args)"
expect_out 'ok +c1-contract-conformance' "the passing run says ok"
expect_out '26 checkable / 12 cross-repo / 7 prose' \
  "the passing run states the clause tally it verified"
expect_out '45 total' "the passing run states the grand total of clauses"

PIN_SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "$REAL_PIN")"
REAL_SHA="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$REAL_CONTRACT")"
if [ "$PIN_SHA" = "$REAL_SHA" ]; then
  pass "the ratified C-1 contract is byte-identical to the pin"
else
  fail "the contract no longer matches the pin's sha256 ($REAL_SHA vs $PIN_SHA)"
fi

# The pin must really be its recorded commit's content — the provenance claim.
# Skipped (loudly) where that commit is unreachable; the hash arm above still
# carries the assertion, so this is not a vacuous pass.
PIN_BLOB="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["git_blob_sha1"])' "$REAL_PIN")"
PIN_AT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pinned_at_commit"])' "$REAL_PIN")"
PIN_PATH="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pinned_file"])' "$REAL_PIN")"
if git -C "$REPO_ROOT" cat-file -e "${PIN_AT}^{commit}" 2>/dev/null; then
  AT_PIN="$(git -C "$REPO_ROOT" rev-parse "${PIN_AT}:${PIN_PATH}")"
  if [ "$AT_PIN" = "$PIN_BLOB" ]; then
    pass "the pin's git_blob_sha1 is exactly ${PIN_AT:0:8}'s blob for the contract"
  else
    fail "pin git_blob_sha1 $PIN_BLOB != ${PIN_AT:0:8}'s blob $AT_PIN — the provenance claim is false"
  fi
else
  printf 'SKIP  %s unreachable (shallow checkout) — provenance arm not run\n' "${PIN_AT:0:8}"
fi

# ============ Arm 2: an unmodified COPY of contract + pin + root =============
# Byte-identical copies at different paths must pass, proving the gate compares
# CONTENT and is not keyed to any path or to repo state.
CLEAN_CONTRACT="$(copy_contract clean)"
CLEAN_ROOT="$(make_root clean)"
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$CLEAN_ROOT"
expect_rc 0 "an unmodified COPY of the contract + a copied source root passes"
expect_out 'ok +c1-contract-conformance' "the copied-fixture pass says ok"

# ============ Arm 3 (RED): the contract text moved (substantive) =============
# The load-bearing pin: the clause registry was derived from THIS text, so a
# changed contract means the registry is no longer known to describe the doc.
F="$(copy_contract mutated)"
python3 - "$F" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert "registry-admitted GOVERNED entities only" in text
text = text.replace("registry-admitted GOVERNED entities only", "anonymous entities", 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$F" --pin "$REAL_PIN" --root "$CLEAN_ROOT"
expect_rc 1 "a SUBSTANTIVE contract edit HARD-fails (the contract moved)"
expect_out 'has MOVED' "contract-moved says the contract moved"
expect_out 'content differs from the pin' "contract-moved reports a content divergence"
expect_out 'RE-DERIVED' "contract-moved says the clause registry must be RE-DERIVED"
expect_out 'efa8d584' "contract-moved cites the efa8d584 amendment precedent"
expect_no_out 'formatting-only' "a substantive edit is NOT reported as formatting-only"
expect_routes_to_steward "contract-moved"

# ============ Arm 4 (RED): whitespace / formatting-only change ===============
# Documented behaviour of a CONTENT-hash pin. The failure must ALSO say the
# change is formatting-only, so the reader is never left guessing whether the
# ratified text actually moved.
F="$(copy_contract whitespace)"
printf '\n\n' >>"$F"
run_checker --contract "$F" --pin "$REAL_PIN" --root "$CLEAN_ROOT"
expect_rc 1 "a whitespace/formatting-only contract change HARD-fails (content-hash pin, by design)"
expect_out 'content differs from the pin' "whitespace-only reports a content divergence"
expect_out 'WHITESPACE/FORMATTING-ONLY' "whitespace-only says the change is formatting-only"
expect_out 'CONTENT hash' "whitespace-only explains that the pin is deliberately a content hash"
expect_routes_to_steward "whitespace-only"

# ====== Arm 5 (RED): contract missing — TC-37 evaporation path #2 ============
# A gate that cannot see its subject must never report green, and must not
# report a DIVERGENCE either: it computed no verdict at all.
run_checker --contract "$TMPROOT/no-such-contract.md" --pin "$REAL_PIN" --root "$CLEAN_ROOT"
expect_rc 2 "a MISSING contract exits 2 (TC-37 evaporation), never 0 and never 1"
expect_out 'cannot read' "contract-missing says it could not read the contract"
expect_out 'TC-37' "contract-missing cites the vacuous-pass failure class"
expect_no_out 'ok +c1-contract-conformance' "contract-missing prints no ok line"

# ============ Arm 6 (RED): pin missing — evaporation path #3 =================
run_checker --contract "$CLEAN_CONTRACT" --pin "$TMPROOT/no-such-pin.json" --root "$CLEAN_ROOT"
expect_rc 2 "a MISSING pin exits 2 (the gate could not run) and never 0"
expect_out 'the gate cannot run' "pin-missing says the gate could not run"

# ====== Arm 7 (RED): pin malformed — a counts entry DELETED ==================
# The counts block is the backstop that catches an internally inconsistent
# re-pin, which makes the counts block itself a target. A count the gate cannot
# read is a MALFORMED PIN (exit 2), never a silently skipped check.
for KEY in checkable cross_repo prose total; do
  P="$(edit_pin "omit-$KEY" "del pin['counts']['$KEY']")"
  run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
  expect_rc 2 "a pin that OMITS counts.$KEY HARD-fails as MALFORMED"
  expect_out "'counts' has no '$KEY' entry" "omit-counts.$KEY names the missing count entry"
  expect_out 'MALFORMED' "omit-counts.$KEY says the pin is malformed"
  expect_out "DO NOT 'fix' this by regenerating the pin" \
    "omit-counts.$KEY forbids regenerating the pin to clear it"
  expect_no_out 'ok +c1-contract-conformance' "omit-counts.$KEY prints no ok line"
done

# ====== Arm 8 (RED): pin malformed — a counts entry MISTYPED =================
# Same hole through a different door. `true` is included because
# isinstance(True, int) is True in Python and `True == 1` would let a boolean
# masquerade as a count; `26.0 == 26` would likewise compare equal.
P="$(edit_pin "type-string" "pin['counts']['checkable'] = '26'")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a STRING counts.checkable HARD-fails as MALFORMED (never a divergence)"
expect_out 'not an integer' "string-count says the count is not an integer"
expect_no_out 'DO NOT re-pin' \
  "a malformed pin does NOT print the divergence-routing block (broken gate, not a moved contract)"

P="$(edit_pin "type-null" "pin['counts']['prose'] = None")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a NULL counts.prose HARD-fails as MALFORMED"
expect_out 'not an integer' "null-count says the count is not an integer"

P="$(edit_pin "type-float" "pin['counts']['checkable'] = 26.0")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a FLOAT counts.checkable HARD-fails as MALFORMED (26.0 == 26 would have passed)"
expect_out 'not an integer' "float-count says the count is not an integer"

P="$(edit_pin "type-bool" "pin['counts']['total'] = True")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a BOOLEAN counts.total HARD-fails (isinstance(True, int) is True in Python)"
expect_out 'not an integer' "bool-count says the count is not an integer"

P="$(edit_pin "hash-null" "pin['sha256'] = None")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a NULL sha256 in the pin HARD-fails as MALFORMED, not as a divergence"
expect_out 'not a non-empty string' "bad-hash pin says the hash field is not a string"

# === Arm 9 (RED): a CHECKABLE clause DELETED from the pin registry ===========
# The check set SHRINKS. The gate implements an assertion the pin no longer
# registers — that is a malformed pin (exit 2), and the vanished id must be
# NAMED so the reader can see exactly which check was removed.
P="$(edit_pin "clause-deleted" \
  "pin['clauses'] = [c for c in pin['clauses'] if c['id'] != 'C1-Q6B-NO-ENTITYTYPESPEC-NO-IDPREFIX']
pin['counts']['checkable'] -= 1
pin['counts']['total'] -= 1")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a CHECKABLE clause DELETED from the pin registry HARD-fails as MALFORMED"
expect_out 'C1-Q6B-NO-ENTITYTYPESPEC-NO-IDPREFIX' "clause-deleted NAMES the vanished clause id"
expect_out 'VANISHED|SHRUNK' "clause-deleted says the pinned check set shrank"
expect_no_out 'ok +c1-contract-conformance' "clause-deleted prints no ok line"

# === Arm 10 (RED): an ORPHAN pin id with no implemented assertion ============
# The mirror direction: the pin claims a check this gate does not implement, so
# the pin over-states what is verified. Also exit 2, also NAMED.
P="$(edit_pin "clause-orphan" \
  "pin['clauses'].append({'id': 'C1-INVENTED-CLAUSE', 'category': 'CHECKABLE',
    'obligation': 'a clause nobody implemented', 'evidence': []})
pin['counts']['checkable'] += 1
pin['counts']['total'] += 1")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "an ORPHAN CHECKABLE pin id (no implemented assertion) HARD-fails as MALFORMED"
expect_out 'C1-INVENTED-CLAUSE' "clause-orphan NAMES the orphan clause id"
expect_out 'implements NO assertion' "clause-orphan says the gate implements no assertion for it"

# === Arm 11 (RED): a clause RECLASSIFIED CHECKABLE -> PROSE ==================
# The quietest way to buy a green: leave the id in the registry but demote it out
# of the checked set. Caught twice over — the category counts move, and the
# implemented assertion is no longer registered.
P="$(edit_pin "clause-reclassified" \
  "for c in pin['clauses']:
    if c['id'] == 'C1-Q4-NO-PROVISIONAL-CONCEPT':
        c['category'] = 'PROSE'")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a clause RECLASSIFIED CHECKABLE -> PROSE HARD-fails as MALFORMED"
expect_out 'C1-Q4-NO-PROVISIONAL-CONCEPT' "clause-reclassified NAMES the demoted clause id"
expect_out 'internally inconsistent' "clause-reclassified reports the moved category counts"

# The same demotion with the counts "fixed up" to match is still caught, by the
# registry bijection alone — this is the arm that proves the counts check is not
# the only thing standing between a demotion and a green.
P="$(edit_pin "clause-reclassified-consistent" \
  "for c in pin['clauses']:
    if c['id'] == 'C1-Q4-NO-PROVISIONAL-CONCEPT':
        c['category'] = 'PROSE'
pin['counts']['checkable'] -= 1
pin['counts']['prose'] += 1")"
run_checker --contract "$CLEAN_CONTRACT" --pin "$P" --root "$CLEAN_ROOT"
expect_rc 2 "a demotion WITH the counts fixed up is still caught by the registry bijection"
expect_out 'C1-Q4-NO-PROVISIONAL-CONCEPT' "the consistent demotion still NAMES the clause id"

# === Arm 12 (RED): THE LOAD-BEARING SOURCE ARMS =============================
# Divergent SOURCE fixtures in which a CHECKABLE clause's obligation is actually
# violated. Without these the whole gate could be satisfied by a `true` script:
# the real tree passes, so only a fixture that BREAKS the code can prove the
# clause assertions are wired to anything at all.
#
# 12a — the Q6(b) NEGATIVE-SPACE clause. The amendment's own factual assertion is
# that no EntityTypeSpec / id_prefix symbol exists anywhere under src/. A fixture
# that INTRODUCES one must FAIL the gate.
BAD_ROOT="$(make_root entitytypespec)"
mkdir -p "$BAD_ROOT/src/rust/crates/fathomdb-engine/src"
cat >"$BAD_ROOT/src/rust/crates/fathomdb-engine/src/entity_type_spec.rs" <<'RS'
// Fixture only. The un-amended C-1 clause (b) mandated exactly this symbol; the
// ratified TC-11 pin A forbids it. Its presence must turn the gate RED.
pub struct EntityTypeSpec {
    pub id_prefix: String,
}
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$BAD_ROOT"
expect_rc 1 "a source root that INTRODUCES EntityTypeSpec/id_prefix HARD-fails the gate"
expect_out 'C1-Q6B-NO-ENTITYTYPESPEC-NO-IDPREFIX' "the negative-space failure NAMES the clause id"
expect_out 'entity_type_spec.rs' "the negative-space failure NAMES the offending file"
expect_out 'FAILS' "the negative-space failure says the clause fails"
expect_routes_to_steward "the negative-space clause failure"
expect_no_out 'has MOVED' "a clause failure is not reported as a contract move"

# 12b — a POSITIVE-PRESENCE clause. Remove a required symbol from the copied
# source and the gate must fail, naming the clause.
BAD_ROOT2="$(make_root symbol-removed)"
python3 - "$BAD_ROOT2/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert "pub enum DenseReadiness {" in text
text = text.replace("pub enum DenseReadiness {", "pub enum VectorFreshness {", 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$BAD_ROOT2"
expect_rc 1 "a source root with a REQUIRED symbol removed HARD-fails the gate"
expect_out 'C1-Q4-DENSE-READINESS-TWO-MEMBERS' "the missing-symbol failure NAMES the clause id"
expect_out 'found 0 match' "the missing-symbol failure states what it looked for and did not find"
expect_routes_to_steward "the missing-symbol clause failure"

# 12c — a NAMED-TEST clause. The gate asserts that the test which proves a
# behavioural obligation still exists in the tree; deleting it must go red.
BAD_ROOT3="$(make_root test-deleted)"
python3 - "$BAD_ROOT3/src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert "fn boot_rederive_converges_after_simulated_crash()" in text
text = text.replace("fn boot_rederive_converges_after_simulated_crash()",
                    "fn boot_rederive_removed_by_fixture()", 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$BAD_ROOT3"
expect_rc 1 "deleting the named crash-heal test HARD-fails the gate"
expect_out 'C1-AA-CRASH-HEAL-BOOT-REDERIVE' "the deleted-test failure NAMES the clause id"

# === Arm 13 (RED): a source file an assertion reads is MISSING ===============
# TC-37 evaporation path #4: the assertion could not be EVALUATED. That is
# neither a pass (0) nor a clause failure (1) — the gate computed no verdict.
GONE_ROOT="$(make_root file-missing)"
rm -f "$GONE_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs"
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$GONE_ROOT"
expect_rc 2 "a MISSING source file exits 2 (TC-37 #4) — not 0 and not 1"
expect_out 'could not be EVALUATED' "file-missing says the assertion could not be evaluated"
expect_out 'TC-37' "file-missing cites the evaporation failure class"
expect_no_out 'ok +c1-contract-conformance' "file-missing prints no ok line"

# An entirely empty root is the same class, not a vacuous pass.
mkdir -p "$TMPROOT/root-empty"
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$TMPROOT/root-empty"
expect_rc 2 "an EMPTY source root exits 2 (every assertion evaporates), never 0"

# ==================== Arm 14: usage / environment errors ====================
run_checker --not-a-flag
expect_rc 2 "an unknown flag exits 2 (usage), distinct from a divergence"

run_checker --help
expect_rc 0 "--help exits 0"
expect_out 'Usage: scripts/check-c1-conformance.sh' "--help prints usage"

run_checker --list-sources
expect_rc 0 "--list-sources exits 0"
expect_out 'file\s+src/rust/crates/fathomdb-engine/src/lib.rs' \
  "--list-sources names the engine source the assertions read"
expect_out 'tree\s+src' "--list-sources names the tree the negative-space clause scans"

# ================ Arm 15: preflight.sh --landing wiring (PREVENT) ===========
# Pre-wiring these are the RED witness for the gap: a tree whose code no longer
# satisfies the ratified contract cleared --landing with 0.

NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# make_repo <primary> <linked> — a throwaway repo carrying COPIES of everything
# preflight's landing gates read, so only the new §10 is under test. A linked
# worktree is required: TC-RUBRIC-5 forbids --landing in a primary checkout.
#
# §9's governed surface is seeded SYNTHETICALLY (lib/governed-surface-fixture.sh)
# rather than copied: it is incidental to this suite, and that pin is EXPECTED to
# trip during 0.8.20, so copying the real pair would couple this suite to an
# unrelated signing state. §10's subject is seeded by lib/c1-conformance-fixture.sh
# — the same helper the sibling suites use, so the seeder itself gets exercised
# here too.
make_repo() {
  local primary="$1" linked="$2"
  mkdir -p "$primary/scripts" "$primary/dev/steward"
  git init -q -b main "$primary"
  git -C "$primary" config user.email c1-test@example.invalid
  git -C "$primary" config user.name 'C1 Test'
  git -C "$primary" config commit.gpgsign false
  git -C "$primary" config core.hooksPath "$NO_HOOKS"
  seed_governed_surface_fixture "$primary"
  seed_c1_conformance_fixture "$primary"
  printf '{"seq":1,"note":"fixture"}\n' >"$primary/dev/steward/steward-ledger.jsonl"
  printf '%s' 1 >"$primary/dev/steward/steward-ledger.jsonl.seq"
  git -C "$primary" add -A
  git -C "$primary" commit -q -m 'fixture: initial commit'
  git -C "$primary" worktree add -q -b landing-fixture "$linked" >/dev/null 2>&1
}

run_preflight() {
  local cwd="$1"; shift
  set +e
  OUT="$(cd "$cwd" && bash "$PREFLIGHT" "$@" 2>&1)"
  RC=$?
  set -e
}

CLEAN_PRIMARY="$TMPROOT/repo-clean"; CLEAN_LINKED="$TMPROOT/repo-clean-wt"
make_repo "$CLEAN_PRIMARY" "$CLEAN_LINKED"

DIVERGED_PRIMARY="$TMPROOT/repo-diverged"; DIVERGED_LINKED="$TMPROOT/repo-diverged-wt"
make_repo "$DIVERGED_PRIMARY" "$DIVERGED_LINKED"
# Break the CODE, not the contract: the point of R-20-H7 is that as-built code
# must keep satisfying a contract that has NOT moved.
mkdir -p "$DIVERGED_LINKED/src/rust/crates/fathomdb-engine/src"
printf 'pub struct EntityTypeSpec { pub id_prefix: String }\n' \
  >"$DIVERGED_LINKED/src/rust/crates/fathomdb-engine/src/regression.rs"

run_preflight "$DIVERGED_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing HARD-fails in a worktree whose code breaks a C-1 clause"
else
  fail "--landing MUST fail on a C-1 conformance failure; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'HARD.*c1-contract-conformance:'; then
  pass "--landing failure output names the c1-contract-conformance check"
else
  fail "expected a HARD line naming c1-contract-conformance; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'C1-Q6B-NO-ENTITYTYPESPEC-NO-IDPREFIX'; then
  pass "--landing failure carries the specific clause id through to the operator"
else
  fail "expected the HARD line to name the failing clause id; got: $OUT"
fi

run_preflight "$CLEAN_LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing still exits 0 in a worktree that conforms to the pinned contract"
else
  fail "--landing must not regress a conforming tree; got rc=$RC, out: $OUT"
fi

# Mirrors §7/§8/§9's contract: --landing-only, so plain preflight stays lean.
run_preflight "$DIVERGED_LINKED"
if printf '%s' "$OUT" | grep -q 'c1-contract-conformance:'; then
  fail "c1-contract-conformance must be --landing-only; it ran without --landing: $OUT"
else
  pass "regression guard: c1-contract-conformance is inert without --landing"
fi

# ANTI-FAIL-OPEN: a non-zero rc with NO FAIL line means the checker itself could
# not run (exit 2). That must still BLOCK the land, never degrade into INFO lines
# and a green summary — the exact hole §8/§9 close for their own checkers.
EVAP_PRIMARY="$TMPROOT/repo-evap"; EVAP_LINKED="$TMPROOT/repo-evap-wt"
make_repo "$EVAP_PRIMARY" "$EVAP_LINKED"
rm -f "$EVAP_LINKED/dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md"
run_preflight "$EVAP_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing HARD-fails when the C-1 gate itself cannot run (anti-fail-open)"
else
  fail "--landing MUST block when the gate evaporates; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'refusing to certify this tree for landing'; then
  pass "the anti-fail-open path says it refuses to certify the tree"
else
  fail "expected the anti-fail-open refusal line; got: $OUT"
fi

# ======================== Arm 16: CI wiring is ALWAYS-ON =====================
# A docs_only-gated job never fires on a code push — and the C-1 contract is a
# DESIGN DOC while the code it governs is SOURCE, so either fast path would make
# this job absent on exactly the pushes that matter. Assert statically that the
# job exists, runs the SHARED script, carries the recurrence guard, and has no
# `if:` and no `needs:` at all.
CI_JOB_BLOCK="$(awk '
  /^  c1-contract-conformance:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a c1-contract-conformance job"
else
  fail "ci.yml has no c1-contract-conformance job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/check-c1-conformance.sh'; then
  pass "the CI job runs the SHARED scripts/check-c1-conformance.sh (one predicate, two callers)"
else
  fail "the CI job must invoke scripts/check-c1-conformance.sh, not a reimplementation"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/tests/test_check_c1_conformance.sh'; then
  pass "the CI job carries the recurrence guard for the gate itself"
else
  fail "the CI job must also run scripts/tests/test_check_c1_conformance.sh"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*if:'; then
  fail "the c1-contract-conformance job must be ALWAYS-ON (no if:/docs_only gate); block: $CI_JOB_BLOCK"
else
  pass "the c1-contract-conformance job is always-on (no if: condition, not docs_only-gated)"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*needs:'; then
  fail "the c1-contract-conformance job must not depend on the changes job; block: $CI_JOB_BLOCK"
else
  pass "the c1-contract-conformance job has no needs: (does not ride the changes/docs_only fast path)"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0'; then
  pass "the CI job pins actions/checkout to the same SHA as its sibling jobs"
else
  fail "the CI job must pin actions/checkout by SHA, as the sibling gate jobs do"
fi

# ================= Arm 17: the fixture suite is registered ===================
if grep -q 'scripts/tests/test_check_c1_conformance.sh' "$REPO_ROOT/scripts/agent-test.sh"; then
  pass "agent-test.sh registers this fixture suite alongside its siblings"
else
  fail "scripts/agent-test.sh must register scripts/tests/test_check_c1_conformance.sh"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-c1-conformance tests passed\n'
