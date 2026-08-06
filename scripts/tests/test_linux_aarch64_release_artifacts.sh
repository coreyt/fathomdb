#!/usr/bin/env bash
# Enforce the end-user Linux AArch64 release path: native build artifacts,
# matching npm package, ordered publication, and registry-install smokes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="$REPO_ROOT/.github/workflows/release.yml"
PLATFORM_PACKAGE="$REPO_ROOT/src/ts/npm/linux-arm64-gnu/package.json"
PREFLIGHT="$REPO_ROOT/.github/workflows/aarch64-release-preflight.yml"

if ! command -v actionlint >/dev/null 2>&1; then
  printf 'FAIL  actionlint is required to validate release workflows\n' >&2
  exit 1
fi
actionlint "$WORKFLOW" "$PREFLIGHT"

# Workflow syntax is actionlint's responsibility. The release-contract checker
# intentionally reads only known job blocks and JSON contract files, so it can
# enforce the exact five-target topology without accepting invalid Actions YAML.
env REPO_ROOT="$REPO_ROOT" python3 "$REPO_ROOT/scripts/check-release-contract-truth.py"

require_contains() {
  local subject="$1"
  local expected="$2"
  local message="$3"
  if [[ "$subject" != *"$expected"* ]]; then
    printf 'FAIL  %s\n' "$message" >&2
    exit 1
  fi
}

release_job() {
  local job="$1"
  awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^[:space:]#][^:]*:$/ { exit }
    found { print }
  ' "$WORKFLOW"
}

preflight_job() {
  awk '
    $0 == "  native-aarch64-artifacts:" { found = 1; next }
    found && /^  [^[:space:]#][^:]*:$/ { exit }
    found { print }
  ' "$PREFLIGHT"
}

if [ ! -f "$PREFLIGHT" ]; then
  printf 'FAIL  AArch64 release preflight workflow is missing\n' >&2
  exit 1
fi

build_python="$(release_job build-python)"
require_contains "$build_python" $'          - runner: ubuntu-24.04-arm\n            target: aarch64-unknown-linux-gnu\n            manylinux: "2_28"' \
  'build-python must include the native ARM64 manylinux 2_28 row'

build_napi="$(release_job build-napi)"
require_contains "$build_napi" $'          - runner: ubuntu-24.04-arm\n            target: aarch64-unknown-linux-gnu\n            label: linux-arm64-gnu' \
  'build-napi must include the exact native ARM64 platform row'

node - "$PLATFORM_PACKAGE" <<'NODE'
const fs = require("node:fs");
const path = process.argv[2];
const platformPackage = JSON.parse(fs.readFileSync(path, "utf8"));
const expected = {
  name: "fathomdb-linux-arm64-gnu",
  os: ["linux"],
  cpu: ["arm64"],
  libc: ["glibc"],
  main: "fathomdb.linux-arm64-gnu.node",
  files: ["fathomdb.linux-arm64-gnu.node"],
};
for (const [key, value] of Object.entries(expected)) {
  if (JSON.stringify(platformPackage[key]) !== JSON.stringify(value)) {
    console.error(`FAIL  linux-arm64-gnu package ${key} must be ${JSON.stringify(value)}, got ${JSON.stringify(platformPackage[key])}`);
    process.exit(1);
  }
}
NODE

preflight_trigger="$(awk '
  $0 == "on:" { found = 1; next }
  found && /^[^[:space:]#][^:]*:$/ { exit }
  found { print }
' "$PREFLIGHT")"
expected_trigger=$'  push:\n    paths:\n      - .github/workflows/aarch64-release-preflight.yml\n      - .github/workflows/release.yml\n      - src/python/**\n      - src/rust/**\n      - src/ts/**\n      - Cargo.toml\n      - Cargo.lock\n  workflow_dispatch:'
if [ "$preflight_trigger" != "$expected_trigger" ]; then
  printf 'FAIL  AArch64 release preflight triggers or watched paths drifted\n' >&2
  exit 1
fi

native_preflight="$(preflight_job)"
require_contains "$native_preflight" 'runs-on: ubuntu-24.04-arm' \
  'AArch64 release preflight must run on native ARM64'
for required in \
  'PyO3/maturin-action@' \
  'aarch64-unknown-linux-gnu' \
  'manylinux' \
  'npm ci' \
  'npm run build:native' \
  'fathomdb.linux-arm64-gnu.node' \
  'src/ts/npm/linux-arm64-gnu' \
  'npm pack --dry-run'; do
  require_contains "$native_preflight" "$required" \
    "AArch64 release preflight must exercise $required"
done
if [[ "${native_preflight,,}" == *publish* ]] || [[ "$native_preflight" == *npm-publish-if-new* ]]; then
  printf 'FAIL  AArch64 release preflight must not contain a publishing step\n' >&2
  exit 1
fi

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cp "$REPO_ROOT/src/ts/package.json" "$scratch/package.json"
bash "$REPO_ROOT/scripts/release/npm-inject-optional-deps.sh" "$scratch" "$REPO_ROOT/src/ts/npm" >/dev/null
injected="$(node -e 'process.stdout.write(JSON.stringify(require(process.argv[1]).optionalDependencies))' "$scratch/package.json")"
version="$(node -e 'process.stdout.write(require(process.argv[1]).version)' "$REPO_ROOT/src/ts/package.json")"
expected="{\"fathomdb-darwin-arm64\":\"$version\",\"fathomdb-darwin-x64\":\"$version\",\"fathomdb-linux-arm64-gnu\":\"$version\",\"fathomdb-linux-x64-gnu\":\"$version\",\"fathomdb-win32-x64-msvc\":\"$version\"}"
if [ "$injected" != "$expected" ]; then
  printf 'FAIL  main npm package must inject every stable platform dependency, got: %s\n' "$injected" >&2
  exit 1
fi

printf 'PASS  Linux AArch64 release artifacts are built, published, and smoke-tested natively\n'
