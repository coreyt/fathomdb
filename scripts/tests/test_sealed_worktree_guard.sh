#!/usr/bin/env bash
# scripts/tests/test_sealed_worktree_guard.sh — arms for
# .claude/hooks/sealed-worktree-guard.sh
#
# TWO OF THESE ARMS ARE REGRESSION ARMS FOR REAL BUGS the guard shipped with in
# its first draft, both caught by running the controls before trusting it:
#
#   Arm 3/4  — `IFS=$'\t' read` COLLAPSES runs of tabs, because tab is an IFS
#              whitespace character. With no `file_path`, the payload for
#              `git init /tmp/fix` parsed as FILE_PATH=git / COMMAND=init, so
#              rules 2 and 3 never fired and the guard silently allowed a
#              `git init` (TC-128's exact hazard). Fixed by separating fields
#              with US (0x1f), which is not whitespace.
#   Arm 7    — the sealed worktree lives at `.../fathomdb-worktrees/<name>`,
#              which CONTAINS the primary path `.../fathomdb` as a plain
#              substring, so a bare `*"$PRIMARY"*` match denied every
#              legitimate in-worktree write. Fixed by requiring a path
#              boundary: `/`, end-of-string, whitespace, or a metacharacter.
#
# Both bugs were invisible to inspection and obvious to a control. That is the
# whole argument for the "prove it non-vacuous" rule this repo runs on.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 1

GUARD=".claude/hooks/sealed-worktree-guard.sh"
PASS=0; FAIL=0
[ -f "$GUARD" ] || { echo "FAIL  guard not found at $GUARD" >&2; exit 1; }

SEALED_T="/home/coreyt/projects/fathomdb-worktrees/genview"
PRIMARY_T="/home/coreyt/projects/fathomdb"

# decide <payload> -> prints "deny" or "allow"
decide() {
  local out
  out="$(printf '%s' "$1" | FATHOMDB_SEALED_WORKTREE="$SEALED_T" \
        FATHOMDB_PRIMARY_CHECKOUT="$PRIMARY_T" bash "$GUARD" 2>/dev/null)"
  case "$out" in
    *'"permissionDecision":"deny"'*) echo deny ;;
    *)                               echo allow ;;
  esac
}

check() { # check <label> <want> <payload>
  local label="$1" want="$2" payload="$3" got
  got="$(decide "$payload")"
  if [ "$got" = "$want" ]; then
    PASS=$((PASS+1)); echo "PASS  $label ($got)"
  else
    FAIL=$((FAIL+1)); echo "FAIL  $label: want=$want got=$got" >&2
  fi
}

echo "== must DENY =="
check "1 write into the primary"        deny '{"tool_name":"Write","tool_input":{"file_path":"'"$PRIMARY_T"'/dev/x.md"}}'
check "2 bash redirect into the primary" deny '{"tool_name":"Bash","tool_input":{"command":"echo hi > '"$PRIMARY_T"'/z"}}'
check "3 git init (TC-128 regression)"  deny '{"tool_name":"Bash","tool_input":{"command":"git init /tmp/fix"}}'
check "4 cd escape (field-shift regression)" deny '{"tool_name":"Bash","tool_input":{"command":"cd ../.. && ls"}}'
check "5 GIT_DIR redirection"           deny '{"tool_name":"Bash","tool_input":{"command":"GIT_DIR=/x git status"}}'
check "6 primary as a bare final token" deny '{"tool_name":"Bash","tool_input":{"command":"ls '"$PRIMARY_T"'"}}'
check "6b git worktree add"             deny '{"tool_name":"Bash","tool_input":{"command":"git worktree add /tmp/w HEAD"}}'
check "6c primary then a metachar"      deny '{"tool_name":"Bash","tool_input":{"command":"ls '"$PRIMARY_T"'; echo done"}}'

echo "== must ALLOW =="
check "7 in-worktree write (prefix regression)" allow '{"tool_name":"Write","tool_input":{"file_path":"'"$SEALED_T"'/scripts/x.sh"}}'
check "8 in-worktree relative script"   allow '{"tool_name":"Bash","tool_input":{"command":"bash scripts/check-release-state-views.sh --write"}}'
check "9 tmp scratch"                   allow '{"tool_name":"Bash","tool_input":{"command":"cd /tmp && ls"}}'
check "10 read inside the worktree"     allow '{"tool_name":"Bash","tool_input":{"command":"ls '"$SEALED_T"'/dev"}}'
check "11 ordinary in-worktree grep"    allow '{"tool_name":"Bash","tool_input":{"command":"grep -rn TODO scripts/"}}'

echo "== must be INERT when unsealed =="
UNSEALED_OUT="$(printf '%s' '{"tool_name":"Write","tool_input":{"file_path":"'"$PRIMARY_T"'/dev/x.md"}}' | bash "$GUARD" 2>&1)"
if [ -z "$UNSEALED_OUT" ]; then
  PASS=$((PASS+1)); echo "PASS  12 unsealed => silent no-op"
else
  FAIL=$((FAIL+1)); echo "FAIL  12 unsealed must be silent; got: $UNSEALED_OUT" >&2
fi

# A sealed root INSIDE the primary is incoherent and must say so, not guard nothing.
NESTED_OUT="$(printf '%s' '{"tool_name":"Bash","tool_input":{"command":"ls"}}' \
  | FATHOMDB_SEALED_WORKTREE="$PRIMARY_T/sub" FATHOMDB_PRIMARY_CHECKOUT="$PRIMARY_T" bash "$GUARD" 2>/dev/null)"
case "$NESTED_OUT" in
  *misconfigured*) PASS=$((PASS+1)); echo "PASS  13 nested sealed root => misconfiguration deny" ;;
  *) FAIL=$((FAIL+1)); echo "FAIL  13 nested sealed root must deny as misconfigured" >&2 ;;
esac

echo "---- $PASS passed, $FAIL failed ----"
[ "$FAIL" -eq 0 ] || exit 1
