# B-1 — FathomDB 0.8.x interaction-point answer sheet + Option-B tasklist corrections

> **Purpose.** The FathomDB-side contract-of-record for bringing Memex 0.5.1 onto the FathomDB
> 0.8.x governed API (BLOCKER B-1). Answers every `KEY FATHOMDB INTERACTION POINT` in
> `memex/dev/fathomdb/memex-0.5.1-fathom-0.8.11.2-option-b-tasklist.md`, lists the corrections the
> tasklist needs to be **correct for FathomDB**, and enumerates FathomDB-side action items.
> All cites are `file:line` in the 0.8.11.2 worktree (`fathomdb-engine` = `src/rust/crates/fathomdb-engine/src/lib.rs`,
> `fathomdb-py` = `src/rust/crates/fathomdb-py/src/lib.rs`, `fathomdb-schema` = `src/rust/crates/fathomdb-schema/src/lib.rs`).
> Sourced from three read-only investigations 2026-06-30. **This is the authoritative API
> surface; trust it over the tasklist's pre-investigation assumptions.**

## TL;DR — the three load-bearing corrections

1. **I-4 (FTS/projection) — there is NO consumer projection/FTS declaration API.** FTS is an
   engine-owned fixed projection of the single `body` field (tokenizer `porter unicode61
   remove_diacritics 2`, uniform BM25), built automatically at store-open by immutable internal
   migrations. Memex does **not rewrite** `m003–m007` against a new API — it **deletes** them and
   ensures searchable text lands in each record's `body`. The real risk moves from "migration
   data-safety" to **FTS-capability parity** (multi-field paths / custom tokenizer / BM25 weights
   are all gone).
   - **Resolution (HITL 2026-06-30):** no FathomDB FTS extension. Un-conflate retrieval vs ranking —
     multi-field/payload = a **recall** concern (Memex content-model into `body`), **recall-gated**
     (CE-rerank cannot recover a recall miss); per-column BM25 weights = a **ranking** concern,
     won't-add, recovery via **CE-rerank** (ranking-gated); custom tokenizers off-table. T3.4 splits
     into a recall/coverage drift test and a ranking drift test.
2. **I-5 (write shape) — one path: `engine.write([dict…]) → WriteReceipt`.** Several flat symbols
   have **no analog**: `ChunkInsert`/`ChunkPolicy` (chunk client-side), `NodeRetire` (emulate via
   tombstone-bodied re-write keyed on `logical_id`), `LastAccessTouchRequest` (gone), `new_id`
   (caller supplies `logical_id`). Row id = the returned `write_cursor`.
3. **I-2 (paging) — `read.list` has NO cursor/`after_id`/`ORDER BY`, only `limit`.** The tasklist
   assumed a `limit + after_id` cursor; that exists only on `read.collection` (op-store), **not**
   `read.list`. Stable/deep paging over a node partition is not possible today (FathomDB gap A-3).

---

## Interaction-point answers (the contract)

### I-5 — Governed write request shape  ✅ available on pinned build
- **Single path:** `engine.write(batch: list[dict]) → WriteReceipt` (py 795; engine `Engine::write` 2719).
- **Variants** (`PreparedWrite` closed enum, engine 1728; Python dispatch `translate_write_item` py 1280):
  - `{"node": {kind, body?, source_id?, logical_id?}}` (bare `{"kind":…}` also defaults to Node; `body` defaults `"{}"`).
  - `{"edge": {kind, from, to, source_id?, logical_id?, body?, t_valid?, t_invalid?}}` (confidence/extractor/temporal_fallback only via `ingest_with_extractor`).
  - `{"op_store": {collection, record_key, schema_id?, body}}`.
  - `{"admin_schema": {name, kind, schema_json, retention_json?}}`.
- **Receipt** (`WriteReceipt`, engine 1034 / py 311): `cursor: u64` (batch high-water), `row_cursors: list[u64]` (1:1 input order), `dangling_edge_endpoints: u64` (flag-and-count).
- **IDs/provenance:** row id = `write_cursor` (server-assigned; no `new_row_id`/`new_id`). `logical_id` (caller-supplied) → transaction-time supersession (tombstone-then-insert, engine 9584/9635); `None` = plain insert. `source_id` (nullable column) = provenance; no `ProvenanceMode`.
- **Retired-symbol map:** `WriteRequest`→`[PreparedWrite]`; `NodeInsert`→`node`; `EdgeInsert`→`edge`; `new_row_id`→`row_cursors`; `ProvenanceMode`→`source_id` column. **No analog:** `ChunkInsert`, `ChunkPolicy`, `NodeRetire` (= supersede-by-rewrite, requires `logical_id`), `LastAccessTouchRequest`, `new_id`.

