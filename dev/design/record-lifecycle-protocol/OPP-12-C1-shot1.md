# FathomDB C-1 co-design — SHOT 1 (FATHOM opens)

> **Status: DRAFT, FATHOM-voice, HITL-review-pending before Memex routing.** Full text of the SHOT-1 entry in
> `OPP-12-sub-ledger.jsonl`. Two-shot bounded co-design loop for OPP-12 residual **C-1** (Memex `EntityTypeSpec`
> DRIVES engine `ProjectionSpec`; one registration flow). Grounded in `record-lifecycle-protocol/{projection-registry-and-async-embed.md, api-surface.md}`
> (FathomDB, builds 0.9.1 P2·S-F) + Memex `dev/design/entity-schema-registry/DESIGN-0.5.x-runtime-entity-schema-registry.md`
> (Commission C, landing NOW). **Freezes nothing; forces the mapping decisions before Commission C locks `AttributeSpec`.**

## 1. ProjectionSpec surface FathomDB will build (0.9.1 P2·S-F)

- **`configure_projections(spec, drop?)`** — declarative, idempotent apply; engine **diffs against `read.projections` + backfills**. Drop is EXPLICIT (omission ≠ drop; destructive delta surfaced first).
- **`ProjectionSpec { name, roles: Set<ProjectionRole>, fts?:{tokenizer}, vector?:{embedder} }`**, `ProjectionRole ∈ {filterable, rankable, searchable}` (a **set** — an attribute can be filterable AND searchable; named `roles` not `kind`, S8 fix). `fts`/`vector` sub-objects select the `searchable` sub-target.
- **Projects FROM** the net-new **EAV attribute store + property-FTS** (only `body`-FTS exists today); wraps the shipped **EXP-S per-kind index-target seam (TC-1)**.
- **Staleness split:** `filterable` + `searchable→FTS` = **same-transaction**; `searchable→vector` = **async, rebuild-durable**, carries `dense_readiness ∈ {ready, embedding}` with the **atomic `{vector-insert ∧ dense_readiness:=ready}` flip**.
- **Net-new vs shipped:** the async projection worker (`projection_dispatcher_loop`/`projection_worker_loop`, `engine:876-887`) **already exists and is kept**; net-new = the EAV store + property-FTS, `dense_readiness`, `drain(timeout_ms)` reused as the quiescence barrier, the readiness-flip, `configure_projections`/`ProjectionSpec`.

## 2. Proposed EntityTypeSpec → ProjectionSpec mapping (the ONE flow)

Memex `EntityTypeSpec` (`name`, `id_prefix`, `attributes: tuple[AttributeSpec…]`, `relationships`); `AttributeSpec` (`key`, `group`, `py_type ∈ {str|int|bool|float|json}`, `required`, `default`, `multi`).

| Memex source | → | FathomDB target | Rule (proposed) |
|---|---|---|---|
| `EntityTypeSpec.name` | → | per-kind index-target namespace (TC-1) | one projection group per registered type |
| `AttributeSpec.key` | → | `ProjectionSpec.name` | one ProjectionSpec per projected attribute |
| `py_type ∈ {int,bool,float}` / scalar `str` | → | `roles ∋ filterable` | cheap equality/range; default for scalars |
| `py_type == "str"` free-text | → | `roles ∋ searchable`, `fts:{tokenizer}` | property-FTS; tokenizer default unless declared |
| graded/dial attr (confidence/salience) | → | `roles ∋ rankable` | **F9 signal algebra — OPEN until ~0.8.16 (Q6)** |
| `EntityTypeSpec.relationships` | → | typed-edge filterable projection | edges already in `canonical_edges`; vocab Memex-owned |
| `id_prefix` (CR-059) | → | `IdSpace` / `l:` logical_id space | reconcile with engine surrogate minting (Q6) |

**Gap flagged (honest, not invented): `AttributeSpec` carries NO role field today** (verified: Memex EAV = group/key/value, no projection-role). So role acquisition is either engine-inferred-from-`py_type` (lossy) or requires an explicit hint Memex must add — this is Q1 and the load-bearing Commission-C-timing decision.

**Flow (single path, all reconciliation in the engine):**

1. Memex `register(EntityTypeSpec, persist=True)` → app-side validate + write-through to `WMEntityTypeSpec` governed node (**R-A persist**).
2. Registry runs pure fn `derive: EntityTypeSpec → [ProjectionSpec]` (the §2 table).
3. Registry calls engine `configure_projections(specs, drop?)` — engine **diffs vs `read.projections`**, backfills new/changed, no-ops unchanged (**idempotent**).
4. Engine applies: `filterable` + `searchable→FTS` same-transaction; `searchable→vector` enqueued on the existing worker with `dense_readiness=embedding`.
5. **Boot:** `load_from_store()` replays persisted specs → re-derive → `configure_projections` (idempotent). **This is where drift between the two registries would surface — must resolve to engine-as-authority, never a two-way sync.**
6. `provisional` types: projection timing is **Q4** (project immediately for the adaptive loop, or defer to `promote()`?).

