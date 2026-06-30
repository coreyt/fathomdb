# Multi-field + recursive-payload FTS — design-on-spec (0.8.11.2)

> **DESIGN-ON-SPEC — NOT APPROVED. HITL is still deciding whether to build this.
> Do not implement.**
>
> This document is a *contingent* design produced so that, **if** HITL elects to build opt-in
> multi-field / recursive-payload FTS, there is a grounded, source-anchored plan ready. It is **not**
> a build order. As of the R-I4 / Q-B5 resolution (HITL 2026-06-30,
> `dev/design/0.5.1x0.8.11.2/10-fathomdb-side-design.md` §6), FathomDB owes **NO FTS extension** for
> 0.8.x; the refit recovers FTS parity Memex-side by content-modeling searchable text into `body`.
> This design exists for the *high-bar, recall-gated* future case where that content-modeling proves
> insufficient. **Nothing here is greenlit.** The load-bearing section is §8 (Value Test): it is the
> gate that decides whether this feature is genuinely valuable or merely a workaround for earlier weak
> search — and it must pass before any of §2–§7 is built.

---

## 0. Provenance, scope, and what this is not

This design is a **scoped revival** of the `ProjectionTarget` / `FtsPropertyPathSpec` /
`FtsPropertyPathMode` surface that 0.6.0 deliberately removed (the "5-verb strip"). The answer sheet
records the removal explicitly: there is **no analog** for `ProjectionTarget`, `FtsPropertyPathSpec`,
`FtsPropertyPathMode`, tokenizer spec, or BM25-weight spec in 0.8.x
(`dev/plans/runs/B-1-fathomdb-answer-sheet.md` §I-4, line 52). Re-introducing any part of that surface
is a **governance event** (§9) — it re-opens a declaration surface that was removed on purpose.

**Recall axis, not ranking axis.** This is the single most important framing in the document and it is
non-negotiable (per HITL 2026-06-30, B-1 §I-4 lines 21-25; 10-fathomdb-side-design.md §6):

- **IN SCOPE (RECALL):** multi-field indexing (N JSON property paths → FTS-indexed text) and
  recursive-payload flattening (nested JSON → indexed text). These change **what enters the candidate
  pool** — i.e. recall / coverage.
- **OUT OF SCOPE (RANKING) — explicit non-goals:**
  - **Per-column BM25 weights — WON'T ADD.** Ranking stays uniform BM25 + CE-rerank. Ordering within
    the pool is recovered by CE-rerank (`SearchHit.ce_score`, engine lib.rs:1121), never by engine
    field-weights. (B-1 §I-4; 10-fathomdb-side-design.md §6.)
  - **Custom / per-kind tokenizers — OFF-TABLE for 0.8.x.** No tests, not pursued. Any per-kind
    normalization stays Memex-side.

So this design indexes **more fields**; it does **not** add ranking weights or new tokenizers. A
multi-field index that surfaces a document into the pool, then lets CE-rerank order it, is the entire
mechanism. The query path is unchanged (one `bm25 MATCH`; §9).

---

## 1. Motivation + the recall-vs-ranking framing

### 1.1 What FathomDB FTS does today

FTS is an **engine-owned, fixed projection of a single `body` field**, built automatically at
store-open by immutable internal migrations — there is no consumer FTS/projection API
(B-1 §I-4, lines 49-53). Concretely, on every node write the engine projects the node body verbatim
into the FTS5 shadow:

- Node body is assembled and inserted into `canonical_nodes`
  (`src/rust/crates/fathomdb-engine/src/lib.rs:9591-9595`); for the extractor path the body is just the
  entity `name` (engine lib.rs:3006-3011, `body: name.to_string()`).
- That same `body` string is projected 1:1 into the `search_index` FTS5 vtable
  (engine lib.rs:9596-9599): `INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)`.
