#!/usr/bin/env bash
# scripts/tests/test_check_transcript_hygiene.sh — coverage for TC-86 transcript
# hygiene: the shared pattern (scripts/lib/agent-state-paths.sh), the
# CAPTURE-TIME filter folded into dev/agent-tools/codex-nostdin.sh, and the
# LANDING GATE scripts/check-transcript-hygiene.sh with both of its homes
# (`preflight.sh --landing` = PREVENT, an always-on CI job = DETECT).
#
# TC-86, steward `seq-129`, todos `TC-86`, master `F-36`. Cross-cutting — no
# slice number, no `R-20-xx` id, no pico label.
#
# ======================== WHAT IS BEING PROTECTED ============================
# A codex §9 review transcript arrived carrying 216 lines of raw Claude Code
# session JSONL — codex, running under --dangerously-bypass-approvals-and-sandbox,
# had run `rg` across ~/.claude and slurped the results into its own stdout,
# which TC-RUBRIC-7 then required be `tee`d to a TRACKED path under
# dev/plans/runs/codex/. Those lines held conversation content from three
# projects other than this one, and github.com/coreyt/fathomdb is PUBLIC. It was
# caught and redacted before landing; `git grep` proved zero already-committed
# files carry that shape, so reachability in history is ZERO.
#
# The mechanism is STRUCTURAL: every §9 review runs with the bypass flag,
# TC-RUBRIC-7 requires the transcript be persisted under a tracked path, and
# nothing in between inspects the contents — while the person committing is
# reading a verdict at the end of a multi-megabyte file.
#
# ============== WHY THE POSITIVE CONTROL IS THE LOAD-BEARING ARM =============
# The real tree PASSES today (that is the point — a gate that lands red is the
# TC-16 / F-30 failure this repo is still carrying). So asserting only against
# the real repo would prove nothing: `exit 0` would pass it. Arms B/C/E below run
# against purpose-built DIRTY FIXTURES so each can only go green because the
# predicate actually FIRED. This is the arm-16a / TC-79 lesson stated plainly:
# a hygiene gate that cannot be demonstrated FAILING is worse than none, because
# it converts an unexamined tree into a certified one.
#
# ================== WHY EVERY FIXTURE LINE IS CONCATENATED ===================
# `slurped_line` and friends build their output through `printf` format
# placeholders rather than writing a literal path. That is not obfuscation and it
# is not an exemption: a literal slurped path in this file would make the SUITE
# ITSELF a match, and the gate scans all tracked files — so the fixtures are
# assembled at runtime and this file stays honestly clean under its own gate. No
# path is excluded from the scan to make that work.
#
# THE FIXTURES ARE THROWAWAY. Every dirty fixture lives under `mktemp -d`; no
# real transcript under dev/plans/runs/** is ever read for mutation or written.
# Rewriting those is explicitly out of scope for TC-86 (they are clean —
# verified).
#
# ============================ EXIT-CODE HONESTY ==============================
# Every arm captures rc on the line immediately following the command, before any
# command substitution. The wrapper arms (L–R) exist because the wrapper had to
# stop using `exec` to gain a filter, and a pipeline reports the LAST command's
# status: without PIPESTATUS every review would report `sed`'s success instead of
# codex's verdict. That is the exact exit-code-capture bug this repo has been
# bitten by twice, so it is asserted with a stub codex rather than argued.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="$REPO_ROOT/scripts/check-transcript-hygiene.sh"
WRAPPER="$REPO_ROOT/dev/agent-tools/codex-nostdin.sh"
SHARED_LIB="$REPO_ROOT/scripts/lib/agent-state-paths.sh"
PREFLIGHT="$REPO_ROOT/scripts/preflight.sh"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"

# The no-argument arm exercises the checker's repo-relative default (tracked
# files of the enclosing checkout). Pin cwd so it tests THIS tree wherever the
# suite is invoked from — mirrors the c1-conformance / governed-surface suites.
cd "$REPO_ROOT"

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

run_checker() {
  set +e
  OUT="$(bash "$CHECKER" "$@" 2>&1)"
  RC=$?
  set -e
}

