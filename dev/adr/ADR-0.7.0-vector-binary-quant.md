---
title: ADR-0.7.0-vector-binary-quant
date: 2026-05-27
target_release: 0.7.0
desc: Binary quantization (bit[768]) plus f32 rerank on the existing sqlite-vec extension, with metadata + partition_key schema migration, as the data-encoding change that closes AC-013 within the proposed 80 / 300 ms latency envelope. AC-019 keeps its existing 10× bound and is collaterally re-measured under the new query path. Architectural-lever accounting reaffirmed: PCACHE2 remains the 0.7.0 architectural lever; this is a data-encoding change.
blast_radius: src/rust/crates/fathomdb-engine/src/lib.rs:2317-2323 (read_search_in_tx hot SQL; Pack 2 rewrite); src/rust/crates/fathomdb-engine/src/lib.rs:2846 (_fathomdb_vector_rows writer insert; Pack 1 double-write); src/rust/crates/fathomdb-engine/src/lib.rs:3278-3283 (vector_default CREATE VIRTUAL TABLE; Pack 1 schema migration); src/rust/crates/fathomdb-engine/src/lib.rs:3107 (register_sqlite_vec_extension; unchanged but inspected); src/rust/crates/fathomdb-engine/src/lib.rs:3248-3260 (_fathomdb_embedder_profiles; UNCHANGED — embedder contract preserved); src/rust/crates/fathomdb-engine/src/lib.rs:2174,2242 (writer-loop pins; unchanged); src/rust/crates/fathomdb-engine/tests/perf_gates.rs:149-150 (AC013_BUDGET_P50/P99 re-pin in Pack 2); src/rust/crates/fathomdb-engine/tests/perf_gates.rs:487 (ac_013_vector_retrieval_latency); src/rust/crates/fathomdb-engine/tests/perf_gates.rs:609 (ac_019_mixed_retrieval_stress_workload_tail); src/rust/crates/fathomdb-engine/tests/perf_gates.rs new ac_NNN_recall_at_10 test (Pack 2); src/rust/crates/fathomdb-engine/Cargo.toml:18 (sqlite-vec pin; stays =0.1.7); dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md (numeric lock-flip target, post Pack 2); dev/notes/pcache2-followups.md (post-0.7.0 ANN follow-ups)
status: draft, HITL-required
---

# ADR-0.7.0 — Vector binary quantization (Pack 1 + Pack 2)

**Status:** draft, HITL-required.

This ADR records the data-encoding decision that closes AC-013
within the proposed 80 / 300 ms envelope in 0.7.0. AC-019 is
collaterally re-measured under the new query path — its existing
10× stress-bound is expected to improve proportionally but its
budget shape is unchanged and its numeric row in the budgets ADR
remains a placeholder pending the post-Pack-2 canonical-CI run
(see § 6). It is paired with — and feeds into — the numeric
lock-flip that will be made post-Pack-2 against
`dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md`. That ADR
remains the budget owner; this ADR commits the architectural shape
that makes those budgets reachable.

## 1. Problem

Canonical-CI measurement on 2026-05-27 under the locked
W4.1-stacked-O1 (`L-Y1+L-Y2+L-B5+L-B4+L-D1-pcache2-O1`) stack
(`dev/plans/runs/0.7.0-PERF-EXP-W4.1-ac013-canonical-output.json`)
reports AC-013 at **p50 = 2048 ms, p99 = 2327 ms** on the
N = 1,000,000 / dim 768 f32 fixture seeded by `VaryingEmbedder`
(`src/rust/crates/fathomdb-engine/tests/perf_gates.rs:296` and
`:317`). AC-019 stress p99 = 8388 ms under bound 20964 ms; the
bound is met today but the underlying per-query cost is the same
hot path.

