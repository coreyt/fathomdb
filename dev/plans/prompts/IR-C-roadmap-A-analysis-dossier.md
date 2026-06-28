# Agent Prompt A — IR-C Analysis Dossier (input for the Fable-5 roadmap review)

**Agent type:** general-purpose (needs Read, Bash/Grep/Glob, WebSearch, WebFetch).
**Model:** **Opus or Sonnet** (orchestration rule — NOT Fable; Fable is reserved for the
Step-B roadmap review).
**Upstream input:** the deep-research report `dev/plans/runs/IR-C-roadmap-deep-research.md`
(produced by a separate deep-research workflow on Opus/Sonnet — see §5).
**Output:** write `dev/plans/runs/IR-C-roadmap-analysis-dossier.md` and return a ≤300-word summary.

## Objective

Produce a single, self-contained **analysis dossier** that gives a downstream reviewer
everything needed to design FathomDB's retrieval roadmap — grounded in the repo's real
code and measured data, plus fresh external research. **Every quantitative claim MUST
cite a source file (path:line or path:§) or an external URL. Do not invent numbers; if a
value isn't in a source, say so.** This dossier is consumed by a Fable-5 reviewer (Prompt
B) that will build a roadmap with success probabilities — so completeness and traceability
matter more than prose.

## Grounding sources (READ these first — they are the source of truth)

Measured results & analysis:

- `dev/plans/runs/IR-C-retrieval-findings.md` — full-corpus lexical+dense measures, the
  Nomic A/B, the "exploratory is structural" conclusion.
- `dev/notes/IR-C-bge-small-literature-benchmark.md` — BEIR empirical anchor, literature
  positioning, reranker/graph/long-context lever framing, peer (Mem0/Zep) numbers.
- `dev/notes/IR-C-embedder-options-research.md` — embedder constraint gate + candidates.
- `dev/plans/runs/IR-C-api-surface-knobs-to-review.md` — the production knob inventory.
- `dev/plans/runs/ir-c-full-run.7d3011d.log` — the printed **FX_ROW** table (per-config
  exact_fact|exploratory R@5/10/20/50 + abstain). The **same rows** are the `configs` object
  in `IR-C-ws1-fusion-experiment-full.json` (keys `h_whole_1:3` = shipped default,
  `h_128/96_1:3`, `text_only_ORc`, `v_whole_max`, `v_128/96_max`); use the JSON for exact
  values. NB: the string "FX_ROW" appears only in the log, not the JSON.
- `IR-C-bge-small-beir-anchor.json` (BEIR anchor: SciFact/NFCorpus/ArguAna, published vs
  meanpool-noprefix), `IR-C-pooling-floor-gate.json` (CLS-vs-mean **1-bit binary floor only**
  — 0.944/0.946, NOT the exploratory rank). All under `dev/plans/runs/`.
- Corpus provenance (state these in the dossier): 10,506-doc frozen corpus, 4,472 positive
  queries, `qrels_version = ir-c-reused-v2`, `corpus_hash fe973fcd…`
  (`IR-C-retrieval-findings.md` §provenance).

Code (extract exact knobs/values — do not trust the docs blindly, confirm against code):

- `src/rust/crates/fathomdb-engine/src/lib.rs` — RRF fusion: `RRF_K` (≈ line 3604),
  `fuse_rrf` (3633) / `rerank_fused` (3707, currently an **identity stub** — the rerank
  seam), `RRF_WEIGHT_TEXT`/`RRF_WEIGHT_VECTOR` (=3:1, lib.rs:3611-3612), the vector stage
  (1-bit sign-quant bit-KNN `TOP_K_BIT_CANDIDATES`=192 @ lib.rs:3411 + f32 rerank),
  `set_vector_stage_only_for_test` seam (lib.rs:2917), whole-doc embedding path.
  (NB: `RrfHybrid` is a **test-harness** enum in `tests/support/ir_eval.rs`, not a lib.rs symbol.)
- `src/rust/crates/fathomdb-query/src/lib.rs` — `compile_text_query` (content-OR compile)
  - `bm25()` ordering.
- The fusion harness `src/rust/crates/fathomdb-engine/tests/ir_c_fusion_experiment.rs` and
  the gold diagnostics `…/tests/ir_c_gold_diagnostics.rs` (the weights/geometries swept).

Graph design (for §3):

- `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` — the binary property-graph
  substrate, corpus(GraphRAG)-vs-memory(Graphiti fact-on-edge) ontologies, opaque-id edge
  addressing, fact-on-edge vs fact-on-node.
- `dev/adr/ADR-0.8.0-graph-traversal-scope.md` — traversal scope (0.8.1 direction).

## Required sections (map 1:1 to the deliverable)

1. **Experiments completed to date.** Chronological table: experiment → hypothesis →
   result (with the key numbers) → pointer to the data file/section. Cover: WS1 fusion
   (AND→OR, weighting, k), the negative-abstain text variant (`text_only_ORc`, abstain_rate
   ≈0.008), vector-arm prefix probe, passage chunking (the FX_ROW configs), pooling A/B
   (CLS vs mean), the **Nomic A/B**, and the BEIR anchor. State each one's verdict in one line.

