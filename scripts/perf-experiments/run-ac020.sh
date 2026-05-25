#!/usr/bin/env bash
# Run the AC-020 perf gate (single-reader concurrency speedup).
# Env knobs:
#   AGENT_LONG  — must be set to "1" or the test early-returns
#   LOG_PATH    — log file to tee output to (default ./ac020.log)
# Output: writes the raw log to $LOG_PATH; stdout includes the
# AC020_NUMBERS line (sequential_ms / concurrent_ms / bound_ms).
set -euo pipefail

LOG_PATH="${LOG_PATH:-./ac020.log}"
AGENT_LONG="${AGENT_LONG:-1}"

export AGENT_LONG
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

cargo test --release --no-run -p fathomdb-engine --test perf_gates >/dev/null

set +e
cargo test --release -p fathomdb-engine --test perf_gates -- \
  --nocapture --test-threads=1 ac_020_reads_do_not_serialize \
  2>&1 | tee "$LOG_PATH"
status=${PIPESTATUS[0]}
set -e

grep -E '^AC020_NUMBERS ' "$LOG_PATH" || true

exit "$status"
