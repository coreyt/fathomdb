#!/usr/bin/env bash
# Capture host spec for perf-experiment provenance. Emits a single-line
# JSON object on stdout suitable for embedding in the experiment's
# closure JSON. Robust to any individual command failing (substitutes
# "unknown" without leaking the failure command's stderr / partial output).
set -uo pipefail

safe() {
  # Run "$@" piped through "$_FILTER"; return single-line trimmed output
  # or "unknown" on any failure / empty result. _FILTER must be set by the
  # caller in subshell scope; we exec it via bash -c so set -e doesn't
  # propagate.
  local out
  out="$("$@" 2>/dev/null | head -n 1 | tr -d '\n\r' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')" || true
  if [ -z "$out" ]; then
    printf 'unknown'
  else
    printf '%s' "$out"
  fi
}

cpu_model="$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | sed 's/.*: //' | tr -d '\n\r' || true)"
[ -z "$cpu_model" ] && cpu_model="unknown"

cores="$(nproc 2>/dev/null | tr -d '\n\r' || true)"
[ -z "$cores" ] && cores=0

os="$(. /etc/os-release 2>/dev/null && printf '%s' "$PRETTY_NAME" || printf 'unknown')"

kernel="$(uname -srm 2>/dev/null | tr -d '\n\r' || true)"
[ -z "$kernel" ] && kernel="unknown"

glibc_raw="$(ldd --version 2>/dev/null | head -n 1 | tr -d '\n\r' || true)"
glibc="$(printf '%s' "$glibc_raw" | sed 's/.*) //')"
[ -z "$glibc" ] && glibc="unknown"

rustc_raw="$(rustc --version 2>/dev/null | head -n 1 | tr -d '\n\r' || true)"
[ -z "$rustc_raw" ] && rustc_raw="unknown"

sqlite="$(grep -A1 'name = "libsqlite3-sys"' Cargo.lock 2>/dev/null | grep version | head -n 1 | sed 's/.*"\(.*\)".*/libsqlite3-sys-\1/' | tr -d '\n\r' || true)"
[ -z "$sqlite" ] && sqlite="unknown"

# Use python to JSON-escape values robustly (handles any embedded special chars).
python3 - "$cpu_model" "$cores" "$os" "$kernel" "$glibc" "$rustc_raw" "$sqlite" <<'PYEOF'
import json, sys
cpu, cores, os, kernel, glibc, rustc, sqlite = sys.argv[1:]
try:
    cores = int(cores)
except Exception:
    cores = 0
print(json.dumps({
    "cpu_model": cpu,
    "cores": cores,
    "os": os,
    "kernel": kernel,
    "glibc": glibc,
    "rustc": rustc,
    "libsqlite3_sys": sqlite,
}, separators=(",", ":")))
PYEOF