The proposed budgets in
`dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md` are
**80 ms p50 / 300 ms p99**. The measurement is therefore
**~25–40× over** the budget envelope. The current
`AC013_BUDGET_P50 = 50ms` / `AC013_BUDGET_P99 = 200ms` constants in
`src/rust/crates/fathomdb-engine/tests/perf_gates.rs:149-150` are
the unindexed-path-pre-lock values; the revised budgets ADR drafts
80 / 300 ms but they have not yet been pinned in the test, by
design (the re-pin is itself the Pack 2 RED gate).

**Root cause.** The hot SQL at
`src/rust/crates/fathomdb-engine/src/lib.rs:2317-2323`

```
SELECT rowid FROM vector_default
WHERE embedding MATCH vec_f32(?1)
ORDER BY distance
LIMIT 10
```

drives a per-query O(N) brute-force scan over the
`vector_default` vec0 table declared at
`src/rust/crates/fathomdb-engine/src/lib.rs:3278-3283` as
`USING vec0(embedding float[<dim>])` — i.e. a single global f32
table with no metadata columns, no partition keys, no auxiliary
index. At N = 1 M × dim 768 × 4 B that is 3.0 GB of memory
streamed per query; the observed 2048 ms p50 corresponds to
~1.5 GB/s, which sits at the single-thread memory-read ceiling
on the canonical AMD EPYC 7763 runner. No SQLite engine knob can
move this — the bytes-per-row × rows-per-query product is the
floor. To close the gate we must either shrink the bytes per row,
shrink the row count visited per query, or both.

Cross-reference: `dev/notes/0.7.0-vector-cost-research.md`
§ "Problem restated" through § "Tier 1 combined estimate" enumerates
the lever surface; the present ADR commits to the Tier-1 #4 +
Tier-1 #1/#2 bundle as the load-bearing decision.

## 2. Decision

Adopt the following, to ship in two packs within the 0.7.0 release
line (Pack 1 then Pack 2; pack boundary is for review granularity,
not release-boundary):

1. **Binary quantization** of the f32 embedding into a sibling
   `bit[768]` column on the existing `vector_default` vec0 table
   (Pack 1.1 default per
   `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`
   § 1.1). The bit column is computed via sqlite-vec's
   `vec_quantize_binary(embedding)` and written inside the same
   writer transaction as the existing f32 insert. Sibling-table
   fallback (`vector_default_bin`) is selected only if the
   sibling-column shape hits the vec0 metadata column limit
   (16 cols). The f32 `embedding` column is **retained** for
   the rerank phase and for the recall-floor ground-truth pass —
   it has two jobs after Pack 1.

2. **Two-phase query** (Pack 2): bit-KNN top-K=64 candidates →
   f32 rerank → top-10. The exact SQL shape (subject to the
   sqlite-vec syntax confirmation called out in
   `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md` § 2.2)
   replaces the single-phase scan at
   `src/rust/crates/fathomdb-engine/src/lib.rs:2317-2323`. K is
   a named constant / config knob so future tuning is cheap; the
   K = 64 default is taken from the Kyle Howells and Cohere
   binary-quant benches cited in
   `dev/notes/0.7.0-vector-cost-research.md` § Tier 1 #4.

3. **Schema extension shipped on the same migration** (Pack 1):
   - `source_type TEXT` as a vec0 **partition_key**
     (HITL-locked 2026-05-27 in
     `dev/plans/0.7.0-HITL-recommendations.md`; cardinality ~6:
     email, article, paper, meeting, note, todo). Maps from the
     existing chunk-record vector kind at write time
     (deterministic 1:1 mapping for the current single-kind
     fixture; `source_type` is the authoritative going-forward
     vocabulary).
   - Additional metadata columns (`kind`, `created_at`, `tags`,
     `project_mentions`, …) populated from the existing chunk
     record. Per HITL Q5 in
     `dev/plans/0.7.0-HITL-recommendations.md` and
     `dev/notes/0.7.0-vector-cost-research.md` Tier 1 #1,
     descriptive retrieval filters live in metadata columns +
     WHERE predicates, not in partition keys. The vec0 limit is
     16 metadata columns; the Pack 1 schema MUST budget headroom.

