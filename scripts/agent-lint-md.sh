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

# prettier --check — format check
if [ -d node_modules/.bin ] && [ -x node_modules/.bin/prettier ]; then
  run_capped lint-md-format ./node_modules/.bin/prettier --check '**/*.md' --log-level warn
else
  skip_notice lint-md-format "prettier not installed (run scripts/bootstrap.sh)"
fi

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
