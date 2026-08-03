#!/usr/bin/env bash
# Enforce the end-user Linux AArch64 release path: native build artifacts,
# matching npm package, ordered publication, and registry-install smokes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKFLOW="$REPO_ROOT/.github/workflows/release.yml"
PLATFORM_PACKAGE="$REPO_ROOT/src/ts/npm/linux-arm64-gnu/package.json"
PREFLIGHT="$REPO_ROOT/.github/workflows/aarch64-release-preflight.yml"

python3 - "$WORKFLOW" "$PLATFORM_PACKAGE" "$PREFLIGHT" <<'PY'
import json
from pathlib import Path
import sys

import yaml

workflow_path, platform_package_path, preflight_path = sys.argv[1:]
workflow = yaml.safe_load(open(workflow_path))
jobs = workflow["jobs"]


def fail(message):
    print(f"FAIL  {message}", file=sys.stderr)
    raise SystemExit(1)


def includes(job):
    return jobs[job]["strategy"]["matrix"]["include"]


def entry(job, target):
    for candidate in includes(job):
        if candidate.get("target") == target:
            return candidate
    fail(f"{job} must include {target}")


python_arm = entry("build-python", "aarch64-unknown-linux-gnu")
if python_arm != {
    "runner": "ubuntu-24.04-arm",
    "target": "aarch64-unknown-linux-gnu",
    "manylinux": "2_28",
}:
    fail(f"build-python AArch64 entry is wrong: {python_arm!r}")

napi_arm = entry("build-napi", "aarch64-unknown-linux-gnu")
if napi_arm != {
    "runner": "ubuntu-24.04-arm",
    "target": "aarch64-unknown-linux-gnu",
    "label": "linux-arm64-gnu",
}:
    fail(f"build-napi AArch64 entry is wrong: {napi_arm!r}")

with open(platform_package_path) as package_file:
    platform_package = json.load(package_file)
expected_package = {
    "name": "fathomdb-linux-arm64-gnu",
    "os": ["linux"],
    "cpu": ["arm64"],
    "libc": ["glibc"],
    "main": "fathomdb.linux-arm64-gnu.node",
    "files": ["fathomdb.linux-arm64-gnu.node"],
}
for key, value in expected_package.items():
    if platform_package.get(key) != value:
        fail(f"linux-arm64-gnu package {key} must be {value!r}, got {platform_package.get(key)!r}")

publish_arm = jobs.get("publish-npm-platform-linux-arm64-gnu")
if publish_arm is None:
    fail("release workflow must publish the linux-arm64-gnu platform package")
if publish_arm.get("runs-on") != "ubuntu-24.04-arm":
    fail("linux-arm64-gnu publish job must run on native ARM64")
if publish_arm.get("needs") != "all-builds-passed":
    fail("linux-arm64-gnu publish job must wait for all builds")
publish_text = json.dumps(publish_arm)
if "napi-linux-arm64-gnu" not in publish_text or "src/ts/npm/linux-arm64-gnu" not in publish_text:
    fail("linux-arm64-gnu publish job must stage its matching artifact and package")

main_needs = jobs["publish-npm"].get("needs", [])
if not {"publish-npm-platform-linux-x64-gnu", "publish-npm-platform-linux-arm64-gnu"} <= set(main_needs):
    fail("main npm publish must wait for both Linux platform packages")

smoke_arm = jobs.get("post-publish-smoke-aarch64")
if smoke_arm is None:
    fail("release workflow must smoke published AArch64 Python and npm artifacts")
if smoke_arm.get("runs-on") != "ubuntu-24.04-arm":
    fail("AArch64 post-publish smoke must run on native ARM64")
smoke_text = json.dumps(smoke_arm)
if "smoke-pypi-wheel.sh" not in smoke_text or "smoke-npm-package.sh" not in smoke_text:
    fail("AArch64 post-publish smoke must exercise both registry bindings")

release_needs = jobs["github-release"].get("needs", [])
if "post-publish-smoke-aarch64" not in release_needs:
    fail("GitHub release must wait for AArch64 registry smokes")

if not Path(preflight_path).is_file():
    fail("AArch64 release preflight workflow is missing")
preflight = yaml.safe_load(open(preflight_path))
if set(preflight.get(True, {})) != {"workflow_dispatch"}:
    fail("AArch64 release preflight must be workflow_dispatch-only")
preflight_jobs = preflight.get("jobs", {})
native_preflight = preflight_jobs.get("native-aarch64-artifacts")
if native_preflight is None:
    fail("AArch64 release preflight must define native-aarch64-artifacts")
if native_preflight.get("runs-on") != "ubuntu-24.04-arm":
    fail("AArch64 release preflight must run on native ARM64")
preflight_text = json.dumps(native_preflight)
for required in [
    "PyO3/maturin-action@",
    "aarch64-unknown-linux-gnu",
    "manylinux",
    "npm ci",
    "npm run build:native",
    "fathomdb.linux-arm64-gnu.node",
    "src/ts/npm/linux-arm64-gnu",
    "npm pack --dry-run",
]:
    if required not in preflight_text:
        fail(f"AArch64 release preflight must exercise {required!r}")
if "publish" in preflight_text.lower() or "npm-publish-if-new" in preflight_text:
    fail("AArch64 release preflight must not contain a publishing step")

print("PASS  Linux AArch64 release artifacts are built, published, and smoke-tested natively")
PY

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cp "$REPO_ROOT/src/ts/package.json" "$scratch/package.json"
bash "$REPO_ROOT/scripts/release/npm-inject-optional-deps.sh" "$scratch" "$REPO_ROOT/src/ts/npm" >/dev/null
injected="$(node -e 'process.stdout.write(JSON.stringify(require(process.argv[1]).optionalDependencies))' "$scratch/package.json")"
expected='{"fathomdb-linux-arm64-gnu":"0.8.20","fathomdb-linux-x64-gnu":"0.8.20"}'
if [ "$injected" != "$expected" ]; then
  printf 'FAIL  main npm package must inject both Linux platform dependencies, got: %s\n' "$injected" >&2
  exit 1
fi
printf 'PASS  main npm package injects both Linux platform dependencies\n'
