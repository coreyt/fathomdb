#!/usr/bin/env bash
# scripts/tests/test_preflight_landing.sh — coverage for `preflight.sh --landing`.
#
# TC-RUBRIC-5 (HITL-ADOPTED 2026-07-11) requires that release orchestration and
# ALL landing git-writes run in a dedicated linked worktree, never in the primary
# checkout. `--landing` is the mechanical enforcement of that rule, per the
# standing guardrail-failures-fix-tooling-not-people principle.
#
# Detection under test: primary checkout <=> `git rev-parse --git-dir` equals
# `git rev-parse --git-common-dir`, compared AFTER resolving both to absolute,
# symlink-resolved paths.
#
# On the canonicalization: on git 2.43 invoked from a SUBDIRECTORY, --git-dir
# returns an absolute path while --git-common-dir returns a relative one
# ('../.git'). preflight.sh cds to the repo toplevel before the check, which
# normalizes that away today — so a raw string compare currently behaves the same
# and the subdir arms below pass either way. The canonicalization still matters:
# measured, with that toplevel cd removed, a RAW compare from a subdirectory of
# the primary checkout fails OPEN (exit 0, "cleared for landing") while the
# canonicalized compare still HARD-fails. Since no behavioral arm can distinguish
# the two while the cd stands, Arm 7 asserts the canonicalization structurally —
# same idiom as the 'User-Agent:' grep in test_assert_co_tagging.sh.
#
# Isolation: every arm runs against a throwaway repo built under mktemp -d. The
# test never git-writes into the real checkout and does not care whether the
# developer's tree is clean.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
# shellcheck source=lib/governed-surface-fixture.sh
. "$SCRIPT_DIR/lib/governed-surface-fixture.sh"
# shellcheck source=lib/c1-conformance-fixture.sh
. "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  # Only ever remove a path we created under the system temp dir.
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

# --- Fixture: a throwaway primary checkout + a linked worktree inside it -------
# preflight.sh does `git rev-parse main` under `set -euo pipefail`, so the fixture
# repo must actually have a `main` branch with a commit.
#
# The fixture must not inherit the developer's/CI image's global git config: a
# global `commit.gpgsign = true` makes the fixture commit try to GPG-sign with the
# throwaway identity and abort (measured: exit 128, "failed to write commit
# object"), and a global `core.hooksPath` — or an `init.templateDir` that seeds
# .git/hooks — runs a foreign pre-commit hook that can veto it (measured: exit 1).
# Either aborts the script before a single assertion runs, so an unrelated
# user-local setting would fail agent-test.sh. All three are neutralized in the
# fixture repo's LOCAL config; the real checkout's config is never touched.
# Arm 8 is the regression guard. A linked worktree shares the common config file,
# so the local settings cover it too — no separate treatment needed.
NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# make_fixture <primary-dir> <linked-dir> — builds a self-contained repo + worktree.
make_fixture() {
  local primary="$1" linked="$2"
  mkdir -p "$primary"
  git init -q -b main "$primary"
  git -C "$primary" config user.email preflight-test@example.invalid
  git -C "$primary" config user.name 'Preflight Test'
  git -C "$primary" config commit.gpgsign false
  git -C "$primary" config core.hooksPath "$NO_HOOKS"
  mkdir -p "$primary/src" "$primary/scripts" "$primary/dev/steward"
  printf 'fixture\n' >"$primary/src/keep.txt"
  # A minimal, CONSISTENT ledger + sidecar. preflight.sh --landing now also runs
  # the ledger-integrity gate (DOC-HYGIENE-2 T1b), whose TC-37 vacuous-pass
  # guard HARD-fails a tree in which it discovers zero ledgers — a repo with no
  # ledger at all cannot be vouched for. This fixture models a real checkout, so
  # it carries one; its own corruption arms live in test_check_ledgers.sh.
  printf '{"seq":1,"note":"fixture"}\n' >"$primary/dev/steward/steward-ledger.jsonl"
  printf '%s' 1 >"$primary/dev/steward/steward-ledger.jsonl.seq"
  # And a minimal, self-consistent governed surface + pin: `--landing` also runs
  # the governed-surface pin gate (DOC-HYGIENE-2 T1e), which HARD-fails a tree
  # whose pin it cannot read — the same TC-37 stance as the ledger gate above, and
  # equally correct. This fixture models a real checkout, so it carries one; the
  # pin's own arms live in scripts/tests/test_check_governed_surface_pin.sh.
  seed_governed_surface_fixture "$primary"
  seed_c1_conformance_fixture "$primary"
  git -C "$primary" add -A
  git -C "$primary" commit -q -m 'fixture: initial commit'
  git -C "$primary" worktree add -q -b landing-fixture "$linked" >/dev/null 2>&1
}

