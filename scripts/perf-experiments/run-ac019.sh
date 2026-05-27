#!/usr/bin/env bash
# Run the AC-019 perf gate (concurrent-mixed retrieval stress tail).
# Shares AC-013's corpus + seeding cost — re-seeds inside the test
# itself.
# Env knobs:
#   AC013_CORPUS_N  — corpus row count (default 10000; 1000000 for canonical)
#   AC_FULL_SCALE   — set to "1" to honor ADR canonical scale
#   AGENT_LONG      — must be set to "1" or the test early-returns
#   LOG_PATH        — log file to tee output to (default ./ac019.log)
set -euo pipefail

LOG_PATH="${LOG_PATH:-./ac019.log}"
AGENT_LONG="${AGENT_LONG:-1}"

export AGENT_LONG
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

cargo test --release --no-run -p fathomdb-engine --test perf_gates >/dev/null

set +e
cargo test --release -p fathomdb-engine --test perf_gates -- \
  --nocapture --test-threads=1 ac_019 \
  2>&1 | tee "$LOG_PATH"
status=${PIPESTATUS[0]}
set -e

grep -E '^AC019_NUMBERS ' "$LOG_PATH" || true

exit "$status"
