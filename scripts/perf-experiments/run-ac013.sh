#!/usr/bin/env bash
# Run the AC-013 perf gate (vector retrieval latency).
# Env knobs:
#   AC013_CORPUS_N  — corpus row count (default 10000; 1000000 for canonical)
#   AC_FULL_SCALE   — set to "1" to honor ADR canonical scale
#   AGENT_LONG      — must be set to "1" or the test early-returns
#   LOG_PATH        — log file to tee output to (default ./ac013.log)
set -euo pipefail

LOG_PATH="${LOG_PATH:-./ac013.log}"
AGENT_LONG="${AGENT_LONG:-1}"

export AGENT_LONG
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

cargo test --release --no-run -p fathomdb-engine --test perf_gates >/dev/null

set +e
cargo test --release -p fathomdb-engine --test perf_gates -- \
  --nocapture --test-threads=1 ac_013 \
  2>&1 | tee "$LOG_PATH"
status=${PIPESTATUS[0]}
set -e

grep -E '^AC013_NUMBERS ' "$LOG_PATH" || true

exit "$status"