expect_rc() {
  local want="$1" desc="$2"
  if [ "$RC" -eq "$want" ]; then
    pass "$desc"
  else
    fail "$desc — expected rc=$want, got rc=$RC; out: $OUT"
  fi
}

expect_out() {
  if printf '%s' "$OUT" | grep -qE -- "$1"; then
    pass "$2"
  else
    fail "$2 — expected output matching /$1/; got: $OUT"
  fi
}

expect_no_out() {
  if printf '%s' "$OUT" | grep -qE -- "$1"; then
    fail "$2 — output must NOT match /$1/; got: $OUT"
  else
    pass "$2"
  fi
}

# ---------------------------------------------------------------------------
# Fixture builders. See "WHY EVERY FIXTURE LINE IS CONCATENATED" above: the
# `%s` placeholders keep a literal agent-state path out of this file's bytes.
# ---------------------------------------------------------------------------

# slurped_line [project-dir] — one line of the exact shape that leaked: `rg`
# output, i.e. an absolute session-transcript path, a colon, then raw JSONL.
slurped_line() {
  printf '/home/coreyt/%s/projects/%s/019fa6d5-3392-77e3-9890-bdc0c24403c1.jsonl:{"type":"user","message":{"role":"user","content":"CONVERSATION CONTENT FROM ANOTHER PROJECT"}}\n' \
    '.claude' "${1:--home-coreyt-projects-memex}"
}

# macos_slurped_line — the /Users/ (macOS) spelling of the same shape.
macos_slurped_line() {
  printf '/Users/alice/%s/projects/%s/subagents/7f3c1a90-0b2e-4d55-9a01-2c6f8e4b1d33.jsonl:{"type":"assistant"}\n' \
    '.claude' "${1:--Users-alice-work-secret-client}"
}

# synthetic_placeholder_line — the PreToolUse fixture already tracked in
# scripts/tests/test_seat_path_guard.sh (and quoted back inside ten codex
# transcripts under dev/plans/runs/codex/**). `x` is an invented placeholder, not
# an encoded cwd, so it is NOT a leak and MUST NOT be flagged.
synthetic_placeholder_line() {
  printf '    "transcript_path": "/home/nobody/%s/projects/x/y.jsonl",\n' '.claude'
}

# relative_claude_refs — repo configuration this project's docs discuss
# constantly. The false-positive control.
relative_claude_refs() {
  cat <<'REL'
See .claude/agents/steward.md for the coordinating seat definition.
The PreToolUse guard lives at .claude/hooks/seat-path-guard.sh and ships unwired.
It is deliberately NOT wired via .claude/settings.json, which is project-global.
Frontmatter anchors ${CLAUDE_PROJECT_DIR}/.claude/hooks/seat-path-guard.sh.
REL
}

# prior_banner_text — the redaction banner ALREADY ON MAIN in
# dev/plans/runs/codex/agent-seat-hardening/ASH-Phase2-20260728T034657Z.log. It
# quotes the incident's own `git grep` invocation. A gate that tripped on the
# record of its predecessor incident would be self-defeating, so this is a
# control, not a curiosity.
prior_banner_text() {
  cat <<'BAN'
=== STEWARD REDACTION 2026-07-28 ===
216 line(s) removed from this transcript before it was committed.
WHAT THEY WERE: codex, running under --dangerously-bypass-approvals-and-sandbox,
ran `rg` across ~/.claude and slurped raw Claude Code SESSION JSONL into its own
output.
NO PRIOR EXPOSURE: `git grep -l '^/home/coreyt/.claude/projects/' -- dev/plans/runs/codex/**`
returned ZERO committed files, so this was a first occurrence caught before landing.
=== END STEWARD REDACTION ===
BAN
}

CLEAN_PROSE='OpenAI Codex v0.136.0 -- workdir: /home/coreyt/projects/fathomdb -- verdict: no [P1].'

