#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

python3 scripts/check-version-consistency.py

cargo bench -p fathomdb --bench production_paths --features sqlite-vec -- --sample-size=10
