#!/usr/bin/env bash
# scripts/lib/agent-state-paths.sh — THE single definition of the "agent state
# path" pattern, plus the redaction vocabulary built on it.
#
# TC-86 (steward `seq-129`, todos `TC-86`, master `F-36`). NOT a slice; no
# `R-20-xx` id; no pico label. Cross-cutting transcript hygiene.
#
# ============================ WHY THIS FILE EXISTS ===========================
# On 2026-07-28 a codex §9 review transcript arrived carrying 216 lines of raw
# Claude Code session JSONL. codex, running under
# --dangerously-bypass-approvals-and-sandbox (which is what lets it read outside
# the repo at all), had run `rg` across the user's ~/.claude state directory and
# slurped the results into its own stdout, which TC-RUBRIC-7 then required be
# `tee`d to a file under dev/plans/runs/codex/ — a TRACKED path in a repository
# that is PUBLIC. Those lines carried conversation content from three projects
# other than this one. It was caught and redacted before landing, and
# `git grep` proved zero already-committed files carried that shape, so
# reachability in history is ZERO and there is nothing to scrub.
#
# The mechanism is STRUCTURAL, not a one-off: every §9 review runs with the
# bypass flag, TC-RUBRIC-7 *requires* the transcript be persisted under a tracked
# path, and nothing in between inspects the contents — while the person doing the
# commit is reading a verdict at the end of a multi-megabyte file. So it is fixed
# in the tooling, per `guardrail-failures-fix-tooling-not-people`.
#
# ====================== WHY ONE DEFINITION, NOT A COPY =======================
# Two consumers act on this pattern:
#   * dev/agent-tools/codex-nostdin.sh   (CAPTURE-TIME filter — the slurped line
#                                         never reaches the transcript file)
#   * scripts/check-transcript-hygiene.sh (LANDING GATE — preflight --landing +
#                                         an always-on CI job)
# If those two ever drift, the failure is silent and it is a FALSE GREEN BUILT
# INTO THE DESIGN: the gate certifies files the filter would have cleaned, or the
# filter cleans a shape the gate does not recognise and the gate's silence means
# nothing. So the regex is defined HERE, exactly once, and both consumers source
# this file. scripts/tests/test_check_transcript_hygiene.sh asserts mechanically
# that there is exactly one assignment of AGENT_STATE_PATH_RE in the whole tree
# and that both consumers reference this file — so the drift cannot be
# reintroduced silently.
#
# ========================= WHAT THE PATTERN MATCHES ==========================
# HOME-ANCHORED ABSOLUTE paths to ANYTHING beneath a user's encoded Claude Code
# PROJECT directory:
#
#     /home/<user>/.claude/projects/<PROJ>/<anything>
#     /Users/<user>/.claude/projects/<PROJ>/<anything>
#
# where <PROJ> is the encoded-cwd project directory, which always begins with a
# `-` (see discriminator 2). The placeholders above are written WITHOUT that
# leading `-` on purpose, so this header does not trip the gate it documents.
#
# WIDENED IN FIX-1 (TC-86, steward `seq-129`). The pattern originally ALSO
# demanded a `.jsonl` or `subagents/` component beneath <PROJ>. That was
# NARROWER THAN THE THREAT and it was found by the implementer's own disclosure,
# not by a leak: Claude Code writes PERSISTED TOOL OUTPUT under
# <PROJ>/<session>/tool-results/, whenever a tool result is too large to inline,
# and those files hold the FULL UNTRUNCATED OUTPUT of whatever the tool did. That
# is the same content class that leaked in the incident below, in the same
# directory, ONE COMPONENT OVER — and a reviewer running `rg` across ~/.claude
# (exactly what produced this gate) hits them just as readily as the .jsonl
# files. The old pattern returned rc=0 on such a line, so the gate would have
# CERTIFIED a transcript that leaked them. Session state is not only transcripts;
# the shape that matters is "a path INTO someone's project state", so that is now
# what is matched. The widening is STRICT: everything matched before still
# matches (a `.jsonl`/`subagents/` component is itself a component beneath
# <PROJ>), so no prior positive was traded away for it.
#
# Two deliberate discriminators, each load-bearing and each KEPT through the
# widening — the pattern is anchored, not loosened into "the word projects":
#
#  1. `/home/` or `/Users/` PREFIX REQUIRED. A RELATIVE `.claude/...` reference
#     — `.claude/agents/steward.md`, `.claude/hooks/seat-path-guard.sh`,
#     `.claude/settings.json` — is repo configuration this project's docs discuss
#     constantly (several such paths were added to dev/design/orchestration.md and
#     the STATUS board this week). A gate that trips on those is unusable and
#     would be turned off, so it must not match them, and it does not.
#
#  2. THE PROJECT DIRECTORY MUST BEGIN WITH `-`. Claude Code names a project
#     state directory after the absolute cwd with `/` replaced by `-`
#     (/home/coreyt/projects/memex -> -home-coreyt-projects-memex), and an
#     absolute path always starts with `/`, so a REAL project directory ALWAYS
#     begins with `-`. This is not cosmetic tightening: without it the pattern
#     fires on scripts/tests/test_seat_path_guard.sh, whose PreToolUse fixture
#     carries the SYNTHETIC payload path /home/nobody/.claude/projects/x/y.jsonl
#     (and on the ten codex transcripts that quote that fixture back). Those are
#     invented placeholders, not anyone's session state. `x` has no leading `-`;
#     a real leak always does. See the "GREEN ON THE TREE" note below.
#
# AT LEAST ONE COMPONENT BENEATH <PROJ> is still required — `[^[:space:]]` at the
# tail, one non-space byte after the project directory's `/`. This is not a third
# discriminator on the content's SHAPE (that is what fix-1 removed); it is the
# boundary that keeps the pattern off PROSE that merely *names* the directory,
# including the redaction banner this very file emits and the one already on main
# in dev/plans/runs/codex/agent-seat-hardening/ASH-Phase2-20260728T034657Z.log,
# which quotes `git grep -l '^/home/coreyt/.claude/projects/'`. That string stops
# AT `projects/` with nothing beneath it, so it does not match — a gate that
# tripped on the record of its own predecessor incident would be self-defeating.
# A path with a real component beneath <PROJ> names a real file in someone's
# project state, whatever its extension.
#
# GREEN ON THE TREE: with both discriminators in place this pattern has ZERO
# matches across ALL TRACKED FILES — verified at baseline 1cbde587 before the
# widening and RE-VERIFIED across all tracked files after it, since widening is
# exactly when false positives appear. Scope therefore did NOT have to be
# narrowed to dev/plans/runs/**; the gate scans everything tracked, which is what
# catches a transcript written somewhere unexpected.
#
# ============================== WHAT IT IS NOT ===============================
# This is a HYGIENE pattern for an ACCIDENT, not a secrets scanner and not an
# adversarial control. It recognises the one mechanical shape by which a
# bypass-sandboxed reviewer's tool output carries another project's session
# transcript into a public repo. Content slurped WITHOUT its home-anchored path
# prefix (say `cat`-ing a single jsonl) is out of its reach, by construction and
# by the scope TC-86 was authorised with.
#
# Sourceable from any cwd; defines variables + functions only, runs nothing.

