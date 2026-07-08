# OPP-12 C-1 — converged co-design contract (EntityTypeSpec → ProjectionSpec)

> **Status: CONVERGED (2-shot loop closed) → RATIFIED-pending-HITL.** FathomDB-side design home + seed for the
> joint **FathomDB 0.9.1 P2·S0 ADR**. Product of the bounded two-shot loop on `OPP-12-sub-ledger.jsonl`
> (seq 1 protocol · seq 2 SHOT-1/FATHOM · seq 3 SHOT-2/MEMEX · seq 4 reconcile/FATHOM). Memex SHOT-2 substance +
> `file:line` grounding: memex `dev/design/entity-schema-registry/ADR-C1-eav-projection-lifecycle.md`.
> **Freezes nothing to build now (build ≠ adopt).** C-1 co-lands at FathomDB **0.9.1 (P2·S-F)** / a coordinated
> Memex 0.5.x-successor pair. HITL-directed 2026-07-07.

## Cohesion seam (governs every decision)

FathomDB owns **structure & mechanism** (EAV storage, property-FTS, indexes, surrogate-id value-minting,
projection build/drop). Memex owns **semantics & judgment** (which attributes exist, what they mean, which
roles they take, the promotion gate). The LLM stays Memex-side.

## Agreements (Q1 / Q3 / Q5)

- **Q1 — role source: optional `AttributeSpec.index: Set[ProjectionRole]`.** Absent → engine `py_type`
  inference (0.9.1 default). **The carrier LANDED inert in Commission C** (memex `95ed450`, persisted,
  byte-identical round-trip) — the one break-if-later field, frozen while the seam is fresh. Role *values* are
  assigned later per domain; only the field's *presence* is the forward-compat win.
- **Q3 — drift authority: engine is the sole projection authority** via `configure_projections` diff/backfill.
  Memex runs only the pure `derive: EntityTypeSpec → [ProjectionSpec]` and holds no projection state. An
  incompatible change on an existing `name` surfaces a **destructive delta requiring explicit `drop`**
  (omission ≠ drop).
- **Q5 — persist boundary: Memex persists the spec; the engine `ProjectionSpec` is a DERIVED cache**, re-driven
  idempotently on boot (`load_from_store → derive → configure_projections`). One durable source of truth; drift
  resolves **to the Memex spec**; no two-registry sync.

## Q2 — EAV convergence: ONE engine-owned EAV (sequenced, no over-build)

End-state: a single **engine-owned EAV attribute store + property-FTS**. Phasing:

| Phase | Attributes live | Memex builds | Engine builds |
|---|---|---|---|
| Today | nested dict on the governed `WMEntity` node (`fathom_store.py:4635`) | interim already ships | `body`-FTS only |
| Commission C (0.5.x, now) | same nested-dict interim | carries the optional `index` hint; does **not** promote the `WorldModelEntityAttribute` DTO into a persisted table | nothing (PLAN-C §7 decouple boundary) |
| C-1 co-land (0.9.1) | **engine EAV + property-FTS** | repoints `set_entity_attribute` onto the governed engine-EAV verb; retires the interim | builds EAV + property-FTS once; projects via `configure_projections` |

- **Forbidden over-build (Memex):** a *persisted* standalone attribute node/table. The `WorldModelEntityAttribute`
  DTO stays a read-materialized facade shape, not a stored table.
- **Breaking, no data migration (explicit):** at 0.9.1 the write path repoints; existing nested-dict attributes
  are **not** migrated — entities are re-created under the engine-EAV model going forward. Memex owns this data
  call (breaking-OK stance).
- Property-level projection does not exist until 0.9.1 (recall rides `body`-FTS + R-B until then — a scope line,
  not a gap).

## Q4 — provisional / promotion projection timing (tiered by cost)

Policy (Memex-owned; the promotion gate is a **design commitment, not yet built**):

- Cheap same-transaction projections (`filterable`, `searchable→FTS`) build **immediately** for provisional
  types (instant lexical/filter recall for the adaptive loop).
- The expensive `searchable→vector` (async embed + `dense_readiness` + atomic flip) **defers to `promote()`**.
- A rejected provisional drops **only** the cheap projections → no embedding rebuild-churn.

**Mechanism refinement (FathomDB, P2·S0 confirm — does not change Memex policy):** the **engine needs no
`provisional` concept.** The tiering is expressed entirely through **Memex's role-declaration timing + explicit
`drop`** — declare cheap roles now, add the `vector` role at `promote()`, drop on reject. This is the **same
graceful-graft mechanism as Q6a**: the engine idempotently builds/drops the roles it is currently declared,
nothing more. Policy stays 100% Memex-side; the engine stays mechanical.

