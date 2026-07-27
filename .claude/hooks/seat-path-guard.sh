#!/usr/bin/env bash
# .claude/hooks/seat-path-guard.sh — PreToolUse write-path guard for the
# COORDINATING agent seats.
#
# ============================ PHASE 1: UNWIRED ==============================
# THIS HOOK IS DELIBERATELY NOT INSTALLED. Nothing in .claude/settings.json
# references it, so it currently runs never and changes nothing. Wiring it is a
# separate, HITL-gated act, because hooks declared in settings.json are
# PROJECT-GLOBAL: they fire for the main thread AND for every spawned subagent
# in the repo, including sessions already in flight. scripts/tests/
# test_seat_path_guard.sh arm 16 asserts the unwired state and is designed to go
# RED the day someone wires it — that red arm IS the review gate.
# ============================================================================
#
# WHAT IT ENFORCES
#   dev/design/orchestration.md § 1.2 (authoritative write-path boundary):
#
#     Orchestrator / Steward  MAY write:        dev/plans/**, dev/design/**,
#                                               STATUS boards, ledgers, scripts/**
#                             MUST NEVER write: src/**, engine/**, test sources
#     Implementer             writes source + tests inside its own worktree
#
#   § 1.2 also explains why the boundary is about PATHS and not tools: a seat
#   file's `tools:` allowlist is inert for a main-thread session, and even for a
#   spawned seat a Bash grant writes any file via `cat >` / heredoc no matter
#   what Edit/Write grants say. This hook is the first mechanical expression of
#   that boundary. It is a DENY-LIST, not an allow-list: it objects only to the
#   MUST-NEVER column and stays silent about everything else, so it can never
#   block a coordinating seat from editing a README, a workflow, or a config.
#
# ------------------------------ SEAT RESOLUTION -----------------------------
# Resolved in this order; the first hit wins:
#
#   1. $FATHOMDB_SEAT   — the escape hatch, and the ONLY channel a main-thread
#                         seat has. See "what the payload cannot tell us" below.
#                         NOTE: it must be set when the session is LAUNCHED.
#                         `export FATHOMDB_SEAT=...` run through the Bash TOOL
#                         does NOT persist — each Bash call is its own process,
#                         and this hook is spawned by the harness, not by that
#                         shell. An agent therefore cannot set (or clear) its
#                         own seat mid-session, which is what makes it safe for
#                         env to outrank the payload.
#   2. .agent_type      — present for a SPAWNED subagent (alongside .agent_id),
#                         and on the main thread of a session started with
#                         `--agent`.
#   3. otherwise        — seat UNKNOWN: make NO decision, exit 0.
#
# WHAT THE PAYLOAD CANNOT TELL US (verified against the Claude Code v2.1.220
# binary's own field documentation, not from prose elsewhere):
#   * `agent_id`  — "Subagent identifier. Present only when the hook fires from
#     within a subagent... ABSENT FOR THE MAIN THREAD, even in --agent sessions.
#     Use this field (not agent_type) to distinguish subagent calls from
#     main-thread calls."
#   * `agent_type` — present for a subagent (with agent_id), or on the main
#     thread of a session started with `--agent` (without agent_id).
#
#   CONSEQUENCE, stated plainly because it bounds what this hook can ever be:
#   a SPAWNED orchestrator/steward subagent IS identifiable from the payload
#   alone. A `/orchestrate` or `/steward` MAIN-THREAD session is NOT — an
#   ordinary session started without `--agent` carries neither agent_id nor
#   agent_type, so from the payload it is indistinguishable from any other
#   session in the repo. There is no way to close that gap from inside a hook.
#   $FATHOMDB_SEAT exists precisely for it, and it is opt-in: a main-thread seat
#   that does not set it is simply not guarded. That is a real hole, not a
#   detail — it is the *usual* shape of a Steward session.
#
# ------------------------------- FAIL OPEN ----------------------------------
# EVERY failure path allows. A project-global PreToolUse hook that errors, or
# that defaults to deny, breaks every ordinary session in this checkout. So:
# malformed JSON, absent jq AND python3, absent base64 decoder, unparseable
# input, oversized input, unknown seat, unknown tool, an unexpected bash error
# anywhere — all produce exit 0 with empty stdout and no decision.
#   * There is deliberately NO `set -e` / `set -u` (a departure from the repo's
#     `set -euo pipefail` convention, which is right for gates and wrong here).
#   * `trap 'exit 0' EXIT` makes exit 0 structurally unavoidable. This is safe
#     because the deny signal travels in STDOUT, not in the exit code.
#   * Silence on allow: nothing is written to stdout or stderr unless denying.
#     An emitted "allow" would auto-approve the call and bypass the human's
#     permission prompt, so an allowed call gets no decision at all, not
#     `permissionDecision: allow`.
#
# DENY CONTRACT (exit 0 + this exact stdout envelope):
#   {"hookSpecificOutput":{"hookEventName":"PreToolUse",
#    "permissionDecision":"deny","permissionDecisionReason":"<why>"}}
# The exit-2 "blocking error" form is deliberately not used: it carries only
# free-text stderr, where this form carries a structured, quotable reason.
#
# ------------------------------ WHAT IS PROTECTED ---------------------------
# Matching is SEGMENT-BASED on the NORMALIZED target (`.` and `..` resolved), so
# it behaves identically for absolute paths, repo-relative paths, and paths
# inside any worktree. A guarded seat must not write source ANYWHERE, not only
# under $CLAUDE_PROJECT_DIR.
#   * any path segment named  src  |  engine  |  tests
#   * test-source basenames:  test_*.{py,sh,rs,ts,js}   *_test.{py,sh,rs,go,ts,js}
#                             *.test.{ts,tsx,js,mjs}    *.spec.{ts,tsx,js,mjs}
#                             conftest.py
#
# PRECEDENCE RULE (the deliberate tension named in § 1.2): scripts/tests/** is
# BOTH under the allowed scripts/** and a test source. TEST SOURCE WINS — it is
# DENIED. Rationale: the MUST-NEVER column is the invariant and the MAY column
# is the convenience, so on collision the invariant governs. There is no
# allow-list to lose to, because this hook only ever objects (see above).
#
# ---------------------- LIMITATIONS — READ BEFORE TRUSTING ------------------
# BASH GUARDING IS BEST-EFFORT, NOT A SANDBOX. It raises the cost of an
# ACCIDENTAL violation. It does not stop a determined one, and it is not
# security. Known, unclosed evasions:
#   * variable indirection            (D=src; printf x > "$D/lib.rs")
#   * decoded/constructed commands    (base64 -d <<<... | sh ; eval "$(...)")
#   * command substitution in targets (printf x > "$(printf src/lib.rs)")
#   * writing through a helper script (./my-writer.sh, make, cargo build scripts)
#   * `git apply` / `patch`: the PATCH BODY is not inspected, so a diff that
#     touches src/** passes unless a protected path appears in the argv
#   * quoted paths containing spaces are tokenized naively
#   * `;`/`|`/`&&` inside a quoted string split the command incorrectly
#   * MultiEdit and NotebookEdit are NOT covered (only Edit, Write, Bash).
#     A named Phase-2 gap, not an oversight.
# Accepted over-block: a path whose ancestor directory is literally named src/
# engine/tests OUTSIDE the repo (e.g. ~/src/notes.md) is denied. Guarded seats
# work inside the repo, and over-blocking is the safe direction for a guard whose
# whole purpose is stopping source writes.
# THE HONEST GUARD REMAINS THE DISCIPLINE IN § 1.2 PLUS CODE REVIEW. This hook
# is a tripwire under that discipline, never a replacement for it.

