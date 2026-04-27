---
title: 0.6.0 Followups
date: 2026-04-24
target_release: 0.6.0
desc: Items deferred beyond 0.6.0; write-only during 0.6.0 doc phase
blast_radius: TBD
status: living
---

# Followups

**Read-discipline:** this file is **write-mostly** during 0.6.0. Working agents
append items but MUST NOT read this file unless explicitly told. Keeps working
context clean.

Item format:

```
## FU-NNN: <title>

**Origin:** <who/when/why>
**Target release:** 0.6.1 | 0.7.0 | TBD
**Notes:**
```

Seeded:
- **Upgrade path for 0.5.x users** — deferred from 0.6.0. Design in later release.

---

## FU-TWB1: CI lint gate for raw-SQL leaks

**Origin:** critic-3 TWB-1 (2026-04-27); enforces ADR-0.6.0-typed-write-boundary.
**Target release:** 0.6.0 (pre-implementation gate).
**Notes:** Add CI step that fails on any new public API across bindings exposing a string-typed SQL parameter. Initial pattern: `rg 'pub.*sql.*&str|pub.*query.*&str' crates/*/src` returns zero. Lint must also catch the same in PyO3 / napi-rs binding source. Lives in pre-merge check; fails build on first regression.

## FU-TWB2: Recovery verb set enumeration

**Origin:** critic-3 TWB-2 (2026-04-27); ADR-0.6.0-typed-write-boundary cites recovery as "typed CLI flags, not SQL."
**Target release:** 0.6.0 (Phase 3e interfaces/cli.md).
**Notes:** Enumerate every recovery / inspection verb the CLI must expose so users have no reason to ask for an SQL escape hatch. Examples: dump-schema, dump-row-counts, dump-profile, vacuum, integrity-check, export-op-store, repair-vector-index. Land in `interfaces/cli.md`.

## FU-JSON1: Operator-config site enumeration

**Origin:** critic-3 JSON-1 (2026-04-27); ADR-0.6.0-operator-config-json-only.
**Target release:** 0.6.0 (pre Phase 3e lock).
**Notes:** Enumerate every config-accepting surface and confirm JSON-only. Known sites: `load_vector_regeneration_config`; engine-open options; embedder config; op-store payload-schema-validation config; FTS opts. Add a row per site; flag any that accept a non-JSON format.

## FU-JSON2: Strict RFC-8259 documentation

**Origin:** critic-3 JSON-2 (2026-04-27).
**Target release:** 0.6.0 (Phase 3e interfaces).
**Notes:** Document in every operator-facing config doc that JSON is strict RFC-8259: no comments, no trailing commas, no JSON5/JSONC. Recommend a sidecar `<config>.md` for human-readable notes. Add a parser-level rejection test for JSONC-style comments.

## FU-OPS1: Op-store schema namespacing rule

**Origin:** critic-3 OPS-1 (2026-04-27); ADR-0.6.0-op-store-same-file.
**Target release:** 0.6.0 (Phase 3 design/engine.md).
**Notes:** `operational_*` prefix (folded-design convention: `operational_collections`, `operational_mutations`, `operational_current`). Document migration ordering: op-store tables created in the same schema-migration step as the primary tables they reference. Reject any op-store table without the prefix in CI.

## FU-OPS2: safe_export op-store coverage + redaction policy

**Origin:** critic-3 OPS-2 (2026-04-27); ADR-0.6.0-op-store-same-file.
**Target release:** **0.8.0 (HITL deferral 2026-04-27).**
**Notes:** `safe_export` of op-store rows + redaction policy deferred to 0.8.0. Rationale: 0.6.0 op-store payloads are operationally-bounded (connector health, cursors, counters, heartbeats) — high-sensitivity secrets are unlikely to land there in practice. Premature redaction policy adds surface without forcing function. Revisit when (a) op-store gains a use case with operator-supplied secrets, or (b) safe_export becomes a release-blocking feature for an external client. Until then, `safe_export` may emit op-store rows verbatim or omit them entirely; specific behavior decided at implementation time, not pinned by ADR.

