#!/usr/bin/env bash
# Driver for one perf-experiments cell. Delegates JSON work to
# emit-output-json.py to avoid bash<->python heredoc fragility.
#
# Required env:
#   EXP_ID         — experiment id (e.g. "W1.1")
#   LEVER_ID       — lever id from the taxonomy (e.g. "L-A0")
#
# Optional env:
#   TARGETS        — space-separated list: "ac012" "ac020" "ac013"
#                    (default: "ac012 ac020")
#   AC012_CORPUS_N — default 100000 dev; 1000000 canonical
#   AC013_CORPUS_N — default 10000
#   AC_FULL_SCALE  — set to "1" to honor ADR canonical scale
#   OUT_JSON       — output JSON path (default
#                    dev/plans/runs/0.7.0-PERF-EXP-${EXP_ID}-output.json)
#   RUN_LOCATION   — "dev-box" | "canonical-ci" (default "dev-box")
set -euo pipefail

: "${EXP_ID:?EXP_ID required}"
: "${LEVER_ID:?LEVER_ID required}"
TARGETS="${TARGETS:-ac012 ac020}"
RUN_LOCATION="${RUN_LOCATION:-dev-box}"
AC012_CORPUS_N="${AC012_CORPUS_N:-100000}"
AC013_CORPUS_N="${AC013_CORPUS_N:-10000}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

OUT_JSON_DEFAULT="dev/plans/runs/0.7.0-PERF-EXP-${EXP_ID}-output.json"
OUT_JSON="${OUT_JSON:-$OUT_JSON_DEFAULT}"
mkdir -p "$(dirname "$OUT_JSON")"

LOG_DIR="$(mktemp -d -t perf-exp-XXXXXX)"

echo "==> EXP_ID=$EXP_ID LEVER_ID=$LEVER_ID TARGETS=\"$TARGETS\" RUN_LOCATION=$RUN_LOCATION" >&2
echo "==> AC012_CORPUS_N=$AC012_CORPUS_N AC013_CORPUS_N=$AC013_CORPUS_N OUT_JSON=$OUT_JSON" >&2

HOST_JSON_PATH="$LOG_DIR/host.json"
bash "$SCRIPT_DIR/collect-host-spec.sh" > "$HOST_JSON_PATH"

# Run requested targets sequentially. Each target writes its own log.
LOG_PATHS=()
for target in $TARGETS; do
  case "$target" in
    ac012)
      LOG="$LOG_DIR/ac012.log"
      AC012_CORPUS_N="$AC012_CORPUS_N" \
      AC_FULL_SCALE="${AC_FULL_SCALE:-}" \
      LOG_PATH="$LOG" \
      AGENT_LONG=1 \
      bash "$SCRIPT_DIR/run-ac012.sh" >/dev/null 2>&1 || echo "warn: ac012 exited non-zero" >&2
      LOG_PATHS+=("$LOG")
      ;;
    ac020)
      LOG="$LOG_DIR/ac020.log"
      LOG_PATH="$LOG" \
      AGENT_LONG=1 \
      bash "$SCRIPT_DIR/run-ac020.sh" >/dev/null 2>&1 || echo "warn: ac020 exited non-zero" >&2
      LOG_PATHS+=("$LOG")
      ;;
    ac013)
      LOG="$LOG_DIR/ac013.log"
      AC013_CORPUS_N="$AC013_CORPUS_N" \
      AC_FULL_SCALE="${AC_FULL_SCALE:-}" \
      LOG_PATH="$LOG" \
      AGENT_LONG=1 \
      bash "$SCRIPT_DIR/run-ac013.sh" >/dev/null 2>&1 || echo "warn: ac013 exited non-zero" >&2
      LOG_PATHS+=("$LOG")
      ;;
    ac019)
      LOG="$LOG_DIR/ac019.log"
      AC013_CORPUS_N="$AC013_CORPUS_N" \
      AC_FULL_SCALE="${AC_FULL_SCALE:-}" \
      LOG_PATH="$LOG" \
      AGENT_LONG=1 \
      bash "$SCRIPT_DIR/run-ac019.sh" >/dev/null 2>&1 || echo "warn: ac019 exited non-zero" >&2
      LOG_PATHS+=("$LOG")
      ;;
    *)
      echo "unknown target: $target" >&2
      exit 2
      ;;
  esac
done

HEAD_SHA="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

python3 "$SCRIPT_DIR/emit-output-json.py" \
  --out "$OUT_JSON" \
  --exp-id "$EXP_ID" \
  --lever-id "$LEVER_ID" \
  --head-sha "$HEAD_SHA" \
  --branch "$BRANCH" \
  --timestamp "$TIMESTAMP" \
  --run-location "$RUN_LOCATION" \
  --host-json "$HOST_JSON_PATH" \
  --logs "${LOG_PATHS[@]:-}"

echo "==> wrote $OUT_JSON" >&2
echo "==> logs preserved at $LOG_DIR" >&2
