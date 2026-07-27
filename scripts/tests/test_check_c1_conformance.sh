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
#
# FIX-1 ARMS (arms 12d–12j) are the recurrence guard for codex §9 round 1: two
# [P2] FALSE NEGATIVES in which the gate exited 0 on a tree that violates the
# pinned contract — an open readiness vocabulary guarded only by a blacklist of
# one name, and case-sensitive `absent` regexes over SQL. Both classes were
# swept for and are fixed across every clause that carried them.
#
# FIX-2 ARMS (arms 12k–12p) are the recurrence guard for codex §9 round 2, which
# found three REFINEMENTS of those same two classes: a SQL table name may be
# SCHEMA-QUALIFIED (`main.canonical_attributes`), and an accepted string may be
# admitted OUTSIDE a simple match arm (an `if` guard, an or-pattern). 12p is the
# sweep's own product, in the same narrow-regex class.
#
# FIX-3 ARMS (arms 12q–12u, plus the tree-evaporation and probe-kind arms) are
# the recurrence guard for codex §9 round 3 — a DIFFERENT defect class from
# rounds 1–2. Not a regex shape: a SCOPE bug. Every negative (`absent`) probe read
# ONE FILE (a crate's lib.rs), so a literal, correctly-spelled, fully-qualified
# violation placed in a SIBLING MODULE of the same crate exited 0. Fixed by
# scoping every negative probe to the crate SOURCE TREE and by DELETING the
# file-scoped negative probe kind outright.
#
# FIX-4 ARMS (arms 12v–12ac, plus the `min`-probe-kind arms) are the recurrence
# guard for codex §9 round 4 — a FOURTH distinct class again. Not a regex shape
# (rounds 1–2) and not a file-vs-tree scope bug (round 3): SUBJECT BINDING. Every
# probe below asserted that some text existed SOMEWHERE IN A FILE, while the
# clause it implements is about a NAMED SUBJECT inside that file — a particular
# table's tokenizer, a particular struct's field, a particular enum's variant, a
# particular function's signature or body, a particular index's COLUMNS, a
# particular test's DEFINITION (as opposed to a comment mentioning it). Each
# fixture leaves a DECOY that satisfies the old probe, so each exited 0 while the
# obligation was violated. Fixed by extracting the named subject structurally and
# asserting inside it, and by DELETING the `min` count-probe kind outright.
#
# READ THE GATE'S "RESIDUAL SCOPE" HEADER SECTION BEFORE ADDING A ROUND 5, AND
# CLASSIFY THE FINDING FIRST. The header states, in writing, the evasion classes a
# static/lexical check CANNOT close (dynamically composed SQL, const/macro-
# indirected identifiers, ATTACH aliases, normalising comparisons); arms for those
# belong to a DIFFERENT mechanism — a runtime `sqlite_master` assertion or a real
# parse — not to another regex here. But round 3 was NOT one of those: a LITERAL
# spelling of a PINNED name in ordinary source text that this gate does not see is
# a real defect with a definite fix, and it gets fixed.
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

# === Arm 12d–12g (RED, fix-1): CLOSED VOCABULARY, not a blacklist ===========
# codex §9 round 1, finding #1 [P2]. Three contract clauses say a vocabulary is
# EXACTLY some set ("EXACTLY {ready, embedding}", "exactly {filterable,
# rankable, searchable}", "total over exactly those three"). The gate used to
# assert "the members I want are PRESENT" plus, in one case, "one specific bad
# name is ABSENT" — which is a BLACKLIST OF ONE, not a closed vocabulary. Any
# member added under a name nobody had thought to forbid walked straight
# through, and the gate reported 0 on a tree that violates the pinned contract:
# a false assurance in a PUBLISH-PRECONDITION gate, the TC-37 class in costume.
#
# Each arm below adds a member whose NAME the old blacklist could not have
# known. Every one of them exited 0 before this round.

# 12d — codex's own demonstration: a THIRD DenseReadiness variant.
VOCAB_ROOT="$(make_root readiness-third-variant)"
python3 - "$VOCAB_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
i = text.index("pub enum DenseReadiness {")
j = text.index("\n}", i)
text = text[:j] + "\n    Failed," + text[j:]
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$VOCAB_ROOT"
expect_rc 1 "a THIRD DenseReadiness variant HARD-fails (the readiness vocabulary is CLOSED)"
expect_out 'C1-Q4-DENSE-READINESS-TWO-MEMBERS' "the third-variant failure NAMES the clause id"
expect_out 'Failed' "the third-variant failure NAMES the unpinned variant it found"
expect_routes_to_steward "the readiness-vocabulary failure"