PRIMARY="$TMPROOT/primary"
LINKED="$TMPROOT/linked"
make_fixture "$PRIMARY" "$LINKED"

# run_preflight <cwd> [args...] -> sets RC and OUT (stdout+stderr merged)
run_preflight() {
  local cwd="$1"; shift
  set +e
  OUT="$(cd "$cwd" && bash "$PREFLIGHT" "$@" 2>&1)"
  RC=$?
  set -e
}

# --- Arm 1: --landing in a linked worktree => exit 0, JSON reports pass --------
run_preflight "$LINKED" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing in a linked worktree exits 0"
else
  fail "--landing in a linked worktree should exit 0; got rc=$RC, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '"preflight":"pass"'; then
  pass "--landing in a linked worktree reports \"preflight\":\"pass\""
else
  fail "expected \"preflight\":\"pass\" in summary; got: $OUT"
fi
# ANTI-VACUITY: a "pass" must mean every --landing gate ran and cleared, not that
# the fixture failed to carry a gate's input and the gate therefore said nothing.
# These are the two gates that HARD-fail a tree whose input they cannot read.
for gate_line in 'ledger-integrity:' 'governed-surface-pin:'; do
  if printf '%s' "$OUT" | grep -qE "^ok +${gate_line}"; then
    pass "the passing --landing run really executed the $gate_line gate (ok, not skipped)"
  else
    fail "$gate_line did not report ok in a passing --landing run; out: $OUT"
  fi
done

# --- Arm 2: --landing in the primary checkout => HARD fail, non-zero ----------
run_preflight "$PRIMARY" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing in the primary checkout exits non-zero"
else
  fail "--landing in the primary checkout MUST fail; got rc=0, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q '^HARD .*TC-RUBRIC-5'; then
  pass "--landing in the primary checkout emits a HARD line naming TC-RUBRIC-5"
else
  fail "expected a HARD line naming TC-RUBRIC-5; got: $OUT"
fi
if printf '%s' "$OUT" | grep -q '"preflight":"fail"'; then
  pass "--landing in the primary checkout reports \"preflight\":\"fail\""
else
  fail "expected \"preflight\":\"fail\" in summary; got: $OUT"
fi

# --- Arm 3: regression guard — no --landing, primary checkout still passes -----
run_preflight "$PRIMARY"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: primary checkout without --landing still exits 0"
else
  fail "plain invocation on the primary checkout must still pass; got rc=$RC, out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'TC-RUBRIC-5'; then
  fail "plain invocation must not mention TC-RUBRIC-5; got: $OUT"
else
  pass "regression guard: plain invocation says nothing about TC-RUBRIC-5"
fi

# --- Arm 4: invoked from a SUBDIRECTORY of each tree --------------------------
# Guards the cwd-independence of the check as a whole (the toplevel cd plus the
# canonicalization). See the header for why this arm cannot, on its own,
# distinguish a raw compare from a canonicalized one.
run_preflight "$PRIMARY/scripts" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing from a subdir of the primary checkout still fails (canonicalized)"
else
  fail "subdir of primary MUST still fail (relative vs absolute git-dir); got rc=0, out: $OUT"
fi

mkdir -p "$LINKED/scripts"
run_preflight "$LINKED/scripts" --landing
if [ "$RC" -eq 0 ]; then
  pass "--landing from a subdir of a linked worktree still passes"