# ============================================================================
# Arm A — the REGRESSION half: the real tree is clean, over ALL TRACKED FILES.
# ============================================================================
# TC-16 / F-30: a new always-on CI job that is red the day it lands is worse than
# no job, because the repo learns to ignore a red main. This arm is the promise
# that TC-86 did not ship one.
run_checker
expect_rc 0 "the real tree passes the transcript-hygiene gate over all tracked files"
expect_out 'transcript-hygiene: no .*agent-state path' "the clean run reports the specific predicate it verified, not a bare 'ok'"

# ============================================================================
# Arm B — POSITIVE CONTROL. A slurped line IS detected.
# ============================================================================
B_DIR="$TMPROOT/dirty-one"
mkdir -p "$B_DIR/dev/plans/runs/codex/some-slice"
{
  printf '%s\n' "$CLEAN_PROSE"
  slurped_line
  printf 'verdict: LGTM\n'
} >"$B_DIR/dev/plans/runs/codex/some-slice/review.log"

run_checker --root "$B_DIR"
expect_rc 1 "a transcript carrying a slurped agent-state path FAILS the gate"
expect_out 'FAIL' "the failure is reported on a FAIL line (preflight parses these)"
expect_out 'review\.log' "the failure NAMES the offending file"
expect_out '1 line' "the failure reports a match count"
expect_out 'PUBLIC' "the failure explains the stake (this repository is public)"
expect_out '--redact' "the failure tells the reader how to fix it"

# ============================================================================
# Arm C — the reported count is real, not a boolean dressed up as one.
# ============================================================================
C_DIR="$TMPROOT/dirty-three"
mkdir -p "$C_DIR/dev/plans/runs"
{
  slurped_line '-home-coreyt-projects-agent-aware'
  printf '%s\n' "$CLEAN_PROSE"
  slurped_line '-home-coreyt-projects-local-unifi-openwrt'
  slurped_line '-home-coreyt-projects-memex'
} >"$C_DIR/dev/plans/runs/three.log"

run_checker --root "$C_DIR"
expect_rc 1 "three slurped lines still fail the gate"
expect_out '3 (line|match)' "the failure reports the true match count (3), not a boolean"

# ============================================================================
# Arm D — FALSE-POSITIVE CONTROL. Relative `.claude/...` refs are NOT flagged.
# ============================================================================
# Repo docs discuss .claude/agents/*.md, .claude/hooks/* and .claude/settings.json
# constantly — several such paths were added to dev/design/orchestration.md and
# the STATUS board this week. A gate that trips on those is unusable and would be
# switched off, which is a worse outcome than not having it.
D_DIR="$TMPROOT/relative-only"
mkdir -p "$D_DIR/dev/design"
relative_claude_refs >"$D_DIR/dev/design/orchestration.md"

run_checker --root "$D_DIR"
expect_rc 0 "relative .claude/ references (agents, hooks, settings.json) are NOT flagged"

# ============================================================================
# Arm E — /Users/ (macOS) matches as well as /home/.
# ============================================================================
E_DIR="$TMPROOT/macos"
mkdir -p "$E_DIR"
{ printf '%s\n' "$CLEAN_PROSE"; macos_slurped_line; } >"$E_DIR/mac.log"

run_checker --root "$E_DIR"
expect_rc 1 "the /Users/ (macOS) spelling is detected too"
expect_out 'mac\.log' "the /Users/ failure names its file"

# ============================================================================
# Arm F — the SYNTHETIC placeholder already tracked in the repo stays clean.
# ============================================================================
# scripts/tests/test_seat_path_guard.sh:98 carries
# "transcript_path": "/home/nobody/<state-dir>/projects/x/y.jsonl" as a PreToolUse
# payload fixture, and ten codex transcripts quote it back. Claude Code names a
# project directory after the absolute cwd with `/` -> `-`, so a REAL one always
# begins with `-`; `x` does not. Without that discriminator this gate would land
# RED on eleven tracked files.
F_DIR="$TMPROOT/synthetic"
mkdir -p "$F_DIR"
{ printf '%s\n' "$CLEAN_PROSE"; synthetic_placeholder_line; } >"$F_DIR/seat-fixture.log"

run_checker --root "$F_DIR"
expect_rc 0 "the synthetic /home/nobody/.../projects/x/y.jsonl fixture is NOT flagged"

