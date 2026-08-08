#!/usr/bin/env bash
# Regression guard for Slice 15's pre-publish, local native-artifact runtime gate.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CI_YML="${CI_YML:-$REPO_ROOT/.github/workflows/ci.yml}"
PS1_HELPER="${PS1_HELPER:-$REPO_ROOT/scripts/release/smoke/smoke-local-native-artifacts.ps1}"

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

step_run_command() {
  awk '
    /^[[:space:]]*run:[[:space:]]*/ {
      command = $0
      sub(/^[[:space:]]*run:[[:space:]]*/, "", command)
      if (command ~ /^[|>][-+]?([[:space:]]*(#.*)?)?$/) {
        in_run = 1
        next
      }
      print command
      exit
    }
    in_run {
      if ($0 == "") {
        print
        next
      }
      if ($0 ~ /^          /) {
        command = $0
        sub(/^          /, "", command)
        print command
        next
      }
      exit
    }
  ' <<<"$1"
}

run_contains_invocation() {
  local command="$1"
  local required="$2"
  local continuation="$3"
  awk -v required="$required" -v continuation="$continuation" '
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    function start_bash_heredoc(value, remainder, quote, closing) {
      if (index(value, "<<") == 0) {
        return 0
      }
      remainder = value
      if (remainder !~ /<<-?[[:space:]]*/) {
        return -1
      }
      heredoc_strip_tabs = remainder ~ /<<-[[:space:]]*/
      sub(/^.*<<-?[[:space:]]*/, "", remainder)
      quote = substr(remainder, 1, 1)
      if (quote == sprintf("%c", 39) || quote == "\"") {
        closing = index(substr(remainder, 2), quote)
        if (closing == 0) {
          return -1
        }
        heredoc_delimiter = substr(remainder, 2, closing - 1)
        remainder = substr(remainder, closing + 2)
      } else if (match(remainder, /^[A-Za-z_][A-Za-z0-9_]*/)) {
        heredoc_delimiter = substr(remainder, RSTART, RLENGTH)
        remainder = substr(remainder, RLENGTH + 1)
      } else {
        return -1
      }
      if (heredoc_delimiter !~ /^[A-Za-z_][A-Za-z0-9_]*$/ || remainder !~ /^[[:space:]]*(#.*)?$/) {
        return -1
      }
      return 1
    }
    function start_powershell_here_string(value, single_quote) {
      single_quote = sprintf("%c", 39)
      if (value == "@" single_quote) {
        here_string_end = single_quote "@"
        return 1
      }
      if (value == "@\"") {
        here_string_end = "\"@"
        return 1
      }
      if (index(value, "@" single_quote) != 0 || index(value, "@\"") != 0) {
        return -1
      }
      return 0
    }
    function check_invocation(value, suffix) {
      value = trim(value)
      gsub(/[[:space:]]+/, " ", value)
      if (substr(value, 1, length(required)) != required) {
        return
      }
      suffix = substr(value, length(required) + 1)
      if (suffix == "" || suffix ~ /^ (#[[:space:]]*|&&[[:space:]]+|\|\|[[:space:]]+|;[[:space:]]*)/) {
        found = 1
      }
    }
    {
      raw = $0
      if (heredoc_delimiter != "") {
        candidate = raw
        if (heredoc_strip_tabs) {
          sub(/^\t*/, "", candidate)
        }
        if (candidate == heredoc_delimiter) {
          heredoc_delimiter = ""
          heredoc_strip_tabs = 0
        }
        next
      }
      if (here_string_end != "") {
        if (raw == here_string_end) {
          here_string_end = ""
        }
        next
      }
      line = raw
      sub(/^[[:space:]]+/, "", line)
      if (line == "" || line ~ /^#/) {
        next
      }
      literal = start_bash_heredoc(line)
      if (literal < 0 || (literal > 0 && pending != "")) {
        invalid = 1
        next
      }
      if (literal > 0) {
        next
      }
      literal = start_powershell_here_string(line)
      if (literal < 0 || (literal > 0 && pending != "")) {
        invalid = 1
        next
      }
      if (literal > 0) {
        next
      }
      continues = 0
      if (continuation == "backslash" && line ~ /\\$/) {
        sub(/\\$/, "", line)
        continues = 1
      } else if (continuation == "backtick" && line ~ /`$/) {
        sub(/`$/, "", line)
        continues = 1
      }
      line = trim(line)
      pending = pending (pending == "" ? "" : " ") line
      if (!continues) {
        check_invocation(pending)
        pending = ""
      }
    }
    END {
      if (heredoc_delimiter != "" || here_string_end != "") {
        invalid = 1
      }
      if (pending != "") {
        check_invocation(pending)
      }
      exit !(found && !invalid)
    }
  ' <<<"$command"
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

unix_validation_step="$(named_step 'Validate local wheel and N-API package')" \
  || fail 'missing Unix local-artifact validation step'
grep -Fqx "        if: matrix.runner != 'windows-latest'" <<<"$unix_validation_step" \
  || fail 'Unix local-artifact validation must exclude the Windows runner'
grep -Fqx '        shell: bash' <<<"$unix_validation_step" \
  || fail 'Unix local-artifact validation must use bash'
unix_validation_command="$(step_run_command "$unix_validation_step")"
[ -n "$unix_validation_command" ] \
  || fail 'Unix local-artifact validation must define an executable run command'
run_contains_invocation \
  "$unix_validation_command" \
  "bash scripts/release/smoke/smoke-local-native-artifacts.sh \"\$PWD/src/python/dist\" \"\$PWD/src/ts\" \"\$PWD/src/ts/npm/\${{ matrix.label }}\" \"\${{ matrix.label }}\"" \
  backslash \
  || fail 'Unix local-artifact validation must pass the wheel, TypeScript, platform-package, and N-API label arguments'

windows_validation_step="$(named_step 'Validate local wheel and N-API package (Windows)')" \
  || fail 'missing Windows local-artifact validation step'
grep -Fqx "        if: matrix.runner == 'windows-latest'" <<<"$windows_validation_step" \
  || fail 'Windows local-artifact validation must select only the Windows runner'
grep -Fqx '        shell: pwsh' <<<"$windows_validation_step" \
  || fail 'Windows local-artifact validation must use PowerShell'
windows_validation_command="$(step_run_command "$windows_validation_step")"
[ -n "$windows_validation_command" ] \
  || fail 'Windows local-artifact validation must define an executable run command'
run_contains_invocation \
  "$windows_validation_command" \
  "./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"\$PWD/src/python/dist\" -TsDirectory \"\$PWD/src/ts\" -PlatformPackageDirectory \"\$PWD/src/ts/npm/\${{ matrix.label }}\" -NapiLabel \"\${{ matrix.label }}\"" \
  backtick \
  || fail 'Windows local-artifact validation must pass the wheel, TypeScript, platform-package, and N-API label arguments'

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

require_ps1_exit_check() {
  local invocation="$1"
  awk -v invocation="$invocation" '
    $0 == invocation {
      found = 1
      if (getline && $0 ~ /^[[:space:]]*if \(\$LASTEXITCODE -ne 0\)[[:space:]]*\{[[:space:]]*throw[[:space:]]+[^}]+[[:space:]]*\}[[:space:]]*$/) {
        valid = 1
      }
      exit
    }
    END { exit !(found && valid) }
  ' "$PS1_HELPER"
}

require_ps1_exit_check \
  "  & \$python -m pip install --no-index --find-links \$WheelDirectory fathomdb" \
  || fail 'PowerShell wheel install must immediately propagate its Python exit code'
require_ps1_exit_check \
  "'@ | & \$python - (Join-Path \$work 'python-smoke.fdb')" \
  || fail 'PowerShell Python smoke must immediately propagate its Python exit code'

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
  ps1_fixture="$(mktemp)"
  trap 'rm -f "$fixture" "$ps1_fixture"' EXIT
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

  sed 's#"\$PWD/src/ts/npm/${{ matrix.label }}"#"\$PWD/src/ts/npm/not-the-matrix-label"#g' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a wrong platform-package path'
  fi

  sed 's/"${{ matrix.label }}"/"wrong-napi-label"/g' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a wrong N-API label'
  fi

  sed \
    -e 's#"\$PWD/src/ts/npm/${{ matrix.label }}"#"\$PWD/src/ts/npm/not-the-matrix-label"#g' \
    -e 's/-NapiLabel "${{ matrix.label }}"/-NapiLabel "wrong-napi-label"/g' \
    "$CI_YML" \
    | awk '
        $0 == "      - name: Validate local wheel and N-API package" { unix_step = 1 }
        unix_step && $0 == "        run: |" {
          print
          print "          # bash scripts/release/smoke/smoke-local-native-artifacts.sh \"$PWD/src/python/dist\" \"$PWD/src/ts\" \"$PWD/src/ts/npm/${{ matrix.label }}\" \"${{ matrix.label }}\""
          unix_step = 0
          next
        }
        $0 == "      - name: Validate local wheel and N-API package (Windows)" {
          print
          print "        # ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\""
          next
        }
        { print }
      ' > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted canonical invocations present only in comments'
  fi

  awk '
    $0 == "      - name: Validate local wheel and N-API package" { unix_step = 1 }
    unix_step && $0 == "        run: |" {
      print
      print "          cat <<'\''EOF'\''"
      print "          bash scripts/release/smoke/smoke-local-native-artifacts.sh \"$PWD/src/python/dist\" \"$PWD/src/ts\" \"$PWD/src/ts/npm/${{ matrix.label }}\" \"${{ matrix.label }}\""
      print "          EOF"
      print "          bash scripts/release/smoke/smoke-local-native-artifacts.sh \"$PWD/src/python/dist\" \"$PWD/src/ts\" \"$PWD/src/ts/npm/not-the-matrix-label\" \"wrong-napi-label\""
      replacing = 1
      next
    }
    replacing && /^      - name: / { replacing = 0; unix_step = 0; replaced = 1 }
    replacing { next }
    { print }
    END { exit !replaced }
  ' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a canonical Bash invocation inside a heredoc'
  fi

  awk '
    $0 == "        run: ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\"" {
      print "        run: |"
      print "          @'\''"
      print "          ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\""
      print "          '\''@"
      print "          ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/not-the-matrix-label\" -NapiLabel \"wrong-napi-label\""
      replaced = 1
      next
    }
    { print }
    END { exit !replaced }
  ' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a canonical PowerShell invocation inside a here-string'
  fi

  awk '
    $0 == "        run: ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\"" {
      print "        run: |"
      print "          Write-Output '\''./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\"'\''"
      print "          ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/not-the-matrix-label\" -NapiLabel \"wrong-napi-label\""
      replaced = 1
      next
    }
    { print }
    END { exit !replaced }
  ' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a PowerShell string spoof'
  fi

  awk '
    $0 == "        run: ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\"" {
      print "        run: |"
      print "          ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" `"
      print "            -TsDirectory \"$PWD/src/ts\" `"
      print "            -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" `"
      print "            -NapiLabel \"${{ matrix.label }}\""
      replaced = 1
      next
    }
    { print }
    END { exit !replaced }
  ' "$CI_YML" > "$fixture"
  if ! NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard rejected a PowerShell backtick continuation'
  fi

  awk '
    $0 == "      - name: Validate local wheel and N-API package" { unix_step = 1 }
    unix_step && $0 == "        run: |" {
      print
      print "          bash scripts/release/smoke/smoke-local-native-artifacts.sh \\ "
      print "            \"$PWD/src/python/dist\" \\"
      print "            \"$PWD/src/ts\" \\"
      print "            \"$PWD/src/ts/npm/${{ matrix.label }}\" \\"
      print "            \"${{ matrix.label }}\""
      replacing = 1
      next
    }
    replacing && /^      - name: / { replacing = 0; unix_step = 0; replaced = 1 }
    replacing { next }
    { print }
    END { exit !replaced }
  ' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a Bash continuation with trailing whitespace'
  fi

  awk '
    $0 == "        run: ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" -TsDirectory \"$PWD/src/ts\" -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" -NapiLabel \"${{ matrix.label }}\"" {
      print "        run: |"
      print "          ./scripts/release/smoke/smoke-local-native-artifacts.ps1 -WheelDirectory \"$PWD/src/python/dist\" ` "
      print "            -TsDirectory \"$PWD/src/ts\" `"
      print "            -PlatformPackageDirectory \"$PWD/src/ts/npm/${{ matrix.label }}\" `"
      print "            -NapiLabel \"${{ matrix.label }}\""
      replaced = 1
      next
    }
    { print }
    END { exit !replaced }
  ' "$CI_YML" > "$fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 CI_YML="$fixture" bash "$0" >/dev/null 2>&1; then
    fail 'native validation guard accepted a PowerShell continuation with trailing whitespace'
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

  awk '
    $0 == "'"'"'@ | & $python - (Join-Path $work '\''python-smoke.fdb'\'')" { after_python_smoke = 1 }
    after_python_smoke && /\$LASTEXITCODE -ne 0/ && !removed { removed = 1; next }
    { print }
    END { exit !removed }
  ' "$PS1_HELPER" > "$ps1_fixture"
  if NATIVE_RUNTIME_VALIDATION_FIXTURE=1 PS1_HELPER="$ps1_fixture" bash "$0" >/dev/null 2>&1; then
    fail 'PowerShell exit-code control accepted removal of the Python smoke guard'
  fi
fi

printf 'PASS test-native-artifact-runtime-validation\n'
