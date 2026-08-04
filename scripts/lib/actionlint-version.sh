#!/usr/bin/env bash
# actionlint release binaries report `1.7.12`; Go-installed binaries report
# `v1.7.12`. Normalize only that conventional leading marker before callers
# compare against their exact required numeric pin.

read_actionlint_version() {
  local actionlint_bin="$1"
  local raw
  # Bound first: `$bin --version | sed` would swallow a failing binary's exit
  # status behind sed's success.
  raw="$("$actionlint_bin" --version)"
  printf '%s\n' "$raw" | sed -n '1{s/^v//;p;}'
}

# Resolve a Go-installed actionlint when its GOPATH/bin directory has not been
# added to PATH yet. This is the default on GitHub-hosted runners: bootstrap
# and agent-verify run in separate steps, so relying on an in-process `export`
# would make the second step falsely report that the exact installed pin is
# missing.
find_actionlint_bin() {
  local actionlint_bin go_path

  actionlint_bin="$(command -v actionlint || true)"
  if [ -n "$actionlint_bin" ]; then
    printf '%s\n' "$actionlint_bin"
    return 0
  fi

  if ! command -v go >/dev/null 2>&1; then
    return 1
  fi
  go_path="$(go env GOPATH 2>/dev/null)" || return 1
  actionlint_bin="$go_path/bin/actionlint"
  if [ -x "$actionlint_bin" ]; then
    printf '%s\n' "$actionlint_bin"
    return 0
  fi

  return 1
}
