#!/usr/bin/env bash
# Regression fixtures for the release-ready native-artifact contract checker.
# The checker deliberately reads only its manifest, workflow, and npm package
# metadata; current public documentation is checked separately after publish.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECKER="${CHECKER_UNDER_TEST:-$REPO_ROOT/scripts/check-release-contract-truth.py}"

TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT

make_fixture() {
  local root="$1"
  mkdir -p "$root/.github/workflows" "$root/dev" "$root/src/ts/npm"
  cp "$REPO_ROOT/dev/platform-capabilities.json" "$root/dev/"
  cp "$REPO_ROOT/.github/workflows/release.yml" "$root/.github/workflows/"
  cp -R "$REPO_ROOT/src/ts/npm/." "$root/src/ts/npm/"
}

expect_pass() {
  local root="$1" description="$2"
  if REPO_ROOT="$root" python3 "$CHECKER" >/dev/null; then
    printf 'PASS  %s\n' "$description"
  else
    printf 'FAIL  %s\n' "$description" >&2
    exit 1
  fi
}

expect_fail() {
  local root="$1" description="$2"
  if REPO_ROOT="$root" python3 "$CHECKER" >/dev/null 2>&1; then
    printf 'FAIL  %s\n' "$description" >&2
    exit 1
  fi
  printf 'PASS  %s\n' "$description"
}

FIXTURE="$TMPROOT/fixture"
make_fixture "$FIXTURE"
expect_pass "$FIXTURE" 'baseline release-ready contract agrees'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = (
    "  github-release:\n"
    "    runs-on: ubuntu-latest\n"
    "    if: ${{ inputs.dry_run != true && !(github.event_name == 'workflow_dispatch' && "
    "inputs.recovery_skip_npm == true && inputs.release_version == '0.8.20') }}\n"
    "    needs:\n"
    "      - promote-npm-latest\n"
)
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains the github-release promotion dependency")
path.write_text(text.replace(needle, needle.replace("    needs:\n      - promote-npm-latest\n", "    needs: publish-npm\n")))
PY
expect_fail "$FIXTURE" 'rejects a GitHub Release that bypasses npm latest promotion'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "  post-publish-smoke:\n    runs-on: ubuntu-latest\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains the shared release-ready smoke job")
path.write_text(text.replace(needle, needle + "    continue-on-error: true\n", 1))
PY
expect_fail "$FIXTURE" 'rejects a release-ready smoke job that continues after failure'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "      - name: Smoke AArch64 Python wheel\n        run: bash scripts/release/smoke/smoke-pypi-wheel.sh \"${{ steps.ver.outputs.version }}\"\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains the AArch64 Python smoke step")
path.write_text(text.replace(needle, needle.replace("        run:", "        continue-on-error: true\n        run:"), 1))
PY
expect_fail "$FIXTURE" 'rejects a release-ready smoke step that continues after failure'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "  post-publish-smoke-darwin-x64:\n    runs-on: macos-15-intel\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains the Darwin x64 smoke job")
path.write_text(text.replace(needle, needle + "    continue-on-error: ${{ inputs.dry_run != true }}\n", 1))
PY
expect_fail "$FIXTURE" 'rejects a dynamic release-ready smoke continuation'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "  promote-npm-latest:\n    runs-on: ubuntu-latest\n    if: ${{ inputs.dry_run != true && !(github.event_name == 'workflow_dispatch' && inputs.recovery_skip_npm == true && inputs.release_version == '0.8.20') }}\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains the npm promotion guard")
path.write_text(text.replace(needle, needle.replace("inputs.dry_run != true", "always() && inputs.dry_run != true"), 1))
PY
expect_fail "$FIXTURE" 'rejects an npm promotion that bypasses failed smoke dependencies'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "  github-release:\n    runs-on: ubuntu-latest\n    if: ${{ inputs.dry_run != true && !(github.event_name == 'workflow_dispatch' && inputs.recovery_skip_npm == true && inputs.release_version == '0.8.20') }}\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains the GitHub Release guard")
path.write_text(text.replace(needle, needle.replace("inputs.dry_run != true", "!cancelled() && inputs.dry_run != true"), 1))
PY
expect_fail "$FIXTURE" 'rejects a GitHub Release success-bypass condition'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "    runs-on: ${{ matrix.runner }}\n"
if text.count(needle) != 2:
    raise SystemExit("test fixture no longer contains both matrix runner bindings")
