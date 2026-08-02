# ADR-0.8.20 — Unscoped npm platform packages

- **Status: ACCEPTED (HITL approved 2026-08-02).** Supersedes the scoped-name
  portion of ADR-0.8.18's per-platform npm package topology.

## Decision

The thin main package remains `fathomdb`. Each native platform package is
unscoped and named `fathomdb-<triple>`, beginning with
`fathomdb-linux-x64-gnu`.

## Rationale

The npm account owns the unscoped `fathomdb` package and intentionally has no
`@fathomdb` organization. The scoped platform name could therefore not be
published, leaving the release partial after crates.io and PyPI completed.

## Consequences

The release-time optional dependency and runtime loader use the unscoped name.
Future platform packages follow the same convention. The non-`latest` `next`
dist-tag remains unchanged while only Linux x64 has a prebuilt binary.
