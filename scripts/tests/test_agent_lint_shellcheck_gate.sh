#!/usr/bin/env bash
# scripts/tests/test_agent_lint_shellcheck_gate.sh — 0.8.21 Slice 30
# (SHELLCHECK). The RED-FIRST test for the gate itself.
#
# THE INCIDENT CLASS. Under `set -o pipefail`, `producer | head -n1` makes the
# producer die of SIGPIPE once head has its line; the pipeline's status is 141
# and the caller usually never looks. Three earlier hand-fixes of that exact
# shape landed in this repo; Slice 25 was the FOURTH. The cure is not a fifth
# hand-fix, it is a gate that refuses the shape at authoring time.
#
# ⚠ IT TAKES TWO LEGS, AND THAT IS A MEASURED FINDING, NOT A PREFERENCE.
# dev/design/ci-verify-robustness-review.md line 738 says shellcheck's SC2312
# (check-extra-masked-returns) "would have caught [the 2026-08-04 bug]
# deterministically". Measured on the pinned shellcheck 0.11.0, it does NOT:
# fed the verbatim pre-fix line, shellcheck reports nothing under SC2312 or any
# of its eleven optional checks, because there the substitution's status IS the
# assignment's status. So arm A asserts BOTH legs — SC2312 for the masked-return
# family it does cover, and the repo's own early-exiting-consumer detector
# (scripts/lib/shell-early-consumer.sh) for the `| head` shape it does not.
# Asserting only SC2312 here would have been a green test for a gate that misses
# the very bug it was commissioned to stop.
#
# RED-FIRST EVIDENCE. Against the pre-slice tree (`git show <base>:scripts/
# agent-lint.sh`, which contains no lint-shell leg at all) arm A of this test
# FAILS: agent-lint.sh walks straight past a script carrying both defects. The
# transcript is recorded in the slice closure output. A pre-fix copy is
# deliberately NOT checked in — a stale fixture nobody re-derives is itself a
# vacuous-green trap.
#
# ⛔ Every arm drives the REAL scripts/agent-lint.sh and scripts/
# agent-lint-shell.sh, copied into a throwaway git repo. Nothing here writes
# into this checkout. Arms A/B reach the shell leg because it runs before the
# language toolchains; the fixture's fake ruff/actionlint satisfy the two
# preflights that precede it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck source=../lib/shellcheck-version.sh
. "$REPO_ROOT/scripts/lib/shellcheck-version.sh"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

fail_arm() {
  printf 'FAIL  %s\n' "$1" >&2
  exit 1
}

# The gate cannot be tested without the tool it gates on. Missing => FAIL, and
# never a skip: a "skipped" arm here would be the very vacuous green the slice
# exists to abolish.
REAL_SHELLCHECK="$(find_shellcheck_bin || true)"
REAL_SHELLCHECK_VERSION=""
if [ -n "$REAL_SHELLCHECK" ]; then
  REAL_SHELLCHECK_VERSION="$(read_shellcheck_version "$REAL_SHELLCHECK")"
fi
if [ "$REAL_SHELLCHECK_VERSION" != "$SHELLCHECK_VERSION" ]; then
  fail_arm "shellcheck $SHELLCHECK_VERSION is required to run this test. Run scripts/bootstrap.sh."
fi

# ---------------------------------------------------------------------------
# Fixture: a throwaway repo carrying the real gate + a real pinned shellcheck.
# ---------------------------------------------------------------------------
mkdir -p "$FIX/repo/scripts/lib" "$FIX/bin" "$FIX/home/.local/bin"
cp "$REPO_ROOT/scripts/agent-lint.sh" "$FIX/repo/scripts/agent-lint.sh"
cp "$REPO_ROOT/scripts/agent-lint-shell.sh" "$FIX/repo/scripts/agent-lint-shell.sh"
cp "$REPO_ROOT/scripts/lib/agent-output.sh" "$FIX/repo/scripts/lib/agent-output.sh"
cp "$REPO_ROOT/scripts/lib/actionlint-version.sh" "$FIX/repo/scripts/lib/actionlint-version.sh"
cp "$REPO_ROOT/scripts/lib/shellcheck-version.sh" "$FIX/repo/scripts/lib/shellcheck-version.sh"
cp "$REPO_ROOT/scripts/lib/shell-early-consumer.sh" "$FIX/repo/scripts/lib/shell-early-consumer.sh"
cp "$REPO_ROOT/.shellcheckrc" "$FIX/repo/.shellcheckrc"
chmod +x "$FIX/repo/scripts/agent-lint.sh" "$FIX/repo/scripts/agent-lint-shell.sh"