trap 'exit 0' EXIT

MAX_PAYLOAD_BYTES=200000
GUARD_AUTHORITY='dev/design/orchestration.md § 1.2'

# --------------------------------------------------------------------------
# emit_deny <reason> — the whole deny contract, in one atomic printf so a
# partially-written envelope can never reach the harness.
# --------------------------------------------------------------------------
emit_deny() {
  local reason="$1"
  reason="${reason//\\/\\\\}"
  reason="${reason//\"/\\\"}"
  reason="${reason//$'\n'/ }"
  reason="${reason//$'\r'/ }"
  reason="${reason//$'\t'/ }"
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$reason"
  exit 0
}

# --------------------------------------------------------------------------
# normalize_path <path> — strip surrounding quotes, resolve `.` / `..`, drop
# empty segments. Returns the segment-joined path WITHOUT a leading slash;
# absolute and relative inputs therefore compare identically.
# --------------------------------------------------------------------------
normalize_path() {
  local p="$1" seg rest out n=0
  local -a parts=()
  p="${p%\"}"; p="${p#\"}"
  p="${p%\'}"; p="${p#\'}"
  [ -n "$p" ] || return 0
  rest="$p"
  while [ -n "$rest" ]; do
    seg="${rest%%/*}"
    if [ "$seg" = "$rest" ]; then rest=""; else rest="${rest#*/}"; fi
    case "$seg" in
      ''|'.') ;;
      '..') if [ "$n" -gt 0 ]; then n=$((n - 1)); unset "parts[$n]"; fi ;;
      *) parts[$n]="$seg"; n=$((n + 1)) ;;
    esac
  done
  [ "$n" -gt 0 ] || return 0
  local IFS=/
  out="${parts[*]}"
  printf '%s' "$out"
}