2. **Current code architecture, knobs, knob settings, IR numbers.** (a) The retrieval
   pipeline end-to-end (text arm: content-OR compile + bm25; vector arm: whole-doc embed →
   1-bit bit-KNN K=192 + f32 rerank; fusion: unconditional weighted RRF). (b) A knob table:
   name, where set (file:line), current value, what it controls — at minimum `RRF_K` (=30),
   text:vector weight (=3:1), bit-KNN K (=192), whole-doc vs chunk, embedder (bge-small,
   mean-pool, 384-d, 1-bit). (c) The headline IR numbers grounded in the result files:
   exact_fact ~0.90 fused R@10 (lexical-bound), exploratory ~0.33 R@10 / ~0.53 R@50, dense
   median rank 99, oracle-union ~0.62, ~38% hard. Confirm each against its source file.
   **Distinguish the shipped default `h_whole_1:3` (whole-doc, 3:1, k=30 → exploratory R@10
   = 0.307) from the ~0.33 ceiling** (the text-only/chunked arm); do not conflate them.

3. **Graph-related discussion — node / edge / both.** Ground in the two graph ADRs, and
   **read the ADR's "conflation to avoid" callout first**: GraphRAG's node value is
   entity/**community** nodes (NOT "fact-on-node"), GraphRAG relationships are also edges,
   Graphiti is fact-on-**edge**, and the true fact-on-**node** tradition is **HyperGraphRAG**
   (reified fact-nodes, arXiv 2503.21322). Lay out: (a) what FathomDB's substrate already is
   (binary property graph, opaque-id edges, logical_id identity); (b) the three options mapped
   to the ADR's real taxonomy — (1) **binary edges** [status quo], (2) **temporal fact-EDGES**
   [Graphiti], (3) **reified fact-NODES** [HyperGraphRAG] — with the retrieval implication of
   each for exploratory/deep-exploratory (multi-hop, temporal). (c) which option the signed
   ADR already leans to and what is still open. Be concrete about how each would generate
   retrieval *candidates* (the lever that can raise the ~0.62 ceiling).

4. **Check the work.** Independently re-derive/verify the §1–§2 numbers from the JSON/MD
   sources (re-read the FX_ROW table, the diagnostics summaries). Produce a short
   "verification log": each load-bearing number → source → ✓/✗/discrepancy. Flag any number
   that the docs assert but you cannot find in a result file. This section gates the dossier's
   trustworthiness — do not skip it. **Labeling note:** the FX_ROW R@K rows and the BEIR/floor
   numbers are JSON-backed and independently re-derivable; the **Nomic A/B (99→135) and pooling
   exploratory (99→121) deltas are MEASURED but recorded as prose** in the findings/embedder
   notes (no raw result JSON) — verify against the prose, label them `MEASURED-prose`, and do
   NOT mark them ✗ for lacking a JSON.

5. **External research — incorporate the deep-research report.** A separate
   **deep-research workflow (run on Opus/Sonnet)** produces
   `dev/plans/runs/IR-C-roadmap-deep-research.md` covering the two buckets below. **If that
   file is absent, STOP and report it — the deep-research report is a hard prerequisite; do
   not fabricate its contents.** Your job
   here is to **integrate and filter** it, not to re-do it: extract its candidate
   architectures, filter each to FathomDB's constraints (local-first, CPU, no API,
   **1-bit-binary-safe**, small footprint), and tabulate per candidate: expected benefit,
   footprint/CPU fit, binary-quant compatibility, integration cost into the existing
   hybrid+graph stack, and the report's confidence/citation. The two buckets the report
   covers: (a) **Reranking / retrieval architecture** — small/CPU cross-encoder rerankers
   (e.g. bge-reranker-base/-v2-m3, ms-marco MiniLM), late-interaction (ColBERT — flag 1-bit
   incompatibility), listwise LLM rerankers (flag cost); (b) **Graph retrieval** — GraphRAG,
   Graphiti, HippoRAG/HippoRAG-2, LightRAG — candidate-generation/multi-hop mechanism +
   reported LoCoMo/LongMemEval gains. **Verify the report's load-bearing claims against its
   own citations; flag any weak/unsupported ones.** If it is missing or thin on an angle the
   roadmap will need, do *targeted* supplementary WebSearch/WebFetch to fill the gap and cite
   it — but the deep-research report is the primary external source.

6. **FathomDB's goal.** State explicitly: the target is **retrieval/answer quality
   as-good-or-better than Mem0 and Zep**, achieved within the local-first / on-device /
   CPU / no-API / binary-compact footprint. Carry the **metric-comparability caveat** (peers
   report end-to-end LoCoMo/LongMemEval answer accuracy; FathomDB measures first-stage
   Recall@K — not directly comparable) and note what FathomDB would need to measure to make
   a fair comparison (an end-to-end QA eval over its corpus).

## Constraints & quality bar

- Cite or flag every number. Distinguish MEASURED (our files) from CLAIMED (external) from
  INFERRED. No silent assumptions.
- Respect the footprint: any option that needs an API, a GPU, or breaks 1-bit quantization
  must be labeled as footprint-violating.
- Keep it a *dossier*, not a recommendation — Prompt B decides the roadmap. But DO surface
  open questions and the candidate-recall ceiling math (reranker bounded by ~0.53–0.62).
- Length: thorough but skimmable (tables over prose). Return a short summary + the dossier path.