# 12e — the same hole through the STRING vocabulary: the enum keeps exactly two
# variants, but a third accepted SPELLING is admitted. The clause reserves
# `pending` for the orthogonal admission axis, and the old probes could not see
# this at all (they matched `DenseReadiness::Pending`, which never appears).
SPELL_ROOT="$(make_root readiness-third-spelling)"
python3 - "$SPELL_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = '            "ready" => Some(DenseReadiness::Ready),'
assert old in text
text = text.replace(old, old + '\n            "pending" => Some(DenseReadiness::Ready),', 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SPELL_ROOT"
expect_rc 1 "a THIRD accepted readiness SPELLING HARD-fails even with the enum unchanged"
expect_out 'C1-Q4-DENSE-READINESS-TWO-MEMBERS' "the third-spelling failure NAMES the clause id"
expect_out 'pending' "the third-spelling failure NAMES the reserved token it found"

# 12f — the SAME DEFECT CLASS in a sibling clause: a FOURTH ProjectionRole. The
# old probes blacklisted exactly two names (Vector, Fts), so any other name
# passed.
ROLE_ROOT="$(make_root fourth-projection-role)"
python3 - "$ROLE_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
i = text.index("pub enum ProjectionRole {")
j = text.index("\n}", i)
text = text[:j] + "\n    Embeddable," + text[j:]
text = text.replace('            ProjectionRole::Searchable => "searchable",',
                    '            ProjectionRole::Searchable => "searchable",\n'
                    '            ProjectionRole::Embeddable => "embeddable",', 1)
text = text.replace('            "searchable" => Some(ProjectionRole::Searchable),',
                    '            "searchable" => Some(ProjectionRole::Searchable),\n'
                    '            "embeddable" => Some(ProjectionRole::Embeddable),', 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$ROLE_ROOT"
expect_rc 1 "a FOURTH ProjectionRole HARD-fails (the role vocabulary is exactly three)"
expect_out 'C1-Q6A-THREE-ROLES' "the fourth-role failure NAMES the clause id"
expect_out 'Embeddable' "the fourth-role failure NAMES the unpinned role it found"

# 12g — same class again, and this one had NO blacklist at all: a FOURTH
# IdSpaceKind. The clause says the typed id space is TOTAL over exactly three,
# so a fourth variant is precisely the violation, and four `present` probes
# could never see it.
IDSPACE_ROOT="$(make_root fourth-id-space)"
python3 - "$IDSPACE_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
i = text.index("pub enum IdSpaceKind {")
j = text.index("\n}", i)
text = text[:j] + "\n    Chunk," + text[j:]
text = text.replace('            Self::Passage => "p:",',
                    '            Self::Passage => "p:",\n            Self::Chunk => "c:",', 1)
text = text.replace('            Self::Passage => "passage",',
                    '            Self::Passage => "passage",\n'
                    '            Self::Chunk => "chunk",', 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$IDSPACE_ROOT"
expect_rc 1 "a FOURTH IdSpaceKind HARD-fails (the typed id space is total over exactly three)"
expect_out 'C1-Q6B-IDSPACE-TOTAL-THREE' "the fourth-id-space failure NAMES the clause id"
expect_out 'Chunk' "the fourth-id-space failure NAMES the unpinned variant it found"

# === Arm 12h–12j (RED, fix-1): SQL is CASE-INSENSITIVE ======================
# codex §9 round 1, finding #2 [P2]. The negative-space probes over SQL were
# anchored to ONE uppercase spelling with single spaces. SQL is case-insensitive
# and admits `INSERT OR REPLACE INTO` / `INSERT OR IGNORE INTO` with an
# intervening conflict clause and arbitrary whitespace (newlines included), so
# a lowercase or modifier-carrying backfill cleared the gate with 0.

# 12h — codex's own demonstration: a lowercase migration-time backfill.
BACKFILL_ROOT="$(make_root lowercase-backfill)"
cat >>"$BACKFILL_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'RS'

// Fixture only. A LOWERCASE migration-time backfill of pre-existing rows —
// exactly what the Q2 NO-DATA-MIGRATION clause forbids, and exactly what an
// uppercase-anchored `absent` regex walked straight past.
const FIXTURE_BACKFILL_SQL: &str = "insert into canonical_attributes select * from old_attrs;";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$BACKFILL_ROOT"
expect_rc 1 "a LOWERCASE backfill into the EAV store HARD-fails the NO-DATA-MIGRATION clause"
expect_out 'C1-Q2-NO-DATA-MIGRATION' "the lowercase-backfill failure NAMES the clause id"
expect_routes_to_steward "the lowercase-backfill clause failure"

# 12i — the modifier + whitespace form, on the property-FTS half of the clause.
MODIFIER_ROOT="$(make_root insert-or-replace-backfill)"
cat >>"$MODIFIER_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'RS'

// Fixture only. `INSERT OR REPLACE INTO` — a valid SQLite backfill form with an
// intervening conflict clause and a NEWLINE before the table name.
const FIXTURE_BACKFILL_SQL: &str = "INSERT OR REPLACE INTO
    property_search_index(attr_value, attr_name, write_cursor)
    SELECT value, name, cursor FROM legacy_props;";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$MODIFIER_ROOT"
expect_rc 1 "an INSERT OR REPLACE INTO backfill (newline before the table) HARD-fails"
expect_out 'C1-Q2-NO-DATA-MIGRATION' "the modifier-backfill failure NAMES the clause id"

# 12j — the SAME DEFECT CLASS in a sibling clause: the deferred-custom-tokenizer
# clause forbids the ENGINE from creating the property-FTS table itself, and its
# `absent` probe was uppercase-anchored too.
LOWER_DDL_ROOT="$(make_root lowercase-fts-ddl)"
cat >>"$LOWER_DDL_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'RS'

// Fixture only. The ENGINE creating the property-FTS table, in lowercase.
const FIXTURE_DDL: &str = "create virtual table property_search_index using fts5(attr_value)";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$LOWER_DDL_ROOT"
expect_rc 1 "a LOWERCASE engine-side property-FTS DDL HARD-fails the deferred-tokenizer clause"
expect_out 'C1-TE-CUSTOM-TOKENIZER-DEFERRED' "the lowercase-DDL failure NAMES the clause id"

# === Arm 12k–12m (RED, fix-2): SQL TABLE NAMES MAY BE SCHEMA-QUALIFIED =======
# codex §9 round 2, findings #1 and #2 [P2]. fix-1 widened the SQL negative-space
# probes for CASE and WHITESPACE but still required the pinned table name to sit
# IMMEDIATELY after `INTO` / `TABLE`. SQLite names a table as
# `[<schema>.]<table>`, and `main.` is always a valid schema for the main
# database, so `INSERT INTO main.canonical_attributes ...` writes exactly the
# forbidden table and cleared the gate with 0. The same shape cleared the
# engine-side FTS DDL probe.
#
# Each arm below is a VALID SQLite spelling of the forbidden statement. Every one
# of them exited 0 before this round.

# 12k — the bare `main.` qualifier on the EAV store (codex's own example).
QUAL_INSERT_ROOT="$(make_root schema-qualified-backfill)"
cat >>"$QUAL_INSERT_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'RS'

// Fixture only. A SCHEMA-QUALIFIED migration-time backfill. `main` is always a
// valid schema name for the main database, so this writes the same table the
// Q2 NO-DATA-MIGRATION clause forbids backfilling.
const FIXTURE_BACKFILL_SQL: &str = "INSERT INTO main.canonical_attributes SELECT * FROM old_attrs;";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$QUAL_INSERT_ROOT"
expect_rc 1 "a SCHEMA-QUALIFIED backfill (main.canonical_attributes) HARD-fails NO-DATA-MIGRATION"
expect_out 'C1-Q2-NO-DATA-MIGRATION' "the schema-qualified-backfill failure NAMES the clause id"
expect_routes_to_steward "the schema-qualified-backfill clause failure"

# 12l — the same evasion stacked with every fix-1 tolerance at once: lowercase, a
# conflict clause, a QUOTED schema name, and whitespace around the dot. This is
# the property-FTS half of the clause.
QUAL_QUOTED_ROOT="$(make_root schema-qualified-quoted-backfill)"
cat >>"$QUAL_QUOTED_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'RS'

// Fixture only. Lowercase + `OR IGNORE` + a QUOTED schema + whitespace around
// the dot — all four are legal SQLite, and all four are one statement.
const FIXTURE_BACKFILL_SQL: &str = "insert or ignore into `main` . property_search_index(attr_value) select v from legacy;";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$QUAL_QUOTED_ROOT"
expect_rc 1 "a QUOTED, spaced schema qualifier (\`main\` . property_search_index) HARD-fails"
expect_out 'C1-Q2-NO-DATA-MIGRATION' "the quoted-qualifier failure NAMES the clause id"

# 12m — the SAME DEFECT CLASS in the sibling clause: the engine creating the
# property-FTS table under a schema-qualified name.
QUAL_DDL_ROOT="$(make_root schema-qualified-fts-ddl)"
cat >>"$QUAL_DDL_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'RS'

// Fixture only. The ENGINE creating the property-FTS table, schema-qualified.
const FIXTURE_DDL: &str = "CREATE VIRTUAL TABLE main.property_search_index USING fts5(attr_value)";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$QUAL_DDL_ROOT"
expect_rc 1 "a SCHEMA-QUALIFIED engine-side property-FTS DDL HARD-fails the deferred-tokenizer clause"
expect_out 'C1-TE-CUSTOM-TOKENIZER-DEFERRED' "the schema-qualified-DDL failure NAMES the clause id"

# === Arm 12n–12o (RED, fix-2): ACCEPTED SPELLINGS OUTSIDE A SIMPLE ARM ======
# codex §9 round 2, finding #3 [P2]. fix-1 closed the string vocabulary by
# harvesting `"s" => Some(Ty::V)` match arms — but a token can be ADMITTED by
# other Rust syntax that never forms such an arm. Two of them are ordinary code
# a contributor might really write:
#   * a guard BEFORE the match: `if value == "pending" { return Some(..) }`
#   * an OR-PATTERN arm: `"vector" | "searchable" => Some(..)`, where only the
#     LAST alternative sits immediately before the `=>`.
# In both cases arm_pairs() harvested exactly the pinned pairs and reported the
# closed vocabulary as EXACT while the function accepted a third token. Both
# exited 0 before this round.

# 12n — the `if` guard. The enum is untouched and every pinned arm is intact, so
# enum_exact and the `present` probes all hold; only the extra ACCEPTED TOKEN is
# the violation, and `pending` is precisely the token the clause reserves for the
# orthogonal admission axis.
GUARD_ROOT="$(make_root readiness-if-guard-spelling)"
python3 - "$GUARD_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = '''    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "ready" => Some(DenseReadiness::Ready),'''
assert old in text
new = '''    pub fn from_str_opt(value: &str) -> Option<Self> {
        if value == "pending" {
            return Some(DenseReadiness::Ready);
        }
        match value {
            "ready" => Some(DenseReadiness::Ready),'''
open(p, "w", encoding="utf-8").write(text.replace(old, new, 1))
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$GUARD_ROOT"
expect_rc 1 'an accepted spelling admitted by an `if` GUARD before the match HARD-fails'
expect_out 'C1-Q4-DENSE-READINESS-TWO-MEMBERS' "the if-guard failure NAMES the clause id"
expect_out 'pending' "the if-guard failure NAMES the unpinned token it found"
expect_routes_to_steward "the if-guard vocabulary failure"

# 12o — the OR-PATTERN arm, in a sibling clause. Note this fixture deliberately
# admits the string `"vector"` and NOT the identifier `ProjectionRole::Vector`,
# so the clause's existing `absent` probe cannot fire: the only thing that can
# catch it is a genuinely closed string vocabulary.
ORPAT_ROOT="$(make_root role-or-pattern-spelling)"
python3 - "$ORPAT_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = '            "searchable" => Some(ProjectionRole::Searchable),'
assert old in text
new = '            "vector" | "searchable" => Some(ProjectionRole::Searchable),'
open(p, "w", encoding="utf-8").write(text.replace(old, new, 1))
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$ORPAT_ROOT"
expect_rc 1 "an accepted spelling admitted by an OR-PATTERN arm HARD-fails"
expect_out 'C1-Q6A-THREE-ROLES' "the or-pattern failure NAMES the clause id"
expect_out 'vector' "the or-pattern failure NAMES the unpinned token it found"

# === Arm 12p (RED, fix-2 SWEEP): the same NARROW-REGEX class, elsewhere ======
# Not a codex finding — the product of sweeping every remaining negative probe
# for the shape the three findings share. C1-Q6B-ID-NON-NULL's `absent` probe was
# the literal string `pub id: Option<IdSpace>`, so any legal Rust respacing
# (`pub id : Option < IdSpace >`) evades it.
#
# The clause is NOT trivially exploitable — its paired `present` probe
# (`pub id: IdSpace,`) fails on the same edit — so this fixture also plants a
# decoy struct carrying that exact text. With the present probe satisfied, the
# narrow `absent` probe was the only thing left standing, and it exited 0.
SPACED_OPT_ROOT="$(make_root id-non-null-respaced)"
python3 - "$SPACED_OPT_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert "    pub id: IdSpace,\n" in text
text = text.replace("    pub id: IdSpace,\n", "    pub id : Option < IdSpace > ,\n", 1)
text += (
    "\n/// Fixture only. A decoy carrying the exact text the clause's `present`\n"
    "/// probe looks for, so that probe still holds and the NARROW `absent` probe\n"
    "/// is the only thing standing between this tree and a green.\n"
    "pub struct FixtureDecoy {\n    pub id: IdSpace,\n}\n"
)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SPACED_OPT_ROOT"
expect_rc 1 "a RESPACED \`pub id : Option < IdSpace >\` HARD-fails the id-non-null clause"
expect_out 'C1-Q6B-ID-NON-NULL' "the respaced-Option failure NAMES the clause id"

# === Arm 12q–12u (RED, fix-3): A NEGATIVE PROBE'S SCOPE IS THE CRATE TREE ====
# codex §9 round 3, findings #1 and #2 [P2]. This is NOT another variant of the
# rounds 1–2 regex-shape class, and it is NOT inside the gate's stated residual
# scope: every fixture below is a LITERAL, correctly-spelled, fully-qualified
# violation — precisely what the gate claims to catch — that the gate walked past
# because its `absent` probes read ONE FILE (the crate's lib.rs) instead of the
# crate's SOURCE TREE. A migration added as a NEW MODULE next to lib.rs exited 0.
#
# The engine crate ALREADY carries sibling modules (lifecycle.rs, pcache2.rs), so
# "somebody adds a module" is not a hypothetical: it is the default way a crate
# grows. A negative assertion scoped to one file is only as strong as the
# assumption that nobody ever does that.
#
# Every arm below is a NEW FILE in a crate source tree; lib.rs is untouched, so
# every `present` / `enum_exact` / `arms_exact` probe of the same clause still
# holds and the negative probe is the ONLY thing standing. All five exited 0
# before this round.

# 12q — codex finding #1: a migration/backfill in a SIBLING MODULE of the schema
# crate. Both halves of the clause are exercised in one file.
SIBLING_MIGRATION_ROOT="$(make_root schema-sibling-module-backfill)"
cat >"$SIBLING_MIGRATION_ROOT/src/rust/crates/fathomdb-schema/src/migrations_extra.rs" <<'RS'
// Fixture only. A NEW MODULE in the schema crate — NOT an edit to lib.rs. Both
// statements are exactly what the Q2 NO-DATA-MIGRATION clause forbids, spelled
// literally, correctly and (for the first) fully schema-qualified.
pub const FIXTURE_BACKFILL_ATTRS: &str =
    "INSERT INTO main.canonical_attributes SELECT * FROM old_attrs;";
pub const FIXTURE_BACKFILL_FTS: &str =
    "INSERT INTO property_search_index(attr_value) SELECT value FROM legacy_props;";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SIBLING_MIGRATION_ROOT"
expect_rc 1 "a backfill in a SIBLING MODULE of the schema crate HARD-fails NO-DATA-MIGRATION"
expect_out 'C1-Q2-NO-DATA-MIGRATION' "the sibling-module backfill failure NAMES the clause id"
expect_out 'migrations_extra\.rs' "the sibling-module backfill failure NAMES the offending FILE"
expect_routes_to_steward "the sibling-module backfill clause failure"

# 12r — codex finding #2, verbatim: a new engine module that creates the
# property-FTS table itself.
SIBLING_FTS_ROOT="$(make_root engine-sibling-module-fts-ddl)"
cat >"$SIBLING_FTS_ROOT/src/rust/crates/fathomdb-engine/src/extra_fts.rs" <<'RS'
// Fixture only — codex §9 round 3's own demonstration, file name included. The
// ENGINE creating the property-FTS table is what the deferred-custom-tokenizer
// clause forbids; the schema migration owns that table.
pub const FIXTURE_DDL: &str =
    "CREATE VIRTUAL TABLE main.property_search_index USING fts5(attr_value)";
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SIBLING_FTS_ROOT"
expect_rc 1 "engine-side property-FTS DDL in a SIBLING MODULE HARD-fails the deferred-tokenizer clause"
expect_out 'C1-TE-CUSTOM-TOKENIZER-DEFERRED' "the sibling-module DDL failure NAMES the clause id"
expect_out 'extra_fts\.rs' "the sibling-module DDL failure NAMES the offending FILE"

# 12s — fix-3 SWEEP (not a codex finding): the same module-scope class in
# C1-Q6B-ID-NON-NULL. A NULLABLE typed id carrier declared next to lib.rs.
SIBLING_OPT_ID_ROOT="$(make_root engine-sibling-module-optional-id)"
cat >"$SIBLING_OPT_ID_ROOT/src/rust/crates/fathomdb-engine/src/hit_v2.rs" <<'RS'
// Fixture only (fix-3 SWEEP). The contract's id is NON-NULL; this sibling module
// declares a nullable one. lib.rs is untouched, so the clause's `present` probe
// (`pub id: IdSpace,`) still holds and the negative probe is all that is left.
pub struct SearchHitV2 {
    pub id: Option<IdSpace>,
}
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SIBLING_OPT_ID_ROOT"
expect_rc 1 "a nullable typed id in a SIBLING MODULE HARD-fails the id-non-null clause"
expect_out 'C1-Q6B-ID-NON-NULL' "the sibling-module optional-id failure NAMES the clause id"
expect_out 'hit_v2\.rs' "the sibling-module optional-id failure NAMES the offending FILE"

# 12t — fix-3 SWEEP: the same class in C1-Q6A-THREE-ROLES. Vector/Fts are TIER
# LABELS on the sub-objects, not roles — the confusion the clause calls out — and
# reintroducing them as roles from a sibling module was invisible.
SIBLING_TIER_ROOT="$(make_root engine-sibling-module-tier-roles)"
cat >"$SIBLING_TIER_ROOT/src/rust/crates/fathomdb-engine/src/tier_labels.rs" <<'RS'
// Fixture only (fix-3 SWEEP). ProjectionRole::Vector / ::Fts reintroduced as
// roles from a sibling module. The enum in lib.rs still holds exactly three
// variants, so enum_exact and arms_exact both pass.
pub const DEFAULT_TIER: ProjectionRole = ProjectionRole::Vector;
pub const TEXT_TIER: ProjectionRole = ProjectionRole::Fts;
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SIBLING_TIER_ROOT"
expect_rc 1 "ProjectionRole::Vector/::Fts in a SIBLING MODULE HARD-fails the three-roles clause"
expect_out 'C1-Q6A-THREE-ROLES' "the sibling-module tier-role failure NAMES the clause id"
expect_out 'tier_labels\.rs' "the sibling-module tier-role failure NAMES the offending FILE"

# 12u — fix-3 SWEEP: the last remaining file-scoped negative, in
# C1-Q4-DENSE-READINESS-TWO-MEMBERS. `pending` is reserved by the clause for the
# orthogonal admission axis.
SIBLING_PENDING_ROOT="$(make_root engine-sibling-module-pending-readiness)"
cat >"$SIBLING_PENDING_ROOT/src/rust/crates/fathomdb-engine/src/readiness_ext.rs" <<'RS'
// Fixture only (fix-3 SWEEP). The reserved `Pending` readiness state, referenced
// from a sibling module. The enum in lib.rs is untouched.
pub const RESERVED: DenseReadiness = DenseReadiness::Pending;
RS
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SIBLING_PENDING_ROOT"
expect_rc 1 "DenseReadiness::Pending in a SIBLING MODULE HARD-fails the readiness clause"
expect_out 'C1-Q4-DENSE-READINESS-TWO-MEMBERS' "the sibling-module Pending failure NAMES the clause id"
expect_out 'readiness_ext\.rs' "the sibling-module Pending failure NAMES the offending FILE"

# === Arm 12v–12ac (RED, fix-4): A PROBE MUST BE BOUND TO ITS SUBJECT =========
# codex §9 round 4 [P2], plus the sweep for the same shape everywhere else. This
# is a FOURTH distinct class, and again not a regex-shape or scope problem: each
# probe below was pointed at a FILE rather than at THE THING THE CLAUSE IS ABOUT,
# so an unrelated coincidence elsewhere in that same file satisfied it while the
# named subject was broken.
#
# The shape, in one sentence: a `present`/`min` probe over a whole file asserts
# "this text exists SOMEWHERE in this file", but every one of these clauses says
# something about a NAMED subject — property_search_index's tokenizer,
# ProjectionSpec's `roles` field, EngineError's variant, the apply verb's
# signature, the index's COLUMNS, the named test's DEFINITION.
#
# EVERY FIXTURE BELOW LEAVES A DECOY (or an existing unrelated occurrence) that
# satisfies the OLD probe, so every one of them exited 0 before this round while
# the contract obligation was violated in the tree.

# 12v — codex §9 round 4's own demonstration, verbatim: ONLY
# `property_search_index`'s tokenizer is changed. The old probe counted file-wide
# occurrences of the default tokenizer string (`min`, n=2) and the schema file
# carries several for UNRELATED FTS tables (search_index, search_index_v2, the
# edge index), so the count stayed satisfied by tables the clause is not about.
TOKENIZER_ROOT="$(make_root property-fts-tokenizer-diverged)"
python3 - "$TOKENIZER_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = """CREATE VIRTUAL TABLE property_search_index USING fts5(
                  attr_value,
                  attr_name UNINDEXED,
                  write_cursor UNINDEXED,
                  tokenize = 'porter unicode61 remove_diacritics 2'
              );"""
new = """CREATE VIRTUAL TABLE property_search_index USING fts5(
                  attr_value,
                  attr_name UNINDEXED,
                  write_cursor UNINDEXED,
                  tokenize = 'unicode61'
              );"""
assert old in text
open(p, "w", encoding="utf-8").write(text.replace(old, new, 1))
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$TOKENIZER_ROOT"
expect_rc 1 "property-FTS diverging from the body-FTS tokenizer HARD-fails the default-tokenizer clause"
expect_out 'C1-TE-DEFAULT-TOKENIZER' "the diverged-tokenizer failure NAMES the clause id"
expect_out 'property_search_index' "the diverged-tokenizer failure NAMES the subject table"
expect_out 'search_index_v2' "the diverged-tokenizer failure NAMES the body-FTS table it is compared against"
expect_routes_to_steward "the diverged-tokenizer clause failure"

# The same clause must ALSO fail when the property-FTS tokenizer is deleted
# outright (FTS5 then silently falls back to its own `unicode61` default, which is
# NOT the engine default the clause names).
NO_TOKENIZER_ROOT="$(make_root property-fts-tokenizer-deleted)"
python3 - "$NO_TOKENIZER_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = """                  write_cursor UNINDEXED,
                  tokenize = 'porter unicode61 remove_diacritics 2'
              );",
    },
];"""
new = """                  write_cursor UNINDEXED
              );",
    },
];"""
assert old in text
open(p, "w", encoding="utf-8").write(text.replace(old, new, 1))
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$NO_TOKENIZER_ROOT"
expect_rc 1 "DELETING the property-FTS tokenize clause HARD-fails (fts5 would default to unicode61)"
expect_out 'C1-TE-DEFAULT-TOKENIZER' "the deleted-tokenizer failure NAMES the clause id"

# 12w — fix-4 SWEEP: SEVEN clauses whose obligation is a FIELD OF A NAMED STRUCT
# were probed as a bare `pub <field>: <ty>,` anywhere in engine lib.rs. This
# fixture renames every one of them IN ITS OWN STRUCT and plants ONE decoy struct
# carrying the seven original lines, so all seven old probes stay satisfied while
# ProjectionSpec has no `roles`, ProjectionDelta no `built`/`deferred`/`unchanged`,
# SearchHit no typed `id`, ProjectionVector no `embedder` and ProjectionFts no
# `tokenizer`. The gate exited 0 on this tree.
DECOY_FIELD_ROOT="$(make_root decoy-struct-fields)"
python3 - "$DECOY_FIELD_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
renames = [
    ("    pub roles: BTreeSet<ProjectionRole>,", "    pub role_set: BTreeSet<ProjectionRole>,"),
    ("    pub built: Vec<String>,", "    pub built_names: Vec<String>,"),
    ("    pub deferred: Vec<String>,", "    pub deferred_names: Vec<String>,"),
    ("    pub unchanged: bool,", "    pub no_op: bool,"),
    ("    pub id: IdSpace,", "    pub hit_id: IdSpace,"),
    ("    pub embedder: Option<String>,", "    pub embedder_name: Option<String>,"),
    ("    pub tokenizer: Option<String>,", "    pub tokenizer_name: Option<String>,"),
]
for old, new in renames:
    assert text.count(old) == 1, old
    text = text.replace(old, new, 1)
text += """
/// Fixture only (fix-4 SWEEP). A decoy carrying the exact text each clause's
/// `present` probe looked for — in a struct the contract never names. With this
/// present, every one of those seven probes holds while the SUBJECT struct of
/// each clause no longer carries the field at all.
pub struct FixtureFieldDecoy {
    pub roles: BTreeSet<ProjectionRole>,
    pub built: Vec<String>,
    pub deferred: Vec<String>,
    pub unchanged: bool,
    pub id: IdSpace,
    pub embedder: Option<String>,
    pub tokenizer: Option<String>,
}
"""
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$DECOY_FIELD_ROOT"
expect_rc 1 "fields moved OUT of their named structs (with a decoy) HARD-fail every affected clause"
for CLAUSE in C1-Q1-ROLE-SET C1-Q5-DERIVED-CACHE-IDEMPOTENT C1-Q4-CHEAP-SAME-TRANSACTION \
              C1-Q6A-RANKABLE-GRACEFUL-DEFER C1-Q6B-ID-NON-NULL C1-TE-DEFAULT-EMBEDDER \
              C1-TE-DEFAULT-TOKENIZER; do
  expect_out "$CLAUSE" "the decoy-field failure NAMES $CLAUSE"
done
expect_routes_to_steward "the decoy-field clause failures"

# 12x — fix-4 SWEEP: two clauses name an ERROR VARIANT of `enum EngineError`, but
# were probed for `<Variant> {` anywhere in the file — which the Display impl and
# the construction sites spell too. This fixture DELETES both variants from the
# enum and leaves those uses in place.
DECOY_VARIANT_ROOT="$(make_root variant-deleted-uses-kept)"
python3 - "$DECOY_VARIANT_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
for decl in ("""    NotLifecycleAddressable {
        id_space: IdSpaceKind,
    },
