#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1
# shellcheck source=lib/actionlint-version.sh
. "$SCRIPT_DIR/lib/actionlint-version.sh"
# shellcheck source=lib/shellcheck-version.sh
. "$SCRIPT_DIR/lib/shellcheck-version.sh"

echo "FathomDB scaffold bootstrap"
echo "Public docs live in docs/ and build with MkDocs."
echo "Internal engineering docs live in dev/."
echo "Rust workspace members live under src/rust/crates/."
echo "Run scripts/agent-verify.sh during the agent loop, scripts/check.sh as the broader CI gate."

# Repo-tracked git hooks: activate via core.hooksPath (repo-relative, so linked
# worktrees inherit it too). pre-commit = fast fmt/ruff + AST-guarded markdown
# auto-fix/enforce; pre-push = fast clippy/actionlint (full verify opt-in via
# FATHOMDB_PREPUSH_FULL=1). See scripts/install-hooks.sh.
scripts/install-hooks.sh

# Python dev tooling — pytest, hypothesis, ruff, pyright.
if [ -f src/python/pyproject.toml ]; then
  echo "Installing Python dev tooling into .venv (pytest + hypothesis + ruff + pyright)..."
  python3 -m venv .venv
  # 0.8.9 Slice 1 (R-BOOT-2): no output masking — a future dev-tooling failure
  # (pip resolution, an unguarded import that fails pyright) must be VISIBLE in
  # the CI log, not swallowed. Dropping `--quiet`/`>/dev/null` is what surfaced
  # the httpx import-not-found error that was silently failing bootstrap on main.
  .venv/bin/python -m pip install --upgrade pip
  .venv/bin/python -m pip install -e 'src/python[dev]'
  .venv/bin/python -c 'import pytest, hypothesis'
  .venv/bin/pyright -p src/python
fi

# TypeScript dev tooling.
if [ -f src/ts/package.json ] && [ ! -d src/ts/node_modules ]; then
  echo "Installing TypeScript dev tooling..."
  (cd src/ts && npm install --silent)
fi

# Repo-wide markdown tooling (markdownlint-cli2 + prettier).
if [ -f package.json ] && [ ! -d node_modules ]; then
  echo "Installing markdown dev tooling (markdownlint-cli2 + prettier)..."
  npm install --silent
fi

# Lychee link checker (Rust binary).
if ! command -v lychee >/dev/null 2>&1; then
  echo "Installing lychee link checker..."
  cargo install --locked --quiet lychee
fi

# strace — required by the AC-036 no-listen and AC-037 netns-deny-egress
# security fixtures under scripts/security/. ~50KB, unprivileged at
# runtime. Skip silently if apt isn't available (non-Debian hosts); the
# fixtures will report a BLOCKER exit themselves.
if ! command -v strace >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    echo "Installing strace (AC-036/AC-037 security fixtures)..."
    # GitHub-hosted runners ship with stale apt indexes; without an
    # update first, `apt-get install` can fail on 404. Local dev runs
    # bootstrap rarely, so the extra ~5s is acceptable.
    sudo apt-get update -qq >/dev/null 2>&1 || true
    sudo apt-get install --no-install-recommends -y strace >/dev/null 2>&1 || \
      echo "strace install failed; AC-036/AC-037 will report BLOCKER until installed" >&2
  else
    echo "strace not installed and apt-get unavailable; install via host package manager" >&2
    echo "  (required by scripts/security/check-no-listen.sh + check-netns-deny-egress.sh)" >&2
  fi
fi

# actionlint — workflow validator. Pinned: yaml.safe_load passes
# schema-invalid syntax that GitHub silently rejects, so we need a real
# linter for .github/workflows/*.yml. A missing or different binary is not a
# usable bootstrap result: install the exact CI version and verify it.
readonly ACTIONLINT_VERSION="1.7.12"
actionlint_bin="$(find_actionlint_bin || true)"
actionlint_version=""
if [ -n "$actionlint_bin" ]; then
  actionlint_version="$(read_actionlint_version "$actionlint_bin")"
fi

if [ "$actionlint_version" != "$ACTIONLINT_VERSION" ]; then
  if ! command -v go >/dev/null 2>&1; then
    echo "actionlint $ACTIONLINT_VERSION is required but go is unavailable; install it manually" >&2
    echo "  see https://github.com/rhysd/actionlint/releases (pin v$ACTIONLINT_VERSION)" >&2
    exit 1
  fi
  echo "Installing actionlint v$ACTIONLINT_VERSION via go install..."
  GO111MODULE=on go install "github.com/rhysd/actionlint/cmd/actionlint@v$ACTIONLINT_VERSION"
  actionlint_bin="$(go env GOPATH)/bin/actionlint"
  installed_actionlint_version=""
  if [ -x "$actionlint_bin" ]; then
    installed_actionlint_version="$(read_actionlint_version "$actionlint_bin")"
  fi
  if [ "$installed_actionlint_version" != "$ACTIONLINT_VERSION" ]; then
    echo "actionlint v$ACTIONLINT_VERSION installation did not produce the required binary" >&2
    exit 1
  fi
  echo "actionlint v$ACTIONLINT_VERSION is installed at $actionlint_bin"
fi

# ShellCheck — the shell linter. Pinned for the same reason actionlint and ruff
# are: shellcheck's finding set changes between releases, so an unpinned linter
# silently redefines what "green" means. Installed from the upstream static
# release tarball, verified against a recorded SHA-256 (we are fetching an
# executable over the network; a checksum is not optional), into ~/.local/bin so
# no sudo is needed and so it wins over whatever the host or the CI image put on
# PATH.
#
# ⛔ NO SILENT SKIP. If shellcheck cannot be installed this exits non-zero. A
# bootstrap that "succeeds" without the linter produces a lint run that cannot
# fail, which is the TC-37 vacuous-green trap that hid a red `main` for three
# weeks.
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
      echo "  and record its checksum in scripts/bootstrap.sh" >&2
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

  shellcheck_bin="$(find_shellcheck_bin || true)"
  shellcheck_found_version=""
  if [ -n "$shellcheck_bin" ]; then
    shellcheck_found_version="$(read_shellcheck_version "$shellcheck_bin")"
  fi
  if [ "$shellcheck_found_version" != "$SHELLCHECK_VERSION" ]; then
    echo "shellcheck v$SHELLCHECK_VERSION installation did not produce the required binary" >&2
    echo "  resolved: '${shellcheck_bin:-<none>}' reporting '${shellcheck_found_version:-<none>}'" >&2
    exit 1
  fi
  echo "shellcheck v$SHELLCHECK_VERSION is installed at $shellcheck_bin"
fi

# GitHub Actions applies GITHUB_PATH only to later steps. Persist the resolved
# directories so `agent-verify` can invoke the exact bootstrap-installed
# binaries.
if [ -n "${GITHUB_PATH:-}" ]; then
  actionlint_dir="$(dirname "$actionlint_bin")"
  shellcheck_dir="$(dirname "$shellcheck_bin")"
  printf '%s\n%s\n' "$actionlint_dir" "$shellcheck_dir" >>"$GITHUB_PATH"
fi
