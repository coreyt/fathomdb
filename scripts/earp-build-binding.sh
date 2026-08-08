#!/usr/bin/env bash
# Build the native binding into a WORKTREE-LOCAL venv.
#
# Standing guidance forbids `maturin develop` from a worktree because it
# repoints the SHARED main-checkout .venv at worktree source. This script never
# touches that venv: it builds into ./.venv inside this worktree, which is
# gitignored and disposable. It is EARP scaffolding, not release tooling.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
venv="${root}/.venv"

if [[ ! -x "${venv}/bin/maturin" ]]; then
  echo "no maturin in ${venv}; create the venv first" >&2
  exit 2
fi

case "${VIRTUAL_ENV:-}" in
  "${root}"/*) ;;
  *) export VIRTUAL_ENV="${venv}" ;;
esac

export PATH="${venv}/bin:${PATH}"
cd "${root}/src/python"
exec "${venv}/bin/maturin" develop --release