## 3. Ownership seam (confirm)

| Memex-owned (type + semantics + declarations) | FathomDB-owned (projection mechanism) |
|---|---|
| `EntityTypeSpec` / `AttributeSpec`, `register()` | `configure_projections` / `ProjectionSpec` / `ProjectionRole` |
| `id_prefix` vocab (CR-059), `relationships` vocab | `IdSpace`, surrogate `logical_id` minting |
| promotion / quarantine gate (`provisional`→canonical) | EAV attribute store + property-FTS |
| R-A: persist `WMEntityTypeSpec` + boot-reload | `dense_readiness` + atomic readiness-flip; async worker |
| which attributes mean what (dials, edges) | diff / backfill / idempotency / drift authority |

## 4. Convergence questions for Memex (BOUNDED — max 6)

- **Q1 — Role source (highest leverage).** Does `AttributeSpec` carry an **explicit optional role hint** (e.g. `index: Set[ProjectionRole]`), engine `py_type`-inference as the default when absent? FathomDB's `ProjectionSpec` needs a role source and has none defined. **Recommend: add the optional field NOW in Commission C** — adding it post-lock re-serializes every persisted `WMEntityTypeSpec`.
- **Q2 — EAV convergence (load-bearing, under-specified both sides).** Memex writes app-side EAV (`WorldModelEntityAttribute`). FathomDB's registry projects from a **net-new ENGINE EAV**. Converge on **one engine-owned EAV** (Memex stops writing app-side attribute nodes), or projection reads *from* Memex's governed-node EAV? "Converges" is named with no mechanism.
- **Q3 — Idempotent re-registration + drift authority.** Confirm **engine is the sole projection authority** via `configure_projections` diff (Memex never runs a second projection registry). On incompatible change (embedder/tokenizer swap on an existing `name`), engine surfaces a **destructive delta requiring explicit `drop`** — accepted?
- **Q4 — provisional / promotion interaction.** Do `provisional=True` types get projections **immediately** (adaptive-loop recall) or only on `promote()`? If immediate, does a **rejected** provisional trigger a projection `drop` (rebuild churn)?
- **Q5 — R-A persist boundary.** Proposal: **Memex persists the spec (`WMEntityTypeSpec`); the engine's ProjectionSpec is a DERIVED cache re-driven idempotently on boot** — engine never persists a spec Memex can't re-derive → one durable source of truth, no second persistent registry to sync. Confirm.
- **Q6 — `rankable`/F9 + surrogate id.** (a) F9 lands ~0.8.16; can C-1 co-land (0.9.1) ship **filterable + searchable only**, `rankable` grafting when F9 lands? (b) How does `EntityTypeSpec.id_prefix` reconcile with the engine-minted surrogate `logical_id` for entities that begin anonymous?

## 5. Co-land / timing ask

**Freeze NOW in Commission C (does NOT foreclose C-1):** the `EntityTypeSpec`/`AttributeSpec` shape, JSON serialization contract, R-A persistence, `register()` as the single entry point. Matches Commission C's own decouple boundary ("build the seam so C-1 grafts on later; do NOT wire the engine projection now").

**The one forward-compat ask:** land the **optional per-`AttributeSpec` role hint (Q1) in Commission C** even though inert until 0.9.1 — the only field whose later addition is a **breaking re-serialization** of every persisted spec.

**Must wait for FathomDB 0.9.1 (P2·S-F):** `configure_projections`/`ProjectionSpec`, the engine EAV + property-FTS, `dense_readiness`. Memex builds no engine projection now.

**Decision forced this loop:** agree **Q1 (role hint + role source)** and **Q5 (engine-as-derived-cache, Memex-spec-as-sole-durable-source)** before Commission C locks `AttributeSpec`. Q2/Q3 may co-design to the P2·S0 ADR (0.9.1) but flag open now so neither side over-builds an EAV the other discards.

## Honest open items in the FathomDB design (not invented)

1. No role-source defined on `AttributeSpec`→`ProjectionRole` (Q1); 2. register→derive→`configure_projections` not specified as one atomic transaction — only the vector readiness-flip atomicity is (flag for P2·S0); 3. EAV convergence named, no mechanism (Q2); 4. drift-detection beyond "diffs + backfills" unspecified (Q3); 5. `rankable`/F9 open until ~0.8.16.
