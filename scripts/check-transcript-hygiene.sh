#!/usr/bin/env bash
# check-transcript-hygiene.sh — the TC-86 landing gate: no tracked file in this
# PUBLIC repository may carry a home-anchored path into ANOTHER project's Claude
# Code state directory, because such a line is raw agent SESSION TRANSCRIPT
# content.
#
# TC-86, steward `seq-129` (raised) + `seq-130` (threat model ruled), todos
# `TC-86`, master `F-36`. Cross-cutting — no slice number, no `R-20-xx` id, no
# pico label.
#
# ======================= THE RULED THREAT MODEL (seq-130) ====================
# FOREIGN project state  -> HARD FAIL (exit 1). Another project's session state
#                           is never legitimately in this repository. After the
#                           fix-1 widening the class is closed completely,
#                           INCLUDING the `tool-results/` shape.
# THIS repo's own state  -> WARN (exit 0). This repository's README, hand-off
#                           prompts and prune measurements cite paths into this
#                           project's own memory store deliberately; a gate that
#                           hard-failed them would be switched off.
#
# The self-exemption is DELIBERATELY VISIBLE: this script prints
# agent_state_self_exemption_notice on EVERY run, clean or failing, naming the
# exempted directory, the reason, and the ACCEPTED RESIDUAL (this repo's own
# session state, tool-results/ content included, can still be committed; the WARN
# lines are the only thing that surfaces it). `seq-130` explicitly rejected an
# exemption hidden inside the regex, because such an exemption is invisible at
# the only moment it matters — when a reader takes the green output to mean the
# tree is clean. Nothing suppresses the warnings; there is no quiet flag.
# The exemption's argument lives at its definition site,
# scripts/lib/agent-state-paths.sh.
#
# Shared by two callers, exactly like its siblings scripts/check-ledgers.sh,
# scripts/check-board-currency.sh, scripts/check-governed-surface-pin.sh and
# scripts/check-c1-conformance.sh:
#   * scripts/preflight.sh --landing        (PREVENT, land-time gate)
#   * .github/workflows/ci.yml transcript-hygiene job
#                                           (DETECT, always-on backstop)
# Reuse, not reimplementation: both callers invoke THIS script, so the predicate
# cannot diverge between the two homes. CI is the LOAD-BEARING home — a
# pre-commit hook alone is bypassable with `--no-verify`, and the incident this
# gate exists for was noticed by a human reading a verdict at the end of a
# multi-megabyte file.
#
# ============================== WHAT HAPPENED ================================
# On 2026-07-28 a codex §9 review transcript arrived carrying 216 lines of raw
# Claude Code session JSONL. codex, running under
# --dangerously-bypass-approvals-and-sandbox (the flag is what lets it read
# outside the repo at all), had run `rg` across the user's ~/.claude state
# directory and slurped the results into its own stdout, which TC-RUBRIC-7 then
# required be `tee`d to a file under dev/plans/runs/codex/ — a TRACKED path.
# Those lines held conversation content from three projects other than this one,
# and github.com/coreyt/fathomdb is PUBLIC.
#
# It was caught and redacted before landing, and `git grep` proved zero
# already-committed files carry that shape: REACHABILITY IN HISTORY IS ZERO, so
# nothing needed scrubbing and no history was rewritten.
#
# The mechanism is STRUCTURAL, not a one-off: every §9 review runs with the
# bypass flag, TC-RUBRIC-7 REQUIRES the transcript be persisted under a tracked
# path, and nothing in between inspects the contents. So it is fixed in the
# tooling (`guardrail-failures-fix-tooling-not-people`), in two layers:
#   Layer 1, CAPTURE TIME — dev/agent-tools/codex-nostdin.sh filters codex's
#            stdout, so the line never reaches the transcript file at all.
#   Layer 2, LANDING TIME — this gate, for anything the filter did not see (a
#            transcript captured some other way, a hand-pasted excerpt, a
#            reviewer substituted per the hand-off's §6 fallback).
# Both layers read ONE pattern, defined once in scripts/lib/agent-state-paths.sh.
# See that file for the pattern's two discriminators (the `/home/`|`/Users/`
# absolute prefix and the leading `-` on the encoded project directory), why each
# is load-bearing, and why the fix-1 widening dropped the original third one; do
# NOT restate the regex here.
#
# Fix-2 does NOT add a third discriminator, and that is the point. The FOREIGN /
# OWN split above is applied AFTER the pattern matches, by comparing the matched
# project directory to one named constant — it is a SEVERITY axis, not a matching
# rule. Folding it into the regex would have made the exemption invisible, which
# is precisely what `seq-130` rejected. The pattern still matches everything it
# matched at fix-1; what changed is what the gate DOES with a match.
#
# ============ WHAT THIS GATE DOES *NOT* DO (read this) =======================
# It is a HYGIENE gate for a known ACCIDENT SHAPE, not a secrets scanner and not
# an adversarial control:
#   * It finds session content that arrived WITH its home-anchored path prefix —
#     the `rg`/`ls`/`find` output shape by which a bypass-sandboxed reviewer's
#     tool output carries another project's transcript into this repo. Content
#     pasted WITHOUT that prefix is outside its reach, by construction. (That is
#     a real gap, not a theoretical one: see `--redact`'s CONTENT-BLOCK mode in
#     scripts/lib/agent-state-paths.sh, which exists because the 0.8.14 review
#     transcript carries dumped memory-store CONTENT that this predicate cannot
#     see. Remediation reaches further than detection, on purpose.)
#   * It does not hard-check this repository's OWN project directory. That is a
#     stated, printed exemption with a stated residual — see above.
#   * It does not detect credentials, tokens, PII, or any other class of secret.
#     Its silence is NOT a statement that a file is safe to publish; it is a
#     statement about exactly one mechanical shape.
#   * It does not judge whether a transcript SHOULD exist. TC-RUBRIC-7 says it
#     must, and `--redact` exists so that requirement and this one can both hold.
#
# ============================ SCOPE OF THE SCAN ==============================
# ALL TRACKED FILES by default — not just dev/plans/runs/**. A transcript written
# somewhere unexpected is exactly the case a runs/-only scope would miss, and the
# pattern is narrow enough that ordinary prose does not trip it. No narrowing to
# dev/plans/runs/** was needed and none was applied. (A new always-on CI job that
# is red on arrival is the TC-16 / F-30 failure this repo is still carrying;
# shipping another would be worse than shipping nothing.)
#
# GREEN ON THE TREE, HONESTLY STATED. The FOREIGN class has ZERO matches across
# every tracked file, which is why this gate can be always-on. The OWN class has
# SIXTEEN, in eleven files, all of them deliberate citations of this project's own
# memory store. Those are printed as WARN on every run; the earlier claim that the
# pattern had "zero matches across every tracked file" was true only of the
# narrower pre-fix-1 pattern and is corrected here rather than left standing.
#
# ============================= FIXING A FAILURE ==============================
# `--redact` REWRITES in place, using the shared banner and markers. REDACT,
# NEVER DELETE: TC-RUBRIC-7 closes a review on a persisted terminal artifact, so
# the transcript has to survive as evidence with the removal VISIBLE. Deleting
# the file, or silently dropping the lines, trades one integrity problem for
# another. Two modes, both in scripts/lib/agent-state-paths.sh:
#   * FOREIGN PATH LINES  — whole-line replacement (the incident shape: the rest
#                           of such a line is the slurped content itself).
#   * CONTENT BLOCKS      — the OUTPUT of an echoed command that read a Claude
#                           Code state directory, foreign or own. The echo is
#                           KEPT; only its output goes.
# OWN-project path lines are NOT rewritten: the exemption covers paths, and
# auto-rewriting them would gut this repo's own docs behind a banner.
#
# Usage:
#   scripts/check-transcript-hygiene.sh                 # all tracked files
#   scripts/check-transcript-hygiene.sh --redact        # ... and clean them
#   scripts/check-transcript-hygiene.sh --root <dir>    # every file under <dir>
#   scripts/check-transcript-hygiene.sh <path>...       # exactly these files
#
# --root/<path> exist so the fixture suite can point at COPIES under `mktemp -d`
# (scripts/tests/test_check_transcript_hygiene.sh); both real callers invoke this
# script with no arguments, or with --redact alone.
#
# There is deliberately NO quiet/suppress option. A warning that can be turned
# off is a warning that will be turned off, and the own-project WARN is the only
# thing that surfaces the accepted residual.
#
# Exit codes:
#   0  no FOREIGN agent-state path (own-project hits, if any, are WARN)
#   1  at least one file carries a FOREIGN agent-state path
#   2  the checker could not run (bad usage, unreadable path, no shared pattern,
#      not a git repository). ANTI-FAIL-OPEN: a gate that could not see its
#      subject must never be mistaken for a gate that saw a clean subject.

