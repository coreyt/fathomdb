#!/usr/bin/env bash
# Behavioral contract for the canonical Rust-workspace test runner.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUNNER="$REPO_ROOT/scripts/test-rust-workspace.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

FAKE_BIN="$TMPROOT/bin"
mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: "${FAKE_CARGO_ARGS:?}"
printf '%s\n' "$@" >"$FAKE_CARGO_ARGS"
if [ -n "${FAKE_CARGO_ENV:-}" ]; then
  printf '%s\n' "${AGENT_LONG:-unset}" >"$FAKE_CARGO_ENV"
fi
exit "${FAKE_CARGO_RC:-0}"
EOF
chmod +x "$FAKE_BIN/cargo"

run_runner() {
  local label="$1"
  shift
  ARGS_FILE="$TMPROOT/$label.args"
  RUN_OUTPUT="$TMPROOT/$label.out"
  set +e
  PATH="$FAKE_BIN:$PATH" FAKE_CARGO_ARGS="$ARGS_FILE" "$RUNNER" "$@" >"$RUN_OUTPUT" 2>&1
  RUN_RC=$?
  set -e
}

assert_args() {
  local label="$1"
  shift
  local expected actual
  expected="$(printf '%s\n' "$@")"
  actual="$(cat "$ARGS_FILE" 2>/dev/null || true)"
  if [ "$actual" = "$expected" ]; then
    pass "$label uses the exact Cargo argument vector"
  else
    fail "$label arguments differed; expected [$expected], got [$actual]"
  fi
}

run_runner serial --serial
if [ "$RUN_RC" -eq 0 ]; then
  pass "--serial returns Cargo's zero exit unchanged"
else
  fail "--serial returned $RUN_RC, expected 0: $(cat "$RUN_OUTPUT")"
fi
assert_args --serial test --workspace --quiet --no-fail-fast --jobs 1 -- --test-threads=1

ARGS_FILE="$TMPROOT/serial-long.args"
ENV_FILE="$TMPROOT/serial-long.env"
RUN_OUTPUT="$TMPROOT/serial-long.out"
set +e
AGENT_LONG=1 PATH="$FAKE_BIN:$PATH" FAKE_CARGO_ARGS="$ARGS_FILE" FAKE_CARGO_ENV="$ENV_FILE" \
  "$RUNNER" --serial >"$RUN_OUTPUT" 2>&1
RUN_RC=$?
set -e
if [ "$RUN_RC" -eq 0 ] && [ "$(cat "$ENV_FILE")" = "1" ]; then
  pass "--serial preserves AGENT_LONG for callers that explicitly select the mode"
else
  fail "--serial must preserve AGENT_LONG without using it to select a mode"
fi

run_runner parallel --parallel-report
if [ "$RUN_RC" -eq 0 ]; then
  pass "--parallel-report returns Cargo's zero exit unchanged"
else
  fail "--parallel-report returned $RUN_RC, expected 0: $(cat "$RUN_OUTPUT")"
fi
assert_args --parallel-report test --workspace --quiet --no-fail-fast

for mode in --serial --parallel-report; do
  label="failure-${mode#--}"
  ARGS_FILE="$TMPROOT/$label.args"
  RUN_OUTPUT="$TMPROOT/$label.out"
  set +e
  PATH="$FAKE_BIN:$PATH" FAKE_CARGO_ARGS="$ARGS_FILE" FAKE_CARGO_RC=73 \
    "$RUNNER" "$mode" >"$RUN_OUTPUT" 2>&1
  RUN_RC=$?
  set -e
  if [ "$RUN_RC" -eq 73 ]; then
    pass "$mode returns Cargo's non-zero exit unchanged"
  else
    fail "$mode returned $RUN_RC, expected 73: $(cat "$RUN_OUTPUT")"
  fi
done

for invalid_case in missing repeated unknown; do
  label="invalid-$invalid_case"
  ARGS_FILE="$TMPROOT/$label.args"
  RUN_OUTPUT="$TMPROOT/$label.out"
  set +e
  case "$invalid_case" in
    missing)
      PATH="$FAKE_BIN:$PATH" FAKE_CARGO_ARGS="$ARGS_FILE" "$RUNNER" >"$RUN_OUTPUT" 2>&1
      ;;
    repeated)
      PATH="$FAKE_BIN:$PATH" FAKE_CARGO_ARGS="$ARGS_FILE" "$RUNNER" --serial --serial >"$RUN_OUTPUT" 2>&1
      ;;
    unknown)
      PATH="$FAKE_BIN:$PATH" FAKE_CARGO_ARGS="$ARGS_FILE" "$RUNNER" --unknown >"$RUN_OUTPUT" 2>&1
      ;;
  esac
  RUN_RC=$?
  set -e
  if [ "$RUN_RC" -eq 2 ] && [ ! -e "$ARGS_FILE" ]; then
    pass "$label exits 2 before Cargo executes"
  else
    fail "$label must exit 2 before Cargo executes (rc=$RUN_RC, args=$(cat "$ARGS_FILE" 2>/dev/null || true), out=$(cat "$RUN_OUTPUT"))"
  fi
done

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nall Rust-workspace runner tests passed\n'
