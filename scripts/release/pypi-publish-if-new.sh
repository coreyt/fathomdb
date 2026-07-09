#!/usr/bin/env bash
# scripts/release/pypi-publish-if-new.sh — idempotent PyPI publish guard for
# the release.yml tier-8 pypi leg.
#
# PyPI (like npm) rejects a re-upload of an already-present file with HTTP 400,
# so any partial-release retry or the coordinated 0.8.20 OPP-12 breaking-pair
# re-run fails hard. This wrapper makes the PyPI leg idempotent, matching
# cargo-publish-if-new.sh / npm-publish-if-new.sh:
#
#   • query the PyPI JSON API for <project>/<version>;
#   • if that version already exists → log + exit 0 (no-op);
#   • otherwise                      → upload via twine with --skip-existing
#     (defence-in-depth: a same-second race still no-ops per-file at twine).
#
# In CI the pypa/gh-action-pypi-publish action carries `skip-existing: true`
# (its trusted-publishing/OIDC path cannot be replaced by a shell twine call);
# this script is the locally-exercisable equivalent that the idempotency test
# drives, and is the fallback for a manual/emergency re-publish.
#
# Usage: pypi-publish-if-new.sh <project> <version> <dist-dir>
#
# Env:
#   PYPI_PUBLISH_IF_NEW_REGISTRY  JSON-API base in place of https://pypi.org
#                                 (tests point this at a fixture http server)
#   TWINE_BIN                     twine invocation (default: "twine"; tests shim)
#
# Exit: 0 = uploaded or already-present (idempotent); non-zero = real failure.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  cat <<'USAGE' >&2
Usage: pypi-publish-if-new.sh <project> <version> <dist-dir>

Uploads <dist-dir>/* to PyPI only if <project>@<version> is not already
present. Uses twine --skip-existing for same-second-race safety.
USAGE
  exit 2
fi

PROJECT="$1"
VERSION="$2"
DIST_DIR="$3"
BASE="${PYPI_PUBLISH_IF_NEW_REGISTRY:-https://pypi.org}"
TWINE_BIN="${TWINE_BIN:-twine}"

# Returns 0 if PROJECT==VERSION is already released, 1 if absent, 2 on error.
# The PyPI JSON API returns 404 for an unreleased project/version.
registry_has_version() {
  local url status
  url="${BASE}/pypi/${PROJECT}/${VERSION}/json"
  status="$(curl --silent --show-error --max-time 30 -o /dev/null -w '%{http_code}' \
    -H "User-Agent: fathomdb-release-pypi-guard (https://github.com/coreyt/fathomdb)" \
    "$url" 2>/dev/null || printf '000')"
  case "$status" in
    200) return 0 ;;
    404) return 1 ;;
    *)   return 2 ;;
  esac
}

set +e
registry_has_version
rc=$?
set -e
case "$rc" in
  0)
    printf 'pypi-publish-if-new: %s %s already released; skipping\n' "$PROJECT" "$VERSION"
    exit 0
    ;;
  1)
    printf 'pypi-publish-if-new: %s %s not on PyPI; uploading\n' "$PROJECT" "$VERSION"
    exec "$TWINE_BIN" upload --skip-existing "$DIST_DIR"/*
    ;;
  *)
    printf 'pypi-publish-if-new: PyPI query failed for %s; refusing to blind-upload\n' \
      "$PROJECT" >&2
    exit 1
    ;;
esac
