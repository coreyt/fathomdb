#!/usr/bin/env bash
# Ensure the default-embedder's Linux AArch64 CPU closure never selects Gemm's
# F16 backend, which emits unsupported `fullfp16` instructions on the Tier-1
# baseline. Other targets retain Candle's upstream F16 selection.
set -euo pipefail

if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "aarch64" ]; then
  printf 'SKIP  Linux AArch64-only Candle feature-closure check\n'
  exit 0
fi

set +e
output="$(RUSTUP_TOOLCHAIN=stable cargo tree -e features -p fathomdb-embedder \
  --locked --features default-embedder -i gemm-f16 2>&1)"
status=$?
set -e

if [ "$status" -ne 0 ]; then
  printf '%s\n' "$output" >&2
  printf '%s\n' 'FAIL  Cargo tree did not establish the Linux AArch64 Candle feature closure' >&2
  exit 1
fi

if printf '%s\n' "$output" | grep -q 'warning: nothing to print.'; then
  printf '%s\n' 'PASS  default-embedder does not resolve gemm-f16 on Linux AArch64'
  exit 0
fi

printf '%s\n' "$output" >&2
printf '%s\n' 'FAIL  default-embedder must not resolve gemm-f16 on Linux AArch64' >&2
exit 1