# ============================================================================
# Arm G — the PRIOR incident's own redaction banner does not trip the gate.
# ============================================================================
G_DIR="$TMPROOT/prior-banner"
mkdir -p "$G_DIR"
prior_banner_text >"$G_DIR/ASH-Phase2.log"

run_checker --root "$G_DIR"
expect_rc 0 "the redaction banner already on main (which quotes the incident's git grep) is NOT flagged"

# ============================================================================
# Arm H — --redact: REDACT, NEVER DELETE.
# ============================================================================
# TC-RUBRIC-7 closes a review on a persisted terminal artifact, so the transcript
# must survive as evidence. --redact therefore rewrites matched lines in place
# and prepends a banner; it must never drop the file or the surrounding content.
H_DIR="$TMPROOT/redact"
mkdir -p "$H_DIR"
H_FILE="$H_DIR/review.log"
{
  printf 'line-before-untouched\n'
  slurped_line '-home-coreyt-projects-memex'
  printf 'line-after-untouched\n'
  macos_slurped_line
  printf 'verdict: no [P1] findings\n'
} >"$H_FILE"
H_SLURPED_COUNT=2

run_checker --root "$H_DIR" --redact
expect_rc 0 "--redact exits 0 after cleaning"
expect_out 'redact' "--redact reports what it did"

if [ -f "$H_FILE" ]; then
  pass "--redact leaves the file PRESENT (evidence survives; TC-RUBRIC-7)"
else
  fail "--redact must never delete the transcript"
fi

if grep -q 'STEWARD REDACTION' "$H_FILE"; then
  pass "--redact prepends the shared redaction banner"
else
  fail "expected the shared banner in the redacted file; got: $(cat "$H_FILE")"
fi
if grep -q "$H_SLURPED_COUNT line" "$H_FILE"; then
  pass "the banner states the removed-line count"
else
  fail "the banner must state how many lines were removed; got: $(cat "$H_FILE")"
fi
if grep -q -- '-home-coreyt-projects-memex' "$H_FILE" && grep -q -- '-Users-alice-work-secret-client' "$H_FILE"; then
  pass "the banner names the foreign projects whose content was removed"
else
  fail "the banner must name the foreign projects; got: $(cat "$H_FILE")"
fi
if grep -q 'PUBLIC' "$H_FILE" && grep -qi 'no .*finding' "$H_FILE"; then
  pass "the banner states the reason and that no finding came from the removed lines"
else
  fail "the banner must state the reason and the no-finding-lost claim; got: $(cat "$H_FILE")"
fi
if grep -q 'REDACTED TC-86' "$H_FILE"; then
  pass "each removed line is REPLACED IN PLACE by the shared marker"
else
  fail "expected the in-place redaction marker; got: $(cat "$H_FILE")"
fi
if grep -q 'CONVERSATION CONTENT FROM ANOTHER PROJECT' "$H_FILE"; then
  fail "the slurped session content SURVIVED --redact; got: $(cat "$H_FILE")"
else
  pass "the slurped session content is gone from the redacted file"
fi
if grep -qx 'line-before-untouched' "$H_FILE" \
   && grep -qx 'line-after-untouched' "$H_FILE" \
   && grep -qx 'verdict: no \[P1\] findings' "$H_FILE"; then
  pass "surrounding content is byte-intact (only matched lines were rewritten)"
else
  fail "--redact disturbed unmatched lines; got: $(cat "$H_FILE")"
fi

run_checker --root "$H_DIR"
expect_rc 0 "a re-run of the gate over the redacted tree now PASSES"

# ============================================================================
# Arm I — --redact is IDEMPOTENT. A second pass changes nothing.
# ============================================================================
# If the banner (or the marker) matched the pattern, --redact would stack banners
# forever and could never converge. This arm is that proof, by bytes.
cp "$H_FILE" "$TMPROOT/redact-first-pass.snapshot"
run_checker --root "$H_DIR" --redact
expect_rc 0 "a second --redact exits 0"
if cmp -s "$H_FILE" "$TMPROOT/redact-first-pass.snapshot"; then
  pass "a second --redact leaves the file BYTE-IDENTICAL (idempotent)"
