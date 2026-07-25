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

cd "$(git rev-parse --show-toplevel)"

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

if [ "$FAIL" -eq 0 ]; then
  printf 'ok    lint-findings: %s finding ids in %s, all unique\n' "$COUNT" "$FILE" >&2
fi

exit "$FAIL"
