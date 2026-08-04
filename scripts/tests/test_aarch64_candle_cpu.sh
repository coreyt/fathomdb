#!/usr/bin/env bash
# Run the Candle CPU regression only where the Linux AArch64 dependency branch
# is selected. Other hosts retain Candle's upstream F16 behavior.
set -euo pipefail

host_os="$(uname -s)"
host_arch="$(uname -m)"
if [ "$host_os" != "Linux" ] || [ "$host_arch" != "aarch64" ]; then
  printf 'SKIP  Linux AArch64-only Candle CPU regression\n'
  exit 0
fi

RUSTUP_TOOLCHAIN=stable cargo test --locked -p fathomdb-embedder \
  --features default-embedder --test aarch64_candle_cpu
