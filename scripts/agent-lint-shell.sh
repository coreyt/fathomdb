#!/usr/bin/env bash
# scripts/agent-lint-shell.sh — the enforced shell lint. Invoked by
# scripts/agent-lint.sh as `run_capped lint-shell`; runnable standalone.
#
# 0.8.21 Slice 30 (SHELLCHECK), design of record
# dev/design/ci-verify-robustness-review.md R1.1 / §3.1 / §2.5.
#
# WHY THIS EXISTS. Before this slice shellcheck was invoked NOWHERE: 140 tracked
# shell files, 123 of them under `set -euo pipefail`, zero linting. All 43
# `shellcheck` string hits in the tree were `# shellcheck ...` comment
# directives — the repo already spoke the annotation dialect but never ran the
# tool. Slice 25 was the FOURTH hand-fix of the same `cmd | head` SIGPIPE /
# fail-open class. This is the gate that stops the fifth.
#
# THREE LEGS, all enforced:
#
#   leg 1  shellcheck DEFAULT RULESET, every tracked *.sh, --severity=style.
#          Two info-level checks are deferred and NAMED below, on the command
#          line rather than hidden in .shellcheckrc.
#
#   leg 2  shellcheck SC2312 (check-extra-masked-returns), RATCHETED coverage.
#          SC2312 is optional in shellcheck and off unless requested; it is the
#          check for the masked-return family, so it is never suppressed
#          globally. Files not yet clean are enumerated in
#          scripts/shellcheck-sc2312-ratchet.txt and that list may only shrink.
#
#   leg 3  the EARLY-EXITING-CONSUMER detector (scripts/lib/shell-early-consumer.sh),
#          also ratcheted. ⚠ This leg is not redundant with leg 2: measured on
#          the pinned shellcheck 0.11.0, NO shellcheck check — SC2312 included,
#          and none of the eleven optional ones — flags the verbatim 2026-08-04
#          line or the P0 `if producer | grep -q .` shape. See that library's
#          header for the measurement and for the correction it records to the
#          design doc's line 738. Without leg 3 this slice would ship a gate
#          that does not cover the bug it was commissioned to stop.
#
# Leg 1 passes --exclude=SC2312 because .shellcheckrc enables it repo-wide (so
# editors agree with the gate); SC2312 is NOT skipped, it is adjudicated by
# leg 2 against its ratchet. Excluding it here only prevents double-reporting.
#
# ⛔ NOTHING here may turn a real failure into a pass or a skip. There is no
# "shellcheck not installed -> skip" branch (TC-37: agent-lint-md.sh once exited
# 0 when markdownlint-cli2 was absent and hid a red `main` for three weeks). A
# missing or wrong-version shellcheck is a FAILED lint.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/shellcheck-version.sh
. "$SCRIPT_DIR/lib/shellcheck-version.sh"
# shellcheck source=lib/shell-early-consumer.sh
. "$SCRIPT_DIR/lib/shell-early-consumer.sh"
# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1

SC2312_RATCHET="scripts/shellcheck-sc2312-ratchet.txt"
EARLY_CONSUMER_RATCHET="scripts/shell-early-consumer-ratchet.txt"

# DEFERRED — named here, not disabled in .shellcheckrc, and tracked as follow-up
# FUP-SHELLCHECK-1. Both are info-level style checks with no relationship to the
# masked-return/SIGPIPE class this slice exists to close, and both are dominated
# by deliberate idioms, so clearing them is a large mechanical diff that would
# compete with the real work in the same slice:
#   SC2016  (91 sites) "Expressions don't expand in single quotes" — fired by
#           single-quoted heredocs and awk/jq/python programs written for a
#           DIFFERENT interpreter, where non-expansion is the whole point.
#   SC2015  (40 sites) "A && B || C is not if-then-else" — fired by the test
#           harnesses' `cmd && pass … || fail …` reporting idiom.
# Removing either name from this list is the follow-up's definition of done.
DEFERRED_CHECKS="SC2016,SC2015"

shellcheck_bin="$(require_shellcheck_bin lint-shell)"

# NUL-delimited, via a temp file rather than a process substitution: `git`
# runs as its own command so `set -e` sees its exit status (a failing
# `git ls-files` inside `< <(...)` would read as "no shell files" and pass), and
# a command substitution cannot be used because it strips NUL bytes.
shell_file_list="$(mktemp)"
trap 'rm -f "$shell_file_list"' EXIT
git ls-files -z '*.sh' >"$shell_file_list"
mapfile -t -d '' shell_files <"$shell_file_list"
if [ "${#shell_files[@]}" -eq 0 ]; then
  printf 'FAIL lint-shell: no tracked *.sh files found; refusing to report a vacuous pass.\n' >&2
  exit 1
fi
tracked_index=" ${shell_files[*]} "

rc=0

# ---------------------------------------------------------------------------
# Ratchet plumbing, shared by legs 2 and 3.
#
# The invariant, in one sentence: a ratchet lists files that are NOT YET
# covered, everything else IS covered (so new files are covered by default and
# nothing has to be remembered), and the list may only SHRINK.
# ---------------------------------------------------------------------------