## Q6 — rankable / F9 + surrogate id

- **(a) rankable = graceful-absent, never blocking.** Co-land `filterable` + `searchable` at 0.9.1. Memex MAY
  declare `rankable` roles now; the engine **defers** any `rankable` projection it cannot yet honor (F9 signal
  algebra ~0.8.16) — no error, no build — and grafts it on the **next idempotent `configure_projections`** once
  F9 exists. (Same mechanism as the Q4 refinement.)
- **(b) `id_prefix` = space, engine mints the value.** `EntityTypeSpec.id_prefix` names the `IdSpace`/namespace
  (Memex vocab; uniform `WM_ENTITY` today, per-type is intended-future); the engine mints the opaque surrogate
  `logical_id` **value** within it for anonymous entities. The **typed carrier** (`l:`/`h:`/`p:` → `IdSpace`
  newtype) is **OPP-12 C-2 / TC-8** (lands-together with the Cause-A `SearchHit.id: write_cursor → logical_id`
  swap, ≥0.9.x; additive `stable_id` base landed, typed swap not started). **C-1 needs only the space/value
  split, not the typed carrier.**

## Resolved now (contract-level) — previously flagged for P2·S0

Both items Memex flagged for deferral have a contract-level answer that **follows from the agreements above**;
only FathomDB-internal *implementation* mechanics remain for the 0.9.1 build slice (not a co-design item).

### Apply atomicity — RESOLVED: persist-first, idempotent, boot-heal (no cross-step runtime transaction)

Follows from Q5 (Memex spec = sole durable source; `ProjectionSpec` = derived cache) + the OPP-12 async model:

1. **Persist-first.** `register()` durably writes the Memex spec (`WMEntityTypeSpec`) *before* any projection work.
2. **Then `derive → configure_projections`**, which is idempotent (Q3 diff/backfill).
3. **Per-projection atomicity is already specified:** cheap roles (`filterable`, `searchable→FTS`) apply
   same-transaction; `searchable→vector` is async with the atomic `dense_readiness` flip (torn
   `ready`-without-vector forbidden).
4. **`register()` does not block on embedding** — it returns once the spec is persisted, cheap projections
   applied, and vector projections enqueued (`dense_readiness=embedding`).
5. **Crash-heal.** Because the spec is durable and `configure_projections` is idempotent, a crash at any point
   self-heals on the next boot `load_from_store → derive → configure_projections` (diffs actual vs desired,
   backfills the gap). Worst case is incomplete projections in the crash→restart window — surfaced by the
   existing partial-dense / `dense_readiness` read signals, acceptable for the local-first single-user envelope.
   **No cross-step runtime transaction is required** — this fully satisfies Memex's SHOT-2 requirement
   (idempotent + crash-safe boot re-derive).

*P2·S0 (FathomDB-internal build detail only, not co-design):* the exact SQLite transaction boundary for the
same-transaction tier and the worker-enqueue ordering.

### Tokenizer / embedder defaults — RESOLVED: engine defaults; custom rides the separate FTS work

- **Default embedder** = the engine's shipped default (CLS-corrected bge-small today); Memex's `vector:{embedder}`
  is an optional override, absent → engine default. Already how the engine works — no new imposition (Memex uses
  bge-small today).
- **Default tokenizer** = the engine's default FTS5 tokenizer (the one `body`-FTS uses); `fts:{tokenizer}` is an
  optional override, absent → engine default.
- **Custom / per-kind precision tokenizers** are the separately-tracked **≥0.9.x multi-field / per-kind-tokenizer
  FTS** work (`multifield-fts-deferred-0.9.x`), **not a C-1 open item** — C-1 co-lands on the default tokenizer,
  and custom tokenizers graft later via the same idempotent `configure_projections` (same graceful-graft pattern
  as Q4 / Q6a).

**Net: nothing is genuinely open at the contract level.** The residual P2·S0 work is pure FathomDB-internal 0.9.1
implementation (transaction boundaries) — which every build slice has, and is not a co-design deferral.

## Landing

Co-lands **FathomDB 0.9.1 (P2·S-F)** ↔ a coordinated **Memex 0.5.x-successor**. Build ≠ adopt; publish is a
separate HITL gate on an even `x.y.z`. Sequencing unchanged (`0.8.16 F9 → 0.8.18 publish → 0.9.0 → 0.9.1`,
master F-18). The Commission C `index` carrier is the only piece that had to move now, and it has (memex `95ed450`).
