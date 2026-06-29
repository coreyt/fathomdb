#!/usr/bin/env bash
# scripts/release/verify-embedder-api-no-drift.sh — Axis-E published-API
# drift guard. Run as a release preflight (release.yml verify-release job
# and local-dry-run.sh) BEFORE a tag is pushed.
#
# WHAT IT PREVENTS (the v0.8.9 partial-publish incident):
#   fathomdb-embedder-api carries an INDEPENDENT version ("Axis E") that
#   does not move on every workspace lockstep release. A defaulted trait
#   method (`Embedder::embed_batch`) was added to its public surface for
#   the GPU batch path, and fathomdb-engine started calling it — but the
#   Axis-E version was NOT bumped. The registry's published embedder-api
#   at that pinned version therefore lacked the method. On the real
#   `v0.8.9` publish, `cargo publish` VERIFY of fathomdb-engine compiled
#   its tarball against the PUBLISHED embedder-api and failed with
#   `error[E0599]: no method named embed_batch found for Arc<dyn Embedder>`,
#   after schema/query/embedder 0.8.9 had already uploaded (immutable) —
#   a partial publish. Recovery was an Axis-E bump 0.6.0 -> 0.6.1.
#
#   The release dry-run missed it because the dry-run cargo publish path
#   uses `--no-verify` (see cargo-publish-if-new.sh / release.yml), which
#   skips exactly the tarball verify-compile that surfaces this. A blanket
#   removal of --no-verify is NOT viable: dependent crates (engine/facade/
#   cli) need NEW-version siblings that are not on the registry during a
#   dry-run.
#
# THE INVARIANT THIS ENFORCES (and why it is detectable pre-tag):
#   The bug was a mismatch against an ALREADY-PUBLISHED, version-UNCHANGED
#   dependency — local code declared embedder-api version V while V was
#   already on the registry with a DIFFERENT public surface. So the guard
#   is: if the local Axis-E version V is already published, the local
#   embedder-api source surface MUST match the published V. If it differs,
#   the API moved without an Axis-E bump — FAIL and tell the author to run
#   `scripts/set-version.sh --embedder-api <next>`.
#
#   This needs ONLY the published crate at the pinned version — no
#   unpublished 0.8.x siblings, no engine compile. If V is NOT yet
#   published (a legitimate new Axis-E version), the guard passes: that
#   case is verify-compiled for real at publish time (the embedder-api
#   leaf tier runs a full `cargo publish`, verify included).
#
# Test seams (env overrides; NOT for production CI):
#   EMB_API_DRIFT_REGISTRY        base URL in place of https://crates.io
#                                 (used for BOTH the version query and the
#                                  /api/v1/crates/<c>/<v>/download tarball).
#   EMB_API_DRIFT_LOCAL_VERSION   skip the manifest read; use this version.
#                                 (Reproduces the pre-fix drift: pin V to a
#                                  published version whose surface differs.)
#
# Exit: 0 = no drift (or new unpublished version); non-zero = drift / a
# toolchain or registry failure (fail-closed; never a silent skip-pass).
set -euo pipefail

CRATE="fathomdb-embedder-api"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CRATE_DIR="$REPO_ROOT/src/rust/crates/$CRATE"
CRATE_MANIFEST="$CRATE_DIR/Cargo.toml"
BASE_URL="${EMB_API_DRIFT_REGISTRY:-https://crates.io}"
UA="fathomdb-release-publish-guard (https://github.com/coreyt/fathomdb)"

die() {
  printf 'embedder-api-drift: %s\n' "$*" >&2
  exit 1
}

# Fail loudly when a required tool is absent — STRICT, never skip-pass.
for tool in curl jq tar diff awk; do
  command -v "$tool" >/dev/null 2>&1 \
    || die "required tool '$tool' not found on PATH (refusing to skip — a missing tool would hide drift)"
done

# Read [package].version from the crate manifest (mirrors the reader in
# cargo-publish-if-new.sh / set-version.sh: first version line in the block).
read_crate_version() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=[[:space:]]*"/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$CRATE_MANIFEST"
}

if [ -n "${EMB_API_DRIFT_LOCAL_VERSION:-}" ]; then
  LOCAL_VERSION="$EMB_API_DRIFT_LOCAL_VERSION"
else
  [ -f "$CRATE_MANIFEST" ] || die "manifest not found at $CRATE_MANIFEST"
  LOCAL_VERSION="$(read_crate_version)"
  [ -n "$LOCAL_VERSION" ] || die "cannot read [package].version for $CRATE"
