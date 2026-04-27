---
title: ADR-0.6.0-json-schema-policy
date: 2026-04-27
target_release: 0.6.0
desc: In-repo schemas/; save-time validation; reject-write on failure; schema_id versioned across major releases
blast_radius: schemas/ directory; ADR-0.6.0-op-store-same-file (operational_collections.schema_json); ADR-0.6.0-operator-config-json-only; design/engine.md validation hook; followups FU-M5
status: accepted
---

# ADR-0.6.0 — JSON Schema validation policy

**Status:** accepted (HITL 2026-04-27).

FU-M5 promoted to Phase 2 ADR. Cross-cuts operator-config + op-store payload validation.

## Context

ADR-0.6.0-operator-config-json-only locked operator config to strict JSON. ADR-0.6.0-op-store-same-file specified `operational_collections.schema_json` as declarative metadata. Open: where schemas live, when validation runs, how failures surface, how schemas evolve.

HITL 2026-04-27 settled:
- Schema location: in-repo `schemas/`.
- Operator-supplied schemas: not accepted in 0.6.0.

This ADR settles the remaining sub-questions.

## Decision

### Schema location

- **In-repo `schemas/`** at workspace root (or `crates/fathomdb-engine/schemas/` if engine-internal).
- Each schema file is JSON Schema draft 2020-12; named `<entity>_v<N>.json` (e.g. `operational_collection_v1.json`, `vector_regeneration_config_v1.json`).
- Each schema has a stable `schema_id` (the filename without `.json`) referenced by op-store collections (`operational_collections.schema_json` carries the `schema_id`, not the schema body).
- **Operator-supplied schemas: not accepted** in 0.6.0. Re-opening this restriction requires an ADR amendment.

### Validation cadence

- **Save-time only.** Every write that targets a schema-bound entity validates the payload against the schema before commit.
- No open-time re-validation. Historical rows are not re-checked at `Engine.open`.
- Engine-internal validation cost paid on the writer thread; benchmarked as part of write-throughput SLI #24.

### Failure mode

- **Reject-write.** Validation failure returns `EngineError::SchemaValidation { schema_id, errors }` (per error-taxonomy ADR); the write does not commit.
- No warn-and-write. No log-only mode.

### Schema versioning

- New `schema_id` per breaking change (`operational_collection_v2.json` is a new file alongside v1).
- Old `schema_id`s remain valid within the same major release per ADR-0.6.0-no-shims-policy (no within-0.6.x deprecation cycles); they may be removed only at the 0.7+ major boundary.
- Migration between schema versions is a client-driven re-write, not an engine-side migration (per `feedback_no_data_migration` and the no-shims ADR).

## Options considered

**A — In-repo schemas + save-time + reject-write + `schema_id` versioning (chosen).** Save-time + reject-write is the structural posture; matches no-shims; clear failure mode. Schema location settled by HITL.

**B — Save-time + open-time re-validation; reject-write + reject-open.** Stronger guarantee; surfaces schema drift on startup; adds open-time cost; risks blocking open over historical rows that were valid under their original schema. Reopens the data-migration non-goal.

**C — Save-time; warn-and-write on failure; logged.** Speculative-knob anti-pattern; hides bugs; failure mode hidden behind log levels. Rejected.

## Consequences

- `schemas/` directory created at workspace root. Index file `schemas/README.md` enumerates each schema with its `schema_id`, version, and consumers.
- `crates/fathomdb-engine` carries a typed loader for `schemas/` (compile-time embed via `include_str!` is acceptable; runtime read from disk also acceptable per implementer choice — design/engine.md decides).
- `EngineError::SchemaValidation` variant added to the error taxonomy.
- `design/engine.md` documents the save-time validation hook in the writer path.
- Op-store `operational_collections.schema_json` field stores the `schema_id` (string), not the schema body.
- Future operator-supplied schema support is a 0.7+ ADR.
- Cross-cite ADR-0.6.0-op-store-same-file: payload validation under `schema_id`.
- Cross-cite ADR-0.6.0-operator-config-json-only: future config validation under engine-shipped schemas.

## Citations

- HITL 2026-04-27 (location: in-repo `schemas/`; no operator-supplied).
- ADR-0.6.0-op-store-same-file (operational_collections.schema_json).
- ADR-0.6.0-operator-config-json-only (strict JSON).
- ADR-0.6.0-no-shims-policy (no within-0.6.x deprecation cycles).
- ADR-0.6.0-error-taxonomy (`SchemaValidationError` is a distinct module variant per § Module-error boundary table).
- `feedback_no_data_migration` memory.