- Edges with a non-null body mirror this into `search_index_edges` (engine lib.rs:9703-9708).
- The vtable has exactly **one indexed column**, `body`, plus two `UNINDEXED` columns (`kind`,
  `write_cursor`); tokenizer `porter unicode61 remove_diacritics 2`; uniform BM25
  (`src/rust/crates/fathomdb-schema/src/lib.rs:158-162` for nodes, 355-360 for edges, 268-278 for the
  tokenizer drop+recreate).
- JSON subfields are **filter-only** today, via `json_extract(body,'$.x')` in the read.list/filter
  machinery (engine lib.rs:1403-1432) — they are **never FTS index targets**. A nested or sibling
  property is queryable as an exact/range predicate but is invisible to `bm25 MATCH`.

### 1.2 The gap and the framing

If a record carries searchable natural-language text in a property **other than** the one composed
into `body` — e.g. `payload.summary`, `payload.transcript`, `metadata.title` — that text never enters
the FTS index, so a lexical query matching only that text returns **nothing**. CE-rerank cannot fix
this: rerank only re-orders the candidate pool, and a document that never entered the pool cannot be
re-ordered into it. **This is a recall miss, and recall misses are upstream of ranking.**

That is the whole motivation: multi-field / recursive-payload indexing is a **recall feature**. It
widens what `bm25 MATCH` can surface. Ranking remains exactly as it is today: uniform BM25 to form the
pool, then CE-rerank (`ce_score`) to order it. We add fields to the index; we do not touch how the pool
is scored.

**The risk this design must guard against** (and §8 exists to test): multi-field indexing may be a
*workaround for earlier weak search*. If the modern stack — CE-rerank + a vector retrieval arm — already
recovers the documents that multi-field would surface, then multi-field buys ~0 product value and we
should not re-open the removed surface to ship it. The value test (§8) is the gate.

### 1.3 Provenance finding — multi-field-recall is the *most substitutable* of the three pieces

The Memex provenance investigation (2026-06-30) sharpens this risk into a **skeptical prior**. It found
that the original `m003–m007` FTS structure was **not** a workaround for weak FathomDB/SQLite search
*relevance* — it fixed Memex's own **Python-scan latency** (load-500-nodes + substring scan, ~44–56s
per turn). The relevance semantics were a faithful **port** of structure the product already had, not a
patch for a retrieval-quality deficit. Decomposed per feature:

- **(a) Multi-field + recursive-payload recall — the piece THIS design covers — is the MOST
  substitutable.** Its adoption was a *latency* port, and Memex's own forward plan is already willing to
  "rank in Python on the unified surface" (`memex/dev/notes/fathomdb-new-surface-adoption.md:192`), with
  the option-b plan (T3.2) folding multi-field into one ordered `body` column as the parity fallback.
  So the feature in scope here is the **least-necessary** FathomDB addition of the three.
- **(b) Per-column BM25 weights** are genuine product relevance policy (title ≫ metadata ≫ payload),
  but Memex explicitly **does not ask FathomDB to bake weights into the engine** — handle-able
  Memex-side / via CE-rerank. (Non-goal here; §0.)
- **(c) Custom per-kind tokenizers (m006)** are the genuine, **least-substitutable** need:
  precision / no-stemming for two identifier kinds (`WMSemanticMapping`, `WMProvenanceLink`) vs
  recall / stemming for prose — exact-identifier matching that stemming corrupts. This is **out of
  scope** for this design (HITL off-table for 0.8.x, §0) but is flagged as the real residual in §10.

**Net for this design:** carry a *skeptical prior* into the value test — multi-field-recall's lift is
**expected to shrink** once content-modeling into `body` + CE-rerank + a vector arm are all on, because
its original justification was latency, not relevance. §8.4's decision rule is built to detect exactly
that collapse.

---

## 2. The opt-in projection-declaration config (scoped `ProjectionTarget` revival)

Re-introduce a **minimal, default-off** per-kind projection declaration — a scoped revival of the
0.6.0-removed `ProjectionTarget` / `FtsPropertyPath*` surface, cut down to the recall axis only (no
weights, no tokenizer spec).

### 2.1 The config type