else
  fail "--redact is not idempotent; diff: $(diff "$TMPROOT/redact-first-pass.snapshot" "$H_FILE" || true)"
fi

# ============================================================================
# Arm J — explicit path arguments, and a missing path is an ERROR not a pass.
# ============================================================================
run_checker "$C_DIR/dev/plans/runs/three.log"
expect_rc 1 "an explicit file argument is scanned"

run_checker "$D_DIR/dev/design/orchestration.md"
expect_rc 0 "an explicit clean file argument passes"

run_checker "$TMPROOT/does-not-exist.log"
expect_rc 2 "an unreadable path is a hard ERROR (exit 2), never a silent pass"

# ============================================================================
# Arms L–R — the CAPTURE-TIME filter in dev/agent-tools/codex-nostdin.sh.
# ============================================================================
STUB_DIR="$TMPROOT/stub-bin"
mkdir -p "$STUB_DIR"

# make_stub <exit-code> <body...> — a fake `codex` on PATH.
make_stub() {
  local rc="$1"; shift
  {
    printf '#!/usr/bin/env bash\n'
    printf '%s\n' "$@"
    printf 'exit %s\n' "$rc"
  } >"$STUB_DIR/codex"
  chmod +x "$STUB_DIR/codex"
}

run_wrapper() {
  set +e
  OUT="$(PATH="$STUB_DIR:$PATH" bash "$WRAPPER" "$@" 2>&1)"
  RC=$?
  set -e
}

# ---- Arm L: EXIT-CODE PRESERVATION. The load-bearing wrapper arm. ----------
# The wrapper had to give up `exec` to gain a filter. A pipeline reports the LAST
# command's status, so without PIPESTATUS captured on the very next line every
# review would report `sed`'s success — a §9 FAIL silently reported as a pass.
make_stub 42 'echo "codex ran and failed"'
run_wrapper exec review --base deadbeef
expect_rc 42 "the wrapper preserves codex's NON-ZERO exit code through the filter (PIPESTATUS)"
expect_out 'codex ran and failed' "the wrapper still relays codex's output"

make_stub 7 'echo hi'
run_wrapper exec --dangerously-bypass-approvals-and-sandbox 'prompt'
expect_rc 7 "a second non-zero code is preserved too (not just a fixed sentinel)"

make_stub 0 'echo "clean review"'
run_wrapper exec review
expect_rc 0 "a zero exit code stays zero"

# ---- Arm M: the wrapper ACTUALLY FILTERS ----------------------------------
# The stub reproduces the incident: it emits an `rg` hit against ~/.claude in the
# middle of an otherwise ordinary review.
make_stub 0 \
  'echo "OpenAI Codex v0.136.0"' \
  'printf "/home/coreyt/%s/projects/%s/019fa6d5.jsonl:{\"content\":\"LEAKED SESSION TEXT\"}\n" ".claude" "-home-coreyt-projects-memex"' \
  'echo "verdict: no [P1]"'
run_wrapper exec review
expect_rc 0 "the filtering run still reports codex's own exit code"
expect_no_out 'LEAKED SESSION TEXT' "the slurped session content NEVER reaches the wrapper's output"
expect_out 'REDACTED TC-86' "the filtered line is replaced by the shared marker, not silently dropped"
expect_out 'OpenAI Codex v0\.136\.0' "ordinary codex output before the slurped line is untouched"
expect_out 'verdict: no \[P1\]' "ordinary codex output after the slurped line is untouched"

# ---- Arm N: relative .claude/ output survives the wrapper ------------------
# A §9 review that discusses .claude/agents/steward.md must be able to say so.
make_stub 0 'echo "the guard lives at .claude/hooks/seat-path-guard.sh and is unwired in .claude/settings.json"'
run_wrapper exec review
expect_out 'seat-path-guard\.sh and is unwired' "the wrapper does NOT redact relative .claude/ references"

