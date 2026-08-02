#!/usr/bin/env bash
# Run the Rust workspace with an explicit, reviewable concurrency mode.
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: test-rust-workspace.sh --serial|--parallel-report

  --serial           Run the release-gating Rust workspace suite serially.
  --parallel-report  Run the equivalent parallel diagnostic suite.
USAGE
}

if [ "$#" -ne 1 ]; then
  usage
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel)"
cd "$REPO_ROOT"

case "$1" in
  --serial)
    exec cargo test --workspace --quiet --no-fail-fast --jobs 1 -- --test-threads=1
    ;;
  --parallel-report)
    exec cargo test --workspace --quiet --no-fail-fast
    ;;
  *)
    usage
    exit 2
    ;;
esac