4. **Recall floor: ≥ 0.90 recall@10** vs f32 brute-force ground
   truth on the AC-013 fixture
   (`dev/plans/0.7.0-HITL-recommendations.md`,
   `dev/notes/0.7.0-vector-cost-research.md` § HITL responses #1).
   Enforced by a new perf-gates test (`ac_NNN_recall_at_10`)
   landed RED in Pack 2 before the query rewrite. Ground truth
   is computed at test-setup via a one-pass brute-force scan
   through the retained f32 column — single corpus, two jobs.

5. **sqlite-vec pin stays `=0.1.7`**
   (`src/rust/crates/fathomdb-engine/Cargo.toml:18`). 0.1.7
   already ships `bit[N]` and `vec_quantize_binary` per the
   sqlite-vec release notes
   (`dev/notes/0.7.0-vector-cost-research.md` Tier 1 #4
   citation). Bump to `=0.1.9` is allowed in Pack 1 **only if
   needed** for constraint-column refinements; not required and
   not recommended without justification.

6. **Embedder unchanged.** The `EmbedderIdentity` contract,
   `VaryingEmbedder`, and the profile-pinning logic at
   `src/rust/crates/fathomdb-engine/src/lib.rs:3248-3260` are
   not touched. HITL Q2 in
   `dev/notes/0.7.0-vector-cost-research.md` § HITL responses
   locked: KEEP existing embedder. Tier-1 #3 (smaller
   embeddings) and Tier-1 #5 (matryoshka) are out of scope
   for 0.7.0.

7. **Single-writer + projection-cursor contracts preserved.**
   Both column inserts commit in the same writer transaction at
   the `_fathomdb_vector_rows` insert site near
   `src/rust/crates/fathomdb-engine/src/lib.rs:2846`. The
   writer-loop pins at `:2174` and `:2242` are not touched.
   AC-017 / AC-018 / AC-020 behavior is preserved.

## 3. Alternatives considered (and rejected)

Each alternative below was considered against the AC-013 gap; the
research doc carries the long-form analysis.

- **Vectorlite (HNSW SQLite extension).** Mechanism: load a
  separate `USING vectorlite(...)` virtual table backed by an
  HNSW graph stored in shadow tables. Rejected because the
  upstream has had no release since 2024-08-19, has no
  published 1 M × 768 benchmark, and counts as a structural
  architectural lever stacked on top of PCACHE2 — violates the
  one-architectural-lever-per-release rule. Cited:
  `dev/notes/0.7.0-vector-cost-research.md` § Tier 2.

- **sqlite-vec ANN alpha (v0.1.10-alpha.1).** Mechanism: use
  the upstream-experimental ANN indices (`rescore`, `ivf`,
  `DiskANN`). Rejected because the release is alpha-tagged and
  upstream classifies it as not production-ready. Cited:
  `dev/notes/0.7.0-vector-cost-research.md` § Tier 3,
  sqlite-vec releases.

- **Rust-side `usearch` / `instant-distance`.** Mechanism:
  pull KNN out of SQLite into a Rust-side ANN library and
  rebuild the search verb around it. Rejected because the
  blast radius (leaving SQLite for the KNN step, snapshot /
  freshness handshake redesign, AC-017 cursor-freshness
  re-derivation) is far larger than two packs of in-place
  schema + query work, and the data-encoding lever closes the
  gap without it. Cited:
  `dev/notes/0.7.0-vector-cost-research.md` § Tier 3 (integration
  shape C).

- **Embedder swap (dim reduction 768 → 384 or 256).**
  Mechanism: replace `VaryingEmbedder` with a smaller embedder
  to shrink bytes-per-row. Rejected by HITL Q2 lock 2026-05-27:
  keep existing embedder. Rationale: switching embedders
  requires a profile rotation through
  `_fathomdb_embedder_profiles` and a corpus rebuild
  (writer-cooperation cost), and even at dim 128 the
  arithmetic-only saving is ~6× — well short of the ~25–40×
  gap. Cited:
  `dev/notes/0.7.0-vector-cost-research.md` § Tier 1 #3,
  § HITL responses.

