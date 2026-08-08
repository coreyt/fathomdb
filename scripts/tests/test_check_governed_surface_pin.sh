#!/usr/bin/env bash
# scripts/tests/test_check_governed_surface_pin.sh — coverage for the governed-
# surface pin gate (scripts/check-governed-surface-pin.sh) AND for its two
# wirings: `preflight.sh --landing` (PREVENT) and the always-on CI job (DETECT).
#
# WHAT IS BEING PROTECTED: the HITL PRE-SIGNED the accumulated governed-surface
# delta of 0.8.20 Slices 5d+10b+15b+15d (AC-079) — pinned to the exact content of
# src/conformance/governed-surface-allowlist.json at the provenance commit
# recorded in the pin (34 allowlist members, 5 core, recovery_denylist unchanged
# at the five REQ-054 names). A pre-sign keyed to specific content is worth
# exactly as much as the mechanism that notices when that content moves.
#
# RED-first: the file MATCHES the pin today, so asserting only against the real
# repo would prove nothing — a `true` script would pass it. Every failure arm
# below therefore runs against a purpose-built DIVERGENT FIXTURE, so each arm can
# only go green because the predicate actually fires. The real-repo arm is the
# regression half of the same pair.
#
# THE FIXTURES ARE COPIES. src/conformance/governed-surface-allowlist.json is
# NEVER written by this suite — mutating it is the exact thing the gate exists to
# catch. Copies live under mktemp -d (the checker takes --file/--pin for exactly
# this reason); the preflight arms build throwaway git repos + linked worktrees.
#
# NOTE ON ARM 5 (whitespace-only): failing on a formatting-only change is a
# DELIBERATE, DOCUMENTED property of a content-hash pin, not an accident — see
# the gate's header. It is asserted here so the behaviour is a contract.
#
# NOTE ON ARMS 8b-8f (the pin's own well-formedness): Arm 8 proves the counts
# block is what catches a lazy re-pin, which makes the counts block itself a
# target. These arms attack it: a pin that DELETES, nulls or mistypes one of its
# three counts must fail as a MALFORMED PIN (exit 2 — the gate could not run),
# never skip that list and never be reported as a surface divergence (exit 1 —
# "go get it signed"). Each was captured exiting 0 on the pre-fix gate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-governed-surface-pin.sh"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"
REAL_FILE="$REPO_ROOT/src/conformance/governed-surface-allowlist.json"
REAL_PIN="$REPO_ROOT/scripts/governed-surface-pin.json"
# Keep the two fragments separate so this control does not make its own source
# match the stale full commit identifier it is meant to prohibit.
STALE_PIN_COMMIT="427d""2712"
# shellcheck source=lib/c1-conformance-fixture.sh
. "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"

# The no-argument arm exercises the checker's REPO-RELATIVE defaults, which it
# resolves from `git rev-parse --show-toplevel` — i.e. from the cwd. Pin the cwd
# to this checkout so that arm tests THIS tree no matter where the suite is
# invoked from (agent-test.sh cd_repo_root's first; a bare `bash scripts/tests/...`
# from a sibling checkout would otherwise silently check the wrong repo).
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

# copy_file <name> -> prints the path of a fresh COPY of the real allowlist
copy_file() {
  local d="$TMPROOT/$1"
  mkdir -p "$d"
  cp "$REAL_FILE" "$d/governed-surface-allowlist.json"
  printf '%s' "$d/governed-surface-allowlist.json"
}

# mutate <path> <python-body>  — edits the COPY in place. `d` is the parsed dict.
mutate() {
  local path="$1" body="$2"
  python3 - "$path" <<PY
import json, sys
p = sys.argv[1]
with open(p) as fh:
    d = json.load(fh)
$body
with open(p, "w") as fh:
    json.dump(d, fh, indent=2)
    fh.write("\n")
PY
}

run_checker() {
  set +e
  OUT="$(bash "$CHECKER" "$@" 2>&1)"
  RC=$?
  set -e
}