else
  fail "subdir of a linked worktree should pass; got rc=$RC, out: $OUT"
fi

# --- Arm 5: --landing composes with the existing flags ------------------------
run_preflight "$LINKED" --landing --worktree "$LINKED" --min-disk-gb 1
if [ "$RC" -eq 0 ]; then
  pass "--landing composes with --worktree/--min-disk-gb in a linked worktree"
else
  fail "--landing + --worktree should pass in a linked worktree; got rc=$RC, out: $OUT"
fi

run_preflight "$PRIMARY" --worktree "$LINKED" --landing --min-disk-gb 1
if [ "$RC" -ne 0 ]; then
  pass "--landing composes with --worktree and still fails on the primary checkout"
else
  fail "--landing must fail on primary even alongside --worktree; got rc=0, out: $OUT"
fi

# --- Arm 6: usage header documents --landing ----------------------------------
if grep -q -- '--landing' "$PREFLIGHT"; then
  pass "preflight.sh documents/handles --landing"
else
  fail "preflight.sh has no --landing handling"
fi

# --- Arm 7: structural — the landing compare is canonicalized, not raw --------
# Regression guard for a fail-open no behavioral arm can currently reach: if the
# comparands are ever reverted to raw `git rev-parse` output, removing or moving
# the toplevel cd silently clears the primary checkout for landing.
if grep -qE 'GITDIR_ABS="\$\(abs_dir ' "$PREFLIGHT" \
  && grep -qE 'COMMONDIR_ABS="\$\(abs_dir ' "$PREFLIGHT"; then
  pass "landing check compares canonicalized paths (abs_dir), not raw git output"
else
  fail "landing check must canonicalize both git-dir and git-common-dir via abs_dir"
fi

if grep -qE '^\s*GITDIR_ABS="\$\(git rev-parse' "$PREFLIGHT" \
  || grep -qE '^\s*COMMONDIR_ABS="\$\(git rev-parse' "$PREFLIGHT"; then
  fail "landing check compares RAW git rev-parse output — fails open without the toplevel cd"
else
  pass "landing check does not compare raw git rev-parse output"
fi

# --- Arm 8: the fixture survives a hostile inherited global git config --------
# Regression guard for the three neutralizations above. Reproduces the exact
# conditions measured to abort the fixture before any assertion: forced commit
# signing (exit 128) and a foreign pre-commit hook via core.hooksPath (exit 1).
# init.templateDir is covered by the same core.hooksPath override, which takes
# precedence over the .git/hooks the template seeds.
HOSTILE_HOME="$TMPROOT/hostile"
mkdir -p "$HOSTILE_HOME/hooks"
printf '#!/bin/sh\nexit 1\n' >"$HOSTILE_HOME/hooks/pre-commit"
chmod +x "$HOSTILE_HOME/hooks/pre-commit"
{
  printf '[commit]\n\tgpgsign = true\n'
  printf '[tag]\n\tgpgsign = true\n'
  printf '[core]\n\thooksPath = %s\n' "$HOSTILE_HOME/hooks"
  printf '[init]\n\ttemplateDir = %s\n' "$HOSTILE_HOME"
} >"$HOSTILE_HOME/gitconfig"

set +e
(
  # Re-arm errexit inside the subshell: the `set +e` above is inherited, and
  # without this the failed fixture commit would be skipped over and this arm
  # would report the status of a later command instead (measured: vacuous pass).
  set -e
  export GIT_CONFIG_GLOBAL="$HOSTILE_HOME/gitconfig"
  make_fixture "$TMPROOT/hostile-primary" "$TMPROOT/hostile-linked"
) >/dev/null 2>&1
HOSTILE_RC=$?
set -e
if [ "$HOSTILE_RC" -eq 0 ]; then
  pass "fixture builds under a global config forcing gpgsign + a vetoing hooksPath"
else
  fail "fixture must not inherit global git config; got rc=$HOSTILE_RC"
fi

# And the assertion path itself still works against that hostile-config fixture.
run_preflight "$TMPROOT/hostile-linked" --landing
if [ "$RC" -eq 0 ]; then
  pass "hostile-config fixture: --landing in the linked worktree still exits 0"
