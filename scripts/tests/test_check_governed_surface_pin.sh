#!/usr/bin/env bash
# scripts/tests/test_check_governed_surface_pin.sh — coverage for the governed-
# surface pin gate (scripts/check-governed-surface-pin.sh) AND for its two
# wirings: `preflight.sh --landing` (PREVENT) and the always-on CI job (DETECT).
#
# WHAT IS BEING PROTECTED: the HITL PRE-SIGNED the accumulated governed-surface
# delta of 0.8.20 Slices 5d+10b+15b+15d (AC-079) — pinned to the exact content of
# src/conformance/governed-surface-allowlist.json as of commit 427d2712 (30
# allowlist members, 5 core, recovery_denylist unchanged at the five REQ-054
# names). A pre-sign keyed to specific content is worth exactly as much as the
# mechanism that notices when that content moves.
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
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-governed-surface-pin.sh"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"
REAL_FILE="$REPO_ROOT/src/conformance/governed-surface-allowlist.json"
REAL_PIN="$REPO_ROOT/scripts/governed-surface-pin.json"

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
expect_out '30 allowlist / 5 core / 5 recovery_denylist' \
  "the passing run states the pinned counts it verified"

PIN_SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "$REAL_PIN")"
REAL_SHA="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$REAL_FILE")"
if [ "$PIN_SHA" = "$REAL_SHA" ]; then
  pass "src/conformance/governed-surface-allowlist.json is byte-identical to the pin"
else
  fail "the real allowlist json no longer matches the pin's sha256 ($REAL_SHA vs $PIN_SHA)"
fi

# The pin must really be 427d2712's content, which is the whole provenance claim.
# Skipped (loudly) on a shallow checkout where that commit is unreachable — the
# hash arms above still carry the assertion, so this is not a vacuous pass.
PIN_BLOB="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["git_blob_sha1"])' "$REAL_PIN")"
if git -C "$REPO_ROOT" cat-file -e '427d2712^{commit}' 2>/dev/null; then
  AT_PIN="$(git -C "$REPO_ROOT" rev-parse '427d2712:src/conformance/governed-surface-allowlist.json')"
  if [ "$AT_PIN" = "$PIN_BLOB" ]; then
    pass "the pin's git_blob_sha1 is exactly 427d2712's blob for the allowlist"
  else
    fail "pin git_blob_sha1 $PIN_BLOB != 427d2712's blob $AT_PIN — the provenance claim is false"
  fi
else
  printf 'SKIP  427d2712 unreachable (shallow checkout) — provenance arm not run\n'
fi

# =================== Arm 7 (ordered early): unmodified COPY ===================
# A byte-identical copy at a different path must pass, proving the gate compares
# CONTENT and is not keyed to the path or to any repo state.
F="$(copy_file unmodified)"
check_fixture "$F"
expect_rc 0 "an unmodified COPY of the allowlist passes"

# ======================= Arm 1 (RED): added member (31) =======================
F="$(copy_file added-member)"
mutate "$F" 'd["allowlist"].append("shiny.new_verb")'
check_fixture "$F"
expect_rc 1 "an ADDED allowlist member (31) HARD-fails"
expect_out "'allowlist' diverges from the pin" "added-member names the diverging key"
expect_out 'ADDED shiny.new_verb' "added-member NAMES the member that appeared"
expect_out 'Pinned 30 member\(s\), on disk 31' "added-member states pinned-vs-on-disk counts"
expect_out 'content differs from the pin' "added-member reports the content-hash divergence too"
expect_routes_to_hitl "added-member"

# ====================== Arm 2 (RED): removed member (29) ======================
F="$(copy_file removed-member)"
mutate "$F" 'd["allowlist"].remove("purge")'
check_fixture "$F"
expect_rc 1 "a REMOVED allowlist member (29) HARD-fails"
expect_out 'REMOVED purge' "removed-member NAMES the member that vanished"
expect_out 'Pinned 30 member\(s\), on disk 29' "removed-member states pinned-vs-on-disk counts"
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
expect_out 'counts block says 30' "lazy-repin is also caught by the counts assertion"
if printf '%s' "$OUT" | grep -q 'content differs from the pin'; then
  fail "lazy-repin's hashes DO match by construction; a hash complaint means the arm is not testing what it claims: $OUT"
else
  pass "lazy-repin's hash check genuinely passes — only the member/count checks carry this arm"
fi
expect_routes_to_hitl "lazy-repin"

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
