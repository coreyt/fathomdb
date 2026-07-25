#!/usr/bin/env bash
# scripts/lint-design-status.sh — T2c enforcement (DOC-HYGIENE-2).
#
# Sibling of scripts/lint-plans-status.sh, for the OTHER governed doc tier.
# `dev/design/**` is the largest doc surface in the repo (~108 .md / ~1.9 MB at
# the time this landed) and it is the tier a Steward must cite to an
# orchestrator — yet nothing linted it: lint-plans-status.sh is scoped to
# `dev/plans/*.md` TOP-LEVEL ONLY (see its § "Scope") and never reached here.
# 62 of those 108 docs carried no `status:` at all, so "which design docs are
# still current?" could only be answered by reading all of them.
#
# The rule: every `dev/design/**/*.md` (RECURSIVE — subdirectories such as
# record-lifecycle-protocol/ and fathomdb-memex-overall-roadmap/ are in scope)
# must carry YAML frontmatter with a `status:` key. A doc whose status is
# SUPERSEDED must additionally carry a `superseded_by:` pointing somewhere — a
# supersession that names no successor is a dead end for the next reader.
#
# WHAT THIS GATE DOES AND DOES NOT PROVE (HITL 2026-07-25, todos TC-50):
# it proves the PRESENCE of a status field, never its TRUTH. The backfill that
# accompanied it deliberately defaulted to UNREVIEWED, because a doc wrongly
# marked ACTIVE is worse than one with no marker. The real classification is a
# separate owed slice (TC-50). Do not read a green run here as "the statuses
# are correct".
#
# UNREVIEWED vs UNKNOWN — near-synonyms, kept distinct on purpose:
#   UNKNOWN    (inherited from lint-plans-status.sh) = someone LOOKED and the
#              true status genuinely could not be sourced from the master's §4
#              allocation table or the doc's own banners. An honest dead end.
#   UNREVIEWED (added here) = nobody has classified this doc yet. The TC-50
#              backfill default. It is a queue marker, not a verdict.
# A doc moving UNREVIEWED -> UNKNOWN is progress (it was examined); the reverse
# is not. Flagged to the Steward as a candidate for consolidation once TC-50
# has drained the UNREVIEWED backlog.
#
# LEGACY STATUS VALUES (frozen inventory, monotone ratchet):
# 46 design docs predate this gate with free-form values — `locked` (the 0.8.0
# architecture specs), `accepted`, `decision-ready`, `SIGNED (...)`, `PROPOSAL —
# awaiting HITL review (...)` and so on. None of them fit the governed
# vocabulary. Rewriting them here would BE the TC-50 classification this tranche
# is explicitly forbidden to attempt, and would destroy real signal, so they are
# grandfathered by VALUE (leading token, case-insensitive), not by filename:
# lint-plans-status.sh's own header records why a filename carve-out is "exactly
# the shape of gate that rots", and that reasoning applies here too — the rule
# below is total and name-blind.
# The grandfathering is bounded by a ratchet: LEGACY_BUDGET is a CEILING that
# must never rise. A new doc using a legacy value pushes the count over the
# ceiling and fails. As TC-50 retires legacy values the count drops BELOW the
# ceiling — and that fails too, naming the new number, so the budget must be
# lowered in the same change that lowered the count. The count and the ceiling
# are therefore always equal, and both can only ever fall.
#
# Zero-discovery hard fail (TC-37, this repo's named vacuous-pass class): if the
# scan finds NO files it fails loudly rather than exiting 0. A gate that silently
# passes because its scope evaporated (a moved directory, a broken glob) is the
# exact failure mode TC-37 is named for.
#
# Scope: dev/design/**/*.md ONLY. Nothing outside dev/design/ is read.
#
# Pass: silent, exit 0. Fail: one line per offending file + nonzero exit.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The governed vocabulary for dev/design/**. Deliberately its OWN constant:
# lint-plans-status.sh's ALLOWED_RE stays byte-identical (it does NOT admit
# UNREVIEWED — dev/plans has no unclassified backlog and must not grow one).
ALLOWED_RE='^(ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN|UNREVIEWED)$'

# Frozen at DOC-HYGIENE-2 T2c (2026-07-25). Leading token, case-insensitive.
# DO NOT EXTEND — see the LEGACY block in the header. TC-50 retires these.
LEGACY_RE='^(accepted|adopted|analysis|decision-ready|design-note|draft|implementation|implementation-notes|locked|open|proposal|proposed|ratified|signed|tracked)$'
LEGACY_BUDGET=46

FAIL=0
SCANNED=0
LEGACY_SEEN=0

