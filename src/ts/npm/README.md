# npm per-platform binary packages

Each subdirectory here is a standalone npm package that ships **one** prebuilt
napi-rs binding (`fathomdb.<triple>.node`) for a single host, tagged with
`os` / `cpu` / `libc`. They are published as `@fathomdb/fathomdb-<triple>` and
wired as `optionalDependencies` of the thin main `fathomdb` package, so npm
installs only the one matching the host and skips the rest.

The main package's loader (`src/ts/src/platform.ts`) resolves the host triple
and `require`s the matching platform package, throwing a clear
`UnsupportedPlatformError` when none is present — never a silent runtime
segfault (R-REL-4f, `dev/design/0.8.18-slice-20-publish-pipeline.md`).

## Populated for 0.8.18 (critical path = x86_64-unknown-linux-gnu, D5)

- `linux-x64-gnu/` — `@fathomdb/fathomdb-linux-x64-gnu`

The `.node` binary is NOT committed; the release workflow's `build-napi` job
stages it into this directory before publishing.

## Deferred to the follow-on orchestrator (R-REL-4d)

`darwin-x64`, `darwin-arm64`, `win32-x64-msvc`, `linux-arm64-gnu`,
`linux-x64-musl`, … — added here (each a sibling directory) when the follow-on
completes the cross-platform matrix and promotes the main package to `latest`.
