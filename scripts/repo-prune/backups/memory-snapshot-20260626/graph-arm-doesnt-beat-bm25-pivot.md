---
name: graph-arm-doesnt-beat-bm25-pivot
description: "Measured + literature-confirmed — FathomDB's BFS graph arm adds ~0 recall over BM25 on LongMemEval; co-mingling entities hurts (length-norm bias). PIVOT to index-key enrichment. Tag 0.8.1-beat-bm25-pivot-2."
metadata: 
  node_type: memory
  type: project
  originSessionId: 54df08ae-c01b-4eae-8862-514eb2cfd198
---

**SUPERSEDED/CONFIRMED by [[m1-graph-arm-nogo-registered-n300]] (2026-06-17):** the "seed from lexical
top-K would rescue it" hypothesis below was TESTED at registered n=300 — lexically-seeded PPR still loses
to fused-RRF (ΔF1 −0.0405, CI upper bound below materiality). Graph question is CLOSED → NO-GO.

The 0.8.1 "make the graph arm beat BM25" effort reached a clear, robust NEGATIVE (2026-06-16).
Built `eval/graph_arm_recall.py`: extract a real entity/edge graph with **Qwen3.6-27B via the
Airlock vLLM batch gateway** ($0 local; `enable_thinking:false`; conc=8 is the compute-bound knee;
mt=3072), write docs+entities+edges, run LLM-free recall@K vs BM25/FTS.

**40q (10/class, 1907 sessions, 28.9k entities, 38k edges) R@10:** bm25 0.70 · fts_only 0.80 ·
graph_OFF 0.65 · graph_ON 0.65. → **The BFS arm adds 0 NET and DEGRADES its target class**:
ON−OFF pooled=0 but per-class +.10 factoid/+.10 temporal/**−.20 multi_session (0.30→0.10)** —
gains cancel a real loss on the class it was built for (same-engine A/B, not a pollution
artifact). **Entity co-mingling HURTS** (fts 0.80→graph 0.65): 28.9k short
entity rows distort BM25 corpus stats = **document-length-normalization bias / corpus-heterogeneity**
(Verboseness Fission), NOT "pollution". Needs a SEPARATE entity index, not just a result filter.

**Why (literature, sister-agent synthesis, high-confidence):** raw BFS ~0 over strong lexical is
NORMAL (SPRIG: plain RRF *beats* the graph arm; only LLM-reasoning-guided traversal clears BM25).
Graph pays off ONLY when **seeded from the lexical top-K** — and **C1 seeds from the graph's own
edge-fact/entity FTS, NOT the lexical hits** ⇒ likely THE zero-lift cause. BM25 strong OOD (BEIR).
LongMemEval wins = query routing + **index-key enrichment (+9.4%: extracted facts as doc keys)**.

**Why:** corroborates [[fathomdb-recall-fidelity-vs-relevance]] (graph doesn't recover what lexical
misses unless seeded right).
**How to apply / PIVOT:** DROP source-A + the engine `edge_fact` fix + n=160-as-designed (~0 value).
Pivot to **index-key enrichment** (append a session's entities/facts to ITS OWN doc's FTS content —
no graph arm, no length-norm pollution; reuses the 1906 cached graphs in `/tmp/gar_dry/extractions.json`).
Engine FOLLOW-UPS (real bugs, not blockers): edge-with-body auto-enqueues `edge_fact` vector
projection → SchedulerError (no embedder) / StorageError (~8-14k edges, with one). Committed +
pushed + tagged `0.8.1-beat-bm25-pivot-2`. Report: `dev/plans/runs/0.8.1-beat-bm25-report.md`.
Updates [[c1-graph-arm-seeding-live]] (C1 works mechanically but seeds from the wrong place).

**FOLLOW-UP LEVERS TESTED (2026-06-16, 40q, all $0, MDE ~15pp → within-noise):** R6 **index-key
enrichment** (append a session's extracted facts to its OWN doc FTS, one row/doc — `eval/r6_index_key_enrichment.py`,
7 ACs, codex PASS): pooled R@10 fts_enriched 0.775 vs fts_only **0.80** (−0.025) — but a length-matched
PLACEBO (−0.10) proved a real **FTS length-norm penalty** + that enrichment's CONTENT helps (+0.075 over
placebo; +0.05 on BM25). **BM25 `b`-sweep** (`--tune-b`): lower b lifts recall (plain 0.70→0.75; enriched
0.75→0.775) — length-norm CONFIRMED — but FTS5's b is FIXED, so enriched+low-b on FTS5's better tokenizer
is untested. **Net: graph arm, enrichment, b-tuning — none beats the strong lexical baseline (FathomDB-FTS
0.80) @N=40.** Matches BEIR (BM25 strong OOD). **Only remaining lever with plausible upside = an engine-side
tunable-/lower-`b` FTS5 ranking + enrichment** (custom FTS5 ranking; uncertain). Data:
`0.8.1-R6-recall-n40.json`, `0.8.1-R6-bsweep-n40.json`. Commits a9be9e8→143ebd6 (pushed).
