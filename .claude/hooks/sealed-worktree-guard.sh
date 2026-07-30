#!/usr/bin/env bash
# .claude/hooks/sealed-worktree-guard.sh — PreToolUse guard that SEALS an agent
# into one git worktree.
#
# ===========================================================================
# WHY THIS EXISTS (HITL request, 2026-07-30)
# ===========================================================================
# A commissioned orchestrator runs in a linked worktree, but nothing stopped it
# reaching back into the PRIMARY checkout — and "nothing" is not hyperbole:
#
#   * TC-128: a `git init` with GIT_DIR unscrubbed re-initialised whatever
#     GIT_DIR named. It set `core.bare = true` on the PRIMARY repository TWICE
#     on 2026-07-29.
#   * TC-132: `git add -A` swept untracked files from a SIBLING checkout into a
#     commit.
#   * `scripts/check-release-state-views.sh --write` takes paths from a state
#     file; run with the wrong cwd it rewrites documents in another tree.
#
# The HITL asked for a guard "against writing anything to the main checkout,
# EVEN VIA A SCRIPT". A tool allowlist cannot deliver that — a `Bash` grant
# writes any file via `cat >` or a heredoc no matter what Edit/Write allow, and
# a script's writes are invisible to the tool layer entirely. So the boundary
# has to be enforced on PATHS, at the call site, before the process starts.
#
# ===========================================================================
# THE RULE — deliberately blunt, and blunt is the point
# ===========================================================================
# When $FATHOMDB_SEALED_WORKTREE is set, the PRIMARY CHECKOUT PATH MAY NOT
# APPEAR IN A TOOL CALL AT ALL — not as a write target, not as a read, not as a
# `cd`, not inside a longer string.
#
# Why total rather than write-only: a linked worktree is a FULL checkout of the
# same repository, so a sealed agent has a complete copy of every file it could
# legitimately want. It never needs to name the primary for any purpose. That
# makes "the primary path must not appear" a rule with no false-positive cost
# and no parsing ambiguity — and unlike a write-detector, it cannot be defeated
# by a redirection this script failed to anticipate, by an `eval`, or by a
# script three levels down that takes its output path from a config file.
#
# A write-only rule would require this hook to understand shell. It does not,
# and pretending otherwise is how a guard becomes decorative.
#
# ===========================================================================
# WHAT THIS GUARD DOES NOT DO — read this before trusting it
# ===========================================================================
# It is PREVENTION, and prevention here is necessarily heuristic at the edges:
#
#   * It sees the command STRING. A path assembled at runtime from variables
#     ($HOME/projects/$repo), read out of a file, or produced by a subshell is
#     NOT visible to it.
#   * `cd ..` far enough, then a relative write, reaches anywhere. This guard
#     denies obvious upward escapes but cannot enumerate them all.
#
# THEREFORE IT IS NOT THE GUARANTEE. The guarantee is DETECTION:
# `scripts/snapshot-tree.sh` hashes the primary before and after the run, and
# the commissioning Steward compares them. The snapshot is independent of the
# agent, cannot be reasoned around by anything the agent does, and is what the
# acceptance criterion is actually written against. This hook exists to stop
# the accidents; the snapshot exists to prove the absence of all of them.
#
# Anyone tempted to relax this hook to silence a false positive should instead
# widen the sealed root, or ask why a sealed agent wants the primary at all.
#
# ===========================================================================
# ACTIVATION — opt-in, and inert by default
# ===========================================================================
# Silent no-op unless BOTH hold:
#   * $FATHOMDB_SEALED_WORKTREE is set (absolute path to the allowed root), and
#   * $FATHOMDB_PRIMARY_CHECKOUT is set (absolute path to the forbidden root).
# Both must be set when the session/agent is LAUNCHED. A `Bash` tool call cannot
# change the environment the harness spawns this hook in, so a sealed agent
# cannot unseal itself — the same property that makes seat-path-guard.sh's env
# channel safe (see its "SEAT RESOLUTION" note).
#
# Unset => exit 0 in silence, so this file is inert for every ordinary session
# in this checkout and can never break one.
#
# DENY CONTRACT (identical to seat-path-guard.sh): exit 0, plus this exact
# stdout envelope. The deny signal travels in STDOUT, not the exit code.
#   {"hookSpecificOutput":{"hookEventName":"PreToolUse",
#    "permissionDecision":"deny","permissionDecisionReason":"<why>"}}
# Silence on allow: nothing on stdout or stderr unless denying.
set -uo pipefail

