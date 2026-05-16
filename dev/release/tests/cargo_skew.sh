#!/usr/bin/env bash
# AC-051a: cargo version-skew detected at resolve time.
#
# Runs `cargo update` against the fixture in
# dev/release/fixtures/cargo-skew/, which pins
# `fathomdb-embedder-api = "=99.99.99"` against a vendored copy at
# "0.6.0". Asserts non-zero exit and that stderr names the crate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE="$SCRIPT_DIR/../fixtures/cargo-skew"

if ! command -v cargo >/dev/null 2>&1; then
  echo "skip: cargo not on PATH" >&2
  exit 0
fi

cd "$FIXTURE"

# Use CARGO_TARGET_DIR to avoid polluting the repo workspace target.
export CARGO_TARGET_DIR="$(mktemp -d)"
trap 'rm -rf "$CARGO_TARGET_DIR"' EXIT

# Offline so we never accidentally hit crates.io. The fixture is fully
# vendored; resolution should fail strictly on the version-pin mismatch.
if out="$(cargo update --offline 2>&1)"; then
  printf 'FAIL: cargo update unexpectedly succeeded; expected resolver error\n%s\n' "$out" >&2
  exit 1
fi

if ! printf '%s' "$out" | grep -q 'fathomdb-embedder-api'; then
  printf 'FAIL: cargo update error did not name fathomdb-embedder-api\n%s\n' "$out" >&2
  exit 1
fi

printf 'PASS: AC-051a — cargo resolver detected fathomdb-embedder-api skew\n'
