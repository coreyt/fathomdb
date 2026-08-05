#!/usr/bin/env bash
# scripts/lint-findings.sh — T1c enforcement (DOC-HYGIENE-2).
#
# The master release plan's §6 "Integration findings" is the program's findings
# REGISTER: `F-n` is a stable id that ~39 files, the steward ledger, and every
# release plan cite by number. An id is only worth citing if it resolves to
# exactly one entry. It did not: a duplicate `F-11` sat in §6 from 2026-06-28
# (a superseded first draft retained under the same id when the ruled version
# was appended), so 18 in-file `F-11` pointers resolved ambiguously for four
# weeks. The rename was ruled 2026-07-03 (`c6d3449a`; M18 in
# dev/design/fathomdb-memex-overall-roadmap/00-priorities-and-misalignments.md)
# and never executed — a ruling with no mechanism behind it does not get done.
# This is that mechanism, per the standing "fix the tooling, not the people"
# rule: a second entry minted under a live id fails here instead of silently
# making every citation of that id ambiguous.
#
# SCOPE: the master ONLY (one file). This is deliberate and is NOT a repo-wide
# grep, because two other trees mint their OWN independent `F-0NN` namespaces
# that legitimately collide with this register's numbers and with each other:
#   - dev/archive/**            — closed historical records, frozen.
#   - dev/plans/runs/codex/**   — per-review codex §9 finding ids, minted fresh
#                                 per transcript (every review has an F-001).
# Both are also already outside the markdownlint scope (.markdownlint-cli2.jsonc).
# Widening this gate to either tree would make it permanently and falsely red.
#
# WHAT IT CHECKS: exactly one thing — id uniqueness. No schema, no title format,
# no required fields. A register entry is recognized in either shape:
#   - `### F-n — …`      the promoted heading form (T1c and after)
#   - `- **F-n — …`      the legacy flat-bullet form (any indent; pre-T1c, and
#                        still the shape of the F-8a/F-8b sub-findings)
# Both shapes are matched so the gate cannot be defeated by reverting an entry
# to a bullet, and so it works on any historical revision of the master.
#
# VACUOUS-PASS GUARD (TC-37 class — the repo's named failure mode: a gate that
# reports green without having run). If the scan finds ZERO entries — a moved
# section, a renamed file, a changed entry convention, a wrong --file — the
# duplicate loop never executes, no duplicate is possible, and the script would
# exit 0 having vouched for nothing. That is a silent vacuous pass, so it is
# itself a HARD failure. It can only convert a silent pass into a loud failure;
# it never fires when >=1 entry is found.
#
# Usage:
#   scripts/lint-findings.sh [--file <path>]
#
# Exit codes: 0 = every finding id in the register is unique (and >=1 was found);
#             1 = a duplicate id, OR zero findings found (vacuous-pass guard);
#             2 = bad usage / unreadable file.
set -euo pipefail

# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1

FILE="dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md"

while [ $# -gt 0 ]; do
  case "$1" in
    --file) FILE="${2:?--file needs a path}"; shift 2 ;;
    *) printf 'lint-findings: unknown arg %q\n' "$1" >&2; exit 2 ;;
  esac
done

if [ ! -r "$FILE" ]; then
  printf 'lint-findings: %s is not readable from %s\n' "$FILE" "$PWD" >&2
  exit 2
fi

