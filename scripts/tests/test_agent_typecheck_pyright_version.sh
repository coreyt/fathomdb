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
OUT="$(cd "$FIX" && AGENT_VERBOSE=1 PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
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

cat >"$FIX/.venv/bin/pyright" <<'PYRIGHT'
#!/usr/bin/env bash
if [ "$1" = "--version" ]; then
  printf 'pyright 1.1.410\n'
  printf 'A newer version of pyright is available (1.1.411).\n'
fi
PYRIGHT
chmod +x "$FIX/.venv/bin/pyright"

set +e
OUT="$(cd "$FIX" && AGENT_VERBOSE=1 PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -ne 0 ]; then
  printf 'FAIL  agent-typecheck.sh rejected the pinned Pyright version because it emitted an update warning\n' >&2
  exit 1
fi
if ! grep -Fq 'ok typecheck-python ' <<<"$OUT"; then
  printf 'FAIL  agent-typecheck.sh did not run Pyright after accepting its pinned version line\n' >&2
  exit 1
fi
printf 'PASS  agent-typecheck.sh accepts the pinned version line with a Pyright update warning\n'

cat >"$FIX/.venv/bin/pyright" <<'PYRIGHT'
#!/usr/bin/env bash
if [ "$1" = "--version" ]; then
  printf 'pyright 1.1.409\n'
  printf 'pyright 1.1.410\n'
fi
PYRIGHT
chmod +x "$FIX/.venv/bin/pyright"

set +e
OUT="$(cd "$FIX" && PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-typecheck.sh accepted a wrong first Pyright version masked by a later matching line\n' >&2
  exit 1
fi
if ! grep -Fq 'pyright 1.1.409' <<<"$OUT"; then
  printf 'FAIL  mixed Pyright version output did not report the wrong first line\n' >&2
  exit 1
fi
printf 'PASS  agent-typecheck.sh rejects a wrong first Pyright version despite a later matching line\n'

cat >"$FIX/.venv/bin/pyright" <<'PYRIGHT'
#!/usr/bin/env bash
if [ "$1" = "--version" ]; then
  printf 'unparseable pyright version output\n'
  printf 'pyright 1.1.410\n'
fi
PYRIGHT
chmod +x "$FIX/.venv/bin/pyright"

set +e
OUT="$(cd "$FIX" && PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-typecheck.sh accepted a malformed first line masked by a later matching Pyright version\n' >&2
  exit 1
fi
if ! grep -Fq 'unparseable pyright version output' <<<"$OUT"; then
  printf 'FAIL  mixed malformed Pyright output did not report the first line\n' >&2
  exit 1
fi
printf 'PASS  agent-typecheck.sh rejects a malformed first line despite a later matching Pyright version\n'

printf '#!/usr/bin/env bash\nprintf "unparseable pyright version output\\n"\n' >"$FIX/.venv/bin/pyright"
chmod +x "$FIX/.venv/bin/pyright"

set +e
OUT="$(cd "$FIX" && PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-typecheck.sh accepted malformed Pyright version output\n' >&2
  exit 1
fi
if ! grep -Fq 'unparseable pyright version output' <<<"$OUT"; then
  printf 'FAIL  malformed Pyright version output was not reported\n' >&2
  exit 1
fi
printf 'PASS  agent-typecheck.sh rejects malformed Pyright version output\n'

printf '#!/usr/bin/env bash\n' >"$FIX/.venv/bin/pyright"
chmod +x "$FIX/.venv/bin/pyright"

set +e
OUT="$(cd "$FIX" && PATH="$FIX/.venv/bin:$PATH" bash scripts/agent-typecheck.sh 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -eq 0 ]; then
  printf 'FAIL  agent-typecheck.sh accepted missing Pyright version output\n' >&2
  exit 1
fi
if ! grep -Fq 'Pyright 1.1.410 is required; selected ' <<<"$OUT"; then
  printf 'FAIL  missing Pyright version output was not reported\n' >&2
  exit 1
fi
printf 'PASS  agent-typecheck.sh rejects missing Pyright version output\n'
