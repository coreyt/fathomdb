#!/usr/bin/env bash
# scripts/tests/test_lint_md_hard_fail_on_missing_linter.sh — TC-37 recurrence guard.
#
# TC-37: scripts/agent-lint-md.sh used to skip_notice (exit 0) when
# markdownlint-cli2 was absent from node_modules/.bin — the NORMAL state inside an
# orchestration worktree. That vacuous green hid a genuinely red `main` (9
# markdown errors, 2026-07-02 -> 2026-07-24) for three weeks: every orchestrator
# ran the gate, saw green, and never knew the structural check hadn't run at all.
#
# This test builds a throwaway fixture repo containing ONLY a copy of
# scripts/agent-lint-md.sh + its lib/agent-lint-docs.sh dependencies, with
# deliberately NO node_modules directory, and a PATH stripped down to the bare
# system tools (so markdownlint-cli2 is genuinely unresolvable inside the
# fixture — not merely "not on this machine", but structurally absent from
# every place the script looks). It then asserts:
#   1. scripts/agent-lint-md.sh exits NON-ZERO (not the old vacuous 0).
#   2. the failure message names the missing tool and how to fix it.
#
# Against the CURRENT (fixed) script this is GREEN. Run it against a git-show
# of the script as of HEAD~1 (pre-fix) to reproduce the RED (see the closure
# report for that transcript) — a checked-in "old" copy is deliberately NOT
# kept here since that would itself be a vacuous-green trap (a stale fixture
# nobody re-derives).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

(
  cd "$FIX"
  git init -q
  git config user.email test@example.com
  git config user.name test
  mkdir -p scripts/lib
  cp "$REPO_ROOT/scripts/agent-lint-md.sh" scripts/agent-lint-md.sh
  cp "$REPO_ROOT/scripts/lib/agent-output.sh" scripts/lib/agent-output.sh
  cp "$REPO_ROOT/scripts/agent-lint-docs.sh" scripts/agent-lint-docs.sh
  chmod +x scripts/agent-lint-md.sh scripts/agent-lint-docs.sh
  echo "# fixture" >README.md
  git add -A
  git commit -q -m fixture
) >/dev/null

# No node_modules/ anywhere under $FIX (genuinely absent), and PATH stripped so
# `command -v markdownlint-cli2` cannot resolve one from the ambient environment
# either. Note: scripts/agent-lint-docs.sh independently falls back to the
# primary checkout's real node_modules (an intentional, orthogonal resolution
# tier — see its own header) but finds no docs/ dir in this fixture and is a
# no-op either way; the check under test here is agent-lint-md.sh's own
# lint-md-structure leg, which has no such fallback and must fail loud.
set +e
OUT="$(cd "$FIX" && PATH="/usr/bin:/bin" bash scripts/agent-lint-md.sh 2>&1)"
RC=$?
set -e

echo "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-lint-md.sh exited 0 with markdownlint-cli2 genuinely unavailable (TC-37 vacuous-green regression)\n' >&2
  exit 1
fi

if ! grep -qi 'markdownlint-cli2' <<<"$OUT"; then
  printf 'FAIL  hard-fail message does not name the missing tool\n' >&2
  exit 1
fi

if ! grep -qi 'bootstrap' <<<"$OUT"; then
  printf 'FAIL  hard-fail message does not tell the operator how to fix it (scripts/bootstrap.sh)\n' >&2
  exit 1
fi

printf 'PASS  agent-lint-md.sh hard-fails (exit=%d) with an actionable message when markdownlint-cli2 is unavailable (TC-37)\n' "$RC"