fi

# Query the registry for the published version list. Fail-closed on any
# network / 5xx / malformed-body error (return 2), exactly like
# cargo-publish-if-new.sh's registry_has_version.
registry_has_version() {
  local version="$1"
  local url="${BASE_URL}/api/v1/crates/${CRATE}"
  local body
  if ! body="$(curl --silent --show-error --fail --max-time 30 \
        -H "User-Agent: $UA" "$url" 2>&1)"; then
    printf 'embedder-api-drift: registry query failed for %s — %s\n' \
      "$CRATE" "$body" >&2
    return 2
  fi
  if ! printf '%s' "$body" | jq . >/dev/null 2>&1; then
    printf 'embedder-api-drift: registry returned malformed JSON for %s\n' \
      "$CRATE" >&2
    return 2
  fi
  if printf '%s' "$body" | jq -e --arg v "$version" \
       '.versions | map(.num) | index($v) != null' >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

# Normalize a crate root's src/ surface: every *.rs under src/, sorted, with
# blank lines, trailing whitespace, and comment-only lines (//, ///, //!)
# removed. This compares the CODE/API surface and ignores doc-comment and
# whitespace churn, so the guard fails on signature drift (the real bug)
# without false-reds on a re-worded doc string.
normalize_src() {
  local root="$1" f rel
  while IFS= read -r f; do
    rel="${f#"$root"/}"
    printf '### %s\n' "$rel"
    awk '
      {
        line = $0
        sub(/[[:space:]]+$/, "", line)
        t = line
        sub(/^[[:space:]]+/, "", t)
        if (t == "")           next
        if (t ~ /^\/\//)       next
        print line
      }
    ' "$f"
  done < <(find "$root/src" -type f -name '*.rs' | LC_ALL=C sort)
}

set +e
registry_has_version "$LOCAL_VERSION"
rc=$?
set -e

case "$rc" in
  1)
    printf 'embedder-api-drift: %s@%s is not yet on the registry — new Axis-E version; verify-compiled at publish time. OK.\n' \
      "$CRATE" "$LOCAL_VERSION"
    exit 0
    ;;
  2)
    exit 2
    ;;
esac

# Version IS published. Download that exact published crate and diff its
# source surface against the working tree.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
TARBALL="$WORK/$CRATE-$LOCAL_VERSION.crate"
DL_URL="${BASE_URL}/api/v1/crates/${CRATE}/${LOCAL_VERSION}/download"

if ! curl --silent --show-error --fail --location --max-time 60 \
      -H "User-Agent: $UA" "$DL_URL" -o "$TARBALL"; then
  die "failed to download published $CRATE@$LOCAL_VERSION from $DL_URL"
fi
if ! tar -xzf "$TARBALL" -C "$WORK"; then
  die "failed to extract published crate tarball $TARBALL"
fi

PUB_ROOT="$WORK/$CRATE-$LOCAL_VERSION"
[ -d "$PUB_ROOT/src" ] \
  || die "published tarball has no src/ tree at $PUB_ROOT (unexpected layout)"

if diff -u \
     <(normalize_src "$PUB_ROOT") \
     <(normalize_src "$CRATE_DIR") >"$WORK/drift.diff" 2>&1; then
  printf 'embedder-api-drift: OK — local %s source surface matches published @%s.\n' \
    "$CRATE" "$LOCAL_VERSION"
  exit 0
fi

{
  printf '\n'
  printf 'AXIS-E DRIFT DETECTED\n'
  printf '=====================\n'
  printf 'Local %s declares version %s, which is ALREADY published, but the\n' \
    "$CRATE" "$LOCAL_VERSION"
  printf 'local source surface DIFFERS from the published crate at that version.\n'
  printf '\n'
  printf 'This is the v0.8.9 partial-publish class: a consumer (e.g.\n'
  printf 'fathomdb-engine) may use an API absent from the published Axis-E\n'
  # shellcheck disable=SC2016  # backticks are literal help text, not command substitution
  printf 'crate, so `cargo publish` verify will fail AFTER earlier tiers have\n'
  printf 'already uploaded immutably.\n'
  printf '\n'
  printf 'FIX: bump Axis E before tagging:\n'
  printf '     scripts/set-version.sh --embedder-api <next-version>\n'
  printf '(or revert the embedder-api source change if it was unintended).\n'
  printf '\n'
  printf '%s\n' "--- published @$LOCAL_VERSION (left)  vs  working tree (right) ---"
  cat "$WORK/drift.diff"
} >&2
exit 1
