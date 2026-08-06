#!/usr/bin/env bash
# Regression guard for Slice 15's pre-publish, local native-artifact runtime gate.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CI_YML="${CI_YML:-$REPO_ROOT/.github/workflows/ci.yml}"

fail() {
  printf 'FAIL test-native-artifact-runtime-validation: %s\n' "$1" >&2
  exit 1
}

job_block() {
  awk '
    $0 == "  native-artifact-runtime-validation:" { found = 1; in_job = 1 }
    in_job { print }
    in_job && /^  [[:alnum:]_-]+:$/ && $0 != "  native-artifact-runtime-validation:" { exit }
    END { exit !found }
  ' "$CI_YML"
}

named_step() {
  local name="$1"
  awk -v name="$name" '
    $0 == "      - name: " name { found = 1; in_step = 1 }
    in_step { print }
    in_step && /^      - (name:|uses:|run:)/ && $0 != "      - name: " name { exit }
    END { exit !found }
  ' <<<"$block"
}

block="$(job_block)" || fail 'missing native-artifact-runtime-validation job'
matrix_target="\${{ matrix.target }}"

grep -Fqx '    needs: changes' <<<"$block" \
  || fail 'runtime job must run after the non-docs detector'
grep -Fqx "    if: needs.changes.outputs.docs_only != 'true'" <<<"$block" \
  || fail 'runtime job must be always-on for every non-docs change'
grep -Fqx "    runs-on: \${{ matrix.runner }}" <<<"$block" \
  || fail 'runtime job must execute on every selected native runner'

rows="$(awk '
  /^          - runner: / { runner = $3; target = ""; label = ""; next }
  /^            target: / { target = $2; next }
  /^            label: / { label = $2; print runner "|" target "|" label; runner = ""; target = ""; label = "" }
' <<<"$block" | sort)"
expected="$(cat <<'EOF' | sort
macos-14|aarch64-apple-darwin|darwin-arm64
macos-15-intel|x86_64-apple-darwin|darwin-x64
ubuntu-24.04-arm|aarch64-unknown-linux-gnu|linux-arm64-gnu
ubuntu-latest|x86_64-unknown-linux-gnu|linux-x64-gnu
windows-latest|x86_64-pc-windows-msvc|win32-x64-msvc
EOF
)"
[ "$rows" = "$expected" ] || fail "runtime matrix must cover exactly the five release-ready native triples; got: ${rows:-<none>}"

for required in \
  'Build local Python wheel' \
  'Build local N-API artifact and TypeScript package' \
  'scripts/release/smoke/smoke-local-native-artifacts.sh' \
  'scripts/release/smoke/smoke-local-native-artifacts.ps1'; do
  grep -Fq "$required" <<<"$block" \
    || fail "runtime job must locally consume both artifacts via ${required}"
done

python_build_step="$(named_step 'Build local Python wheel')" \
  || fail 'missing local Python wheel build step'
grep -Fqx '        uses: PyO3/maturin-action@e83996d129638aa358a18fbd1dfb82f0b0fb5d3b # v1.51.0' \
  <<<"$python_build_step" \
  || fail 'local Python wheel must use the pinned maturin-action release builder'
grep -Fqx "          target: $matrix_target" <<<"$python_build_step" \
  || fail 'local Python wheel must build for matrix.target'
grep -Fqx '          args: --release --out dist --features pyo3/extension-module,default-embedder -i python3.11' \
  <<<"$python_build_step" \
  || fail 'local Python wheel must be a release extension-module/default-embedder artifact'

napi_build_step="$(named_step 'Build local N-API artifact and TypeScript package')" \
  || fail 'missing local N-API/TypeScript build step'
grep -Fqx '        working-directory: src/ts' <<<"$napi_build_step" \
  || fail 'local N-API artifact must build from src/ts'
grep -Fqx "          CARGO_BUILD_TARGET: $matrix_target" <<<"$napi_build_step" \
  || fail 'local N-API artifact must target matrix.target'
grep -Fqx '          npm run build:native' <<<"$napi_build_step" \
  || fail 'local N-API artifact must invoke npm run build:native'
grep -Fqx '          npm exec -- tsc -p tsconfig.build.json' <<<"$napi_build_step" \
  || fail 'local TypeScript package must invoke its tsc build'

for helper_and_command in \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.sh:-m pip install --no-index --find-links" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.sh:npm install --offline" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.sh:Engine.open" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.sh:engine.search" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.sh:await engine.search" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.ps1:python -m pip install --no-index" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.ps1:npm install --offline" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.ps1:Engine.open" \
  "$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.ps1:await engine.search"; do
  helper="${helper_and_command%%:*}"
  command="${helper_and_command#*:}"
  [ -f "$helper" ] || fail "missing local-artifact validation helper $helper"
  grep -Fq -- "$command" "$helper" \
    || fail "${helper##*/} must locally validate with ${command}"
done

for forbidden in \
  'smoke-pypi-wheel.sh' \
  'smoke-npm-package.sh' \
  'pip install --quiet "fathomdb==' \
  'npm install --silent "fathomdb@'; do
  if grep -Fq "$forbidden" <<<"$block"; then
    fail "pre-publish runtime job must not use registry smoke command ${forbidden}"
  fi
done

# Non-vacuous control: the exact-five assertion must reject a sixth matrix row,
# rather than merely finding the five required rows somewhere in the workflow.
if [ "${NATIVE_RUNTIME_VALIDATION_FIXTURE:-0}" != "1" ]; then
  fixture="$(mktemp)"
  trap 'rm -f "$fixture"' EXIT
  awk '
    $0 == "  native-artifact-runtime-validation:" { in_job = 1 }
    in_job && /^  [[:alnum:]_-]+:$/ && $0 != "  native-artifact-runtime-validation:" { in_job = 0 }
    in_job && $0 == "            label: linux-x64-gnu" && !inserted {
      print
      print "          - runner: ubuntu-latest"
      print "            target: x86_64-unknown-linux-musl"
      print "            label: linux-x64-musl"
      inserted = 1
      next
    }
    { print }
    END { exit !inserted }
  ' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'exact-five control accepted an unsupported sixth matrix row'
  fi

  sed '0,/smoke-local-native-artifacts\.sh/s//smoke-pypi-wheel.sh/' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'local-artifact command control accepted a registry smoke substitution'
  fi

  sed 's/default-embedder/default-embedder-removed/' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'wheel-build control accepted a missing default-embedder feature'
  fi

  sed 's/CARGO_BUILD_TARGET:/CARGO_BUILD_PLATFORM:/' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'N-API target control accepted a missing CARGO_BUILD_TARGET wiring'
  fi

  sed 's/npm run build:native$/npm run build:native:debug/' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'N-API build control accepted a non-release native build command'
  fi

  sed 's/npm exec -- tsc -p tsconfig.build.json/npm exec -- tsc -p tsconfig.json/' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'TypeScript build control accepted the wrong tsc configuration'
  fi
fi

printf 'PASS test-native-artifact-runtime-validation\n'