SEALED="${FATHOMDB_SEALED_WORKTREE:-}"
PRIMARY="${FATHOMDB_PRIMARY_CHECKOUT:-}"

# --- second activation channel: CLI args from an agent's `hooks:` frontmatter -
# The env channel above works for a session LAUNCHED with the vars set. It does
# NOT work for a SPAWNED subagent: the harness spawns this hook in the PARENT
# session's environment, and the parent (the Steward) must stay unsealed because
# it is the seat that legitimately writes the primary. Sealing the parent to seal
# the child would be exactly backwards.
#
# So a seat definition passes the roots as arguments instead:
#   command: ".../sealed-worktree-guard.sh --sealed <abs> --primary <abs>"
# Args are baked into the committed, reviewable seat file, and a `Bash` tool call
# cannot rewrite the agent frontmatter the harness already loaded — so a sealed
# agent still cannot unseal itself, which is the property that matters.
#
# Env wins when both are present: it is the operator's override.
while [ $# -gt 0 ]; do
  case "$1" in
    --sealed)  [ -n "$SEALED" ]  || SEALED="${2:-}";  shift 2 || shift $# ;;
    --primary) [ -n "$PRIMARY" ] || PRIMARY="${2:-}"; shift 2 || shift $# ;;
    *) shift ;;
  esac
done

# Inert unless explicitly sealed. Never make a decision we were not asked to.
[ -n "$SEALED" ] && [ -n "$PRIMARY" ] || exit 0

RAW="$(cat 2>/dev/null || true)"
[ -n "$RAW" ] || exit 0

emit_deny() {
  # One atomic printf: a partial write is not a valid envelope and would be
  # read as "no decision", i.e. allow.
  local reason="$1"
  reason="${reason//\\/\\\\}"
  reason="${reason//\"/\\\"}"
  reason="${reason//$'\n'/ }"
  reason="${reason//$'\t'/ }"
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$reason"
  exit 0
}

