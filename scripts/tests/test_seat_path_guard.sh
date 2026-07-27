#!/usr/bin/env bash
# scripts/tests/test_seat_path_guard.sh — coverage for the PreToolUse write-path
# guard `.claude/hooks/seat-path-guard.sh` (agent-seat-hardening ASH-B).
#
# WHAT THE HOOK IS FOR
#   dev/design/orchestration.md § 1.2 states the authoritative write-path
#   boundary: the COORDINATING seats (orchestrator, steward) may write
#   dev/plans/**, dev/design/**, STATUS boards, ledgers and scripts/**, and MUST
#   NEVER write src/**, engine/** or test sources. The IMPLEMENTER seat is the
#   complement — it writes source and tests by design and must never be blocked.
#   Until now that boundary was pure discipline: § 1.2 itself records that a
#   `tools:` frontmatter allowlist is inert for a main-thread seat, and that even
#   for a spawned seat a Bash grant writes any file via `cat >` / heredoc
#   regardless of Edit/Write grants. This hook is the first mechanical check.
#
# WHY THE BASH ARMS ARE THE LOAD-BEARING ONES
#   Removing Edit/Write from a seat is not a guard at all while Bash remains.
#   Measured on the two observed orchestrator sessions: 76 and 193 Bash calls,
#   ZERO Edit/Write calls. A guard that only inspects Edit/Write would therefore
#   have fired on 0 of the actual write path. Arms 3, 9, 10, 11 and the extra
#   Bash-verb arms below are the ones that make this suite worth having.
#
# WHY THIS SUITE IS NOT VACUOUS (TC-37)
#   The hook's whole contract is "exit 0 and stay silent unless you are sure",
#   so a `true` binary — or a hook that crashes on every input — satisfies every
#   ALLOW arm trivially. Non-vacuity therefore rests entirely on the DENY arms,
#   and RED was OBSERVED twice before the hook existed:
#     (a) hook absent      -> `bash <hook>` rc=127, empty stdout: every arm red
#                             (both the DENY arms and the ALLOW arms, since
#                             ALLOW also asserts rc=0).
#     (b) hook = `#!/usr/bin/env bash` + `exit 0` (the always-fail-open stub)
#         -> every ALLOW arm GREEN, every DENY arm RED. That second run is the
#            real witness: it proves the DENY arms, and only the DENY arms,
#            discriminate a working guard from a no-op.
#   Arm 16 (the UNWIRED marker) carries its own vacuity trap — the file it reads
#   is gitignored and absent from a fresh clone — so it first proves its own
#   detector fires against a positive fixture before applying it to the real
#   settings file(s).
#
# ISOLATION
#   Every arm pipes a synthetic PreToolUse payload into the hook under
#   `env -u FATHOMDB_SEAT` (or an explicit FATHOMDB_SEAT=...). The hook is a
#   pure stdin->stdout function: it never touches the filesystem, so the only
#   temp state here is the mktemp -d fixture used by arm 16's positive control.
#   No real file is ever written by an arm, and no arm mutates this checkout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$REPO_ROOT/.claude/hooks/seat-path-guard.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

if ! command -v python3 >/dev/null 2>&1; then
  printf 'FAIL  python3 is required to build synthetic payloads for this suite\n' >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# payload <tool_name> <file_path> <command> [agent_type] [agent_id]
#   Emits a PreToolUse stdin payload with the field set the real harness sends
#   (session_id / cwd / permission_mode / hook_event_name / tool_use_id are
#   included so the hook is exercised against realistic, not minimal, input).
#   An EMPTY agent_type/agent_id argument means the KEY IS OMITTED entirely —
#   that is the documented main-thread shape, and omission (not empty-string)
#   is what the hook must handle.
# ---------------------------------------------------------------------------
payload() {
  python3 - "$@" <<'PY'
import json, sys
tool, file_path, command = sys.argv[1], sys.argv[2], sys.argv[3]
agent_type = sys.argv[4] if len(sys.argv) > 4 else ""
agent_id = sys.argv[5] if len(sys.argv) > 5 else ""
d = {
    "session_id": "11111111-2222-3333-4444-555555555555",
    "prompt_id": "prompt-1",
    "transcript_path": "/home/nobody/.claude/projects/x/y.jsonl",
    "cwd": "/home/nobody/repo",
    "permission_mode": "default",
    "hook_event_name": "PreToolUse",
    "tool_name": tool,
    "tool_use_id": "toolu_01",
    "effort": "medium",
}
ti = {}
if file_path:
    ti["file_path"] = file_path
if command:
    ti["command"] = command
d["tool_input"] = ti
if agent_type:
    d["agent_type"] = agent_type
if agent_id:
    d["agent_id"] = agent_id
sys.stdout.write(json.dumps(d))
PY
}

# run_hook <payload> [SEAT]  -- SEAT empty => FATHOMDB_SEAT explicitly UNSET.
# Sets OUT (stdout), ERR (stderr) and RC.
run_hook() {
  local pl="$1" seat="${2-}"
  local errfile="$TMPROOT/stderr.$$"
  set +e
  if [ -n "$seat" ]; then
    OUT="$(printf '%s' "$pl" | env FATHOMDB_SEAT="$seat" bash "$HOOK" 2>"$errfile")"
  else
    OUT="$(printf '%s' "$pl" | env -u FATHOMDB_SEAT bash "$HOOK" 2>"$errfile")"
  fi
  RC=$?
  set -e
  ERR="$(cat "$errfile" 2>/dev/null || true)"
  rm -f "$errfile"
}

# decision_of <stdout> -> prints the permissionDecision, or "" when the hook
# stayed silent. Prints "__UNPARSEABLE__" if it emitted non-JSON, which must
# itself be a failure: a malformed decision is worse than no decision.
decision_of() {
  local out="$1"
  [ -n "$out" ] || { printf ''; return 0; }
  printf '%s' "$out" | python3 -c '
import json, sys
raw = sys.stdin.read()
try:
    d = json.loads(raw)
except Exception:
    sys.stdout.write("__UNPARSEABLE__"); sys.exit(0)
h = d.get("hookSpecificOutput") or {}
sys.stdout.write(str(h.get("permissionDecision") or ""))
'
}

