#!/usr/bin/env bash
# agent-typecheck.sh must reject a local Pyright that differs from CI's pin
# before type checking can report a misleading green result.
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
  cp "$REPO_ROOT/scripts/agent-typecheck.sh" scripts/agent-typecheck.sh
  cp "$REPO_ROOT/scripts/lib/agent-output.sh" scripts/lib/agent-output.sh
  printf '#!/usr/bin/env bash\nexit 0\n' >.venv/bin/cargo
  printf '#!/usr/bin/env bash\nprintf "pyright 1.1.409\\n"\n' >.venv/bin/pyright
  chmod +x scripts/agent-typecheck.sh .venv/bin/cargo .venv/bin/pyright
  touch README.md
  git add -A
  git commit -q -m fixture
) >/dev/null

set +e
OUT="$(cd "$FIX" && PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-typecheck.sh accepted an out-of-date Pyright\n' >&2
  exit 1
fi
if ! grep -Fq 'Pyright 1.1.410' <<<"$OUT" || ! grep -Fq 'pyright 1.1.409' <<<"$OUT"; then
  printf 'FAIL  Pyright version guard did not identify required and selected versions\n' >&2
  exit 1
fi
if ! grep -Fq 'scripts/bootstrap.sh in a clean non-worktree checkout' <<<"$OUT"; then
  printf 'FAIL  Pyright version guard lacks bootstrap remediation\n' >&2
  exit 1
fi
printf 'PASS  agent-typecheck.sh rejects a Pyright version that differs from CI\n'