printf '#!/usr/bin/env bash\nprintf "1.7.12\\n"\n' >"$FIX/bin/actionlint"
printf '#!/usr/bin/env bash\nprintf "ruff 0.15.17\\n"\n' >"$FIX/bin/ruff"
chmod +x "$FIX/bin/actionlint" "$FIX/bin/ruff"
ln -s "$REAL_SHELLCHECK" "$FIX/home/.local/bin/shellcheck"

# The two defects, in one file, in the exact shapes this repo has shipped:
#   1. `grep … | head -n1` under pipefail — the Slice 25 SIGPIPE class.
#   2. a masked return: the substituted command's status is swallowed by the
#      command it is an argument to, so a crashed producer reads as "no output",
#      which is a PASS to every `-z`/`-n` guard downstream (a fail-open).
write_defect() {
  cat >"$FIX/repo/scripts/defect.sh" <<'DEFECT'
#!/usr/bin/env bash
set -euo pipefail

# leg 3: the 2026-08-04 / Slice 25 shape. `head` closes the pipe under `grep`.
first_hit="$(grep -n 'needle' haystack.txt | head -n 1 | cut -d: -f1)"
printf 'first: %s\n' "$first_hit"

# leg 2 (SC2312): git's exit status is swallowed by the printf it feeds, so a
# crashed producer reads as "no unmerged paths" — a fail-open.
printf 'unmerged: %s\n' "$(git ls-files --unmerged)"
DEFECT
}

# A file that is clean under BOTH legs, so the "gate can pass" direction is
# proven and the red arms are not passing for a trivial reason.
write_clean() {
  cat >"$FIX/repo/scripts/clean.sh" <<'CLEAN'
#!/usr/bin/env bash
set -euo pipefail

first_hit="$(grep -m1 -n 'needle' haystack.txt || true)"
printf 'first: %s\n' "$first_hit"

unmerged="$(git ls-files --unmerged)"
if [ -n "$unmerged" ]; then
  printf 'unmerged paths present\n' >&2
fi
CLEAN
}

commit_fixture() {
  (
    cd "$FIX/repo"
    git add -A
    git commit -q -m fixture
  ) >/dev/null 2>&1 || true
}

(
  cd "$FIX/repo"
  git init -q
  git config user.email test@example.com
  git config user.name test
)
write_clean
: >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
: >"$FIX/repo/scripts/shell-early-consumer-ratchet.txt"
commit_fixture
# The ordinary arms model a protected PR instead of inheriting whatever
# GitHub environment happens to run this test. The real ratchet correctly
# requires origin/main there, so establish that full-checkout baseline before
# any fixture mutation.
BASE="$(cd "$FIX/repo" && git rev-parse HEAD)"
(cd "$FIX/repo" && git update-ref refs/remotes/origin/main "$BASE")

run_agent_lint() {
  set +e
  OUT="$(cd "$FIX/repo" && HOME="$FIX/home" PATH="$FIX/bin:/usr/bin:/bin" \
    bash scripts/agent-lint.sh 2>&1)"
  RC=$?
  set -e
}

run_lint_shell() {
  set +e
  OUT="$(cd "$FIX/repo" && HOME="$FIX/home" PATH="$FIX/bin:/usr/bin:/bin" \
    GITHUB_REF='refs/pull/1/merge' GITHUB_EVENT_NAME='pull_request' GITHUB_BASE_REF='main' \
    bash scripts/agent-lint-shell.sh 2>&1)"
  RC=$?
  set -e
}