set -euo pipefail

SELF="$(basename "${BASH_SOURCE[0]}")"
SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHARED_LIB="$SELF_DIR/lib/agent-state-paths.sh"

# ANTI-FAIL-OPEN #1. Without the shared pattern this script has nothing to
# assert, and exiting 0 would certify an unexamined tree.
if [ ! -f "$SHARED_LIB" ]; then
  printf '%s: cannot read the shared pattern %s — refusing to certify anything\n' \
    "$SELF" "$SHARED_LIB" >&2
  exit 2
fi
# shellcheck source=lib/agent-state-paths.sh
. "$SHARED_LIB"

usage() {
  cat <<EOF
Usage: scripts/$SELF [--redact] [--root <dir>] [--] [PATH...]

  --redact       rewrite matched lines in place (banner + marker), never delete
  --root <dir>   scan every regular file under <dir> (fixture mode)
  PATH...        scan exactly these files
  (no arguments) scan every TRACKED file of the enclosing git checkout

Exit: 0 clean, 1 agent-state paths found, 2 the checker could not run.
EOF
}

REDACT=0
ROOT=""
EXPLICIT=()

while [ $# -gt 0 ]; do
  case "$1" in
    --redact)     REDACT=1; shift ;;
    --root)       ROOT="${2:?--root needs a directory}"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    --)           shift; while [ $# -gt 0 ]; do EXPLICIT+=("$1"); shift; done ;;
    -*)           printf '%s: unknown option %q\n\n' "$SELF" "$1" >&2; usage >&2; exit 2 ;;
    *)            EXPLICIT+=("$1"); shift ;;
  esac
