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
#   * an option that carries a PATH AS ITS VALUE is parsed only where it was
#     measured to matter: sed/perl `-e -f --expression --file`, cp/mv/install/ln
#     `-t --target-directory`, and git's value-taking globals (`-C`, `-c`,
#     `--git-dir`, `--work-tree`, `--namespace`, `--exec-path`, `--super-prefix`,
#     `--config-env`). Any OTHER option-carried path is invisible — e.g.
#     `rsync --files-from=…`, or a `-t` on rsync itself, which is deliberately
#     NOT read as a target because there it means "preserve times" and treating
#     its neighbour as a destination would be a false positive.
#   * git's value-taking global list is enumerated, so a global added by a
#     future git release would again be mistaken for the subcommand
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
# skip_as_flag <token> — the ONE place the question "is this token an OPTION
# rather than an operand?" is answered. Returns 0 = skip it, it is an option;
# returns 1 = it is an operand, deal with it. Every verb branch below routes
# through this, so the same mistake cannot survive in a sibling branch.
#
# The naive rule "it starts with `-` so it is a flag" is WRONG twice, and both
# errors point the dangerous way — they hide a target rather than invent one:
#   `-`   is NOT an option. It is the operand meaning stdin/stdout. Skipping it
#         is precisely how `sed -i -f - src/rust/lib.rs` got through: the `-`
#         (the script, read from stdin) was dropped, the program slot stayed
#         empty, and the real src/** FILE was then eaten as "the first bare
#         operand IS the program". No operand was ever considered.
#   `--`  ends the options. Every token after it is an operand however many
#         dashes it starts with.
#
# Relies on bash DYNAMIC scoping for ENDOPTS, which scan_simple_command owns and
# each verb branch resets — the same convention absorb_quoted_word uses for k.
# --------------------------------------------------------------------------
skip_as_flag() {
  [ "${ENDOPTS:-0}" -eq 0 ] || return 1
  case "$1" in
    --)  ENDOPTS=1; return 0 ;;
    -)   return 1 ;;
    -?*) return 0 ;;
  esac
  return 1
}

