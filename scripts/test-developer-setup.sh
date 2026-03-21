#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/developer-setup.sh"

fail() {
  printf 'test failure: %s\n' "$*" >&2
  exit 1
}

assert_true() {
  local description="$1"
  shift
  if ! "$@"; then
    fail "$description"
  fi
}

assert_false() {
  local description="$1"
  shift
  if "$@"; then
    fail "$description"
  fi
}

assert_eq() {
  local expected="$1"
  local actual="$2"
  local description="$3"
  if [[ "$expected" != "$actual" ]]; then
    fail "$description: expected '$expected', got '$actual'"
  fi
}

test_sqlite_version_at_least() {
  assert_true "3.46.0 should satisfy 3.41.0 minimum" sqlite_version_at_least "3.46.0" "3.41.0"
  assert_true "3.41.0 should satisfy 3.41.0 minimum" sqlite_version_at_least "3.41.0" "3.41.0"
  assert_false "3.31.1 should not satisfy 3.41.0 minimum" sqlite_version_at_least "3.31.1" "3.41.0"
}

test_sqlite_supported_predicate() {
  assert_true "3.41.0 should be supported" sqlite_version_supported "3.41.0"
  assert_true "3.46.0 should be supported" sqlite_version_supported "3.46.0"
  assert_false "3.38.0 should not be supported" sqlite_version_supported "3.38.0"
}

test_sqlite_project_install_needed() {
  assert_true "missing project-local sqlite should trigger install" sqlite_project_install_needed ""
  assert_true "older project-local sqlite should trigger install" sqlite_project_install_needed "3.45.2"
  assert_false "target sqlite version should not trigger install" sqlite_project_install_needed "3.46.0"
}

test_sqlite_download_metadata() {
  assert_eq "3460000" "$(sqlite_numeric_version "3.46.0")" "numeric sqlite version should match upstream packaging format"
  assert_eq "2024" "$(sqlite_release_year "3.46.0")" "sqlite 3.46.0 release year should match upstream download path"
}

main() {
  test_sqlite_version_at_least
  test_sqlite_supported_predicate
  test_sqlite_project_install_needed
  test_sqlite_download_metadata
  printf 'developer-setup tests passed\n'
}

main "$@"