# check_fixture <file> [pin]  — always pins explicitly so the arm is independent
# of the cwd the suite happens to run from.
check_fixture() {
  run_checker --file "$1" --pin "${2:-$REAL_PIN}"
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

# The HITL-routing block must appear on EVERY divergence, not just some of them:
# a failure the reader cannot act on is how a gate gets silently re-pinned.
expect_routes_to_hitl() {
  local desc="$1"
  local ok=1
  printf '%s' "$OUT" | grep -q 'this re-opens' || ok=0
  printf '%s' "$OUT" | grep -q 'HITL sign-off' || ok=0
  printf '%s' "$OUT" | grep -q 'DO NOT update the pin to make this pass' || ok=0
  printf '%s' "$OUT" | grep -q 'failure mode this gate exists to' || ok=0
  if [ "$ok" -eq 1 ]; then
    pass "$desc routes the reader to the HITL for a fresh sign-off"
  else
    fail "$desc did not print the full HITL-routing block; got: $OUT"
  fi
}

# ======================= Arm 0: the real, unmodified tree =====================
# Regression half. Also a standing assertion that the pinned artifact itself is
# byte-unmodified — if this suite ever "fixes" a red arm by editing the real
# allowlist, this arm goes red.
run_checker
expect_rc 0 "the real repo's governed surface matches the pin (default args)"
expect_out 'ok +governed-surface-pin' "the passing run says ok"
expect_out '34 allowlist / 5 core / 5 recovery_denylist' \
  "the passing run states the pinned counts it verified"

PIN_SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "$REAL_PIN")"
REAL_SHA="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$REAL_FILE")"
if [ "$PIN_SHA" = "$REAL_SHA" ]; then
  pass "src/conformance/governed-surface-allowlist.json is byte-identical to the pin"
else
  fail "the real allowlist json no longer matches the pin's sha256 ($REAL_SHA vs $PIN_SHA)"
fi

# Slice 22 C5 is an exact signed 34-member shape, not merely a count increase.
# This rejects a stale 32-member pin and a re-pin that swaps an unrelated name
# while retaining the same count. The full list is deliberately explicit here:
# it is the reviewable test oracle for HITL decision steward-ledger seq-247.
set +e
SLICE22_C5_SHAPE="$(python3 - "$REAL_FILE" "$REAL_PIN" <<'PY'
import json
import sys

allowlist_path, pin_path = sys.argv[1:]
with open(allowlist_path, encoding="utf-8") as fh:
    allowlist = json.load(fh)
with open(pin_path, encoding="utf-8") as fh:
    pin = json.load(fh)

expected_allowlist = [
    "Engine.open",
    "admin.configure",
    "write",
    "transition",
    "purge",
    "erase_source",
    "eraseSource",
    "search",
    "search_text_only",
    "searchTextOnly",
    "search_projected_text",
    "searchProjectedText",
    "close",
    "read.get",
    "read.get_many",
    "read.collection",
    "read.mutations",
    "read.list",
    "ingest_with_extractor",
    "ingestWithExtractor",
    "consolidate_with_provider",
    "consolidateWithProvider",
    "graph.neighbors",
    "graph.search_expand",
    "graph.searchExpand",
    "rerank",
    "embed",
    "read.crossed_boundary_since",
    "read.crossedBoundarySince",
    "configure_projections",
    "configureProjections",
    "read.projections",
    "read.projection_status",
    "read.projectionStatus",
]
expected_counts = {"allowlist": 34, "core": 5, "recovery_denylist": 5}
expected_denylist = ["recover", "restore", "repair", "fix", "rebuild"]

problems = []
if allowlist.get("allowlist") != expected_allowlist:
    problems.append("allowlist is not the exact signed 34-member Slice 22 C5 shape")
if pin.get("allowlist") != expected_allowlist:
    problems.append("pin allowlist is not the exact signed 34-member Slice 22 C5 shape")
if pin.get("counts") != expected_counts:
    problems.append(f"pin counts are {pin.get('counts')!r}, not {expected_counts!r}")
