---
title: ADR-0.8.11-filter-grammar-unification
date: 2026-06-28
target_release: 0.8.11
desc: Unify the two SHIPPED filter surfaces — G4 `Predicate` (json-path predicates over canonical_nodes `body`, surfaced on `read.list`) and G10 `SearchFilter` (closed 4-field struct over vec0 metadata columns, surfaced on `search_filtered`) — into ONE typed `Filter` contract with TWO internal compilation backends (canonical_nodes/json_extract and vec0-metadata pre-KNN WHERE). Option A (HITL-decided): a single superset type that dispatches; NOT a thin adapter over two types. Total backend-dispatch with typed rejection — every predicate has a defined outcome (compiles to backend X, or typed-rejects with a stated reason) in EACH context; no silent fallback, no demotion of an indexed pre-KNN predicate to a slow post-KNN json_extract. Closes reserved-gap 37.
blast_radius: src/rust/crates/fathomdb-engine/src/lib.rs (SearchFilter struct + vec0 compilation; Predicate enum + read_list compilation; the two are joined under one Filter type + dispatch); src/rust/crates/fathomdb-py/src/lib.rs + src/rust/crates/fathomdb-napi/src/lib.rs (SDK lowering); src/python/fathomdb/{types.py,read.py} + src/ts/src/{index.ts,read.ts} (public SearchFilter + Predicate re-expressed as sugar; no behavior change); tests/pr_g10_filtered_knn.rs + tests/slice35_filter_grammar.rs (parity fixture + RED→GREEN). The slice-10 `filter=None → byte-identical 0.7.2 SQL` pin MUST keep holding.
status: proposed (Slice 0 deliverable; provisional pending HITL confirmation of Option A)
origin: dev/plans/plan-0.8.11.md Track G / Slice 40 (#17 filter-grammar unification); ADR-0.8.0-filter-grammar.md reserved-gap 37 (HITL 2026-06-06: "unification is NEEDED, not optional"); MEMORY planner-router-experiment-ladder → folded into 0.8.11
inherits: ADR-0.8.0-filter-grammar (G4 closed Predicate enum D-F1..D-F5; EXCLUDE list — no DSL/no-fused/no-_unchecked/parameterized-only/allowlisted-paths); dev/design/slice-10-design.md (G10 SearchFilter, shipped Slice 10; filter=None byte-identity pin); ADR-0.8.0-canonical-identity-substrate (folded canonical_nodes(kind) index; active = superseded_at IS NULL on logical_id-alone)
---

# ADR-0.8.11 — Filter-grammar unification (G4 + G10)

**Status:** proposed (Slice 0 deliverable; provisional pending HITL confirmation of
Option A). This ADR **specifies** Slice 40; it ships **no** production code or test
(Slice 40 implements). It closes **reserved-gap 37** (ADR-0.8.0-filter-grammar
D-F3), the committed-not-optional full single-grammar unification.

> **HITL decision inherited (do not re-litigate):** **Q1 = Option A** — ONE unified
> typed `Filter` contract with **TWO internal compilation backends**, NOT a thin
> adapter over two types. The TYPE unifies and dispatches; the data does **not**
> funnel into one SQL, because the same logical predicate filters **two different
> physical stores**.

---

## 1. Context — what is being unified, and the load-bearing finding

Two filter surfaces shipped in 0.8.0 and are **both already built and SDK-exposed**:

- **G10 `SearchFilter`** (shipped Slice 10): a **closed struct**
  `{source_type, kind, created_after, status}`
  (`src/rust/crates/fathomdb-engine/src/lib.rs:1487-1493`), threaded as
  `Option<SearchFilter>` through `Engine::search_filtered`, compiled to an **indexed
  pre-KNN `WHERE` over vec0 metadata columns** co-located with the vectors in the
  `vector_default` virtual table
  (`vector_filter_clause` `lib.rs:5469-5494`, `vector_filter_values`
  `lib.rs:5499-5521`, `build_vector_phase1_sql` `lib.rs:5528-5545`). The pre-KNN
  filter is the perf point (prune *before* the bit-KNN, not after).
- **G4 `Predicate`** (shipped Slice 35 — **NOT deferred**): the closed enum
  `{JsonPathEq, JsonPathCompare}` with `ComparisonOp{Gt/Gte/Lt/Lte}` and
  `ScalarValue{Text/Integer/Bool}` (`lib.rs:1277-1330`), validated against the
  `PREDICATE_PATH_ALLOWLIST` (`lib.rs:1310-1311`) at construction, surfaced on
  `Engine::read_list(kind, predicates, limit)` (`lib.rs:501-506`), compiled to
  **`json_extract(body,'$.path') <op> ?`** over `canonical_nodes` rows
  (`Predicate::to_sql_clause` `lib.rs:1389-1426`; `read_list_in_tx` `lib.rs:6436-6494`).

> **Load-bearing investigation finding (corrects the slice premise).** The Slice 40
> brief described G4 as "NOT YET BUILT, deferred." **It is built.** `Engine::read_list`,
> the `Predicate` enum + validated constructors, `read_list_in_tx`, the path
> allowlist, and Py/TS SDK surfaces (`src/python/fathomdb/read.py`,
> `src/ts/src/read.ts:68-72`, `_fathomdb.pyi`, with conformance + functional tests)
> all exist on `0.8.11`. **Both backends already compile correctly and independently.**
> This makes Slice 40 a **TYPE-unification + dispatch refactor over two shipped
> compilation paths** — not a from-scratch build — which *strengthens* the
> anti-laziness mandate: there is no "not-built-yet" excuse for a reserved-gap.

**Why two backends, not one SQL:** the two surfaces filter **different physical
stores**. G10 filters indexed metadata columns *inside the vec0 virtual table*
(`source_type TEXT partition key, kind TEXT, created_at INTEGER, status TEXT` —
`vector_partition_create_sql` `lib.rs:8439-8451`), pre-KNN. G4 filters JSON *inside
`canonical_nodes.body`* via `json_extract` — no vectors involved. Collapsing into a
single SQL would force G10 onto `json_extract` (losing its indexed pre-KNN filter — a
perf regression) or G4 onto only four metadata columns (losing arbitrary json-path).
So: **the TYPE unifies; the COMPILATION dispatches** to one of two backends.

**Inherited (fixed inputs, not re-litigated):** ADR-0.8.0-filter-grammar D-F1..D-F5
(closed grammar, no DSL, no fused, no `_unchecked`, parameterized-only, allowlisted
paths, implicit-AND); slice-10 `filter=None → byte-identical 0.7.2 SQL` pin
(`slice-10-design.md:122-130,204-211`); the folded `canonical_nodes(kind)` index +
`superseded_at IS NULL` active-row rule.

---

## 2. Open questions (the "RED" — each settled below, no bare TBD)

- **Q1 — Shape of the unified `Filter` type.** Enum? variants? how do G10's four
  fields express, and how does the closed grammar stay closed? *(HITL: Option A.)*
- **Q2 — Field-availability matrix.** For every filterable field, which physical
  store(s) hold it? Is any field resolvable in only ONE store?
- **Q3 — Total backend-dispatch + typed-rejection rule.** What does each surface
  accept vs typed-reject, and how is the indexed pre-KNN guarantee preserved with
  no silent demotion?
- **Q4 — Backward compat.** How do the shipped public `SearchFilter` + `Predicate`
  SDK shapes survive unchanged, and how does the byte-identity pin keep holding?
- **Q5 — Parity fixture + RED→GREEN plan** for Slice 40.
- **Q6 — Reserved-gap trigger.** The ONE precise condition under which Slice 40 may
  fall back to a reserved-gap, and the assertion that dispatch-avoidance is not it.

---

## 3. Decision (one concrete answer per question)

### D1 — The unified `Filter` type (Option A: one superset, closed grammar)

Slice 40 introduces ONE closed superset type. It reuses the **already-shipped**
shared vocabulary (`ScalarValue`, `ComparisonOp`, `Predicate`) verbatim — no new
scalar/op grammar — and adds the four G10 shorthand fields as first-class terms:

```text
Filter (closed)  =  { terms: Vec<FilterTerm> }      // implicit AND (inherits D-F5)

FilterTerm (closed enum):
  // The G10 four — "shorthand" metadata fields. Each LOWERS to the right
  // physical form per backend (see D2/D3); shared across both surfaces.
  SourceType(String)
  Kind(String)
  CreatedAfter(i64)            // created_at >= bound (unix seconds)
  Status(String)
  // The G4 general json-path predicate (UNCHANGED shipped grammar, D-F1):
  Json(Predicate)             //  Predicate ∈ { JsonPathEq | JsonPathCompare }
                              //  over PREDICATE_PATH_ALLOWLIST, parameterized
```

- **Closed grammar preserved (inherit ADR-0.8.0):** no DSL, no caller SQL, no
  `JsonPathFused*`, no `*_unchecked`, no OR/nesting (implicit AND only); `Json`
  terms are constructed ONLY through the validated `Predicate::json_path_eq` /
  `Predicate::json_path_compare` (allowlist enforced at construction,
  `lib.rs:1337,1349`); values always bind as `?`.
- **G10's four fields are dedicated shorthand variants, not arbitrary json**, so
  they can lower to the *indexed* vec0 columns on the search backend (D3). They are
  NOT `Json(Predicate)` over `$.source_type` etc. — keeping them typed lets the
  search backend accept them while typed-rejecting arbitrary `Json` terms.
- **Naming/exhaustiveness is a closed set** — an exhaustiveness test pins
  `FilterTerm` to exactly these five variants (mirrors D-F1's enumeration test).

*(Slice-40 impl detail, marked TBD: whether `FilterTerm::Kind` inside `Filter` is
accepted as a redundant term or constant-folded against the `read.list(kind=…)`
partition argument. Both are total; pick at impl time. Not a grammar question.)*

### D2 — Field-availability matrix (the load-bearing investigation)

Physical stores investigated:

- **vec0 `vector_default` metadata columns** (`vector_partition_create_sql`
  `lib.rs:8439-8451`): `source_type` (partition key), `kind`, `created_at` (INTEGER),
  `status` (TEXT). Filterable **pre-KNN** in the phase-1 `MATCH … WHERE`.
- **`canonical_nodes` real columns** (`fathomdb-schema/src/lib.rs:110,198,300-301`):
  `write_cursor`, `kind`, `body` (JSON), `source_id`, `logical_id`, `superseded_at`.
  The only user-facing filterable real column is **`kind`**; everything else of
  interest lives inside **`body` JSON** (allowlist `$.status,$.priority,$.tags,$.kind,$.created_at`,
  `lib.rs:1311`). `source_type` is **NOT stored** in canonical_nodes — it is a
  deterministic function of `kind` via `resolve_source_type` (`lib.rs:8640-8655`).

| Filterable field | vec0 (`search_filtered`) | canonical_nodes (`read.list`) | Resolves in both? |
|---|---|---|---|
| **kind** | real metadata col `kind` ✓ (pre-KNN) | real col `canonical_nodes.kind` ✓ (and the partition arg itself) | **YES** |
| **created_after** (`created_at`) | real metadata col `created_at` INTEGER ✓ (pre-KNN) | `json_extract(body,'$.created_at') >= ?` ✓ (`$.created_at` allowlisted) | **YES** |
| **status** | real metadata col `status` TEXT ✓ (pre-KNN; empty-string sentinel, no population source yet) | `json_extract(body,'$.status') = ?` ✓ (`$.status` allowlisted) | **YES** |
| **source_type** | real partition-key metadata col `source_type` ✓ (pre-KNN) | **NOT a column**; deterministic = `resolve_source_type(kind)` → **constant-fold** within the single-kind list (pass-all iff `resolve_source_type(kind)==value`, else empty) | **YES** (derived) |
| **general json-path body** (`$.priority`, `$.tags`, …) | **NOT a vec0 metadata column** → would need post-KNN `json_extract` → **TYPED-REJECT** (preserve pre-KNN guarantee) | `json_extract(body,'$.path') <op> ?` ✓ (allowlisted) | resolves only on read.list **by design** (see D3) |

**Verdict:** **all four G10 shorthand fields resolve in BOTH stores.** The only
nuance is `source_type`: it is not a stored column in `canonical_nodes`, but because
`read.list` is always partitioned by a single `kind` and `source_type` is a pure
function of `kind` (`resolve_source_type`, a 6-value-locked + `edge_fact`/`doc` map),
a `SourceType` term on `read.list` **constant-folds** (the whole list shares one
`source_type`). That is a clean, total resolution — **NOT** a reserved-gap trigger
(D6). The general-json-path row is asymmetric *by design* (it has no vec0 home and
must not be demoted), which D3 turns into a typed rejection, not a gap.

### D3 — Total backend-dispatch + typed-rejection (no silent fallback, no demotion)

Every `FilterTerm` has a **defined outcome in EACH surface** — either "compiles to
backend X" or "typed rejection with a stated reason." There is **no silent fallback**
and **no demotion** of an indexed-metadata predicate to a slow post-KNN `json_extract`.

**`search_filtered` (vec0 backend — indexed pre-KNN):** accepts the **metadata
subset** `{SourceType, Kind, CreatedAfter, Status}` → lowered to the existing
`vector_filter_clause` / `vector_filter_values` (`lib.rs:5469-5521`), appended to the
single phase-1 `MATCH … {clause} ORDER BY distance LIMIT top_k`
(`build_vector_phase1_sql` `lib.rs:5528-5545`); the text branch is constrained in
Rust by `text_hit_passes_filter` (`lib.rs:5556+`), exactly as today.
**TYPED-REJECTS** `FilterTerm::Json(_)` with `EngineError::InvalidFilter` —
reason: *"arbitrary json-path predicate not supported on search_filtered; it would
require a post-KNN json_extract that defeats the indexed pre-KNN filter."* This is
the explicit no-demotion guarantee.

**`read.list` (canonical_nodes backend — `json_extract`):** accepts the **full**
set. Lowering:
- `Json(p)` → `p.to_sql_clause(param_idx)` (`lib.rs:1389-1426`), the shipped path.
- `Status(s)` → `Predicate::json_path_eq("$.status", Text(s))`.
- `CreatedAfter(b)` → `Predicate::json_path_compare("$.created_at", Gte, Integer(b))`.
- `Kind(k)` → real-column `canonical_nodes.kind = ?` (or constant-fold vs the
  partition arg — D1 impl-TBD).
- `SourceType(s)` → **constant-fold** vs `resolve_source_type(kind)`: pass-all
  no-op term iff equal, else compile to a guaranteed-empty result (e.g. `0=1`).
  Never a per-row `json_extract` (the column does not exist in body).

Both lowerings keep the inherited invariants: parameterized values only, allowlisted
paths only, `superseded_at IS NULL` + `logical_id IS NOT NULL` + `json_valid(body)`
guards on read.list (`lib.rs:6453-6459`), `canonical_nodes(kind)` index drives the
scan.

### D4 — Backward compat: shipped SDK shapes re-expressed as sugar, no behavior change

The public, shipped SDK types stay **byte-shape-accepted**:

- **`SearchFilter{source_type,kind,created_after,status}`** (Rust `lib.rs:1487-1493`;
  Py `types.py:111-127`; TS `index.ts:118-123`; Py/TS napi lowering
  `fathomdb-py/src/lib.rs:850,1422`) → becomes **sugar** that lowers to
  `Filter{terms: [SourceType?, Kind?, CreatedAfter?, Status?]}` (only present fields).
  `Engine::search_filtered(query, Option<SearchFilter>)` keeps its signature;
  internally it builds the unified `Filter` and dispatches to the vec0 backend.
- **`Predicate` / `read.list`** (Py `read.py`; TS `read.ts:68-72`) → lowers to
  `Filter{terms: [Json(p), …]}` and dispatches to the canonical_nodes backend.
- **Byte-identity pin holds:** `SearchFilter::is_unfiltered()` / `filter=None`
  (`lib.rs:1499-1504`) ⇒ empty `terms` ⇒ empty `{filter_clause}` ⇒ the phase-1 SQL
  is **byte-identical to 0.7.2**. The unification re-expresses the *input type* but
  must not touch the produced SQL string — `vector_phase1_sql_for_test(None)`
  (`lib.rs:5552-5554`) must still equal the frozen 0.7.2 literal.

"Re-express without behavior change": for the existing fields, the compiled SQL +
bound params + result rows are identical before and after; only the internal type
threading changes.

### D5 — Parity fixture + RED→GREEN plan (Slice 40)

- **Shared fixture** exercising BOTH backends: one DB seeded with canonical_nodes
  bodies carrying `$.status`/`$.created_at` AND a populated `vector_default` with the
  matching `source_type/kind/created_at/status` metadata, so a single logical
  predicate (e.g. `kind="todo" AND created_after=T`) is asserted on
  `search_filtered` (vec0 pre-KNN) and on `read.list` (json_extract) from the same
  rows.
- **RED-first G10 regression:** add the assertion that `search_filtered` **typed-rejects**
  a `Json`/arbitrary-body-path term (D3) and that the metadata subset still prunes
  pre-KNN — write it RED (before the dispatch lands), then GREEN. Extend
  `tests/pr_g10_filtered_knn.rs` and `tests/slice35_filter_grammar.rs`.
- **Byte-identical-SQL assertion:** keep/extend the `vector_phase1_sql_for_test(None)`
  == frozen-0.7.2-literal pin; add one asserting `SearchFilter{..}` sugar lowers to
  the **same** SQL the pre-unification struct produced.
- **Py↔TS parity (X1):** the unified surface (or, minimally, the unchanged
  `SearchFilter` + `Predicate` SDK shapes after re-expression) is asserted identical
  across Py and TS — exhaustiveness of `FilterTerm`/`Predicate` variants and the
  typed-rejection error type match in both bindings.
- **Exhaustiveness:** a test pins `FilterTerm` to exactly the five D1 variants and
  asserts no `Fused`/`_unchecked` symbol leaks into the unified surface (inherit
  D-F2).

### D6 — Reserved-gap trigger (the ONE legitimate condition)

Slice 40 may fall back to a reserved-gap slice **iff and only iff** a required
shorthand field **genuinely cannot resolve in a required store** — i.e. a field that
exists as an indexed vec0 metadata column but has **no real column, no allowlisted
`body` json-path, and no deterministic derivation** in `canonical_nodes` (so a
`read.list` predicate on it could only return wrong/empty results). **The
investigation found NO such field** (D2): all four resolve in both stores
(`source_type` via constant-fold). Therefore **no reserved-gap is triggered** by the
field matrix.

**Explicitly NOT a reserved-gap trigger:** skipping or under-building the dispatch
work. The two backends already exist (§1); "the dispatch was hard / I punted to a
gap" is a forbidden outcome. A reserved-gap fallback that is not justified by a
NAMED unresolvable field is rejected at review.

---

## 4. EXCLUDE list (out of scope — named so Slice 40 does not drift)

- **Demoting an indexed metadata predicate to post-KNN `json_extract`** on
  `search_filtered` — forbidden; arbitrary `Json` terms are typed-rejected (D3).
- **Any new grammar** beyond the shipped `Predicate` + the four shorthand fields —
  no DSL, no `JsonPathFused*`, no `*_unchecked`, no OR/nesting, no `JsonPathIn`
  (those remain ADR-0.8.0 reserved-additive).
- **Reshaping the vec0 schema or the canonical_nodes schema** — unification is a
  type+dispatch change; no migration, no new column (`status` population stays
  reserved-gap candidate 13).
- **Changing the produced SQL for existing inputs** — byte-identity + behavior
  parity are blockers (D4).
- **A real `status` population source** — orthogonal; still reserved-gap candidate 13.
- **Raw SQL / string-interpolated paths or values** — foreclosed (inherit D-F4).

---

## 5. Cross-ADR consistency (required self-check)

- **Closes reserved-gap 37.** ADR-0.8.0-filter-grammar D-F3 named full single-grammar
  unification (touching the shipped `SearchFilter` struct + its compilation) as
  reserved-gap 37, "needed, not optional." This ADR is that work; on landing,
  reserved-gap 37 is resolved (update `dev/roadmap/0.8.1.md` / plan ledger).
- **Inherits, does not re-open, D-F1/D-F2/D-F4/D-F5.** The closed grammar, the
  EXCLUDE set, parameterized-allowlisted compilation, and implicit-AND are carried
  verbatim; `ScalarValue`/`ComparisonOp`/`Predicate` are reused, not redefined — so
  the "shared value vocabulary" of D-F3 becomes a real single type.
- **G0 substrate untouched.** `read.list` keeps the folded `canonical_nodes(kind)`
  index + `superseded_at IS NULL` (logical_id-alone) active-row rule.
- **Slice-10 invariants honored.** RRF/recency/rerank seams untouched; the
  `filter=None` byte-identity pin is a landing blocker.

---

## 6. Consequences / reserved follow-on

- **For 0.8.11 Slice 40:** one unified `Filter` type + a total dispatch over the two
  existing compilation backends; the public `SearchFilter` + `Predicate` SDK shapes
  survive as sugar; reserved-gap 37 closed.
- **Reserved-additive (no reshape):** `JsonPathIn`, OR/nested boolean — clean future
  `FilterTerm` additions on the unified enum (now a single extension point).
- **Still reserved:** `status` real population (gap candidate 13).

---

## 7. De-risking checklist for Slice 40 (the implementer cannot quietly punt)

1. **Every `FilterTerm` × every surface has a defined outcome** — a table in code/test
   showing compile-to-backend-X or typed-reject-with-reason for all 5 variants on
   both `search_filtered` and `read.list`. No "falls through."
2. **`search_filtered` typed-rejects arbitrary `Json` terms** with `InvalidFilter`
   (RED-first test) — proving no silent post-KNN `json_extract` demotion.
3. **All four shorthand fields verified on BOTH backends** — incl. `source_type`
   constant-fold via `resolve_source_type(kind)` on `read.list`.
4. **`filter=None`/all-empty ⇒ byte-identical 0.7.2 phase-1 SQL** — pin still green.
5. **Existing-input behavior parity** — `SearchFilter{..}` sugar lowers to the same
   SQL + params + rows as the pre-unification struct (diff test).
6. **`FilterTerm` exhaustiveness pinned to the 5 D1 variants**; no `Fused`/`_unchecked`
   leak (inherit D-F2).
7. **Py↔TS parity (X1)** on the surface + the typed-rejection error.
8. **No schema migration; no vec0/canonical_nodes column change.**
9. **A reserved-gap fallback requires a NAMED unresolvable field** (none found in D2);
   dispatch-avoidance is rejected at review.

---

## 8. Sources

Engine: `src/rust/crates/fathomdb-engine/src/lib.rs` — `SearchFilter` `:1487-1493`,
`is_unfiltered` `:1499-1504`; vec0 compilation `vector_filter_clause` `:5469-5494`,
`vector_filter_values` `:5499-5521`, `build_vector_phase1_sql` `:5528-5545`,
`vector_phase1_sql_for_test` `:5552-5554`, `text_hit_passes_filter` `:5556+`;
`status='' ` sentinel INSERTs `:4136-4138,4952-4954,7815-7817`; vec0 Pack-2 shape
`vector_partition_create_sql` `:8439-8451`; `PREDICATE_PATH_ALLOWLIST` `:1310-1311`;
`Predicate` enum `:1325-1330` + constructors `:1337,1349` + `to_sql_clause`
`:1389-1426` + `bind_value` `:1429-1439`; `Engine::read_list` `:501-506`, dispatch
`:791`; `read_list_in_tx` `:6436-6494`; `resolve_source_type` `:8640-8655`.
Schema: `src/rust/crates/fathomdb-schema/src/lib.rs:110` (canonical_nodes
write_cursor/kind/body), `:198` (source_id), `:300-305` (logical_id, superseded_at,
active unique index). SDK: `src/python/fathomdb/types.py:111-127` (SearchFilter),
`src/python/fathomdb/read.py` (Predicate/read.list), `src/ts/src/index.ts:118-123`
(SearchFilter), `src/ts/src/read.ts:68-72` (Predicate); napi lowering
`src/rust/crates/fathomdb-py/src/lib.rs:850,1422`,
`src/rust/crates/fathomdb-napi/src/lib.rs`. Design/ADR: `dev/design/slice-10-design.md:96-130,196-211`
(G10 SearchFilter, byte-identity pin, test plan); `dev/adr/ADR-0.8.0-filter-grammar.md`
(D-F1..D-F5, EXCLUDE list, reserved-gap 37). Tests touched: `tests/pr_g10_filtered_knn.rs`,
`tests/slice35_filter_grammar.rs`.