else
  fail "hostile-config fixture: linked worktree should pass; got rc=$RC, out: $OUT"
fi
run_preflight "$TMPROOT/hostile-primary" --landing
if [ "$RC" -ne 0 ]; then
  pass "hostile-config fixture: --landing in the primary checkout still fails"
else
  fail "hostile-config fixture: primary MUST fail; got rc=0, out: $OUT"
fi

# --- SLICE-ID-HARDENING: fractional-slice-id plan fixtures --------------------
# The --expect-closed gate (preflight.sh §5) exists to refuse a spawn whose
# declared dependency never closed. Two independent ways it said yes anyway:
#
#   site 4 — the trailing `[^0-9]` matches the `.` in `Slice 39.5`, so ONE
#            UNIT'S CLOSED WITNESS SATISFIES ANOTHER'S: `--expect-closed 39` is
#            cleared by a plan that only ever closed Slice 39.5.
#   [DETERMINE] duty 1 — `${EXPECT_CLOSED}` is interpolated UNESCAPED into an
#            ERE, so with `--expect-closed 39.5` the `.` is a WILDCARD and a
#            plan line reading `Slice 39x5 … CLOSED` clears the gate.
#
# These are separately reachable and a fix for one does not fix the other —
# measured, not reasoned: tightening the trailing class alone still clears
# `Slice 39x5`, and escaping the value alone still clears `Slice 39.5` for
# `--expect-closed 39`. Both classes get their own arms, and the alternation has
# TWO alternatives (`Slice…CLOSED` and `CLOSED…Slice`) which are NOT symmetric —
# the first ends `[^0-9]`, the second `([^0-9]|$)` — so both are graded.
#
# Fixtures live outside the fixture repo (absolute --plan paths) so they cannot
# dirty it. Fractional ids in throwaway fixtures are IN scope; the prohibition
# on fractional ids applies to real state, the ladder and the board.
PLANS="$TMPROOT/plans"
mkdir -p "$PLANS"
# ONLY Slice 39.5 is CLOSED. Slice 39 has NO closed witness anywhere.
printf '# fixture plan\n\n- Slice 39.5 — collect-all harness: CLOSED\n' \
  >"$PLANS/frac-alt1.md"
printf '# fixture plan\n\n- CLOSED — Slice 39.5 collect-all harness\n' \
  >"$PLANS/frac-alt2.md"
# ONLY a regex-wildcard near-miss. No real Slice 39.5 witness anywhere.
printf '# fixture plan\n\n- Slice 39x5 — collect-all harness: CLOSED\n' \
  >"$PLANS/wildcard-alt1.md"
printf '# fixture plan\n\n- CLOSED — Slice 39x5 collect-all harness\n' \
  >"$PLANS/wildcard-alt2.md"
# The same fractional-neighbour safety property must hold for each accepted
# LANDED form: local close record, status-first close record, and roll-up.
printf '# fixture plan\n\n- Slice 39.5 — collect-all harness: LANDED\n' \
  >"$PLANS/landed-frac-alt1.md"
printf '# fixture plan\n\n- LANDED — Slice 39.5 collect-all harness\n' \
  >"$PLANS/landed-frac-alt2.md"
printf '# fixture plan\n\n**LANDED on main:** Slices 39.5 (`b6cc8fa6`)\n' \
  >"$PLANS/landed-frac-rollup.md"
# True positives that must keep clearing the gate.
printf '# fixture plan\n\n- Slice 39 — TC-86 redact: CLOSED\n' >"$PLANS/true-int.md"
# `release-state` generated plan roll-ups record landed slices as LANDED, not
# CLOSED. A landed exact-id dependency is equally a valid closure witness.
printf '# fixture plan\n\n- Slice 39 — TC-86 redact: LANDED\n' >"$PLANS/landed-int.md"
# The generated plan roll-up groups ids after its `LANDED` marker. It is the
# canonical form the gate must recognize for a landed dependency.
printf '# fixture plan\n\n**LANDED on main:** Slices 30 (`9b3ed0e3`)\n' \
  >"$PLANS/landed-rollup.md"