# ---- Arm O: `codex` absent still exits 127, unchanged ---------------------
EMPTY_BIN="$TMPROOT/empty-bin"
mkdir -p "$EMPTY_BIN"
set +e
OUT="$(PATH="$EMPTY_BIN" bash "$WRAPPER" exec review 2>&1)"
RC=$?
set -e
expect_rc 127 "the pre-existing 'codex not found' behaviour (exit 127) is intact"
expect_out "not found" "the 127 path still explains itself"

# ---- Arm P: arguments are forwarded VERBATIM -------------------------------
make_stub 0 'printf "ARGS:[%s]\n" "$*"'
run_wrapper exec review --base deadbeef --dangerously-bypass-approvals-and-sandbox
expect_out 'ARGS:\[exec review --base deadbeef --dangerously-bypass-approvals-and-sandbox\]' \
  "all arguments still pass through the wrapper untouched"

# ---- Arm Q: stdin is STILL closed (the wrapper's original reason to exist) --
make_stub 0 'if read -r _line; then echo "STDIN-HAD-DATA"; else echo "STDIN-AT-EOF"; fi'
run_wrapper exec review
expect_out 'STDIN-AT-EOF' "stdin is still closed — the wrapper's original anti-deadlock contract holds"

# ---- Arm R: ANTI-FAIL-OPEN. No shared pattern => codex does not run --------
# If the wrapper could not source the pattern it must refuse, not run codex
# unfiltered: an unfiltered run is exactly the incident.
ORPHAN="$TMPROOT/orphan/dev/agent-tools"
mkdir -p "$ORPHAN"
cp "$WRAPPER" "$ORPHAN/codex-nostdin.sh"
SENTINEL="$TMPROOT/orphan-codex-ran"
make_stub 0 "touch '$SENTINEL'" 'echo "codex should NOT have run"'
set +e
OUT="$(PATH="$STUB_DIR:$PATH" bash "$ORPHAN/codex-nostdin.sh" exec review 2>&1)"
RC=$?
set -e
if [ "$RC" -ne 0 ]; then
  pass "the wrapper refuses (non-zero) when the shared pattern file is missing"
else
  fail "the wrapper must NOT succeed without its filter; out: $OUT"
fi
if [ -e "$SENTINEL" ]; then
  fail "codex RAN UNFILTERED when the shared pattern was missing — fail-open; out: $OUT"
else
  pass "codex is not invoked at all when the filter is unavailable (fail CLOSED)"
fi

# ============================================================================
# Arm S — EXACTLY ONE definition of the pattern. The anti-drift assertion.
# ============================================================================
# If the capture-time filter and the landing gate ever carry separate copies of
# the regex, the gate certifies files the filter would have cleaned, or vice
# versa — a false green built into the design. This arm makes the drift
# impossible to reintroduce silently. The needle is split so this file does not
# itself count as a definition.
RE_NEEDLE='AGENT_STATE_PATH_RE''='
DEF_FILES="$(git -C "$REPO_ROOT" grep -l -- "$RE_NEEDLE" || true)"
DEF_COUNT="$(printf '%s' "$DEF_FILES" | grep -c . || true)"
if [ "$DEF_COUNT" -eq 1 ]; then
  pass "the agent-state pattern is DEFINED exactly once in the tree"
else
  fail "expected exactly ONE definition of the shared pattern, found $DEF_COUNT: $DEF_FILES"
fi
if [ "$DEF_FILES" = 'scripts/lib/agent-state-paths.sh' ]; then
  pass "the single definition lives in scripts/lib/agent-state-paths.sh"
else
  fail "the pattern must be defined in scripts/lib/agent-state-paths.sh, not $DEF_FILES"
fi

# ============================================================================
# Arm T — both consumers SOURCE that one definition.
# ============================================================================
for consumer in "$CHECKER" "$WRAPPER"; do
  if [ ! -f "$consumer" ]; then
    fail "consumer $consumer does not exist"
    continue
  fi
  if grep -q 'agent-state-paths\.sh' "$consumer"; then
    pass "$(basename "$consumer") sources the shared pattern rather than restating it"
  else
    fail "$(basename "$consumer") must source scripts/lib/agent-state-paths.sh"
  fi
done

if [ -f "$SHARED_LIB" ]; then
  pass "scripts/lib/agent-state-paths.sh exists"
