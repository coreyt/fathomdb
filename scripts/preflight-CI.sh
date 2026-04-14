#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$REPO_ROOT"

log() {
  printf '\n[%s] %s\n' "preflight" "$*"
}

run() {
  log "$*"
  "$@"
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'error: required command not found: %s\n' "$1" >&2
    exit 1
  }
}

local_napi_label() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="linux" ;;
    Darwin) os="darwin" ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) os="win32" ;;
    *)
      printf 'error: unsupported OS for local napi prebuild label: %s\n' "$os" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x64" ;;
    aarch64|arm64) arch="arm64" ;;
    *)
      printf 'error: unsupported architecture for local napi prebuild label: %s\n' "$arch" >&2
      exit 1
      ;;
  esac

  printf 'fathomdb.%s-%s.node\n' "$os" "$arch"
}

local_napi_source() {
  local os
  os="$(uname -s)"
  case "$os" in
    Linux) printf 'target/release/libfathomdb.so\n' ;;
    Darwin) printf 'target/release/libfathomdb.dylib\n' ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) printf 'target/release/fathomdb.dll\n' ;;
    *)
      printf 'error: unsupported OS for local napi prebuild source: %s\n' "$os" >&2
      exit 1
      ;;
  esac
}

stage_local_napi_prebuild() {
  local src dest
  src="$(local_napi_source)"
  dest="typescript/packages/fathomdb/prebuilds/$(local_napi_label)"
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
  printf 'staged local napi prebuild: %s\n' "$dest"
}