if allowlist.get("recovery_denylist") != expected_denylist:
    problems.append("allowlist recovery denylist changed")
if pin.get("recovery_denylist") != expected_denylist:
    problems.append("pin recovery denylist changed")
if "seq-247" not in pin.get("ac", "") or "seq-247" not in "\n".join(pin.get("_comment", [])):
    problems.append("pin lacks transparent HITL steward-ledger seq-247 provenance")

if problems:
    raise SystemExit("; ".join(problems))
print("exact Slice 22 C5 34-member shape and seq-247 provenance")
PY
)"
SLICE22_C5_SHAPE_RC=$?
set -e
if [ "$SLICE22_C5_SHAPE_RC" -eq 0 ]; then
  pass "$SLICE22_C5_SHAPE"
else
  fail "the pin must retain the exact signed Slice 22 C5 34-member shape and seq-247 provenance: $SLICE22_C5_SHAPE"
fi

# The pin must really describe the allowlist at its own provenance commit. Reading
# that commit from the pin keeps this arm valid after a future authorized re-pin;
# hardcoding a historical commit makes a correct pin look stale.
# Skipped (loudly) on a shallow checkout where the recorded commit is unreachable
# — the hash arms above still carry the assertion, so this is not a vacuous pass.
PIN_BLOB="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["git_blob_sha1"])' "$REAL_PIN")"
PIN_COMMIT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pinned_at_commit"])' "$REAL_PIN")"
PIN_COMMIT_SHORT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pinned_at_commit_short"])' "$REAL_PIN")"
if git -C "$REPO_ROOT" cat-file -e "${PIN_COMMIT}^{commit}" 2>/dev/null; then
  AT_PIN="$(git -C "$REPO_ROOT" rev-parse "${PIN_COMMIT}:src/conformance/governed-surface-allowlist.json")"
  if [ "$AT_PIN" = "$PIN_BLOB" ]; then
    pass "the pin's git_blob_sha1 is exactly ${PIN_COMMIT_SHORT}'s blob for the allowlist"
  else
    fail "pin git_blob_sha1 $PIN_BLOB != ${PIN_COMMIT_SHORT}'s blob $AT_PIN — the provenance claim is false"
  fi
else
  printf 'SKIP  %s unreachable (shallow checkout) — provenance arm not run\n' "$PIN_COMMIT_SHORT"
fi

# B7 keeps the pin's documented provenance aligned with the actual re-issued
# pin. A source-level check is deliberate: the pin checker can still return 0
# while a stale comment or failure message directs a maintainer to the wrong
# commit. This scan includes this test; STALE_PIN_COMMIT is split above so the
# control cannot pass merely by excluding itself.
set +e
STALE_PIN_REFS="$(grep -R -n -F "$STALE_PIN_COMMIT" "$REPO_ROOT/scripts")"
STALE_PIN_REFS_RC=$?
set -e
case "$STALE_PIN_REFS_RC" in
  1) pass "no script retains the superseded governed-surface pin provenance" ;;
  0) fail "scripts retain superseded governed-surface pin provenance: $STALE_PIN_REFS" ;;
  *) fail "could not scan scripts for superseded governed-surface pin provenance (grep rc=$STALE_PIN_REFS_RC)" ;;
esac

# =================== Arm 7 (ordered early): unmodified COPY ===================
# A byte-identical copy at a different path must pass, proving the gate compares
# CONTENT and is not keyed to the path or to any repo state.
F="$(copy_file unmodified)"
check_fixture "$F"
expect_rc 0 "an unmodified COPY of the allowlist passes"

# ======================= Arm 1 (RED): added member (35) =======================
F="$(copy_file added-member)"
mutate "$F" 'd["allowlist"].append("shiny.new_verb")'
check_fixture "$F"
expect_rc 1 "an ADDED allowlist member (35) HARD-fails"
expect_out "'allowlist' diverges from the pin" "added-member names the diverging key"
expect_out 'ADDED shiny.new_verb' "added-member NAMES the member that appeared"
expect_out 'Pinned 34 member\(s\), on disk 35' "added-member states pinned-vs-on-disk counts"
expect_out 'content differs from the pin' "added-member reports the content-hash divergence too"
expect_routes_to_hitl "added-member"

