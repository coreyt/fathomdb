#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# `git rev-parse` failing here used to degrade to `cd ""` — a bash no-op that
# leaves the script running in an arbitrary cwd. Bind and check it instead.
_repo_toplevel="$(git rev-parse --show-toplevel)" || exit 1
cd "$_repo_toplevel" || exit 1
# shellcheck source=lib/actionlint-version.sh
. "$SCRIPT_DIR/lib/actionlint-version.sh"

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
# silently redefines what "green" means.
#
# The installer itself lives in scripts/install-shellcheck.sh — ONE file, so
# that the 0.8.21 Slice 35 `shell-lint` CI job can install the exact same pinned
# linter WITHOUT paying for the rest of this bootstrap (rust, node, the python
# venv, `cargo install lychee`), and so a pin bump cannot leave that job and this
# script on different linters. That file also owns the "the runner image ships
# its own shellcheck" handling and persists its directory to $GITHUB_PATH.
#
# ⛔ NO SILENT SKIP. install-shellcheck.sh exits non-zero on every failure path,
# and `set -e` here propagates it. A bootstrap that "succeeds" without the
# linter produces a lint run that cannot fail, which is the TC-37 vacuous-green
# trap that hid a red `main` for three weeks.
bash "$SCRIPT_DIR/install-shellcheck.sh"

# GitHub Actions applies GITHUB_PATH only to later steps. Persist the resolved
# actionlint directory so `agent-verify` can invoke the exact bootstrap-installed
# binary. (install-shellcheck.sh persists shellcheck's own directory.)
if [ -n "${GITHUB_PATH:-}" ]; then
  actionlint_dir="$(dirname "$actionlint_bin")"
  printf '%s\n' "$actionlint_dir" >>"$GITHUB_PATH"
fi