# --------------------------------------------------------------------------
# protected_reason <normalized-path> — prints WHY the path is protected, or
# nothing if it is not. Test-source basenames are checked FIRST so the
# precedence rule (test source beats the scripts/** allow) is structural.
# --------------------------------------------------------------------------
protected_reason() {
  local norm="$1" base seg rest
  [ -n "$norm" ] || return 0
  base="${norm##*/}"
  case "$base" in
    test_*.py|test_*.sh|test_*.rs|test_*.ts|test_*.js|\
    *_test.py|*_test.sh|*_test.rs|*_test.go|*_test.ts|*_test.js|\
    *.test.ts|*.test.tsx|*.test.js|*.test.mjs|\
    *.spec.ts|*.spec.tsx|*.spec.js|*.spec.mjs|conftest.py)
      printf "it is a test source (basename '%s')" "$base"
      return 0
      ;;
  esac
  rest="$norm"
  while [ -n "$rest" ]; do
    seg="${rest%%/*}"
    if [ "$seg" = "$rest" ]; then rest=""; else rest="${rest#*/}"; fi
    case "$seg" in
      src|engine|tests)
        printf "it is under a '%s/' path segment (source/test tree)" "$seg"
        return 0
        ;;
    esac
  done
  return 0
}

# --------------------------------------------------------------------------
# consider <raw-target> — normalize, test, and deny on the first protected hit.
# --------------------------------------------------------------------------
consider() {
  local raw="$1" norm why
  [ -n "$raw" ] || return 0
  case "$raw" in
    -*|'&'*|'|'*) return 0 ;;
  esac
  norm="$(normalize_path "$raw")"
  [ -n "$norm" ] || return 0
  why="$(protected_reason "$norm")"
  [ -n "$why" ] || return 0
  emit_deny "Blocked: the '${SEAT}' seat may not write ${raw} — ${why}. ${GUARD_AUTHORITY} reserves src/**, engine/** and test sources for the implementer seat; the coordinating seats write dev/plans/**, dev/design/**, STATUS boards, ledgers and scripts/**. Hand this edit to an implementer slice. (Seat read from ${SEAT_SOURCE}; set FATHOMDB_SEAT at session launch to change it.)"
}