# ====================== Arm 2 (RED): removed member (33) ======================
F="$(copy_file removed-member)"
mutate "$F" 'd["allowlist"].remove("purge")'
check_fixture "$F"
expect_rc 1 "a REMOVED allowlist member (33) HARD-fails"
expect_out 'REMOVED purge' "removed-member NAMES the member that vanished"
expect_out 'Pinned 34 member\(s\), on disk 33' "removed-member states pinned-vs-on-disk counts"
expect_routes_to_hitl "removed-member"

# =================== Arm 3 (RED): recovery_denylist widened ===================
# AC-041 / REQ-054: the recovery denylist is five names. Widening it to six must
# fail on the DENYLIST rule by name, not merely as "some list changed".
F="$(copy_file denylist-widened)"
mutate "$F" 'd["recovery_denylist"].append("reset")'
check_fixture "$F"
expect_rc 1 "a recovery_denylist widened to SIX HARD-fails"
expect_out 'REQ-054' "denylist-widened cites the REQ-054 rule by name"
expect_out 'recovery denylist is five names' "denylist-widened states the five-names rule"
expect_out 'WIDENED by reset' "denylist-widened NAMES the added denylist entry"
expect_routes_to_hitl "denylist-widened"

# ================= Arm 4 (RED): recovery_denylist name changed ================
# Still FIVE names, so every count assertion passes — only an exact-membership
# check can catch this. That is precisely why counts are secondary, not the test.
F="$(copy_file denylist-renamed)"
mutate "$F" 'd["recovery_denylist"] = ["recover", "restore", "repair", "mend", "rebuild"]'
check_fixture "$F"
expect_rc 1 "a RENAMED recovery_denylist entry (still five) HARD-fails"
expect_out 'REQ-054' "denylist-renamed cites the REQ-054 rule by name"
expect_out 'WIDENED by mend' "denylist-renamed names the substituted entry"
expect_out 'DROPPED fix' "denylist-renamed names the REQ-054 entry that went missing"
expect_routes_to_hitl "denylist-renamed"

# =================== Arm 5 (RED): whitespace / formatting only ================
# Documented behaviour of a CONTENT-hash pin. The failure must ALSO say that the
# parsed members are identical, so the reader is not left guessing.
F="$(copy_file whitespace-only)"
printf '\n' >>"$F"
check_fixture "$F"
expect_rc 1 "a whitespace/formatting-only change HARD-fails (content-hash pin, by design)"
expect_out 'content differs from the pin' "whitespace-only reports a content-hash divergence"
expect_out 'formatting/whitespace-only change' "whitespace-only says the change is formatting-only"
expect_out 'CONTENT hash' "whitespace-only explains that the pin is a content hash (deliberate)"
if printf '%s' "$OUT" | grep -qE 'ADDED|REMOVED|diverges from the pin'; then
  fail "whitespace-only must NOT report a member divergence; got: $OUT"
else
  pass "whitespace-only reports no member divergence (members are genuinely unchanged)"
fi
expect_routes_to_hitl "whitespace-only"

# ============= Arm 6 (RED): file missing — TC-37 vacuous-pass guard ===========
# A gate that cannot see its subject and reports green is an active false
# assurance. Missing/unreadable must be LOUD and must never exit 0.
check_fixture "$TMPROOT/does-not-exist/governed-surface-allowlist.json"
expect_rc 1 "a MISSING allowlist file HARD-fails (TC-37 vacuous-pass guard)"
expect_out 'cannot read' "file-missing says it could not read the file"
expect_out 'TC-37' "file-missing cites the vacuous-pass failure class"
expect_out 'largest possible change to the governed surface' \
  "file-missing explains why a vanished allowlist is itself a surface change"