### I-4 — Projection / FTS declaration model  ⚠️ no consumer API (engine-owned, fixed)
- No projection/FTS verb exists. `admin.configure` only registers an **operational collection** (JSON-Schema body, `kind ∈ {append_only_log, latest_state}`) — **not** FTS (py 1036; engine 9456).
- FTS is fixed in the immutable migration list: `search_index`/`search_index_edges` over a single `body` column, tokenizer `porter unicode61 remove_diacritics 2`, uniform BM25 (schema 158/268/347; engine bm25 6116). JSON subfields are filter-only via `json_extract(body,'$.x')`, never FTS index targets.
- **No analog** for `ProjectionTarget`, `FtsPropertyPathSpec`, `FtsPropertyPathMode`, tokenizer spec, BM25-weight spec.
- **Migration-safety:** projections are **not consumer-redeclarable** (engine rebuilds FTS from `body` at open). Nothing to forward-migrate ⇒ aligns with HITL **Q-B1 = no DB migration**.

### I-3 — `ce_score` on SearchHit / PerHitExplain  ✅ available on pinned build
- `SearchHit.ce_score: Option<f64> = sigmoid(ce_logit) ∈ [0,1]` (engine 1121); `Some` only for in-pool reranked hits, else `None`. Mirror on `PerHitExplain.ce_score` (engine 1281). Never participates in ranking.
- Knobs: `alpha` (clamp [0,1], default 0.3; `1.0`/`pool_n=10` = measured-parity), `pool_n` (default `rerank_depth`) — py `search`/`search_explained` 833; `rerank_passages` py 1523. pyo3 `PySearchHit.ce_score` py 393.

### I-6 — embed primitive / VectorRegenerationConfig  ⚠️ symbols removed
- `engine.embed(text) → list[float]` (py 988; TS `binding.ts:272`), default embedder `fathomdb-bge-small-en-v1.5`; raises `EmbedderNotConfiguredError` if `use_default_embedder=False`.
- `BuiltinEmbedder` **and** `VectorRegenerationConfig` are **removed tree-wide** (zero grep hits). The I-6 fallback "keep BuiltinEmbedder if unchanged" is **invalid** — both must be dropped.

### I-7 — source_id / stable_id (Cause-A)  ⛔ stable_id gated behind Cause-A merge
- `SearchHit.source_id: Option<String>` (graph-arm provenance, engine 1114) is **distinct** from `SearchHit.stable_id: Option<String>` (Cause-A, engine 1132). Both coexist.
- `stable_id` = `"l:<logical_id>"` else `"h:<sha256(body)>"` (`derive_stable_id` engine 9027); telemetry `result_stable_ids` parallel to `result_ids`.
- **Availability:** `source_id` + `ce_score` are on `origin/main` (`ba80866d`) NOW. `stable_id` lives **only** on `0.8.11.2-pico-umbrella` (commit `d3f6b994`, `git merge-base --is-ancestor … origin/main` → NO). Memex wires the `source_id` slot inert now; consumes `stable_id` only **after Cause-A merges**.

### A-1 — `$.action_kind` predicate-path allowlist  → FathomDB one-line add
- `PREDICATE_PATH_ALLOWLIST = {$.status, $.priority, $.tags, $.kind, $.created_at}` (engine 1321). `$.action_kind` **absent** → `InvalidFilterError`. Enforced at construct (1350) and read (3894).
- **FathomDB action:** add the literal `"$.action_kind"` (engine 1322) — additive, no API change. Gates only Slice-5's hot WMAction filter; Memex client-side split is the fallback.

### A-2 — bool-eq server-side  ✅ resolved on read.list
- No `FilterJsonBoolEq` symbol; bool-eq is `Predicate::JsonPathEq{ ScalarValue::Bool }` and **executes server-side** on `read.list` with a `json_type IN ('true','false')` guard (engine 1401). Python `bool` → `ScalarValue::Bool` (py 1160).
- Caveat: rejected on the **search** backend (post-KNN), by design (engine 1614). Path must still be allowlisted (so `action_kind` bool-eq depends on A-1).

### I-2 — read.list Filter grammar + paging  ⚠️ no stable paging (FathomDB gap A-3)
- `read_list_filter(engine, kind, terms, limit=100)` (py 1230). `FilterTerm` ∈ {`source_type`, `kind`, `created_after`, `status`, `json`(allowlisted Predicate)} (engine 1537). **AND-only**, no OR/nesting/DSL/SQL. Allowlisted paths = the A-1 set. Result rows `NodeRecord{logical_id,kind,body,write_cursor}` (engine 1174); anonymous rows excluded.
- **Public verb:** the public consumer verb = `read.list(engine, kind, predicates=… ｜ filter=Filter(…))`; `read_list` / `read_list_filter` are the underlying pyo3 bindings (`read.list` dispatches to the filter binding when `filter=` is passed).
- **Paging:** `limit` only — **no `ORDER BY`, no cursor, no `after_id`** on `read.list` (engine 6779). The `after_id` cursor exists only on `read.collection` (op-store, engine 492/3824).
- **Op-store recall:** `read.collection` is pagination-only (`ORDER BY id`, mandatory `limit`, no filter terms); a `record_key` lookup filters client-side over `OpStoreRow[]`.

---

## Corrections the Option-B tasklist needs (to be correct for FathomDB)