""", """    ProjectionDestructive {
        name: String,
        delta: String,
    },
"""):
    assert text.count(decl) == 1, decl
    text = text.replace(decl, "", 1)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$DECOY_VARIANT_ROOT"
expect_rc 1 "deleting an error VARIANT while its uses remain HARD-fails the clauses that name it"
expect_out 'C1-Q3-DESTRUCTIVE-DELTA' "the deleted-variant failure NAMES the destructive-delta clause"
expect_out 'C1-Q6B-H-TERMINAL-NOT-LIFECYCLE-ADDRESSABLE' \
  "the deleted-variant failure NAMES the h-terminal clause"

# 12y — fix-4 SWEEP: the COHESION SEAM clause says the engine verb takes the
# specs AND an explicit drop list and returns a delta. It was three INDEPENDENT
# file-wide probes, so they need not describe the same function. This fixture
# rewrites `configure_projections`'s actual signature and plants a decoy verb
# carrying the three probed fragments.
DECOY_SIG_ROOT="$(make_root apply-verb-signature-rewritten)"
python3 - "$DECOY_SIG_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = """    pub fn configure_projections(
        &self,
        specs: &[ProjectionSpec],
        drop: &[String],
    ) -> Result<ProjectionDelta, EngineError> {"""
new = """    pub fn configure_projections(
        &self,
        specs: &[ProjectionSpec],
    ) -> Result<ProjectionDelta, ()> {
        let drop: &[String] = &[];"""
assert text.count(old) == 1
text = text.replace(old, new, 1)
text += """
/// Fixture only (fix-4 SWEEP). A decoy carrying the two signature fragments and
/// the return type the clause's probes looked for, on a function that is NOT the
/// projection apply verb.
pub fn fixture_decoy_verb(
    specs: &[ProjectionSpec],
    drop: &[String],
) -> Result<ProjectionDelta, EngineError> {
    let _ = (specs, drop);
    Ok(ProjectionDelta::default())
}
"""
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$DECOY_SIG_ROOT"
expect_rc 1 "an apply verb that no longer takes an explicit drop list HARD-fails the seam clause"
expect_out 'C1-SEAM-ENGINE-BUILD-DROP' "the rewritten-signature failure NAMES the seam clause id"
expect_out 'configure_projections' "the rewritten-signature failure NAMES the subject function"

# 12z — fix-4 SWEEP: two clauses are about what happens INSIDE the apply verb —
# the cheap tier is built in the SAME TRANSACTION as the apply, and the apply
# WAKES the dispatcher rather than blocking on embeds. Both were probed as a
# fragment anywhere in the file. This fixture takes the `&tx` apply out of the
# transaction and removes the wake, leaving a decoy call and the three unrelated
# `notify_new_work()` call sites the file already carries.
OUTSIDE_VERB_ROOT="$(make_root apply-verb-body-gutted)"
python3 - "$OUTSIDE_VERB_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old_apply = "            let applied = apply_projection_config(&tx, specs, drop, dense_arm_live)?;"
new_apply = "            let applied = apply_projection_config(&connection, specs, drop, dense_arm_live)?;"
assert text.count(old_apply) == 1
text = text.replace(old_apply, new_apply, 1)
old_wake = """        if enqueued_backfill {
            self.projection_runtime.notify_new_work();
        }"""
new_wake = """        if enqueued_backfill {
            let _ = enqueued_backfill;
        }"""
assert text.count(old_wake) == 1
text = text.replace(old_wake, new_wake, 1)
text += """
/// Fixture only (fix-4 SWEEP). A decoy carrying the exact call text the
/// same-transaction clause's probe looked for, OUTSIDE the apply verb.
pub fn fixture_decoy_apply(tx: &Transaction<'_>) {
    let _ = apply_projection_config(&tx, &[], &[], true);
}
"""
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$OUTSIDE_VERB_ROOT"
expect_rc 1 "gutting the apply verb's body HARD-fails the same-transaction and no-block clauses"
expect_out 'C1-Q4-CHEAP-SAME-TRANSACTION' "the gutted-verb failure NAMES the same-transaction clause"
expect_out 'C1-AA-NO-BLOCK-ON-EMBEDDING' "the gutted-verb failure NAMES the no-block-on-embedding clause"

# 12aa — fix-4 SWEEP: the BEHAVIOURAL clauses assert that the named test/function
# still EXISTS. `fn <name>(` matched a COMMENT just as happily as a definition, so
# deleting the proof and leaving a reference to it behind exited 0.
COMMENT_ONLY_ROOT="$(make_root definition-replaced-by-comment)"
python3 - "$COMMENT_ONLY_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert text.count("fn commit_projection_outcomes(") == 1
text = text.replace("fn commit_projection_outcomes(", "fn commit_outcomes_v2(", 1)
text += """
// Fixture only (fix-4 SWEEP). The definition is gone; only this reference to
// `fn commit_projection_outcomes(` remains, and the probe could not tell the
// difference.
"""
open(p, "w", encoding="utf-8").write(text)
PY
python3 - "$COMMENT_ONLY_ROOT/src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert text.count("fn boot_rederive_converges_after_simulated_crash()") == 1
text = text.replace("fn boot_rederive_converges_after_simulated_crash()",
                    "fn boot_rederive_deleted_by_fixture()", 1)
text += """
// Fixture only (fix-4 SWEEP). The proof is deleted; only this doc reference to
// fn boot_rederive_converges_after_simulated_crash() survives.
"""
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$COMMENT_ONLY_ROOT"
expect_rc 1 "a definition deleted but MENTIONED IN A COMMENT HARD-fails the clauses that name it"
expect_out 'C1-AA-ATOMIC-FLIP' "the comment-only failure NAMES the atomic-flip clause"
expect_out 'C1-AA-CRASH-HEAL-BOOT-REDERIVE' "the comment-only failure NAMES the crash-heal clause"

# 12ab — fix-4 SWEEP: two SQL probes named a thing but not the OBLIGATION about
# it. The composite index was probed by NAME only, so re-creating it on a single
# column (the exact thing the cheap filterable tier cannot use) passed; and
# `fts_tokenizer TEXT,` was probed file-wide, so moving the recording column out
# of the projection registry passed as long as any table declared that column.
SQL_SUBJECT_ROOT="$(make_root sql-subject-swapped)"
python3 - "$SQL_SUBJECT_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old_idx = """CREATE INDEX canonical_attributes_name_value_idx
                  ON canonical_attributes(attr_name, attr_value);"""
new_idx = """CREATE INDEX canonical_attributes_name_value_idx
                  ON canonical_attributes(attr_value);"""
assert text.count(old_idx) == 1
text = text.replace(old_idx, new_idx, 1)
assert text.count("fts_tokenizer TEXT,") == 1
text = text.replace("fts_tokenizer TEXT,", "fts_tokenizer_name TEXT,", 1)
text += """
// Fixture only (fix-4 SWEEP). A decoy carrying the exact column text the
// deferred-tokenizer clause's probe looked for, in a table that is NOT the
// projection registry.
const FIXTURE_DECOY_DDL: &str = "CREATE TABLE fixture_notes(
                  fts_tokenizer TEXT,
                  note TEXT
              );";
"""
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$SQL_SUBJECT_ROOT"
expect_rc 1 "an index rebuilt on the wrong columns / a column moved to another table HARD-fails"
expect_out 'C1-Q2-ENGINE-EAV-PROPERTY-FTS' "the swapped-SQL-subject failure NAMES the EAV clause"
expect_out 'C1-TE-CUSTOM-TOKENIZER-DEFERRED' \
  "the swapped-SQL-subject failure NAMES the deferred-tokenizer clause"

# 12ac — fix-4 SWEEP: the landing-slot clause requires the 0.8.20 plan to CARRY
# the four C-1 co-land requirement ROWS. `\| R-20-PR \|` matched any mention
# between two pipes, including a sentence in running prose.
PLAN_MENTION_ROOT="$(make_root plan-row-demoted-to-mention)"
python3 - "$PLAN_MENTION_ROOT/dev/plans/plan-0.8.20.md" <<'PY'
import re
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
rows = [line for line in text.splitlines() if line.startswith("| R-20-PR |")]
assert len(rows) == 1, rows
text = text.replace(
    rows[0],
    "Withdrawn from the requirement table — the old | R-20-PR | row is archived.",
    1,
)
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$PLAN_MENTION_ROOT"
expect_rc 1 "a requirement ROW demoted to a prose MENTION HARD-fails the landing-slot clause"
expect_out 'C1-LAND-0820-SLOT' "the demoted-row failure NAMES the landing-slot clause id"

# === Arm 12ad (RED, fix-4 SWEEP): A STRING LITERAL IS NOT CODE ===============
# Found by turning the fix-4 question on the fix-4 patch itself: the structural
# readers read a COMMENT-STRIPPED view, but string literals were left in it, so
# `fn foo() { }` sitting inside a `&str` constant read exactly like a definition
# — and `enum DenseReadiness { Ready, Embedding }` inside one read exactly like a
# declaration. That is the fix-4 class again (a probe satisfied by something that
# is not its subject), and it is a false GREEN, not the safe direction.
#
# Both halves below exited 0 against the fix-4 gate:
#   * the crash-heal proof DELETED, its name surviving only inside a string;
#   * a THIRD DenseReadiness variant added to the real enum, with a decoy string
#     EARLIER in the file carrying a well-formed two-variant enum — `enum_exact`
#     brace-matched the decoy and reported the vocabulary closed.
STRING_DECOY_ROOT="$(make_root definition-inside-a-string-literal)"
python3 - "$STRING_DECOY_ROOT/src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
assert text.count("fn boot_rederive_converges_after_simulated_crash()") == 1
text = text.replace("fn boot_rederive_converges_after_simulated_crash()",
                    "fn boot_rederive_deleted_by_fixture()", 1)
text += """
// Fixture only (fix-4 SWEEP). The proof is deleted. Its name survives ONLY
// inside a string literal, which is data, not a definition.
const FIXTURE_DECOY_FN: &str =
    "fn boot_rederive_converges_after_simulated_crash() { assert!(true); }";
"""
open(p, "w", encoding="utf-8").write(text)
PY
python3 - "$STRING_DECOY_ROOT/src/rust/crates/fathomdb-engine/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
i = text.index("pub enum DenseReadiness {")
j = text.index("\n}", i)
text = text[:j] + "\n    Failed," + text[j:]
# The decoy must sit EARLIER in the file than the real declaration, because the
# structural reader takes the first parseable one it finds.
text = (
    "// Fixture only (fix-4 SWEEP). A well-formed two-variant enum inside a\n"
    "// string literal, ahead of the real three-variant declaration.\n"
    "const FIXTURE_DECOY_ENUM: &str = \"pub enum DenseReadiness { Ready, Embedding }\";\n"
) + text
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$STRING_DECOY_ROOT"
expect_rc 1 "a definition/declaration planted inside a STRING LITERAL does not satisfy a structural probe"
expect_out 'C1-AA-CRASH-HEAL-BOOT-REDERIVE' "the string-decoy failure NAMES the crash-heal clause"
expect_out 'C1-Q4-DENSE-READINESS-TWO-MEMBERS' "the string-decoy failure NAMES the readiness clause"
expect_out 'Failed' "the string-decoy failure NAMES the unpinned variant the real enum carries"
expect_routes_to_steward "the string-decoy clause failures"

# === Arm 12ae (RED, fix-4 SWEEP): A COMMENT IS NOT A DECLARATION EITHER ======
# The other half of 12ad, and the last reachable false green in the sweep. The
# structural readers were taught that a string literal is not code (fix-4b), but
# the remaining `present` probes still read RAW file text — so COMMENTING OUT a
# declaration and putting a replacement beside it left them satisfied. Two of
# those probes are the ONLY probe their clause has for that obligation, which
# makes this reachable rather than merely untidy:
#   * the shipped default embedder NAME (nothing else names that const), and
#   * the composite index the cheap filterable tier reads (the sibling probes are
#     about the TABLE, not the index).
# Both exited 0.
#
# `doc_text` probes are exempt BY CONSTRUCTION — their subject IS a written
# statement, and two of them live in ordinary `//` comments.
COMMENTED_OUT_ROOT="$(make_root declaration-commented-out)"
python3 - "$COMMENTED_OUT_ROOT/src/rust/crates/fathomdb-embedder/src/candle_bge.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = 'pub const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1.5";'
assert text.count(old) == 1
text = text.replace(
    old,
    'pub const SHIPPED_MODEL_ID: &str = "some-other-model";\n'
    '// Fixture only (fix-4 SWEEP). The shipped default is gone; the pinned text\n'
    '// survives only in this comment:\n'
    '// ' + old,
    1,
)
open(p, "w", encoding="utf-8").write(text)
PY
python3 - "$COMMENTED_OUT_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = """CREATE INDEX canonical_attributes_name_value_idx
                  ON canonical_attributes(attr_name, attr_value);"""
assert text.count(old) == 1
text = text.replace(old, "", 1)
text += """
// Fixture only (fix-4 SWEEP). The index is no longer created; the pinned DDL
// survives only in this comment:
// CREATE INDEX canonical_attributes_name_value_idx ON canonical_attributes(attr_name, attr_value);
"""
open(p, "w", encoding="utf-8").write(text)
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$COMMENTED_OUT_ROOT"
expect_rc 1 "a declaration COMMENTED OUT does not satisfy the probe that names it"
expect_out 'C1-TE-DEFAULT-EMBEDDER' "the commented-out failure NAMES the default-embedder clause"
expect_out 'C1-Q2-ENGINE-EAV-PROPERTY-FTS' "the commented-out failure NAMES the EAV clause"
expect_routes_to_steward "the commented-out clause failures"

# The mirror assertion: `doc_text` probes MUST still read comments, or the two
# clauses that carry one would go permanently red for the wrong reason. This is
# the regression half of the arm above.
DOC_TEXT_ROOT="$(make_root doc-text-probe-still-reads-comments)"
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$DOC_TEXT_ROOT"
expect_rc 0 "a doc_text probe still reads ordinary // comments (its subject IS the statement)"
DOC_TEXT_GONE_ROOT="$(make_root doc-text-statement-removed)"
python3 - "$DOC_TEXT_GONE_ROOT/src/rust/crates/fathomdb-schema/src/lib.rs" <<'PY'
import sys
p = sys.argv[1]
text = open(p, encoding="utf-8").read()
old = "recorded in the registry but not honoured here"
assert text.count(old) == 1
open(p, "w", encoding="utf-8").write(text.replace(old, "handled somewhere else now", 1))
PY
run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$DOC_TEXT_GONE_ROOT"
expect_rc 1 "deleting the DOCUMENTED STATEMENT a doc_text probe names HARD-fails its clause"
expect_out 'DOCUMENTED STATEMENT' "the doc_text failure says what kind of probe it is"

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

# TC-37 #4 FOR A TREE-SCOPED PROBE (fix-3). Once a probe's subject is a TREE
# rather than a file, "missing" needs a definition. The gate's answer, stated in
# its header: an ABSENT directory is exit 2, and so is a directory that yields
# ZERO candidate files — a negative assertion that examined nothing has not been
# evaluated, and reporting it as satisfied is the TC-37 vacuous pass in its
# purest form.
#
# HONEST LIMIT OF THIS ARM: every crate tree the gate scans also holds that
# crate's lib.rs, which the same clause's file-scoped `present` probes read. So
# emptying the tree trips BOTH rules and the CLI cannot say which fired first.
# What the arm asserts is the property that matters — NO GREEN IS REACHABLE, and
# the verdict is "gate could not run" (2), never "clause failed" (1).
for TREE in fathomdb-engine fathomdb-schema; do
  EMPTY_TREE_ROOT="$(make_root "empty-tree-$TREE")"
  rm -rf "${EMPTY_TREE_ROOT:?}/src/rust/crates/$TREE/src"
  mkdir -p "$EMPTY_TREE_ROOT/src/rust/crates/$TREE/src"
  run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$EMPTY_TREE_ROOT"
  expect_rc 2 "an EMPTIED $TREE source tree exits 2 (TC-37 #4), never 0 and never 1"
  expect_out 'TC-37' "empty-tree-$TREE cites the evaporation failure class"
  expect_no_out 'ok +c1-contract-conformance' "empty-tree-$TREE prints no ok line"

  GONE_TREE_ROOT="$(make_root "gone-tree-$TREE")"
  rm -rf "${GONE_TREE_ROOT:?}/src/rust/crates/$TREE/src"
  run_checker --contract "$CLEAN_CONTRACT" --pin "$REAL_PIN" --root "$GONE_TREE_ROOT"
  expect_rc 2 "an ABSENT $TREE source tree exits 2 (TC-37 #4), never 0 and never 1"
  expect_no_out 'ok +c1-contract-conformance' "gone-tree-$TREE prints no ok line"
done

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
# fix-3: the crate SOURCE TREES are now first-class subjects, so they must appear
# in the manifest — that manifest is what builds every fixture root here AND the
# preflight seeder in lib/c1-conformance-fixture.sh. A tree-scoped probe whose
# tree is not listed would leave those roots without it and silently evaporate.
expect_out 'tree\s+src/rust/crates/fathomdb-engine/src' \
  "--list-sources names the ENGINE crate source tree the negative probes scan"
expect_out 'tree\s+src/rust/crates/fathomdb-schema/src' \
  "--list-sources names the SCHEMA crate source tree the negative probes scan"

# THE STRUCTURAL GUARD FOR THIS WHOLE CLASS (fix-3). Widening seven probes fixes
# today's holes; deleting the file-scoped negative probe KIND is what stops the
# class recurring. `absent` (single file) no longer exists — a negative probe can
# only be written as `absent_tree`. Assert that, so re-introducing a file-scoped
# negative is a deliberate, reviewed act rather than the path of least effort.
if grep -nE '^\s*\("absent",' "$REPO_ROOT/scripts/check-c1-conformance.sh" >/dev/null; then
  fail "the gate still declares file-scoped (\"absent\", <file>, ...) probes; every negative probe must be tree-scoped (fix-3)"
else
  pass "no file-scoped \`absent\` probe remains: every negative assertion is tree-scoped"
fi
if grep -nE 'kind == "absent"' "$REPO_ROOT/scripts/check-c1-conformance.sh" >/dev/null; then
  fail "the gate still IMPLEMENTS the file-scoped \`absent\` probe kind — removing the kind is what prevents the fix-3 class from recurring"
else
  pass "the file-scoped \`absent\` probe kind is gone from the gate entirely"
fi

# THE SAME STRUCTURAL GUARD FOR THE fix-4 CLASS. `min` was a COUNT of file-wide
# matches — the weakest possible binding between a probe and its subject, and
# exactly what let codex §9 round 4 change `property_search_index`'s tokenizer
# while UNRELATED FTS tables kept the count satisfied. Widening one probe fixes
# today's hole; deleting the KIND is what stops a future editor reaching for a
# count again. A count has no legitimate remaining use in this gate: every clause
# is about a NAMED subject, so the honest probe is a structural extraction bound
# to that subject (or, failing that, a regex tight enough to name it uniquely).
if grep -nE '^\s*\("min",' "$REPO_ROOT/scripts/check-c1-conformance.sh" >/dev/null; then
  fail "the gate still declares (\"min\", <file>, <regex>, n) COUNT probes; a count is not a binding to a subject (fix-4)"
else
  pass "no file-wide COUNT (\`min\`) probe remains: no clause is asserted by counting matches"
fi
if grep -nE 'kind == "min"' "$REPO_ROOT/scripts/check-c1-conformance.sh" >/dev/null; then
  fail "the gate still IMPLEMENTS the \`min\` COUNT probe kind — removing the kind is what prevents the fix-4 class from recurring"
else
  pass "the \`min\` COUNT probe kind is gone from the gate entirely"
fi

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