- **Partitioning alone (metadata prefilter / partition_key
  without quantization).** Mechanism: keep the f32 column,
  add `source_type` partition_key + metadata columns, push
  filters into the WHERE clause. Rejected as load-bearing:
  the AC-013 fixture is single-kind, so partitioning yields
  zero benefit at the gate. Partitioning is **kept** as a
  bundled deliverable in Pack 1 because it is the correct
  shape for real workloads and avoids a second migration in
  0.7.1 — but it does not close the gate on its own. Cited:
  `dev/notes/0.7.0-vector-cost-research.md` § Tier 1 #1, #2.

## 4. Consequences

- **Schema change (Pack 1).** `vector_default` gains the
  `embedding_bin bit[768]` column, the `source_type` vec0
  partition_key, and additional metadata columns (within the
  vec0 16-column budget). Migration is a one-pass
  `INSERT … SELECT vec_quantize_binary(embedding) FROM
  vector_default` populating the bit column for the existing
  corpus. Migration cost: ~3 GB read + ~96 MB write on the
  1 M-row test corpus; ~64 s on the dev box per the handoff
  estimate, well under the workflow timeout.

- **Ingest cost (Pack 1).** The writer at the
  `_fathomdb_vector_rows` insert site near
  `src/rust/crates/fathomdb-engine/src/lib.rs:2846` performs
  a single additional `vec_quantize_binary()` and one extra
  column write per row inside the same writer transaction.
  No new commit; no new lock; no new SQLite handle. The
  single-writer + projection-cursor freshness contract is
  preserved bit-for-bit. Storage grows by ~3 % (96 B per row
  on top of the 3072 B f32 footprint).

- **Query-path rewrite (Pack 2).** Replace
  `src/rust/crates/fathomdb-engine/src/lib.rs:2317-2323` with
  the two-phase shape outlined in
  `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md` § 2.2.
  No change to the writer side, no change to the projection-
  cursor handshake, no change to `register_sqlite_vec_extension`
  at `src/rust/crates/fathomdb-engine/src/lib.rs:3107`. K = 64
  is named as a constant for future tuning.

- **Recall floor enforced as a RED test.** A new perf-gates
  test (e.g. `ac_NNN_recall_at_10`) computes brute-force f32
  ground truth at test-setup against the retained f32 column,
  runs the production search path, and asserts
  `recall@10 ≥ 0.90` on the AC-013 fixture. Lands RED in
  Pack 2.1 alongside the latency budget re-pin
  (`tests/perf_gates.rs:149-150` → 80 / 300 ms) and must
  remain RED at canonical-CI N = 1 M scale until Pack 2.2's
  query rewrite lands. Both gates GREEN at canonical-CI is
  the Pack 2 close criterion.

- **AC-019 carry.** The existing 10× bound on AC-019 stays
  unchanged; the same two-phase query shape benefits AC-019
  proportionally. AC-019 is expected to improve substantially
  but no new bound is pinned here.

- **AC-012 / AC-017 / AC-018 / AC-020 unchanged.** The
  writer-loop pins (`:2174`, `:2242`), the embedder profile
  table, and the reader-pool concurrency surface are all
  untouched. Pack 1 changes schema but not query semantics, so
  these tests stay GREEN through both packs.

- **Post-0.7.0 follow-ups.** Graph-index ANN (vectorlite,
  sqlite-vec ANN alpha, Rust-side `usearch`) and
  matryoshka / smaller-embedder work are tracked in
  `dev/notes/pcache2-followups.md` for 0.7.1+ if real-workload
  telemetry shows the binary-quant scan is insufficient.

## 5. Architectural-lever accounting