# The entrypoint runs locally as well as in CI. The history comparison is only
# mandatory for a protected-branch/PR verdict, so exercise the protected-main
# path explicitly rather than making an ordinary local edit impossible to lint.
run_lint_shell_on_main() {
  set +e
  OUT="$(cd "$FIX/repo" && HOME="$FIX/home" PATH="$FIX/bin:/usr/bin:/bin" \
    GITHUB_REF='refs/heads/main' GITHUB_EVENT_NAME='push' \
    bash scripts/agent-lint-shell.sh 2>&1)"
  RC=$?
  set -e
}

run_lint_shell_on_pr() {
  set +e
  OUT="$(cd "$FIX/repo" && HOME="$FIX/home" PATH="$FIX/bin:/usr/bin:/bin" \
    GITHUB_REF='refs/pull/1/merge' GITHUB_EVENT_NAME='pull_request' GITHUB_BASE_REF='main' \
    bash scripts/agent-lint-shell.sh 2>&1)"
  RC=$?
  set -e
}

# --- arm A: agent-lint.sh REJECTS the Slice 25 defect class -----------------
write_defect
commit_fixture
run_agent_lint
printf -- '---- arm A output ----\n%s\nexit=%d\n' "$OUT" "$RC"

[ "$RC" -ne 0 ] || fail_arm "arm A: agent-lint.sh accepted a script with a pipefail SIGPIPE and a masked return"
grep -Fq 'FAIL lint-shell' <<<"$OUT" \
  || fail_arm "arm A: the failure was not attributed to the lint-shell leg"
grep -Fq 'SC2312' <<<"$OUT" \
  || fail_arm "arm A: SC2312 (check-extra-masked-returns) did not fire — is .shellcheckrc's enable= reaching the gate?"
grep -Fq 'scripts/defect.sh' <<<"$OUT" \
  || fail_arm "arm A: the report did not name the offending file"
grep -Fq 'git ls-files --unmerged' <<<"$OUT" \
  || fail_arm "arm A: the masked-return site (SC2312) was not among the findings"
grep -Fq 'early-exiting consumer' <<<"$OUT" \
  || fail_arm 'arm A: the early-exiting-consumer leg did not fire; that shape is the one shellcheck does NOT cover'
grep -Fq 'head -n 1' <<<"$OUT" \
  || fail_arm "arm A: the SIGPIPE pipeline site was not among the findings"
printf 'PASS  arm A: agent-lint.sh rejects both the early-exiting-consumer shape (leg 3) and a masked return (leg 2, SC2312)\n'

# --- arm B: the DEFAULT ruleset leg is live too, not just SC2312 ------------
cat >"$FIX/repo/scripts/defect.sh" <<'DEFECT2'
#!/usr/bin/env bash
set -euo pipefail
target="a b"
rm -rf $target
DEFECT2
commit_fixture
run_agent_lint
printf -- '---- arm B output ----\n%s\nexit=%d\n' "$OUT" "$RC"

[ "$RC" -ne 0 ] || fail_arm "arm B: agent-lint.sh accepted an unquoted word-splitting expansion"
grep -Fq 'SC2086' <<<"$OUT" || fail_arm "arm B: the default ruleset leg did not fire (SC2086 expected)"
printf 'PASS  arm B: the default-ruleset leg fires independently of SC2312\n'

# --- arm C: a clean tree PASSES (the gate is not stuck red) -----------------
rm -f "$FIX/repo/scripts/defect.sh"
commit_fixture
run_lint_shell
printf -- '---- arm C output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -eq 0 ] || fail_arm "arm C: the shell lint failed on a clean tree — the red arms above prove nothing if this is red"
printf 'PASS  arm C: a clean tree passes both legs (the red arms are non-vacuous)\n'

# --- arm C1/C2: a ratchet is mechanically SHRINK-ONLY in CI ---------------
# A currently-dirty file is the attack that the older "remove entries once
# clean" check missed: adding it to the list makes the actual finding disappear
# before it can ever become stale. On main/PR history must reject the addition
# itself, for BOTH independently-owned ratchets.
cat >"$FIX/repo/scripts/sc2312-dirty.sh" <<'DIRTY_SC2312'
#!/usr/bin/env bash
printf 'unmerged: %s\n' "$(git ls-files --unmerged)"
DIRTY_SC2312
printf 'scripts/sc2312-dirty.sh\n' >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
commit_fixture
run_lint_shell_on_main
printf -- '---- arm C1 output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm C1: main accepted a newly-added SC2312 exemption for a still-dirty file"
grep -Fq 'adds exemption(s)' <<<"$OUT" \
  || fail_arm "arm C1: the added SC2312 exemption was not attributed to the shrink-only history guard"