# Emit "<line-number><TAB><id>" for every register entry, in file order.
# The id is `F-` plus digits plus an optional lowercase suffix (F-11a), so
# `F-110` captures `110` and can never be confused with `F-11` — the greedy
# digit run consumes the whole number before the suffix is considered.
extract_entries() {
  awk '
    {
      id = ""
      if (match($0, /^### F-[0-9]+[a-z]*/)) {
        id = substr($0, RSTART + 4, RLENGTH - 4)
      } else if (match($0, /^[ \t]*- \*\*F-[0-9]+[a-z]*/)) {
        s = substr($0, RSTART, RLENGTH)
        sub(/^[ \t]*- \*\*/, "", s)
        id = s
      }
      if (id != "") printf "%d\t%s\n", NR, id
    }
  ' "$1"
}

declare -A FIRST_LINE=()
COUNT=0
FAIL=0

while IFS=$'\t' read -r lineno id; do
  [ -n "$id" ] || continue
  COUNT=$((COUNT + 1))
  if [ -n "${FIRST_LINE[$id]+x}" ]; then
    printf 'FAIL %s: duplicate finding id %s — first defined at line %s, redefined at line %s.\n' \
      "$FILE" "$id" "${FIRST_LINE[$id]}" "$lineno" >&2
    printf '  A finding id is a citation target; two entries under one id make every\n' >&2
    printf '  reference to %s ambiguous. Give the superseded entry a distinct id\n' "$id" >&2
    printf '  (e.g. %sa, marked historical / superseded-by %s) rather than deleting it.\n' "$id" "$id" >&2
    FAIL=1
  else
    FIRST_LINE[$id]="$lineno"
  fi
done < <(extract_entries "$FILE")

if [ "$COUNT" -eq 0 ]; then
  printf 'FAIL %s: ZERO findings matched — the register scan vouched for nothing.\n' "$FILE" >&2
  printf '  Expected `### F-n — …` headings (or legacy `- **F-n — …` bullets) in §6.\n' >&2
  printf '  Either the file/section moved or the entry convention changed; a gate that\n' >&2
  printf '  cannot see its subject must fail loudly, never report a silent pass (TC-37).\n' >&2
  exit 1
fi

# --- CHECK 2: §6a index reconciles with the finding bodies ---------------
#
# WHY (measured, not hypothetical). On 2026-07-31 a cold-start agent found §6a
# headed "Findings index (all 35, in file order)" while listing 36 entries, with
# F-37 and F-38 carrying full bodies and NO index entry, and F-36 carrying no
# heading at all — it existed only inline in the §4 0.8.20 table row. Three of
# the register's "stable citation targets" were not citable from the index. The
# defect was UNDETECTABLE, not un-noticed: check 1 verifies uniqueness and has no
# opinion on whether the index sees every entry. Per the standing "fix the
# tooling, not the actor" rule, that gap is this check.
#
# WHICH IDS MUST BE INDEXED. Not just headings: a first draft of this check
# reconciled `### F-n` headings only, which exempted EVERY legacy `- **F-n`
# bullet. F-8a/F-8b genuinely must stay unindexed — they are sub-findings inside
# F-8's body — but the carve-out was far wider than that, so a top-level finding
# regressing to bullet form would pass uniqueness, be skipped by reconciliation,
# and go missing from §6a with the gate green (codex, 2026-07-31).
#
# The rule is SUB-finding, not FORM: a bullet id is exempt only if it carries a
# letter suffix AND its numeric stem has its own heading (F-8a under F-8).
# Everything else — heading or bullet — must appear in §6a. F-11a has its own
# heading and is indexed, which this rule preserves.
index_entries() { awk '/^### 6a\./{f=1;next} f&&/^### /{exit} f&&match($0,/^- \[F-[0-9]+[a-z]*/){print substr($0,RSTART+3,RLENGTH-3)}' "$1"; }
heading_ids()   { awk 'match($0,/^### F-[0-9]+[a-z]*/){print substr($0,RSTART+4,RLENGTH-4)}' "$1"; }
bullet_ids()    { awk 'match($0,/^[ \t]*- \*\*F-[0-9]+[a-z]*/){s=substr($0,RSTART,RLENGTH); sub(/^[ \t]*- \*\*/,"",s); print s}' "$1"; }
# Ids that owe an index entry = headings + bullets that are not sub-findings.
indexable_ids() {
  local hd; hd="$(heading_ids "$1")"
  { printf '%s\n' "$hd"
    bullet_ids "$1" | while IFS= read -r b; do
      [ -n "$b" ] || continue
      case "$b" in
        *[a-z]) printf '%s\n' "$hd" | grep -qx "${b%[a-z]}" || printf '%s\n' "$b" ;;
        *)      printf '%s\n' "$b" ;;
      esac
    done
  } | grep -v '^$' | sort -u
}

IDX="$(index_entries "$FILE")"
HDS="$(indexable_ids "$FILE")"
N_IDX=$(printf '%s\n' "$IDX" | grep -c . || true)
N_HDS=$(printf '%s\n' "$HDS" | grep -c . || true)

if [ "$N_IDX" -eq 0 ]; then
  printf 'FAIL %s: §6a findings index matched ZERO entries — the reconciliation vouched for nothing.\n' "$FILE" >&2
  printf '  Expected `### 6a.` followed by `- [F-n — …](#…)` bullets. Either the section\n' >&2
  printf '  moved or the entry shape changed; a gate that cannot see its subject must fail\n' >&2
  printf '  loudly, never report a silent pass (TC-37).\n' >&2
  FAIL=1
else
  MISSING_IDX="$(comm -23 <(printf '%s\n' "$HDS" | sort -u) <(printf '%s\n' "$IDX" | sort -u) | tr '\n' ' ')"
  MISSING_BODY="$(comm -13 <(printf '%s\n' "$HDS" | sort -u) <(printf '%s\n' "$IDX" | sort -u) | tr '\n' ' ')"
  if [ -n "${MISSING_IDX// }" ]; then
    printf 'FAIL %s: finding(s) with a body but NO §6a index entry: %s\n' "$FILE" "$MISSING_IDX" >&2
    printf '  An id advertised as a stable citation target is not citable if the index\n' >&2
    printf '  cannot see it. Add the entry; do not delete the body.\n' >&2
    FAIL=1
  fi
  if [ -n "${MISSING_BODY// }" ]; then
    printf 'FAIL %s: §6a indexes finding(s) with NO body section: %s\n' "$FILE" "$MISSING_BODY" >&2
    printf '  The index points at nothing. Give the finding a real `### F-n — …` section —\n' >&2
    printf '  recording it inline in a §4 table cell is what made F-36 uncitable.\n' >&2
    FAIL=1
  fi
  # A stated count is a second source of truth for something already derivable,
  # and it rotted: "all 35" survived the register reaching 38. Ban it outright.
  if grep -qE '^### 6a\..*[0-9]' "$FILE"; then
    printf 'FAIL %s: the §6a heading states a COUNT. Remove it.\n' "$FILE" >&2
    printf '  A count in the heading is a second source of truth for something the list\n' >&2
    printf '  already is, and it goes stale silently — "all 35" outlived the register\n' >&2
    printf '  reaching 38. A heading that cannot be wrong beats one that is currently right.\n' >&2
    FAIL=1
  fi
fi

if [ "$FAIL" -eq 0 ]; then
  printf 'ok    lint-findings: %s finding ids in %s, all unique; §6a indexes all %s indexable finding(s)\n' \
    "$COUNT" "$FILE" "$N_HDS" >&2
fi

exit "$FAIL"