else
  fail "scripts/lib/agent-state-paths.sh must exist (Layer 0)"
fi

# The shared library must be CLEAN under its own pattern — otherwise the gate
# fails on the file that defines it and the whole design is unusable.
if [ -f "$SHARED_LIB" ]; then
  run_checker "$SHARED_LIB"
  expect_rc 0 "the shared pattern file is itself clean under the pattern it defines"
fi

# ============================================================================
# Arms U/V — the two homes. CI is the load-bearing one.
# ============================================================================
# A pre-commit hook alone is bypassable with --no-verify, so the CI job must be
# ALWAYS-ON: no `if:`, no `needs:`, not docs_only-gated. A transcript is a `.log`
# under dev/plans/runs/**, which is a NON-markdown path, so a docs_only fast path
# would not have excluded it — but a `needs: changes` gate would still make the
# job's presence contingent on another job, and the whole point is that this one
# always runs.
CI_JOB_BLOCK="$(awk '
  /^  transcript-hygiene:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

if [ -n "$CI_JOB_BLOCK" ]; then
  pass "ci.yml defines a transcript-hygiene job"
else
  fail "ci.yml has no transcript-hygiene job"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/check-transcript-hygiene.sh'; then
  pass "the CI job runs the SHARED scripts/check-transcript-hygiene.sh (one predicate, two callers)"
else
  fail "the CI job must invoke scripts/check-transcript-hygiene.sh, not a reimplementation"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'scripts/tests/test_check_transcript_hygiene.sh'; then
  pass "the CI job carries the recurrence guard for the gate itself"
else
  fail "the CI job must also run scripts/tests/test_check_transcript_hygiene.sh"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*if:'; then
  fail "the transcript-hygiene job must be ALWAYS-ON (no if:/docs_only gate); block: $CI_JOB_BLOCK"
else
  pass "the transcript-hygiene job is always-on (no if: condition, not docs_only-gated)"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -qE '^\s*needs:'; then
  fail "the transcript-hygiene job must not depend on the changes job; block: $CI_JOB_BLOCK"
else
  pass "the transcript-hygiene job has no needs: (does not ride the changes/docs_only fast path)"
fi
if printf '%s' "$CI_JOB_BLOCK" | grep -q 'actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0'; then
  pass "the CI job pins actions/checkout to the same SHA as its sibling jobs"
else
  fail "the CI job must pin actions/checkout by SHA, as the sibling gate jobs do"
fi

if grep -q 'scripts/tests/test_check_transcript_hygiene.sh' "$REPO_ROOT/scripts/agent-test.sh"; then
  pass "agent-test.sh registers this fixture suite alongside its siblings"
else
  fail "scripts/agent-test.sh must register scripts/tests/test_check_transcript_hygiene.sh"
fi

# ============================================================================
# Arm W — preflight.sh --landing wiring, FUNCTIONALLY, on throwaway repos.
# ============================================================================
# Static "the string appears in preflight.sh" would pass on a block that never
# runs. These arms build real throwaway repos + linked worktrees (TC-RUBRIC-5
# requires landing from a linked worktree) and prove the gate BLOCKS a land.
# The fixture seeds the sibling gates' minimal fixtures for the same reason
# test_check_ledgers.sh does: --landing runs them all, and an unrelated red would
# make these arms prove nothing about THIS gate.
NO_HOOKS="$TMPROOT/no-hooks"
mkdir -p "$NO_HOOKS"

# shellcheck source=lib/governed-surface-fixture.sh
. "$SCRIPT_DIR/lib/governed-surface-fixture.sh"
# shellcheck source=lib/c1-conformance-fixture.sh
. "$SCRIPT_DIR/lib/c1-conformance-fixture.sh"

write_ledger_fixture() {
  local root="$1" rel="$2"
  mkdir -p "$(dirname "$root/$rel")"
  printf '{"seq":1,"note":"entry 1"}\n{"seq":2,"note":"entry 2"}\n' >"$root/$rel.jsonl"
  printf '2' >"$root/$rel.jsonl.seq"
}