grep -Fq 'scripts/sc2312-dirty.sh' <<<"$OUT" \
  || fail_arm "arm C1: the added SC2312 exemption was not named"
printf 'PASS  arm C1: main rejects a newly-added SC2312 exemption even while it is still dirty\n'

: >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
rm -f "$FIX/repo/scripts/sc2312-dirty.sh"
cat >"$FIX/repo/scripts/early-dirty.sh" <<'DIRTY_EARLY'
#!/usr/bin/env bash
producer | grep --quiet needle
DIRTY_EARLY
printf 'scripts/early-dirty.sh\n' >"$FIX/repo/scripts/shell-early-consumer-ratchet.txt"
commit_fixture
run_lint_shell_on_main
printf -- '---- arm C2 output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm C2: main accepted a newly-added early-consumer exemption for a still-dirty file"
grep -Fq 'adds exemption(s)' <<<"$OUT" \
  || fail_arm "arm C2: the added early-consumer exemption was not attributed to the shrink-only history guard"
grep -Fq 'scripts/early-dirty.sh' <<<"$OUT" \
  || fail_arm "arm C2: the added early-consumer exemption was not named"
printf 'PASS  arm C2: main rejects a newly-added early-consumer exemption even while it is still dirty\n'

# Restore the clean ratchets so the pre-existing rot arms below continue to
# prove their own independent invariants.
: >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
: >"$FIX/repo/scripts/shell-early-consumer-ratchet.txt"
rm -f "$FIX/repo/scripts/sc2312-dirty.sh" "$FIX/repo/scripts/early-dirty.sh"
commit_fixture

# The PR path must use origin/$GITHUB_BASE_REF rather than HEAD^, otherwise a
# merge commit (or a branch with several commits) could grow the list one
# commit at a time. Model the full-checkout ref that the workflow provides.
BASE="$(cd "$FIX/repo" && git rev-parse HEAD)"
(cd "$FIX/repo" && git update-ref refs/remotes/origin/main "$BASE")
cat >"$FIX/repo/scripts/pr-dirty.sh" <<'DIRTY_PR'
#!/usr/bin/env bash
producer |& head -n1
DIRTY_PR
printf 'scripts/pr-dirty.sh\n' >"$FIX/repo/scripts/shell-early-consumer-ratchet.txt"
commit_fixture
run_lint_shell_on_pr
printf -- '---- arm C3 output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm C3: PR accepted an exemption newly added after origin/main"
grep -Fq 'adds exemption(s)' <<<"$OUT" \
  || fail_arm "arm C3: the PR exemption was not attributed to the shrink-only history guard"
grep -Fq 'scripts/pr-dirty.sh' <<<"$OUT" \
  || fail_arm "arm C3: the PR-added exemption was not named"
printf 'PASS  arm C3: PR compares to origin/main and rejects a newly-added exemption\n'

: >"$FIX/repo/scripts/shell-early-consumer-ratchet.txt"
rm -f "$FIX/repo/scripts/pr-dirty.sh"
commit_fixture

# --- arm D: the ratchet may only SHRINK ------------------------------------
# scripts/clean.sh is SC2312-clean. Listing it must FAIL the gate, so a stale
# exemption cannot rot into a permanent hole.
printf 'scripts/clean.sh\n' >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
commit_fixture
run_lint_shell
printf -- '---- arm D output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm D: a now-clean file kept its SC2312 exemption silently"
grep -Fq 'is now SC2312-clean' <<<"$OUT" || fail_arm "arm D: the ratchet-rot failure was not explained"
printf 'PASS  arm D: a file that became SC2312-clean must leave the ratchet\n'

# --- arm E: a stale ratchet path FAILS -------------------------------------
printf 'scripts/renamed-away.sh\n' >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
commit_fixture
run_lint_shell
printf -- '---- arm E output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm E: the ratchet listed a path that is not a tracked *.sh and the gate passed"
grep -Fq 'not a tracked' <<<"$OUT" || fail_arm "arm E: the stale-path failure was not explained"
printf 'PASS  arm E: a stale ratchet path fails the gate (a rename cannot carry its exemption)\n'

