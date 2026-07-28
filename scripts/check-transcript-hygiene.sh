#!/usr/bin/env bash
# check-transcript-hygiene.sh — the TC-86 landing gate: no tracked file in this
# PUBLIC repository may carry a home-anchored path into a user's Claude Code
# state directory, because such a line is raw agent SESSION TRANSCRIPT content.
#
# TC-86, steward `seq-129`, todos `TC-86`, master `F-36`. Cross-cutting — no
# slice number, no `R-20-xx` id, no pico label.
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
# See that file for the pattern's three discriminators and why each is
# load-bearing; do NOT restate the regex here.
#
# ============ WHAT THIS GATE DOES *NOT* DO (read this) =======================
# It is a HYGIENE gate for a known ACCIDENT SHAPE, not a secrets scanner and not
# an adversarial control:
#   * It finds session content that arrived WITH its home-anchored path prefix —
#     the `rg`/`ls`/`find` output shape by which a bypass-sandboxed reviewer's
#     tool output carries another project's transcript into this repo. Content
#     pasted WITHOUT that prefix is outside its reach, by construction.
#   * It does not detect credentials, tokens, PII, or any other class of secret.
#     Its silence is NOT a statement that a file is safe to publish; it is a
#     statement about exactly one mechanical shape.
#   * It does not judge whether a transcript SHOULD exist. TC-RUBRIC-7 says it
#     must, and `--redact` exists so that requirement and this one can both hold.
#
# ============================ SCOPE OF THE SCAN ==============================
# ALL TRACKED FILES by default — not just dev/plans/runs/**. A transcript written
# somewhere unexpected is exactly the case a runs/-only scope would miss, and the
# pattern is narrow enough that ordinary prose does not trip it. That was
# verified EMPIRICALLY, not assumed: at baseline 1cbde587 the pattern has zero
# matches across every tracked file, so this gate is GREEN THE DAY IT LANDS. No
# narrowing to dev/plans/runs/** was needed and none was applied. (A new
# always-on CI job that is red on arrival is the TC-16 / F-30 failure this repo
# is still carrying; shipping another would be worse than shipping nothing.)
#
# ============================= FIXING A FAILURE ==============================
# `--redact` REWRITES the matched lines in place, using the shared banner and
# marker. REDACT, NEVER DELETE: TC-RUBRIC-7 closes a review on a persisted
# terminal artifact, so the transcript has to survive as evidence with the
# removal VISIBLE. Deleting the file, or silently dropping the lines, trades one
# integrity problem for another.
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
# Exit codes:
#   0  clean (or: --redact ran and the tree is now clean)
#   1  at least one file carries an agent-state path
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
# drops binaries), then a per-file count only for the files that actually hit.
HITS=()
if [ "${#FILES[@]}" -gt 0 ]; then
  while IFS= read -r -d '' p; do
    [ -n "$p" ] && HITS+=("$p")
  done < <(printf '%s\0' "${FILES[@]}" \
             | xargs -0 --no-run-if-empty grep -IlZE -e "$AGENT_STATE_PATH_RE" -- 2>/dev/null \
             || true)
fi

if [ "${#HITS[@]}" -eq 0 ]; then
  printf 'ok    transcript-hygiene: no %s carries an agent-state path (%s scanned, %d file(s))\n' \
    'file' "$SCOPE_DESC" "${#FILES[@]}"
  exit 0
fi

# -------------------------------------------------------------------- redact --
if [ "$REDACT" -eq 1 ]; then
  for p in "${HITS[@]}"; do
    count="$(grep -IcE -e "$AGENT_STATE_PATH_RE" -- "$p" || true)"
    projects="$(agent_state_project_dirs <"$p" | paste -sd', ' - || true)"
    tmp="$p.tc86-redact.$$"
    {
      agent_state_redaction_banner "$count" "${projects:-(none identified)}"
      agent_state_redact_stream <"$p"
    } >"$tmp"
    # `cat >` rather than `mv`: rewrites the EXISTING inode, so the file keeps
    # its mode and its identity. The transcript is evidence; it is edited, not
    # replaced.
    cat "$tmp" >"$p"
    rm -f "$tmp"
    printf 'redacted %s line(s) in %s (projects: %s)\n' "$count" "$p" "${projects:-(none identified)}"
  done
  printf 'ok    transcript-hygiene: %d file(s) redacted in place; every transcript still present as TC-RUBRIC-7 evidence\n' \
    "${#HITS[@]}"
  exit 0
fi

# ---------------------------------------------------------------------- fail --
# One FAIL line per offending file, naming the file AND its match count.
# preflight.sh promotes every FAIL line to a HARD fail, so nothing below this
# point may start with FAIL unless it is a real defect.
for p in "${HITS[@]}"; do
  count="$(grep -IcE -e "$AGENT_STATE_PATH_RE" -- "$p" || true)"
  printf 'FAIL  transcript-hygiene: %s carries %s line(s) with an agent-state path\n' "$p" "$count"
done

printf 'FAIL  transcript-hygiene: %d file(s) carry raw Claude Code session-transcript paths\n' \
  "${#HITS[@]}"

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
PUBLIC repository, so committing it publishes it.

HOW IT GETS THERE. codex §9 reviews run under
--dangerously-bypass-approvals-and-sandbox, which lets the reviewer read outside
the repo; TC-RUBRIC-7 then requires the terminal transcript be persisted under a
tracked path. Anything the reviewer read can therefore ride into the repo inside
its own transcript.

WHAT TO DO.
  1. Run: scripts/check-transcript-hygiene.sh --redact
     It rewrites the matched lines IN PLACE with a banner stating the count, the
     projects touched, the reason, and that no review finding came from them. It
     NEVER deletes the file — TC-RUBRIC-7 closes a review on a persisted
     artifact, so the transcript must survive as evidence.
  2. Re-run this gate; it must exit 0.
  3. If the match is NOT session content — i.e. this gate has a FALSE POSITIVE —
     do NOT weaken the pattern to get green. Take it to the STEWARD: the pattern
     lives in scripts/lib/agent-state-paths.sh and its discriminators are argued
     there, and a hygiene gate quietly loosened to clear a land is how this class
     comes back.

DO NOT land this tree, and do not push it, until this exits 0.
EOF
exit 1