main() {
  need_cmd bash
  need_cmd cargo
  need_cmd python
  need_cmd npm
  need_cmd go
  need_cmd golangci-lint
  need_cmd ruff
  need_cmd mkdocs
  need_cmd cargo-nextest

  # Clean /tmp of stale fathomdb test artifacts that accumulate across runs.
  # Sources of accumulation that do NOT clean up after themselves:
  #  - typescript/apps/sdk-harness/src/skip.ts:tempDbPath() mints per-scenario
  #    /tmp/fathomdb-harness-<scenario>-<timestamp>-<random>.db files and
  #    never removes them, so every `npm test -w @fathomdb/sdk-harness` leaks
  #    one .db + sidecars per scenario.
  #  - examples.harness.app writes the per-scenario sibling databases via
  #    HarnessContext.sibling_db(), which the main-only cleanup below (pre-
  #    invocation) also has to glob-remove.
  #  - go/fathom-integrity e2e tests leak /tmp/fathomdb-e2e-toolchain-* dirs.
  # Left unchecked these accumulate into hundreds of files in /tmp, causing
  # disk pressure that eventually flakes vitest `afterEach` hook timeouts in
  # typescript/packages/fathomdb/test (seen during 0.3.1 release prep as a
  # 30-second afterEach on a test that normally completes in <100ms).
  # Wipe the family at preflight start so every run begins from a clean slate.
  run bash -c 'rm -rf /tmp/fathomdb-harness-* /tmp/fathomdb-e2e-toolchain-*'

  run bash .git/hooks/pre-commit
  run bash .git/hooks/pre-push

  run bash scripts/test-setup-dev.sh
  run python scripts/check-doc-hygiene.py
  run python scripts/check-version-consistency.py
  run bash docs/build.sh

  run cargo fmt --check
  run cargo clippy --workspace --all-targets -- -D warnings -A missing-docs
  run cargo clippy --workspace --all-targets --features tracing -- -D warnings -A missing-docs
  run cargo clippy --workspace --all-targets --features python -- -D warnings -A missing-docs
  run cargo build --workspace
  run cargo nextest run --workspace
  run cargo nextest run --workspace --features tracing
  run cargo check --tests -p fathomdb --features python

  run ruff check python/
  run python -m pip install -e python --no-build-isolation
  run env PYTHONPATH=python pytest python/tests -v --timeout=60
  # The harness app opens the given --db path as-is and does not recreate it
  # (python/examples/harness/app.py:84). A fixed /tmp path + no cleanup means
  # a second preflight invocation starts against a database already populated
  # by the first, which either trips UNIQUE constraints on upsert scenarios
  # or skews row counts in trace_and_excise. Additionally, many scenarios
  # call context.sibling_db(...) (python/examples/harness/models.py:31) to
  # open isolated engines at derived paths like
  # /tmp/fathomdb-harness-baseline-trace-and-excise.db, so a main-only
  # cleanup still leaves per-scenario siblings around. Glob-remove the
  # entire family — main, siblings, and their WAL/SHM/lock sidecars —
  # before each harness invocation so the run always starts cold.
  run bash -c 'rm -f /tmp/fathomdb-harness-baseline*.db /tmp/fathomdb-harness-baseline*.db-wal /tmp/fathomdb-harness-baseline*.db-shm /tmp/fathomdb-harness-baseline*.db.lock'
  run env PYTHONPATH=python python -m examples.harness.app --db /tmp/fathomdb-harness-baseline.db --mode baseline --telemetry off
  run bash -c 'rm -f /tmp/fathomdb-harness-vector*.db /tmp/fathomdb-harness-vector*.db-wal /tmp/fathomdb-harness-vector*.db-shm /tmp/fathomdb-harness-vector*.db.lock'
  run env PYTHONPATH=python python -m examples.harness.app --db /tmp/fathomdb-harness-vector.db --mode vector --telemetry off

  run bash scripts/install-project-sqlite.sh
  # shellcheck disable=SC1091
  source "$REPO_ROOT/tooling/sqlite.env"
  export PATH="$REPO_ROOT/.local/sqlite-$SQLITE_VERSION/bin:$PATH"

  run bash -lc 'cd go/fathom-integrity && test -z "$(gofmt -l .)"'
  run bash -lc 'cd go/fathom-integrity && go vet ./...'
  run bash -lc 'cd go/fathom-integrity && XDG_CACHE_HOME=/tmp/golangci-cache golangci-lint run ./...'
  run bash -lc 'cd go/fathom-integrity && go test $(go list ./... | grep -v /test/e2e)'
  run bash -lc 'cd go/fathom-integrity && go test ./test/e2e/...'

  run npm install --ignore-scripts --prefix typescript
  run bash -lc 'cd typescript && npm audit --audit-level=high --omit=dev'
  run cargo build -p fathomdb --features node
  run bash -lc 'cd typescript && npm run typecheck'
  run bash -lc 'cd typescript && npm run build'
  run bash -lc 'cd typescript && npm test'

  run cargo build --release --features node,sqlite-vec,tracing -p fathomdb
  run stage_local_napi_prebuild
  run bash -lc 'cd typescript/packages/fathomdb && npm pack --dry-run'
  run bash -lc 'cd python && maturin build --release --out dist -i python3.10'
  # Only the leaf crates (fathomdb-query, fathomdb-schema) can be
  # dry-run-published before a real release. The non-leaf crates
  # (fathomdb-engine, fathomdb) depend on the leaf crates at the
  # about-to-be-released version — e.g. `fathomdb-engine` carries
  # `fathomdb-query = "^0.3.1"` — and cargo publish --dry-run resolves
  # dependency versions from the crates.io index, not from local paths.
  # During a release where the workspace has been bumped to a version
  # newer than what is published, the non-leaf dry-runs always fail
  # with "failed to select a version for the requirement". The real
  # release workflow (.github/workflows/release.yml) publishes the
  # leaf tier first, waits 60s for index propagation, then publishes
  # each subsequent tier — that ordering is what makes actual
  # publishing work, and there is no way to simulate it in a dry-run.
  # This asymmetry was introduced when the tiered publish was added
  # to release.yml; preflight kept the four dry-runs and they only
  # "worked" in practice because preflight was always run on unbumped
  # main. Keep the two leaf dry-runs as genuine pre-publish checks.
  run cargo publish --dry-run -p fathomdb-query
  run cargo publish --dry-run -p fathomdb-schema

  if command -v semgrep >/dev/null 2>&1; then
    run semgrep --config auto crates/ python/ typescript/ --error
  else
    log "skipping semgrep: command not installed"
  fi

  if command -v gh >/dev/null 2>&1 && [[ -n "${GITHUB_REPOSITORY:-}" ]] && [[ -n "${GITHUB_SHA:-}" ]] && [[ -n "${GITHUB_REF_NAME:-}" ]]; then
    run python scripts/verify-release-gates.py
  else
    log "skipping release gate verification: set gh + GITHUB_REPOSITORY + GITHUB_SHA + GITHUB_REF_NAME to mirror release.yml"
  fi

  log "preflight complete"
}

main "$@"
