# Dev Document Status Audit - 2026-04-22

## Scope

This audit covers active developer documentation only:

- `dev/*.md`
- `dev/notes/*.md`

It intentionally excludes `dev/archive/`, because those files are already
historical and are not expected to be current.

## Status Meanings

- `up-to-date`: still valid as active developer documentation.
- `needs-update`: mostly valid, but needs small corrections such as stale paths,
  version wording, status banners, or references to archived docs.
- `needs-rewrite`: still covers an important topic, but the document's current
  framing is stale enough that patching individual lines would be misleading.
- `no-longer-valid`: should not remain active; archive it or replace it with a
  current document.

## Summary

| Status | Count |
|---|---:|
| `up-to-date` | 32 |
| `needs-update` | 33 |
| `needs-rewrite` | 7 |
| `no-longer-valid` | 16 |

## Top-Level `dev/`

| File | Status | Reason / next action |
|---|---|---|
| `dev/ARCHITECTURE-deferred-expansion.md` | `up-to-date` | Current architecture-boundary supplement. |
| `dev/ARCHITECTURE.md` | `needs-update` | Still authoritative, but contains stale references such as `python_types.rs` and `v0.1` recovery wording. |
| `dev/SDK-typescript-spec.md` | `no-longer-valid` | TypeScript SDK is implemented; archive or replace with current TS SDK architecture. |
| `dev/USER_NEEDS.md` | `up-to-date` | Current product/architecture framing. |
| `dev/arch-decision-vector-embedding-recovery.md` | `needs-update` | Still useful, but anchored to the old `v0.1` vector recovery contract. |
| `dev/architecture-note-memex-support.md` | `needs-rewrite` | Large Memex gap analysis mixes implemented work, stale gaps, and current ideas. Rewrite as a current Memex integration status. |
| `dev/db-integrity-management.md` | `needs-rewrite` | Conceptually useful, but examples and projection names are older than the current repair/admin surface. |
| `dev/dbim-playbook.md` | `needs-update` | Better current operational doc than `db-integrity-management.md`, but should be checked against current commands. |
| `dev/design-adaptive-text-search-surface-addendum-1-vec.md` | `needs-update` | Relevant design, but status should reflect what has shipped vs. remains proposed. |
| `dev/design-adaptive-text-search-surface.md` | `needs-update` | Relevant design, but current implementation has moved beyond parts of its "proposed/current state" wording. |
| `dev/design-add-operational-store-feature.md` | `up-to-date` | Still valid as the operational-store design. |
| `dev/design-automatic-background-retention.md` | `up-to-date` | Current boundary decision: engine primitives plus external scheduling. |
| `dev/design-bridge-input-safety.md` | `needs-update` | Security design appears implemented; update status and current file anchors. |
| `dev/design-detailed-safe_export.md` | `no-longer-valid` | Describes old safe-export gaps as current. Archive; use current docs/tests instead. |
| `dev/design-detailed-supersession.md` | `needs-rewrite` | Important topic, but references archived typed-write design and old writer paths; rewrite against current supersession behavior. |
| `dev/design-external-content-memex-integration.md` | `up-to-date` | Still valid as future/application integration design. |
| `dev/design-external-content-objects.md` | `up-to-date` | Still valid as future design. |
| `dev/design-grouped-read-batching.md` | `needs-update` | Likely implemented; update status and stale `coordinator.rs` line anchors. |
| `dev/design-id-generation-policy.md` | `up-to-date` | Current policy design. |
| `dev/design-logging-and-tracing.md` | `up-to-date` | Current observability design. |
| `dev/design-note-encryption-at-rest-in-motion.md` | `up-to-date` | Current security boundary note. |
| `dev/design-note-telemetry-and-profiling.md` | `up-to-date` | Current telemetry design. |
| `dev/design-note-typescript-sdk.md` | `no-longer-valid` | TypeScript SDK design predates implementation. Archive or replace with current architecture. |
| `dev/design-operational-payload-schema-validation.md` | `up-to-date` | Current implemented-slice design. |
| `dev/design-operational-retention-scheduling.md` | `needs-update` | Current idea, but stale `admin.rs` anchors. |
| `dev/design-operational-secondary-indexes.md` | `up-to-date` | Current implemented-slice design. |
| `dev/design-prepared-write-representation.md` | `needs-update` | Still relevant, but stale writer module anchors. |
| `dev/design-projection-coverage.md` | `needs-update` | Still relevant, but old writer anchors and backfill wording need review. |
| `dev/design-provenance-policy.md` | `needs-update` | Mostly valid, but references future retire handling that now exists. |
| `dev/design-provenance-retention.md` | `up-to-date` | Current future design. |
| `dev/design-python-bindings.md` | `needs-update` | Large binding design predates current PyO3/FFI module split. |
| `dev/design-python-thread-safety.md` | `up-to-date` | Current design note. |
| `dev/design-read-execution.md` | `needs-update` | Still useful, but should be reconciled with current query compiler/coordinator code. |
| `dev/design-read-result-diagnostics.md` | `needs-update` | Still useful, but has old `coordinator.rs` anchors. |
| `dev/design-reader-connection-pool.md` | `needs-update` | Likely implemented; update status and old line references. |
| `dev/design-remaining-work-fathomdb-support-memex.md` | `no-longer-valid` | "Remaining work" note is stale after the 0.5.x work. Archive. |
| `dev/design-repair-provenance-primitives.md` | `needs-update` | Useful repair/provenance design, but should state which primitives shipped. |
| `dev/design-response-cycle-feedback.md` | `up-to-date` | Current feedback design. |
| `dev/design-restore-edge-validation.md` | `needs-update` | Relevant, but old `admin.rs` anchors and current restore behavior need reconciliation. |
| `dev/design-safe-export-manifest-atomicity.md` | `needs-update` | Relevant, but safe-export implementation has moved; update status and anchors. |
| `dev/design-schema-migration-safety.md` | `up-to-date` | Current migration-safety design. |
| `dev/design-shape-cache-bounds.md` | `needs-update` | Relevant, but old coordinator anchors. |
| `dev/design-structured-node-full-text-projections.md` | `needs-rewrite` | Old global `fts_node_properties` design conflicts with current per-kind `fts_props_<kind>` tables. |
| `dev/design-vector-regeneration-failure-audit-typing.md` | `needs-update` | Relevant, but old `admin.rs` anchor and current vector config shape need review. |
| `dev/design-wal-size-limit.md` | `up-to-date` | Current future design. |
| `dev/design-windows-executable-trust-support.md` | `no-longer-valid` | Subprocess vector generator surface was removed from the engine. Archive. |
| `dev/design-writer-thread-safety.md` | `needs-update` | Useful, but old `writer.rs` anchors should target `writer/mod.rs` or current code. |
| `dev/doc-governance.md` | `up-to-date` | Updated in the docs cleanup pass. |
| `dev/engine-vs-application-boundary.md` | `up-to-date` | Current boundary statement. |
| `dev/fathom-integrity-recovery.md` | `needs-update` | Mostly useful, but has stale link to `0.1_IMPLEMENTATION_PLAN.md` and should reflect current modules. |
| `dev/fathomdb-v1-path-to-production-checklist.md` | `needs-rewrite` | Still wired into hygiene checks, but `v0.1` framing is stale for a `0.5.3` repo. |
| `dev/pathway-to-basic-cypher-2026-04-17.md` | `up-to-date` | Current 0.6.0 Cypher design anchor. |
| `dev/plan-automatic-background-retention.md` | `needs-update` | Plan may still be relevant, but must be reconciled with current retention implementation. |
| `dev/plan-operational-payload-schema-validation.md` | `no-longer-valid` | Implementation plan for shipped work. Archive. |
| `dev/plan-operational-secondary-indexes.md` | `no-longer-valid` | Implementation plan for shipped work. Archive. |
| `dev/plan-structured-node-full-text-projections.md` | `no-longer-valid` | Old implementation plan for superseded global property FTS shape. Archive. |
| `dev/plan-typescript-sdk.md` | `no-longer-valid` | TypeScript SDK has shipped. Archive. |
| `dev/production-acceptance-bar.md` | `up-to-date` | Current production-gate policy after cleanup. |
| `dev/publish-steps.md` | `no-longer-valid` | One-off v0.1.0 publishing note. Archive. |
| `dev/pyo3-log-gil-deadlock-evidence.md` | `no-longer-valid` | Historical investigation tied to 0.1-era state. Archive. |
| `dev/release-policy.md` | `up-to-date` | Current release policy. |
| `dev/repair-support-contract.md` | `needs-update` | Current support contract, but still uses `v0.1` framing and should be refreshed. |
| `dev/restore-reestablishes-retired-projection-state.md` | `up-to-date` | Current restore/projection design. |
| `dev/schema-declared-full-text-projections-over-structured-node-properties.md` | `needs-rewrite` | Implemented status is useful, but old `fts_node_properties` table design is stale. |
| `dev/security-review.md` | `needs-update` | Findings are resolved, but code line anchors need current-module verification. |
| `dev/setup-admin-bridge.md` | `needs-update` | Setup doc likely useful, but should be checked against current bridge/CLI commands. |
| `dev/setup-round-trip-fixtures.md` | `needs-update` | Setup doc likely useful, but should be checked against current fixtures and SDK harness. |
| `dev/test-plan.md` | `needs-update` | Still valuable, but contains old module names (`python_types.rs`) and several stale/open test statuses. |