D="$TMPROOT/unreadable"
mkdir -p "$D"
cp "$REAL_FILE" "$D/governed-surface-allowlist.json"
chmod 000 "$D/governed-surface-allowlist.json"
if [ -r "$D/governed-surface-allowlist.json" ]; then
  # root ignores the mode bits; the missing-file arm above already carries TC-37.
  printf 'SKIP  running as root — chmod 000 is not enforced, unreadable arm not run\n'
else
  check_fixture "$D/governed-surface-allowlist.json"
  expect_rc 1 "an UNREADABLE allowlist file HARD-fails (TC-37 vacuous-pass guard)"
fi
chmod 644 "$D/governed-surface-allowlist.json"

# ================= Arm 8 (RED): the LAZY RE-PIN — hash-only update ============
# The failure mode with teeth: a member is added to the surface and the pin's
# hashes are updated to match, but the signed member lists/counts are left alone.
# The gate must still fail, because the member lists are compared independently.
F="$(copy_file lazy-repin)"
mutate "$F" 'd["allowlist"].append("smuggled.verb")'
LAZY_PIN="$TMPROOT/lazy-repin/pin.json"
python3 - "$REAL_PIN" "$F" "$LAZY_PIN" <<'PY'
import hashlib, json, sys
pin = json.load(open(sys.argv[1]))
raw = open(sys.argv[2], "rb").read()
pin["sha256"] = hashlib.sha256(raw).hexdigest()
pin["git_blob_sha1"] = hashlib.sha1(b"blob %d\0" % len(raw) + raw).hexdigest()
json.dump(pin, open(sys.argv[3], "w"), indent=2)
PY
check_fixture "$F" "$LAZY_PIN"
expect_rc 1 "a LAZY RE-PIN (hashes updated, signed member list untouched) still HARD-fails"
expect_out 'ADDED smuggled.verb' "lazy-repin names the smuggled member"
expect_out 'counts block says 34' "lazy-repin is also caught by the counts assertion"
if printf '%s' "$OUT" | grep -q 'content differs from the pin'; then
  fail "lazy-repin's hashes DO match by construction; a hash complaint means the arm is not testing what it claims: $OUT"
else
  pass "lazy-repin's hash check genuinely passes — only the member/count checks carry this arm"
fi
expect_routes_to_hitl "lazy-repin"

# ========= Arm 8b (RED): a pin that DELETES one of its own counts ============
# Arm 8 proved the counts block is what catches a lazy re-pin. So the counts
# block is itself worth attacking: reading counts.<list> permissively meant an
# ABSENT entry came back as None and BOTH the internal-consistency check and the
# file-count check silently skipped that list. Deleting a count therefore
# disarmed the backstop — the gate would have kept reporting "ok" while checking
# strictly less than it claimed. Captured on the pre-fix script, all three of
# these exited 0. A count the gate cannot read is a MALFORMED PIN (exit 2, "the
# gate could not run"), never a divergence (exit 1, "go get the surface signed").
#
# omit_count_pin <key> -> prints the path of a pin with counts.<key> deleted
omit_count_pin() {
  local key="$1" out="$TMPROOT/pin-omit-$1.json"
  python3 - "$REAL_PIN" "$out" "$key" <<'PY'
import json, sys
pin = json.load(open(sys.argv[1]))
del pin["counts"][sys.argv[3]]
json.dump(pin, open(sys.argv[2], "w"), indent=2)
PY
  printf '%s' "$out"
}

for KEY in allowlist core recovery_denylist; do
  check_fixture "$REAL_FILE" "$(omit_count_pin "$KEY")"
  expect_rc 2 "a pin that OMITS counts.$KEY HARD-fails as malformed (was a silent exit 0)"
  expect_out "'counts' has no '$KEY' entry" "omit-counts.$KEY names the missing count entry"
  expect_out 'MALFORMED' "omit-counts.$KEY says the pin is malformed"
  expect_out "DO NOT 'fix' this by regenerating the pin" \
    "omit-counts.$KEY forbids regenerating the pin to clear it"
  if printf '%s' "$OUT" | grep -qE 'ok[[:space:]]+governed-surface-pin'; then
    fail "omit-counts.$KEY must never print an ok line; got: $OUT"
  else
    pass "omit-counts.$KEY prints no ok line (no vacuous pass)"
  fi
