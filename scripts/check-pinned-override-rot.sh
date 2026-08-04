#!/usr/bin/env bash
# Check that dependency overrides remain justified, non-vulnerable, and scoped.
#
# The predicate is deliberately offline: advisory data and governed override
# rationale live in scripts/pinned-override-rot.json. A missing or malformed
# record is UNVERIFIED and fails; R2 also fails closed for every live override.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel)"

if ! command -v python3 >/dev/null 2>&1; then
  printf 'UNVERIFIED pinned-override-rot: python3 is required; refusing to report a clean result\n' >&2
  exit 2
fi

exec python3 "$SCRIPT_DIR/check-pinned-override-rot.py" --root "$REPO_ROOT" "$@"