# find (not a glob): the scan must be RECURSIVE, unlike the top-level-only
# dev/plans scope. -print0 so a path with whitespace cannot split.
while IFS= read -r -d '' f; do
  SCANNED=$((SCANNED + 1))

  first_line="$(head -n1 "$f")"
  if [ "$first_line" != "---" ]; then
    printf 'FAIL %s: no YAML frontmatter (must open with a `---` block and carry\n' "$f" >&2
    printf '  status: ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN|UNREVIEWED — use UNREVIEWED\n' >&2
    printf '  if you are not certain; a wrong ACTIVE is worse than an honest UNREVIEWED)\n' >&2
    FAIL=1
    continue
  fi

  # The opening `---` must actually be CLOSED. Without this check the awk
  # below (which prints "until the next `---`, or EOF") reads the WHOLE
  # document as the frontmatter block, so a `status:` line sitting in the prose
  # satisfies a file that has no valid frontmatter at all — the gate is
  # bypassable exactly for the malformed docs it exists to catch.
  # First `---` at NR>1 wins, so a later horizontal rule in the body is
  # irrelevant: it can only ever appear after the real closing delimiter.
  if [ -z "$(awk 'NR>1 && /^---$/{print "closed"; exit}' "$f")" ]; then
    printf 'FAIL %s: unterminated YAML frontmatter — the block opens with `---` but is\n' "$f" >&2
    printf '  never closed. Add the closing `---`; until then a `status:` line in the body\n' >&2
    printf '  does NOT count as frontmatter.\n' >&2
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
  if [ -z "$value" ]; then
    printf 'FAIL %s: `status:` key is present but empty\n' "$f" >&2
    FAIL=1
    continue
  fi

  if grep -qE "$ALLOWED_RE" <<<"$value"; then
    # Governed value. SUPERSEDED must name a successor.
    if [ "$value" = "SUPERSEDED" ]; then
      superseded_by="$(printf '%s\n' "$block" | grep -E '^superseded_by:[[:space:]]*' | head -n1 || true)"
      target="$(printf '%s\n' "$superseded_by" | sed -E 's/^superseded_by:[[:space:]]*//; s/[[:space:]]+$//')"
      if [ -z "$target" ]; then
        printf 'FAIL %s: status SUPERSEDED requires a non-empty `superseded_by:` key\n' "$f" >&2
        printf '  (a supersession that names no successor is a dead end for the next reader)\n' >&2
        FAIL=1
      fi
    fi
    continue
  fi

  # Not governed — is it one of the frozen legacy values?
  leading="$(printf '%s\n' "$value" | awk '{print $1}')"
  if grep -qiE "$LEGACY_RE" <<<"$leading"; then
    LEGACY_SEEN=$((LEGACY_SEEN + 1))
    continue
  fi

  printf 'FAIL %s: status %s is not one of ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN|UNREVIEWED\n' "$f" "$value" >&2
  FAIL=1
done < <(find dev/design -type f -name '*.md' -print0 | sort -z)

# TC-37 vacuous-pass guard.
if [ "$SCANNED" -eq 0 ]; then
  printf 'FAIL lint-design-status: scanned 0 files under dev/design/**/*.md.\n' >&2
  printf '  A gate that finds nothing must never report a pass (TC-37). Either the\n' >&2
  printf '  directory moved, or this script is being run outside the repo it lints.\n' >&2
  exit 1
fi

# Monotone legacy ratchet. Ceiling only: it may fall, never rise.
if [ "$LEGACY_SEEN" -gt "$LEGACY_BUDGET" ]; then
  printf 'FAIL lint-design-status: %d docs carry a pre-gate legacy status value, over the\n' "$LEGACY_SEEN" >&2
  printf '  frozen ceiling of %d. The legacy vocabulary is grandfathered, NOT open: a new\n' "$LEGACY_BUDGET" >&2
  printf '  doc must use ACTIVE|COMPLETE|PROPOSED|SUPERSEDED|UNKNOWN|UNREVIEWED.\n' >&2
  FAIL=1
elif [ "$LEGACY_SEEN" -lt "$LEGACY_BUDGET" ]; then
  # Below-budget FAILS rather than merely advising. An advisory left the stale
  # ceiling in place, so a later change could re-add legacy statuses back up to
  # the old number for free — a ceiling that never falls is just a constant,
  # not a ratchet. Failing here forces the budget down in the SAME change that
  # retires the docs, which is what makes the mechanism one-way.
  printf 'FAIL lint-design-status: legacy count is now %d, ceiling is %d — lower `LEGACY_BUDGET`\n' "$LEGACY_SEEN" "$LEGACY_BUDGET" >&2
  printf '  to %d in scripts/lint-design-status.sh in this same change. The ratchet is one-way:\n' "$LEGACY_SEEN" >&2
  printf '  leaving the ceiling high would let a later change re-add legacy statuses for free.\n' >&2
  FAIL=1
fi

exit "$FAIL"
