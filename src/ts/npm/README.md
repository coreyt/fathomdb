# npm per-platform binary packages

Each subdirectory here is a standalone npm package that ships **one** prebuilt
napi-rs binding (`fathomdb.<triple>.node`) for a single host, tagged with
`os` / `cpu` / `libc`. They are published as `fathomdb-<triple>` and
wired as `optionalDependencies` of the thin main `fathomdb` package, so npm
installs only the one matching the host and skips the rest.

The main package's loader (`src/ts/src/platform.ts`) resolves the host triple
and `require`s the matching platform package, throwing a clear
`UnsupportedPlatformError` when none is present — never a silent runtime
segfault (R-REL-4f, `dev/design/0.8.18-slice-20-publish-pipeline.md`).

## Supported packages for 0.8.22

- `linux-x64-gnu/` — `fathomdb-linux-x64-gnu`
- `linux-arm64-gnu/` — `fathomdb-linux-arm64-gnu`
- `darwin-x64/` — `fathomdb-darwin-x64`
- `darwin-arm64/` — `fathomdb-darwin-arm64`
- `win32-x64-msvc/` — `fathomdb-win32-x64-msvc`

The `.node` binary is NOT committed; the release workflow's `build-napi` job
stages it into this directory before publishing.

## Unsupported targets

Linux musl, Windows ARM/32-bit, and other targets deliberately have no
platform package. Their loader error is part of the public install contract;
they must not be described as supported until an artifact, registry smoke, and
compatibility documentation land together.