done

# ===== Arm 8c (RED): counts entry present but not an integer =================
# Same hole through a different door: null skipped the check exactly like an
# absent key (pre-fix exit 0), a float 30.0 compared EQUAL to 30 and also passed
# (pre-fix exit 0), and a string "30" tripped the consistency check with a
# nonsense message ("counts.allowlist says 30 but its list holds 30"). All three
# are a malformed pin, and all three must now say so.
type_count_pin() {
  local label="$1" literal="$2"
  local out="$TMPROOT/pin-type-$label.json"
  python3 - "$REAL_PIN" "$out" "$literal" <<'PY'
import json, sys
pin = json.load(open(sys.argv[1]))
pin["counts"]["allowlist"] = json.loads(sys.argv[3])
json.dump(pin, open(sys.argv[2], "w"), indent=2)
PY
  printf '%s' "$out"
}

check_fixture "$REAL_FILE" "$(type_count_pin null 'null')"
expect_rc 2 "a NULL counts.allowlist HARD-fails as malformed (was a silent exit 0)"
expect_out 'not an integer' "null-count says the count is not an integer"

check_fixture "$REAL_FILE" "$(type_count_pin string '"30"')"
expect_rc 2 "a STRING counts.allowlist HARD-fails as malformed (was a confusing exit 1)"
expect_out 'not an integer' "string-count says the count is not an integer"

check_fixture "$REAL_FILE" "$(type_count_pin float '30.0')"
expect_rc 2 "a FLOAT counts.allowlist HARD-fails as malformed (30.0 == 30 used to pass)"
expect_out 'not an integer' "float-count says the count is not an integer"

check_fixture "$REAL_FILE" "$(type_count_pin bool 'true')"
expect_rc 2 "a BOOLEAN counts.allowlist HARD-fails (isinstance(True, int) is True in Python)"
expect_out 'not an integer' "bool-count says the count is not an integer"

# ==== Arm 8d (RED): THE KILLER — drop the count, add a member, rehash ========
# The full defeat of the backstop, and the reason 8b is P2 and not cosmetic: a
# re-pin that adds a member to the surface, updates the signed member list AND
# the hashes to match, and DELETES counts.allowlist. Every surviving check then
# agreed with the pin, and the one check that still knew the signed size was 34
# had been quietly switched off. Pre-fix this exited 0 and printed
#   "ok ... (None allowlist / 5 core / 5 recovery_denylist)"
# for a 35-member surface carrying smuggled.verb.
F="$(copy_file killer-drop-count)"
mutate "$F" 'd["allowlist"].append("smuggled.verb")'
KILLER_PIN="$TMPROOT/killer-drop-count/pin.json"
python3 - "$REAL_PIN" "$F" "$KILLER_PIN" <<'PY'
import hashlib, json, sys
pin = json.load(open(sys.argv[1]))
raw = open(sys.argv[2], "rb").read()
pin["allowlist"] = json.loads(raw.decode("utf-8"))["allowlist"]  # signed list updated
pin["sha256"] = hashlib.sha256(raw).hexdigest()                  # hashes recomputed
pin["git_blob_sha1"] = hashlib.sha1(b"blob %d\0" % len(raw) + raw).hexdigest()
del pin["counts"]["allowlist"]                                   # backstop DELETED
json.dump(pin, open(sys.argv[3], "w"), indent=2)
PY
check_fixture "$F" "$KILLER_PIN"
expect_rc 2 "a re-pin that DROPS counts.allowlist, adds a member and rehashes HARD-fails (was exit 0)"
expect_out 'MALFORMED' "killer-drop-count reports a malformed pin"
if printf '%s' "$OUT" | grep -qE 'ok +governed-surface-pin|None allowlist'; then
  fail "killer-drop-count must never report ok / a None count; got: $OUT"
