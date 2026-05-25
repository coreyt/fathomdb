#!/usr/bin/env bash
# Run the AC-012 perf gate (text-query latency).
# Env knobs:
#   AC012_CORPUS_N  — corpus row count (default 100000 for dev; 1000000 for canonical)
#   AC_FULL_SCALE   — set to "1" to honor ADR canonical scale
#   AGENT_LONG      — must be set to "1" or the test early-returns
#   LOG_PATH        — log file to tee output to (default ./ac012.log)
# Output: writes the raw harness log to $LOG_PATH; stdout includes the
# AC012_NUMBERS line emitted by the harness on stderr.
set -euo pipefail

LOG_PATH="${LOG_PATH:-./ac012.log}"
AGENT_LONG="${AGENT_LONG:-1}"

export AGENT_LONG
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

# Build only the perf_gates test binary in release.
cargo test --release --no-run -p fathomdb-engine --test perf_gates >/dev/null

# Run AC-012 only.
set +e
cargo test --release -p fathomdb-engine --test perf_gates -- \
  --nocapture --test-threads=1 ac_012 \
  2>&1 | tee "$LOG_PATH"
status=${PIPESTATUS[0]}
set -e

# Extract the AC012_NUMBERS line (emitted via eprintln! in the harness).
grep -E '^AC012_NUMBERS ' "$LOG_PATH" || true

exit "$status"