# --- extract the fields, jq first, python3 as the fallback ------------------
# ⚠ The field separator is US (0x1f), NOT tab. A tab IS an IFS whitespace
# character, so `IFS=$'\t' read` COLLAPSES runs of tabs and strips leading ones
# — which silently shifts every field after the first empty one. That bug was
# real in this file: with no `file_path`, the command `git init /tmp/fix` parsed
# as FILE_PATH=git / COMMAND=init and rules 2 and 3 never fired. It was caught
# by the control arms in scripts/tests/test_sealed_worktree_guard.sh, which is
# precisely what they are for. 0x1f is not whitespace, so runs are preserved.
US=$'\x1f'
FIELDS=""
if command -v jq >/dev/null 2>&1; then
  FIELDS="$(printf '%s' "$RAW" | jq -j --arg us "$US" '
    [(.tool_name // ""), (.tool_input.file_path // ""),
     (.tool_input.command // ""), (.tool_input.path // ""),
     (.tool_input.notebook_path // "")] | join($us)' 2>/dev/null || true)"
fi
if [ -z "$FIELDS" ] && command -v python3 >/dev/null 2>&1; then
  FIELDS="$(printf '%s' "$RAW" | python3 -c '
import json,sys
def s(v): return "" if v is None else str(v)
try: d=json.load(sys.stdin)
except Exception: raise SystemExit(0)
ti=d.get("tool_input") or {}
if not isinstance(ti,dict): ti={}
sys.stdout.write("\x1f".join(x.replace("\n"," ") for x in (
  s(d.get("tool_name")), s(ti.get("file_path")), s(ti.get("command")),
  s(ti.get("path")), s(ti.get("notebook_path")))))
' 2>/dev/null || true)"
fi
# If we could not parse the payload we cannot judge it. Fail OPEN here rather
# than deny-all: a parser change must not brick every session in the repo. The
# snapshot is the backstop for exactly this gap.
[ -n "$FIELDS" ] || exit 0

IFS="$US" read -r TOOL FILE_PATH COMMAND PATH_ARG NB_PATH <<<"$FIELDS"

# Normalise the two roots: strip any trailing slash so prefix tests are exact.
PRIMARY="${PRIMARY%/}"
SEALED="${SEALED%/}"

# A worktree nested INSIDE the primary would make the rule self-contradictory.
# Say so loudly rather than guarding nothing.
case "$SEALED/" in
  "$PRIMARY"/*)
    emit_deny "sealed-worktree-guard misconfigured: the sealed worktree ($SEALED) is INSIDE the primary checkout ($PRIMARY), so no coherent boundary exists. Re-cut the worktree outside the primary and relaunch."
    ;;
esac

HAYSTACK="$FILE_PATH $COMMAND $PATH_ARG $NB_PATH"

# --- rule 1: the primary path may not appear, in any tool, for any reason ---
# ⚠ MATCH ON A PATH BOUNDARY, never a bare substring. The sealed worktree here
# lives at .../fathomdb-worktrees/<name>, which CONTAINS the primary path
# .../fathomdb as a plain substring — so a naive `*"$PRIMARY"*` denies every
# legitimate in-worktree write. That bug was real in this file and was caught by
# the control arm "in-worktree write must ALLOW". A match therefore requires the
# primary to be followed by `/`, or to end the token (end-of-string, space, or a
# shell metacharacter).
PRIMARY_HIT=no
case "$HAYSTACK" in
  *"$PRIMARY"/*)                       PRIMARY_HIT=yes ;;  # a path INSIDE the primary
  *"$PRIMARY")                         PRIMARY_HIT=yes ;;  # the primary, at end of input
  *"$PRIMARY"[[:space:]]*)             PRIMARY_HIT=yes ;;  # the primary, then a space
  *"$PRIMARY"[\;\&\|\)\'\"\`]*)        PRIMARY_HIT=yes ;;  # the primary, then a metachar
esac
case "$PRIMARY_HIT" in
  yes)
    emit_deny "Blocked by sealed-worktree-guard: this agent is SEALED into ${SEALED} and the primary checkout path (${PRIMARY}) appeared in a ${TOOL:-tool} call. The primary is off-limits for reads AND writes -- your worktree is a full checkout of the same repository, so every file you legitimately need is already under ${SEALED}. If you believe you need the primary, STOP and hand back to the Steward; do not reshape the command to evade this."
    ;;
esac

# --- rule 2: git-dir redirection and re-init (TC-128) ----------------------
# `git init` with GIT_DIR unscrubbed re-initialised the primary TWICE on
# 2026-07-29 and set core.bare=true on it. Deny the whole family here rather
# than trusting the caller to scrub.
case "$COMMAND" in
  *"git init"*|*"git --git-dir"*|*"GIT_DIR="*|*"git --work-tree"*|*"git worktree add"*|*"git worktree remove"*)
    emit_deny "Blocked by sealed-worktree-guard: git-dir redirection / worktree management / 'git init' is refused for a sealed agent (TC-128 -- an unscrubbed GIT_DIR re-initialised the PRIMARY repository twice on 2026-07-29 and set core.bare=true on it). Build throwaway git fixtures under \$TMPDIR with an explicitly scrubbed environment, or hand back to the Steward."
    ;;
esac

# --- rule 3: obvious upward escapes from the sealed root -------------------
# Not exhaustive, and not claimed to be -- see the header. This catches the
# careless case; the snapshot catches the rest.
case "$COMMAND" in
  *"cd /"*|*"cd ~"*|*"cd .."*|*"pushd /"*|*"pushd ~"*|*"pushd .."*)
    case "$COMMAND" in
      *"cd $SEALED"*|*"pushd $SEALED"*|*"cd \$TMPDIR"*|*"cd /tmp"*|*"pushd /tmp"*) : ;;
      *)
        emit_deny "Blocked by sealed-worktree-guard: a directory change that leaves the sealed worktree (${SEALED}) was refused. Work with paths relative to the worktree root, or absolute paths under it. Scratch space under /tmp is permitted. If you need to leave, STOP and hand back to the Steward."
        ;;
    esac
    ;;
esac

exit 0