expect_deny() {
  local desc="$1" dec
  dec="$(decision_of "$OUT")"
  if [ "$RC" -ne 0 ]; then
    fail "$desc — hook must exit 0 even when denying (deny travels in stdout JSON); got rc=$RC, out: $OUT, err: $ERR"
    return
  fi
  if [ "$dec" = "deny" ]; then
    pass "$desc"
  else
    fail "$desc — expected permissionDecision=deny, got '${dec:-<silent>}'; out: $OUT"
  fi
}

# ALLOW here means "the hook makes NO decision": stdout empty, rc 0. It must not
# emit permissionDecision=allow either — auto-approving would silently bypass
# the human's permission prompt for a tool call the guard has no opinion on.
expect_allow() {
  local desc="$1"
  if [ "$RC" -ne 0 ]; then
    fail "$desc — expected rc=0, got rc=$RC; out: $OUT, err: $ERR"
    return
  fi
  if [ -n "$OUT" ]; then
    fail "$desc — expected NO decision (empty stdout), got: $OUT"
    return
  fi
  pass "$desc"
}

# ============================ shape of the hook ==============================

if [ -f "$HOOK" ]; then
  pass "hook exists at .claude/hooks/seat-path-guard.sh"
else
  fail "hook missing at $HOOK"
fi
if [ -x "$HOOK" ]; then
  pass "hook is executable (settings.json invokes it as a command)"
else
  fail "hook is not executable: $HOOK"
fi
if head -1 "$HOOK" 2>/dev/null | grep -qE '^#!.*bash'; then
  pass "hook has a bash shebang"
else
  fail "hook has no bash shebang"
fi
# The hook must NOT be `set -e` hardened the way ordinary scripts are: a
# PreToolUse hook that exits non-zero on an unexpected error becomes a
# repo-wide nuisance (exit 2 blocks the call outright). Assert the fail-open
# posture is implemented, not merely promised in prose.
if grep -qE "^[[:space:]]*trap[[:space:]]+'exit 0'[[:space:]]+EXIT" "$HOOK" 2>/dev/null; then
  pass "hook installs a fail-open EXIT trap (never exits non-zero)"
else
  fail "hook must guarantee exit 0 on every path (expected: trap 'exit 0' EXIT)"
fi

# ================================ ARM 1 ======================================
# seat=orchestrator, Edit, src/** -> DENY
run_hook "$(payload Edit src/engine/query.rs '' orchestrator)"
expect_deny "arm 1: orchestrator Edit of src/** is DENIED"

# ================================ ARM 2 ======================================
# seat=orchestrator, Write, src/** -> DENY
run_hook "$(payload Write src/python/fathomdb/api.py '' orchestrator)"
expect_deny "arm 2: orchestrator Write of src/** is DENIED"

# ================================ ARM 3 ======================================
# seat=orchestrator, Bash heredoc into src/** -> DENY. THE load-bearing arm:
# this is the exact evasion an Edit/Write-only guard cannot see, and the one
# the measured 76/193-Bash-call sessions would actually have used.
run_hook "$(payload Bash '' "cat > src/rust/lib.rs <<'EOF'
pub fn f() {}
EOF" orchestrator)"
expect_deny "arm 3: orchestrator Bash heredoc 'cat > src/...' is DENIED"

# ================================ ARM 4 ======================================
run_hook "$(payload Edit dev/plans/plan-0.8.20.md '' orchestrator)"
expect_allow "arm 4: orchestrator Edit of dev/plans/** is ALLOWED"

# ================================ ARM 5 ======================================
run_hook "$(payload Write dev/plans/runs/STATUS-0.8.20.md '' orchestrator)"
expect_allow "arm 5: orchestrator Write of dev/plans/** is ALLOWED"

# ================================ ARM 6 ======================================
run_hook "$(payload Bash '' "printf 'x\n' > dev/plans/runs/notes.md" orchestrator)"
expect_allow "arm 6: orchestrator Bash redirect into dev/plans/** is ALLOWED"

# ================================ ARM 7 ======================================
# The implementer writes source BY DESIGN. If this ever goes red the guard has
# broken the seat it exists to protect.
run_hook "$(payload Write src/rust/engine/mod.rs '' implementer)"
expect_allow "arm 7: implementer Write of src/** is ALLOWED (never block the implementing seat)"

# ================================ ARM 8 ======================================
# Neither agent_id/agent_type nor FATHOMDB_SEAT: this is the shape of EVERY
# ordinary main-thread session in this repo. A project-global PreToolUse hook
# that decided anything here would break the whole checkout.
run_hook "$(payload Write src/rust/engine/mod.rs '')"
expect_allow "arm 8: UNKNOWN seat makes NO decision (fail-open; ordinary sessions unaffected)"

# ================================ ARM 9 ======================================
run_hook "$(payload Bash '' "sed -i 's/foo/bar/' src/rust/lib.rs" steward)"
expect_deny "arm 9: steward Bash 'sed -i' against src/** is DENIED"

# ================================ ARM 10 =====================================
run_hook "$(payload Bash '' "printf 'x\n' >> engine/native/build.rs" orchestrator)"
expect_deny "arm 10: orchestrator Bash '>>' append into an engine/ path is DENIED"

# ================================ ARM 11 =====================================
# FALSE-POSITIVE GUARD. A read-only command that merely MENTIONS a protected
# path must sail through; over-blocking reads would make the guard unusable and
# it would be turned off, which is strictly worse than no guard.
run_hook "$(payload Bash '' 'grep -n foo src/rust/lib.rs' orchestrator)"
expect_allow "arm 11: orchestrator Bash read-only 'grep foo src/x.rs' is ALLOWED"

# ================================ ARM 12 =====================================
# Malformed and empty stdin: fail OPEN, silently.
run_hook 'this is not json at all {{{'
expect_allow "arm 12a: malformed stdin -> exit 0, no decision (fail-open)"
run_hook ''
expect_allow "arm 12b: empty stdin -> exit 0, no decision (fail-open)"
run_hook '{"tool_name":"Write"}'
expect_allow "arm 12c: JSON with no tool_input / no seat -> exit 0, no decision"
run_hook '[]'
expect_allow "arm 12d: valid JSON of the wrong shape -> exit 0, no decision"

