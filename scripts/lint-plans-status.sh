#!/usr/bin/env bash
# scripts/lint-plans-status.sh — T3/8 enforcement (DOC-HYGIENE-1).
#
# Every top-level `dev/plans/*.md` file must carry YAML frontmatter with a
# `status:` key whose value is one of ACTIVE | COMPLETE | PROPOSED | SUPERSEDED
# — machine-readable so an agent can filter release plans without reading each
# one. This is the recurrence guard for that convention (T3/9): a plan that
# lands without a status, or with a value outside the allowed set, fails here
# instead of silently drifting (the same "fix the tooling, not the people"
# reasoning as TC-37).
#
# Scope: dev/plans/*.md ONLY (top-level).
#   - NOT dev/plans/runs/**   — slice logs / status boards / run artifacts, not plans.
#   - NOT dev/plans/prompts/** — one-shot execution prompts, not plans.
# See dev/plans/README.md for the directory-split rationale.
#
# Pass: silent, exit 0. Fail: one line per offending file + nonzero exit.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

ALLOWED_RE='^(ACTIVE|COMPLETE|PROPOSED|SUPERSEDED)$'
FAIL=0

# EXCEPTION (single, explicit, documented — not a general carve-out mechanism):
# dev/plans/plan-0.8.20.md is the live in-flight release board. DOC-HYGIENE-1
# T3 landed while 0.8.20 is between slices and is explicitly forbidden from
# touching plan-0.8.20.md / any 0.8.20-* artifact (F-7 collision rule — a wide
# docs diff must not race a live release orchestrator's own edits to its own
# board). Backfill this file's frontmatter at 0.8.20 close, mirroring the
# already-landed plan-0.8.19.md / plan-0.8.21.md shape.
SKIP_FILES=("dev/plans/plan-0.8.20.md")

shopt -s nullglob
for f in dev/plans/*.md; do
  skip=0
  for s in "${SKIP_FILES[@]}"; do
    [ "$f" = "$s" ] && skip=1 && break
  done
  [ "$skip" -eq 1 ] && continue

  first_line="$(head -n1 "$f")"
  if [ "$first_line" != "---" ]; then
    printf 'FAIL %s: no YAML frontmatter (must open with a `---` block and carry\n' "$f" >&2
    printf '  status: ACTIVE|COMPLETE|PROPOSED|SUPERSEDED)\n' >&2
    FAIL=1
    continue
  fi

  # Frontmatter block = the lines strictly between the first `---` and the
  # next line that is exactly `---`.
  block="$(awk 'NR==1{next} /^---$/{exit} {print}' "$f")"
  status_line="$(printf '%s\n' "$block" | grep -E '^status:[[:space:]]*' | head -n1 || true)"

  if [ -z "$status_line" ]; then
    printf 'FAIL %s: frontmatter present but missing a `status:` key\n' "$f" >&2
    FAIL=1
    continue
  fi

  value="$(printf '%s\n' "$status_line" | sed -E 's/^status:[[:space:]]*//; s/[[:space:]]+$//')"
  if ! grep -qE "$ALLOWED_RE" <<<"$value"; then
    printf 'FAIL %s: status %s is not one of ACTIVE|COMPLETE|PROPOSED|SUPERSEDED\n' "$f" "$value" >&2
    FAIL=1
  fi
done
shopt -u nullglob

exit "$FAIL"
