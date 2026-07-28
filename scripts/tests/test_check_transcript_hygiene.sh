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

# ---------------------------------------------------------------------------
# FIX-1 FIXTURES (TC-86, steward `seq-129`). The pattern originally also demanded
# a `.jsonl` or `subagents/` component BENEATH the project directory. That was
# narrower than the threat: Claude Code writes PERSISTED TOOL OUTPUT under
# <session>/tool-results/ whenever a tool result is too large to inline, and
# those files hold the full untruncated output of whatever the tool did — the
# same content class that leaked, in the same directory, one component over. A
# reviewer running `rg` across ~/.claude (exactly what produced this gate) hits
# them just as readily. The three builders below are the shapes that escaped.
# ---------------------------------------------------------------------------

# tool_results_line — THE REGRESSION. `rg` output naming a persisted tool-result
# file under a session directory. No `.jsonl`, no `subagents/`; the old pattern
# returned rc=0 on this and the gate would have certified a transcript with it.
tool_results_line() {
  printf '/home/coreyt/%s/projects/%s/019fa6d5-3392-77e3-9890-bdc0c24403c1/tool-results/leak.txt:1:PERSISTED TOOL OUTPUT FROM ANOTHER PROJECT\n' \
    '.claude' "${1:--home-coreyt-projects-memex}"
}

# shell_snapshot_line — a second non-`.jsonl` shape. Claude Code snapshots the
# invoking shell under <project>/shell-snapshots/; those carry the user's
# environment and aliases.
shell_snapshot_line() {
  printf '/home/coreyt/%s/projects/%s/shell-snapshots/snapshot-bash-1753670000-9c2f.sh:alias deploy=PRIVATE COMMAND FROM ANOTHER PROJECT\n' \
    '.claude' "${1:--home-coreyt-projects-hermes}"
}

# state_dir_listing_line — a third: `ls -l`/`find` output, macOS spelling, with
# the path at the END of the line rather than the start. Neither `.jsonl` nor
# `subagents/`, and the file it names is auto-memory prose.
state_dir_listing_line() {
  printf -- '-rw-rw-r-- 1 alice staff 12288 Jul 28 10:47 /Users/alice/%s/projects/%s/memory/MEMORY.md\n' \
    '.claude' "${1:--Users-alice-work-secret-client}"
}

# ---------------------------------------------------------------------------
# FIX-2 FIXTURES (TC-86, steward `seq-130`). The HITL ruled the threat model:
# FOREIGN project state is a HARD failure; THIS repo's own project state is an
# advisory WARNING. The builders below are the own-project side of that split.
# The literal is spelled here as a bare directory name (no `/home/` prefix), so
# this file stays clean under its own gate — discriminator 1 doing its job.
# ---------------------------------------------------------------------------
OWN_PROJECT_DIR_LITERAL='-home-coreyt-projects-fathomdb'

# own_slurped_line — the incident shape, but pointing at THIS repo's own project
# state. WARN, not FAIL.
own_slurped_line() {
  printf '/home/coreyt/%s/projects/%s/019fa6d5-3392-77e3-9890-bdc0c24403c1.jsonl:{"type":"user","message":{"role":"user","content":"THIS REPOS OWN CONVERSATION"}}\n' \
    '.claude' "$OWN_PROJECT_DIR_LITERAL"
}

# own_tool_results_line — the ACCEPTED RESIDUAL, named explicitly so it is a
# tested property rather than a claim: this repo's own persisted tool output can
# still be committed, and the WARN is the only thing that surfaces it.
own_tool_results_line() {
  printf '/home/coreyt/%s/projects/%s/019fa6d5-3392-77e3-9890-bdc0c24403c1/tool-results/own.txt:1:THIS REPOS OWN PERSISTED TOOL OUTPUT\n' \
    '.claude' "$OWN_PROJECT_DIR_LITERAL"
}

# own_exec_content_block — the 0.8.14 SHAPE, and the one TC-86 exists to stop.
# A codex `exec` record: the command ECHO names a Claude Code state directory,
# then the transcript carries that command's OUTPUT — the state's CONTENT, not
# merely its path. The echo is path-only (WARN); the output block is content and
# `--redact` must remove it. Terminated by a top-level `codex` marker, with blank
# lines INSIDE the block (the 0.8.14 dump has them, so a blank-line terminator
# would truncate the redaction and leave content behind).
own_exec_content_block() {
  printf 'exec\n'
  printf "/bin/bash -c 'ls /home/coreyt/%s/projects/%s/memory' in /home/coreyt/projects/fathomdb\n" \
    '.claude' "$OWN_PROJECT_DIR_LITERAL"
  printf ' succeeded in 0ms:\n'
  printf 'OWN-MEMORY-CONTENT-ALPHA\n'
  printf '\n'
  printf 'OWN-MEMORY-CONTENT-BETA\n'
  printf '\n'
}