The 0.7.0 architectural lever is **`SQLITE_CONFIG_PCACHE2`**, locked
by `dev/adr/ADR-0.7.0-ac020-architectural-lever.md`. The decision
in § 2 of this ADR is a **data-encoding change** (schema +
quantization function + two-phase SQL), not a second architectural
lever:

- It does not change the SQLite build, the SQLite vendor, the WAL
  format, or the page-cache allocator.
- It does not change the reader-pool topology, the writer-lock
  shape, the projection-cursor contract, or the snapshot-freshness
  contract.
- It does not change the embedder identity or the profile-pinning
  protocol.
- It changes only the bytes stored per row in the vec0 table and
  the SQL the reader emits against it.

This disposition is reaffirmed verbatim from
`dev/notes/0.7.0-vector-cost-research.md` § "Architectural-lever
accounting". The HITL doc records the same call.

**Fallback.** If a future reviewer (codex or HITL) counts
binary-quant + rerank as a second architectural lever stacked on
PCACHE2, the agreed fallback per the handoff and the research doc
is to ship **Pack 1 only in 0.7.0** (schema migration + ingest
double-write; no reader behavior change; AC-013 stays at
2048 / 2327 ms) and slip **Pack 2 to 0.7.1** (query rewrite + RED
tests + lock-flip). The Pack 1-only shape preserves the on-disk
shape for the future Pack 2 cutover and avoids a second migration.
Escalation trigger is called out in
`dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md` § "Open
items the implementing agent should escalate to HITL".

## 6. Open questions / HITL gates

- **Final numeric AC-013 / AC-019 budgets.** The 80 / 300 ms
  numbers in § 1 are the proposed envelope from
  `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md`; the
  final pinned values land after the P2-CANONICAL run, HITL
  signs the numeric budgets, and the lock-flip is applied to the
  budgets ADR. The Pack 2 perf-gates re-pin
  (`src/rust/crates/fathomdb-engine/tests/perf_gates.rs:149-150`)
  uses 80 / 300 ms initially; HITL may tighten by ~10 % headroom
  against the measured numbers at lock time.

- **K-value selection.** Default K = 64 from the binary-quant
  literature. If the recall-floor test falls below 0.90 at K = 64
  on canonical-CI scale, escalate before Pack 2 GREEN-test commit
  (per `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`
  § "Open items"). K may need to rise to 128 or 256; the choice
  is documented in the closing slice JSON and reflected in the
  named constant.

- **vec0 metadata column budget.** The vec0 16-column cap
  constrains the Pack 1 schema. If the desired metadata schema
  (including future fields for the corpus / API work tracked
  separately) cannot fit alongside the partition_key + bit
  column, escalate before Pack 1 schema design freezes. Fallback
  is the sibling-table shape (`vector_default_bin`) called out
  in § 2 / Pack 1.1.

