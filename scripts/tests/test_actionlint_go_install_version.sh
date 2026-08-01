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

for script in scripts/bootstrap.sh scripts/agent-lint.sh; do
  if ! grep -Fq 'lib/actionlint-version.sh' "$REPO_ROOT/$script" \
    || ! grep -Fq 'read_actionlint_version' "$REPO_ROOT/$script"; then
    printf 'FAIL  %s must use the shared actionlint version normalizer\n' "$script" >&2
    exit 1
  fi
done

printf 'PASS  Go-installed v1.7.12 normalizes without weakening the exact pin\n'