# ================================ ARM 13 =====================================
# THE MAIN-THREAD OPT-IN. A /orchestrate or /steward session's top-level thread
# carries NEITHER agent_id NOR agent_type (verified against the Claude Code
# v2.1.220 field docs: agent_id is "Absent for the main thread, even in --agent
# sessions"). FATHOMDB_SEAT is the only way such a session can be identified,
# so this arm is what makes the guard reachable for a main-thread seat at all.
run_hook "$(payload Write src/rust/lib.rs '')" orchestrator
expect_deny "arm 13: FATHOMDB_SEAT=orchestrator with NO agent_type still DENIES src/** (main-thread opt-in works)"

# ================================ ARM 14 =====================================
# PRECEDENCE. scripts/** is on the guarded seats' MAY-write list and test
# sources are on their MUST-NEVER list; scripts/tests/** is both. The hook rules
# test-source WINS. Chosen deliberately: § 1.2's never-list is the invariant and
# the may-list is the convenience, so on collision the never-list governs.
run_hook "$(payload Write scripts/tests/test_foo.sh '' orchestrator)"
expect_deny "arm 14: orchestrator Write of scripts/tests/test_foo.sh is DENIED (test-source beats the scripts/** allow)"
run_hook "$(payload Write scripts/check-ledgers.sh '' orchestrator)"
expect_allow "arm 14b: orchestrator Write of a non-test scripts/** file is ALLOWED (the allow half of the same pair)"

# ================================ ARM 15 =====================================
# DENY SHAPE. The harness only honours the documented envelope; a deny with a
# typo'd key is a silent no-op, i.e. a guard that looks like it fired and did
# not. Assert the exact contract, key by key.
run_hook "$(payload Edit src/rust/lib.rs '' orchestrator)"
SHAPE="$(printf '%s' "$OUT" | python3 -c '
import json, sys
try:
    d = json.loads(sys.stdin.read())
except Exception as e:
    print("not-json: %s" % e); sys.exit(0)
if list(d.keys()) != ["hookSpecificOutput"]:
    print("top-level keys are %r, want exactly [hookSpecificOutput]" % (list(d.keys()),)); sys.exit(0)
h = d["hookSpecificOutput"]
if not isinstance(h, dict):
    print("hookSpecificOutput is not an object"); sys.exit(0)
if sorted(h.keys()) != ["hookEventName", "permissionDecision", "permissionDecisionReason"]:
    print("inner keys are %r" % (sorted(h.keys()),)); sys.exit(0)
if h["hookEventName"] != "PreToolUse":
    print("hookEventName is %r" % (h["hookEventName"],)); sys.exit(0)
if h["permissionDecision"] != "deny":
    print("permissionDecision is %r" % (h["permissionDecision"],)); sys.exit(0)
r = h["permissionDecisionReason"]
if not isinstance(r, str) or not r.strip():
    print("permissionDecisionReason is empty"); sys.exit(0)
print("OK")
')"
if [ "$SHAPE" = "OK" ]; then
  pass "arm 15: deny payload matches the documented PreToolUse envelope exactly"
else
  fail "arm 15: deny payload violates the contract — $SHAPE; raw: $OUT"
fi
# The reason has to be actionable by the seat that just got denied: it must name
# the offending path and point at the authority, or the seat will simply retry.
if printf '%s' "$OUT" | grep -qF 'src/rust/lib.rs'; then
  pass "arm 15b: the deny reason names the offending path"
else
  fail "arm 15b: the deny reason does not name the path; got: $OUT"
fi
if printf '%s' "$OUT" | grep -qF 'orchestration.md'; then
  pass "arm 15c: the deny reason cites the authority (orchestration.md § 1.2)"
else
  fail "arm 15c: the deny reason does not cite orchestration.md; got: $OUT"
fi

# ================================ ARM 16 =====================================
# ############################################################################
# # PHASE-1 UNWIRED MARKER — FLIP THIS ARM WHEN PHASE 2 WIRES THE HOOK.       #
# # ASH-B ships the guard DELIBERATELY UNWIRED: nothing in .claude/settings   #
# # .json references seat-path-guard, so installing this branch changes the   #
# # behaviour of exactly zero running sessions. Hooks in settings.json are    #
# # PROJECT-GLOBAL — they fire for the main thread AND every spawned subagent #
# # — so wiring is an HITL-gated act, not an implementation detail. When      #
# # Phase 2 wires it, THIS ARM GOES RED. That is the point: the red arm is    #
# # the tripwire that forces wiring to be a conscious, reviewed change rather #
# # than something that drifts in.                                            #
# ############################################################################
# Vacuity trap this arm has to dodge: .claude/settings.json is gitignored
# (.gitignore's `.claude/*` block) and therefore ABSENT from a fresh clone, from
# CI, and from this linked worktree. "grep found nothing" would then be true for
# the wrong reason. So: (1) prove the detector fires against a positive fixture,
# (2) resolve every settings file that really exists — including the primary
# checkout's, reached via `git rev-parse --git-common-dir`, because a worktree
# has none of its own — and (3) apply the proven detector to each.
NEEDLE='seat-path-guard'
POSITIVE="$TMPROOT/positive-settings.json"
cat >"$POSITIVE" <<'JSON'
{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command",
"command":"$CLAUDE_PROJECT_DIR/.claude/hooks/seat-path-guard.sh"}]}]}}
JSON
if grep -qF "$NEEDLE" "$POSITIVE"; then
  pass "arm 16a: the unwired-detector fires on a positive fixture (not a vacuous grep)"
else
  fail "arm 16a: the unwired-detector failed to fire on a settings file that DOES wire the hook"
fi