# --------------------------------------------------------------------------
# consider <raw-target> — normalize, test, and deny on the first protected hit.
#
# A leading `-` alone does NOT disqualify a token here either: `-/src/lib.rs` is
# a writable path under a directory named `-`, and refusing to normalize it was
# the same false negative one level down. A dash-leading token is dismissed only
# when it carries no `/` at all, which is the shape of a real option and cannot
# be the shape of a protected path (every protected path is either multi-segment
# or a bare test-source basename, and a basename does not start with `-`).
# Callers already strip their own options via skip_as_flag, so this is a second
# line of defence, deliberately biased toward considering.
# --------------------------------------------------------------------------
consider() {
  local raw="$1" norm why
  [ -n "$raw" ] || return 0
  case "$raw" in
    '&'*|'|'*) return 0 ;;
    -*) case "$raw" in */*) ;; *) return 0 ;; esac ;;
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
  # --- output redirections. Every form listed here names the arm in
  # scripts/tests/test_seat_path_guard.sh that asserts it, because "verified by
  # an arm" WAS the drift codex round 2 caught: the list claimed `&>>` and no
  # `&>>` arm existed. A claim without an arm number is not made.
  #
  #                  SPACED (`> FILE`)        GLUED (`>FILE`)
  #     >            arm 3 deny / arm 6 allow arm 61a
  #     >>           arm 10                   arm 61b
  #     1>           arm 56b                  arm 56a
  #     2>>          arm 56d                  arm 56c
  #     &>           arm 56f                  arm 56e
  #     &>>          arm 61d                  arm 61c   (+ allow half: arm 61d2)
  #     >|           arm 54                   arm 53
  #     2>|                                   arm 54b   (+ allow half: arm 55)
  #
  # The `>|` pair works via the `>|`->`>` rewrite that scan_bash_command applies
  # BEFORE it splits on `|` (see the note there).
  # `>>|` and `&>|` are bash SYNTAX ERRORS — re-measured with `bash -n` for
  # fix-2, both fail to parse — so no write can happen through them and nothing
  # needs guarding. The `>|`->`>` fold does incidentally make them deny; that is
  # a harmless over-block on a command that could never run, and arms 61e/61f
  # assert it so the code and this comment stay in step.
  # `>&2` cannot reach here as a target: the `&` split below has already put the
  # fd on its own line, leaving a bare `>` with no following token (arm 61g).
  # `&>FILE` survives that split as `>FILE`, which the glued branch catches.
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

  # ENDOPTS is the `--` end-of-options latch that skip_as_flag sets and reads by
  # dynamic scope. Every verb branch resets it, because it is per-command state.
  local k last="" has_inplace=0 sub ENDOPTS=0
  local arg flagbody before after
  case "$verb" in
    tee)
      ENDOPTS=0
      for ((k = j + 1; k < count; k++)); do
        skip_as_flag "${toks[k]}" && continue
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
      #   1. skip_as_flag decides what is an option. NOT "it starts with `-`":
      #      a bare `-` is an OPERAND (stdin) and `--` ends the options. See
      #      that function for why the naive rule was a false negative here.
      #   2. A short cluster containing `e`, `E` or `f` supplies the program:
      #      GLUED if characters follow that letter (`-e's/a/b/'`, `-pe'EXPR'`),
      #      otherwise the NEXT TOKEN is the program (`-e EXPR`, `-pe EXPR`,
      #      `-f script.sed`, and `-f -` = read the script from stdin). Either
      #      way the program is consumed and never considered.
      #   3. `--expression`/`--file`, with or without `=`, do the same.
      #   4. If no flag supplied a program, the FIRST bare operand IS the
      #      program (`sed -i 's/a/b/' FILE...`) and is skipped.
      #   5. EVERY remaining bare operand is a file and IS considered.
      #
      # Rule 2 makes the pending program slot swallow the next token WHATEVER it
      # looks like, including a dash-leading one. That is the deny-biased
      # reading: filling the slot early means later tokens are FILES and get
      # considered, where leaving it open means the next real path is mistaken
      # for the program and nothing is ever checked. Over-blocking a seat's
      # `sed` is a visible annoyance; under-blocking is a silent § 1.2 breach,
      # so every ambiguous case in this branch resolves toward DENY.
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
      local want_prog=0 prog_seen=0
      ENDOPTS=0
      for ((k = j + 1; k < count; k++)); do
        arg="${toks[k]}"
        # (a) a pending -e/-f/--expression/--file operand. THE NEXT TOKEN IS THE
        #     PROGRAM, whatever it looks like — `-f -` reads the script from
        #     stdin, and `-` is an operand, not a flag (rule 2).
        if [ "$want_prog" -eq 1 ]; then
          want_prog=0
          prog_seen=1
          absorb_quoted_word
          continue
        fi
        # (b) options, via the one shared decision (rules 1-3).
        if skip_as_flag "$arg"; then
          case "$arg" in
            --expression=*|--file=*) prog_seen=1; absorb_quoted_word ;;
            --expression|--file)     want_prog=1 ;;
            --in-place*)             has_inplace=1 ;;
            --*)                     : ;;
            *)
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
              ;;
          esac
          continue
        fi
        # (c) an operand: the first one is the program if no flag supplied one
        #     (rule 4), every later one is a file (rule 5).
        if [ "$prog_seen" -eq 0 ]; then
          prog_seen=1
          absorb_quoted_word
          continue
        fi
        operands+=("$arg")
      done
      [ "$has_inplace" -eq 1 ] || return 0
      for arg in "${operands[@]}"; do
        consider "$arg"
      done
      ;;
    cp|mv|install|rsync|ln)
      # Destination is the last non-flag operand — UNLESS an option supplied it.
      # `cp -t DIR file`, `install -m 644 -t DIR file` and the glued/long forms
      # put the destination IN THE OPTION, where a last-operand rule never looks:
      # measured, `cp -t src/rust dev/plans/x.md` was allowed (arm 64). Handled
      # for cp/mv/install/ln only: rsync's `-t` means "preserve times" and has no
      # operand, so reading its neighbour as a target would be a false positive.
      local want_target=0 has_t=0
      case "$verb" in cp|mv|install|ln) has_t=1 ;; esac
      ENDOPTS=0
      for ((k = j + 1; k < count; k++)); do
        tok="${toks[k]}"
        if [ "$want_target" -eq 1 ]; then
          want_target=0
          consider "$tok"
          continue
        fi
        if skip_as_flag "$tok"; then
          [ "$has_t" -eq 1 ] || continue
          case "$tok" in
            --target-directory=*) consider "${tok#--target-directory=}"; continue ;;
            --target-directory)   want_target=1; continue ;;
            --*)                  continue ;;
            *)
              # short cluster: `-t DIR` (t last) or `-tDIR` / `-vtDIR` (glued).
              flagbody="${tok#-}"
              before="${flagbody%%t*}"
              if [ "$before" != "$flagbody" ]; then
                after="${flagbody#"$before"?}"
                if [ -n "$after" ]; then consider "$after"; else want_target=1; fi
              fi
              continue
              ;;
          esac
        fi
        last="$tok"
      done
      consider "$last"
      ;;
    dd)
      for ((k = j + 1; k < count; k++)); do
        case "${toks[k]}" in of=*) consider "${toks[k]#of=}" ;; esac
      done
      ;;
    rm|shred|unlink|touch|truncate|mkfifo)
      ENDOPTS=0
      for ((k = j + 1; k < count; k++)); do
        skip_as_flag "${toks[k]}" && continue
        case "${toks[k]}" in [0-9]*) continue ;; esac
        consider "${toks[k]}"
      done
      ;;
    git)
      # git's GLOBAL options take their value in the NEXT TOKEN, and the old
      # "skip anything starting with -, the next bare word is the subcommand"
      # scan therefore read `-C`'s VALUE as the subcommand: `git -C "$WT"
      # checkout -- src/rust/lib.rs` resolved sub="$WT", matched none of the
      # write verbs, and allowed the rewrite. That is the coordinating seats'
      # own idiom, so it was the most reachable false negative in the file
      # (arm 63). The value-taking globals are enumerated; the `--opt=value`
      # spellings need no entry because the value travels inside the token.
      sub=""
      local -a gitargs=()
      local skipnext=0
      ENDOPTS=0
      for ((k = j + 1; k < count; k++)); do
        if [ "$skipnext" -eq 1 ]; then skipnext=0; continue; fi
        if [ "$ENDOPTS" -eq 0 ]; then
          case "${toks[k]}" in
            -C|-c|--git-dir|--work-tree|--namespace|--exec-path|--super-prefix|--config-env)
              skipnext=1; continue ;;
          esac
        fi
        skip_as_flag "${toks[k]}" && continue
        if [ -z "$sub" ]; then sub="${toks[k]}"; continue; fi
        gitargs+=("${toks[k]}")
      done
      case "$sub" in
        checkout|restore|apply|mv|rm|stash|clean) ;;
        *) return 0 ;;
      esac
      for arg in ${gitargs[@]+"${gitargs[@]}"}; do
        consider "$arg"
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
