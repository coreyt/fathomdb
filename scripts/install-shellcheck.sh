#!/usr/bin/env bash
# scripts/install-shellcheck.sh — the SINGLE installer for this repo's pinned
# ShellCheck. Extracted from scripts/bootstrap.sh (which now calls this file) in
# 0.8.21 Slice 35 (SHELL-LINT-CI) so the `shell-lint` CI job can install exactly
# the linter it gates on WITHOUT the rest of bootstrap — no rust toolchain, no
# node, no python venv, no `cargo install lychee`. That job exists to report in
# about a minute; a full bootstrap would defeat its entire purpose.
#
# ONE INSTALLER, TWO CALLERS. The logic is not duplicated into the workflow
# YAML: `scripts/bootstrap.sh` runs this file, and so does the `shell-lint` job.
# A pin bump therefore lands in exactly one place
# (scripts/lib/shellcheck-version.sh) and cannot leave CI and local dev on
# different linters — which is the whole reason the version is pinned at all.
#
# ⚠ THE RUNNER IMAGE SHIPS ITS OWN SHELLCHECK, AND IT IS NOT THE PIN.
# `ubuntu-latest` has a shellcheck on PATH at a version GitHub chooses, so a
# naive "shellcheck is pre-installed, just call it" job would hard-fail the
# repo's version preflight (require_shellcheck_bin) the moment the image moved.
# That is handled DELIBERATELY, and NOT by relaxing the pin:
#   * `find_shellcheck_bin` (scripts/lib/shellcheck-version.sh) probes
#     $HOME/.local/bin/shellcheck FIRST and only then PATH, so the binary this
#     script installs wins over the image's copy regardless of PATH order.
#   * this script NAMES the image copy and its version in the log, so the two
#     are never confused when reading a run.
#   * installing into $HOME/.local/bin needs no sudo and overwrites nothing the
#     image owns.
#
# ⛔ NO SILENT SKIP, EVER. Every failure path below exits non-zero: an
# unrecorded platform, a failed download, a checksum mismatch, or an installed
# binary that does not report the pin. A "successful" install that did not
# produce the pinned linter would yield a lint run that cannot fail — the TC-37
# vacuous-green trap that hid a red `main` for three weeks.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/shellcheck-version.sh
. "$SCRIPT_DIR/lib/shellcheck-version.sh"

# What is already on PATH — reported, never trusted. On a GitHub-hosted runner
# this is the image's own copy.
image_bin="$(command -v shellcheck 2>/dev/null || true)"
image_version=""
if [ -n "$image_bin" ]; then
  image_version="$(read_shellcheck_version "$image_bin")"
  if [ "$image_version" != "$SHELLCHECK_VERSION" ]; then
    printf 'note: PATH carries shellcheck %s at %s; this repo pins %s.\n' \
      "$image_version" "$image_bin" "$SHELLCHECK_VERSION"
    printf 'note: the pinned binary is installed into %s/.local/bin, which find_shellcheck_bin prefers.\n' \
      "$HOME"
  fi
fi

shellcheck_bin="$(find_shellcheck_bin || true)"
shellcheck_found_version=""
if [ -n "$shellcheck_bin" ]; then
  shellcheck_found_version="$(read_shellcheck_version "$shellcheck_bin")"
fi

if [ "$shellcheck_found_version" != "$SHELLCHECK_VERSION" ]; then
  echo "Installing shellcheck v$SHELLCHECK_VERSION into $HOME/.local/bin ..."
  shellcheck_os="$(uname -s)"
  shellcheck_arch="$(uname -m)"
  case "$shellcheck_os/$shellcheck_arch" in
    Linux/x86_64)
      shellcheck_slug="linux.x86_64"
      shellcheck_sha256="8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198"
      ;;
    Linux/aarch64 | Linux/arm64)
      shellcheck_slug="linux.aarch64"
      shellcheck_sha256="12b331c1d2db6b9eb13cfca64306b1b157a86eb69db83023e261eaa7e7c14588"
      ;;
    Darwin/x86_64)
      shellcheck_slug="darwin.x86_64"
      shellcheck_sha256="3c89db4edcab7cf1c27bff178882e0f6f27f7afdf54e859fa041fca10febe4c6"
      ;;
    Darwin/arm64)
      shellcheck_slug="darwin.aarch64"
      shellcheck_sha256="56affdd8de5527894dca6dc3d7e0a99a873b0f004d7aabc30ae407d3f48b0a79"
      ;;
    *)
      echo "shellcheck $SHELLCHECK_VERSION is required but no release tarball is recorded for $shellcheck_os/$shellcheck_arch" >&2
      echo "  install it manually from https://github.com/koalaman/shellcheck/releases (pin v$SHELLCHECK_VERSION)" >&2
      echo "  and record its checksum in scripts/install-shellcheck.sh" >&2
      exit 1
      ;;
  esac

  shellcheck_url="https://github.com/koalaman/shellcheck/releases/download/v$SHELLCHECK_VERSION/shellcheck-v$SHELLCHECK_VERSION.$shellcheck_slug.tar.xz"
  shellcheck_tmp="$(mktemp -d)"
  if ! curl -fsSL --retry 3 -o "$shellcheck_tmp/shellcheck.tar.xz" "$shellcheck_url"; then
    rm -rf "$shellcheck_tmp"
    echo "shellcheck $SHELLCHECK_VERSION download failed: $shellcheck_url" >&2
    exit 1
  fi
  if ! printf '%s  %s\n' "$shellcheck_sha256" "$shellcheck_tmp/shellcheck.tar.xz" | sha256sum -c - >/dev/null 2>&1; then
    rm -rf "$shellcheck_tmp"
    echo "shellcheck $SHELLCHECK_VERSION download failed its SHA-256 check ($shellcheck_url)" >&2
    exit 1
  fi
  tar -xJf "$shellcheck_tmp/shellcheck.tar.xz" -C "$shellcheck_tmp"
  mkdir -p "$HOME/.local/bin"
  install -m 0755 "$shellcheck_tmp/shellcheck-v$SHELLCHECK_VERSION/shellcheck" "$HOME/.local/bin/shellcheck"
  rm -rf "$shellcheck_tmp"
fi

# The post-condition, asserted the SAME way the lint leg asserts it: resolve
# through require_shellcheck_bin, so "installed" means "the gate will select
# this binary and it reports the pin" rather than "a file exists".
shellcheck_bin="$(require_shellcheck_bin install-shellcheck)" || exit 1
printf 'shellcheck v%s is installed at %s\n' "$SHELLCHECK_VERSION" "$shellcheck_bin"

# GitHub Actions applies GITHUB_PATH only to LATER steps, so persist the
# resolved directory for them. The repo's own scripts resolve the binary
# directly via find_shellcheck_bin and do not depend on this.
if [ -n "${GITHUB_PATH:-}" ]; then
  shellcheck_dir="$(dirname "$shellcheck_bin")"
  printf '%s\n' "$shellcheck_dir" >>"$GITHUB_PATH"
fi
