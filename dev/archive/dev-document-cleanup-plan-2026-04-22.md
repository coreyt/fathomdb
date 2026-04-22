# Dev Documentation Cleanup Plan - 2026-04-22

**Status:** Completed in branch `doc-cleanup-2026-04-22`

## Inputs

Use these documents as the cleanup source material:

- `dev/notes/dev-document-status-audit-2026-04-22.md`
- `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md`
- `dev/notes/documentation-review-plan-2026-04-22.md`

The database-wide embedding / per-kind vector indexing design is new and
correct. Treat it as authoritative for vector-related developer documentation.
Older docs that imply caller-managed vector tables, per-kind embedding engines,
raw vector insertion as the normal application path, or `configure_vec()` as
both global identity and per-kind readiness must be updated, rewritten, or
archived.

## Desired End State

- `dev/` contains current architecture, current support contracts, current
  designs, and live operational/developer references.
- `dev/notes/` contains current planning and active design-change notes.
- `dev/archive/` contains completed implementation plans, stale investigations,
  old release plans, superseded designs, and historical evidence.
- Every active document has an explicit status banner or a clearly current
  purpose statement.
- Vector docs consistently reflect:
  - one database-wide embedding identity;
  - per-kind vector indexing enablement and text source configuration;
  - per-kind sqlite-vec tables as an implementation detail;
  - FathomDB-managed async/incremental vector projection as the target design;
  - raw `VecInsert` as internal/admin/unsafe, not the normal application path.

## Phase 1: Archive `no-longer-valid`

Move these files to `dev/archive/` unless a replacement is created in the same
patch:

- `dev/SDK-typescript-spec.md`
- `dev/design-detailed-safe_export.md`
- `dev/design-note-typescript-sdk.md`
- `dev/design-remaining-work-fathomdb-support-memex.md`
- `dev/design-windows-executable-trust-support.md`
- `dev/plan-operational-payload-schema-validation.md`
- `dev/plan-operational-secondary-indexes.md`
- `dev/plan-structured-node-full-text-projections.md`
- `dev/plan-typescript-sdk.md`
- `dev/publish-steps.md`
- `dev/pyo3-log-gil-deadlock-evidence.md`
- `dev/notes/design-text-query-ast-2026-04-11.md`
- `dev/notes/implementation-plan-text-query-tdd-2026-04-11.md`
- `dev/notes/improving-with-better-tokenization-2026-04-11.md`
- `dev/notes/investigation-edge-properties-20260417.md`
- `dev/notes/pyo3-0.28-upgrade-plan.md`

After the move, update links in active docs. Do not chase or rewrite links
inside `dev/archive/` unless they affect current entry points.

## Phase 2: Vector Documentation Reconciliation

Use `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md`
as the source of truth.

Update or rewrite:

- `dev/arch-decision-vector-embedding-recovery.md`
- `dev/repair-support-contract.md`
- `dev/design-vector-regeneration-failure-audit-typing.md`
- `dev/fathom-integrity-recovery.md`
- `dev/test-plan.md`
- `dev/ARCHITECTURE.md`
- `dev/notes/project-vector-identity-invariant.md`
- `dev/notes/release-checklist.md`

Required cleanup:

- Replace old `v0.1` vector recovery framing with current release-line language.
- Remove active-doc assumptions that application code normally creates vec
  tables or submits raw vector rows.
- Clarify the difference between current implemented behavior and the new
  managed-vector target design.
- Rename old table-specific language (`vec_nodes_active`) to per-kind vector
  tables unless the text is explicitly historical.
- Make `configure_vec`, `get_vec_profile`, and future `configure_embedding` /
  `get_vec_index_status` wording unambiguous.

## Phase 3: Rewrite `needs-rewrite`

Rewrite or archive these seven documents:

- `dev/architecture-note-memex-support.md`
- `dev/db-integrity-management.md`
- `dev/design-detailed-supersession.md`
- `dev/design-structured-node-full-text-projections.md`
- `dev/fathomdb-v1-path-to-production-checklist.md`
- `dev/schema-declared-full-text-projections-over-structured-node-properties.md`
- `dev/notes/fts-level-three-2026-04-11.md`

Preferred outcomes:

- Memex support becomes a short current integration-status document that points
  to active generic designs instead of carrying old gap analysis.
- DB integrity becomes a current playbook or is folded into
  `dev/dbim-playbook.md`.
- Supersession becomes a current contract doc grounded in existing behavior and
  current restore/projection semantics.
- Structured property FTS docs stop describing the old global
  `fts_node_properties` design as active and either become a current
  per-kind-FTS contract or historical archive material.
- The production checklist is reframed away from `v0.1` or replaced with a
  current production/release readiness tracker compatible with
  `scripts/check-doc-hygiene.py`.

## Phase 4: Mechanical `needs-update`

For the remaining `needs-update` docs, make narrow corrections:

- Replace stale module paths:
  - `crates/fathomdb-engine/src/admin.rs` -> current `admin/` module paths.
  - `crates/fathomdb-engine/src/writer.rs` -> `writer/mod.rs` or
    `writer/fts_extract.rs`.
  - `crates/fathomdb/src/python_types.rs` -> `crates/fathomdb/src/ffi_types.rs`
    where appropriate.
- Replace old active table names:
  - `vec_nodes_active` -> per-kind vector table wording.
  - `fts_node_properties` -> per-kind `fts_props_<kind>` wording, unless
    describing compile-time placeholder SQL or historical migrations.
- Add status banners:
  - `Status: Current`
  - `Status: Implemented; retained as design rationale`
  - `Status: Draft`
  - `Status: Historical; move to archive`
- Update links to point to `dev/archive/...` only when referencing historical
  context intentionally.

## Phase 5: Governance And Entry Points

Update:

- `dev/doc-governance.md`
- `README.md` if the `dev/` layout wording changed
- `docs/index.md` only if public docs now point to different developer docs
- `scripts/check-doc-hygiene.py` if the production checklist path or headings
  change

Consider adding a lightweight machine-readable status convention later, but do
not block this cleanup on new tooling.

## Phase 6: Verification

Run after each cleanup batch:

```bash
python3 scripts/check-doc-hygiene.py
bash docs/build.sh
```

Run after the vector cleanup batch:

```bash
rg -n "vec_nodes_active|raw vector|VecInsert|configure_vec|get_vec_profile|VectorRegenerationConfig" dev/*.md dev/notes/*.md docs
```

Run after mechanical path cleanup:

```bash
rg -n "python_types\\.rs|crates/fathomdb-engine/src/admin\\.rs|crates/fathomdb-engine/src/writer\\.rs|fts_node_properties|vec_nodes_active" dev/*.md dev/notes/*.md
```

Expected final active-doc shape:

- No `no-longer-valid` files remain outside `dev/archive/`.
- `needs-rewrite` count is zero: each file is either rewritten or archived.
- Remaining `needs-update` items are documented exceptions or explicitly
  scheduled follow-up work.
