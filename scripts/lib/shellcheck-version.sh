#!/usr/bin/env bash
# Single source of truth for the shellcheck pin, shared by scripts/bootstrap.sh
# (installer), scripts/agent-lint.sh (preflight) and scripts/agent-lint-shell.sh
# (the lint legs themselves).
#
# Why pinned at all: shellcheck's finding set changes between releases — new
# checks appear, existing ones widen or narrow. An unpinned linter silently
# changes what "green" means underneath the repo. This is not hypothetical here:
# pyright is unpinned at >=1.1.380 and a point release red-lined `main` for ~2
# days. ruff (0.15.17) and actionlint (1.7.12) are already pinned and hard-fail
# on drift in agent-lint.sh; shellcheck follows the same discipline.
#
# `shellcheck --version` prints a multi-line banner; the numeric pin lives on
# the `version:` line.

# Deliberately NOT `readonly`: this file is sourced by several scripts and, in
# some of them, more than once via nested sourcing. `readonly` would abort the
# second source under `set -e`.
SHELLCHECK_VERSION="0.11.0"

read_shellcheck_version() {
  local shellcheck_bin="$1" banner
  # Bound the banner first: `$bin --version | sed` would swallow a failing
  # binary's exit status behind sed's success.
  banner="$("$shellcheck_bin" --version)"
  printf '%s\n' "$banner" | sed -n 's/^version: //p'
}

# Resolve the shellcheck binary this repo should use.
#
# Candidate order is deliberate: the bootstrap-installed pinned binary in
# ~/.local/bin wins over whatever the host package manager or a CI image put on
# PATH. GitHub-hosted runners ship a shellcheck of their own choosing, so
# preferring PATH would make a correctly bootstrapped machine fail the pin
# check. If no candidate matches the pin we still return the first candidate we
# found, so the caller can name the *actual* offending version in its failure
# message instead of the useless "not installed".
find_shellcheck_bin() {
  local candidates=() candidate path_bin fallback=""

  candidates+=("${HOME:-}/.local/bin/shellcheck")
  path_bin="$(command -v shellcheck 2>/dev/null || true)"
  if [ -n "$path_bin" ]; then
    candidates+=("$path_bin")
  fi

  for candidate in "${candidates[@]}"; do
    [ -n "$candidate" ] || continue
    [ -x "$candidate" ] || continue
    local found_version
    found_version="$(read_shellcheck_version "$candidate")"
    if [ "$found_version" = "$SHELLCHECK_VERSION" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
    if [ -z "$fallback" ]; then
      fallback="$candidate"
    fi
  done

  if [ -n "$fallback" ]; then
    printf '%s\n' "$fallback"
    return 0
  fi
  return 1
}

# Print the pinned shellcheck binary on stdout, or write a FAIL line to stderr
# and return 1.
#
# ⛔ There is no "shellcheck unavailable -> skip" branch here, by design. TC-37:
# agent-lint-md.sh once skip_notice'd (exit 0) when markdownlint-cli2 was
# absent, and that vacuous green hid a genuinely red `main` for three weeks. A
# missing linter is a FAILED lint, never a passed one.
require_shellcheck_bin() {
  local label="${1:-lint-shell}"
  local shellcheck_bin found_version

  shellcheck_bin="$(find_shellcheck_bin || true)"
  if [ -z "$shellcheck_bin" ]; then
    printf 'FAIL %s: shellcheck %s is required but not installed. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' \
      "$label" "$SHELLCHECK_VERSION" >&2
    return 1
  fi

  found_version="$(read_shellcheck_version "$shellcheck_bin")"
  if [ "$found_version" != "$SHELLCHECK_VERSION" ]; then
    printf 'FAIL %s: shellcheck %s is required; selected shellcheck %s. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' \
      "$label" "$SHELLCHECK_VERSION" "$found_version" >&2
    return 1
  fi

  printf '%s\n' "$shellcheck_bin"
}
