#!/usr/bin/env bash
# Markdown lint + format check + link integrity.
# Pass: silent. Fail: structured diagnostic + spill path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# markdownlint-cli2 — structural lint.
#
# TC-37 (fixed here): this used to skip_notice (exit 0) when the binary was absent —
# the NORMAL state inside an orchestration worktree — which hid a genuinely red `main`
# (9 markdown errors, 2026-07-02 -> 2026-07-24) for three weeks: every orchestrator ran
# this gate, saw green, and never knew it hadn't actually run. A gate that cannot run
# must say so loudly and fail, never report a silent pass. See scripts/tests/
# test_lint_md_hard_fail_on_missing_linter.sh for the recurrence-guard test.
if [ -d node_modules/.bin ] && [ -x node_modules/.bin/markdownlint-cli2 ]; then
  run_capped lint-md-structure ./node_modules/.bin/markdownlint-cli2
else
  {
    echo "FAIL lint-md-structure: markdownlint-cli2 not found at node_modules/.bin/markdownlint-cli2."
    echo "  A missing structural markdown linter must never report a silent pass (TC-37)."
    echo "  Fix: run scripts/bootstrap.sh to install it, OR (inside a linked worktree)"
    echo "  symlink the primary checkout's node_modules:"
    echo "    ln -s /home/coreyt/projects/fathomdb/node_modules node_modules"
  } >&2
  exit 1
fi

# T3/8+9 (DOC-HYGIENE-1): every top-level dev/plans/*.md must carry a valid
# `status:` frontmatter value — the recurrence guard for that convention.
# Pure bash/grep; no external binary, so this leg has no absent-tool skip path.
run_capped lint-plans-status "$SCRIPT_DIR/lint-plans-status.sh"

# docs/** structural lint. The repo .markdownlint-cli2.jsonc IGNORES docs/** (it is
# otherwise gated only by `mkdocs build --strict`, which does NOT enforce markdownlint
# style). agent-lint-docs.sh lints docs/** with the same .markdownlint.jsonc rules via
# an out-of-tree copy (so the ignore does not apply). It self-skips if the binary is
# absent, so run it unconditionally.
run_capped lint-md-docs "$SCRIPT_DIR/agent-lint-docs.sh"

# NOTE: prettier --check was REMOVED from the markdown gate (0.8.9.1, HITL 2026-06-28).
# prettier's markdown formatter is non-configurable for emphasis style and its *->_ reflow
# CORRUPTS multi-line / nested / adjacent-to-`code` emphasis spans (broken spans, snake_case
# `_` loss, word-joins that change tokenization). markdownlint-cli2 (AST/token-aware) above is
# the sole structural formatter; it does not have this failure mode. Neutrality of any future
# bulk markdown reformat is verified out-of-band by dev/tools/md-neutrality-guard (markdown-it-py
# AST visible-text diff), not by re-introducing prettier.

# lychee — link integrity. Offline by default; pass AGENT_LINK_CHECK=online to hit the network.
#
# TC-37 follow-up (deliberately left as skip-on-absent, unlike lint-md-structure above):
# lychee is a separate Rust binary (cargo install), not part of the node_modules symlink
# convention that makes markdownlint-cli2 trivially available in a worktree, and it is not
# the tool implicated in the TC-37 incident (that was markdownlint-cli2 missing 9 real
# structural errors). Hard-failing here would break every worktree that has not run the
# full `scripts/bootstrap.sh` (which `cargo install`s lychee) for a check whose own online/
# offline modality already signals partial, best-effort coverage. Revisit if a lychee-only
# vacuous-green incident is ever observed.
if command -v lychee >/dev/null 2>&1; then
  if [ "${AGENT_LINK_CHECK:-offline}" = "online" ]; then
    run_capped lint-md-links lychee --offline=false --no-progress '**/*.md'
  else
    run_capped lint-md-links lychee --offline --no-progress '**/*.md'
  fi
else
  skip_notice lint-md-links "lychee not installed (run scripts/bootstrap.sh)"
fi