```text
FtsProjectionSpec {
    kind: String,                 // the node/edge kind this spec governs
    paths: Vec<FtsPropertyPath>,  // ordered JSON property paths to index
    recursive: Option<FtsRecursiveSpec>,  // optional payload-flatten (see §5); None = off
}

FtsPropertyPath {
    path: String,   // a json_extract path, e.g. "$.summary", "$.metadata.title"
    // NOTE (non-goal): NO `weight` field, NO `tokenizer` field. Recall axis only.
}

FtsRecursiveSpec {
    root: String,        // json_extract path to the subtree to flatten, e.g. "$.payload"
    max_depth: u32,      // bounded tree-walk depth (storage knob, §5)
    max_array_fanout: u32, // bounded array elements per level (storage knob, §5)
}
```

Key properties:

- **Default-off, per-kind.** A kind with **no** `FtsProjectionSpec` registered behaves **exactly as
  today**: single-`body` projection, byte-identical. This mirrors the existing per-kind vector opt-in
  gate `kind_is_vector_indexed` (engine lib.rs:8711-8719), which checks membership in
  `_fathomdb_vector_kinds` and returns `false` (→ no projection) for unregistered kinds. Multi-field
  opt-in must mirror that pattern exactly: a registry table, a membership probe, and a default of
  "behave as before."
- **Allowlisted paths.** `path` values reuse the validated `json_extract` path discipline already in
  the filter machinery (engine lib.rs:1403-1432) and the `PREDICATE_PATH_ALLOWLIST` precedent
  (B-1 §A-1). Paths are validated at registration, not interpolated raw.
- **Probe-gated.** Registration is rejected unless a probe confirms the declared paths resolve to
  TEXT-typed JSON on a sample of existing rows of that kind (fail-closed, like the migration preflight
  at schema lib.rs:234-242). A path that resolves to an object/array (and is not covered by a
  `recursive` spec) is rejected with a typed error.

### 2.2 The admin verb to register it

Extend the existing admin surface rather than inventing a new verb family. `admin_configure`
(`src/rust/crates/fathomdb-py/src/lib.rs:1036-1064`) today only registers an **operational collection**
(`kind ∈ {latest_state, append_only_log}`, B-1 §I-4 line 50). Add a sibling admin operation
`admin.configure_fts(kind, spec_json)` that writes an `FtsProjectionSpec` row into a new registry
table (§3). It rides the same `PreparedWrite`/`Engine::write` path and returns the same
`PyWriteReceipt`, and is exported in the pymodule alongside `admin_configure`
(py lib.rs:1606-1640, where `admin_configure` is wrapped at line 1626).

Registration is itself a write (so it is cursored, replayable, and auditable), and it **does not**
retro-project existing rows — it only changes the projection for **subsequent** writes plus an explicit
**reproject** (§6).

---

## 3. Schema migration (widen the FTS5 vtable; backward-compatible)

FTS5 has no `ALTER … ADD COLUMN` for indexed columns and no `ALTER … tokenize`. The established
pattern in this codebase for FTS5 schema change is **drop + recreate the virtual table, then
reproject from the canonical source rows** — exactly what the tokenizer-upgrade migration step 11 does
(schema lib.rs:268-278, with the `MIGRATION-ACCRETION-EXEMPTION` marker and the
`reproject_search_index_after_tokenizer_upgrade` open-path reindex, engine lib.rs:7776-7802).

### 3.1 Two registry tables (additive, no exemption needed)

```text
-- new migration step (additive CREATE TABLE — no FTS reshape, no exemption marker)
CREATE TABLE IF NOT EXISTS _fathomdb_fts_projection_kinds(
    kind        TEXT PRIMARY KEY,
    spec_json   TEXT NOT NULL,   -- serialized FtsProjectionSpec
    created_at  INTEGER NOT NULL DEFAULT 0
);
```

This table is the **multi-field analog of `_fathomdb_vector_kinds`** (schema lib.rs:171-175). The
membership probe `kind_has_fts_projection(kind)` mirrors `kind_is_vector_indexed`
(engine lib.rs:8711-8719) verbatim in shape.