# make_repo <primary> <linked> <dirty:0|1>
make_repo() {
  local primary="$1" linked="$2" dirty="$3"
  mkdir -p "$primary"
  git init -q -b main "$primary"
  git -C "$primary" config user.email transcript-hygiene-test@example.invalid
  git -C "$primary" config user.name 'Transcript Hygiene Test'
  git -C "$primary" config commit.gpgsign false
  git -C "$primary" config core.hooksPath "$NO_HOOKS"
  mkdir -p "$primary/src" "$primary/scripts" "$primary/dev/plans/runs/codex"
  printf 'fixture\n' >"$primary/src/keep.txt"
  write_ledger_fixture "$primary" dev/steward/steward-ledger
  seed_governed_surface_fixture "$primary"
  seed_c1_conformance_fixture "$primary"
  {
    printf 'OpenAI Codex v0.136.0\n'
    [ "$dirty" -eq 1 ] && slurped_line '-home-coreyt-projects-memex'
    printf 'verdict: no [P1]\n'
  } >"$primary/dev/plans/runs/codex/review.log"
  git -C "$primary" add -A
  git -C "$primary" commit -q -m 'fixture: initial commit'
  git -C "$primary" worktree add -q -b landing-fixture "$linked" >/dev/null 2>&1
}

run_preflight() {
  local cwd="$1"; shift
  set +e
  OUT="$(cd "$cwd" && bash "$PREFLIGHT" "$@" 2>&1)"
  RC=$?
  set -e
}

DIRTY_PRIMARY="$TMPROOT/repo-dirty"
DIRTY_LINKED="$TMPROOT/repo-dirty-wt"
make_repo "$DIRTY_PRIMARY" "$DIRTY_LINKED" 1

CLEAN_PRIMARY="$TMPROOT/repo-clean"
CLEAN_LINKED="$TMPROOT/repo-clean-wt"
make_repo "$CLEAN_PRIMARY" "$CLEAN_LINKED" 0

run_preflight "$DIRTY_LINKED" --landing
if [ "$RC" -ne 0 ]; then
  pass "--landing HARD-fails a tree whose tracked transcript carries an agent-state path"
else
  fail "--landing MUST refuse a tree carrying slurped session content (the incident reproduced); out: $OUT"
fi
if printf '%s' "$OUT" | grep -q 'HARD.*transcript-hygiene:'; then
  pass "--landing failure output names the transcript-hygiene check"
else
  fail "expected a HARD line naming transcript-hygiene; got: $OUT"
fi

run_preflight "$CLEAN_LINKED" --landing
expect_rc 0 "--landing still exits 0 in a worktree whose transcripts are clean"
if printf '%s' "$OUT" | grep -q 'ok .*transcript-hygiene:'; then
  pass "--landing reports the transcript-hygiene check as ok on a clean tree"
else
  fail "expected an ok line naming transcript-hygiene; got: $OUT"
fi

# ============================================================================
# Arm X — preflight's ANTI-FAIL-OPEN branch for this gate.
# ============================================================================
# Mirrors §7/§8/§9/§10: a non-zero rc with no FAIL line means the checker itself
# could not run. That must still block the land — a gate that could not see its
# subject is exactly the case where landing on its silence is worst.
PREFLIGHT_BLOCK="$(awk '
  /transcript-hygiene gate/ { inblock = 1 }
  inblock { print }
  inblock && /^# --- / && !/transcript-hygiene gate/ { inblock = 0 }
' "$PREFLIGHT")"
if printf '%s' "$PREFLIGHT_BLOCK" | grep -q 'refusing to certify this tree for landing'; then
  pass "preflight's transcript-hygiene block carries the anti-fail-open refusal"
else
  fail "preflight must refuse to certify when check-transcript-hygiene.sh cannot run; block: $PREFLIGHT_BLOCK"
fi
if printf '%s' "$PREFLIGHT_BLOCK" | grep -q 'check-transcript-hygiene.sh'; then
  pass "preflight invokes the SHARED checker (one predicate, two callers)"
else
  fail "preflight must invoke scripts/check-transcript-hygiene.sh"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-transcript-hygiene tests passed\n'