# --------------------------------------------------------------------------
# strip_heredocs — drop heredoc BODIES from a command before scanning it.
# The body is DATA, not shell: markdown blockquotes and pasted shell examples
# routinely contain `>` and would otherwise deny at random. The heredoc's own
# redirection (`cat > FILE <<'EOF'`) stays on the introducing line and is still
# seen by the redirect scanner.
# --------------------------------------------------------------------------
strip_heredocs() {
  local line delim="" trimmed
  while IFS= read -r line || [ -n "$line" ]; do
    if [ -n "$delim" ]; then
      trimmed="${line#"${line%%[![:space:]]*}"}"
      trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
      if [ "$trimmed" = "$delim" ]; then delim=""; fi
      continue
    fi
    if [[ "$line" =~ \<\<-?[[:space:]]*[\'\"]?([A-Za-z_][A-Za-z0-9_]*)[\'\"]? ]]; then
      delim="${BASH_REMATCH[1]}"
    fi
    printf '%s\n' "$line"
  done
}

# --------------------------------------------------------------------------
# absorb_quoted_word — advance past the REST of a quoted shell word.
# `read -ra` splits on whitespace with no idea about quoting, so a single word
# like  's,^,# see src/rust/lib.rs,'  arrives as three tokens. When the token at
# index $k opens a quote it does not close, this walks $k forward to the token
# that closes it, so the shards of one word are never read as separate file
# operands. Backslash escapes are honoured outside single quotes, which is what
# makes the `'a'\''b'` idiom balance correctly.
#
# Relies on bash's DYNAMIC scoping: k, toks and count are scan_simple_command's
# locals and this is only ever called from there. Bounded by $count, so it
# cannot loop. Worst case it over-absorbs and the guard says nothing — the
# fail-open direction, consistent with the rest of the hook.
# --------------------------------------------------------------------------
absorb_quoted_word() {
  local q="" c word len idx
  while :; do
    word="${toks[k]}"
    len="${#word}"
    idx=0
    while [ "$idx" -lt "$len" ]; do
      c="${word:idx:1}"
      if [ -n "$q" ]; then
        if [ "$q" = '"' ] && [ "$c" = '\' ]; then
          idx=$((idx + 1))
        elif [ "$c" = "$q" ]; then
          q=""
        fi
      else
        case "$c" in
          '\')     idx=$((idx + 1)) ;;
          "'"|'"') q="$c" ;;
        esac
      fi
      idx=$((idx + 1))
    done
    [ -n "$q" ] || return 0
    [ $((k + 1)) -lt "$count" ] || return 0
    k=$((k + 1))
  done
}

# --------------------------------------------------------------------------
# scan_simple_command <one simple command> — the per-command target extractor.
# --------------------------------------------------------------------------
scan_simple_command() {
  local line="$1"
  local -a toks=()
  read -ra toks <<<"$line"
  local count="${#toks[@]}"
  [ "$count" -gt 0 ] || return 0

  local i tok next
  # --- output redirections. Covered, each verified by an arm in
  # scripts/tests/test_seat_path_guard.sh (arms 6, 10, 53-56):
  #     >  >>  1>  2>>  &>  &>>       spaced   (`> FILE`)
  #     >  >>  1>  2>>  &>  &>>       glued    (`>FILE`)
  #     >|  2>|                       both, via the `>|`->`>` rewrite that
  #                                   scan_bash_command applies BEFORE it splits
  #                                   on `|` (see the note there).
  # `>>|` and `&>|` are bash SYNTAX ERRORS — measured, not assumed — so no write
  # can happen through them and nothing needs guarding. The `>|`->`>` fold does
  # incidentally make them deny; that is a harmless over-block on a command that
  # could never run, recorded here so the code and this comment stay in step.
  # `>&2` cannot reach here as a target: the `&` split below has already put the
  # fd on its own line, leaving a bare `>` with no following token. `&>FILE`
  # survives that split as `>FILE`, which the glued branch catches.
  for ((i = 0; i < count; i++)); do
    tok="${toks[i]}"
    if [[ "$tok" =~ ^[0-9]*\>\>?$ ]]; then
      next="${toks[i + 1]-}"
      consider "$next"
    elif [[ "$tok" =~ ^[0-9]*\>\>?(.+)$ ]]; then
      consider "${BASH_REMATCH[1]}"
    fi
  done

  # --- verb-driven writers. Skip leading VAR=value assignments to find the verb.
  local j=0 verb
  while [ "$j" -lt "$count" ] && [[ "${toks[j]}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; do
    j=$((j + 1))
  done
  [ "$j" -lt "$count" ] || return 0
  verb="${toks[j]}"
  verb="${verb##*/}"

  local k last="" has_inplace=0 sub
  case "$verb" in
    tee)
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in -*) continue ;; esac
        consider "${toks[k]}"
      done
      ;;
    sed|gsed|perl|ruby)
      # THE EDIT PROGRAM IS NOT A FILE. These four take a script/expression
      # argument that is prose, not a path, and pushing it through consider()
      # denies allowed writes whose only sin is mentioning src/ in a
      # substitution (`sed -i 's/src/lib/' dev/plans/note.md`). Over-blocking is
      # how a guard gets switched off, so the operand rule is explicit:
      #
      #   1. A token starting with `-` is a flag, never a target.
      #   2. A short cluster containing `e`, `E` or `f` supplies the program:
      #      GLUED if characters follow that letter (`-e's/a/b/'`, `-pe'EXPR'`),
      #      otherwise the NEXT non-flag token is the program (`-e EXPR`,
      #      `-pe EXPR`, `-f script.sed`). Either way the program is consumed
      #      and never considered.
      #   3. `--expression`/`--file`, with or without `=`, do the same.
      #   4. If no flag supplied a program, the FIRST bare operand IS the
      #      program (`sed -i 's/a/b/' FILE...`) and is skipped.
      #   5. EVERY remaining bare operand is a file and IS considered.
      #
      # Whenever a token is identified as the program, absorb_quoted_word walks
      # past the rest of that shell word — `read -ra` split on whitespace, so a
      # program like `'s,^,# see src/rust/lib.rs,'` arrived as three tokens and
      # its middle shards would otherwise read as file operands.
      #
      # Only reached when an in-place flag is present; without one these
      # commands write nothing and the hook stays silent. In-place detection
      # runs over FLAG tokens only, so a program argument can never be mistaken
      # for `-i`.
      # Known imperfection, stated rather than hidden: BSD's `sed -i '' EXPR
      # FILE` passes the backup suffix as a SEPARATE argument, so rule 4 eats
      # the empty suffix and rule 5 then treats EXPR as a file. Harmless here
      # (an EXPR rarely normalizes onto a protected segment, and this repo's
      # hosts are GNU), but it is a real edge, not a covered one.
      local -a operands=()
      local arg flagbody before after want_prog=0 prog_seen=0
      for ((k = j + 1; k < count; k++)); do
        arg="${toks[k]}"
        if [ "$want_prog" -eq 1 ]; then
          case "$arg" in
            -*) want_prog=0 ;;   # a flag, not the program: fall through below
            *)  want_prog=0; prog_seen=1; absorb_quoted_word; continue ;;
          esac
        fi
        case "$arg" in
          --expression=*|--file=*) prog_seen=1; absorb_quoted_word; continue ;;
          --expression|--file)     want_prog=1; continue ;;
          --in-place*)             has_inplace=1; continue ;;
          --*)                     continue ;;
          -*)
            case "$arg" in *i*) has_inplace=1 ;; esac
            flagbody="${arg#-}"
            before="${flagbody%%[eEf]*}"
            if [ "$before" != "$flagbody" ]; then
              after="${flagbody#"$before"?}"
              if [ -n "$after" ]; then
                prog_seen=1
                absorb_quoted_word
              else
                want_prog=1
              fi
            fi
            continue
            ;;
          *)
            if [ "$prog_seen" -eq 0 ]; then
              prog_seen=1
              absorb_quoted_word
              continue
            fi
            operands+=("$arg")
            ;;
        esac
      done
      [ "$has_inplace" -eq 1 ] || return 0
      for arg in "${operands[@]}"; do
        consider "$arg"
      done
      ;;
    cp|mv|install|rsync|ln)
      # Destination is the last non-flag operand.
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in -*) continue ;; esac
        last="${toks[k]}"
      done
      consider "$last"
      ;;
    dd)
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in of=*) consider "${toks[k]#of=}" ;; esac
      done
      ;;
    rm|shred|unlink|touch|truncate|mkfifo)
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in
          -*) continue ;;
          [0-9]*) continue ;;
        esac
        consider "${toks[k]}"
      done
      ;;
    git)
      sub=""
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in -*) continue ;; esac
        sub="${toks[k]}"
        break
      done
      case "$sub" in
        checkout|restore|apply|mv|rm|stash|clean) ;;
        *) return 0 ;;
      esac
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in
          -*|"$sub") continue ;;
        esac
        consider "${toks[k]}"
      done
      ;;
  esac
}

