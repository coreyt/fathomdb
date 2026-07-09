#!/usr/bin/env bash
# scripts/release/npm-inject-optional-deps.sh — inject the napi per-platform
# `optionalDependencies` into the main `fathomdb` package.json AT PUBLISH TIME
# (R-REL-4f).
#
# Why publish-time injection (the napi-rs idiom) and not a committed field:
# the `@fathomdb/fathomdb-<triple>` binary packages are published DURING the
# release, so at dev time they do not exist on the registry. Committing them as
# `optionalDependencies` would desync `package-lock.json` and break every
# `npm ci`. Instead the committed package.json is `npm ci`-clean, and this
# script writes the optionalDependencies into it just before `npm publish` so
# the PUBLISHED package carries the correct end-user contract.
#
# One entry is injected for EACH platform present under src/ts/npm/<triple>/
# (each such directory is a package published this release), pinned to the main
# package's own version. For 0.8.18 only `linux-x64-gnu` exists → only
# `@fathomdb/fathomdb-linux-x64-gnu` is injected; the follow-on adds more
# platform directories and they are picked up automatically.
#
# Usage: npm-inject-optional-deps.sh [<main-package-dir>] [<npm-platform-dir>]
#   defaults: <main-package-dir>=src/ts, <npm-platform-dir>=src/ts/npm
#
# Idempotent: re-running overwrites the optionalDependencies block with the
# same computed value.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MAIN_DIR="${1:-$REPO_ROOT/src/ts}"
PLATFORM_DIR="${2:-$REPO_ROOT/src/ts/npm}"
MAIN_PKG="$MAIN_DIR/package.json"

if [ ! -f "$MAIN_PKG" ]; then
  printf 'npm-inject-optional-deps: main package.json not found at %s\n' "$MAIN_PKG" >&2
  exit 1
fi

node -e '
  const fs = require("fs");
  const path = require("path");
  const mainPkgPath = process.argv[1];
  const platformDir = process.argv[2];

  const pkg = JSON.parse(fs.readFileSync(mainPkgPath, "utf8"));
  const version = pkg.version;

  const optional = {};
  if (fs.existsSync(platformDir)) {
    for (const entry of fs.readdirSync(platformDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const platPkgPath = path.join(platformDir, entry.name, "package.json");
      if (!fs.existsSync(platPkgPath)) continue;
      const platPkg = JSON.parse(fs.readFileSync(platPkgPath, "utf8"));
      optional[platPkg.name] = version;
    }
  }

  if (Object.keys(optional).length === 0) {
    console.error("npm-inject-optional-deps: no platform packages found under " + platformDir);
    process.exit(1);
  }

  // Sort keys for deterministic output.
  pkg.optionalDependencies = Object.fromEntries(
    Object.keys(optional).sort().map((k) => [k, optional[k]]),
  );
  fs.writeFileSync(mainPkgPath, JSON.stringify(pkg, null, 2) + "\n");

  const names = Object.keys(pkg.optionalDependencies).join(", ");
  console.log(`npm-inject-optional-deps: injected optionalDependencies @ ${version}: ${names}`);
' "$MAIN_PKG" "$PLATFORM_DIR"
