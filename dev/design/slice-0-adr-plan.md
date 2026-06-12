# Slice 0 — ADR Plan Design Memo

> **Status:** Slice 0 design artifact (`[design-adr]`).
> **Purpose:** One-paragraph per ADR stating the key decisions and the primary source artifact.

## ADR 1 — BYO-LLM Extraction Provider Protocol (`fathomdb.extract.v1`)

The key decision is that FathomDB proxies graph construction to a **caller-supplied extraction
harness** via a local, versioned stdio-NDJSON subprocess protocol — `fathomdb.extract.v1`. FathomDB
makes **no network call** and contains no LLM; all model connectivity lives entirely in the
harness. The protocol's five additive pins (ratified 2026-06-12 in the Memex ELPS consult) are:
`options.instructions` (Q1) for optional steering, `source_span` as UTF-8 byte half-open
`[start,end)` offsets (Q4), replay-determinism (cache-scoped, not cold-generation-deterministic,
Q3), a typed `warnings.kind` v1 enum (`supersedes|doc_dropped|no_facts|validation_failed`, D5),
and `ELPS_TIMEOUT_S` as a per-document timeout (Q7). The primary source artifacts are the v1 brief
(`dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md`) and the Memex decision record
(`~/projects/memex/dev/elps/FATHOMDB-CONSULT.md`). The ADR is the engine-side contract Slice 15
implements.

## ADR 2 — IR-measure/Eval Design: R0 + R2

The key decisions are: (R0) measure `found@K` for K ∈ {50, 100, 200, 500, 1000} per query class
on the frozen corpus (`corpus_hash fe973fcd…`, 10,506 docs), plus CPU cross-encoder latency, to
produce a committed artifact (`dev/plans/runs/IR-C-recall-cdf.json`) that fixes the rerank-depth
knob before Slice 10 (R1) begins — the C1 correction from the IR-C roadmap shows the depth-50
ceiling (~0.53) is depth-conditional and may reach ~0.86 at depth 1000; and (R2) build a
LongMemEval-style end-to-end eval with an **identical answerer** across FathomDB, naive-RAG, and
local Mem0-OSS, scored per-class (factoid / temporal / multi-hop / knowledge-update /
multi-session) with abstention. The pivotal Decision ① (from `dev/plans/runs/IR-C-roadmap.md`):
**AC-077 (Evidence Recall@K) is the product gate; R2 is report-only north-star** for the
Mem0/Zep parity goal. R2 is the only instrument that can see R3's value (C3), so R3 go/no-go
is gated on R2 data. The primary source artifacts are `dev/design/ir-recall-measure.md` (Phase-1
consensus-signed measure definition) and `dev/plans/runs/IR-C-roadmap.md` (C1/C2 corrections +
R0/R2 roadmap items).

## ADR 3 — Graph Substrate G11 Migration

The key decision is to **activate the H3 reservation** from `ADR-0.8.0-graph-model-and-edge-addressing.md`
(HITL-signed 2026-06-05) by implementing step-14 of the schema migration: four additive nullable
columns on `canonical_edges` — `body TEXT`, `t_valid TEXT`, `t_invalid TEXT`, `confidence REAL` —
bumping `SCHEMA_VERSION` 13 → 14. These are the fact-on-edge enrichment columns for the
Graphiti-shaped memory ontology. Edge `body` text is projected into both `search_index` (FTS5)
and `vector_default` (1-bit embedding) with `source_type='edge_fact'` as the partition discriminant,
making fact-edges semantically searchable (R8 from the graph-model ADR). The invalidate-not-accumulate
contract requires the engine to tombstone prior edges on `(from_id, to_id, kind)` overlap when
a new BYO-LLM ingest supersedes them. The primary source artifacts are
`ADR-0.8.0-graph-model-and-edge-addressing.md` (H3 reservation), `dev/plans/0.8.1-implementation.md`
(Slice 15 keystone contract), and the v1 protocol brief (the column names in the extract response
match the schema columns exactly).
