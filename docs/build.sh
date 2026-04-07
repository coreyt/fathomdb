#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

case "${1:-build}" in
  build)
    mkdocs build --strict --config-file mkdocs.yml
    ;;
  serve)
    mkdocs serve --config-file mkdocs.yml
    ;;
  *)
    echo "Usage: $0 {build|serve}" >&2
    exit 1
    ;;
esac
