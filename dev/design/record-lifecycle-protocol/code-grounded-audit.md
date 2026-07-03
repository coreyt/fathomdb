# Record-lifecycle-protocol — code-grounded audit

> Verifies the PROPOSED record-lifecycle contract (`README.md`,
> `structural-lifecycle-contract.md`, `projection-registry-and-async-embed.md`)
> against FathomDB's actual code. Date: 2026-07-02.

**Ground truth** (schema `fathomdb-schema/src/lib.rs`, `SCHEMA_VERSION=15`; engine
`fathomdb-engine/src/lib.rs`, 10,858 lines; py/napi bindings):

- `canonical_nodes` columns: `write_cursor, kind, body, source_id, logical_id,
  superseded_at`. **No** `is_latest`, **no** `valid_*`, **no** lifecycle-state column.
- `canonical_edges` adds `body, t_valid, t_invalid, confidence, extractor_model_id,
  temporal_fallback`.
- `is_latest` / `valid_from` / `valid_until` = **0 occurrences** in the engine.

## REAL MISSES (B / C / D) — where the doc is wrong about today

### 1. (C) Version-currency is `superseded_at`, not `is_latest`

The CR-060 keystone contradicts the shipped G0 substrate.

- Doc: `structural-lifecycle-contract.md:58-65` — "`is_latest` (bool)… enforced by
  `UNIQUE(logical_id) WHERE is_latest = 1`"; `README.md:72` repeats it.
- Code: currency is a `superseded_at INTEGER` "transaction-time tombstone"; the shipped
  invariant is `CREATE UNIQUE INDEX canonical_nodes_logical_active_idx ON
  canonical_nodes(logical_id) WHERE superseded_at IS NULL`
  (`fathomdb-schema/src/lib.rs:300-307`, comment `:282-296`). This is the HITL-SIGNED
  **logical_id-alone** G0 index (ADR-0.8.0). There is no `is_latest` column; `is_latest`
  is only a *derivable predicate* (`superseded_at IS NULL`).
- Correction: the "single is-latest authority" the contract wants **already exists** as
  the `superseded_at` partial-unique index. Describe currency in those terms, or
  explicitly mark `is_latest` as a net-new *rename* of a shipped index — not a new
  mechanism. Most material miss: it misdescribes the exact invariant the contract is
  built to leverage.

### 2. (C) The "never call it a tombstone" ban contradicts pervasive shipped terminology

- Doc: `contract:92, 137-139` — head-advance "is **never** called a 'tombstone'."
- Code: schema and engine name it exactly that everywhere: `superseded_at` =
  "transaction-time tombstone" (`schema:284`); "tombstone-then-insert" at
  `engine:1796, 3261, 3470, 6508, 9439, 10014, 10067`.
- Correction: the ban targets load-bearing, shipped names. Drop it or acknowledge it as
  a costly rename of existing code/schema vocabulary.

### 3. (C) The `t_invalid` naming ban contradicts a frozen, in-use edge column

- Doc: `contract:141` — bans `t_invalid`, "use `valid_from`/`valid_until`."
- Code: `t_invalid` is a shipped `canonical_edges` column (`schema:352`, step 14 /
  0.8.1) driving the live temporal filter `t_invalid IS NULL OR datetime(t_invalid) >
  datetime('now')` (`engine:4084, 6690`) and the 0.8.12 rebuild filter
  (`engine:5428-5437`).
- Correction: `t_invalid` is frozen and load-bearing; the doc can't both ban the name
  and (in §3) rely on its validity semantics.

### 4. (B) Temporal validity: edge-only, ISO-8601 strings, inline `datetime('now')`

Not node-level integer windows via `SearchFilter`+bound `:now`.

- Doc: `contract:94-107`, `README:57` — `valid_from`/`valid_until` half-open **integer**
  interval on **records/nodes**, "evaluated via the typed `SearchFilter` seam with a
  **bound `:now`** (never `unixepoch()` inline)."