## FU-OPS4: Op-store transaction boundary detail

**Origin:** critic-3 OPS-4 (2026-04-27); ADR-0.6.0-op-store-same-file.
**Target release:** 0.6.0 (Phase 3 design/engine.md).
**Notes:** Document the exact transactional API shape for the "primary entity write + step row + op-store row" tuple. Land in `design/engine.md` writer section. The invariant (atomic commit on the writer thread) is settled by the ADR; only the API shape is open.

## FU-EMB3: Per-platform wheel-size CI gate

**Origin:** critic-3 EMB-3 (2026-04-27); ADR-0.6.0-default-embedder.
**Target release:** 0.6.0 (CI matrix).
**Notes:** CI fails if the published wheel grows by more than 20 MB between releases on any tier-1 platform (linux x86_64, linux aarch64, darwin universal, windows x86_64). Threshold prevents silent dep bloat from candle / tokenizers / hf-hub upgrades. Implementation: store wheel size per platform in a small JSON manifest under `dist-meta/`; CI compares.

## FU-EMB5: hf-hub replacement design

**Origin:** critic-3 EMB-5 (2026-04-27); ADR-0.6.0-default-embedder.
**Target release:** 0.6.0 (Phase 3 design/embedder.md).
**Notes:** `hf-hub` carries Tokio + reqwest. Design a replacement model-resolver that reuses an existing HTTP client (rusqlite has none; ureq is candidate) and a flat on-disk cache layout. Cache layout per ADR-0.6.0-operator-config-json-only X-3: HF cache files are internal artifacts, not user-facing config — exempt from JSON-only.

## FU-EMB7: Structural lint for vector identity invariant

**Origin:** critic on M-4 (2026-04-27); ADR-0.6.0-vector-identity-embedder-owned.
**Target release:** 0.6.0 (CI gate).
**Notes:** Replace the grep sketch with a typed AST / typegraph check: "no struct reachable from `VectorConfig` references `EmbedderIdentity` or any of its fields by type." Concrete crate path is `fathomdb-core::config::*`. Implementation candidates: a unit test over the type graph, a `#[cfg(test)]` `static_assertions` set, or a clippy lint. Pick whichever is simplest at implementation time.

## FU-ASYNC5: TS cancellation semantics

**Origin:** critic-3 ASYNC-5 (2026-04-27); ADR-0.6.0-async-surface.
**Target release:** TBD (0.6.x or 0.7).
**Notes:** TS `Promise<...>` returns are not cancellable in 0.6.0 initial. Design: optional `AbortSignal` parameter on each TS verb; signal cancellation surfaces as a typed `EngineError::Cancelled`. Open question: whether cancellation aborts the writer-thread submission or only the napi waiter. Defer until first user request.

## FU-WIRE15: Subprocess bridge wire format

**Origin:** Phase 2 #15 deferral (HITL 2026-04-27).
**Target release:** 0.8.0.
**Notes:** Per ADR-0.6.0-subprocess-bridge-deferral, no subprocess bridge in 0.6.0; revisit in 0.8.0 (skip 0.7.0). If a forcing function emerges before 0.8.0 (non-PyO3 Python flavor, process-isolation requirement for embedders), the ADR is re-opened. Default-on-revisit format: JSON over stdio with versioned envelope `{ "v": 1, ... }` unless the consumer's needs argue otherwise.

## FU-RET17: Composable middleware retrieval pipeline

**Origin:** Phase 2 #17 deferral (HITL 2026-04-27).
**Target release:** 0.8.0.
**Notes:** Per ADR-0.6.0-retrieval-pipeline-shape, 0.6.0 ships fixed stages with per-stage config. Revisit composable middleware pipeline (trait-object stages, user-spliced stages) in 0.8.0 with concrete user needs. Forcing function: a real retrieval requirement that fixed-stage config cannot express. Until then, fixed stages remain.

## FU-VEC13-CORRUPTION: Single-file corruption-recovery posture

