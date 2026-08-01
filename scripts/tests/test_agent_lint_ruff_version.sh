#!/usr/bin/env bash
# agent-lint.sh must reject a local Ruff that differs from CI's pinned version.
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
  mkdir -p scripts/lib .venv/bin
  cp "$REPO_ROOT/scripts/agent-lint.sh" scripts/agent-lint.sh
  cp "$REPO_ROOT/scripts/lib/agent-output.sh" scripts/lib/agent-output.sh
  printf '#!/usr/bin/env bash\nprintf "ruff 0.15.10\\n"\n' >.venv/bin/ruff
  chmod +x scripts/agent-lint.sh .venv/bin/ruff
  touch README.md
  git add -A
  git commit -q -m fixture
) >/dev/null

set +e
OUT="$(cd "$FIX" && bash scripts/agent-lint.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-lint.sh accepted an out-of-date Ruff\n' >&2
  exit 1
fi

if ! grep -Fq 'Ruff 0.16.1' <<<"$OUT"; then
  printf 'FAIL  guard output does not name the required Ruff version\n' >&2
  exit 1
fi

if ! grep -Fq '0.15.10' <<<"$OUT"; then
  printf 'FAIL  guard output does not name the selected Ruff version\n' >&2
  exit 1
fi

if ! grep -Fq 'scripts/bootstrap.sh on the main checkout' <<<"$OUT"; then
  printf 'FAIL  guard output lacks the main-checkout bootstrap remediation\n' >&2
  exit 1
fi

printf 'PASS  agent-lint.sh rejects a Ruff version that differs from CI\n'