# read_ratchet <path> -> populates the global array `ratchet`
ratchet=()
read_ratchet() {
  local path="$1" line
  ratchet=()
  if [ ! -f "$path" ]; then
    printf 'FAIL lint-shell: %s is missing; its ratchet cannot be evaluated.\n' "$path" >&2
    return 1
  fi
  while IFS= read -r line; do
    case "$line" in
      '' | '#'*) continue ;;
    esac
    ratchet+=("$line")
  done <"$path"
  return 0
}

# is_exempt <file> — 0 when the file is on the ratchet just read.
is_exempt() {
  local file="$1" entry
  for entry in "${ratchet[@]+"${ratchet[@]}"}"; do
    [ "$file" = "$entry" ] || continue
    return 0
  done
  return 1
}

# ---------------------------------------------------------------------------
# leg 1 — shellcheck default ruleset over the whole tree.
# ---------------------------------------------------------------------------
if ! "$shellcheck_bin" --severity=style \
  --exclude="SC2312,$DEFERRED_CHECKS" \
  -- "${shell_files[@]}"; then
  printf 'FAIL lint-shell: shellcheck reported findings above (default ruleset, --severity=style).\n' >&2
  rc=1
fi

# ---------------------------------------------------------------------------
# leg 2 — shellcheck SC2312 over the ratcheted file set.
# ---------------------------------------------------------------------------
read_ratchet "$SC2312_RATCHET" || exit 1
sc2312_ratchet=("${ratchet[@]+"${ratchet[@]}"}")

covered=()
for file in "${shell_files[@]}"; do
  is_exempt "$file" || covered+=("$file")
done

if [ "${#covered[@]}" -eq 0 ]; then
  printf 'FAIL lint-shell: %s exempts every tracked shell file; that is a vacuous pass.\n' "$SC2312_RATCHET" >&2
  exit 1
fi

if ! "$shellcheck_bin" --severity=style --include=SC2312 -- "${covered[@]}"; then
  printf 'FAIL lint-shell: SC2312 (masked-return class) findings above.\n' >&2
  printf 'Fix the site — do NOT add the file to %s. That list only shrinks.\n' "$SC2312_RATCHET" >&2
  rc=1
fi

for entry in "${sc2312_ratchet[@]+"${sc2312_ratchet[@]}"}"; do
  case "$tracked_index" in
    *" $entry "*) ;;
    *)
      printf 'FAIL lint-shell: %s lists "%s", which is not a tracked *.sh file. Remove the stale line.\n' \
        "$SC2312_RATCHET" "$entry" >&2
      rc=1
      continue
      ;;
  esac
  # The ratchet may only shrink: a listed file that has become clean must be
  # removed, otherwise the exemption rots into a permanent hole.
  if "$shellcheck_bin" --severity=style --include=SC2312 --format=quiet -- "$entry"; then
    printf 'FAIL lint-shell: %s is now SC2312-clean. Delete its line from %s (the ratchet only shrinks).\n' \
      "$entry" "$SC2312_RATCHET" >&2
    rc=1
  fi
done

# ---------------------------------------------------------------------------
# leg 3 — early-exiting-consumer detector over its own ratcheted file set.
# ---------------------------------------------------------------------------
read_ratchet "$EARLY_CONSUMER_RATCHET" || exit 1
early_ratchet=("${ratchet[@]+"${ratchet[@]}"}")

early_covered=()
for file in "${shell_files[@]}"; do
  is_exempt "$file" || early_covered+=("$file")
done

if [ "${#early_covered[@]}" -eq 0 ]; then
  printf 'FAIL lint-shell: %s exempts every tracked shell file; that is a vacuous pass.\n' "$EARLY_CONSUMER_RATCHET" >&2
  exit 1
fi

early_hit=0
for file in "${early_covered[@]}"; do
  hits="$(detect_early_consumer "$file")"
  [ -n "$hits" ] || continue
  if [ "$early_hit" -eq 0 ]; then
    printf 'FAIL lint-shell: a producer is piped into an early-exiting consumer under pipefail.\n' >&2
    printf 'The consumer (`head`, `grep -q`) closes the pipe while the producer is still\n' >&2
    printf 'writing; the producer dies of SIGPIPE and pipefail poisons the rc. Inside an\n' >&2
    printf '`if` condition `set -e` is suspended, so the guard silently fails OPEN.\n' >&2
    printf 'Use `grep -m1` (the producer stops itself), or read the producer to completion\n' >&2
    printf 'into a variable and test the value.\n' >&2
    early_hit=1
  fi
  printf -- '--- %s\n%s\n' "$file" "$hits" >&2
done
if [ "$early_hit" -ne 0 ]; then
  printf 'Fix the site — do NOT add the file to %s. That list only shrinks.\n' "$EARLY_CONSUMER_RATCHET" >&2
  rc=1
fi

for entry in "${early_ratchet[@]+"${early_ratchet[@]}"}"; do
  case "$tracked_index" in
    *" $entry "*) ;;
    *)
      printf 'FAIL lint-shell: %s lists "%s", which is not a tracked *.sh file. Remove the stale line.\n' \
        "$EARLY_CONSUMER_RATCHET" "$entry" >&2
      rc=1
      continue
      ;;
  esac
  entry_hits="$(detect_early_consumer "$entry")"
  if [ -z "$entry_hits" ]; then
    printf 'FAIL lint-shell: %s no longer pipes into an early-exiting consumer. Delete its line from %s (the ratchet only shrinks).\n' \
      "$entry" "$EARLY_CONSUMER_RATCHET" >&2
    rc=1
  fi
done

exit "$rc"