else
  pass "killer-drop-count prints no ok line and no None count"
fi
# Guard that this arm keeps testing what it claims: the hashes and the member
# list DO agree by construction, so only the missing-count rule can carry it.
if printf '%s' "$OUT" | grep -qE 'content differs from the pin|ADDED smuggled.verb'; then
  fail "killer-drop-count's hashes and member list match by construction; a hash/member complaint means the arm is not testing the count rule: $OUT"
else
  pass "killer-drop-count's hash and member checks genuinely pass — only the missing-count rule carries it"
fi

# ==== Arm 8e: a malformed pin is a BROKEN GATE (2), not a divergence (1) =====
# The distinction is load-bearing for the reader: exit 1 says "the surface moved,
# take it to the HITL", and printing that for a pin the gate cannot parse would
# invite exactly the re-pin this gate exists to prevent. It also matches what the
# gate already does for a missing pin (Arm 10).
check_fixture "$REAL_FILE" "$(omit_count_pin allowlist)"
if printf '%s' "$OUT" | grep -q 'DO NOT update the pin to make this pass'; then
  fail "a malformed pin must NOT print the surface-divergence HITL-routing block; got: $OUT"
else
  pass "a malformed pin does not print the surface-divergence routing block (it is a broken gate, not a changed surface)"
fi

# ==== Arm 8f: a non-string hash field in the pin is malformed too ============
# Same class as 8b, found by auditing the rest of the pin-parsing path: the
# predicate READS sha256/git_blob_sha1, so a mistyped one is a broken gate. It
# used to be reported as a content divergence (exit 1), sending the reader to the
# HITL to re-sign a surface that never moved.
BAD_HASH_PIN="$TMPROOT/pin-bad-hash.json"
python3 - "$REAL_PIN" "$BAD_HASH_PIN" <<'PY'
import json, sys
pin = json.load(open(sys.argv[1]))
pin["sha256"] = None
json.dump(pin, open(sys.argv[2], "w"), indent=2)
PY
check_fixture "$REAL_FILE" "$BAD_HASH_PIN"
expect_rc 2 "a NULL sha256 in the pin HARD-fails as malformed, not as a divergence"
expect_out 'not a non-empty string' "bad-hash pin says the hash field is not a string"

# ============ Arm 9 (RED): a re-pin that widens the denylist in the PIN =======
# REQ-054 is checked against a constant hardcoded in the gate, in the PIN as well
# as in the file, so this is the one rule a re-pin cannot buy its way past.
WIDE_PIN="$TMPROOT/wide-pin.json"
python3 - "$REAL_PIN" "$WIDE_PIN" <<'PY'
import json, sys
pin = json.load(open(sys.argv[1]))
pin["recovery_denylist"].append("reset")
pin["counts"]["recovery_denylist"] = 6
json.dump(pin, open(sys.argv[2], "w"), indent=2)
PY
check_fixture "$REAL_FILE" "$WIDE_PIN"
expect_rc 1 "a PIN that itself widens recovery_denylist HARD-fails (REQ-054 is not re-pinnable)"
expect_out 'itself declares recovery_denylist' "wide-pin failure points at the pin, not the file"
expect_out 'not something a re-pin can change' "wide-pin says REQ-054 cannot be re-pinned"

# ==================== Arm 10: usage / environment errors = 2 ==================
run_checker --not-a-flag
expect_rc 2 "an unknown flag exits 2 (usage), distinct from a divergence"

check_fixture "$REAL_FILE" "$TMPROOT/no-such-pin.json"
expect_rc 2 "a missing PIN exits 2 (the gate could not run) and never 0"
expect_out 'the gate cannot run' "missing-pin says the gate could not run"

run_checker --help
expect_rc 0 "--help exits 0"
expect_out 'Usage: scripts/check-governed-surface-pin.sh' "--help prints usage"