# projects_prefix_only_line — the WIDENING'S BOUNDARY, and the control that says
# the widening did not become "match the word projects". This is the shape of the
# redaction banner already on main in
# dev/plans/runs/codex/agent-seat-hardening/ASH-Phase2-20260728T034657Z.log: the
# quoted string stops AT `projects/` with NO component beneath it. A gate that
# tripped on the record of its own predecessor incident would be self-defeating.
projects_prefix_only_line() {
  printf 'NO PRIOR EXPOSURE: `git grep -l %s^/home/coreyt/%s/projects/%s -- dev/plans/runs/codex/**`\n' \
    "'" '.claude' "'"
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
#
# FIX-2 (steward `seq-130`) makes this arm say something POSITIVE rather than
# resting on `exit 0`. Under the ruled threat model the real tree is not
# hit-free: it carries own-project agent-state paths in docs, prompts and
# measurement snapshots (README/handoff prompts cite this project's own memory
# store by absolute path, deliberately). Those are WARN, not FAIL. So the arm
# asserts the SPLIT — zero foreign, non-zero own-warn — instead of a bare rc=0,
# which would also have been satisfied by a gate that had stopped looking.
run_checker
expect_rc 0 "the real tree passes the transcript-hygiene gate over all tracked files"
expect_out 'transcript-hygiene: no .*agent-state path' "the clean run reports the specific predicate it verified, not a bare 'ok'"
expect_no_out '^FAIL' "the real tree carries ZERO FOREIGN-project agent-state paths (asserted, not inferred from rc)"
expect_out '^WARN  transcript-hygiene: .*[0-9]+ own-project agent-state path' \
  "the real tree's own-project hits are REPORTED as warnings, not silently dropped"
expect_out 'SELF-EXEMPTION' "every run states, in its own output, what it is NOT hard-checking"

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
expect_out '(Steward|STEWARD)' "the failure routes a suspected false positive to the Steward rather than a pattern edit"

# The failure prose is EXPLANATORY TEXT containing backticks and `$`. It first
# shipped in an UNQUOTED heredoc, so bash ran those backticks as command
# substitutions: `ls` output was spliced into the middle of a security message
# and a "rg: command not found" error was emitted alongside it. That is a real
# defect — the operator reading a hygiene failure is reading garbage at exactly
# the moment they must act correctly — and it is invisible to any arm that only
# greps for keywords. These two arms are its recurrence guard.
expect_no_out 'command not found' "the failure prose does not EXECUTE its own backticks (quoted heredoc)"
expect_out 'rg.{1,3}ls' "the failure prose keeps its backticked prose literal"

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
# Arm K — FIX-1 REGRESSION: shapes with NO `.jsonl` and NO `subagents/`.
# ============================================================================
# The pattern as first written required a `.jsonl` or `subagents/` component
# beneath the project directory. That was narrower than the threat and these
# three lines all returned rc=0 against it. `tool-results/` in particular is
# neither hypothetical nor rare — it is where the harness persists tool output
# too large to inline, i.e. the full untruncated result of whatever the tool did,
# sitting in the same session directory as the transcript. Each of these arms is
# a shape a bypass-sandboxed reviewer's `rg` across ~/.claude produces, and each
# would have been CERTIFIED CLEAN. They are separate files so a single arm's
# failure names the shape that regressed.
K_DIR="$TMPROOT/widened"
mkdir -p "$K_DIR"
{ printf '%s\n' "$CLEAN_PROSE"; tool_results_line; } >"$K_DIR/tool-results.log"

run_checker "$K_DIR/tool-results.log"
expect_rc 1 "a persisted tool-result path under a session dir is detected (no .jsonl, no subagents/)"
expect_out 'tool-results\.log' "the tool-results failure names its file"

{ printf '%s\n' "$CLEAN_PROSE"; shell_snapshot_line; } >"$K_DIR/shell-snapshot.log"
run_checker "$K_DIR/shell-snapshot.log"
expect_rc 1 "a shell-snapshot path under an encoded project dir is detected"

{ printf '%s\n' "$CLEAN_PROSE"; state_dir_listing_line; } >"$K_DIR/listing.log"
run_checker "$K_DIR/listing.log"
expect_rc 1 "an ls-style line with the agent-state path at the END (macOS spelling) is detected"

# All three at once: the count must be the true 3, and all three project
# directories must be identifiable for the banner.
{
  printf '%s\n' "$CLEAN_PROSE"
  tool_results_line
  shell_snapshot_line
  state_dir_listing_line
} >"$K_DIR/all-three.log"
run_checker "$K_DIR/all-three.log"
expect_rc 1 "three widened-shape lines fail the gate"
expect_out '3 (line|match)' "the widened shapes are counted individually, not collapsed"

# ============================================================================
# Arm K2 — the widening's FALSE-POSITIVE BOUNDARY.
# ============================================================================
# Widening is exactly when false positives appear, so the two discriminators that
# were KEPT are asserted here rather than argued:
#   * a path that stops AT `projects/` with nothing beneath it stays clean — that
#     is the banner already on main, quoting the incident's own `git grep`;
#   * the leading `-` on the project directory still discriminates, so the
#     synthetic /home/nobody/.../projects/x/y.jsonl fixture in
#     scripts/tests/test_seat_path_guard.sh:98 (and the ten transcripts quoting
#     it) stay clean. Arm F covers the .jsonl spelling; this adds the widened
#     non-.jsonl spelling of the same placeholder, which is the case the widening
#     could newly have broken.
K2_DIR="$TMPROOT/widened-boundary"
mkdir -p "$K2_DIR"
projects_prefix_only_line >"$K2_DIR/prefix-only.log"
run_checker "$K2_DIR/prefix-only.log"
expect_rc 0 "a path that stops AT projects/ (no component beneath) is still NOT flagged"

{
  synthetic_placeholder_line
  printf '    "cwd": "/home/nobody/%s/projects/x/tool-results/z.txt",\n' '.claude'
} >"$K2_DIR/synthetic.log"
run_checker "$K2_DIR/synthetic.log"
expect_rc 0 "the widening did NOT break the leading-'-' discriminator (projects/x/... stays clean)"

relative_claude_refs >"$K2_DIR/relative.log"
run_checker "$K2_DIR/relative.log"
expect_rc 0 "relative .claude/ references stay clean under the WIDENED pattern too"

# The two files the Steward named explicitly: they must be clean under their own
# gate WITHOUT any exemption, and so must the banner already on main. These scan
# the REAL TRACKED FILES, not copies — Arm A covers the whole tree, but a
# whole-tree pass says nothing about which file it was that could have tripped.
for tracked in \
  "$REPO_ROOT/scripts/lib/agent-state-paths.sh" \
  "$REPO_ROOT/scripts/tests/test_check_transcript_hygiene.sh" \
  "$REPO_ROOT/dev/plans/runs/codex/agent-seat-hardening/ASH-Phase2-20260728T034657Z.log"
do
  if [ ! -f "$tracked" ]; then
    fail "expected tracked file $tracked to exist"
    continue
  fi
  run_checker "$tracked"
  expect_rc 0 "$(basename "$tracked") is clean under the WIDENED pattern, with no exemption"
done

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
# Same class as the failure-prose arm: the banner is a heredoc too, and it MUST
# interpolate its two variables while executing nothing.
if grep -q 'command not found' "$H_FILE"; then
  fail "the redaction banner executed something instead of printing it; got: $(cat "$H_FILE")"
else
  pass "the redaction banner prints prose without executing any of it"
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
# Arm K3 — --redact works END TO END on the WIDENED shapes, and still converges.
# ============================================================================
# Detecting a shape the remediation cannot clean would leave an operator with a
# red gate and no way out but weakening the pattern — the one thing the failure
# prose tells them not to do. So the widened shapes are driven all the way
# through --redact: content gone, marker in place, project named in the banner,
# surrounding lines byte-intact, and a second pass byte-identical (the banner and
# the marker must not themselves match the WIDENED pattern, or --redact would
# stack banners forever).
K3_DIR="$TMPROOT/redact-widened"
mkdir -p "$K3_DIR"
K3_FILE="$K3_DIR/tool-results-review.log"
{
  printf 'line-before-untouched\n'
  tool_results_line '-home-coreyt-projects-memex'
  printf 'line-after-untouched\n'
  shell_snapshot_line '-home-coreyt-projects-hermes'
  printf 'verdict: no [P1] findings\n'
} >"$K3_FILE"

run_checker --root "$K3_DIR" --redact
expect_rc 0 "--redact exits 0 after cleaning the widened shapes"

if grep -q 'PERSISTED TOOL OUTPUT FROM ANOTHER PROJECT' "$K3_FILE" \
   || grep -q 'PRIVATE COMMAND FROM ANOTHER PROJECT' "$K3_FILE"; then
  fail "widened-shape content SURVIVED --redact; got: $(cat "$K3_FILE")"
else
  pass "the tool-results and shell-snapshot content is gone from the redacted file"
fi
if grep -q 'REDACTED TC-86' "$K3_FILE"; then
  pass "each widened-shape line is REPLACED IN PLACE by the shared marker"
else
  fail "expected the in-place marker on the widened shapes; got: $(cat "$K3_FILE")"
fi
if grep -q '2 line' "$K3_FILE"; then
  pass "the banner states the removed-line count for the widened shapes"
else
  fail "the banner must count the widened-shape lines; got: $(cat "$K3_FILE")"
fi
if grep -q -- '-home-coreyt-projects-memex' "$K3_FILE" \
   && grep -q -- '-home-coreyt-projects-hermes' "$K3_FILE"; then
  pass "the banner names the foreign projects behind the widened shapes"
else
  fail "the banner must name the widened shapes' projects; got: $(cat "$K3_FILE")"
fi
if grep -qx 'line-before-untouched' "$K3_FILE" \
   && grep -qx 'line-after-untouched' "$K3_FILE" \
   && grep -qx 'verdict: no \[P1\] findings' "$K3_FILE"; then
  pass "surrounding content survives redaction of the widened shapes byte-intact"
else
  fail "--redact disturbed unmatched lines; got: $(cat "$K3_FILE")"
fi

run_checker --root "$K3_DIR"
expect_rc 0 "a re-run of the gate over the redacted widened tree now PASSES"

cp "$K3_FILE" "$TMPROOT/redact-widened-first-pass.snapshot"
run_checker --root "$K3_DIR" --redact
expect_rc 0 "a second --redact over the widened tree exits 0"
if cmp -s "$K3_FILE" "$TMPROOT/redact-widened-first-pass.snapshot"; then
  pass "--redact stays IDEMPOTENT on the widened shapes (banner+marker do not self-match)"
else
  fail "--redact is not idempotent on widened shapes; diff: $(diff "$TMPROOT/redact-widened-first-pass.snapshot" "$K3_FILE" || true)"
fi

# The CAPTURE-TIME filter must widen with the gate — if it did not, the two
# layers have drifted and the gate's silence about a wrapper-captured transcript
# would mean nothing. Asserted here on the wrapper's own stub in Arm M's style,
# but for a tool-results line. (Arms L–R below cover the wrapper generally.)
WIDEN_STUB_DIR="$TMPROOT/widen-stub-bin"
mkdir -p "$WIDEN_STUB_DIR"
{
  printf '#!/usr/bin/env bash\n'
  printf 'echo "OpenAI Codex v0.136.0"\n'
  printf 'printf "/home/coreyt/%%s/projects/%%s/019fa6d5/tool-results/leak.txt:1:LEAKED TOOL OUTPUT\\n" ".claude" "-home-coreyt-projects-memex"\n'
  printf 'echo "verdict: no [P1]"\n'
  printf 'exit 0\n'
} >"$WIDEN_STUB_DIR/codex"
chmod +x "$WIDEN_STUB_DIR/codex"
set +e
OUT="$(PATH="$WIDEN_STUB_DIR:$PATH" bash "$WRAPPER" exec review 2>&1)"
RC=$?
set -e
expect_rc 0 "the wrapper still reports codex's exit code on a tool-results line"
expect_no_out 'LEAKED TOOL OUTPUT' "the CAPTURE-TIME filter widened with the gate (tool-results never reaches the transcript)"
expect_out 'REDACTED TC-86' "the wrapper marks the filtered tool-results line rather than dropping it"

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

# ============================================================================
# Arm Y — FIX-2: the RULED threat model. FOREIGN = HARD, OWN = WARN.
# ============================================================================
# Steward `seq-130`. The fix-1 widening reached `tool-results/`, and that made
# the gate red on the real tree — because this repo's own docs, prompts and
# measurement snapshots legitimately cite paths into THIS project's own Claude
# Code state. The HITL ruled the axis: another project's state is never
# legitimate here (HARD); this project's own is (WARN). Both halves are asserted,
# in both the `.jsonl` and the `tool-results/` shape, because a split asserted
# on only one shape is a split that can silently regress on the other.

# ---- Y1: FOREIGN, .jsonl shape → HARD ------------------------------------
Y_DIR="$TMPROOT/threat-model"
mkdir -p "$Y_DIR"
{ printf '%s\n' "$CLEAN_PROSE"; slurped_line '-home-coreyt-projects-memex'; } >"$Y_DIR/foreign-jsonl.log"
run_checker "$Y_DIR/foreign-jsonl.log"
expect_rc 1 "a FOREIGN project's .jsonl path HARD-fails the gate"
expect_out 'FAIL.*foreign-jsonl\.log' "the foreign .jsonl failure names its file on a FAIL line"

# ---- Y2: FOREIGN, tool-results/ shape → HARD ------------------------------
# The shape the fix-1 widening reached. It stays hard: the ruling closed the
# incident class "completely, INCLUDING the tool-results/ shape".
{ printf '%s\n' "$CLEAN_PROSE"; tool_results_line '-home-coreyt-projects-agent-aware'; } >"$Y_DIR/foreign-tool-results.log"
run_checker "$Y_DIR/foreign-tool-results.log"
expect_rc 1 "a FOREIGN project's tool-results/ path HARD-fails the gate (the widened shape stays hard)"
expect_out 'FAIL.*foreign-tool-results\.log' "the foreign tool-results failure names its file on a FAIL line"

# ---- Y3: OWN, .jsonl shape → WARN, exit 0 ---------------------------------
{ printf '%s\n' "$CLEAN_PROSE"; own_slurped_line; } >"$Y_DIR/own-jsonl.log"
run_checker "$Y_DIR/own-jsonl.log"
expect_rc 0 "an OWN-project .jsonl path is a WARNING, not a failure (exit 0)"
expect_out '^WARN' "the own-project hit is reported on a WARN line"
expect_out 'WARN.*own-jsonl\.log' "the own-project warning NAMES the file"
expect_out 'own-jsonl\.log.*1 line' "the own-project warning states the match COUNT for that file"
expect_no_out '^FAIL' "an own-project hit produces no FAIL line (preflight must not promote it to HARD)"

# ---- Y4: OWN, tool-results/ shape → WARN, exit 0 --------------------------
# This is the ACCEPTED RESIDUAL in test form.
{ printf '%s\n' "$CLEAN_PROSE"; own_tool_results_line; } >"$Y_DIR/own-tool-results.log"
run_checker "$Y_DIR/own-tool-results.log"
expect_rc 0 "an OWN-project tool-results/ path is a WARNING, not a failure (the accepted residual)"
expect_out 'WARN.*own-tool-results\.log' "the own tool-results warning names its file"

# ---- Y5: BOTH in one tree → HARD, and the own warning is STILL reported ----
# The trap this arm exists for: a gate that reports the foreign failure and then
# returns early would hide the own-project warning exactly when the operator is
# already elbow-deep in the tree.
Y5_DIR="$TMPROOT/threat-model-mixed"
mkdir -p "$Y5_DIR"
{ printf '%s\n' "$CLEAN_PROSE"; slurped_line '-home-coreyt-projects-memex'; } >"$Y5_DIR/foreign.log"
{ printf '%s\n' "$CLEAN_PROSE"; own_tool_results_line; } >"$Y5_DIR/own.log"
run_checker --root "$Y5_DIR"
expect_rc 1 "a tree with BOTH foreign and own hits HARD-fails (the foreign half decides the exit code)"
expect_out 'FAIL.*foreign\.log' "the mixed run reports the FOREIGN failure"
expect_out 'WARN.*own\.log' "the mixed run STILL reports the own-project warning alongside the failure"

# ---- Y6: the warning is NOT silently suppressible -------------------------
# A warning that can be turned off is a warning that will be turned off, and the
# own-project class is the ONLY thing that surfaces the accepted residual.
set +e
OUT="$(TC86_QUIET=1 TC86_NO_WARN=1 QUIET=1 NO_COLOR=1 CI=1 bash "$CHECKER" "$Y_DIR/own-tool-results.log" 2>&1)"
RC=$?
set -e
expect_rc 0 "the own-project run still exits 0 with quiet-looking env vars set"
expect_out '^WARN' "no environment variable suppresses the own-project warning"
run_checker --quiet "$Y_DIR/own-tool-results.log"
expect_rc 2 "a suppression FLAG is rejected as an unknown option (exit 2), never silently honoured"

# ---- Y7: the SELF-EXEMPTION is visible in the gate's own output -----------
# The crux of `seq-130`: a self-exemption buried in a regex was explicitly
# REJECTED. A reader must be able to see, from running the gate, exactly what is
# not hard-checked and why — including on a tree with NO hits at all, where there
# is no warning to carry the message.
Y7_DIR="$TMPROOT/threat-model-clean"
mkdir -p "$Y7_DIR"
printf '%s\n' "$CLEAN_PROSE" >"$Y7_DIR/clean.log"
run_checker --root "$Y7_DIR"
expect_rc 0 "a tree with no hits at all passes"
expect_out 'SELF-EXEMPTION' "the exemption is announced even on a fully clean run"
expect_out 'NOT HARD-CHECKED' "the exemption says plainly what is not being hard-checked"
expect_out "$OWN_PROJECT_DIR_LITERAL" "the exemption NAMES the exempted project directory"
expect_out 'ACCEPTED RESIDUAL' "the exemption states the accepted residual"
expect_out 'tool-results' "the residual names the tool-results/ content that can still be committed"

run_checker "$Y_DIR/foreign-jsonl.log"
expect_out 'SELF-EXEMPTION' "the exemption is announced on a FAILING run too"

# ---- Y8: the own-project identity is DERIVED, and its derivation is tested --
# `seq-130`: derived, not hardcoded blindly, but not over-engineered either. One
# named constant, defined as the '/'->'-' encoding of this repo's absolute path,
# with the encoding itself executable so it can be asserted rather than believed.
# shellcheck source=../lib/agent-state-paths.sh
. "$SHARED_LIB"
if [ "${AGENT_STATE_OWN_PROJECT_DIR:-}" = "$OWN_PROJECT_DIR_LITERAL" ]; then
  pass "AGENT_STATE_OWN_PROJECT_DIR is this repo's encoded project directory"
else
  fail "AGENT_STATE_OWN_PROJECT_DIR must be $OWN_PROJECT_DIR_LITERAL, got '${AGENT_STATE_OWN_PROJECT_DIR:-<unset>}'"
fi
if [ "$(agent_state_encode_project_dir /home/coreyt/projects/memex 2>/dev/null || true)" = '-home-coreyt-projects-memex' ]; then
  pass "agent_state_encode_project_dir implements the '/'->'-' encoding Claude Code uses"
else
  fail "agent_state_encode_project_dir must map an absolute path to its encoded project directory"
fi
# Both sides must be NON-EMPTY as well as equal — an unset constant and a missing
# function compare equal as "", which is precisely the vacuous green this repo
# has been bitten by (see `conformance-rewrite-vacuous-green-trap`).
DERIVED_OWN="$(agent_state_encode_project_dir "${AGENT_STATE_OWN_PROJECT_ABS:-}" 2>/dev/null || true)"
if [ -n "$DERIVED_OWN" ] && [ -n "${AGENT_STATE_OWN_PROJECT_DIR:-}" ] \
   && [ "$DERIVED_OWN" = "${AGENT_STATE_OWN_PROJECT_DIR}" ]; then
  pass "the own-project constant is DERIVED from this repo's absolute path, not restated"
else
  fail "AGENT_STATE_OWN_PROJECT_DIR must be agent_state_encode_project_dir(AGENT_STATE_OWN_PROJECT_ABS); got '$DERIVED_OWN' vs '${AGENT_STATE_OWN_PROJECT_DIR:-<unset>}'"
fi

# ---- Y9: --redact does NOT touch own-project PATH lines -------------------
# The own-project exemption covers PATHS. Auto-rewriting them would gut the
# README, the handoff prompts and the measurement snapshots that cite this
# project's own memory store on purpose — and would do it behind a banner, which
# is worse than the warning it replaced.
Y9_DIR="$TMPROOT/redact-own-paths"
mkdir -p "$Y9_DIR"
Y9_FILE="$Y9_DIR/own-paths.md"
{ printf 'prose-before\n'; own_slurped_line; own_tool_results_line; printf 'prose-after\n'; } >"$Y9_FILE"
cp "$Y9_FILE" "$TMPROOT/own-paths.snapshot"
run_checker --root "$Y9_DIR" --redact
expect_rc 0 "--redact over an own-project-only tree exits 0"
if cmp -s "$Y9_FILE" "$TMPROOT/own-paths.snapshot"; then
  pass "--redact leaves own-project PATH lines BYTE-IDENTICAL (no banner, no marker)"
else
  fail "--redact rewrote own-project path lines; diff: $(diff "$TMPROOT/own-paths.snapshot" "$Y9_FILE" || true)"
fi

# ============================================================================
# Arm Z — FIX-2: CONTENT-BLOCK redaction. The 0.8.14 shape.
# ============================================================================
# `seq-130` ruling 2. The own-project exemption covers PATHS, not CONTENT: a
# transcript that echoes `ls ~/.claude/projects/<own>/memory` and then carries
# that command's OUTPUT is publishing this repo's private agent state, which the
# path-level gate never sees (the content lines carry no path prefix at all).
# `--redact` therefore removes the OUTPUT BLOCK of any agent-state-reading
# command while KEEPING the echo — the echo is evidence of what happened, and is
# path-only, hence WARN.
Z_DIR="$TMPROOT/redact-content-block"
mkdir -p "$Z_DIR"
Z_FILE="$Z_DIR/exec-block.log"
{
  printf 'line-before-untouched\n'
  own_exec_content_block
  printf 'codex\n'
  printf 'verdict: no [P1] findings\n'
} >"$Z_FILE"

run_checker --root "$Z_DIR" --redact
expect_rc 0 "--redact exits 0 after cleaning a content block"

if grep -q 'OWN-MEMORY-CONTENT-ALPHA' "$Z_FILE" || grep -q 'OWN-MEMORY-CONTENT-BETA' "$Z_FILE"; then
  fail "the slurped CONTENT survived --redact; got: $(cat "$Z_FILE")"
else
  pass "the command's OUTPUT (the state's CONTENT) is gone from the redacted file"
fi
if grep -q 'REDACTED TC-86' "$Z_FILE"; then
  pass "the removed content block is REPLACED IN PLACE by a marker, not silently dropped"
else
  fail "expected a content-block marker; got: $(cat "$Z_FILE")"
fi
if grep -q "projects/$OWN_PROJECT_DIR_LITERAL/memory" "$Z_FILE"; then
  pass "the command ECHO survives (path-only evidence of what happened)"
else
  fail "--redact must keep the command echo, only its output goes; got: $(cat "$Z_FILE")"
fi
if grep -qx 'line-before-untouched' "$Z_FILE" \
   && grep -qx 'codex' "$Z_FILE" \
   && grep -qx 'verdict: no \[P1\] findings' "$Z_FILE"; then
  pass "the block terminator and the surrounding transcript are intact (the file is not gutted)"
else
  fail "--redact disturbed the surrounding transcript; got: $(cat "$Z_FILE")"
fi
if grep -q 'STEWARD REDACTION' "$Z_FILE" && grep -q -- "$OWN_PROJECT_DIR_LITERAL" "$Z_FILE"; then
  pass "the banner is prepended and names this repo's OWN project directory"
else
  fail "expected a banner naming the own project; got: $(cat "$Z_FILE")"
fi
if grep -q 'OWN PROJECT' "$Z_FILE" && grep -q 'NO FINDING WAS LOST' "$Z_FILE" && grep -q 'PUBLIC' "$Z_FILE"; then
  pass "the banner states it was this repo's own state, the reason, and that no finding was lost"
else
  fail "the banner must state the own-project provenance, the reason and the no-finding-lost claim; got: $(cat "$Z_FILE")"
fi

run_checker --root "$Z_DIR"
expect_rc 0 "the redacted content-block tree passes the gate"
expect_out '^WARN' "the surviving command echo is still surfaced as an own-project warning"

cp "$Z_FILE" "$TMPROOT/content-block-first-pass.snapshot"
run_checker --root "$Z_DIR" --redact
expect_rc 0 "a second --redact over the content-block tree exits 0"
if cmp -s "$Z_FILE" "$TMPROOT/content-block-first-pass.snapshot"; then
  pass "content-block redaction is IDEMPOTENT (the marker and banner do not re-trigger it)"
else
  fail "content-block redaction is not idempotent; diff: $(diff "$TMPROOT/content-block-first-pass.snapshot" "$Z_FILE" || true)"
fi

# ============================================================================
# Arm ZB — FIX-3: a FOREIGN PROJECT INVENTORY (bare directory listing).
# ============================================================================
# `seq-130` ruling 1, applied to a shape the PATH predicate cannot see. The
# 0.8.14 transcript carries the output of `ls ~/.claude/projects`: seventeen bare
# encoded directory names, an inventory of a private project portfolio, in a
# PUBLIC repo. The echo stops AT `projects` with nothing beneath it, so it does
# not match — and the listed lines are bare names, so nothing matches.
#
# THE KNOWN LIMIT IS ASSERTED, NOT PAPERED OVER. The gate deliberately does NOT
# hard-fail on this: a predicate that saw bare directory names would trip on the
# repo's own prose, on the shared library, and on the redaction banners. Widening
# it is not authorised. So this arm pins BOTH halves — the gate stays silent
# (rc=0), and `--redact` removes it anyway when a human points it at the file.
# Remediation reaching further than detection is the design, stated in
# scripts/lib/agent-state-paths.sh.
ZB_DIR="$TMPROOT/foreign-inventory"
mkdir -p "$ZB_DIR"
ZB_FILE="$ZB_DIR/inventory.log"
{
  printf 'line-before-untouched\n'
  printf 'exec\n'
  printf "/bin/bash -c 'ls /home/coreyt/%s/projects' in /home/coreyt/projects/fathomdb\n" '.claude'
  printf ' succeeded in 0ms:\n'
  # INVENTED names, deliberately. The real listing's entries are the very
  # things fix-3 removes from this repo; re-typing them into a tracked fixture
  # would undo the redaction in a different file.
  printf -- '-home-someone-projects-alpha-widget\n'
  printf -- '-home-someone-projects-beta-service\n'
  printf -- '%s\n' "$OWN_PROJECT_DIR_LITERAL"
  printf '\n'
  printf 'codex\n'
  printf 'verdict: no [P1] findings\n'
} >"$ZB_FILE"

# Half 1: SUPERSEDED BY FIX-4, and flipped rather than deleted.
#
# Fix-3 recorded this as a REACH LIMIT: the gate could not see a bare directory
# listing, because no PATH predicate that could see the listed lines would
# survive contact with ordinary prose. That reasoning was about the wrong half of
# the record. The listing's ENTRIES are unmatched prose, but the COMMAND ECHO
# above them is not: `ls <a directory outside this repository>` followed by an
# output block is a mechanical shape with no legitimate use in a committed
# transcript. Fix-4 (HITL 2026-07-28, ruling "A+D") adds that as a SECOND
# predicate, so this class is now HARD, and the arm asserts the closure instead
# of the limit it replaced. The polarity flip is the deliverable.
run_checker "$ZB_FILE"
expect_rc 1 "the gate now HARD-FAILS a bare foreign-project-directory listing (fix-4 closes the fix-3 reach limit)"

# Half 2: --redact removes it anyway.
run_checker --root "$ZB_DIR" --redact
expect_rc 0 "--redact exits 0 after removing a foreign project inventory"
if grep -q 'alpha-widget' "$ZB_FILE" || grep -q 'beta-service' "$ZB_FILE"; then
  fail "foreign project NAMES survived --redact; got: $(cat "$ZB_FILE")"
else
  pass "the foreign project directory names are gone from the redacted file"
fi
if grep -q 'REDACTED TC-86' "$ZB_FILE"; then
  pass "the inventory is REPLACED IN PLACE by a marker, not silently dropped"
else
  fail "expected a marker in place of the inventory; got: $(cat "$ZB_FILE")"
fi
if grep -q "ls /home/coreyt/.claude/projects' in" "$ZB_FILE"; then
  pass "the command ECHO survives the inventory redaction (path-only evidence)"
else
  fail "--redact must keep the echo, only its output goes; got: $(cat "$ZB_FILE")"
fi
if grep -qx 'line-before-untouched' "$ZB_FILE" && grep -qx 'verdict: no \[P1\] findings' "$ZB_FILE"; then
  pass "the surrounding transcript survives the inventory redaction byte-intact"
else
  fail "--redact disturbed the surrounding transcript; got: $(cat "$ZB_FILE")"
fi
# The banner must COUNT the foreign names without NAMING them — enumerating them
# would put back exactly what was removed. Two of the three listed dirs are
# foreign; the own one is not counted.
if grep -q 'FOREIGN PROJECT INVENTORY (2 line' "$ZB_FILE"; then
  pass "the banner counts the foreign directory names (2), excluding this repo's own"
else
  fail "the banner must count the foreign inventory lines; got: $(cat "$ZB_FILE")"
fi
if grep -q 'NOT enumerated' "$ZB_FILE"; then
  pass "the banner says plainly that it does not name them, and why"
else
  fail "the banner must state why the names are withheld; got: $(cat "$ZB_FILE")"
fi

cp "$ZB_FILE" "$TMPROOT/inventory-first-pass.snapshot"
run_checker --root "$ZB_DIR" --redact
expect_rc 0 "a second --redact over the inventory tree exits 0"
if cmp -s "$ZB_FILE" "$TMPROOT/inventory-first-pass.snapshot"; then
  pass "once redacted, the inventory STAYS redacted (second pass byte-identical)"
else
  fail "inventory redaction is not idempotent; diff: $(diff "$TMPROOT/inventory-first-pass.snapshot" "$ZB_FILE" || true)"
fi

# ---- ZB2: the sibling block — SUPERSEDED BY FIX-4, flipped, not deleted ----
# `ls ~/.claude/projects/<own>` lists THIS repo's own session files. Under fix-3
# it was left BYTE-IDENTICAL: the FOREIGN-INVENTORY rule requires a foreign name
# in the body, and there is none.
#
# Fix-4's predicate is a different axis and the HITL ruled it explicitly: an
# OUT-OF-REPO directory inventory is hard, with NO own/foreign split, because
# `~/.claude/projects/<own>` is still a directory outside this repository and its
# listing is still the user's private machine state in a PUBLIC repo. So this
# block is now REMOVED. The fix-3 expectation is inverted here rather than
# dropped, so the supersession is legible instead of silent — and the two halves
# that still hold (the echo survives; the surrounding transcript is intact) are
# asserted alongside it.
ZB2_DIR="$TMPROOT/own-session-listing"
mkdir -p "$ZB2_DIR"
ZB2_FILE="$ZB2_DIR/own-listing.log"
{
  printf 'exec\n'
  printf "/bin/bash -c 'ls /home/coreyt/%s/projects/%s' in /home/coreyt/projects/fathomdb\n" \
    '.claude' "$OWN_PROJECT_DIR_LITERAL"
  printf ' succeeded in 0ms:\n'
  printf '00869a09-1b56-463d-823c-2285c13af9ab.jsonl\n'
  printf 'memory\n'
  printf '\n'
  printf 'codex\n'
} >"$ZB2_FILE"
run_checker --root "$ZB2_DIR"
expect_rc 1 "an own-project session listing is ALSO out-of-repo and now hard-fails (no own/foreign split on this axis)"
run_checker --root "$ZB2_DIR" --redact
expect_rc 0 "--redact over an own-session listing exits 0"
if grep -q '00869a09-1b56-463d-823c-2285c13af9ab' "$ZB2_FILE"; then
  fail "the own-session file names survived --redact; got: $(cat "$ZB2_FILE")"
else
  pass "the own-session file names are removed too (fix-4 has no own/foreign split)"
fi
if grep -q "projects/$OWN_PROJECT_DIR_LITERAL' in" "$ZB2_FILE" && grep -qx 'codex' "$ZB2_FILE"; then
  pass "the own-session listing's ECHO and terminator survive (path-only evidence kept)"
else
  fail "--redact must keep the echo and the surrounding transcript; got: $(cat "$ZB2_FILE")"
fi

# ---- ZB3: `---` front matter is not mistaken for a directory name ---------
# The memory files dumped in the 0.8.14 transcript open and close with a YAML
# `---`, which matches "a line that is a dash followed by non-space" unless the
# second character is excluded. Left unfixed it inflated the banner's inventory
# count by four, and a banner that miscounts what it removed is worse than one
# that says nothing.
if printf -- '---\n' | grep -qE -- "$AGENT_STATE_BARE_PROJECT_DIR_RE"; then
  fail "YAML front-matter '---' must NOT count as an encoded project directory name"
else
  pass "YAML front-matter '---' is not counted as a project directory name"
fi
if printf -- '-home-coreyt-projects-memex\n' | grep -qE -- "$AGENT_STATE_BARE_PROJECT_DIR_RE"; then
  pass "a real encoded project directory name IS recognised as an inventory entry"
else
  fail "the bare-name rule must still match a real encoded project directory"
fi

# ============================================================================
# Arm AA — the REAL 0.8.14 transcript, redacted per `seq-130` ruling 2.
# ============================================================================
# dev/plans/runs/0.8.14-slice-10-fix1-review-20260704T011100Z.log (on main at
# 5e202ea8) is the ONE committed instance of agent-state CONTENT rather than a
# mere path: a codex session `ls`'d and then `sed`'d this repo's memory store and
# the transcript carried the results. It is under dev/plans/runs/, NOT
# dev/plans/runs/codex/**, so the standing prohibition on rewriting transcripts
# does not cover it, and this redaction was explicitly authorised.
#
# TC-RUBRIC-7 closes a review on a PERSISTED artifact, so the two halves are
# asserted together: the slurped content is gone AND the verdict survives.
LOG_0814="$REPO_ROOT/dev/plans/runs/0.8.14-slice-10-fix1-review-20260704T011100Z.log"
if [ ! -f "$LOG_0814" ]; then
  fail "the 0.8.14 review transcript must still exist (redact, never delete)"
else
  pass "the 0.8.14 review transcript still exists after redaction (TC-RUBRIC-7 evidence)"
  for needle in '# Memory index' 'originSessionId' 'node_type: memory' 'code-grounded-audit.md'; do
    if grep -qF -- "$needle" "$LOG_0814"; then
      fail "the 0.8.14 log still carries slurped memory content: '$needle'"
    else
      pass "the 0.8.14 log is clean of the slurped memory content: '$needle'"
    fi
  done
  # FIX-3, ruling 1: the `ls ~/.claude/projects` block listed seventeen encoded
  # directory names — an inventory of a private project portfolio in a PUBLIC
  # repo, with several entries reading as client or domain work. The needles are
  # assembled from fragments so this suite does not re-type the names it is
  # asserting the absence of.
  for frag in 'dicom-de' 'windchill-' 'ado-mc' 'mesh-seam-ripp' 'decimation-prot'; do
    if grep -qF -- "-home-coreyt-projects-${frag}" "$LOG_0814"; then
      fail "the 0.8.14 log still carries a foreign project directory name: '${frag}...'"
    else
      pass "the 0.8.14 log is clean of the foreign project inventory: '${frag}...'"
    fi
  done
  if grep -qF -- "ls /home/coreyt/.claude/projects' in" "$LOG_0814"; then
    pass "the inventory's command ECHO survives (path-only, own-project WARN as ruled)"
  else
    fail "the 0.8.14 inventory redaction must keep the command echo"
  fi
  # SUPERSEDED BY FIX-4 and flipped, for the reason argued at ZB2: fix-3 left the
  # sibling `ls ~/.claude/projects/<own>` listing byte-intact because it carried
  # no FOREIGN name. Fix-4's axis is "is the enumerated directory outside this
  # repository", which that one is, and the HITL ruled the axis has no
  # own/foreign split. The bare `memory` line was the tail of that listing.
  if grep -qxF -- 'memory' "$LOG_0814"; then
    fail "fix-4 must also remove the own-project session listing from the 0.8.14 log"
  else
    pass "the own-project session listing is removed too (fix-4: out-of-repo is out-of-repo)"
  fi
  if grep -qF -- 'FOREIGN PROJECT INVENTORY' "$LOG_0814"; then
    pass "the 0.8.14 banner records that a foreign project inventory was removed"
  else
    fail "the banner must record the foreign inventory removal"
  fi
  if grep -qF -- '## Verdict: CONCERN' "$LOG_0814"; then
    pass "the 0.8.14 review's VERDICT survives the redaction"
  else
    fail "the 0.8.14 redaction destroyed the review verdict"
  fi
  if grep -qF -- 'Step-17 migration comment still claims' "$LOG_0814"; then
    pass "the 0.8.14 review's finding and reasoning survive the redaction"
  else
    fail "the 0.8.14 redaction destroyed the review's reasoning"
  fi
  if grep -qF -- 'STEWARD REDACTION' "$LOG_0814" && grep -qF -- 'REDACTED TC-86' "$LOG_0814"; then
    pass "the 0.8.14 redaction is VISIBLE (banner + in-place markers), not silent"
  else
    fail "the 0.8.14 redaction must be visible via the shared banner and marker"
  fi
  run_checker "$LOG_0814"
  expect_rc 0 "the redacted 0.8.14 log passes the gate"
  expect_out '^WARN' "its surviving command echoes are own-project warnings, as ruled"
fi

# ============================================================================
# Arm ZC — FIX-4: the SECOND PREDICATE. An OUT-OF-REPO DIRECTORY INVENTORY.
# ============================================================================
# HITL 2026-07-28, ruling "A+D". Fix-3 redacted the `ls ~/.claude/projects`
# inventory and recorded the detection gap as an accepted limit. It missed a
# SECOND, EARLIER block in the same transcript — `ls /home/coreyt/projects`,
# whose output was an inventory of the user's ENTIRE private project portfolio
# (33 directory names, several reading as client or domain work). It had been on
# origin/main for roughly three weeks, in four tracked files.
#
# TC-86's agent-state pattern could never have seen it: there is no `.claude/`
# component anywhere in that command or its output. The gap is not the pattern
# being too narrow — it is that ONE predicate was being asked to cover TWO
# different shapes. So fix-4 adds a SECOND predicate beside it, sharing the same
# library:
#
#   PREDICATE 1 (fix-1/2/3, unchanged) : a home-anchored path INTO a Claude Code
#       project state directory. FOREIGN = hard, OWN = warn.
#   PREDICATE 2 (fix-4, here)          : a command ECHO that ENUMERATES a
#       directory NOT under this repository's root, FOLLOWED BY AN OUTPUT BLOCK.
#       HARD, with NO own/foreign split — an out-of-repo directory inventory is
#       never legitimate in a committed transcript, whoever owns the directory.
#
# WHY THE "FOLLOWED BY AN OUTPUT BLOCK" CLAUSE IS LOAD-BEARING. The echo alone is
# path-only and is kept, exactly as fix-2 keeps agent-state echoes: it is the
# evidence of what happened. What must not be published is the LISTING. So an
# echo with an empty output block, or one whose only output is the command's own
# "No such file or directory", is NOT a finding, and ZC9/ZC10 pin that.
#
# WHY THE FIXTURES ARE ASSEMBLED FROM printf. Same reason as every other builder
# in this file: a literal `exec` / echo / status triple in these bytes would make
# THIS FILE a positive under the very predicate it tests. Nothing is excluded
# from the scan to make that work. The output names are INVENTED — re-typing the
# real portfolio names into a tracked fixture would undo, in a second file,
# exactly what Job A removed from the first.
# The cwd every fixture echo records. It is the REAL worktree the exposed blocks
# ran in, and it is spelled out rather than parameterised because the `..`
# resolution arms (ZC5/ZC6) only mean something against a cwd whose relationship
# to the repo root is fixed and known: one `..` lands on the worktrees root
# (inside the working set), two land on the user's projects directory (outside).
ZC_WT='/home/coreyt/projects/fathomdb-worktrees/0.8.14-slice-10-20260704T002826Z'

# zc_exec_block <shell> <command> <cwd> <status> [output-line...]
zc_exec_block() {
  local sh="$1" cmd="$2" cwd="$3" status="$4"
  shift 4
  printf 'exec\n'
  printf '%s -c %s%s%s in %s\n' "$sh" "'" "$cmd" "'" "$cwd"
  printf ' %s in 0ms:\n' "$status"
  local l
  for l in "$@"; do printf '%s\n' "$l"; done
  printf '\n'
}

# The invented stand-in for the real portfolio listing.
ZC_NAMES=(alpha-widget-client beta-domain-svc gamma-proto delta-mesh-tool epsilon-render)

zc_fixture() {  # zc_fixture <dir> <file> <builder-args...>
  local d="$1" f="$2"
  shift 2
  mkdir -p "$d"
  {
    printf 'line-before-untouched\n'
    zc_exec_block "$@"
    printf 'codex\n'
    printf 'verdict: no [P1] findings\n'
  } >"$d/$f"
}

# ---- ZC1: THE REAL SHAPE. `ls /home/coreyt/projects` + its listing ---------
# The block that was actually exposed, reproduced byte-for-byte in structure with
# invented names. This is the arm that says fix-4 would have caught the thing it
# was written for.
ZC1_DIR="$TMPROOT/oor-projects-listing"
zc_fixture "$ZC1_DIR" 'projects.log' '/bin/bash' 'ls /home/coreyt/projects' "$ZC_WT" 'succeeded' "${ZC_NAMES[@]}"
run_checker --root "$ZC1_DIR"
expect_rc 1 "an \`ls\` of the user's whole projects directory HARD-FAILS the gate (the shape that leaked)"
expect_out 'FAIL  transcript-hygiene: .*out-of-repo' "the failure names the out-of-repo directory-inventory predicate, not the agent-state one"
expect_out 'projects\.log' "the failure names the offending FILE"

# ---- ZC2/ZC3/ZC4: the other spellings of "somewhere outside this repo" -----
ZC2_DIR="$TMPROOT/oor-tilde"
zc_fixture "$ZC2_DIR" 'tilde.log' '/bin/bash' 'ls -la ~' "$ZC_WT" 'succeeded' 'Documents' 'Downloads' 'private-notes.md'
run_checker --root "$ZC2_DIR"
expect_rc 1 "a tilde-anchored listing (\`ls -la ~\`) hard-fails"

ZC3_DIR="$TMPROOT/oor-homevar"
zc_fixture "$ZC3_DIR" 'homevar.log' '/bin/sh' 'ls $HOME/projects2' "$ZC_WT" 'succeeded' 'zeta-client-work'
run_checker --root "$ZC3_DIR"
expect_rc 1 "a \$HOME-anchored listing hard-fails"

ZC4_DIR="$TMPROOT/oor-macos"
zc_fixture "$ZC4_DIR" 'macos.log' '/bin/bash' 'find /Users/alice/clients -maxdepth 1' '/Users/alice/projects/fathomdb' 'succeeded' '/Users/alice/clients/eta-account'
run_checker --root "$ZC4_DIR"
expect_rc 1 "the macOS (/Users/) spelling hard-fails too"

# ---- ZC5: a `..` that ESCAPES. The shape the mandate names explicitly ------
# Two of the four exposed files reached the same portfolio listing through
# `ls ../..` from a worktree, with no home-anchored text on the echo at all. A
# predicate that only read absolute targets would have missed half the incident,
# so the target is RESOLVED against the cwd the echo itself records.
ZC5_DIR="$TMPROOT/oor-dotdot"
zc_fixture "$ZC5_DIR" 'dotdot.log' '/bin/bash' 'ls -la ../..' "$ZC_WT" 'succeeded' "${ZC_NAMES[@]}"
run_checker --root "$ZC5_DIR"
expect_rc 1 "a relative \`../..\` that resolves OUTSIDE the repo hard-fails (the second half of the real incident)"

# ---- ZC6/ZC7: FALSE-POSITIVE CONTROLS. The gate must stay usable ----------
# `..` from a linked worktree resolves to the WORKTREES ROOT, which is part of
# this repository's own working set. A gate that failed on `ls ..` from a
# worktree would fire on almost every codex review in the tree and be switched
# off, which is the failure mode this whole file is written against.
ZC6_DIR="$TMPROOT/oor-dotdot-inside"
zc_fixture "$ZC6_DIR" 'inside.log' '/bin/bash' 'ls ..' "$ZC_WT" 'succeeded' '0.8.11.2' '0.8.12-gpu-rerank'
run_checker --root "$ZC6_DIR"
expect_rc 0 "\`ls ..\` from a linked worktree resolves INSIDE the repo's working set and does NOT fail"

ZC7_DIR="$TMPROOT/oor-inrepo"
mkdir -p "$ZC7_DIR"
{
  printf 'line-before-untouched\n'
  zc_exec_block '/bin/bash' 'ls /home/coreyt/projects/fathomdb/dev' "$ZC_WT" 'succeeded' 'design' 'plans'
  zc_exec_block '/bin/bash' 'ls -la /home/coreyt/projects/fathomdb-worktrees/tc86/scripts' "$ZC_WT" 'succeeded' 'preflight.sh'
  zc_exec_block '/bin/bash' 'ls dev/plans/runs' "$ZC_WT" 'succeeded' 'slice-10.log'
  zc_exec_block '/bin/bash' 'tree -L 1 .' "$ZC_WT" 'succeeded' './scripts'
  printf 'codex\n'
} >"$ZC7_DIR/inrepo.log"
run_checker --root "$ZC7_DIR"
expect_rc 0 "absolute IN-REPO targets, worktree targets and ordinary relative targets do NOT fail"

# ---- ZC8: prose that merely MENTIONS such a command ------------------------
# The predicate is anchored to a codex `exec` record — an echo, then a status
# line, then output. Documentation that discusses `ls ~` (this repo's own docs,
# the library header, and the redaction banner all do) must not trip it.
ZC8_DIR="$TMPROOT/oor-prose"
mkdir -p "$ZC8_DIR"
cat >"$ZC8_DIR/prose.md" <<'PROSE'
Reviewers sometimes run `ls ~` or `find /home/someone/projects` while orienting.
Do not: the transcript is committed to a PUBLIC repository. Use `ls ..` instead,
and never `ls -la /Users/alice/clients`.
PROSE
run_checker --root "$ZC8_DIR"
expect_rc 0 "PROSE that merely mentions an out-of-repo listing command does NOT trip the gate"

# ---- ZC9/ZC10: no output block = no finding -------------------------------
ZC9_DIR="$TMPROOT/oor-empty"
zc_fixture "$ZC9_DIR" 'empty.log' '/bin/bash' 'ls /home/coreyt/M*' "$ZC_WT" 'succeeded'
run_checker --root "$ZC9_DIR"
expect_rc 0 "an out-of-repo echo with an EMPTY output block is not a finding (nothing was enumerated)"

ZC10_DIR="$TMPROOT/oor-error"
zc_fixture "$ZC10_DIR" 'error.log' '/bin/bash' 'ls /home/coreyt/.claude/feedback_*' "$ZC_WT" 'exited 2' \
  "ls: cannot access '/home/coreyt/.claude/feedback_*': No such file or directory"
run_checker --root "$ZC10_DIR"
expect_rc 0 "a block whose only output is the command's own error is not a finding"

# ---- ZC11: NO OWN/FOREIGN SPLIT on this axis ------------------------------
ZC11_DIR="$TMPROOT/oor-own-state"
zc_fixture "$ZC11_DIR" 'own-state.log' '/bin/bash' \
  "ls /home/coreyt/.claude/projects/$OWN_PROJECT_DIR_LITERAL" "$ZC_WT" 'succeeded' \
  '00869a09-1b56-463d-823c-2285c13af9ab.jsonl' 'memory'
run_checker --root "$ZC11_DIR"
expect_rc 1 "enumerating THIS repo's own ~/.claude state directory hard-fails too (ruled: no own/foreign split)"

# ---- ZC12: the fix-2 split on PREDICATE 1 is UNCHANGED --------------------
# The new predicate must not have leaked into the old one. An own-project agent
# state PATH, on its own, is still an advisory WARN and still exits 0.
ZC12_DIR="$TMPROOT/oor-predicate1-intact"
mkdir -p "$ZC12_DIR"
{ printf 'prose-before\n'; own_slurped_line; own_tool_results_line; printf 'prose-after\n'; } >"$ZC12_DIR/own.md"
run_checker --root "$ZC12_DIR"
expect_rc 0 "PREDICATE 1's foreign-hard / own-warn split is untouched by fix-4"
expect_out '^WARN' "own-project agent-state paths are still reported as advisory warnings"
{ printf 'prose-before\n'; slurped_line; } >"$ZC12_DIR/foreign.md"
run_checker --root "$ZC12_DIR"
expect_rc 1 "PREDICATE 1 still hard-fails a FOREIGN agent-state path"
expect_out 'FOREIGN agent-state path' "the two predicates report under their own names, not merged into one message"

# ---- ZC13: `rg --files` is an ENUMERATOR; `rg PATTERN` is not -------------
# One of the four exposed files reached the portfolio through
# `rg --files ... . .. ../.. ../../..`, not through `ls`. `rg --files` lists a
# directory tree and is therefore in; `rg PATTERN <dir>` is a content search and
# is deliberately OUT, because sweeping every out-of-repo read into this
# predicate would make it a different (and much broader) control than the one
# ruled. That boundary is stated here as an assertion, not left to inference.
ZC13_DIR="$TMPROOT/oor-rg-files"
zc_fixture "$ZC13_DIR" 'rgfiles.log' '/bin/bash' "rg --files -g 'MEMORY.md' . .. ../.. ../../.." "$ZC_WT" 'exited 2' \
  '../../../projects2/zeta-client-work/MEMORY.md'
run_checker --root "$ZC13_DIR"
expect_rc 1 "\`rg --files\` over an escaping relative target hard-fails (it enumerates a tree)"

ZC13B_DIR="$TMPROOT/oor-rg-search"
zc_fixture "$ZC13B_DIR" 'rgsearch.log' '/bin/bash' 'rg -n "fn is_available" /home/coreyt/.cargo/registry/src' "$ZC_WT" 'succeeded' \
  'execution_providers.rs:41: fn is_available(&self) -> Result<bool>'
run_checker --root "$ZC13B_DIR"
expect_rc 0 "a content-search \`rg PATTERN <dir>\` is NOT an enumeration — the ruled boundary, asserted"

# ---- ZC14: --redact end to end, and it CONVERGES --------------------------
ZC14_DIR="$TMPROOT/oor-redact"
zc_fixture "$ZC14_DIR" 'redact.log' '/bin/bash' 'ls /home/coreyt/projects' "$ZC_WT" 'succeeded' "${ZC_NAMES[@]}"
ZC14_FILE="$ZC14_DIR/redact.log"
run_checker --root "$ZC14_DIR" --redact
expect_rc 0 "--redact clears an out-of-repo inventory and the gate then exits 0"
ZC14_LEFT=0
for n in "${ZC_NAMES[@]}"; do
  if grep -qF -- "$n" "$ZC14_FILE"; then ZC14_LEFT=$((ZC14_LEFT + 1)); fi
done
if [ "$ZC14_LEFT" -eq 0 ]; then
  pass "every enumerated directory name is gone from the redacted transcript"
else
  fail "$ZC14_LEFT enumerated name(s) survived --redact; got: $(cat "$ZC14_FILE")"
fi
if grep -qF -- "ls /home/coreyt/projects' in" "$ZC14_FILE"; then
  pass "the command ECHO survives (path-only; it is the evidence of what happened)"
else
  fail "--redact must keep the out-of-repo command echo; got: $(cat "$ZC14_FILE")"
fi
if grep -q 'REDACTED TC-86' "$ZC14_FILE" && grep -q 'out-of-repo' "$ZC14_FILE"; then
  pass "the removed listing is REPLACED IN PLACE by a marker naming the reason"
else
  fail "expected an out-of-repo marker in place of the listing; got: $(cat "$ZC14_FILE")"
fi
if grep -qx 'line-before-untouched' "$ZC14_FILE" \
   && grep -qx 'codex' "$ZC14_FILE" \
   && grep -qx 'verdict: no \[P1\] findings' "$ZC14_FILE"; then
  pass "the verdict and the surrounding transcript survive the out-of-repo redaction (TC-RUBRIC-7)"
else
  fail "--redact gutted the surrounding transcript; got: $(cat "$ZC14_FILE")"
fi
if grep -qE 'OUT-OF-REPO DIRECTORY (INVENTORY|LISTING).*[0-9]+ line' "$ZC14_FILE"; then
  pass "the banner states the out-of-repo removal WITH A COUNT"
else
  fail "the banner must state the out-of-repo removal and its line count; got: $(cat "$ZC14_FILE")"
fi
if grep -q 'PUBLIC' "$ZC14_FILE" && grep -q 'NO FINDING WAS LOST' "$ZC14_FILE"; then
  pass "the banner states the reason and the no-finding-lost claim"
else
  fail "the banner must state the reason; got: $(cat "$ZC14_FILE")"
fi
cp "$ZC14_FILE" "$TMPROOT/oor-first-pass.snapshot"
run_checker --root "$ZC14_DIR" --redact
expect_rc 0 "a second --redact over the out-of-repo tree exits 0"
if cmp -s "$ZC14_FILE" "$TMPROOT/oor-first-pass.snapshot"; then
  pass "out-of-repo redaction is IDEMPOTENT (marker and banner do not re-trigger it)"
else
  fail "out-of-repo redaction is not idempotent; diff: $(diff "$TMPROOT/oor-first-pass.snapshot" "$ZC14_FILE" || true)"
fi

# ---- ZC15: the BANNER MUST NOT ENUMERATE WHAT IT REMOVED ------------------
# The ZC14 check above already proves the names are absent from the whole file,
# banner included — this arm states it as its own property so a future change
# that "helpfully" lists the removed directories in the banner is caught by an
# assertion whose name says why it exists.
if grep -qE 'alpha-widget|beta-domain|gamma-proto|delta-mesh|epsilon-render' "$ZC14_FILE"; then
  fail "the banner enumerated the very names the redaction removed"
else
  pass "the banner reports a COUNT and a REASON, never the names it removed"
fi

# ---- ZC16: DETECTION AND REMEDIATION AGREE (the anti-drift assertion) -----
# The gate's silence must mean the redactor found nothing, and the redactor's
# no-op must mean the gate would pass. If those two ever drift the result is a
# FALSE GREEN BUILT INTO THE DESIGN — the same argument that gives the path
# pattern exactly one home. Asserted as a round trip on the same bytes.
ZC16_DIR="$TMPROOT/oor-agreement"
zc_fixture "$ZC16_DIR" 'agree.log' '/bin/bash' 'ls /home/coreyt/projects' "$ZC_WT" 'succeeded' "${ZC_NAMES[@]}"
run_checker --root "$ZC16_DIR"
ZC16_BEFORE="$RC"
run_checker --root "$ZC16_DIR" --redact
run_checker --root "$ZC16_DIR"
ZC16_AFTER="$RC"
if [ "$ZC16_BEFORE" -eq 1 ] && [ "$ZC16_AFTER" -eq 0 ]; then
  pass "detection and remediation agree: the gate fails, --redact fixes it, the gate then passes"
else
  fail "detect/remediate drift — before rc=$ZC16_BEFORE (want 1), after rc=$ZC16_AFTER (want 0)"
fi

# ---- ZC17: PROOF ON THE REAL BYTES, not on a hand-built lookalike ---------
# A fixture proves the predicate fires on a shape someone typed into this file.
# It does not prove it would have fired on the thing that actually leaked. So
# this arm takes the REAL, now-redacted 0.8.14 transcript and puts a listing back
# where fix-4 removed one — every byte of the `exec` record, the echo, the cwd
# and the status line is the original's. Only the enumerated names are invented.
# If the gate passes that file, fix-4 does not close the incident.
ZC17_SRC="$REPO_ROOT/dev/plans/runs/0.8.14-slice-10-fix1-review-20260704T011100Z.log"
if [ ! -f "$ZC17_SRC" ]; then
  fail "the real 0.8.14 transcript must exist for the fix-4 proof arm"
else
  ZC17_DIR="$TMPROOT/oor-real-bytes"
  mkdir -p "$ZC17_DIR"
  sed 's/^\[REDACTED TC-86\].*out-of-repo.*$/REINSTATED-DIRECTORY-ENTRY-A/' "$ZC17_SRC" \
    >"$ZC17_DIR/reinstated.log"
  # ANTI-VACUOUS: if the substitution matched nothing the arm below would pass
  # for the wrong reason — a clean file that was never dirtied.
  if cmp -s "$ZC17_SRC" "$ZC17_DIR/reinstated.log"; then
    fail "the fix-4 proof arm substituted nothing — the real log carries no out-of-repo marker to reinstate"
  else
    pass "the real 0.8.14 log carries an out-of-repo marker, so the proof arm has something to reinstate"
    run_checker "$ZC17_DIR/reinstated.log"
    expect_rc 1 "PRE-REMEDIATION PROOF: the real 0.8.14 exec record, with its listing put back, HARD-FAILS the gate"
  fi
fi

# ---- ZC18: the real tree is CLEAN under the new predicate -----------------
# The regression half. Arm A asserts the tree passes; this asserts it passes for
# the fix-4 reason as well, by naming the predicate in the clean report.
run_checker
expect_rc 0 "the real tree passes with BOTH predicates active"
expect_no_out '^FAIL' "no tracked file carries an out-of-repo directory inventory"
expect_out 'out-of-repo' "the clean run names the second predicate it verified, so its silence is legible"

# ---- ZC19: ONE definition of the fix-4 constants, and the gate sources it --
# Arm S/T's anti-drift argument applied to the new predicate: the roots it
# measures "outside" against must have exactly one home, or the gate and the
# redactor can disagree about what "outside" means.
for sym in AGENT_STATE_OWN_HOME_ABS AGENT_STATE_OWN_WORKTREES_ABS AGENT_STATE_EXEC_STATUS_RE \
           AGENT_STATE_ENUM_COMMANDS AGENT_STATE_OOR_MARKER_FMT; do
  ZC19_FILES="$(git -C "$REPO_ROOT" grep -l -E "^${sym}=" || true)"
  ZC19_N="$(printf '%s' "$ZC19_FILES" | grep -c . || true)"
  if [ "$ZC19_N" -eq 1 ] && [ "$ZC19_FILES" = 'scripts/lib/agent-state-paths.sh' ]; then
    pass "$sym is assigned exactly once, in scripts/lib/agent-state-paths.sh"
  else
    fail "$sym must be assigned exactly once in the shared library; found $ZC19_N: $ZC19_FILES"
  fi
done
if [ "${AGENT_STATE_OWN_WORKTREES_ABS:-}" = "${AGENT_STATE_OWN_PROJECT_ABS:-x}-worktrees" ]; then
  pass "the worktrees root is DERIVED from the repo root, not restated"
else
  fail "AGENT_STATE_OWN_WORKTREES_ABS must be \${AGENT_STATE_OWN_PROJECT_ABS}-worktrees; got '${AGENT_STATE_OWN_WORKTREES_ABS:-<unset>}'"
fi
case "${AGENT_STATE_OWN_PROJECT_ABS:-}" in
  "${AGENT_STATE_OWN_HOME_ABS:-/nonexistent}"/*)
    pass "the repo root is under the stated home directory, so ~ expansion is coherent" ;;
  *)
    fail "AGENT_STATE_OWN_PROJECT_ABS must live under AGENT_STATE_OWN_HOME_ABS ('${AGENT_STATE_OWN_HOME_ABS:-<unset>}')" ;;
esac

# ---- ZC20: the ACCEPTANCE. The exposed names are gone from the WHOLE TREE --
# The fix-3 report claimed the exposed names "all now count 0 in the file" after
# checking only the block it had edited; four of them were still present
# elsewhere in the same tree. So the acceptance is encoded here as a TREE-WIDE
# assertion that cannot be satisfied by looking at one block.
#
# EACH NEEDLE IS SPLIT ACROSS A `|` AND REJOINED AT RUNTIME. Not decoration: a
# literal needle in these bytes would make THIS FILE the file that carries the
# name, and the assertion would fail on itself — or, worse, be "fixed" by
# excluding this file from its own check.
for ZC20_PAIR in '7thplacenw|-client' 'dicom|-de-id' 'windchill|-llm' 'wopr|-markii' \
                 'mesh|-seam-ripper' 'schoo|ner' 'ado|-mcp' 'md|-render' \
                 'decimation|-proto' 'arm|ada'; do
  ZC20_NEEDLE="${ZC20_PAIR%%|*}${ZC20_PAIR##*|}"
  ZC20_FILES="$(git -C "$REPO_ROOT" grep -l -F -- "$ZC20_NEEDLE" || true)"
  ZC20_N="$(printf '%s' "$ZC20_FILES" | grep -c . || true)"
  if [ "$ZC20_N" -eq 0 ]; then
    pass "no tracked file anywhere in the tree carries the exposed name '${ZC20_PAIR}'"
  else
    fail "'${ZC20_PAIR}' still appears in $ZC20_N tracked file(s): $(printf '%s' "$ZC20_FILES" | tr '\n' ' ')"
  fi
done

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll check-transcript-hygiene tests passed\n'
