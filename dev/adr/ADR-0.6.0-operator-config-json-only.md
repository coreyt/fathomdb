---
title: ADR-0.6.0-operator-config-json-only
date: 2026-04-25
target_release: 0.6.0
desc: Operator-supplied config files are JSON-only in 0.6.0+; toml dep dropped
blast_radius: src/rust/crates/fathomdb-engine/src/admin/vector.rs (load_vector_regeneration_config); src/rust/crates/fathomdb-engine/src/admin/mod.rs:3739; src/rust/crates/fathomdb-engine/Cargo.toml; dev/deps/toml.md
status: accepted
---

# ADR-0.6.0 — Operator config = JSON-only

**Status:** accepted (HITL 2026-04-25, decision-recording)

## Context

0.5.x `load_vector_regeneration_config(path)` accepted both `.toml` and
`.json` extensions, branching on file extension. The TOML path has no
known consumer: Hermes uses YAML, OpenClaw uses JSON. The dual-format
branch is a small example of the "speculative knobs" Stop-doing pattern —
two parsers shipped to handle one job; misconfigured extensions silently
take the wrong parser path.

Critic-B F11 flagged `toml` as a soft-keep (kept "for now" pending Phase 2
ADR on configure surface). HITL collapsed the question.

## Decision

**0.6.0+ operator config = JSON only.**

- `load_vector_regeneration_config` accepts `.json` and missing-extension
  (default JSON). The `.toml` branch is removed.
- `toml` direct dep removed from `src/rust/crates/fathomdb-engine/Cargo.toml`.
- The test write site at `src/rust/crates/fathomdb-engine/src/admin/mod.rs:3739`
  switches to `serde_json`.
- Clients wanting YAML / TOML / etc. convert client-side.

## Options considered

**A. JSON only (chosen).** One parser, one syntax. Pros: removes a dep,
removes the dual-format branch, narrows config surface to one well-known
format. Cons: breaks any caller still writing TOML configs (no known
consumer affected).

**B. TOML + JSON dual.** Status quo. Pros: backwards-compatible. Cons:
"speculative knobs" — two parsers for one job; ext-branch is a silent
mis-routing class of bug; no consumer benefits.

**C. YAML.** Hermes uses it. Pros: aligns with one consumer. Cons: pulls
a YAML dep; ambiguous indentation footguns; OpenClaw already uses JSON.
Worse than (A).

## Consequences

- `src/python/`, `src/ts/` examples updated to JSON.
- Implementer change deletes the TOML branch + `toml` workspace dep + the
  TOML test write site. ~14 LoC.
- Followup: any future operator config (engine-open options, etc.) defaults
  to JSON. No TOML re-introduction without revisiting this ADR.
- **HF-Hub cache layout (X-3 cross-cite): not in scope.** This ADR governs
  _operator-supplied_ config files. HuggingFace Hub cache files (`refs/`,
  `blobs/`, `snapshots/`, `meta.json`) are internal artifacts of the model
  loader; their on-disk format is the loader's contract with itself, not
  user-facing config. Whatever serialization HF cache uses (currently a
  mix of plain text refs + JSON metadata + binary blobs) is exempt from
  the JSON-only rule.
- **Strict JSON only (JSON-2 followup).** RFC-8259 strict; no JSON5,
  no JSONC, no comments, no trailing commas. Operators wanting comments
  use a sidecar `.md` next to the config. Tracked as FU-JSON2.
- **Config-site enumeration (JSON-1 followup).** Every config-accepting
  surface (engine-open options, embedder config, op-store
  payload-schema-validation config, FTS opts, future surfaces) must be
  enumerated and confirmed JSON-only before Phase 3 interface lock.
  Tracked as FU-JSON1.
- **JSON Schema validation policy (M-5 followup).** When and where
  operator-config JSON gets validated against a schema is open. Tracked
  as FU-M5; gate on Phase 3 design.

## Citations

- HITL decision 2026-04-25 (deps F11 resolution).
- `src/rust/crates/fathomdb-engine/src/admin/vector.rs:1320-1336`.
- `dev/deps/toml.md`.