SETTINGS_CANDIDATES=()
GIT_COMMON_DIR="$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null || true)"
if [ -n "$GIT_COMMON_DIR" ]; then
  case "$GIT_COMMON_DIR" in
    /*) PRIMARY_CHECKOUT="$(dirname "$GIT_COMMON_DIR")" ;;
    *)  PRIMARY_CHECKOUT="$(cd "$REPO_ROOT" && cd "$(dirname "$GIT_COMMON_DIR")" && pwd)" ;;
  esac
else
  PRIMARY_CHECKOUT=""
fi
for base in "$REPO_ROOT" "$PRIMARY_CHECKOUT" "${CLAUDE_PROJECT_DIR:-}"; do
  [ -n "$base" ] || continue
  for name in settings.json settings.local.json; do
    f="$base/.claude/$name"
    [ -f "$f" ] || continue
    case " ${SETTINGS_CANDIDATES[*]-} " in *" $f "*) continue ;; esac
    SETTINGS_CANDIDATES+=("$f")
  done
done

if [ "${#SETTINGS_CANDIDATES[@]}" -eq 0 ]; then
  # Not a failure: .claude/settings.json is per-user and gitignored, so a fresh
  # clone legitimately has none — and "no settings file" IS "not wired".
  # Reported loudly so the pass is never mistaken for a positive observation.
  pass "arm 16b: no .claude/settings*.json exists here (gitignored/per-user) — UNWIRED holds vacuously, detector proven above"
else
  for f in "${SETTINGS_CANDIDATES[@]}"; do
    if grep -qF "$NEEDLE" "$f"; then
      fail "arm 16b: PHASE-1 MARKER TRIPPED — $f references '$NEEDLE'. ASH-B ships the hook UNWIRED; wiring is an HITL-gated Phase-2 act. If this is intentional, flip this arm."
    else
      pass "arm 16b: $f does NOT reference '$NEEDLE' (hook still UNWIRED, as ASH-B intends)"
    fi
  done
fi

# ======================= EXTRA ARMS — Bash write verbs =======================
# Everything below exists because Bash is the seats' real write path. Each verb
# is a separately-reachable way to put bytes in src/ or engine/, so each needs
# its own arm; a guard that catches `>` but not `tee` is a guard with a hole
# the size of one habit.

run_hook "$(payload Bash '' 'tee -a src/rust/lib.rs < /dev/null' orchestrator)"
expect_deny "arm 17: orchestrator Bash 'tee -a src/...' is DENIED"

run_hook "$(payload Bash '' 'cp dev/plans/x.md src/rust/x.rs' orchestrator)"
expect_deny "arm 18: orchestrator Bash 'cp <src> src/...' DENIES on the DESTINATION"

run_hook "$(payload Bash '' 'mv /tmp/scratch.rs engine/native/scratch.rs' steward)"
expect_deny "arm 19: steward Bash 'mv' into engine/ is DENIED"

run_hook "$(payload Bash '' 'rm -f src/rust/lib.rs' orchestrator)"
expect_deny "arm 20: orchestrator Bash 'rm' of a src/** file is DENIED (deletion is a write)"

run_hook "$(payload Bash '' 'dd if=/dev/null of=engine/native/x.o' orchestrator)"
expect_deny "arm 21: orchestrator Bash 'dd of=' into engine/ is DENIED"

run_hook "$(payload Bash '' 'git checkout -- src/rust/lib.rs' orchestrator)"
expect_deny "arm 22: orchestrator Bash 'git checkout --' of src/** is DENIED (it rewrites the file)"

run_hook "$(payload Bash '' 'git restore src/rust/lib.rs' steward)"
expect_deny "arm 23: steward Bash 'git restore' of src/** is DENIED"

run_hook "$(payload Bash '' "python3 -c \"open('src/rust/lib.rs','w').write('x')\"" orchestrator)"
expect_deny "arm 24: orchestrator Bash inline 'python3 -c' opening src/** for writing is DENIED"

run_hook "$(payload Bash '' "node -e \"require('fs').writeFileSync('src/ts/index.ts','x')\"" orchestrator)"
expect_deny "arm 25: orchestrator Bash inline 'node -e' writeFileSync into src/** is DENIED"

run_hook "$(payload Bash '' 'truncate -s 0 src/rust/lib.rs' orchestrator)"
expect_deny "arm 26: orchestrator Bash 'truncate' of src/** is DENIED"

# Absolute path, and inside a DIFFERENT worktree than $CLAUDE_PROJECT_DIR: a
# guarded seat must not write source ANYWHERE, not merely under the project dir.
# Segment matching on the normalized path is what buys this.
run_hook "$(payload Write /home/coreyt/projects/fathomdb-worktrees/some-slice/src/rust/lib.rs '' orchestrator)"
expect_deny "arm 27: orchestrator Write of an ABSOLUTE src/** path in another worktree is DENIED"

run_hook "$(payload Write /home/coreyt/projects/fathomdb/dev/design/orchestration.md '' orchestrator)"
expect_allow "arm 28: orchestrator Write of an ABSOLUTE dev/design/** path is ALLOWED"

# .. traversal must not launder the target past the segment check.
run_hook "$(payload Write dev/plans/../../src/rust/lib.rs '' orchestrator)"
expect_deny "arm 29: a '..' traversal that lands in src/** is DENIED (path is normalized first)"

# ===================== EXTRA ARMS — false-positive guards ====================
# The other half of every guard: what it must NOT do. An over-blocking guard
# gets disabled, and a disabled guard protects nothing.

run_hook "$(payload Bash '' 'cat src/rust/lib.rs' orchestrator)"
expect_allow "arm 30: 'cat src/x.rs' (read) is ALLOWED"

run_hook "$(payload Bash '' 'git log --oneline -5 src/' orchestrator)"
expect_allow "arm 31: 'git log src/' is ALLOWED (git log never writes the tree)"

run_hook "$(payload Bash '' 'git diff src/rust/lib.rs > dev/plans/runs/diff.txt' orchestrator)"
expect_allow "arm 32: reading src/** while redirecting INTO dev/plans/** is ALLOWED (target, not mention, decides)"

run_hook "$(payload Bash '' 'ls -la src/ && wc -l src/rust/lib.rs' orchestrator)"
expect_allow "arm 33: read-only inspection across a '&&' chain is ALLOWED"

# A heredoc BODY is data, not shell. If the body were scanned, ordinary prose
# (markdown blockquotes, pasted shell examples) would deny at random.
run_hook "$(payload Bash '' "cat > dev/plans/runs/notes.md <<'EOF'
> quoting a shell example: printf x > src/rust/lib.rs
EOF" orchestrator)"
expect_allow "arm 34: a heredoc BODY mentioning 'and even redirecting to' src/** does not deny (body is data)"

# A tool the guard has no rule for must not be decided on.
run_hook "$(payload Read src/rust/lib.rs '' orchestrator)"
expect_allow "arm 35: an unhandled tool (Read) yields NO decision"

# Non-guarded named seats are the complement of the rule and stay untouched.
run_hook "$(payload Bash '' 'cat > src/rust/lib.rs <<EOF
x
EOF' implementer)"
expect_allow "arm 36: implementer Bash heredoc into src/** is ALLOWED"
run_hook "$(payload Write src/rust/lib.rs '' general-purpose)"
expect_allow "arm 37: an unrelated agent_type (general-purpose) yields NO decision"

# ==================== EXTRA ARMS — seat resolution order =====================
# FATHOMDB_SEAT is checked BEFORE agent_type. It has to be, because it is the
# only channel a main-thread seat has (arm 13). Note the escape hatch is only
# settable at session launch: a `export FATHOMDB_SEAT=...` run through the Bash
# TOOL does not persist — each Bash call is its own process, and the hook is
# spawned by the harness, not by that shell. So an agent cannot flip its own
# seat mid-session, which is exactly why env-beats-payload is safe here.
run_hook "$(payload Write src/rust/lib.rs '' orchestrator)" implementer
expect_allow "arm 38: FATHOMDB_SEAT=implementer overrides agent_type=orchestrator (env has precedence)"
run_hook "$(payload Write src/rust/lib.rs '' implementer)" steward
expect_deny "arm 39: FATHOMDB_SEAT=steward overrides agent_type=implementer (env has precedence, both ways)"

# A spawned subagent IS identifiable: agent_id present alongside agent_type is
# the documented subagent shape, and the guard must fire on it with no env help.
run_hook "$(payload Write src/rust/lib.rs '' orchestrator agent_01abc)"
expect_deny "arm 40: a SPAWNED orchestrator (agent_id + agent_type present) is DENIED with no env hint"

# Casing/whitespace tolerance: the seat name comes from human-edited config.
run_hook "$(payload Write src/rust/lib.rs '')" '  Orchestrator  '
expect_deny "arm 41: FATHOMDB_SEAT is matched case-insensitively and trimmed"

# ====================== EXTRA ARM — no stderr noise ==========================
# A PreToolUse hook writing to stderr on the happy path pollutes every tool call
# in the repo once wired. Silence on allow is part of the contract.
run_hook "$(payload Bash '' 'ls dev/plans' orchestrator)"
if [ -z "$ERR" ]; then
  pass "arm 42: the hook writes nothing to stderr on an allowed call"
else
  fail "arm 42: hook emitted stderr on an allowed call: $ERR"
fi

# ...including on the junk-input fail-open path. Measured: a NUL byte in stdin
# makes bash itself warn "ignored null byte in input" from the command
# substitution that reads the payload. That warning is the shell's, not the
# hook's, and it still shows up as hook stderr, so the hook has to suppress it.
JUNK="$TMPROOT/junk.bin"
printf 'abc\000def{"tool_name":\000"Write"}\000' >"$JUNK"
JUNK_ERR="$TMPROOT/junk.err"
set +e
JUNK_OUT="$(env -u FATHOMDB_SEAT bash "$HOOK" <"$JUNK" 2>"$JUNK_ERR")"
JUNK_RC=$?
set -e
if [ "$JUNK_RC" -eq 0 ] && [ -z "$JUNK_OUT" ] && [ -z "$(cat "$JUNK_ERR")" ]; then
  pass "arm 43: NUL-bearing binary stdin -> rc=0, no decision, and NO stderr noise"
else
  fail "arm 43: junk stdin must be a silent no-op; got rc=$JUNK_RC, out: $JUNK_OUT, err: $(cat "$JUNK_ERR")"
fi

# ############################################################################
# # ARMS 44-56 — ASH-B fix-1, the two codex § 9 findings.                     #
# # Each arm below was OBSERVED RED against the pre-fix hook (d31430ce)       #
# # before the fix landed, or is the paired true-positive that must stay      #
# # green so the fix cannot be a false-negative trade. The RED/green status    #
# # at the moment each was written is recorded per arm.                       #
# ############################################################################

# ============ ARMS 44-50 — in-place editors: the PROGRAM is not a path =======
# codex § 9 finding 1. `sed|gsed|perl|ruby` with an in-place flag take a
# script/program argument that is NOT a file. The pre-fix branch pushed every
# non-flag token through consider(), so an edit EXPRESSION that merely CONTAINS
# a protected path segment denied an otherwise-allowed write. That is the
# failure mode that gets a guard switched off, so it is the more serious half.
#
# NOTE ON THE REVIEWER'S WITNESS: the review cites
# `sed -i 's#src/rust/lib.rs#x#' dev/plans/note.md` (arm 44). That exact command
# does NOT reproduce — its `#` delimiters mean `src` is never a bare `/`-segment
# of the normalized token (`s#src/rust/lib.rs#x#` splits to `s#src`, `rust`,
# `lib.rs#x#`), so it was already allowed. The MECHANISM is real, though: arms
# 47-50 are the witnesses that WERE red, where the expression's own tokens do
# normalize to a protected segment.

# arm 44 — the reviewer's literal witness. Status when written: ALREADY GREEN
# (see the note above). Kept because it is the case the review names.
run_hook "$(payload Bash '' "sed -i 's#src/rust/lib.rs#x#' dev/plans/note.md" orchestrator)"
expect_allow "arm 44: 'sed -i <EXPR mentioning src/**> dev/plans/note.md' is ALLOWED (the EXPR is a program, not a target)"

# arm 45 — `-e` supplied the expression, so EVERY remaining operand is a file.
# Status when written: ALREADY GREEN (no protected segment in the EXPR).
run_hook "$(payload Bash '' "sed -i -e 's/a/b/' dev/plans/note.md" orchestrator)"
expect_allow "arm 45: 'sed -i -e EXPR dev/plans/note.md' is ALLOWED"

# arm 46 — perl's program is attached to -pe. Status when written: ALREADY GREEN.
run_hook "$(payload Bash '' "perl -i -pe 's#src/lib.rs#x#' dev/plans/note.md" orchestrator)"
expect_allow "arm 46: 'perl -i -pe <EXPR mentioning src/**> dev/plans/note.md' is ALLOWED"

# arm 47 — OBSERVED RED before the fix: expected NO decision, got
# permissionDecision=deny. The bare expression `s/src/lib/` normalizes to the
# segments s | src | lib, so `src` matched and an allowed dev/plans/** write was
# blocked. This is the minimal reproducing witness for finding 1.
run_hook "$(payload Bash '' "sed -i 's/src/lib/' dev/plans/note.md" orchestrator)"
expect_allow "arm 47: 'sed -i s/src/lib/ dev/plans/note.md' is ALLOWED (RED before fix-1: the EXPR's own 'src' segment denied)"

# arm 48 — OBSERVED RED before the fix (same mechanism via `-e`, segment tests).
run_hook "$(payload Bash '' "sed -i -e 's/tests/checks/' dev/plans/note.md" orchestrator)"
expect_allow "arm 48: 'sed -i -e s/tests/checks/ dev/plans/note.md' is ALLOWED (RED before fix-1)"

# arm 49 — OBSERVED RED before the fix (perl -pe, segment engine).
run_hook "$(payload Bash '' "perl -i -pe 's/engine/motor/' dev/plans/note.md" orchestrator)"
expect_allow "arm 49: 'perl -i -pe s/engine/motor/ dev/plans/note.md' is ALLOWED (RED before fix-1)"

# arm 50 — OBSERVED RED before the fix. A multi-WORD expression: the naive
# whitespace tokenizer shreds it, and the shard `src/rust/lib.rs,'` normalized
# straight onto a protected segment. The fix has to skip the whole program
# argument, not just its first token.
run_hook "$(payload Bash '' "sed -i 's,^,# see src/rust/lib.rs,' dev/plans/note.md" orchestrator)"
expect_allow "arm 50: a multi-word sed EXPR quoting src/** does not deny (RED before fix-1)"

# Arm 50 forced the fix to walk past the WHOLE quoted program word, not just its
# first token. That absorption must not over-swallow, or the real operand behind
# an apostrophe-bearing expression goes unguarded. Both idioms below balance
# their quotes and must still reach the src/** operand.
run_hook "$(payload Bash '' "sed -i 's/it'\\''s/x/' src/rust/lib.rs" orchestrator)"
expect_deny "arm 50b: the '\\'' quote-escape idiom still DENIES its src/** operand (absorption does not over-swallow)"

run_hook "$(payload Bash '' "sed -i \"s/don't/do not/\" src/rust/lib.rs" orchestrator)"
expect_deny "arm 50c: an apostrophe inside a double-quoted EXPR still DENIES its src/** operand"

# --- the true positives the fix must NOT trade away. All were green pre-fix and
# must stay green: a false-positive fix that opens a false negative is not a fix.
run_hook "$(payload Bash '' "sed -i -e 's/a/b/' src/rust/lib.rs" orchestrator)"
expect_deny "arm 51: 'sed -i -e EXPR src/rust/lib.rs' still DENIES (the -e form must still catch a real operand)"

run_hook "$(payload Bash '' "sed -i.bak 's/a/b/' src/rust/lib.rs" orchestrator)"
expect_deny "arm 51b: 'sed -i.bak EXPR src/**' still DENIES (suffixed in-place flag)"

run_hook "$(payload Bash '' "perl -i -pe 's/a/b/' src/rust/lib.rs" steward)"
expect_deny "arm 51c: 'perl -i -pe EXPR src/**' still DENIES"

run_hook "$(payload Bash '' 'sed -i -f fix.sed src/rust/lib.rs' orchestrator)"
expect_deny "arm 51d: 'sed -i -f script.sed src/**' still DENIES (-f supplies the program; the operand is a file)"

# The -f script file is itself a program source, not a write target, so an
# in-place run whose only src/** mention is the script name must not deny.
run_hook "$(payload Bash '' 'sed -i -f dev/plans/fix.sed dev/plans/note.md' orchestrator)"
expect_allow "arm 51e: 'sed -i -f dev/plans/fix.sed dev/plans/note.md' is ALLOWED"

# No in-place flag at all: sed is a reader, and a reader must stay silent.
run_hook "$(payload Bash '' "sed -n 's/a/b/p' src/rust/lib.rs" orchestrator)"
expect_allow "arm 52: 'sed' WITHOUT an in-place flag is ALLOWED (it writes nothing)"

# ============= ARMS 53-56 — glued clobber redirection `>|FILE` ===============
# codex § 9 finding 2, and it is wider than the review states. The review says
# the code caught the spaced `>| FILE` and missed only the glued form. In fact
# BOTH slipped: scan_bash_command rewrites every `|` to a newline BEFORE
# tokenizing, so `>|` was torn in half and the file landed alone on the next
# line, where it is just a bare word. Measured pre-fix, seat=orchestrator:
#   `printf x >|src/rust/lib.rs`   -> no decision   (expected deny)  RED
#   `printf x >| src/rust/lib.rs`  -> no decision   (expected deny)  RED
# `>>|` and `&>|` are deliberately NOT covered: bash rejects both as syntax
# errors, so there is no write to guard.
run_hook "$(payload Bash '' 'printf x >|src/rust/lib.rs' orchestrator)"
expect_deny "arm 53: glued clobber redirect 'printf x >|src/**' is DENIED (RED before fix-1)"

run_hook "$(payload Bash '' 'printf x >| src/rust/lib.rs' orchestrator)"
expect_deny "arm 54: spaced clobber redirect 'printf x >| src/**' is DENIED (RED before fix-1 too, contra the review)"

run_hook "$(payload Bash '' 'printf x 2>|engine/native/log.txt' orchestrator)"
expect_deny "arm 54b: fd-numbered clobber redirect 'printf x 2>|engine/**' is DENIED"

run_hook "$(payload Bash '' 'printf x >|dev/plans/note.md' orchestrator)"
expect_allow "arm 55: glued clobber redirect into dev/plans/** is ALLOWED (the fix must not over-block)"

# The `>|`-normalizing rewrite must not turn a quoted MENTION of `>|` into a
# redirection. A read-only grep for the operator has to stay silent.
run_hook "$(payload Bash '' "grep -n '>|' dev/plans/note.md" orchestrator)"
expect_allow "arm 55b: a read-only command quoting the literal '>|' is ALLOWED"

# --- the rest of the forms the header comment claims, asserted rather than
# believed. Every one of these was green pre-fix; they exist so the comment and
# the code cannot drift apart again (finding 2 was, at root, that drift).
run_hook "$(payload Bash '' 'printf x 1>src/rust/lib.rs' orchestrator)"
expect_deny "arm 56a: '1>' glued is DENIED"
run_hook "$(payload Bash '' 'printf x 1> src/rust/lib.rs' orchestrator)"
expect_deny "arm 56b: '1>' spaced is DENIED"
run_hook "$(payload Bash '' 'printf x 2>>src/rust/lib.rs' orchestrator)"
expect_deny "arm 56c: '2>>' glued is DENIED"
run_hook "$(payload Bash '' 'printf x 2>> src/rust/lib.rs' orchestrator)"
expect_deny "arm 56d: '2>>' spaced is DENIED"
run_hook "$(payload Bash '' 'printf x &>src/rust/lib.rs' orchestrator)"
expect_deny "arm 56e: '&>' glued is DENIED"
run_hook "$(payload Bash '' 'printf x &> src/rust/lib.rs' orchestrator)"
expect_deny "arm 56f: '&>' spaced is DENIED"

# ############################################################################
# # ARMS 57-64 — ASH-B fix-2, the two codex § 9 ROUND-2 findings.            #
# # Round-2 finding 1 was a FALSE NEGATIVE that fix-1 itself introduced:     #
# # a guard that silently lets a src/** write through has failed at its one  #
# # job, where a false positive is merely annoying. Arms 57* are that class  #
# # and every one of them was OBSERVED RED against the fix-1 hook (06b7418f) #
# # before the fix. Arms 62-64 are the SAME assumption found in sibling      #
# # branches by the audit round 2 ordered; they were red too.                #
# # Where an arm was already green when written it says so, and it is kept   #
# # as the regression pin for the paired true/false positive.                #
# ############################################################################

# ====== ARMS 57-60 — in-place editors: `-` is an OPERAND, `--` ends options ==
# codex round-2 finding 1. fix-1's rule "a token starting with `-` is a flag,
# never a target" is wrong for the two POSIX operand spellings:
#   `-`   means "read the script from stdin" (after -f/--file) or "stdin" as a
#         file. It is an OPERAND. Skipping it as a flag left the program slot
#         unfilled, so rule 4 ("the first bare operand IS the program") then ate
#         the real src/** FILE as if it were the sed script — and no operand was
#         ever considered. Measured on the fix-1 hook, seat=orchestrator:
#           sed -i -f - src/rust/lib.rs      -> no decision  (want deny)  RED
#           sed -i --file - src/rust/lib.rs  -> no decision  (want deny)  RED
#           perl -i -pe - src/rust/lib.rs    -> no decision  (want deny)  RED
#           sed -i - src/rust/lib.rs         -> no decision  (want deny)  RED
#   `--`  ends the options; everything after it is an operand, never a flag.

# arm 57 — the reviewer's literal witness. OBSERVED RED (allowed a src/** write).
run_hook "$(payload Bash '' 'sed -i -f - src/rust/lib.rs' orchestrator)"
expect_deny "arm 57: 'sed -i -f - src/**' is DENIED ('-' is the stdin PROGRAM, so the file is still an operand) (RED before fix-2)"

# arm 57b — the long-option spelling of the same thing. OBSERVED RED.
run_hook "$(payload Bash '' 'sed -i --file - src/rust/lib.rs' orchestrator)"
expect_deny "arm 57b: 'sed -i --file - src/**' is DENIED (RED before fix-2)"

# arm 57c — the same bug in the sibling verb. OBSERVED RED.
run_hook "$(payload Bash '' 'perl -i -pe - src/rust/lib.rs' steward)"
expect_deny "arm 57c: 'perl -i -pe - src/**' is DENIED (RED before fix-2)"

# arm 57d — turned up by the audit: a bare `-` with NO preceding -e/-f. It is
# the first OPERAND, so it takes the program slot and the real file behind it is
# a file. Skipping it as a flag made src/** the program. OBSERVED RED.
run_hook "$(payload Bash '' 'sed -i - src/rust/lib.rs' orchestrator)"
expect_deny "arm 57d: 'sed -i - src/**' is DENIED (a leading bare '-' must not push the file into the program slot) (RED before fix-2)"

# arm 58 — the ordinary `-f script` form must still catch the file. Status when
# written: ALREADY GREEN (arm 51d pins the same rule with a pathful script).
run_hook "$(payload Bash '' 'sed -i -f script.sed src/rust/lib.rs' orchestrator)"
expect_deny "arm 58: 'sed -i -f script.sed src/**' still DENIES (already green; the false-negative fix must not be a false-negative trade)"

# arm 58b — and must not over-block the allowed target. ALREADY GREEN.
run_hook "$(payload Bash '' 'sed -i -f script.sed dev/plans/note.md' orchestrator)"
expect_allow "arm 58b: 'sed -i -f script.sed dev/plans/note.md' is ALLOWED (already green)"

# arm 59 — `--` end-of-options terminator. ALREADY GREEN; pinned because fix-2
# rewrites the flag/operand decision that decides it.
run_hook "$(payload Bash '' "sed -i -- 's/a/b/' src/rust/lib.rs" orchestrator)"
expect_deny "arm 59: 'sed -i -- EXPR src/**' is DENIED ('--' ends the options) (already green)"

run_hook "$(payload Bash '' "sed -i -- 's/src/lib/' dev/plans/note.md" orchestrator)"
expect_allow "arm 59b: 'sed -i -- EXPR dev/plans/note.md' is ALLOWED (already green; the allow half of arm 59)"

run_hook "$(payload Bash '' "sed -i -e 's/a/b/' -- src/rust/lib.rs" orchestrator)"
expect_deny "arm 59c: a file operand AFTER '--' is still DENIED (already green)"

# arm 60 — the glued long-option forms: the program came from the OPTION, so
# every remaining bare operand is a file. ALREADY GREEN, pinned for the same
# reason as arm 59.
run_hook "$(payload Bash '' "sed -i --expression='s/a/b/' src/rust/lib.rs" orchestrator)"
expect_deny "arm 60: 'sed -i --expression=EXPR src/**' is DENIED (already green)"

run_hook "$(payload Bash '' "sed -i --expression='s/a/b/' dev/plans/note.md" orchestrator)"
expect_allow "arm 60b: 'sed -i --expression=EXPR dev/plans/note.md' is ALLOWED (already green)"

run_hook "$(payload Bash '' 'sed -i --file=fix.sed src/rust/lib.rs' orchestrator)"
expect_deny "arm 60c: 'sed -i --file=script src/**' is DENIED (already green)"

# ========== ARM 61 — the redirection header's claims, made exhaustive ========
# codex round-2 finding 2: the header listed `&>>` among the forms "each
# verified by an arm", and no `&>>` arm existed. `&>>` IS matched at runtime
# (measured, not assumed), so the fix is the missing arms rather than a softer
# claim. Swept for the whole list at the same time: plain glued `>` and `>>`
# were claimed and unasserted too. Each form in the header now names its arm.
run_hook "$(payload Bash '' 'printf x >src/rust/lib.rs' orchestrator)"
expect_deny "arm 61a: plain '>' GLUED is DENIED (claimed by the header, previously unasserted)"
run_hook "$(payload Bash '' 'printf x >>src/rust/lib.rs' orchestrator)"
expect_deny "arm 61b: plain '>>' GLUED is DENIED (claimed by the header, previously unasserted)"
run_hook "$(payload Bash '' 'printf x &>>src/rust/lib.rs' orchestrator)"
expect_deny "arm 61c: '&>>' GLUED is DENIED (the exact claim codex round-2 found unasserted)"
run_hook "$(payload Bash '' 'printf x &>> src/rust/lib.rs' orchestrator)"
expect_deny "arm 61d: '&>>' SPACED is DENIED (the exact claim codex round-2 found unasserted)"
run_hook "$(payload Bash '' 'printf x &>> dev/plans/note.md' orchestrator)"
expect_allow "arm 61d2: '&>>' into dev/plans/** is ALLOWED (the allow half, so 61c/61d cannot pass by over-blocking)"

# `>>|` and `&>|` are bash SYNTAX ERRORS — re-measured with `bash -n` for fix-2,
# both fail to parse — so no write can happen through either. The `>|`->`>` fold
# denies them anyway; that over-block on an unrunnable command is harmless, and
# these two arms exist only so the header's claim about it is asserted, not
# believed.
run_hook "$(payload Bash '' 'printf x >>|src/rust/lib.rs' orchestrator)"
expect_deny "arm 61e: '>>|' (a bash syntax error) is harmlessly DENIED — asserts the header's over-block note"
run_hook "$(payload Bash '' 'printf x &>|src/rust/lib.rs' orchestrator)"
expect_deny "arm 61f: '&>|' (a bash syntax error) is harmlessly DENIED — asserts the header's over-block note"

# The header also claims `>&2` cannot reach the scanner as a target. Asserted.
run_hook "$(payload Bash '' 'echo hi >&2' orchestrator)"
expect_allow "arm 61g: 'echo hi >&2' is ALLOWED (an fd dup is not a file target) — asserts the header's '>&2' note"

# ===== ARMS 62-64 — the SAME "-* means flag" assumption in sibling branches ==
# Round-2 ordered an audit of every other place the assumption appears. It
# appears in three, and all three were false negatives, not false positives.

# arm 62 — consider() itself refused any target starting with `-`, so a path
# below a directory literally named `-` was never even normalized. OBSERVED RED:
#   printf x > -/src/lib.rs  -> no decision  (want deny)
run_hook "$(payload Bash '' 'printf x > -/src/lib.rs' orchestrator)"
expect_deny "arm 62: a redirect target under a '-' directory is DENIED (a leading '-' does not make a path a flag) (RED before fix-2)"

# arm 63 — THE WORST ONE, and the one the seats would actually hit: git's global
# options take a VALUE IN THE NEXT TOKEN. The old subcommand scan skipped `-C`
# and then read the WORKTREE PATH as the subcommand, matched none of the write
# verbs, and allowed the rewrite. `git -C "$WT" checkout -- <path>` is the
# orchestrator's own idiom. OBSERVED RED (all three):
#   git -C /home/x/wt checkout -- src/rust/lib.rs -> no decision (want deny)
#   git -C /home/x/wt restore src/rust/lib.rs     -> no decision (want deny)
#   git -c core.x=1 checkout -- src/rust/lib.rs   -> no decision (want deny)
run_hook "$(payload Bash '' 'git -C /home/x/wt checkout -- src/rust/lib.rs' orchestrator)"
expect_deny "arm 63: 'git -C <dir> checkout -- src/**' is DENIED (the -C VALUE is not the subcommand) (RED before fix-2)"
run_hook "$(payload Bash '' 'git -C /home/x/wt restore src/rust/lib.rs' steward)"
expect_deny "arm 63b: 'git -C <dir> restore src/**' is DENIED (RED before fix-2)"
run_hook "$(payload Bash '' 'git -c core.editor=true checkout -- src/rust/lib.rs' orchestrator)"
expect_deny "arm 63c: 'git -c <cfg> checkout -- src/**' is DENIED (RED before fix-2)"
# ...without over-blocking the read/commit subcommands that dominate a seat's
# actual git usage. These were green and must stay green.
run_hook "$(payload Bash '' 'git -C /home/x/wt status --porcelain' orchestrator)"
expect_allow "arm 63d: 'git -C <dir> status' is ALLOWED (already green; -C must not turn a read into a write)"
run_hook "$(payload Bash '' 'git -C /home/x/wt commit -m msg' orchestrator)"
expect_allow "arm 63e: 'git -C <dir> commit' is ALLOWED (already green)"
run_hook "$(payload Bash '' 'git -C /home/x/wt log --oneline -5 src/' orchestrator)"
expect_allow "arm 63f: 'git -C <dir> log src/' is ALLOWED (already green)"

# arm 64 — cp/mv/install/ln can take the DESTINATION from an option instead of
# the last operand, and the last-operand rule never looked there. OBSERVED RED:
#   cp -t src/rust dev/plans/x.md                  -> no decision (want deny)
#   cp --target-directory=src/rust dev/plans/x.md  -> no decision (want deny)
run_hook "$(payload Bash '' 'cp -t src/rust dev/plans/x.md' orchestrator)"
expect_deny "arm 64: 'cp -t src/rust <file>' is DENIED (the destination came from the option) (RED before fix-2)"
run_hook "$(payload Bash '' 'cp --target-directory=src/rust dev/plans/x.md' orchestrator)"
expect_deny "arm 64b: 'cp --target-directory=src/rust <file>' is DENIED (RED before fix-2)"
run_hook "$(payload Bash '' 'install -m 644 -t engine/native dev/plans/x.md' steward)"
expect_deny "arm 64c: 'install -t engine/native <file>' is DENIED (RED before fix-2)"
run_hook "$(payload Bash '' 'cp -t dev/plans/runs dev/plans/x.md' orchestrator)"
expect_allow "arm 64d: 'cp -t dev/plans/runs <file>' is ALLOWED (the allow half; -t handling must not over-block)"
run_hook "$(payload Bash '' 'cp -p dev/plans/a.md dev/plans/b.md' orchestrator)"
expect_allow "arm 64e: an ordinary 'cp' between allowed paths is ALLOWED (already green)"

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll seat-path-guard tests passed\n'
