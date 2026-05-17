#!/usr/bin/env bash
# scripts/release/smoke/smoke-crates-cli.sh — AC-056 crates.io smoke.
#
#   $1 = version (e.g. 0.6.0)
#
# Installs fathomdb-cli from crates.io (NOT from this workspace), creates a
# fresh fixture DB in a tempdir, runs `fathomdb doctor check-integrity
# --json` against it, asserts exit 0 + valid JSON output. Engine::open
# creates the DB file lazily, so the smoke does not need an init verb.
#
# Per `feedback_release_verification`: green CI + published artifact is NOT
# done. This script is the gate that proves the published crate actually
# installs and runs cleanly on a fresh ubuntu — distinct from the CI build,
# which only proves the workspace tree compiles.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  printf 'usage: %s <version>\n' "$0" >&2
  exit 2
fi
VERSION="$1"
if ! printf '%s' "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  printf 'smoke-crates-cli: invalid version "%s" — expected semver MAJOR.MINOR.PATCH\n' \
    "$VERSION" >&2
  exit 2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# `cargo install` honors --root for the install prefix; binary lands at
# $WORK/bin/fathomdb.
cargo install fathomdb-cli --version "$VERSION" --root "$WORK" --locked

DB="$WORK/smoke.fdb"
OUT="$("$WORK/bin/fathomdb" doctor check-integrity --json "$DB")"
printf '%s\n' "$OUT" | jq -e . >/dev/null

printf 'smoke-crates-cli: ok — fathomdb-cli %s installed + check-integrity returned valid JSON\n' \
  "$VERSION"
