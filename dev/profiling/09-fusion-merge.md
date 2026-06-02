# 09 — Branch merge / dedup / fusion

**Component:** the step in `read_search_in_tx` that combines the vector branch and
the FTS5 branch into one result. Today: a **scoreless union** — vector hits
pushed first, then FTS5 hits, deduped by `body` via a `BTreeSet<String>`. This is
the G9 (RRF fusion + rerank) target.

## Why it matters

This is where "hybrid" is currently a misnomer: the two branches are concatenated,
not *fused*. G9 replaces this with Reciprocal Rank Fusion (and a rerank hook), the
single highest-leverage retrieval-quality change. Profiling the current merge
gives G9's overhead a baseline; profiling the *quality* (recall/ordering) gives
G9 its justification.

## Retrieval path — what to measure

- **Current merge cost (baseline).** In-memory union + `BTreeSet<String>` dedup
  over ≤ (vector + text) hits. Cheap today; measure it so G9's fusion sort is
  attributable. Note the dedup key is `body` (string compare), not id/score.
- **Ordering quality (the real point).** Capture, for the query set, how often a
  doc returned by *both* branches is ranked below a single-branch doc — the
  scoreless union cannot promote agreement. This is the recall/precision deficit
  G9 fixes; quantify it now (e.g. nDCG or recall@10 vs an RRF reference computed
  offline).
- **G9 RRF overhead (projection).** RRF = keep each branch's rank positions,
  score `Σ 1/(k+rank)` with k=60, accumulate keyed on body, sort desc, truncate
  to `final_limit`. Profile the added sort + accumulate over the candidate set —
  small (bounded by candidate count), but measure it, plus the text-branch switch
  to `bm25()` ordering (`03-fts5.md`) that RRF needs for a real rank.
- **Soft-fallback ordering.** The vector-empty-but-pending signal is computed
  before branch collapse; ensure profiling captures it pre-merge so G9 doesn't
  accidentally move it.
- **Rerank hook (G9 stage 2 / G12).** MMR (diversity, needs candidate embeddings
  via the `vector_default` JOIN) and recency/importance reweight (needs uniform
  timestamps — text hits have none until G12). Baseline the candidate-embedding
  fetch cost that MMR would need.

## Sharp edges

- **Behavior-compat.** Any fusion change alters result *ordering* — a deliberate,
  documented event with a pinned acceptance test + a `Legacy` `fusion_mode`
  escape hatch (lock-free atomic, mirroring `search_limit_override`). Profile both
  modes so the switch cost is known.
- Dedup-by-body must be preserved through G9 (RRF accumulator keyed on body), or
  duplicate bodies from the two branches reappear.
- Recall is half the measurement: an RRF that reorders must not drop recall@10
  below the 0.90 gate / 0.937 anchor.

## Scaling expectation

Merge/fusion is in-memory over a bounded candidate set (≈ `TOP_K_BIT_CANDIDATES`
+ FTS5 hits), so it is ~constant w.r.t. corpus N and tiny relative to the vec0
phase-1 scan. Its value is almost entirely **quality**, not latency — which is
exactly why the profiler must measure ordering/recall here, not just ms.
