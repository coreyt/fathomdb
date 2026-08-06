#!/usr/bin/env bash
# The 0.8.22 SQLite upgrade is coupled: every direct rusqlite consumer uses
# 0.40 and both vector paths retain the sqlite-vec 0.1.9 TC-76 tripwire.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
for manifest in \
  src/rust/crates/fathomdb-cli/Cargo.toml \
  src/rust/crates/fathomdb-engine/Cargo.toml \
  src/rust/crates/fathomdb-schema/Cargo.toml; do
  if ! grep -Fq 'rusqlite = { version = "0.40", features = ["bundled", "fallible_uint"] }' "$root/$manifest"; then
    printf 'FAIL  %s must use bundled rusqlite 0.40 with fallible unsigned SQLite bindings\n' "$manifest" >&2
    exit 1
  fi
done
for manifest in \
  src/rust/crates/fathomdb-engine/Cargo.toml \
  src/rust/crates/fathomdb-schema/Cargo.toml; do
  if ! grep -Fq 'sqlite-vec = "=0.1.9"' "$root/$manifest"; then
    printf 'FAIL  %s must pin sqlite-vec 0.1.9\n' "$manifest" >&2
    exit 1
  fi
done

tc76="$root/src/rust/crates/fathomdb-engine/tests/tc76_vec0_long_metadata_delete.rs"
if ! rg -q 'fn bare_vec0_delete_pins_the_upstream_length_boundary' "$tc76" \
  || ! rg -q 'sqlite-vec 0\.1\.9 must delete a' "$tc76" \
  || ! rg -q 'assert_eq!\(residue, 0' "$tc76"; then
  printf 'FAIL  TC-76 must prove sqlite-vec 0.1.9 deletes long vec0 metadata without residue\n' >&2
  exit 1
fi
printf 'PASS  0.8.22 sqlite dependency contract and long-metadata TC-76 delete tripwire\n'
