#!/usr/bin/env bash
# scripts/lint-plans-status.sh — T3/8 enforcement (DOC-HYGIENE-1).
#
# Every top-level `dev/plans/*.md` file must carry YAML frontmatter with a
# `status:` key whose value is one of ACTIVE | COMPLETE | PROPOSED | SUPERSEDED
# | UNKNOWN — machine-readable so an agent can filter release plans without
# reading each one. This is the recurrence guard for that convention (T3/9): a
# plan that lands without a status, or with a value outside the allowed set,
# fails here instead of silently drifting (the same "fix the tooling, not the
# people" reasoning as TC-37).
#
# UNKNOWN (T3 fix-1, HITL 2026-07-24): reserved for a doc whose true status
# genuinely cannot be sourced from the master's §4 allocation table or its own
# banners — an honest "we don't know" beats guessing. Every UNKNOWN carries a
# matching `found_not_fixed` entry in whichever closure JSON introduced it.
#
# Scope: dev/plans/*.md ONLY (top-level).
#   - NOT dev/plans/runs/**   — slice logs / status boards / run artifacts, not plans.
#   - NOT dev/plans/prompts/** — one-shot execution prompts, not plans.
# See dev/plans/README.md for the directory-split rationale.
#
# Pass: silent, exit 0. Fail: one line per offending file + nonzero exit.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

ALLOWED_RE='^(ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN)$'
FAIL=0

# The first line of $2 matching ERE $1; EMPTY only when there is genuinely no
# match. Replaces `printf '%s\n' "$block" | grep -E … | head -n1 || true`: the
# `|| true` converted a SIGPIPE from the early-exiting `head` (or any grep
# error) into "", which this linter then reports as "missing a `status:` key" —
# a wrong verdict, which is worse than an abort. `grep -m1` stops the producer
# itself, and rc 0/1 is classified while anything else returns non-zero and
# aborts under `set -e`. Kept byte-parallel with lint-design-status.sh's copy
# for the same reason ALLOWED_RE is duplicated rather than shared.
first_matching_line() {
  local re="$1" text="$2" hit rc
  set +e
  hit="$(grep -m1 -E -e "$re" <<<"$text")"
  rc=$?
  set -e
  case "$rc" in
    0) printf '%s' "$hit" ;;
    1) : ;;
    *)
      printf 'lint-plans-status: grep failed (rc=%d) matching `%s`\n' "$rc" "$re" >&2
      return "$rc"
      ;;
  esac
}

# No exceptions: the rule is total over dev/plans/*.md (top-level). An earlier
# revision carved out dev/plans/plan-0.8.20.md while it was under a "do not
# touch" hard constraint from the commissioning brief; that constraint was
# lifted (DOC-HYGIENE-1 T3 fix-1) once plan-0.8.20.md got its frontmatter
# (status: ACTIVE), and the carve-out was removed with it — a lint with a
# hardcoded exception for one live plan is exactly the shape of gate that rots.

shopt -s nullglob
for f in dev/plans/*.md; do
  first_line="$(head -n1 "$f")"
  if [ "$first_line" != "---" ]; then
    printf 'FAIL %s: no YAML frontmatter (must open with a `---` block and carry\n' "$f" >&2
    printf '  status: ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN)\n' >&2
    FAIL=1
    continue
  fi

  # Frontmatter block = the lines strictly between the first `---` and the
  # next line that is exactly `---`.
  block="$(awk 'NR==1{next} /^---$/{exit} {print}' "$f")"
  status_line="$(first_matching_line '^status:[[:space:]]*' "$block")"

  if [ -z "$status_line" ]; then
    printf 'FAIL %s: frontmatter present but missing a `status:` key\n' "$f" >&2
    FAIL=1
    continue
  fi

  value="$(printf '%s\n' "$status_line" | sed -E 's/^status:[[:space:]]*//; s/[[:space:]]+$//')"
  if ! grep -qE "$ALLOWED_RE" <<<"$value"; then
    printf 'FAIL %s: status %s is not one of ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN\n' "$f" "$value" >&2
    FAIL=1
  fi
done
shopt -u nullglob

exit "$FAIL"
