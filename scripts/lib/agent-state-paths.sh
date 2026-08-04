#!/usr/bin/env bash
# scripts/lib/agent-state-paths.sh — THE single definition of the "agent state
# path" pattern, of the FOREIGN/OWN threat axis built on it, and of the redaction
# vocabulary both consumers share.
#
# TC-86 (steward `seq-129` raised it, `seq-130` ruled the threat model; todos
# `TC-86`, master `F-36`). NOT a slice; no `R-20-xx` id; no pico label.
# Cross-cutting transcript hygiene.
#
# ============================ WHY THIS FILE EXISTS ===========================
# On 2026-07-28 a codex §9 review transcript arrived carrying 216 lines of raw
# Claude Code session JSONL. codex, running under
# --dangerously-bypass-approvals-and-sandbox (which is what lets it read outside
# the repo at all), had run `rg` across the user's ~/.claude state directory and
# slurped the results into its own stdout, which TC-RUBRIC-7 then required be
# `tee`d to a file under dev/plans/runs/codex/ — a TRACKED path in a repository
# that is PUBLIC. Those lines carried conversation content from three projects
# other than this one. It was caught and redacted before landing.
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
# is the same content class that leaked in the incident above, in the same
# directory, ONE COMPONENT OVER — and a reviewer running `rg` across ~/.claude
# (exactly what produced this gate) hits them just as readily as the .jsonl
# files. The old pattern returned rc=0 on such a line, so the gate would have
# CERTIFIED a transcript that leaked them. Session state is not only transcripts;
# the shape that matters is "a path INTO someone's project state", so that is now
# what is matched. The widening is STRICT: everything matched before still
# matches (a `.jsonl`/`subagents/` component is itself a component beneath
# <PROJ>), so no prior positive was traded away for it. `seq-130` KEPT it.
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
#     a real leak always does.
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
# ============================== WHAT IT IS NOT ===============================
# This is a HYGIENE pattern for an ACCIDENT, not a secrets scanner and not an
# adversarial control. It recognises the one mechanical shape by which a
# bypass-sandboxed reviewer's tool output carries session state into a public
# repo. Content slurped WITHOUT its home-anchored path prefix (say `cat`-ing a
# single jsonl) is out of the PATTERN's reach — which is exactly why fix-2 adds a
# separate CONTENT-BLOCK remediation below; see "REDACTING CONTENT, NOT PATHS".
#
# Sourceable from any cwd; defines variables + functions only, runs nothing.

# The ONE definition. Extended regular expression (grep -E / sed -E / awk).
# Consumers MUST source this file rather than restate it — see "WHY ONE
# DEFINITION" above.
#
# Contains no `@`, so `@` is safe as a sed delimiter (the path shape is dense in
# `/`). Note the regex text itself does NOT match the regex: `(home|Users)` and
# `[^/[:space:]]+` are not literal path bytes, which is why this file is clean
# under its own gate without needing an exemption.
AGENT_STATE_PATH_RE='/(home|Users)/[^/[:space:]]+/\.claude/projects/-[^/[:space:]]*/[^[:space:]]'

# ============================================================================
# THE THREAT AXIS: FOREIGN is HARD, OWN is a WARNING (steward `seq-130`).
# ============================================================================
# Fix-1's widening was correct and it stays. What it exposed is that the pattern
# alone is not the threat model. The widened pattern has SIXTEEN matches across
# this repository's tracked files, and every one of them is a path into THIS
# project's own Claude Code state directory, cited deliberately:
#
#   dev/experiments/code-markers-eval/README.md + mine_natural_refs.py
#       an experiment that mines this project's own memory store, and must say
#       which directory it reads;
#   dev/plans/prompts/00-handoff-execute.md, 0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md
#       hand-off prompts that TELL the next agent where the memory index is;
#   scripts/repo-prune/measurements/memory-clarity/{baseline,post}.{json,md}
#       before/after snapshots of a prune of that same store;
#   dev/notes/performance-whitepaper-notes.md
#       a note citing one persisted tool-result file by path;
#   dev/plans/0.6.0-Phase-9-Pack-5-performance-diagnostics.md
#   dev/plans/runs/0.8.14-slice-10-fix1-review-20260704T011100Z.log
#
# So the question `seq-130` answered is: foreign project state, or ANY Claude
# Code project state?
#
#   FOREIGN  -> HARD FAIL. Another project's session state is NEVER legitimately
#               in this repository. That is the incident, and after fix-1 the
#               class is closed completely, INCLUDING the `tool-results/` shape.
#   OWN      -> WARN, exit 0. Hard-failing the paths above would make the gate
#               unusable, and an unusable gate gets switched off — which is a
#               worse outcome than the residual it would have prevented.
#
# THE EXEMPTION IS DELIBERATELY NOT HIDDEN. `seq-130` explicitly REJECTED a
# silent self-exemption buried in the regex (e.g. an ERE that quietly refuses to
# match this one directory). The reason is that such an exemption is invisible at
# the only moment it matters: when someone reads the gate's green output and
# concludes the tree is clean. So the exemption lives HERE as a named constant,
# it is argued in prose (this block), and — the part that makes it honest —
# scripts/check-transcript-hygiene.sh PRINTS agent_state_self_exemption_notice on
# EVERY run, clean or failing. A reader can see, from running the gate, exactly
# what is not being hard-checked and why.
#
# THE ACCEPTED RESIDUAL, stated plainly and knowingly traded: this repository's
# OWN session state — including <session>/tool-results/ content, which is the
# full untruncated output of whatever a tool did — CAN STILL BE COMMITTED. The
# WARN lines are the only thing that surfaces it. Nothing suppresses them, and
# `--redact` deliberately does NOT rewrite own-project path lines (that would gut
# the README, the hand-off prompts and the measurement snapshots above, and do it
# behind a banner, which is worse than the warning it replaced).

