#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo check --workspace
cargo test --workspace
python3 -m compileall python/fathomdb python/tests
