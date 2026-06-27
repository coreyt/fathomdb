#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "FathomDB scaffold bootstrap"
echo "Public docs live in docs/ and build with MkDocs."
echo "Internal engineering docs live in dev/."
echo "Rust workspace members live under src/rust/crates/."
echo "Run scripts/agent-verify.sh during the agent loop, scripts/check.sh as the broader CI gate."

# Repo-tracked git hooks (pre-commit fmt/lint, pre-push agent-verify).
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
# linter for .github/workflows/*.yml. Version pin matches a recent stable
# release; bump deliberately, not drifted.
if ! command -v actionlint >/dev/null 2>&1; then
  if command -v go >/dev/null 2>&1; then
    echo "Installing actionlint v1.7.12 via go install..."
    GO111MODULE=on go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12
    if ! command -v actionlint >/dev/null 2>&1; then
      echo "actionlint installed under \$GOBIN (default ~/go/bin); add it to PATH" >&2
      echo "  e.g. export PATH=\"\$(go env GOPATH)/bin:\$PATH\"" >&2
    fi
  else
    echo "actionlint not installed and go toolchain unavailable; install actionlint manually" >&2
    echo "  see https://github.com/rhysd/actionlint/releases (pin v1.7.12)" >&2
  fi
fi