# agent_state_encode_project_dir <absolute-path> — the '/' -> '-' encoding Claude
# Code uses to name a project state directory after the cwd it was launched in.
# Executable rather than prose so the constant below can be DERIVED and the
# derivation asserted in the fixture suite instead of believed.
agent_state_encode_project_dir() {
  printf '%s' "${1:?agent_state_encode_project_dir needs an absolute path}" | tr '/' '-'
}

# This repository's absolute path. It is a constant on purpose and it is NOT
# derived from `git rev-parse --show-toplevel`: landing happens from a LINKED
# WORKTREE (TC-RUBRIC-5), whose toplevel is .../fathomdb-worktrees/<slice>, so a
# runtime derivation would silently exempt the WRONG directory — and would exempt
# a different one in every worktree, which is the opposite of a stated exemption.
# The `--root` fixture mode has no repo at all. One named constant, one reason.
AGENT_STATE_OWN_PROJECT_ABS='/home/coreyt/projects/fathomdb'

# The linked-worktree root. DERIVED, because `git worktree add` puts every linked
# worktree under `<repo>-worktrees/<name>` by this project's convention
# (TC-RUBRIC-5), and a codex review almost always runs with its cwd there rather
# than in the primary checkout. Predicate 2 below treats both as INSIDE.
AGENT_STATE_OWN_WORKTREES_ABS="${AGENT_STATE_OWN_PROJECT_ABS}-worktrees"

# The home directory the repository lives under. Needed only to expand `~` and
# `$HOME` in a command echo into something comparable with the two roots above.
# Stated rather than derived by string surgery on the repo path, and the fixture
# suite asserts the two are consistent (AGENT_STATE_OWN_PROJECT_ABS must live
# under it) so the pair cannot drift into incoherence unnoticed.
AGENT_STATE_OWN_HOME_ABS='/home/coreyt'

# The encoded project directory that is NOT hard-checked. Derived, so the
# encoding rule is exercised rather than transcribed.
AGENT_STATE_OWN_PROJECT_DIR="$(agent_state_encode_project_dir "$AGENT_STATE_OWN_PROJECT_ABS")"