## `dev/notes/`

| File | Status | Reason / next action |
|---|---|---|
| `dev/notes/0.6.0-roadmap.md` | `up-to-date` | Current roadmap and aligned to the active Cypher design. |
| `dev/notes/adaptive-search-response-shape.md` | `needs-update` | Relevant but should be reconciled with shipped/current adaptive search result shape. |
| `dev/notes/adaptive-search-winning-branch.md` | `needs-update` | Relevant but should be reconciled with current adaptive search behavior. |
| `dev/notes/agent-harness-reference.md` | `needs-update` | Useful process doc, but references old `python_types.rs`. |
| `dev/notes/agent-harness-runbook.md` | `up-to-date` | Current orchestration runbook. |
| `dev/notes/design-0.5.4-async-rebuild-lifecycle-hardening.md` | `up-to-date` | Current 0.5.4 candidate. |
| `dev/notes/design-0.5.4-projection-identity-and-tokenizer-hardening.md` | `up-to-date` | Current 0.5.4 candidate. |
| `dev/notes/design-0.5.4-recover-destination-atomicity.md` | `up-to-date` | Current 0.5.4 candidate. |
| `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md` | `up-to-date` | New correct vector projection design. Treat as authoritative for database-wide embedding identity, per-kind vector indexing, and managed vector projection cleanup. |
| `dev/notes/design-text-query-ast-2026-04-11.md` | `no-longer-valid` | TextQuery exists in current code; this "current state" design is now historical. Archive. |
| `dev/notes/documentation-review-plan-2026-04-22.md` | `up-to-date` | Current documentation cleanup record. |
| `dev/notes/fts-level-three-2026-04-11.md` | `needs-rewrite` | Research is useful, but baseline says object values and per-field weighting are unavailable and uses old `fts_node_properties`. |
| `dev/notes/implementation-plan-text-query-tdd-2026-04-11.md` | `no-longer-valid` | Implementation plan for shipped TextQuery work. Archive. |
| `dev/notes/improving-with-better-tokenization-2026-04-11.md` | `no-longer-valid` | Already marked superseded; should not remain active. Archive. |
| `dev/notes/investigation-edge-properties-20260417.md` | `no-longer-valid` | Says no `EdgeRow` / edge property support exists; both shipped in 0.5.x. Archive. |
| `dev/notes/project-vector-identity-invariant.md` | `up-to-date` | Current architectural invariant. |
| `dev/notes/pyo3-0.28-upgrade-plan.md` | `no-longer-valid` | PyO3 0.28 is already in the workspace; archive as historical upgrade plan. |
| `dev/notes/release-checklist.md` | `needs-update` | Useful release checklist, but appendix still references old `vec_nodes_active` incident wording. |
| `dev/notes/spec-supported-query-primitives-and-operators-2026-04-11.md` | `up-to-date` | Current text query spec. |
| `dev/notes/wake-enhancements.md` | `needs-update` | Useful exploratory note, but should be aligned with current tokenizer/profile and adaptive-search behavior. |

## Recommended Next Pass

1. Archive every `no-longer-valid` file listed above.
2. Rewrite the 7 `needs-rewrite` docs as current-state documents, or archive
   them if the topic is already covered elsewhere.
3. For `needs-update`, prioritize mechanical fixes first:
   - replace stale module paths (`admin.rs`, `writer.rs`, `python_types.rs`)
   - replace old global FTS/vector table names where not explicitly historical
   - add status banners: `Implemented`, `Current`, `Draft`, `Historical`
4. Re-run:

```bash
python3 scripts/check-doc-hygiene.py
bash docs/build.sh
rg -n "python_types\\.rs|vec_nodes_active|fts_node_properties|crates/fathomdb-engine/src/admin\\.rs|crates/fathomdb-engine/src/writer\\.rs" dev/*.md dev/notes/*.md
```
