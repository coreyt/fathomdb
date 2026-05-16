#!/usr/bin/env bash
# AC-051a: cargo version-skew detected at resolve time.
#
# Runs `cargo update` against the fixture in
# dev/release/fixtures/cargo-skew/. The fixture wires sibling consumer
# crates (mock-skew-consumer-a, mock-skew-consumer-b) that pin
# incompatible exact versions of a shared dependency (mock-skew-api).
# The probe pulls in both consumers, so the resolver must reject:
# mock-skew-api cannot be both =1.0.0 and =2.0.0 simultaneously.
# Asserts non-zero exit and that stderr names the conflicting crate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE="$SCRIPT_DIR/../fixtures/cargo-skew"

if ! command -v cargo >/dev/null 2>&1; then
  echo "skip: cargo not on PATH" >&2
  exit 0
fi

cd "$FIXTURE"

export CARGO_TARGET_DIR="$(mktemp -d)"
trap 'rm -rf "$CARGO_TARGET_DIR"' EXIT

if out="$(cargo update --offline 2>&1)"; then
  printf 'FAIL: cargo update unexpectedly succeeded; expected resolver error\n%s\n' "$out" >&2
  exit 1
fi

if ! printf '%s' "$out" | grep -q 'mock-skew-api'; then
  printf 'FAIL: cargo update error did not name mock-skew-api\n%s\n' "$out" >&2
  exit 1
fi

printf 'PASS: AC-051a — cargo resolver detected mock-skew-api sibling skew\n'