# Shared awk prologue: ONE implementation of "is this line foreign?", used by the
# capture-time filter and by the gate's classifier, for the same
# no-second-definition reason the regex itself has one home.
#
# The pattern and the own-directory name arrive through ENVIRON, never through
# `awk -v`: -v processes escape sequences, which would turn `\.claude` into
# `.claude` (any char, not a literal dot) and quietly loosen the pattern.
AGENT_STATE_AWK_PROLOGUE='
function tc86_is_foreign(line,   m, found) {
  found = 0
  while (match(line, RE)) {
    m = substr(line, RSTART, RLENGTH)
    sub(/^.*\/\.claude\/projects\//, "", m)
    sub(/\/.*$/, "", m)
    if (m != OWN) found = 1
    line = substr(line, RSTART + RLENGTH)
  }
  return found
}

# ---- PREDICATE 2 (fix-4): is this echo enumerating a directory OUTSIDE? -----
# See "FIX-4" below for the argument. These four functions are the whole of it,
# and they live in the shared prologue for the same reason the pattern does: the
# GATE and the REDACTOR must agree, byte for byte, on what "outside" means.

# tc86_norm — lexical path normalisation. Deliberately NOT realpath: the paths
# here come out of a transcript recorded on some other day, in a worktree that
# no longer exists, so nothing is resolvable and a symlink-following answer
# would be wrong as often as right. Lexical is also the conservative direction:
# it cannot be tricked into calling an outside path inside.
function tc86_norm(p,   n, i, out, parts, k, r) {
  gsub(/\/+/, "/", p)
  n = split(p, parts, "/")
  k = 0
  for (i = 1; i <= n; i++) {
    if (parts[i] == "" || parts[i] == ".") continue
    if (parts[i] == "..") { if (k > 0) k-- ; continue }
    k++
    out[k] = parts[i]
  }
  r = ""
  for (i = 1; i <= k; i++) r = r "/" out[i]
  if (r == "") r = "/"
  return r
}

# tc86_under — component-boundary containment. `index(p, root)` alone would call
# /home/coreyt/projects/fathomdb-notes "inside" /home/coreyt/projects/fathomdb.
function tc86_under(p, root) {
  if (root == "") return 0
  if (p == root) return 1
  if (index(p, root "/") == 1) return 1
  return 0
}

# tc86_target_is_oor — is this argument a directory outside this repository?
#
# THE JUDGEMENT IS DELIBERATELY NARROW, and the narrowness is the reason the gate
# is usable. Only a HOME-ANCHORED resolved path counts as outside: /home/<user>/
# or /Users/<user>/. A listing of /tmp, /etc or /usr is not this threat — it is
# not the operator private data this gate exists for, and /tmp in particular is
# where every fixture in the suite lives. Everything else that is not under one
# of the two repo roots is simply not judged.
function tc86_target_is_oor(t, cwd,   abs) {
  gsub(QRE, "", t)
  if (t == "") return 0
  # A command substitution or a variable this gate cannot resolve is not
  # guessed at. Silence here is a reach limit, stated, not a clearance.
  if (substr(t, 1, 2) == "$(") return 0
  if (substr(t, 1, 1) == "`") return 0
  if (t == "~") t = HOMEABS
  else if (substr(t, 1, 2) == "~/") t = HOMEABS substr(t, 2)
  else if (t == "$HOME" || t == "${HOME}") t = HOMEABS
  else if (substr(t, 1, 6) == "$HOME/") t = HOMEABS substr(t, 6)
  else if (substr(t, 1, 8) == "${HOME}/") t = HOMEABS substr(t, 8)
  if (substr(t, 1, 1) == "/") abs = t
  else {
    # A relative target with no `..` cannot leave the cwd, and the cwd of a
    # codex review is the worktree. Only an ESCAPING relative path is resolved,
    # which is what keeps this off the thousands of ordinary `ls dev/...` lines.
    if (t !~ /(^|\/)\.\.(\/|$)/) return 0
    if (cwd == "") return 0
    abs = cwd "/" t
  }
  abs = tc86_norm(abs)
  if (abs !~ /^\/home($|\/)/ && abs !~ /^\/Users($|\/)/) return 0
  if (tc86_under(abs, ROOT1)) return 0
  if (tc86_under(abs, ROOT2)) return 0
  return 1
}

# tc86_echo_is_oor — how many out-of-repo targets does this command echo carry?
#
# The echo is a codex exec record: `<shell> -c <command> in <cwd>`. The cwd is
# taken from the echo ITSELF, not from the environment running the gate — the
# transcript is the only witness to where the command ran, and `../..` means
# nothing without it.
function tc86_echo_is_oor(line,   cwd, payload, n, i, tok, bare, coll, rgenum, hits) {
  cwd = ""
  payload = line
  if (match(line, / in \/[^ ]+$/)) {
    cwd = substr(line, RSTART + 4)
    payload = substr(line, 1, RSTART - 1)
  }
  # `rg` is an enumerator ONLY with --files. `rg PATTERN <dir>` is a content
  # search; sweeping it in would make this a far broader control than the one
  # that was ruled. Checked on the whole payload before separators are rewritten.
  rgenum = (payload ~ /(^|[ \t])--files([ \t]|$)/)
  gsub(/[;|&<>()]+/, " @TC86SEP@ ", payload)
  n = split(payload, tok, /[ \t]+/)
  coll = 0
  hits = 0
  for (i = 1; i <= n; i++) {
    bare = tok[i]
    gsub(QRE, "", bare)
    if (bare == "") continue
    if (bare == "@TC86SEP@") { coll = 0; continue }
    if (bare in ENUM) { coll = 1; continue }
    if (bare == RGCMD) { coll = rgenum; continue }
    if (!coll) continue
    if (substr(bare, 1, 1) == "-") continue
    if (tc86_target_is_oor(bare, cwd)) hits++
  }
  return hits
}

# tc86_is_cmd_noise — a line that is the COMMAND COMPLAINING, not a listing.
# `ls: cannot access ...: No such file or directory` enumerates nothing, and
# treating it as a finding would put a redaction banner on top of a transcript
# that leaked precisely nothing.
function tc86_is_cmd_noise(l) {
  if (l ~ /: No such file or directory$/) return 1
  if (l ~ /: Permission denied$/) return 1
  if (l ~ /(^|[ \t])(ls|find|tree|rg|sed|grep|cat): /) return 1
  return 0
}

BEGIN { RE = ENVIRON["TC86_RE"]; OWN = ENVIRON["TC86_OWN"];
        PROJRE = ENVIRON["TC86_PROJRE"]; BARERE = ENVIRON["TC86_BARERE"]
        HOMEABS = ENVIRON["TC86_HOMEABS"]; ROOT1 = ENVIRON["TC86_ROOT1"]
        ROOT2 = ENVIRON["TC86_ROOT2"]; STATUSRE = ENVIRON["TC86_STATUSRE"]
        ANSIRE = ENVIRON["TC86_ANSIRE"]
        RGCMD = ENVIRON["TC86_RGCMD"]; QUIET = (ENVIRON["TC86_MODE"] == "count")
        # The quote characters are built rather than written: this whole program
        # is a single-quoted shell string, so a literal apostrophe in it would
        # end the string. chr(39) is that apostrophe.
        QRE = "[" sprintf("%c", 39) "\"]"
        tc86_nc = split(ENVIRON["TC86_ENUMCMDS"], tc86_cv, " ")
        for (tc86_ci = 1; tc86_ci <= tc86_nc; tc86_ci++) ENUM[tc86_cv[tc86_ci]] = 1 }
'

# agent_state_classify_file <file> — print "<foreign-line-count> <own-line-count>".
# A line is FOREIGN if ANY agent-state path on it names a project directory other
# than this repo's own; a line naming only this repo's own is OWN.
agent_state_classify_file() {
  TC86_RE="$AGENT_STATE_PATH_RE" TC86_OWN="$AGENT_STATE_OWN_PROJECT_DIR" \
  awk "$AGENT_STATE_AWK_PROLOGUE"'
    $0 ~ RE { if (tc86_is_foreign($0)) f++; else o++ }
    END { printf "%d %d\n", f + 0, o + 0 }
  ' "${1:?agent_state_classify_file needs a file}"
}

# The in-place replacement for one matched FOREIGN line. REDACT, NEVER DELETE:
# TC-RUBRIC-7 closes a review on a persisted terminal artifact, so the transcript
# has to survive as evidence, with the removal visible rather than silent. Plain
# ASCII, and free of `&` and `\`, so it is safe verbatim as a replacement.
AGENT_STATE_REDACTION_MARKER='[REDACTED TC-86] agent-state path removed (foreign Claude Code session transcript content)'

# ============================================================================
# REDACTING CONTENT, NOT PATHS (steward `seq-130`, ruling 2).
# ============================================================================
# The own-project exemption covers PATHS. It does NOT cover CONTENT.
#
# dev/plans/runs/0.8.14-slice-10-fix1-review-20260704T011100Z.log is the one
# already-committed instance of the difference, and fix-1's widening is what
# surfaced it. A codex session ran `ls` and `sed` against THIS repo's memory
# store, and the transcript carries the results — the memory index, three memory
# files, their front-matter and their prose. The PATH-level gate cannot see any
# of that: the dumped lines carry no home-anchored prefix at all. Only the
# command ECHO does, and the echo is exactly the part that is harmless.
#
# So the remediation gained a second, block-level mode. In a codex transcript an
# exec record is:
#
#     exec
#     /bin/bash -c '<command>' in <cwd>          <- the ECHO (matches the pattern)
#      succeeded in 0ms:                         <- the STATUS line
#     <output ...>                               <- the CONTENT
#     exec | codex                               <- the next top-level marker
#
# When the echo names a Claude Code state directory, that command's OUTPUT IS
# that state's content, whoever owns it. `--redact` therefore replaces the output
# block with a counted marker and KEEPS the echo, the status line and everything
# around them: the echo is the evidence of what happened, it is path-only, and it
# is correctly reported afterwards as an own-project WARNING.
#
# The terminator is a top-level `exec`/`codex` marker, NOT a blank line — the
# 0.8.14 dumps contain blank lines internally, and a blank-line terminator would
# truncate the redaction and leave content behind. Asserted in Arm Z rather than
# assumed.
#
# This is a `--redact` capability, NOT a gate predicate: the gate's two classes
# stay exactly the two `seq-130` ruled, so no tree becomes red because of it.
#
# ============================================================================
# FIX-3: FOREIGN PROJECT *INVENTORIES* (steward `seq-130`, ruling 1).
# ============================================================================
# The 0.8.14 transcript carries one more block the fix-2 pass did not reach:
#
#     exec
#     /bin/bash -c 'ls /home/coreyt/.claude/projects' in <cwd>
#      succeeded in 0ms:
#     -home-someone-projects-<a client name>
#     -home-someone-projects-<a domain name>
#     ... 17 lines ...
#
# The two sample lines above are PLACEHOLDERS, and they were not always. This
# comment originally reproduced two of the real encoded directory names, which
# meant the file explaining the redaction was itself one of the files carrying
# what had been redacted: `git grep` found one of them right here, in the
# library, weeks after fix-3 declared the tree clean. Documentation of a leak is
# not exempt from the leak, and naming the offender in the correction repeats it
# — which is why the sentence you are reading does not name it either. Do not put
# a real name back.
#
# That is an INVENTORY OF A PRIVATE PROJECT PORTFOLIO in a PUBLIC repository, and
# several of the names read as client or domain work. It is lower severity than
# conversation content, but ruling 1 is unambiguous: another project's session
# state is never legitimately here. So it is redacted.
#
# TWO REACH PROBLEMS, both deliberate, both recorded here rather than fixed by
# widening anything:
#
#  1. THE ECHO DOES NOT MATCH AGENT_STATE_PATH_RE. The command stops AT
#     `projects` with no component beneath it — which is the very boundary that
#     keeps the pattern off prose, off this file, and off the ASH-Phase2 banner
#     already on main. So block ENTRY (remediation only) also accepts
#     AGENT_STATE_PROJECTS_DIR_RE below, the bare state-directory prefix.
#
#  2. CLAIMED LIMIT — **SUPERSEDED BY FIX-4 BELOW, AND IT WAS WRONG.** Fix-3
#     recorded that "the gate does NOT detect a bare directory listing", on the
#     ground that the listed lines are bare names and no PATH predicate that
#     could see them would survive contact with prose. The premise was true and
#     the conclusion did not follow: the listing is unmatched, but the COMMAND
#     ECHO ABOVE IT is a mechanical shape, and fix-4 detects THAT instead. The
#     lesson is worth more than the paragraph it replaces — "no predicate can see
#     this content" was allowed to stand in for "no predicate can see this
#     event", and a whole class stayed undetected for it. Remediation-reaches-
#     further-than-detection was a real design in fix-2; here it was a reasoning
#     error dressed in the same words.
#
# WHICH BLOCKS QUALIFY. Entry on the bare prefix alone would also catch
# `ls ~/.claude/projects/<own>` — whose output is this repo's OWN session-file
# names, which ruling 1 makes advisory and which must NOT be touched. So a block
# entered on the bare prefix is redacted only if its output actually CONTAINS a
# foreign inventory: at least one line that is, by itself, an encoded project
# directory name (`^-<no-space-no-slash>$`) other than this repo's own. A listing
# of session UUIDs contains none, and is replayed byte-intact.
#
# THE BANNER DOES NOT NAME THEM. `agent_state_redaction_banner` names PROJECTS
# TOUCHED for path-shaped removals, but the foreign inventory is reported as a
# COUNT ONLY: enumerating the names in the banner would put back exactly what the
# redaction removed.

# The bare Claude Code state-directory prefix — `.../.claude/projects` with NO
# component required beneath it. FOR REMEDIATION REACH ONLY. It is deliberately
# NOT the gate's predicate and must never become one: without the
# one-component-beneath boundary it matches prose, this file, and the redaction
# banners themselves. It is used solely to decide that an exec block's OUTPUT is
# worth INSPECTING for a foreign inventory; the decision to redact then rests on
# the output's own contents.
AGENT_STATE_PROJECTS_DIR_RE='/(home|Users)/[^/[:space:]]+/\.claude/projects'

# One line that IS an encoded project directory name and nothing else — the shape
# of a `ls ~/.claude/projects` listing entry.
#
# The SECOND character must not be another `-`. That is not decoration: an
# encoded cwd always begins `-<first-path-component>`, while `---` is a YAML
# front-matter delimiter, and the memory files dumped elsewhere in the 0.8.14
# transcript open and close with one. Without this the inventory COUNT reported in
# the banner is inflated by four, and a banner that miscounts what it removed is
# the sort of small dishonesty that makes the rest of it unreadable.
AGENT_STATE_BARE_PROJECT_DIR_RE='^-[^-/[:space:]][^/[:space:]]*$'

# The in-place replacement for one removed output block — a printf FORMAT, since
# the marker states how many lines it stands in for. Defined once here and handed
# to awk through ENVIRON, for the same no-second-copy reason as the regex.
# Carries no `/home/`|`/Users/` prefix, so it can never match
# AGENT_STATE_PATH_RE and `--redact` converges (asserted in Arm Z).
AGENT_STATE_CONTENT_MARKER_FMT='[REDACTED TC-86] %d line(s) of agent-state CONTENT removed: this block was the OUTPUT of the command echoed above, which read a Claude Code state directory.'

# ============================================================================
# FIX-4: OUT-OF-REPO DIRECTORY INVENTORIES (HITL 2026-07-28, ruling "A+D").
# ============================================================================
# Fix-3 redacted the `ls ~/.claude/projects` inventory and closed. It missed a
# SECOND, EARLIER block in the very same transcript:
#
#     exec
#     /bin/bash -c 'ls /home/coreyt/projects' in <a worktree>
#      succeeded in 0ms:
#     <33 directory names, several of them client or domain work>
#
# No `.claude/` component appears anywhere in that command or its output, so
# AGENT_STATE_PATH_RE cannot see it in ANY of its widenings, and neither can
# AGENT_STATE_PROJECTS_DIR_RE. It had been on origin/main for roughly three
# weeks, in four tracked files, two of which reached the same listing through
# `ls ../..` from a worktree — with no home-anchored text on the echo at all.
#
# THE DIAGNOSIS IS NOT "THE PATTERN WAS TOO NARROW". It is that ONE predicate was
# being asked to cover TWO shapes. Predicate 1 answers "does this line name a
# file inside somebody\047s Claude Code project state?" — a good question, and no
# widening of it reaches a listing of the user\047s home directory, because that
# listing has nothing to do with Claude Code. So fix-4 adds a SECOND predicate
# beside it rather than stretching the first:
#
#   PREDICATE 2 — a command ECHO that ENUMERATES a directory NOT under this
#                 repository, FOLLOWED BY AN OUTPUT BLOCK.
#
# HARD, WITH NO OWN/FOREIGN SPLIT. That asymmetry with predicate 1 is deliberate
# and was ruled explicitly. Predicate 1 has an exemption because this repo\047s
# docs cite its own memory store BY PATH on purpose, constantly, and a gate that
# failed on those would be switched off. Nothing in this repository ever needs
# the OUTPUT of `ls` on a directory outside it. There is no legitimate case to
# exempt, so there is no exemption — including for `~/.claude/projects/<own>`,
# which fix-3 left byte-intact on the FOREIGN axis and which is removed here on
# this one. Two axes, two answers, and the transcript is judged on both.
#
# WHAT COUNTS AS "ENUMERATES":
#   * AGENT_STATE_ENUM_COMMANDS — `ls`, `find`, `tree`. Their non-flag arguments
#     are targets.
#   * `rg` ONLY WITH --files. `rg --files` lists a tree; `rg PATTERN <dir>` is a
#     content search. One of the four exposed files reached the portfolio through
#     `rg --files . .. ../.. ../../..`, so --files had to be in; sweeping in every
#     out-of-repo READ would make this a much broader control than the one ruled,
#     so the rest is out. The boundary is asserted in the fixture suite (ZC13), so
#     it is a decision on the record rather than an accident of implementation.
#
# WHAT COUNTS AS "NOT UNDER THIS REPOSITORY": see tc86_target_is_oor in the awk
# prologue. In short — the target is resolved (`~`/`$HOME` expanded, `..`
# resolved against the cwd the ECHO ITSELF records), and it is out only if it
# lands under /home/ or /Users/ and outside both AGENT_STATE_OWN_PROJECT_ABS and
# AGENT_STATE_OWN_WORKTREES_ABS. A linked worktree is INSIDE, so the extremely
# common `ls ..` from a review worktree is not a finding; `ls ../..` from the
# same place is.
#
# WHY "FOLLOWED BY AN OUTPUT BLOCK" IS PART OF THE PREDICATE. The echo is kept —
# it is path-only, and it is the evidence of what happened, exactly as fix-2
# keeps agent-state echoes. What must not be published is the LISTING. So an echo
# whose output block is empty, or whose only output is the command complaining
# (`No such file or directory`), is NOT a finding: nothing was enumerated, and
# putting a redaction banner on such a transcript would be theatre.
#
# THE BANNER STATES COUNTS AND THE REASON, AND NEVER THE NAMES. Same rule as the
# foreign inventory, for the same reason: enumerating them in the banner would
# put back exactly what the redaction removed.

# The status line that separates a codex command echo from its output. Its
# presence is also the CHEAP NECESSARY CONDITION the gate prefilters on: a block
# cannot exist without one, so a file with no such line cannot carry a finding —
# which makes the prefilter provably non-hiding rather than merely fast.
AGENT_STATE_EXEC_STATUS_RE='^ (succeeded|exited [0-9]+) in .*:$'

# Older Codex CLI transcripts decorate record markers, command echoes, and
# status lines with terminal SGR colour escapes. They are presentation only;
# the block walker removes them for grammar recognition while replaying every
# original byte until an exposed output block is visibly replaced. Keep this
# pattern here with the grammar, so the checker prefilter and the shared walker
# cannot disagree about which rendered status lines are records.
AGENT_STATE_ANSI_SGR_RE=$'\033\\[[0-9;]*m'
# shellcheck disable=SC2034  # consumed by scripts that source this library
# (scripts/check-transcript-hygiene.sh), not within the library itself.
AGENT_STATE_RENDERED_EXEC_STATUS_RE="^(${AGENT_STATE_ANSI_SGR_RE})* (succeeded|exited [0-9]+) in .*:(${AGENT_STATE_ANSI_SGR_RE})*$"

# The commands whose non-flag arguments are directory targets.
AGENT_STATE_ENUM_COMMANDS='ls find tree'

# `rg` enumerates only under this flag; see the argument above.
AGENT_STATE_ENUM_CONDITIONAL_COMMAND='rg'

# The in-place replacement for a removed out-of-repo listing. Like its sibling
# above it is a printf FORMAT carrying the line count, it contains no
# `/home/`|`/Users/` prefix so it can never match AGENT_STATE_PATH_RE, and it
# begins with the same `[REDACTED TC-86]` prefix so the idempotence rule in
# flush() recognises it. It names the reason in words the next reader can act on.
AGENT_STATE_OOR_MARKER_FMT='[REDACTED TC-86] %d line(s) removed: this block was the OUTPUT of the command echoed above, which enumerated an out-of-repo directory. The echo is kept; the listing is not published.'

# ---------------------------------------------------------------------------
# THE BLOCK WALK — ONE implementation, two modes.
# ---------------------------------------------------------------------------
# The GATE must fail on exactly the blocks `--redact` removes. If those two ever
# disagree the result is a FALSE GREEN BUILT INTO THE DESIGN — the gate certifies
# a file the redactor would have cleaned, or the redactor cleans a shape the gate
# never fails on and the gate\047s silence means nothing. That is the same
# argument that gives AGENT_STATE_PATH_RE exactly one home, so the walk gets one
# home too: this program, with TC86_MODE selecting whether it emits the redacted
# stream or only counts. `emit()` is the only difference between the two.
AGENT_STATE_BLOCK_WALK_AWK='
    function emit(s) { if (!QUIET) print s }
    # Parse presentation-free record syntax but emit raw transcript bytes. The
    # old CLI used SGR around `exec`, its command echo, and the status separator;
    # making colour significant would falsely certify the exact listing shape
    # this walk governs.
    function tc86_plain(s) { gsub(ANSIRE, "", s); return s }
    function flush(   i) {
      if (in_block) {
        # A block is removed when it still holds unredacted lines AND one of:
        #  (a) the echo carried a full agent-state PATH;
        #  (b) the output itself lists other projects\047 encoded directory names
        #      (the fix-3 FOREIGN INVENTORY, whose echo the path pattern cannot
        #      see, and which must not swallow the sibling own-session listing);
        #  (c) fix-4: the echo ENUMERATED AN OUT-OF-REPO DIRECTORY and the block
        #      actually holds a listing rather than an error message.
        if ((fresh > 0 && (echo_is_path || foreignnames > 0)) || (echo_is_oor && oorfresh > 0)) {
          # PREDICATE 2 DETECTION is counted FIRST and INDEPENDENTLY of which
          # cause gets the credit below. The distinction is load-bearing and was
          # a live bug for one round: a block can satisfy BOTH predicates (`ls
          # ~/.claude/projects` is an out-of-repo enumeration AND a foreign
          # inventory), and attributing it to the graver cause for the banner
          # must not make it invisible to the gate. What the GATE fails on is
          # "the fix-4 predicate held"; what the BANNER reports is "which cause
          # removed these lines". Those are different questions.
          if (echo_is_oor && oorfresh > 0) { oorhitblocks++; oorhitlines += nonblank }
          # The marker, and the COUNT, must describe THIS block truthfully.
          # Saying "read a Claude Code state directory" over a listing of the
          # user\047s home directory would be a small lie in the one place a
          # reader has nothing else to go on. So the two causes get two markers
          # and two DISJOINT tallies, and the banner can state each without
          # borrowing the other\047s justification. A block that qualifies under
          # both is attributed to the agent-state cause, which is the graver one.
          if (echo_is_path || foreignnames > 0) {
            blocks++
            removed += nonblank
            fnames += foreignnames
            emit(sprintf(ENVIRON["TC86_CFMT"], nonblank) "\n")
          } else {
            oorblocks++
            oorlines += nonblank
            emit(sprintf(ENVIRON["TC86_OORFMT"], nonblank) "\n")
          }
        } else {
          # IDEMPOTENCE. After one pass the block body IS the marker, and the
          # echo above it still matches — so a second pass would otherwise redact
          # the marker, emit a fresh banner, and never converge. A block whose
          # every non-blank line is already a marker is replayed verbatim and
          # counted as nothing.
          for (i = 1; i <= nbuf; i++) emit(buf[i])
        }
        in_block = 0
      }
    }
    {
      if (in_block) {
        # Top-level record markers END the block. NOT a blank line: real dumps
        # contain blank lines, and stopping at one would leave content behind.
        plain = tc86_plain($0)
        if (plain == "exec" || plain == "codex") {
          flush(); emit($0); in_echo = (plain == "exec"); echobuf = ""; next
        }
        nbuf++
        buf[nbuf] = $0
        if (plain != "") {
          nonblank++
          if (plain !~ /^\[REDACTED TC-86\]/) {
            fresh++
            if (!tc86_is_cmd_noise(plain)) oorfresh++
          }
          if (plain ~ BARERE && plain != OWN) foreignnames++
        }
        next
      }
      plain = tc86_plain($0)
      if (plain == "exec" || plain == "codex") {
        in_echo = (plain == "exec"); echobuf = ""; emit($0); next
      }
      if (in_echo) {
        # The ECHO is everything between the `exec` marker and the status line.
        # Fix-2 read only the single line directly under `exec`; a codex command
        # echo can wrap across lines, and a wrapped echo would then have been
        # invisible to BOTH predicates. Accumulating to the status line also
        # keeps the original anchoring property that mattered — an OUTPUT line
        # carrying a path (the 0.8.14 log has one, a `sed: read error` message)
        # is inside a block, never an echo, so it cannot start a phantom block.
        if (plain ~ STATUSRE) {
          emit($0)
          echo_is_path = (echobuf ~ RE)
          echo_is_oor = (tc86_echo_is_oor(echobuf) > 0)
          # Entering a block BUFFERS it, so entry stays restricted to blocks that
          # could actually be removed. An ordinary `cargo test` block in a
          # multi-megabyte transcript is streamed, not held in memory.
          if (echo_is_path || echobuf ~ PROJRE || echo_is_oor) {
            in_block = 1; nonblank = 0; fresh = 0; oorfresh = 0
            nbuf = 0; foreignnames = 0
          }
          in_echo = 0
          next
        }
        echobuf = echobuf " " plain
        emit($0)
        next
      }
      emit($0)
    }
    END {
      flush()
      if (QUIET)
        printf "%d %d %d %d %d %d %d\n", removed + 0, blocks + 0, fnames + 0, \
               oorblocks + 0, oorlines + 0, oorhitblocks + 0, oorhitlines + 0
      else
        printf "%d %d %d %d %d %d %d\n", removed + 0, blocks + 0, fnames + 0, \
               oorblocks + 0, oorlines + 0, oorhitblocks + 0, oorhitlines + 0 > ENVIRON["TC86_COUNTFILE"]
    }
'

# agent_state_block_walk_env — the ENVIRON handoff for the walk, in one place.
# Everything arrives through the environment rather than `awk -v`, because -v
# processes escape sequences: it would turn `\.claude` into `.claude` (any char,
# not a literal dot) and quietly loosen the pattern.
agent_state_block_walk() {
  TC86_RE="$AGENT_STATE_PATH_RE" TC86_OWN="$AGENT_STATE_OWN_PROJECT_DIR" \
  TC86_PROJRE="$AGENT_STATE_PROJECTS_DIR_RE" TC86_BARERE="$AGENT_STATE_BARE_PROJECT_DIR_RE" \
  TC86_HOMEABS="$AGENT_STATE_OWN_HOME_ABS" TC86_ROOT1="$AGENT_STATE_OWN_PROJECT_ABS" \
  TC86_ROOT2="$AGENT_STATE_OWN_WORKTREES_ABS" TC86_STATUSRE="$AGENT_STATE_EXEC_STATUS_RE" \
  TC86_ANSIRE="$AGENT_STATE_ANSI_SGR_RE" \
  TC86_ENUMCMDS="$AGENT_STATE_ENUM_COMMANDS" TC86_RGCMD="$AGENT_STATE_ENUM_CONDITIONAL_COMMAND" \
  TC86_CFMT="$AGENT_STATE_CONTENT_MARKER_FMT" TC86_OORFMT="$AGENT_STATE_OOR_MARKER_FMT" \
  TC86_MODE="${TC86_MODE:-redact}" TC86_COUNTFILE="${TC86_COUNTFILE:-/dev/null}" \
  awk "$AGENT_STATE_AWK_PROLOGUE$AGENT_STATE_BLOCK_WALK_AWK" "$@"
}

# agent_state_redact_content_blocks <in-file> <out-file> <count-file>
#
# Writes the redacted stream to <out-file> and, to <count-file>, seven fields:
# "<content-lines> <content-blocks> <foreign-inventory-lines> <oor-blocks>
#  <oor-lines> <predicate2-blocks> <predicate2-lines>".
# The first five are BANNER ATTRIBUTION and are disjoint; the last two are
# PREDICATE 2 DETECTION and are a superset of fields 4/5 — see flush().
# Non-block lines pass through byte-intact; a file with nothing
# to remove comes through unchanged, which is what makes `--redact` a no-op on
# the sixteen ordinary own-project path citations.
agent_state_redact_content_blocks() {
  local in="${1:?agent_state_redact_content_blocks needs an input file}"
  local out="${2:?agent_state_redact_content_blocks needs an output file}"
  local cf="${3:?agent_state_redact_content_blocks needs a count file}"
  TC86_MODE=redact TC86_COUNTFILE="$cf" agent_state_block_walk "$in" >"$out"
}

# agent_state_count_out_of_repo_blocks <file> — print "<blocks> <lines>".
#
# THE GATE\047S SECOND PREDICATE, evaluated by running the SAME walk in count
# mode. Not a parallel re-implementation: the whole point of the two-mode program
# above is that "what the gate fails on" and "what --redact removes" cannot drift
# apart, because they are the same code reading the same bytes.
agent_state_count_out_of_repo_blocks() {
  TC86_MODE=count agent_state_block_walk "${1:?agent_state_count_out_of_repo_blocks needs a file}" \
    | awk '{ printf "%d %d\n", $6 + 0, $7 + 0 }'
}

# agent_state_redact_stream — filter stdin to stdout, rewriting every line that
# carries a FOREIGN agent-state path into AGENT_STATE_REDACTION_MARKER.
# Whole-line replacement, not a snip: a matched line is raw session JSONL, so the
# remainder of it is exactly the content that must not be published.
#
# FOREIGN ONLY, per `seq-130`. This function is also the CAPTURE-TIME filter in
# dev/agent-tools/codex-nostdin.sh, so redacting own-project paths here would
# destroy legitimate review output — a §9 review that reads this project's own
# hand-off prompt has to be able to quote the path it read.
#
# Non-matching lines pass through byte-intact. awk block-buffers when stdout is
# not a terminal, which is precisely the tee'd-to-a-file case this exists for; no
# output is lost, it merely arrives in chunks.
agent_state_redact_stream() {
  TC86_RE="$AGENT_STATE_PATH_RE" TC86_OWN="$AGENT_STATE_OWN_PROJECT_DIR" \
  TC86_MARKER="$AGENT_STATE_REDACTION_MARKER" \
  awk "$AGENT_STATE_AWK_PROLOGUE"'
    $0 ~ RE && tc86_is_foreign($0) { print ENVIRON["TC86_MARKER"]; next }
    { print }
  '
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

# agent_state_foreign_project_dirs — the same, minus this repo's own directory.
agent_state_foreign_project_dirs() {
  agent_state_project_dirs | grep -vxF -- "$AGENT_STATE_OWN_PROJECT_DIR" || true
}

# agent_state_self_exemption_notice — the VISIBLE self-exemption. Printed by
# scripts/check-transcript-hygiene.sh on EVERY run, clean or failing, because
# `seq-130` rejected a self-exemption that a reader could not see from the gate's
# own output. It names the exempted directory, says where the name comes from,
# gives the reason, and states the accepted residual.
#
# It must never match AGENT_STATE_PATH_RE, or the gate would fail on its own
# source file: it names the project DIRECTORY with no `/home/`|`/Users/` prefix
# beneath a `.claude/projects/` component, which is discriminator 1 doing its
# job. (Arm K2 scans this very file as a tracked file and asserts rc=0.)
agent_state_self_exemption_notice() {
  cat <<NOTICE
note  transcript-hygiene: SELF-EXEMPTION, STATED IN THE OPEN (TC-86, steward seq-130).
      SCOPE : this exemption applies to PREDICATE 1 (agent-state PATHS) ONLY.
                     PREDICATE 2 — a command that ENUMERATES a directory outside this
                     repository, followed by its output — has NO exemption of any kind,
                     including for this project's own state directory. Nothing in this
                     repository legitimately contains such a listing, so there was
                     nothing to trade (HITL 2026-07-28).
      HARD-CHECKED : every encoded Claude Code project directory EXCEPT this repo's own.
                     A path into another project's session state is never legitimate
                     here and fails this gate, in every shape, tool-results/ included.
      NOT HARD-CHECKED : ${AGENT_STATE_OWN_PROJECT_DIR}
                     — this repository's own project directory, i.e. the '/'->'-'
                     encoding of ${AGENT_STATE_OWN_PROJECT_ABS}. Hits there are
                     reported above as WARN and do NOT fail this gate.
      WHY : this repo's README, hand-off prompts and prune measurements cite paths
                     into this project's own memory store on purpose. A gate that
                     hard-failed those would be switched off, which is worse than the
                     residual below.
      ACCEPTED RESIDUAL (knowingly traded, not an oversight) : this repository's OWN
                     session state — including <session>/tool-results/ content, the
                     full untruncated output of whatever a tool did — CAN STILL BE
                     COMMITTED. The WARN lines are the only thing that surfaces it.
                     Nothing suppresses them, and --redact does not rewrite them.
NOTICE
}

# agent_state_redaction_banner <total> <projects-csv> <path-lines> <content-lines> <blocks> <own:0|1> [foreign-inventory-lines] [oor-blocks] [oor-lines]
#
# The block prepended to a redacted file. Shape descended from the banner already
# on main in dev/plans/runs/codex/agent-seat-hardening/ASH-Phase2-20260728T034657Z.log:
# it states the removed-line count, the projects, the reason, and that no review
# finding came from the removed lines. Fix-2 splits the count into its two causes
# (PATH lines vs CONTENT blocks) and adds the own-project provenance line, because
# "this was your own memory store" is a materially different statement to the
# reader than "this was someone else's transcript".
#
# The banner must never match AGENT_STATE_PATH_RE — otherwise redacting a file
# would leave it dirty and `--redact` could never converge. It names project
# DIRECTORIES only, with NO `/home/`|`/Users/` prefix. Asserted, not assumed: the
# idempotence arms in scripts/tests/test_check_transcript_hygiene.sh prove a
# second `--redact` pass is byte-identical, for the widened and content shapes too.
agent_state_redaction_banner() {
  local total="${1:?agent_state_redaction_banner needs a total line count}"
  local projects="${2:-(none identified)}"
  local path_lines="${3:-0}"
  local content_lines="${4:-0}"
  local blocks="${5:-0}"
  local own="${6:-0}"
  local fnames="${7:-0}"
  local oor_blocks="${8:-0}"
  local oor_lines="${9:-0}"
  printf '=== STEWARD REDACTION (TC-86 transcript hygiene) ===\n'
  printf '%s line(s) redacted in place in this transcript before it was committed.\n' "$total"
  if [ "$path_lines" -gt 0 ]; then
    cat <<PATHS
WHAT THEY WERE (${path_lines} line(s)): home-anchored absolute paths into ANOTHER
project's Claude Code state directory (~/.claude/projects/...) — i.e. raw agent
SESSION TRANSCRIPT content slurped into this file by a tool running under
--dangerously-bypass-approvals-and-sandbox.
PATHS
  fi
  if [ "$content_lines" -gt 0 ]; then
    cat <<CONTENT_BLOCK
WHAT THEY WERE (${content_lines} line(s) in ${blocks} block(s)): the OUTPUT of commands
echoed in this transcript that READ a Claude Code state directory — i.e. the
CONTENT of that state, not merely its path. The command echoes themselves are
kept: they are the evidence of what happened, and they are path-only.
CONTENT_BLOCK
  fi
  if [ "$fnames" -gt 0 ]; then
    cat <<INVENTORY
FOREIGN PROJECT INVENTORY (${fnames} line(s) of the above): part of the removed
output was a listing of ~/.claude/projects — bare encoded directory names
belonging to OTHER projects, i.e. an inventory of a private project portfolio.
They are reported here as a COUNT ONLY and are deliberately NOT enumerated:
naming them in this banner would put back exactly what the redaction removed.
INVENTORY
  fi
  if [ "$oor_lines" -gt 0 ]; then
    cat <<OOR
OUT-OF-REPO DIRECTORY INVENTORY (${oor_lines} line(s) in ${oor_blocks} block(s)):
the commands echoed above ENUMERATED a directory that is not part of
this repository or its worktrees — a home directory, a projects directory, a
package cache, a state directory. Their output was therefore a listing of files
and directories on the operator's machine that have nothing to do with this
project. The commands themselves are kept: an echo is path-only and is the
evidence of what happened. The LISTINGS are reported as a COUNT ONLY and are
deliberately NOT enumerated — naming them here would put back exactly what the
redaction removed.
OOR
  fi
  printf 'PROJECTS TOUCHED: %s\n' "$projects"
  # The own-project provenance note explains a CONTENT removal. With nothing
  # removed on that cause it would be describing lines that are not there.
  if [ "$own" -eq 1 ] && [ "$content_lines" -gt 0 ]; then
    cat <<OWN_BLOCK
OWN PROJECT: this content came from THIS repository's own Claude Code state
directory (${AGENT_STATE_OWN_PROJECT_DIR}) — this project's own memory store.
Under the TC-86 threat model (steward seq-130) an own-project PATH is only a
WARNING, but its CONTENT is still this project's private agent state and does not
belong in a PUBLIC repository, so it is removed here.
OWN_BLOCK
  fi
  cat <<'TAIL'
WHY REMOVED: github.com/coreyt/fathomdb is a PUBLIC repository. Committing
session transcripts or state contents into it would publish them.
NO FINDING WAS LOST: no review finding came from the removed lines. Every verdict
and all reasoning above and below are intact.
NOT DELETED: each removed line was REPLACED IN PLACE by a marker, so this
transcript still stands as TC-RUBRIC-7 evidence.
=== END STEWARD REDACTION ===
TAIL
}