done

if [ -n "$ROOT" ] && [ "${#EXPLICIT[@]}" -gt 0 ]; then
  printf '%s: --root and explicit PATHs are mutually exclusive\n' "$SELF" >&2
  exit 2
fi

# ------------------------------------------------------------------ file list --
FILES=()
SCOPE_DESC=""

if [ "${#EXPLICIT[@]}" -gt 0 ]; then
  SCOPE_DESC="${#EXPLICIT[@]} explicitly named file(s)"
  for p in "${EXPLICIT[@]}"; do
    # An unreadable path is an ERROR, never a silent pass (anti-fail-open #2).
    if [ ! -f "$p" ] || [ ! -r "$p" ]; then
      printf '%s: cannot read %s — refusing to report a tree it could not scan\n' "$SELF" "$p" >&2
      exit 2
    fi
    FILES+=("$p")
  done
elif [ -n "$ROOT" ]; then
  if [ ! -d "$ROOT" ]; then
    printf '%s: --root %s is not a directory\n' "$SELF" "$ROOT" >&2
    exit 2
  fi
  while IFS= read -r -d '' p; do FILES+=("$p"); done \
    < <(find "$ROOT" -path '*/.git' -prune -o -type f -print0)
  SCOPE_DESC="every file under $ROOT"
else
  if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    printf '%s: not inside a git repository and no --root/PATH given\n' "$SELF" >&2
    exit 2
  fi
  while IFS= read -r -d '' p; do FILES+=("$REPO_ROOT/$p"); done \
    < <(git -C "$REPO_ROOT" ls-files -z)
  SCOPE_DESC="all tracked files"
  # ANTI-FAIL-OPEN #3: an EMPTY tracked-file set means the scan evaporated (a
  # broken checkout, a wrong cwd), not that the repo is clean.
  if [ "${#FILES[@]}" -eq 0 ]; then
    printf '%s: git ls-files reported ZERO tracked files under %s — the scan evaporated\n' \
      "$SELF" "$REPO_ROOT" >&2
    exit 2
  fi
fi

# ---------------------------------------------------------------------- scan --
# Two passes on purpose: one batched `grep -l` over everything (fast, and `-I`
# drops binaries), then a per-file CLASSIFICATION only for the files that hit.
#
# collect_hits takes the pattern, because REPORTING and REDACTION do not use the
# same candidate set and conflating them is a real bug, not a tidiness point:
#   * REPORTING scans for AGENT_STATE_PATH_RE — the gate's predicate, unchanged.
#   * REDACTION scans for AGENT_STATE_PROJECTS_DIR_RE, the strictly broader bare
#     state-directory prefix, because a foreign project INVENTORY
#     (`ls ~/.claude/projects` and its output) contains no path-shaped line at
#     all. Under the narrow set such a file is never even opened by --redact.
#     Widening the CANDIDATE set is safe where widening the PREDICATE is not:
#     redact_file is a no-op unless it actually finds something to remove, so a
#     doc that merely names the directory is scanned and left byte-identical.
HITS=()
collect_hits() {
  local pattern="${1:-$AGENT_STATE_PATH_RE}"
  HITS=()
  [ "${#FILES[@]}" -gt 0 ] || return 0
  while IFS= read -r -d '' p; do
    [ -n "$p" ] && HITS+=("$p")
  done < <(printf '%s\0' "${FILES[@]}" \
             | xargs -0 --no-run-if-empty grep -IlZE -e "$pattern" -- 2>/dev/null \
             || true)
}