path.write_text(text.replace(needle, "    runs-on: ubuntu-latest\n", 1))
PY
expect_fail "$FIXTURE" 'rejects a build matrix that ignores matrix.runner'

make_fixture "$FIXTURE"
sed -i '/target: aarch64-apple-darwin/d' "$FIXTURE/.github/workflows/release.yml"
expect_fail "$FIXTURE" 'rejects a missing native build target'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "          - runner: windows-latest\n            target: x86_64-pc-windows-msvc\n            label: win32-x64-msvc\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains exactly one Windows N-API row")
path.write_text(text.replace(needle, needle + needle))
PY
expect_fail "$FIXTURE" 'rejects a duplicate native build row'

make_fixture "$FIXTURE"
sed -i '0,/label: darwin-arm64/s//label: darwin-x64/' "$FIXTURE/.github/workflows/release.yml"
expect_fail "$FIXTURE" 'rejects a wrong N-API label'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = 'smoke-${{ matrix.smoke }}.sh'
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains exactly one shared smoke command")
path.write_text(text.replace(needle, "smoke-crates-cli.sh", 1))
PY
expect_fail "$FIXTURE" 'rejects a shared Linux smoke matrix that does not execute its selected smoke'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = '      - run: bash scripts/release/smoke/smoke-pypi-wheel.sh "${{ steps.ver.outputs.version }}"\n'
if text.count(needle) != 2:
    raise SystemExit("test fixture no longer contains both direct Unix wheel-smoke commands")
path.write_text(text.replace(needle, needle.replace("bash", "echo", 1), 1))
PY
expect_fail "$FIXTURE" 'rejects a direct platform smoke that merely mentions a wheel command'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "  post-publish-smoke-darwin-x64:\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains exactly one Darwin x64 smoke job")
path.write_text(text.replace(needle, "  smoke-darwin-x64-removed:\n", 1))
PY
expect_fail "$FIXTURE" 'rejects a missing actual-runner platform smoke'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "    needs: [publish-rust-t7-cli, publish-pypi, publish-npm]\n"
if text.count(needle) != 3:
    raise SystemExit("test fixture no longer contains three inline platform-smoke dependencies")
path.write_text(text.replace(needle, "    needs: [publish-rust-t7-cli, publish-pypi]\n", 1))
PY
expect_fail "$FIXTURE" 'rejects a platform smoke missing publish-npm'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = "      - post-publish-smoke-win32-x64\n"
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains exactly one Windows promotion need")
path.write_text(text.replace(needle, "", 1))
PY
expect_fail "$FIXTURE" 'rejects a missing platform promotion dependency'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = 'run: npm dist-tag add "fathomdb@${RELEASE_TAG#v}" latest'
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains exactly one main-package promotion command")
path.write_text(text.replace(needle, needle.replace("fathomdb@", "fathomdb-linux-x64-gnu@", 1)))
PY
expect_fail "$FIXTURE" 'rejects a latest promotion of a platform package'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/.github/workflows/release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = '        run: npm dist-tag add "fathomdb@${RELEASE_TAG#v}" latest\n'
if text.count(needle) != 1:
    raise SystemExit("test fixture no longer contains exactly one direct main-package promotion")
extra = (
    "      - name: Mutant platform promotion\n"
    "        run: |\n"
    '          npm dist-tag add "fathomdb-darwin-x64@${RELEASE_TAG#v}" latest\n'
)
path.write_text(text.replace(needle, needle + extra, 1))
PY
expect_fail "$FIXTURE" 'rejects a block-style extra platform-package promotion'

make_fixture "$FIXTURE"
python3 - "$FIXTURE/dev/platform-capabilities.json" <<'PY'
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
manifest = json.loads(path.read_text())
for platform in manifest["platforms"]:
    if platform["triple"] == "linux-x64-musl":
        platform["status"] = "release-ready"
        break
else:
    raise SystemExit("test fixture lacks linux-x64-musl")
path.write_text(json.dumps(manifest, indent=2) + "\n")
PY
expect_fail "$FIXTURE" 'rejects musl becoming release-ready'

printf '\nAll release-contract-truth tests passed\n'