# --------------------------------------------------------------------------
# scan_inline_interpreter <whole command> — `python3 -c` / `node -e` bodies.
# Handled on the WHOLE command because the body is one quoted argument that the
# whitespace tokenizer would shred. Only fires when the body carries a write
# indicator, so a read-only one-liner mentioning src/** is not denied.
# --------------------------------------------------------------------------
scan_inline_interpreter() {
  local cmd="$1"
  [[ "$cmd" =~ (python3?|node|ruby|perl)[[:space:]]+(-c|-e)([[:space:]]|$) ]] || return 0

  local writes=0
  case "$cmd" in
    *write_text*|*write_bytes*|*writeFileSync*|*appendFileSync*|*writeFile*|\
    *createWriteStream*|*os.replace*|*os.rename*|*shutil.copy*|*shutil.move*|\
    *File.write*|*IO.write*) writes=1 ;;
  esac
  if [ "$writes" -eq 0 ] && [[ "$cmd" == *"open("* ]]; then
    if [[ "$cmd" =~ [\'\"][waxWAX]b?\+? ]]; then writes=1; fi
  fi
  [ "$writes" -eq 1 ] || return 0

  # Every quoted literal that looks like a path becomes a candidate target.
  local rest="$cmd" guard=0
  while [ "$guard" -lt 64 ]; do
    guard=$((guard + 1))
    [[ "$rest" =~ [\'\"]([^\'\"]*/[^\'\"]*)[\'\"] ]] || break
    consider "${BASH_REMATCH[1]}"
    rest="${rest#*"${BASH_REMATCH[0]}"}"
  done
}

# --------------------------------------------------------------------------
# scan_bash_command — split into simple commands, then scan each.
# --------------------------------------------------------------------------
scan_bash_command() {
  local cmd="$1" stripped line
  [ -n "$cmd" ] || return 0

  scan_inline_interpreter "$cmd"

  stripped="$(printf '%s\n' "$cmd" | strip_heredocs)"
  # `>|` (clobber-override redirect) MUST be folded to `>` before the `|` split
  # below, or the split tears the operator in half and drops the target on a
  # line of its own where it is just a bare word — which is how both
  # `printf x >|src/lib.rs` and `printf x >| src/lib.rs` escaped the scanner
  # entirely. `>|` is a single unambiguous bash operator, and `>` is the same
  # redirection for scanning purposes, so the fold is lossless here. It runs
  # before the `||` fold too: in valid shell a `>|` can never be half of a `||`.
  stripped="${stripped//>|/>}"
  stripped="${stripped//&&/$'\n'}"
  stripped="${stripped//||/$'\n'}"
  stripped="${stripped//;/$'\n'}"
  stripped="${stripped//|/$'\n'}"
  stripped="${stripped//&/$'\n'}"
  stripped="${stripped//(/$'\n'}"
  stripped="${stripped//)/$'\n'}"

  while IFS= read -r line || [ -n "$line" ]; do
    [ -n "$line" ] || continue
    scan_simple_command "$line"
  done <<<"$stripped"
}

# ============================== main ========================================

# stderr is suppressed around the read because bash itself warns ("ignored null
# byte in input") when a command substitution swallows a NUL. That warning would
# be hook noise on a path that is about to fail open anyway, and a PreToolUse
# hook must be silent unless it is denying.
RAW=""
{ RAW="$(cat)" || RAW=""; } 2>/dev/null
[ -n "$RAW" ] || exit 0
[ "${#RAW}" -le "$MAX_PAYLOAD_BYTES" ] || exit 0

# --- field extraction. jq preferred, python3 as fallback; base64 transport so
# no field value (a multi-line Bash command, a path with odd bytes) can ever be
# mistaken for the delimiter.
FIELDS=""
if command -v jq >/dev/null 2>&1; then
  FIELDS="$(printf '%s' "$RAW" | jq -r '
    [(.tool_name // ""), (.agent_type // ""),
     (.tool_input.file_path // ""), (.tool_input.command // "")]
    | map(tostring | @base64) | .[]' 2>/dev/null)" || FIELDS=""
fi
if [ -z "$FIELDS" ] && command -v python3 >/dev/null 2>&1; then
  FIELDS="$(printf '%s' "$RAW" | python3 -c '
import base64, json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(1)
if not isinstance(d, dict):
    sys.exit(1)
ti = d.get("tool_input")
if not isinstance(ti, dict):
    ti = {}
def s(v):
    return v if isinstance(v, str) else ""
for v in (s(d.get("tool_name")), s(d.get("agent_type")),
          s(ti.get("file_path")), s(ti.get("command"))):
    sys.stdout.write(base64.b64encode(v.encode("utf-8", "replace")).decode() + "\n")
' 2>/dev/null)" || FIELDS=""
fi
[ -n "$FIELDS" ] || exit 0

B64D=""
if command -v base64 >/dev/null 2>&1; then
  B64D="base64 -d"
elif command -v python3 >/dev/null 2>&1; then
  B64D="python3 -m base64 -d"
fi
[ -n "$B64D" ] || exit 0

F_TOOL=""; F_AGENT=""; F_PATH=""; F_CMD=""
{
  IFS= read -r F_TOOL || true
  IFS= read -r F_AGENT || true
  IFS= read -r F_PATH || true
  IFS= read -r F_CMD || true
} <<<"$FIELDS"

# shellcheck disable=SC2086  # $B64D is a deliberate command+flag word split
b64d() { printf '%s' "$1" | $B64D 2>/dev/null; }

TOOL_NAME="$(b64d "$F_TOOL")"
AGENT_TYPE="$(b64d "$F_AGENT")"
FILE_PATH="$(b64d "$F_PATH")"
BASH_CMD="$(b64d "$F_CMD")"

# --- seat resolution (see header for the ordering rationale).
SEAT=""
SEAT_SOURCE=""
if [ -n "${FATHOMDB_SEAT:-}" ]; then
  SEAT="$FATHOMDB_SEAT"
  SEAT_SOURCE='the FATHOMDB_SEAT environment variable'
elif [ -n "$AGENT_TYPE" ]; then
  SEAT="$AGENT_TYPE"
  SEAT_SOURCE='the payload agent_type field'
else
  exit 0   # UNKNOWN seat — the ordinary main-thread session. Never decide.
fi

SEAT="${SEAT#"${SEAT%%[![:space:]]*}"}"
SEAT="${SEAT%"${SEAT##*[![:space:]]}"}"
SEAT="$(printf '%s' "$SEAT" | tr '[:upper:]' '[:lower:]')"

case "$SEAT" in
  orchestrator|steward) ;;
  *) exit 0 ;;
esac

case "$TOOL_NAME" in
  Edit|Write) consider "$FILE_PATH" ;;
  Bash)       scan_bash_command "$BASH_CMD" ;;
  *)          exit 0 ;;
esac

exit 0
