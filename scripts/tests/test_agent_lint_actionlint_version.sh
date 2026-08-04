#!/usr/bin/env bash
# agent-lint.sh must reject a local actionlint that differs from CI's pin
# before any other linter can conceal the workflow-validation drift.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

mkdir -p "$FIX/scripts/lib" "$FIX/bin"
cp "$REPO_ROOT/scripts/agent-lint.sh" "$FIX/scripts/agent-lint.sh"
cp "$REPO_ROOT/scripts/lib/agent-output.sh" "$FIX/scripts/lib/agent-output.sh"
cp "$REPO_ROOT/scripts/lib/actionlint-version.sh" "$FIX/scripts/lib/actionlint-version.sh"
# 0.8.21 Slice 30: agent-lint.sh also sources the shellcheck pin library.
cp "$REPO_ROOT/scripts/lib/shellcheck-version.sh" "$FIX/scripts/lib/shellcheck-version.sh"
printf '#!/usr/bin/env bash\nprintf "1.7.7\\n"\n' >"$FIX/bin/actionlint"
printf '#!/usr/bin/env bash\nprintf "ruff 0.15.17\\n"\n' >"$FIX/bin/ruff"
chmod +x "$FIX/scripts/agent-lint.sh" "$FIX/bin/actionlint" "$FIX/bin/ruff"
(
  cd "$FIX"
  git init -q
)

set +e
OUT="$(cd "$FIX" && PATH="$FIX/bin:$PATH" bash scripts/agent-lint.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-lint.sh accepted an out-of-date actionlint\n' >&2
  exit 1
fi
if ! grep -Fq 'actionlint 1.7.12' <<<"$OUT" || ! grep -Fq '1.7.7' <<<"$OUT"; then
  printf 'FAIL  actionlint version guard did not identify required and selected versions\n' >&2
  exit 1
fi
if ! grep -Fq 'scripts/bootstrap.sh in a clean non-worktree checkout' <<<"$OUT"; then
  printf 'FAIL  actionlint version guard lacks bootstrap remediation\n' >&2
  exit 1
fi
printf 'PASS  agent-lint.sh rejects an actionlint version that differs from CI\n'
