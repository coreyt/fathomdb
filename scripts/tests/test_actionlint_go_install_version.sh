#!/usr/bin/env bash
# A Go-installed actionlint v1.7.12 reports `v1.7.12`, while the release
# download reports `1.7.12`. Both identify the same exact pinned release.
# Keep bootstrap and agent-lint on one normalizer so a clean Go bootstrap does
# not fail after Go automatically switches to actionlint's required toolchain.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

fake_actionlint="$FIX/actionlint"
printf '#!/usr/bin/env bash\nprintf "v1.7.12\\n"\n' >"$fake_actionlint"
chmod +x "$fake_actionlint"

# shellcheck source=../lib/actionlint-version.sh
. "$REPO_ROOT/scripts/lib/actionlint-version.sh"

reported_version="$(read_actionlint_version "$fake_actionlint")"
if [ "$reported_version" != "1.7.12" ]; then
  printf 'FAIL  Go-installed actionlint v1.7.12 must normalize to exact pin 1.7.12\n' >&2
  exit 1
fi

# GitHub Actions runs bootstrap and agent-verify in separate steps. `go
# install` places actionlint under GOPATH/bin, which is not added to PATH by
# default, so the second step must still resolve the exact binary.
fake_go_bin="$FIX/go-bin"
fake_gopath="$FIX/go-path"
mkdir -p "$fake_go_bin" "$fake_gopath/bin"
printf '#!/usr/bin/env bash\nif [ "$1" = env ] && [ "$2" = GOPATH ]; then\n  printf "%%s\\n" "${FAKE_GOPATH:?}"\n  exit 0\nfi\nexit 2\n' >"$fake_go_bin/go"
printf '#!/usr/bin/env bash\nprintf "v1.7.12\\n"\n' >"$fake_gopath/bin/actionlint"
chmod +x "$fake_go_bin/go" "$fake_gopath/bin/actionlint"

export FAKE_GOPATH="$fake_gopath"
resolved_actionlint="$(PATH="$fake_go_bin:/usr/bin:/bin" find_actionlint_bin)"
if [ "$resolved_actionlint" != "$fake_gopath/bin/actionlint" ]; then
  printf 'FAIL  Go-installed actionlint must resolve when GOPATH/bin is absent from PATH\n' >&2
  exit 1
fi

# Exercise agent-lint itself under the same split-step condition. Its first
# real lint command is deliberately made to fail so this fixture proves that
# the actionlint preflight advanced past resolution without needing a full
# checkout's lint dependencies.
lint_fix="$FIX/agent-lint-fixture"
mkdir -p "$lint_fix/scripts/lib" "$lint_fix/bin" "$lint_fix/go-bin" "$lint_fix/go-path/bin"
cp "$REPO_ROOT/scripts/agent-lint.sh" "$lint_fix/scripts/agent-lint.sh"
cp "$REPO_ROOT/scripts/lib/agent-output.sh" "$lint_fix/scripts/lib/agent-output.sh"
cp "$REPO_ROOT/scripts/lib/actionlint-version.sh" "$lint_fix/scripts/lib/actionlint-version.sh"
printf '#!/usr/bin/env bash\nprintf "ruff 0.15.17\\n"\n' >"$lint_fix/bin/ruff"
printf '#!/usr/bin/env bash\nprintf "fake cargo reached\\n" >&2\nexit 7\n' >"$lint_fix/bin/cargo"
printf '#!/usr/bin/env bash\nif [ "$1" = env ] && [ "$2" = GOPATH ]; then\n  printf "%%s\\n" "${FAKE_GOPATH:?}"\n  exit 0\nfi\nexit 2\n' >"$lint_fix/go-bin/go"
printf '#!/usr/bin/env bash\nprintf "v1.7.12\\n"\n' >"$lint_fix/go-path/bin/actionlint"
chmod +x "$lint_fix/scripts/agent-lint.sh" "$lint_fix/bin/ruff" "$lint_fix/bin/cargo" \
  "$lint_fix/go-bin/go" "$lint_fix/go-path/bin/actionlint"
(
  cd "$lint_fix"
  git init -q
)
set +e
lint_output="$(cd "$lint_fix" && PATH="$lint_fix/go-bin:$lint_fix/bin:/usr/bin:/bin" \
  FAKE_GOPATH="$lint_fix/go-path" bash scripts/agent-lint.sh 2>&1)"
lint_status=$?
set -e
if [ "$lint_status" -eq 0 ] || ! grep -Fq 'fake cargo reached' <<<"$lint_output" \
  || grep -Fq 'actionlint 1.7.12 is required but not installed' <<<"$lint_output"; then
  printf 'FAIL  agent-lint must resolve Go-installed actionlint before its first lint command\n' >&2
  printf '%s\n' "$lint_output" >&2
  exit 1
fi

for script in scripts/bootstrap.sh scripts/agent-lint.sh; do
  if ! grep -Fq 'lib/actionlint-version.sh' "$REPO_ROOT/$script" \
    || ! grep -Fq 'read_actionlint_version' "$REPO_ROOT/$script" \
    || ! grep -Fq 'find_actionlint_bin' "$REPO_ROOT/$script"; then
    printf 'FAIL  %s must use the shared actionlint resolver and version normalizer\n' "$script" >&2
    exit 1
  fi
done

if ! grep -Fq 'GITHUB_PATH' "$REPO_ROOT/scripts/bootstrap.sh"; then
  printf 'FAIL  bootstrap must persist its actionlint directory for later GitHub Actions steps\n' >&2
  exit 1
fi

printf 'PASS  Go-installed v1.7.12 resolves outside PATH without weakening the exact pin\n'