# --- arm F: exempting EVERY file is a vacuous pass and must FAIL ------------
(cd "$FIX/repo" && git ls-files '*.sh') >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
commit_fixture
run_lint_shell
printf -- '---- arm F output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm F: a ratchet covering every tracked file reported a (vacuous) pass"
grep -Fq 'vacuous' <<<"$OUT" || fail_arm "arm F: the all-exempt failure was not explained"
printf 'PASS  arm F: a ratchet that exempts every file is rejected as vacuous\n'

# --- arm G: a missing ratchet file FAILS (no fail-open) ---------------------
rm -f "$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
commit_fixture
run_lint_shell
printf -- '---- arm G output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm G: a missing ratchet file was treated as 'nothing exempt, all good'"
grep -Fq 'ratchet cannot be evaluated' <<<"$OUT" || fail_arm "arm G: the missing-ratchet failure was not explained"
printf 'PASS  arm G: a missing ratchet file fails the gate\n'

# --- arm I: leg 3's ratchet obeys the same rules ---------------------------
# The early-consumer ratchet is separate machinery from the SC2312 one, so it
# gets its own rot arm rather than inheriting arms D-G by assertion.
printf 'scripts/clean.sh\n' >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
: >"$FIX/repo/scripts/shellcheck-sc2312-ratchet.txt"
printf 'scripts/clean.sh\n' >"$FIX/repo/scripts/shell-early-consumer-ratchet.txt"
commit_fixture
run_lint_shell
printf -- '---- arm I output ----\n%s\nexit=%d\n' "$OUT" "$RC"
[ "$RC" -ne 0 ] || fail_arm "arm I: a file with no early-consumer site kept its leg-3 exemption silently"
grep -Fq 'no longer pipes into an early-exiting consumer' <<<"$OUT" \
  || fail_arm "arm I: the leg-3 ratchet-rot failure was not explained"
printf 'PASS  arm I: leg 3'"'"'s ratchet only shrinks too\n'

# --- arm H: this repo's own ratchet is honest ------------------------------
# Every listed path is a tracked *.sh AND still has at least one SC2312 finding.
# (agent-lint-shell.sh enforces this at gate time; asserting it here too means
# `agent-test.sh` reports the rot even if someone runs only the test suite.)
stale=0
while IFS= read -r entry; do
  case "$entry" in '' | '#'*) continue ;; esac
  if ! (cd "$REPO_ROOT" && git ls-files --error-unmatch -- "$entry") >/dev/null 2>&1; then
    printf 'FAIL  arm H: ratchet entry is not tracked: %s\n' "$entry" >&2
    stale=1
    continue
  fi
  if (cd "$REPO_ROOT" && "$REAL_SHELLCHECK" --severity=style --include=SC2312 \
    --format=quiet -- "$entry"); then
    printf 'FAIL  arm H: ratchet entry is now SC2312-clean and must be removed: %s\n' "$entry" >&2
    stale=1
  fi
done <"$REPO_ROOT/scripts/shellcheck-sc2312-ratchet.txt"
[ "$stale" -eq 0 ] || fail_arm "arm H: scripts/shellcheck-sc2312-ratchet.txt has rotted"
printf 'PASS  arm H: every entry in the real ratchet is tracked and still non-clean\n'

# A rebase can reintroduce a masked command substitution into a fixture without
# touching either ratchet. Keep this shallow-clone regression source SC2312
# clean under the exact optional check the gate enforces; unrelated historical
# default-rule findings are covered by that gate's separate default-ruleset leg.
STATE_VIEWS_FIXTURE="$REPO_ROOT/scripts/tests/test_check_release_state_views.sh"
if "$REAL_SHELLCHECK" --severity=style --include=SC2312 \
  --format=quiet -- "$STATE_VIEWS_FIXTURE"; then
  printf 'PASS  arm J: release-state shallow-clone fixture is ShellCheck-clean\n'
else
  fail_arm "arm J: release-state shallow-clone fixture must be ShellCheck-clean"
fi

printf 'All shellcheck gate tests passed\n'