# ======================== preflight.sh --landing wiring =======================
# These arms prove the PREVENT wiring. Pre-wiring they are the RED witness for
# the gap: a tree carrying an unsigned governed surface cleared --landing with 0.

NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# make_repo <primary> <linked> — a throwaway repo carrying COPIES of the two
# governed-surface files plus a consistent ledger (so preflight's §8 passes and
# only §9 is under test), plus a linked worktree (TC-RUBRIC-5 forbids --landing
# in a primary checkout).
#
# It also seeds preflight's §10 subject (the C-1 contract + its pin + the sources
# that gate reads) via lib/c1-conformance-fixture.sh. Incidental to THIS suite —
# §10 hard-fails any tree whose subject it cannot see, so without it every
# `--landing` arm below would fail for a reason that has nothing to do with the
# governed surface. Same repair §8 and §9 each needed in turn.
make_repo() {
  local primary="$1" linked="$2"
  mkdir -p "$primary/src/conformance" "$primary/scripts" "$primary/dev/steward"
  git init -q -b main "$primary"
  git -C "$primary" config user.email surface-test@example.invalid
  git -C "$primary" config user.name 'Surface Test'
  git -C "$primary" config commit.gpgsign false
  git -C "$primary" config core.hooksPath "$NO_HOOKS"
  cp "$REAL_FILE" "$primary/src/conformance/governed-surface-allowlist.json"
  cp "$REAL_PIN" "$primary/scripts/governed-surface-pin.json"
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
mutate "$DIVERGED_LINKED/src/conformance/governed-surface-allowlist.json" \
  'd["allowlist"].append("unsigned.verb")'

run_preflight "$DIVERGED_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing HARD-fails in a worktree whose governed surface diverges from the pin"
else
  fail "--landing MUST fail on an unsigned governed surface; out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'HARD.*governed-surface-pin:'; then
  pass "--landing failure output names the governed-surface-pin check"
else
  fail "expected a HARD line naming governed-surface-pin; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'unsigned.verb'; then
  pass "--landing failure carries the specific member through to the operator"
else
  fail "expected the HARD line to name unsigned.verb; got: $OUT"
fi

run_preflight "$CLEAN_LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing still exits 0 in a worktree whose governed surface matches the pin"
else
  fail "--landing must not regress a pinned-clean tree; got rc=$RC, out: $OUT"
fi

# Mirrors §7/§8's contract: --landing-only, so plain preflight stays lean.
run_preflight "$DIVERGED_LINKED"
if printf '%s' "$OUT" | grep -q 'governed-surface-pin:'; then
  fail "governed-surface-pin must be --landing-only; it ran without --landing: $OUT"
else
  pass "regression guard: governed-surface-pin is inert without --landing"
fi

# ========================== CI wiring is ALWAYS-ON ============================
# A docs_only-gated job never fires on a code push, and a governed-surface change
# is BY DEFINITION a code change — so a docs_only gate would make this job absent
# on exactly the pushes it exists to catch. Assert statically that the job exists,
# runs the SHARED script, and carries no `if:` condition at all.
CI_JOB_BLOCK="$(awk '
  /^  governed-surface-pin:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a governed-surface-pin job"
else
  fail "ci.yml has no governed-surface-pin job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/check-governed-surface-pin.sh'; then
  pass "the CI job runs the SHARED scripts/check-governed-surface-pin.sh (one predicate, two callers)"
else
  fail "the CI job must invoke scripts/check-governed-surface-pin.sh, not a reimplementation"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*if:'; then
  fail "the governed-surface-pin job must be ALWAYS-ON (no if:/docs_only gate); block: $CI_JOB_BLOCK"
else
  pass "the governed-surface-pin job is always-on (no if: condition, not docs_only-gated)"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*needs:'; then
  fail "the governed-surface-pin job must not depend on the changes job; block: $CI_JOB_BLOCK"
else
  pass "the governed-surface-pin job has no needs: (does not ride the changes/docs_only fast path)"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-governed-surface-pin tests passed\n'
