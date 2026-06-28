#!/usr/bin/env bash
# Markdown lint + format check + link integrity.
# Pass: silent. Fail: structured diagnostic + spill path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# markdownlint-cli2 — structural lint
if [ -d node_modules/.bin ] && [ -x node_modules/.bin/markdownlint-cli2 ]; then
  run_capped lint-md-structure ./node_modules/.bin/markdownlint-cli2
else
  skip_notice lint-md-structure "markdownlint-cli2 not installed (run scripts/bootstrap.sh)"
fi

# NOTE: prettier --check was REMOVED from the markdown gate (0.8.9.1, HITL 2026-06-28).
# prettier's markdown formatter is non-configurable for emphasis style and its *->_ reflow
# CORRUPTS multi-line / nested / adjacent-to-`code` emphasis spans (broken spans, snake_case
# `_` loss, word-joins that change tokenization). markdownlint-cli2 (AST/token-aware) above is
# the sole structural formatter; it does not have this failure mode. Neutrality of any future
# bulk markdown reformat is verified out-of-band by dev/tools/md-neutrality-guard (markdown-it-py
# AST visible-text diff), not by re-introducing prettier.

# lychee — link integrity. Offline by default; pass AGENT_LINK_CHECK=online to hit the network.
if command -v lychee >/dev/null 2>&1; then
  if [ "${AGENT_LINK_CHECK:-offline}" = "online" ]; then
    run_capped lint-md-links lychee --offline=false --no-progress '**/*.md'
  else
    run_capped lint-md-links lychee --offline --no-progress '**/*.md'
  fi
else
  skip_notice lint-md-links "lychee not installed (run scripts/bootstrap.sh)"
fi
