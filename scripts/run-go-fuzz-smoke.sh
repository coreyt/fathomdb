#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT/go/fathom-integrity"

go test ./internal/commands -run='^$' -fuzz=FuzzSanitizeRecoveredSQL_Idempotent -fuzztime=10s
go test ./internal/bridge -run='^$' -fuzz=FuzzDecodeResponse -fuzztime=10s