| Tasklist ref | Current text | Correction |
|---|---|---|
| **T2.8 / I-6** | "confirm `BuiltinEmbedder`+`VectorRegenerationConfig` shapes; migrate if changed; keep if unchanged" | Both symbols are **removed**. **Drop** both; route to `engine.embed(text)`. No "keep if unchanged" fallback. |
| **Phase 3 / I-4** | "rewrite the projection/FTS registration to the governed model; update import surface" | There is **no governed projection/FTS API**. **Delete `m003–m007`** entirely. Add a task: ensure each searchable record carries its searchable text in `body`; structured fields stay query-filterable via `json_extract(body,'$.field')`. Keep T3.4 FTS-parity but reframe as **capability-parity under fixed single-`body`/uniform-BM25/fixed-tokenizer** (see new risk). |
| **T3.3 / Q-B1** | "in-place rewrite vs additive forward migration — HITL" | **Moot for FTS** (nothing to forward-migrate; FTS rebuilds from `body` at open). HITL ruled **no DB migration**. |
| **T2.2 / I-5** | "migrate writes onto governed `Engine.write` shape" | Correct direction; **call out the no-analog symbols**: `ChunkInsert`/`ChunkPolicy` → chunk client-side before writing nodes; `NodeRetire` → emulate via tombstone-bodied re-write **keyed on `logical_id`** (so every retirable record must carry a `logical_id`); `LastAccessTouchRequest` → **no analog**, drop last-access tracking or move it to an op-store collection; `new_id`/`new_row_id` → caller supplies `logical_id`, row id is the returned `write_cursor`. |
| **T2.4 / A-2** | "client-side boolean predicate eval fallback" | bool-eq **runs server-side on `read.list`** (path must be allowlisted). The client-side bool fallback is **unnecessary** once the path is allowlisted; only `$.action_kind` still needs A-1. |
| **T2.3/T2.4 / I-2** | "paging contract (`limit` + `after_id` cursor) for within-kind lists" | **Wrong for `read.list`** — only `limit`, no cursor/order. Memex must (a) audit whether any `read.list` call site needs deep/stable paging, and (b) if so, treat it as **blocked on FathomDB gap A-3** (add `write_cursor` order + `after_cursor`). `after_id` paging is available only on the op-store `read.collection` surface. |
| **I-8** | "B-1 gates only the real-gold adoption tail" | **Confirmed** — academic/screening arms (OPP-6 EXP-COV, OPP-3 characterization, OPP-1 build arms, V-1/V-3/V-7) run FathomDB-side regardless; only the as-Memex/real-gold ADOPTION arms need Memex-on-0.8.x, and those remain a HITL Adopt-GO hard-stop. |

### New risk to register (was not in the tasklist)
- **R-I4-parity — FTS capability loss.** 0.8.x FTS indexes only the single `body` field with a
  fixed tokenizer and uniform BM25. If Memex's content/knowledge/conversation FTS today relies on
  **multiple weighted property paths, a custom tokenizer, or BM25 field weights**, those
  capabilities do **not** exist in 0.8.x. The refit therefore includes a **content-modeling step**
  (compose the searchable text into `body`) and a **ranking-parity check** under uniform weighting.
  This is the real I-4 risk — larger than migration mechanics. Mitigation: inventory the
  `m003–m007` FTS specs (how many property paths / weights / tokenizer overrides) before T3.

---

## FathomDB-side action items (what FathomDB delivers for the refit)

| ID | Action | Size | Gating | Disposition |
|---|---|---|---|---|
| **A-1** | Add `"$.action_kind"` to `PREDICATE_PATH_ALLOWLIST` (engine 1322) + a guard test | 1-line + test | Slice-5 hot filter only (client-side fallback exists) | **DO** in the 0.8.11.2 worktree (additive; not on critical path) |
| **A-2** | — | — | — | **RESOLVED** (server-side on `read.list`); no FathomDB work |
| **A-3 (new)** | Add deterministic order (`write_cursor`) + `after_cursor` to `read.list` for stable paging | small, additive | only if Memex confirms a deep-paging `read.list` call site | **DEFER** pending Memex paging audit; copy the op-store `read.collection` cursor pattern |
| **I-7 / Cause-A** | Merge `0.8.11.2-pico-umbrella` (`d3f6b994`) so `stable_id` reaches `origin/main` | (existing 0.8.11.2 work) | Memex consumes `stable_id` only post-merge | Memex wires `source_id` slot inert until then |

## Experiment-timeline impact (unchanged by these corrections)
- **Unaffected by B-1 (run now, FathomDB-side):** OPP-6 EXP-COV curve + ceiling, OPP-3 native-gap characterization, OPP-1 build arms (MuSiQue+HotpotQA), Cause-A, V-1 keystone, V-3, V-7.
- **Need Memex-on-0.8.x (gated behind the refit + Adopt-GO):** OPP-1 Adopt-GO, OPP-3 adoption re-measure, OPP-6 real-gold confirmation. Cause-A `stable_id` keys exactly these arms and lands when 0.8.11.2 merges.

## HITL decisions (2026-06-30)
- **Q-B1** = **no DB migration** (consistent with I-4: nothing to forward-migrate).
- **Q-B2** = **keep the `0.5.1` label** for the refit.
- **Q-B3** = **greenlight Slice-15-core after the codex pass** (step 3 of the directive).