# -------------------------------------------------------------------- redact --
# redact_file <path> — returns 0 if the file was rewritten, 1 if it was a no-op.
#
# A NO-OP IS THE COMMON CASE AND IT MATTERS. Eleven tracked files carry
# own-project path citations (this repo's README, its hand-off prompts, its prune
# measurements). `--redact` must leave those BYTE-IDENTICAL: the `seq-130`
# exemption covers own-project PATHS, and rewriting them behind a banner would be
# worse than the warning it replaced. Only two things are removed — foreign path
# lines, and the OUTPUT of a command that read a Claude Code state directory.
redact_file() {
  local p="$1"
  local tmp1="$p.tc86-redact1.$$" tmp2="$p.tc86-redact2.$$" cf="$p.tc86-counts.$$"
  local content_lines=0 blocks=0 fnames=0 path_lines=0 total=0 own=0 dirs projects

  agent_state_redact_content_blocks "$p" "$tmp1" "$cf"
  read -r content_lines blocks fnames <"$cf" || { content_lines=0; blocks=0; fnames=0; }
  rm -f "$cf"

  # Foreign path lines are counted on the CONTENT-REDACTED stream: a foreign path
  # that sat inside a removed output block is already gone, and counting it in
  # both totals would put a wrong number in the banner.
  path_lines="$(agent_state_classify_file "$tmp1" | cut -d' ' -f1)"
  agent_state_redact_stream <"$tmp1" >"$tmp2"

  total=$((content_lines + path_lines))
  if [ "$total" -eq 0 ]; then
    rm -f "$tmp1" "$tmp2"
    return 1
  fi

  dirs="$(agent_state_project_dirs <"$p" || true)"
  projects="$(printf '%s' "$dirs" | paste -sd', ' -)"
  if grep -qxF -- "$AGENT_STATE_OWN_PROJECT_DIR" <<<"$dirs"; then own=1; fi

  {
    agent_state_redaction_banner "$total" "${projects:-(none identified)}" \
      "$path_lines" "$content_lines" "$blocks" "$own" "$fnames"
    cat "$tmp2"
  } >"$tmp1"
  # `cat >` rather than `mv`: rewrites the EXISTING inode, so the file keeps its
  # mode and its identity. The transcript is evidence; it is edited, not replaced.
  cat "$tmp1" >"$p"
  rm -f "$tmp1" "$tmp2"

  printf 'redacted %s line(s) in %s (%s foreign path line(s); %s content line(s) in %s block(s), of which %s foreign project-directory name(s); projects: %s)\n' \
    "$total" "$p" "$path_lines" "$content_lines" "$blocks" "$fnames" "${projects:-(none identified)}"
  return 0
}

if [ "$REDACT" -eq 1 ]; then
  collect_hits "$AGENT_STATE_PROJECTS_DIR_RE"
  REDACTED=0
  for p in "${HITS[@]+"${HITS[@]}"}"; do
    if redact_file "$p"; then REDACTED=$((REDACTED + 1)); fi
  done
  printf 'ok    transcript-hygiene: %d file(s) redacted in place; every transcript still present as TC-RUBRIC-7 evidence\n' \
    "$REDACTED"
fi

# ------------------------------------------------------------------- classify --
# Re-scan AFTER any redaction, so what is reported is the tree as it now stands
# rather than the tree as it was found. This is also what makes `--redact`
# self-verifying: if a redaction failed to converge, the report below says so
# instead of the run ending on an optimistic summary.
collect_hits

FOREIGN_FILES=(); FOREIGN_N=(); OWN_FILES=(); OWN_N=()
FOREIGN_TOTAL=0; OWN_TOTAL=0
for p in "${HITS[@]+"${HITS[@]}"}"; do
  read -r fcount ocount <<<"$(agent_state_classify_file "$p")"
  if [ "${fcount:-0}" -gt 0 ]; then
    FOREIGN_FILES+=("$p"); FOREIGN_N+=("$fcount")
    FOREIGN_TOTAL=$((FOREIGN_TOTAL + fcount))
  fi
  if [ "${ocount:-0}" -gt 0 ]; then
    OWN_FILES+=("$p"); OWN_N+=("$ocount")
    OWN_TOTAL=$((OWN_TOTAL + ocount))
  fi