# The renderer writes the complete landed set in ladder order. A dependency
# need not be the first item in that canonical roll-up.
printf '# fixture plan\n\n**LANDED on `origin/main`, in full:** Slices 0 (`2ea2c884`) · 5 (`a6cf2bbe`) · 10 (`f94275e1`) · 15 (`19d8f072`).\n' \
  >"$PLANS/landed-rollup-prior-items.md"
# Status words must be affirmative closure witnesses, not merely substrings. A
# negated or prefixed status must not authorize a dependent spawn.
printf '# fixture plan\n\n- Slice 39 — NOT CLOSED\n' >"$PLANS/not-closed.md"
printf '# fixture plan\n\n- Slice 39.5 — UNCLOSED\n' >"$PLANS/unclosed.md"
printf '# fixture plan\n\n- Slice 39 — UNLANDED\n' >"$PLANS/unlanded.md"
# A legitimate sentence-final period after an integer id. This is the control
# that rules OUT the naive fix: swapping the trailing `[^0-9]` for `[^0-9.]`
# closes site 4 but makes THIS line stop matching — a new false negative in a
# gate whose failure mode is refusing to spawn. Measured before the fix landed.
printf '# fixture plan\n\n- CLOSED — Slice 39.\n' >"$PLANS/int-trailing-period.md"

# --- Arm 9 (site 4): a FRACTIONAL neighbour's CLOSED witness must NOT satisfy
# the INTEGER dependency. RED-first: pre-fix both of these exit 0 ("ok
# dependency Slice/Phase 39 has a CLOSED witness") on a plan in which Slice 39
# was never closed at all.
for alt in alt1 alt2; do
  run_preflight "$LINKED" --expect-closed 39 --plan "$PLANS/frac-$alt.md"
  if [ "$RC" -ne 0 ]; then
    pass "site 4 ($alt): 'Slice 39.5 … CLOSED' does not satisfy --expect-closed 39"
  else
    fail "site 4 RECURRENCE ($alt): Slice 39.5's CLOSED witness cleared --expect-closed 39; got rc=0, out: $OUT"
  fi
  if printf '%s' "$OUT" | grep -q "^HARD .*Slice/Phase 39 has NO 'CLOSED' or 'LANDED' witness"; then
    pass "site 4 ($alt): the refusal names the dependency that is not closed"
  else
    fail "expected a HARD line naming Slice/Phase 39 as not CLOSED; got: $OUT"
  fi
done

# --- Arm 9b: LANDED witnesses must retain Arm 9's exact-id boundary. The
# gate accepts three LANDED forms, so all three must reject Slice 39.5 when
# the requested dependency is Slice 39.
for landed in landed-frac-alt1 landed-frac-alt2 landed-frac-rollup; do
  run_preflight "$LINKED" --expect-closed 39 --plan "$PLANS/$landed.md"
  if [ "$RC" -ne 0 ]; then
    pass "LANDED boundary ($landed): Slice 39.5 does not satisfy --expect-closed 39"
  else
    fail "LANDED boundary RECURRENCE ($landed): Slice 39.5 cleared --expect-closed 39; got rc=0, out: $OUT"
  fi
  if printf '%s' "$OUT" | grep -q "^HARD .*Slice/Phase 39 has NO 'CLOSED' or 'LANDED' witness"; then
    pass "LANDED boundary ($landed): the refusal names the dependency that is not closed"
  else
    fail "expected a HARD line naming Slice/Phase 39 as not closed or landed; got: $OUT"
  fi
done

