#!/usr/bin/env bash
# actionlint release binaries report `1.7.12`; Go-installed binaries report
# `v1.7.12`. Normalize only that conventional leading marker before callers
# compare against their exact required numeric pin.

read_actionlint_version() {
  local actionlint_bin="$1"
  "$actionlint_bin" --version | sed -n '1{s/^v//;p;}'
}