- Code: validity exists **only on edges** as `t_valid`/`t_invalid` **ISO-8601 TEXT**
  (`schema:351-352`); evaluated by **inline** `datetime(t_invalid) > datetime('now')`
  (`engine:5430, 6690`), **not** through `SearchFilter` (whose only fields are
  `source_type, kind, created_after, status` — `engine:1499-1504`). Nodes have no
  validity columns.
- Correction: (a) validity is edge-only today, node/record validity is net-new; (b) it's
  ISO-8601 strings compared with `datetime()`, not integer half-open; (c) `SearchFilter`
  is not the validity seam and has no `:now` binding; (d) the shipped code uses inline
  `datetime('now')`, directly contradicting "never inline."

### 5. (B) Async-embed: the engine DOES own a background worker

No host-driven cadence, no `flush_embeddings`, no sync-inline default.

- Doc: `projection-registry-and-async-embed.md:66-83` — "the engine **does not own a
  background worker**; the host owns embed cadence," default `sync-inline`, opt-in
  `flush_embeddings()`.
- Code: the engine spawns an in-process **projection dispatcher + worker thread pool**
  (`engine:876-881`, `projection_dispatcher_loop`/`projection_worker_loop`,
  `notify_new_work` `:887`), embedding **async off the write path**, cursor-scheduled via
  `_fathomdb_projection_state.last_enqueued_cursor` (`schema:166-170`). No
  `flush_embeddings`, no `dense_readiness`, and the default is engine-async, not
  sync-inline.
- Correction: today's model is an engine-owned async worker pool; the proposed
  host-owned cadence + sync-inline default + flush verb is a redesign, not the current
  mechanism.

### 6. (B) "The engine already stores EAV attributes" — it does not

- Doc: `projection...:20` — "The engine already stores everything (EAV attributes, typed
  edges live in FathomDB's substrate)."
- Code: typed edges exist (`canonical_edges`), but there is **no EAV/attribute table or
  code** anywhere. Only a few fixed edge columns (`confidence`, `extractor_model_id`).
- Correction: the registry's `filterable`/`rankable`/`searchable` attribute projection
  has **no canonical attribute store to project from** — that store is net-new. Only
  `body`-FTS exists (`search_index`, `search_index_edges`); there is no
  per-property/property-FTS.

### 7. (D) Materialized `admissible` bit / "exclusion at the index, never per query"

Exclusion is actually derived per query.

- Doc: `contract:14-17, 114-117` — liveness exclusion "materialized/indexed, never
  derived per query"; `admissible = active ∧ is_latest` is "the single cheap bit the hot
  path filters on."
- Code: no materialized `admissible`/`is_latest` column exists; exclusion is a
  **query-time** `WHERE superseded_at IS NULL` predicate/JOIN
  (`engine:6526, 6910, 5436, 6412`) backed by the partial index, plus inline edge
  `t_invalid`. `active` does not exist at all.
- Correction: currency exclusion is a query-time predicate over an indexed column, not a
  materialized boolean; the materialized-bit and the "must never be per-query" principle
  are net-new/aspirational.

### 8. (D) The 0.8.12 "rebuild-durable projection filter" is narrower than claimed

- Doc: `README`/§6 imply a general durable admissible projection filter.
- Code: the shipped 0.8.12 filter is exactly the **edge FTS rebuild** mirroring the
  graph-traversal recency filter `t_invalid IS NULL OR datetime(t_invalid) >
  datetime('now')` (`engine:5428-5437`) — edge-only, single-sided invalid-time,
  consolidation-driven. No node coverage, no `is_latest`/`admissible` materialization.
- Correction: scope the claim to edge-`t_invalid` recency; the node-level admissible
  generalization is net-new.

### 9. (D) CR-056 "one engine existence-state replaces the three tombstones" is aspirational

- `README:70`. The existence axis is entirely net-new (see A1), so nothing in the engine
  can replace Memex's three encodings today. Mark as net-new, not resolved.

## NET-NEW INVENTORY (class A — load-bearing pieces that do not exist)

Not bugs (this is a PROPOSED design) — but the doc must mark these net-new so it doesn't
imply they exist:

- **A1. Existence axis in full** — `pending/active/deleted/purged` enum column +
  transition table (promote/reject/soft-delete/restore/purge). No lifecycle-state column
  exists; the only mutation verbs are `PreparedWrite::Node/Edge` supersession + consolidate
  keep/invalidate/supersede (`engine:1785-1819, 1762-1776`). No delete/restore/promote/reject.
- **A2. Physical `purge`/hard-erase** + `secure_delete`/`VACUUM` + edge referential stubs
  - `include_existence_stubs`. No purge path exists.
- **A3. `is_latest` as a stored/materialized column** (currency is `superseded_at`;
  `is_latest` is only derivable).
- **A4. Node/record-level temporal validity** (`valid_from`/`valid_until`). Validity is
  edge-only.
- **A5. Composable read-mode relax-flags** — `include_deleted`, `include_superseded`,
  `valid_as_of(t)`, `ignore_validity`, `include_existence_stubs`. None exist; reads have a
  fixed active-only (+ edge-validity) filter.
- **A6. `crossed_boundary_since(t)`** detection hook.
- **A7. Materialized `admissible` column.**
- **A8. Projection registry** + per-attribute/edge-type `filterable`/`rankable`/`searchable`
  declarations. Fully net-new — no registry, no declaration mechanism.
- **A9. EAV attribute storage + property-FTS.** Only `body`-FTS exists.
- **A10. `dense_readiness ∈ {ready, embedding}`** + the atomic readiness-flip invariant.
- **A11. `flush_embeddings()`** + host-chosen sync-inline/deferred embed modes.
- **A12. `valid_as_of(t)` / bound `:now`** query parameter.
- **A13. F9 signal algebra / `rankable`** — already honestly named as deferred (~0.8.16). ✓
- **A14. `SearchHit.id → logical_id` swap + doc-seeded no-`logical_id` gap** — already
  honestly named GATING (the one the doc got right).

## What the doc got RIGHT (has real code basis)

`SearchHit.id = write_cursor` (`engine:1109`), additive `stable_id` `l:`/`h:` for
doc-seeded nodes (`engine:1122-1132`; py `:427-448`, napi `:444-465`) — the one miss it
already caught. Also grounded: `superseded` positional-head model;
`_fathomdb_vector_rows.kind` coverage tracking (`schema:176-180`) + `verify_embed_db`
gate; rebuild-durable projections (`rebuild_projections`); typed edges + `confidence`
dial; RRF partial-dense under-ranking (`fuse_three_arms`/`fuse_rrf`); recency-reweight
seam (`apply_recency_reweight`); FTS/filter-inline vs vector-async split.

## Single most material miss + overall read

**Most material:** finding #1 — the contract's CR-060 keystone ("expose is-latest once
via `UNIQUE(logical_id) WHERE is_latest=1`") describes a column and index that **do not
exist**, while the real, already-shipped single authority — the `superseded_at IS NULL`
partial-unique index (HITL-SIGNED, logical_id-alone) — goes unnamed. The mechanism the
contract is built to leverage is misdescribed, and §2/§5's naming (`is_latest`, banning
`tombstone` and `t_invalid`) collides with frozen, load-bearing schema/engine vocabulary.

**Overall:** The contract is **honest and precise about the one co-requisite it verified**
(SearchHit id), but **not honest about exists-vs-build across the rest.** The engine today
has **exactly one** of the three axes — version-currency, via `superseded_at` — plus
**edge-only** validity. It has **none** of: the existence axis, node validity, a
materialized `admissible` bit, read-mode relax-flags, the projection registry,
EAV/property-FTS, `dense_readiness`, `flush_embeddings`, or any purge/delete/restore verb.
The document is roughly **90% net-new** but reads as if the three axes + materialized
admissible + registry are near-shipped, and where it does touch shipped mechanisms
(currency index, "tombstone", `t_invalid`, `SearchFilter`-as-validity-seam, host-driven
embed) it misdescribes or bans them. Recommended fix: add a "what exists today" delta
section, reconcile §2/§3/§5 with the shipped G0 substrate (`superseded_at`,
`t_valid`/`t_invalid` on edges, the engine-owned projection worker), and relabel A1–A12
explicitly as net-new.