**Origin:** Phase 2 #13 critic [vec-loc-02] (2026-04-27); ADR-0.6.0-vector-index-location.
**Target release:** 0.6.x (post-freeze if needed) or 0.7.0.
**Notes:** Single SQLite file holds application + op-store + `vec0` shadow tables — one corruption blast-radius. Open: detection mechanism (PRAGMA integrity_check on open? on demand? scheduled?), `Engine.open` behavior on detected corruption (refuse-open vs open-read-only vs auto-attempt-recover), recovery ownership (CLI verb? programmatic API? both?). Decide as ADR-0.6.x-corruption-recovery if the failure mode lands in practice.

## FU-PW19-BATCH-SEMANTICS: Write batch transactional semantics

**Origin:** Phase 2 #19 critic [pw-01] (2026-04-27); ADR-0.6.0-prepared-write-shape.
**Target release:** 0.6.0 (Phase 3 design/engine.md).
**Notes:** ADR-0.6.0-prepared-write-shape commits to `&[PreparedWrite]` shape but defers transactional semantics: is the slice one transaction, N transactions, or per-variant-grouped? Decide in design/engine.md; promote to its own ADR if the answer is non-mechanical. Settling this also unlocks the per-variant-validation-error-with-batch-index question deferred in [pw-02].

## FU-PW19-BINDING-EXHAUSTIVENESS: Non-exhaustive enum across bindings

**Origin:** Phase 2 #19 critic [pw-04] (2026-04-27); ADR-0.6.0-prepared-write-shape.
**Target release:** 0.6.0 (interfaces/python.md + interfaces/typescript.md).
**Notes:** Rust `#[non_exhaustive]` does not protect Python `isinstance` chains or TS discriminated-union switches. Decide per-binding posture: default branch required by lint? runtime check on unknown variant? document the variant set as "stable within minor"? Resolve in the binding interface docs; promote to ADR if cross-binding posture diverges.

## FU-PY20-STUB-GENERATION: Python type-stub generation method

**Origin:** Phase 2 #20 critic [py-02] (2026-04-27); ADR-0.6.0-python-api-shape.
**Target release:** 0.6.0 (Phase 5 implementation).
**Notes:** ADR commits to `.pyi` shipping in the wheel. Generation method (hand-written, `pyo3-stub-gen`, `mypy stubgen`, custom) chosen at implementation time. Keep CI gate (`mypy --strict` against the stubs) regardless of generation choice.

## FU-DEP23-WITHIN-MINOR-REUSE: Within-0.6.x removed-name reuse rule

**Origin:** Phase 2 #23 critic [dep-01] (2026-04-27); ADR-0.6.0-deprecation-policy-0-5-names.
**Target release:** 0.6.0 (release-policy.md) or 0.6.x as needed.
**Notes:** ADR explicitly punts on "may a name removed in 0.6.x be reused with new meaning in a later 0.6.x release?" Resolve in release-policy.md. Default until decided: do not reuse — drop is forever within a minor series. If a real need surfaces, ADR-0.6.x-name-reuse settles it.

## FU-LOWS-2026-04-27: Lite-batch ADR low-severity findings

**Origin:** Critic on lite-batch ADRs (2026-04-27). Logged-not-applied per low-severity policy.
**Target release:** N/A (cleanup at next ADR amendment).
**Notes:**
- [tier1-04] Drop / move "this dev box is Jetson" footnote.
- [tier1-05] Name the perf-gate "reference target" (likely `x86_64-unknown-linux-gnu`) or strike the example.
- [vec-loc-04] Pin `vec0` shadow-table naming as either private-impl or documented convention; not both.
- [pw-05] Cite `AdminSchemaWrite` source or mark provisional pending design/engine.md.
- [py-04] Make snake_case-fields commitment explicit alongside snake_case-methods.
- [py-05] Name the structural enforcement of "asyncio threads never run embedders" or strike the bullet.
- [dep-03] Reword "0.5.x callers cannot run on 0.6.0 anyway" — DB-freshness ≠ API-freshness.
- [X-03] Optionally call out manylinux_2_28 baseline in 0.6.0 release notes.