- **`vec_quantize_binary()` input expectations.** `VaryingEmbedder`
  produces L2-normalised f32 vectors
  (`src/rust/crates/fathomdb-engine/tests/perf_gates.rs:322-326`),
  and the research note records the same
  (`dev/notes/0.7.0-vector-cost-research.md:164-165`). Binary
  quantization via `vec_quantize_binary()` is a sign-pattern
  transform that is invariant to positive scaling, so L2 norm
  itself is not load-bearing, but the function's exact input
  contract (signed-zero handling, NaN behaviour) has not been
  validated against `VaryingEmbedder` output. If sqlite-vec 0.1.7's
  `vec_quantize_binary()` rejects the f32 blobs produced by the
  current writer path or returns degenerate bit patterns, escalate
  before Pack 1 ingest changes land (per the handoff § "Open
  items"). The fixture concentrates mass on 6 of 768 coordinates
  (`src/rust/crates/fathomdb-engine/tests/perf_gates.rs:317`),
  which is atypically friendly to sign-pattern quantisation —
  HITL is owed the note that AC-013's recall headroom may
  overstate real-workload headroom.

## 7. Status / next actions

- **Status: `draft, HITL-required`.** No lock-flip happens in this
  ADR until canonical-CI is GREEN at the re-pinned budgets and
  the recall floor and HITL signs the numeric budgets in
  `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md`.

- **Immediate next slice.** Orchestrator spawns a codex reviewer
  on this ADR draft, then promotes the verdict per
  `dev/design/orchestration.md` § 4. If review is PASS, Pack 1
  schema-migration + ingest-double-write slice is next.

- **Lock-flip sequencing.** Per the handoff § "Done = ?", this
  ADR moves to `status: locked` only after: Pack 1 lands;
  Pack 2 lands; canonical-CI dispatch shows AC-013 GREEN at the
  re-pinned 80 / 300 ms (or HITL-revised) budgets; the new
  recall-floor test is GREEN at ≥ 0.90; AC-019 verdict is
  re-measured under the new query path; HITL signs off on the
  numeric budgets in the revised-budgets ADR. Lock-flip is
  orchestrator's; this draft does not flip prematurely.

## Citations

- `dev/plans/prompts/0.7.0-PERF-VECTOR-QUANT-HANDOFF.md`
  (this ADR's mandating handoff; § Step 0 enumerates the
  must-cover items).
- `dev/notes/0.7.0-vector-cost-research.md` (HITL-RESPONDED;
  Tier 1 / 2 / 3 catalogue; Pack 1 / 2 split locked).
- `dev/plans/0.7.0-HITL-recommendations.md` (HITL decision
  record: recall floor 0.90 @ k=10; `source_type` partition_key;
  cardinality ~6; sqlite-vec 0.1.7).
- `dev/adr/ADR-0.7.0-ac020-architectural-lever.md` (PCACHE2 is
  the 0.7.0 architectural lever; pattern reference for this
  ADR's structure).
- `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md`
  (numeric budget owner; lock-flip target post-Pack-2).
- `dev/plans/runs/0.7.0-PERF-EXP-W4.1-ac013-canonical-output.json`
  (canonical-CI measurement: p50 = 2048 ms, p99 = 2327 ms at
  N = 1 M, W4.1-stacked-O1 stack, 2026-05-27).
- `src/rust/crates/fathomdb-engine/src/lib.rs:2317-2323` (hot
  SQL; Pack 2 rewrite site).
- `src/rust/crates/fathomdb-engine/src/lib.rs:2846`
  (`_fathomdb_vector_rows` writer insert; Pack 1 double-write
  site).
- `src/rust/crates/fathomdb-engine/src/lib.rs:3278-3283`
  (`vector_default` CREATE VIRTUAL TABLE; Pack 1 schema
  migration site).
- `src/rust/crates/fathomdb-engine/src/lib.rs:3107`
  (`register_sqlite_vec_extension`; inspected, unchanged).
- `src/rust/crates/fathomdb-engine/src/lib.rs:3248-3260`
  (`_fathomdb_embedder_profiles`; UNCHANGED — embedder identity
  contract preserved).
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:149-150`
  (`AC013_BUDGET_P50` / `AC013_BUDGET_P99`; Pack 2 re-pin
  target).
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:487`
  (`ac_013_vector_retrieval_latency`).
- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs:609`
  (`ac_019_mixed_retrieval_stress_workload_tail`).
- `src/rust/crates/fathomdb-engine/Cargo.toml:18`
  (`sqlite-vec = "=0.1.7"`).
- sqlite-vec binary quantization guide:
  https://alexgarcia.xyz/sqlite-vec/guides/binary-quant.html
- sqlite-vec API reference (DeepWiki):
  https://deepwiki.com/asg017/sqlite-vec/3-api-reference
- sqlite-vec metadata + partition release:
  https://alexgarcia.xyz/blog/2024/sqlite-vec-metadata-release/index.html
- Kyle Howells binary-quant benchmark:
  https://ikyle.me/blog/2025/binary-quantized-embeddings