# The ONE definition. Extended regular expression (grep -E / sed -E). Consumers
# MUST source this file rather than restate it — see "WHY ONE DEFINITION" above.
#
# Contains no `@`, so `@` is safe as a sed delimiter (the path shape is dense in
# `/`). Note the regex text itself does NOT match the regex: `(home|Users)` and
# `[^/[:space:]]+` are not literal path bytes, which is why this file is clean
# under its own gate without needing an exemption.
AGENT_STATE_PATH_RE='/(home|Users)/[^/[:space:]]+/\.claude/projects/-[^/[:space:]]*/[^[:space:]]'

# The in-place replacement for one matched line. REDACT, NEVER DELETE:
# TC-RUBRIC-7 closes a review on a persisted terminal artifact, so the transcript
# has to survive as evidence, with the removal visible rather than silent. Plain
# ASCII, and free of `&` and `\`, so it is safe verbatim as a sed replacement.
AGENT_STATE_REDACTION_MARKER='[REDACTED TC-86] agent-state path removed (foreign Claude Code session transcript content)'

# agent_state_redaction_banner <removed-line-count> <projects-csv>
#
# The block prepended to a redacted file. Shape copied from the banner already on
# main in dev/plans/runs/codex/agent-seat-hardening/ASH-Phase2-20260728T034657Z.log:
# it states the removed-line count, the foreign projects, the reason, and that no
# review finding came from the removed lines.
#
# The banner must never match AGENT_STATE_PATH_RE — otherwise redacting a file
# would leave it dirty and `--redact` could never converge. It names project
# DIRECTORIES only, with NO `/home/`|`/Users/` prefix, which is discriminator 1
# doing its job. (Under the fix-1 widening the tail no longer requires a
# `.jsonl`/`subagents/` shape, so the prefix — not the extension — is what keeps
# the banner clean. Asserted, not assumed: the idempotence arms in
# scripts/tests/test_check_transcript_hygiene.sh prove a second `--redact` pass
# is byte-identical, for the widened shapes too.)
agent_state_redaction_banner() {
  local count="${1:?agent_state_redaction_banner needs a line count}"
  local projects="${2:-(none identified)}"
  cat <<BANNER
=== STEWARD REDACTION (TC-86 transcript hygiene) ===
${count} line(s) redacted in place in this transcript before it was committed.
WHAT THEY WERE: home-anchored absolute paths into a user's Claude Code state
directory (~/.claude/projects/...) — i.e. raw agent SESSION TRANSCRIPT content
slurped into this file by a tool running under
--dangerously-bypass-approvals-and-sandbox.
PROJECTS TOUCHED: ${projects}
WHY REMOVED: github.com/coreyt/fathomdb is a PUBLIC repository. Committing
another project's session transcripts into it would publish them.
NO FINDING WAS LOST: no review finding came from the removed lines. Every verdict
and all reasoning above and below are intact.
NOT DELETED: each removed line was REPLACED IN PLACE by a marker, so this
transcript still stands as TC-RUBRIC-7 evidence.
=== END STEWARD REDACTION ===
BANNER
}

# agent_state_redact_stream — filter stdin to stdout, rewriting every line that
# carries an agent-state path into AGENT_STATE_REDACTION_MARKER. Whole-line
# replacement, not a snip: a matched line is raw session JSONL, so the remainder
# of it is exactly the content that must not be published.
#
# Non-matching lines pass through byte-intact. sed block-buffers when stdout is
# not a terminal, which is precisely the tee'd-to-a-file case this exists for; no
# output is lost, it merely arrives in chunks.
agent_state_redact_stream() {
  sed -E "s@^.*${AGENT_STATE_PATH_RE}.*\$@${AGENT_STATE_REDACTION_MARKER}@"
}

# agent_state_project_dirs — read stdin, print the DISTINCT Claude project
# directory names (`-home-coreyt-projects-memex`, ...) appearing in agent-state
# paths, one per line. Feeds the banner's PROJECTS TOUCHED field: naming the
# projects is the point of the banner, and a directory name is not conversation
# content.
agent_state_project_dirs() {
  grep -oE "$AGENT_STATE_PATH_RE" \
    | sed -E 's@^.*/\.claude/projects/(-[^/[:space:]]*)/.*$@\1@' \
    | sort -u
}
