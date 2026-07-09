#!/usr/bin/env bash
# scripts/release/npm-publish-if-new.sh — idempotent npm publish guard for the
# release.yml tier-8 npm legs (main package + per-platform packages).
#
# npm rejects a second `publish` of an already-published version with E403,
# which turns any partial-release retry (or the coordinated 0.8.20 OPP-12
# breaking-pair re-run) into a hard failure. This wrapper makes the npm leg
# idempotent — matching the crates.io behaviour of cargo-publish-if-new.sh:
#
#   • query the npm registry for <name>@<version>;
#   • if that exact version already exists  → log + exit 0 (no-op);
#   • otherwise                             → `npm publish` with the given args.
#
# Usage:
#   npm-publish-if-new.sh [--dry-run] --tag <dist-tag> [-- <extra npm args>]
#
# Reads name+version from ./package.json in the current working directory
# (so it works uniformly for the main package and each npm/<target>/ package).
#
# Env:
#   NPM_PUBLISH_IF_NEW_REGISTRY  base URL in place of https://registry.npmjs.org
#                                (tests point this at a fixture http server)
#   NPM_BIN                      publish command to invoke (default: npm; may be
#                                a multi-word command, e.g. "npx npm@latest",
#                                which CI needs for OIDC trusted publishing;
#                                tests shim a single-word `npm`)
#
# Exit: 0 = published or already-present (idempotent); non-zero = real failure.
set -euo pipefail

usage() {
  cat <<'USAGE' >&2
Usage: npm-publish-if-new.sh [--dry-run] --tag <dist-tag> [-- <extra npm publish args>]

Publishes ./package.json's name@version to npm only if that version is not
already on the registry. --dry-run forwards to `npm publish --dry-run`
regardless of registry state.
USAGE
}

DRY_RUN=0
DIST_TAG=""
EXTRA=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --tag) DIST_TAG="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; EXTRA=("$@"); break ;;
    *) printf 'npm-publish-if-new: unexpected arg %s\n' "$1" >&2; usage; exit 2 ;;
  esac
done

if [ -z "$DIST_TAG" ]; then
  printf 'npm-publish-if-new: --tag <dist-tag> is required\n' >&2
  usage
  exit 2
fi
if [ ! -f package.json ]; then
  printf 'npm-publish-if-new: no package.json in %s\n' "$(pwd)" >&2
  exit 1
fi

NPM_BIN="${NPM_BIN:-npm}"
NAME="$(node -e "process.stdout.write(require('./package.json').name)")"
VERSION="$(node -e "process.stdout.write(require('./package.json').version)")"
BASE="${NPM_PUBLISH_IF_NEW_REGISTRY:-https://registry.npmjs.org}"

# Returns 0 if NAME@VERSION exists on the registry, 1 if absent, 2 on a
# registry error we cannot interpret (fail-closed — do not blind-publish).
# The npm registry packument exposes published versions under `.versions`.
registry_has_version() {
  local url body
  # Scoped names (@scope/pkg) must be URL-encoded (/ -> %2f) for the packument.
  url="${BASE}/$(printf '%s' "$NAME" | sed 's#/#%2f#')"
  if ! body="$(curl --silent --show-error --max-time 30 \
        -H "User-Agent: fathomdb-release-npm-guard (https://github.com/coreyt/fathomdb)" \
        "$url" 2>/dev/null)"; then
    return 2
  fi
  # A 404 packument (never-published package) is a valid "absent" answer.
  if printf '%s' "$body" | jq -e '.error == "Not found" or .error == "version not found"' >/dev/null 2>&1; then
    return 1
  fi
  if ! printf '%s' "$body" | jq . >/dev/null 2>&1; then
    return 2
  fi
  if printf '%s' "$body" | jq -e --arg v "$VERSION" '.versions[$v] != null' >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

if [ "$DRY_RUN" -eq 1 ]; then
  printf 'npm-publish-if-new: %s@%s — dry-run\n' "$NAME" "$VERSION"
  # NPM_BIN may be a multi-word command ("npx npm@latest") — intentional split.
  # shellcheck disable=SC2086
  exec $NPM_BIN publish --dry-run --tag "$DIST_TAG" "${EXTRA[@]}"
fi

set +e
registry_has_version
rc=$?
set -e
case "$rc" in
  0)
    printf 'npm-publish-if-new: %s@%s already published; skipping\n' "$NAME" "$VERSION"
    exit 0
    ;;
  1)
    printf 'npm-publish-if-new: %s@%s not on registry; publishing (tag=%s)\n' \
      "$NAME" "$VERSION" "$DIST_TAG"
    # NPM_BIN may be a multi-word command ("npx npm@latest") — intentional split.
    # shellcheck disable=SC2086
    exec $NPM_BIN publish --tag "$DIST_TAG" "${EXTRA[@]}"
    ;;
  *)
    printf 'npm-publish-if-new: registry query failed for %s; refusing to blind-publish\n' \
      "$NAME" >&2
    exit 1
    ;;
esac