# --- Arm 10 ([DETERMINE] duty 1): the interpolated value must be ESCAPED, so
# the `.` in `39.5` cannot act as a regex wildcard. RED-first: pre-fix both of
# these exit 0 on a plan whose only witness is the impostor `Slice 39x5`.
for alt in alt1 alt2; do
  run_preflight "$LINKED" --expect-closed 39.5 --plan "$PLANS/wildcard-$alt.md"
  if [ "$RC" -ne 0 ]; then
    pass "duty 1 ($alt): 'Slice 39x5 … CLOSED' does not satisfy --expect-closed 39.5 (dot is not a wildcard)"
  else
    fail "duty 1 RECURRENCE ($alt): the unescaped '.' matched 'x'; got rc=0, out: $OUT"
  fi
  if printf '%s' "$OUT" | grep -q "^HARD .*Slice/Phase 39.5 has NO 'CLOSED' or 'LANDED' witness"; then
    pass "duty 1 ($alt): the refusal names the dependency that is not closed"
  else
    fail "expected a HARD line naming Slice/Phase 39.5 as not CLOSED; got: $OUT"
  fi
done

# --- Arm 10b: a status must be an affirmative standalone closure token. These
# three forms all previously passed because CLOSED/LANDED was matched as a loose
# substring. Keep both integer and fractional ids covered here; the surrounding
# arms retain the complete exact-id boundary matrix for affirmative witnesses.
for negated in 'not-closed:39:NOT CLOSED' 'unclosed:39.5:UNCLOSED' 'unlanded:39:UNLANDED'; do
  IFS=: read -r fixture expected phrase <<<"$negated"
  run_preflight "$LINKED" --expect-closed "$expected" --plan "$PLANS/$fixture.md"
  if [ "$RC" -ne 0 ]; then
    pass "negation guard ($phrase): does not satisfy --expect-closed $expected"
  else
    fail "negation recurrence ($phrase): a non-affirmative status cleared --expect-closed $expected; got rc=0, out: $OUT"
  fi
  if printf '%s' "$OUT" | grep -q "^HARD .*Slice/Phase $expected has NO 'CLOSED' or 'LANDED' witness"; then
    pass "negation guard ($phrase): the refusal names the dependency that is not closed"
  else
    fail "negation guard ($phrase): expected a HARD refusal for Slice/Phase $expected; got: $OUT"
  fi
done

# --- Arm 11 (regression guards, NOT recurrence arms): the true positives must
# keep clearing. Without these, a fix that simply never matches would pass every
# arm above.
run_preflight "$LINKED" --expect-closed 39.5 --plan "$PLANS/frac-alt1.md"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: a REAL 'Slice 39.5 … CLOSED' witness satisfies --expect-closed 39.5"
else
  fail "a genuine fractional CLOSED witness must clear the gate; got rc=$RC, out: $OUT"
fi
run_preflight "$LINKED" --expect-closed 39 --plan "$PLANS/true-int.md"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: 'Slice 39 … CLOSED' still satisfies --expect-closed 39"
else
  fail "an integer CLOSED witness must still clear the gate; got rc=$RC, out: $OUT"
fi
run_preflight "$LINKED" --expect-closed 39 --plan "$PLANS/landed-int.md"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: 'Slice 39 … LANDED' satisfies --expect-closed 39"
else
  fail "an integer LANDED witness must clear the gate; got rc=$RC, out: $OUT"
fi
run_preflight "$LINKED" --expect-closed 30 --plan "$PLANS/landed-rollup.md"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: a generated LANDED roll-up satisfies --expect-closed 30"
else
  fail "a generated LANDED roll-up must clear the gate; got rc=$RC, out: $OUT"
fi
run_preflight "$LINKED" --expect-closed 15 --plan "$PLANS/landed-rollup-prior-items.md"
if [ "$RC" -eq 0 ]; then
  pass "regression guard: a generated LANDED roll-up recognizes a later landed slice"
else
  fail "a later entry in a generated LANDED roll-up must clear the gate; got rc=$RC, out: $OUT"
fi
run_preflight "$LINKED" --expect-closed 39 --plan "$PLANS/int-trailing-period.md"
if [ "$RC" -eq 0 ]; then
  pass "control: a sentence-final 'CLOSED — Slice 39.' still satisfies --expect-closed 39 (rules out the naive [^0-9.] fix)"
else
  fail "the naive trailing-class fix regressed a legitimate 'Slice 39.' witness; got rc=$RC, out: $OUT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll preflight --landing tests passed\n'
