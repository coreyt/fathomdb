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

if [ "$(read_actionlint_version "$fake_actionlint")" != "1.7.12" ]; then
  printf 'FAIL  Go-installed actionlint v1.7.12 must normalize to exact pin 1.7.12\n' >&2
  exit 1
fi

# GitHub Actions runs bootstrap and agent-verify in separate steps. `go
# install` places actionlint under GOPATH/bin, which is not added to PATH by
# default, so the second step must still resolve the exact binary.
fake_go_bin="$FIX/go-bin"
fake_gopath="$FIX/go-path"
mkdir -p "$fake_go_bin" "$fake_gopath/bin"
printf '#!/usr/bin/env bash\nif [ "$1" = env ] && [ "$2" = GOPATH ]; then\n  printf "%s\\n" "${FAKE_GOPATH:?}"\n  exit 0\nfi\nexit 2\n' >"$fake_go_bin/go"
printf '#!/usr/bin/env bash\nprintf "v1.7.12\\n"\n' >"$fake_gopath/bin/actionlint"
chmod +x "$fake_go_bin/go" "$fake_gopath/bin/actionlint"

resolved_actionlint="$(PATH="$fake_go_bin:/usr/bin:/bin" FAKE_GOPATH="$fake_gopath" find_actionlint_bin)"
if [ "$resolved_actionlint" != "$fake_gopath/bin/actionlint" ]; then
  printf 'FAIL  Go-installed actionlint must resolve when GOPATH/bin is absent from PATH\n' >&2
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