### 3.2 Widening the FTS5 vtable — column model decision

The design widens `search_index` (and `search_index_edges`) from one indexed column to a small **fixed
set of generic indexed columns** rather than one column per declared path (FTS5 columns are static DDL;
per-kind dynamic columns are impossible in one shared vtable). Two viable shapes — HITL chooses (§10):

- **Shape A — single widened `body` (RECOMMENDED).** Keep exactly one indexed column. Multi-field and
  recursive flattening **concatenate** the extracted texts into the one `body` string at projection
  time (newline-joined, deterministic order). **No vtable reshape at all** — the existing `search_index`
  DDL is unchanged, only the *content* written into `body` changes (§4). This is the smallest, safest
  change and the most backward-compatible: existing stores are byte-identical until reprojected.
  Because ranking is uniform BM25 + CE-rerank with **no per-column weights** (§0 non-goal), there is
  **no functional reason to keep fields in separate columns** — separate columns only matter for
  per-column weighting, which we are explicitly not adding. *Shape A is strongly preferred precisely
  because the weights non-goal removes the only motivation for multiple columns.*

- **Shape B — N fixed extra indexed columns (`field0..fieldK`).** Drop+recreate the vtable adding a
  bounded number of generic indexed columns; map declared paths onto columns positionally. Requires the
  drop+recreate+reproject migration (the step-11 pattern, schema lib.rs:268-278) and is only justified
  if a future per-column need (NOT weights) emerges. Carries the `MIGRATION-ACCRETION-EXEMPTION` marker.

For the remainder of this document, **Shape A is assumed** unless noted: the FTS5 schema is unchanged,
the feature lives entirely in *what text is composed into `body`*, gated per-kind. This keeps the
query path a single `bm25 MATCH` over one column (§9) and makes "default-off == byte-identical" trivial
to guarantee.

### 3.3 Reader-path correctness constraint (MANDATORY — projected text is index-only)

**The projected text must be used for FTS matching ONLY, never returned as the public hit body.** The
current text-search reader selects `search_index.body` *directly* as the returned `SearchHit.body`
(engine lib.rs:6116-6117, `SELECT search_index.body, … FROM search_index … JOIN canonical_nodes cn`),
and the fallback `stable_id` content-hash is `"h:<sha256(body)>"` over that same body
(`derive_stable_id`, engine lib.rs:9027; B-1 §I-7). Under Shape A the FTS `body` column holds the
*augmented/flattened* projection for registered kinds — so if the reader kept returning
`search_index.body` it would (a) leak index text instead of the canonical node body to consumers,
breaking the public search-result contract, and (b) make the content-hash `stable_id` depend on FTS
configuration / reprojection state (a registered-vs-unregistered kind, or a pre-vs-post-reproject
store, would hash to different `stable_id`s for the *same* record).