done

# print_own_warnings — ALWAYS called, on the failing path as well as the clean
# one. A run that reported the foreign failure and returned early would hide the
# own-project residual at exactly the moment the operator is already in the tree.
# These lines must NOT start with FAIL: preflight.sh promotes every FAIL line to
# a HARD fail, and `seq-130` ruled this class advisory.
print_own_warnings() {
  local i
  [ "${#OWN_FILES[@]}" -gt 0 ] || return 0
  for i in "${!OWN_FILES[@]}"; do
    printf "WARN  transcript-hygiene: %s carries %s line(s) into this repo's OWN Claude Code project state (advisory — does not fail this gate)\n" \
      "${OWN_FILES[$i]}" "${OWN_N[$i]}"
  done
  printf 'WARN  transcript-hygiene: %d file(s) carry %d own-project agent-state path line(s) — advisory only, exit 0\n' \
    "${#OWN_FILES[@]}" "$OWN_TOTAL"
}

# ------------------------------------------------------------------ clean path --
if [ "${#FOREIGN_FILES[@]}" -eq 0 ]; then
  printf 'ok    transcript-hygiene: no file carries a FOREIGN agent-state path (%s scanned, %d file(s))\n' \
    "$SCOPE_DESC" "${#FILES[@]}"
  print_own_warnings
  agent_state_self_exemption_notice
  exit 0
fi

# ---------------------------------------------------------------------- fail --
# One FAIL line per offending file, naming the file AND its foreign match count.
# preflight.sh promotes every FAIL line to a HARD fail, so nothing below this
# point may start with FAIL unless it is a real defect.
for i in "${!FOREIGN_FILES[@]}"; do
  printf 'FAIL  transcript-hygiene: %s carries %s line(s) with a FOREIGN agent-state path\n' \
    "${FOREIGN_FILES[$i]}" "${FOREIGN_N[$i]}"
done

printf 'FAIL  transcript-hygiene: %d file(s) carry %d line(s) of another project'"'"'s Claude Code session-transcript paths\n' \
  "${#FOREIGN_FILES[@]}" "$FOREIGN_TOTAL"

print_own_warnings

# QUOTED heredoc delimiter, deliberately: this prose contains backticks and `$`,
# and an UNQUOTED delimiter would run them as command substitutions — which is
# exactly how this block first shipped, printing `ls` output into the middle of a
# security message and a "rg: command not found" error alongside it. Anything
# variable is printf'd above/below instead.
cat <<'EOF'

WHAT THIS MEANS. A line matching the shared agent-state pattern is `rg`/`ls`
output naming a file under a user's ~/.claude state directory — and in the
incident that produced this gate, the rest of that line was the session JSONL
itself: another project's conversation content. github.com/coreyt/fathomdb is a
PUBLIC repository, so committing it publishes it. The FAIL lines above name
ANOTHER project's state, which is never legitimately in this repository.

HOW IT GETS THERE. codex §9 reviews run under
--dangerously-bypass-approvals-and-sandbox, which lets the reviewer read outside
the repo; TC-RUBRIC-7 then requires the terminal transcript be persisted under a
tracked path. Anything the reviewer read can therefore ride into the repo inside
its own transcript.

WHAT TO DO.
  1. Run: scripts/check-transcript-hygiene.sh --redact
     It rewrites the foreign lines IN PLACE, and removes the OUTPUT of any
     echoed command that read a Claude Code state directory, behind a banner
     stating the counts, the projects touched, the reason, and that no review
     finding came from them. It NEVER deletes the file — TC-RUBRIC-7 closes a
     review on a persisted artifact, so the transcript must survive as evidence.
  2. Re-run this gate; it must exit 0.
  3. If the match is NOT session content — i.e. this gate has a FALSE POSITIVE —
     do NOT weaken the pattern to get green, and do NOT add a second exemption.
     Take it to the STEWARD: the pattern and the one stated exemption live in
     scripts/lib/agent-state-paths.sh and are argued there, and a hygiene gate
     quietly loosened to clear a land is how this class comes back.

DO NOT land this tree, and do not push it, until this exits 0.
EOF

agent_state_self_exemption_notice
exit 1
