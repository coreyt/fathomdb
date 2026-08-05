#!/usr/bin/env bash
# scripts/lib/shell-early-consumer.sh — the detector for THE bug class.
#
# `producer | consumer` under `set -o pipefail`, where the consumer exits early
# (`head`, `grep -q`), lets the consumer close the pipe while the producer is
# still writing. The producer dies of SIGPIPE, `pipefail` makes 141 the rc of
# the whole pipeline, and whether that happens depends on how much output was
# in flight — which is why these sites pass locally and fail in CI. Inside an
# `if` condition it is worse: `set -e` is suspended there, so the poisoned rc
# does not abort, it flips the guard to FALSE and the check fails OPEN.
#
# ⚠ WHY THIS EXISTS SEPARATELY FROM shellcheck (measured 0.8.21 Slice 30, on
# the pinned shellcheck 0.11.0):
#   dev/design/ci-verify-robustness-review.md line 738 asserts that
#   `shellcheck --enable=check-extra-masked-returns` "would have caught
#   [the 2026-08-04 bug] deterministically". IT WOULD NOT. Fed the verbatim
#   pre-fix line from 308f7922 —
#       FIRST_SUITE_LINE="$(grep -nE '…' "$AGENT_TEST" | head -n1 | cut -d: -f1)"
#   — shellcheck 0.11.0 reports NOTHING, under SC2312 or under any of its
#   eleven optional checks. SC2312 fires where a command substitution's status
#   is discarded by the command it is an argument to; here the substitution's
#   status IS the assignment's status, so by SC2312's rule nothing is masked.
#   The same holds for the P0 shape `if git ls-files --unmerged | grep -q .`.
#   Adopting shellcheck is still right, and its SC2312 leg catches a large and
#   overlapping family — but on its own it would NOT have stopped occurrences
#   one through four, and a slice that shipped only shellcheck while believing
#   otherwise would have been a vacuous green about its own purpose.
#
# So the detector below — lifted verbatim from the positive-controlled arm 5 of
# scripts/tests/test_shell_pipefail_guards.sh (0.8.21 Slice 25), where it is
# proven to flag exactly the five pre-fix idioms and neither a comment nor the
# sanctioned `grep -m1` form — is promoted here so ONE definition serves both
# that test and the enforced lint leg in scripts/agent-lint-shell.sh.
#
# `grep -m1` is the sanctioned shape (the 308f7922 fix): grep stops ITSELF, so
# no consumer ever closes the pipe early and there is nothing to race.

# Lines of $1 that pipe into an early-exiting consumer (`head`, or a quiet
# `grep` carrying no -m). This deliberately recognizes both shell pipeline
# operators (`|` and `|&`) and both quiet spellings (`-q` and `--quiet`), even
# when long-form flags precede the pattern. `(^|[^|])\|&?` excludes `||`, which
# is an or-list and not a pipeline; comment lines are excluded so explanatory
# comments may keep naming the idiom they replaced. Empty output = clean.
detect_early_consumer() {
  grep -nE '(^|[^|])\|&?[[:space:]]*(head([[:space:]]|$)|grep([[:space:]]+[^|[:space:]]+)*[[:space:]]+(-[[:alnum:]]*q[[:alnum:]]*|--quiet)([[:space:]]|$))' "$1" \
    | grep -vE '^[0-9]+:[[:space:]]*#' || true
}