**Required behavior:** for any registered kind, the reader must source the returned `body` (and the
`stable_id` hash input) from `canonical_nodes` / `canonical_edges` — the reader already JOINs
`canonical_nodes cn` at engine lib.rs:6117, so `cn.body` is in hand — while `search_index.body` is used
**only** in the `bm25 MATCH` predicate. For unregistered kinds the two are identical, so the change is a
no-op there and the default path stays byte-unchanged. This constraint is part of the
`SearchHit.body` / `stable_id` contract (10-fathomdb-side-design.md §1, §5 "default query path
byte-unchanged") and an implementer MUST honor it; it is not optional.

### 3.3 Backward-compatibility

- A migrated store with **no** `FtsProjectionSpec` rows projects exactly as today — the new registry
  table is empty, the probe returns `false`, and the write-path takes the unchanged single-`body`
  branch (§4). **Existing stores are unaffected until explicitly reprojected.**
- The registry-table CREATE is purely additive (matches the accretion-guard `names_addition` branch;
  no exemption marker required, unlike the FTS drop+recreate).

---

## 4. Write-path projection changes (gated, mirroring the vector-index gate)

Today the node write unconditionally projects `body` (engine lib.rs:9596-9599). The change adds a
**per-kind gate** in front of body composition, mirroring the vector gate at engine lib.rs:9600
(`if kind_is_vector_indexed(&tx, kind) … else …`):

```text
// at the node write site (engine lib.rs:9591-9599):
let projected_body = match lookup_fts_projection(&tx, kind)? {
    None       => body.clone(),                       // unchanged single-body path
    Some(spec) => compose_projection(&body, &spec)?,  // multi-field + recursive flatten (§5)
};
tx.execute(
    "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
    params![projected_body, kind, cursor],
)?;
```

Where `compose_projection`:

1. Starts with the existing `body` text (so the default content is never *lost*, only *augmented*).
2. For each `FtsPropertyPath`, runs the validated `json_extract(body,'$.path')` (reuse the path
   machinery at engine lib.rs:1403-1432) and appends the extracted TEXT.
3. If `recursive` is set, tree-walks the declared subtree per §5 and appends the flattened text.
4. Joins deterministically (newline) and returns the composed string.

The same gate is added at the edge write site (engine lib.rs:9703-9708), reading the edge-kind spec.
Crucially: the gate is **the only new branch on the hot write path**; an unregistered kind takes the
identical `body.clone()` path it takes today, so **default-off writes are byte-identical**.

Cost note: this is a **write-time** cost (extra `json_extract` calls + string build per write), not a
query-time cost (§9).

---

## 5. Recursive-payload flatten (bounded tree-walk → indexed text)

For nested JSON payloads (e.g. a `payload` object with arbitrarily nested sub-objects and arrays),
`compose_projection` performs a **bounded depth-first tree-walk** rooted at `recursive.root`, emitting
the **leaf scalar values** (strings, and stringified numbers/bools if desired) as indexed text. Keys
are optionally emitted to make key tokens searchable (HITL knob, §10).

The bound is the load-bearing safety property — an unbounded walk over a large nested payload can blow
up the FTS index size and tokenization time:

- **`max_depth`** caps recursion depth; subtrees deeper than the cap are dropped (not error).
- **`max_array_fanout`** caps how many elements of any one array are walked; excess elements are
  dropped. This prevents a single large array (e.g. thousands of log lines) from dominating the index.
- Both are **storage knobs**: they trade recall coverage against index size / write latency. The design
  must surface them in the spec and document the trade-off; defaults should be conservative
  (e.g. `max_depth=4`, `max_array_fanout=16`) so an accidental registration cannot explode storage.

**Storage-knob call-out (REQUIRED).** Recursive flattening is the one part of this design that can
materially grow the on-disk FTS index. The `max_depth` × `max_array_fanout` product bounds the worst-case
text emitted per write. HITL must sign off on the defaults, and the value test (§8) should report index
**size delta** alongside recall, so the storage cost of any recall gain is visible.

---

## 6. Dual rebuild-path changes (both must honor the new projection)

There are **two** code paths that reproject FTS from canonical source rows, and **both** must learn the
new projection or they will silently rebuild the *old* single-`body` content and erase the multi-field
augmentation:

1. **Shadow rebuild** (`include_fts` branch, engine lib.rs:5031-5062). Today it re-inserts
   `row.body` verbatim into `search_index` (line 5033-5037) and edge bodies into `search_index_edges`
   (5055-5061). It must instead route each row through `compose_projection` under that row's kind spec
   — identical gating logic to §4. (Same change in both the node loop and the edge loop.)

2. **Tokenizer-upgrade reproject** (`reproject_search_index_after_tokenizer_upgrade`,
   engine lib.rs:7776-7802). After a tokenizer drop+recreate it deletes and re-inserts every
   `canonical_node_rows` body (lines 7780-7787). It too must compose via the per-kind spec, or a
   tokenizer upgrade would quietly drop all multi-field content.

A shared `compose_projection(kind, body)` helper, called from all three sites (write §4 + both
rebuilds), is the only safe factoring — it guarantees the projection is identical on first-write and on
every rebuild. **This single-helper invariant is a correctness requirement, not a style preference**:
divergence between write-time and rebuild-time projection would make a store's FTS content depend on
*when* it was last rebuilt.

A new **explicit reproject** entry point (triggered by `admin.configure_fts` and exposed for
operational use) reuses the same shadow-rebuild path (engine lib.rs:5031-5062) so that registering a
spec can backfill existing rows on demand.

---

## 7. Backward-compat / default-off / migration-safety

- **Default-off is byte-identical.** No `FtsProjectionSpec` registered ⇒ empty registry ⇒ probe
  returns `false` ⇒ write-path and both rebuild-paths take the unchanged `body` branch. A store that
  never calls `admin.configure_fts` is indistinguishable from today's behavior at the byte level
  (mirrors the vector-gate "non-vector-indexed nodes behave as before" property at
  engine lib.rs:9607-9611).
- **Existing stores unaffected until reprojected.** The migration adds an empty registry table only;
  it does **not** reshape `search_index` (Shape A) and does **not** touch existing FTS content.
  Multi-field content appears only for rows written after a spec is registered, or after an explicit
  reproject.
- **Forward-only, additive migration.** The registry-table CREATE follows the additive accretion path
  (no exemption marker). If Shape B is ever chosen, its FTS drop+recreate carries the
  `MIGRATION-ACCRETION-EXEMPTION` marker exactly like step 11 (schema lib.rs:270).
- **Crash-safety reuses existing patterns.** The reproject rides the same single-transaction
  durable-marker idiom as the tokenizer reproject (engine lib.rs:7776-7802): reindex + completion
  marker commit together; a crash before commit re-runs, after commit skips. Idempotent.
- **Query path unchanged.** Reads still issue one `bm25 MATCH` over `search_index` (Shape A keeps one
  indexed column); no consumer query API changes. This preserves the §1 invariant from
  10-fathomdb-side-design.md (default query path byte-unchanged).

---

## 8. VALUE TEST (REQUIRED — the load-bearing section)

**This is the gate. It must pass before any of §2–§7 is built.** The question it answers:

> *Is multi-field / recursive-payload indexing genuinely valuable, or was it a workaround for earlier
> weak search?*

The discriminating intuition (HITL 2026-06-30, B-1 §I-4): multi-field changes **recall** (what enters
the pool). If the modern stack — **CE-rerank + a vector retrieval arm** — already pulls the same
documents into the answerable set, then multi-field's apparent gain is a relic of an era when lexical
single-`body` search was the only arm. The test must therefore isolate multi-field's contribution
**with the modern stack ON**.

**Skeptical prior (REQUIRED, from the §1.3 provenance finding).** Enter this test *expecting*
`Δ_persist` to be small. The original multi-field structure was a **latency** port, not a relevance
fix, and is the most substitutable of the three FTS pieces; Memex already plans to rank on a unified
`body` surface. So the null hypothesis is "multi-field adds ~0 once content-model + CE-rerank + vector
are on," and the burden of proof is on multi-field to clear §8.4's bar despite that prior — not the
reverse. A large `Δ_naked` is the *expected, uninformative* result and must not be mistaken for value.

### 8.1 Corpus + gold query set

Reuse the 0.8.x eval corpora and the per-corpus `decide_08x` decision hook (P0-4.3,
10-fathomdb-side-design.md §3), so this test slots into the existing screening harness:

- **MuSiQue** (multi-hop, 2,417 rows, `question_decomposition` preserved per P0-3) — the corpus most
  likely to *need* multi-field, because answer evidence is spread across fields/passages. **Primary.**
- **HotpotQA** (multi-hop) — confirmation of the MuSiQue signal on a second multi-hop corpus.
- **LME / LOCOMO** (single/multi-session memory) — corpora where records have rich nested payloads
  (conversation turns, metadata); these exercise the *recursive-payload* arm specifically.

Each corpus carries its own `decide_08x` verdict (per-corpus, not one global gate). The gold query set
is the corpus's existing gold (question → gold-supporting passages/records). **To exercise the feature
at all, the corpus must be ingested so that searchable text lands in a non-`body` property** for the
multi-field arm (otherwise single-`body` and multi-field are identical and the test is vacuous — call
this out as a harness precondition).

### 8.2 Ablation matrix

Run the full `2 × 2 × 2` matrix on each corpus:

```text
{ indexing:    single-body  |  multi-field }
  × { ce_rerank:  off  |  on (alpha=1.0, pool_n=10 — measured-parity, B-1 §I-3) }
  × { vector_arm: off  |  on (default bge-small embedder + KNN) }
```

= 8 cells per corpus. The two diagnostic contrasts:

- **Naked recall lift:** `multi-field` vs `single-body` with **ce_rerank=off, vector=off**. This is the
  maximum apparent value of multi-field (pure lexical, no modern stack). A large lift here is *expected*
  and is **not sufficient** to justify the feature.
- **Persisted lift (the real gate):** `multi-field` vs `single-body` with **ce_rerank=ON and
  vector=ON**. This is multi-field's *marginal* contribution on top of the modern stack.

### 8.3 Metrics

- **recall@k** (k ∈ {10, 20, 50}) — fraction of gold evidence present in the top-k pool. Primary
  recall metric.
- **gold-in-pool** — fraction of queries for which *all* required gold evidence entered the candidate
  pool (binding for multi-hop: a single missing hop fails the chain). This is the metric multi-field
  most directly moves.
- **nDCG@10** — reported for completeness to confirm multi-field does **not** *degrade* ranking (it
  should not, since ranking is uniform BM25 + CE-rerank and unchanged); nDCG is a guard, not the gate.
- **FTS index size delta** (bytes) and **write-latency delta** — the cost side of any recall gain
  (especially for the recursive arm; §5 storage knob).

### 8.4 The DISCRIMINATING decision rule

Let `Δ_naked` = (gold-in-pool multi-field − gold-in-pool single-body) at **ce_rerank=off, vector=off**,
and `Δ_persist` = the same difference at **ce_rerank=ON, vector=ON**.

> **Multi-field is genuinely valuable IFF `Δ_persist` ≥ +0.05 absolute gold-in-pool (equivalently
> recall@20 ≥ +0.05), with the lower bound of its 95% CI > 0, on at least the primary corpus
> (MuSiQue) AND one other — AND this lift PERSISTS with CE-rerank + vector retrieval ON.**
>
> **If `Δ_persist` collapses toward 0 (CI includes 0, or point estimate < +0.02) once the modern
> stack is ON — even when `Δ_naked` is large — then multi-field was a WORKAROUND for weak single-arm
> lexical search, and FathomDB does NOT build it.** Memex recovers via content-modeling into `body`
> (the standing R-I4 resolution).

Rationale: `Δ_naked` large ∧ `Δ_persist` ≈ 0 is the *exact signature* of a feature whose value is
already delivered by the modern retrieval stack. The +0.05 threshold mirrors the decisive-margin
discipline used elsewhere in 0.8.x gating (e.g. the M1 graph-arm CI-upper +0.04 rule). The
storage/latency deltas are a **secondary veto**: even a passing `Δ_persist` is reconsidered if the
recursive arm's index-size blow-up is disproportionate to the recall bought.

### 8.5 What a pass / fail produces

- **PASS** (on ≥2 corpora incl. MuSiQue) → multi-field clears the high bar; promote this design-on-spec
  to a build plan, HITL governance sign-off on re-opening the projection surface (§9) still required.
- **FAIL** → record the verdict; FathomDB ships **no** FTS extension; close R-I4 as "content-model into
  `body`" permanently. This is the expected-default outcome given the 2026-06-30 resolution.

---

## 9. Cost / effort + governance

### 9.1 Engineering cost — MEDIUM

New surface area: a config type (`FtsProjectionSpec` + serde), a registry table + membership probe
(mirrors `_fathomdb_vector_kinds` / `kind_is_vector_indexed`), the shared `compose_projection` helper,
the write-path gate (engine lib.rs:9596-9599 + 9703-9708), **both** rebuild-path edits
(engine lib.rs:5031-5062 + 7776-7802), an admin verb (`admin.configure_fts`, extending py
lib.rs:1036-1064 + exports 1606-1640), and Python/TS bindings. Plus the §8 eval harness. Shape A keeps
the schema migration trivial (one additive CREATE TABLE); Shape B adds the FTS drop+recreate+reproject.

### 9.2 Runtime cost — WRITE + STORAGE only, NOT per-query

- **Write:** extra `json_extract` calls + string composition per write of a registered kind. Linear in
  the number of declared paths and (for recursive) bounded by `max_depth × max_array_fanout`.
- **Storage:** larger FTS index for registered kinds (the §5 knob bounds the blow-up).
- **Query:** **unchanged** — still a single `bm25 MATCH` over one indexed `body` column (Shape A). No
  per-query cost, no new query API. This is a deliberate design constraint: the feature must not make
  reads slower or more complex.

### 9.3 Governance note (REQUIRED)

This design **re-opens the projection-declaration surface that 0.6.0 deliberately removed**
(`ProjectionTarget` / `FtsPropertyPathSpec` / `FtsPropertyPathMode`; B-1 §I-4 line 52). Re-introducing
even a scoped, recall-only subset is a reversal of a prior simplification decision and must be an
explicit HITL governance call — not an incremental feature add. The scoping discipline (recall-only;
no weights, no tokenizers; default-off; probe-gated) is what keeps the revival *minimal*, but it is
still a revival. **Do not build without that sign-off, and not before §8 passes.**

---

## 10. Open questions for HITL

1. **Build at all?** Given the 2026-06-30 resolution (no FTS extension; content-model into `body`), is
   there a concrete Memex case where content-modeling into `body` is genuinely infeasible — or is this
   design purely contingent insurance? (If the latter, it stays a design-on-spec and §8 is never run.)
2. **Shape A vs Shape B** (§3.2). Shape A (single widened `body`, no vtable reshape) is recommended
   precisely because the per-column-weights non-goal removes the only reason for multiple columns. Any
   objection?
3. **Recursive defaults** (§5). Approve `max_depth` / `max_array_fanout` defaults; confirm conservative
   bounds so an accidental registration cannot explode storage.
4. **Key emission** (§5). Should object keys be emitted as searchable tokens, or only leaf values?
5. **Value-test threshold** (§8.4). Is `Δ_persist ≥ +0.05` gold-in-pool (CI lower bound > 0, ≥2 corpora)
   the right bar, or should it be higher given the governance cost of re-opening the surface?
6. **Corpora** (§8.1). Confirm MuSiQue (primary) + HotpotQA + LME/LOCOMO, and that the harness ingests
   searchable text into a non-`body` property so the test is non-vacuous.
7. **Storage veto** (§8.4). Should a disproportionate index-size blow-up veto an otherwise-passing
   recall gain, and at what ratio?
8. **The harder-to-replace residual is NOT this feature — it is precision tokenization by kind**
   (§1.3c, FLAGGED for HITL though out of this design's scope). The provenance finding identifies
   custom per-kind tokenizers (m006) as the *least*-substitutable Memex need: exact-identifier,
   no-stemming matching for `WMSemanticMapping` / `WMProvenanceLink` (stemming corrupts identifier
   matches) vs recall/stemming for prose. That need is currently HITL-off-table for 0.8.x (§0). Given
   that multi-field-recall (this design) is the *most* substitutable piece while the precision-tokenizer
   need is the *least*, should HITL re-prioritize — i.e. is engineering attention better spent
   reconsidering the tokenizer-by-kind question than building this multi-field surface? This is noted
   for HITL's portfolio decision; it does **not** widen this design's scope.
