#!/usr/bin/env bash
# Consume locally built Python and N-API artifacts without contacting a registry.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  printf 'usage: %s <wheel-dir> <ts-dir> <platform-package-dir> <napi-label>\n' "$0" >&2
  exit 2
fi

WHEEL_DIR="$1"
TS_DIR="$2"
PLATFORM_PACKAGE_DIR="$3"
NAPI_LABEL="$4"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

wheel_paths=("$WHEEL_DIR"/*.whl)
if [ "${#wheel_paths[@]}" -ne 1 ] || [ ! -f "${wheel_paths[0]}" ]; then
  printf 'smoke-local-native-artifacts: expected exactly one wheel in %s\n' "$WHEEL_DIR" >&2
  exit 1
fi
if [ ! -f "$TS_DIR/fathomdb.$NAPI_LABEL.node" ]; then
  printf 'smoke-local-native-artifacts: missing native N-API artifact %s\n' \
    "$TS_DIR/fathomdb.$NAPI_LABEL.node" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

python3 -m venv "$WORK/python-venv"
PYTHON="$WORK/python-venv/bin/python"
"$PYTHON" -m pip install --no-index --find-links "$WHEEL_DIR" fathomdb
"$PYTHON" - "$WORK/python-smoke.fdb" <<'PY'
import sys

from fathomdb import Engine

engine = Engine.open(sys.argv[1])
engine.write([
    {
        "kind": "doc",
        "body": "local native wheel runtime validation",
        "source_id": "smoke:local-native-wheel",
    }
])
engine.search("runtime validation")
engine.close()
print("local Python wheel runtime validation: ok")
PY

MAIN="$WORK/main"
NPM_ROOT="$WORK/npm"
PLATFORM="$NPM_ROOT/$NAPI_LABEL"
CONSUMER="$WORK/consumer"
mkdir -p "$MAIN" "$PLATFORM" "$CONSUMER"
cp "$TS_DIR/package.json" "$TS_DIR/LICENSE" "$MAIN/"
cp -R "$TS_DIR/dist" "$MAIN/dist"
cp "$PLATFORM_PACKAGE_DIR/package.json" "$PLATFORM_PACKAGE_DIR/LICENSE" "$PLATFORM/"
cp "$TS_DIR/fathomdb.$NAPI_LABEL.node" "$PLATFORM/fathomdb.$NAPI_LABEL.node"

# This is the same publish-time injection used by the release workflow. The
# local fixture contains only its matched platform package, so npm never needs
# a registry to resolve unrelated native packages.
bash "$REPO_ROOT/scripts/release/npm-inject-optional-deps.sh" "$MAIN" "$NPM_ROOT"

platform_name="$(node -p "require(process.argv[1]).name" "$PLATFORM/package.json")"
main_version="$(node -p "require(process.argv[1]).version" "$MAIN/package.json")"
injected="$(node -p "require(process.argv[1]).optionalDependencies[process.argv[2]] || ''" \
  "$MAIN/package.json" "$platform_name")"
if [ "$injected" != "$main_version" ]; then
  printf 'smoke-local-native-artifacts: %s optionalDependency is %s, expected %s\n' \
    "$platform_name" "${injected:-<missing>}" "$main_version" >&2
  exit 1
fi

platform_tarball="$(cd "$PLATFORM" && npm pack --silent)"
main_tarball="$(cd "$MAIN" && npm pack --silent)"
cat > "$CONSUMER/package.json" <<EOF
{
  "private": true,
  "type": "module",
  "dependencies": {
    "fathomdb": "file:$MAIN/$main_tarball",
    "$platform_name": "file:$PLATFORM/$platform_tarball"
  }
}
EOF

(
  cd "$CONSUMER"
  npm install --offline --ignore-scripts
  node --input-type=module - "$WORK/npm-smoke.fdb" <<'JS'
import { Engine } from "fathomdb";

const engine = await Engine.open(process.argv[2]);
await engine.write([{
  kind: "doc",
  body: "local native npm runtime validation",
  sourceId: "smoke:local-native-npm",
}]);
await engine.search("runtime validation");
await engine.close();
console.log("local N-API package runtime validation: ok");
JS
)

printf 'smoke-local-native-artifacts: ok — local wheel + matched N-API package validated\n'
