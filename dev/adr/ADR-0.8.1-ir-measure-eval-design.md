# ADR-0.8.1 — IR-measure/Eval Design: R0 candidate-recall CDF + R2 end-to-end parity eval

> **Status:** ACCEPTED — HITL-SIGNED 2026-06-13.
> **Decision:** R0 measurement spec binds Slice 5; R2 eval design binds Slice 25.
> **Decision ①:** AC-077 (Evidence Recall@K) is the product gate; R2 is report-only north-star.
> **C3 gate:** R3 (Slice 30) go/no-go MUST NOT start before R2 data exists.
> **Builds on:** `dev/design/ir-recall-measure.md` (Phase-1 measure definition, consensus-signed).

---

## 1. Context

FathomDB's retrieval baseline is `h_whole_1:3`: exact_fact R@10 = **0.905**, exploratory
R@10/R@50 = **0.307/0.520** (MEASURED on the frozen corpus, `IR-C-ws1-fusion-experiment-full.json`).
The IR-C roadmap (`dev/plans/runs/IR-C-roadmap.md`) identified two framing corrections that change
the roadmap math:

- **C1**: The "~0.53–0.62 candidate-recall ceiling" is **depth-conditional** — the depth-50
  fused ceiling (~0.52–0.53) rises toward **~0.86 at depth 1000** (lexical arm alone,
  MEASURED: `IR-C-retrieval-findings.md:88`). The reranker's true ceiling depends on how deep
  the candidate pool is; the rerank depth knob **must be set from measured CDF data**, not from
  the depth-50 number.
- **C2**: The "~38% irreducible hard core" is a depth-50 artifact. At depth 1000 the lexically-
  unreachable stratum is ~14%. The truly reorder-proof core is ~14%, not 38%.
- **C3**: Graph's payoff is mostly **off the Recall@K axis** (temporal/multi-hop/update query
  classes). The only instrument that can see R3's value is an end-to-end memory eval (R2). This
  makes R2 a **prerequisite** for R3's go/no-go, not a parallel nicety.

**Decision ①** (HITL, 2026-06-12): AC-077 (Evidence Recall@K) is the **product gate**; R2's
end-to-end Mem0/Zep eval is a **report-only north-star** for the "as-good-or-better than
Mem0/Zep" goal. R2 does NOT mint a new gated AC; that decision is HITL after R2 data exists.

The measure definition (Evidence Recall@K, per-class structure, K-ladder, gold-set encoding
protocol, retrieval-mode matrix, pooled-qrels methodology, and the pinning principle) is
**consensus-signed** in `dev/design/ir-recall-measure.md` and is **not re-derived here**. This
ADR specifies the *implementation specs*: what Slice 5 measures and how the artifact is structured,
and what Slice 25 builds and what constraints it must honor.

---

## 2. R0 spec (Slice 5 target)

### 2.1 What to measure

Measure `found@K` — the fraction of evaluation queries for which the gold document appears in the
top-K candidates — for:

- **K values**: K ∈ {50, 100, 200, 500, 1000}
- **Query classes**: `factoid` and `exploratory` (the two primary classes; others reported if
  the gold set has them)
- **Retrieval arms** (all four, reported separately):
  - BM25 text arm alone (write-cursor-ordered FTS production path)
  - Dense arm alone (1-bit KNN + f32 rerank, whole-doc)
  - Oracle union (best-of text+dense: a query counts as found@K if the gold document appears in
    the union of text top-K and dense top-K)
  - Fused production arm (RRF-hybrid, the unconditional ranking FathomDB ships)

Additionally measure:

- **CPU cross-encoder latency (ms/pair)** at two model sizes: TinyBERT-L-2 (~4 MB) and
  MiniLM-L6 (~22.7 MB), on this corpus's passage distribution (random sample of ≥ 1,000 pairs)

### 2.2 Corpus and gold set

- **Corpus**: frozen `corpus_hash fe973fcd…` (10 sources, 10,506 docs)
- **Gold set**: `data/corpus-data/eval/ir_gold/all.gold.json`
- Both the `corpus_hash` and the gold-set path **MUST be pinned** in the output artifact
- The code can land before corpus reproduction, but the **CDF artifact cannot be pinned until
  the local hash matches `fe973fcd…`** (COR-2 prerequisite)

### 2.3 Output artifact: `dev/plans/runs/IR-C-recall-cdf.json`

The committed artifact schema (all fields required):

```jsonc
{
  "corpus_hash": "fe973fcd...",      // pinned corpus version
  "gold_set_path": "data/corpus-data/eval/ir_gold/all.gold.json",
  "generated_at": "<ISO-8601>",
  "recall_cdf": [                    // one row per (query_class, arm, k)
    {
      "query_class": "factoid | exploratory",
      "arm": "bm25_text | dense | oracle_union | rrf_fused",
      "k": 50,                       // one of {50, 100, 200, 500, 1000}
      "found_at_k": 0.905,           // fraction ∈ [0, 1]
      "n_queries": 1584              // denominator (queries in this class)
    }
  ],
  "latency": {
    "model": "TinyBERT-L-2 | MiniLM-L6",
    "ms_per_pair_p50": 0.0,
    "ms_per_pair_p95": 0.0,
    "n_pairs_sampled": 1000,
    "hardware_note": "<cpu model, no of threads>"
  }
}
```

The artifact carries **all 5 K-values × both query classes × all 4 arms** (= 40 rows minimum)
plus the latency section for both model sizes. Any test can assert the schema and the
`corpus_hash` pin without asserting specific latency numbers (which are hardware-dependent).

### 2.4 Why this gates Slice 10 (R1)

The rerank depth knob (R1) must be set from the **measured CDF** before implementation:

- The C1 correction shows the depth-50 ceiling (~0.52–0.53) is depth-conditional and may reach
  ~0.86 at depth 1000. The magnitude of R1's benefit depends entirely on where the CDF curve
  bends — an INFERRED interpolation is not an acceptable basis for a product-level rerank depth
  choice.
- The CE latency numbers (ms/pair at the chosen rerank depth) must be reconciled against the
  tiered AC-013 latency budget before Slice 10 commits to a model+depth combination.

**R0 gates R1 (Slice 10). Slice 10 must not open until the CDF artifact is committed.**

### 2.5 Falsifiable bar

The committed `IR-C-recall-cdf.json` carries all 5 K-values + both query classes + all 4 arms

- both latency models. A test can assert:

```python
import json
cdf = json.load(open("dev/plans/runs/IR-C-recall-cdf.json"))
assert cdf["corpus_hash"].startswith("fe973fcd")
arms = {row["arm"] for row in cdf["recall_cdf"]}
assert arms == {"bm25_text", "dense", "oracle_union", "rrf_fused"}
ks = {row["k"] for row in cdf["recall_cdf"]}
assert ks == {50, 100, 200, 500, 1000}
```

---

## 3. R2 design (Slice 25 target)

### 3.1 Goal

LongMemEval-style end-to-end memory eval: "does FathomDB (post-R1) retrieve the evidence the
answerer needs, **as-good-or-better than Mem0-OSS and naive-RAG**?"

The instrument answers Decision ①'s north-star: while AC-077 (Evidence Recall@K) is the
**gated product metric**, a retrieval recall number alone cannot prove "as-good-or-better than
Mem0/Zep." The end-to-end eval is the only honest way to make that comparison — because the peer
systems (Mem0, Zep) report end-to-end answer accuracy, not first-stage recall, and the comparison
must be **on a fair basis with an identical answerer**.

### 3.2 Identical-answerer constraint (the load-bearing design rule)

The **same LLM / same prompt / same context-window** is used as the answerer for all three
systems — FathomDB, Mem0-OSS, and naive-RAG. **Only the retrieval+memory layer differs.** This
isolates the retrieval signal from the model signal. Any comparison that uses different answerers
or different prompts for different systems is NOT R2.

This constraint is non-negotiable:

- Vendor-reported parity claims (e.g. Mem0 92.5% self-reported, Zep 63.8% vs Mem0 49.0% in the
  LoCoMo dispute) use different answerers, prompts, and context compositions — they are **not
  comparable** to a Recall@K number and **not acceptable** as a baseline. Only R2 (same answerer)
  can produce a fair comparison.
- Any HITL parity claim after Slice 25 **MUST cite R2 numbers**, never vendor self-reported numbers.

### 3.3 Baselines

- **Mem0-OSS**: the local open-source Mem0 library (`pip install mem0ai`, NOT the Mem0 cloud API;
  the cloud API would violate the no-API footprint requirement for the eval baseline). Use the
  same local corpus.
- **Naive-RAG baseline**: flat retrieval (BM25 or dense, no memory graph/update logic) over the
  same documents.

Both baselines use the **identical answerer** (same LLM, same prompt, same context-window).

### 3.4 Per-class scoring

Score results stratified by query class. At minimum:

| Class | What it tests |
|-------|--------------|
| `factoid` | Specific retrievable fact (number, name, date) |
| `temporal` | Time-aware questions ("what did we know in March") |
| `multi_hop` | Questions requiring 2+ connected facts |
| `knowledge_update` | Questions about facts that have changed / been superseded |
| `multi_session` | Questions spanning multiple conversation sessions |

**Abstention counts against recall.** If the system returns no answer when one exists, it is a
miss. If the system returns an answer when no answer exists (the negative class), it is a false
positive. Both are scored.

### 3.5 Metric

- **Per-class Evidence Recall@K** (from `dev/design/ir-recall-measure.md`) — the primary metric
- **Per-class answerer accuracy** — the end-to-end score (retrieval + answerer joint accuracy)
- **Abstention rate** per class — separately reported

AC-077 (Evidence Recall@K, strict all-of) is the **product gate** and is measured directly.
The per-class answerer accuracy is the **north-star** comparison metric for Mem0/Zep parity.

### 3.6 Footprint

- The **answerer/baseline LLM** is **test-infra** (BYO/local) — it is not a FathomDB product
  dependency; it is gated behind a feature flag or environment variable (like the embedder tests)
- The **engine SUT** (FathomDB) stays **no-API** throughout the eval; the eval harness does not
  enable any network call from FathomDB itself
- **Local Mem0-OSS** (not cloud API) is mandatory; cloud Mem0 is a footprint violation for the
  eval infrastructure

### 3.7 What Slice 25 delivers

- A **runnable R2 eval harness** (Python) implementing the identical-answerer protocol
- A **local Mem0-OSS adapter** (wraps the OSS library with the same interface as FathomDB)
- A **naive-RAG adapter** as the floor baseline
- A **results document** (`dev/plans/runs/IR-C-r2-eval-results.md` or equivalent) with actual
  per-class numbers for all three systems, including the N (queries), the answerer/version, and
  the corpus snapshot hash
- **DOC-INDEX** updated with the harness and results

### 3.8 Falsifiable bar

Slice 25 commits the R2 harness + baseline adapter + a results doc with actual per-class numbers.
Any HITL review can verify:

1. The identical-answerer constraint (same LLM, same prompt, cited in the results doc)
2. The local-Mem0-OSS condition (not the cloud API)
3. Per-class breakdown present for all 5 classes
4. Abstention reporting present
5. Corpus hash matches `fe973fcd…`

---

## 4. Decision ①: AC-077 gate vs R2 north-star

| Dimension | AC-077 (Evidence Recall@K) | R2 (end-to-end parity eval) |
|-----------|---------------------------|-----------------------------|
| Role | **Product gate** (GATED) | **Report-only north-star** |
| Metric | Evidence Recall@K (strict all-of; see `ir-recall-measure.md`) | Per-class answerer accuracy vs Mem0/Zep |
| Gate AC | AC-077 (minted at the eval gate, HITL-gated) | None in this ADR; minted at the eval gate AFTER R2 data exists (HITL) |
| Instrument | First-stage retrieval (does FathomDB surface the evidence?) | End-to-end answer quality (does the answerer answer correctly?) |
| Gating slices | Slice 5 (R0 CDF), Slice 10 (R1 reranker), Slice 40 (GA) | Slice 25 (R2 harness); R3 go/no-go gated on this |

**Decision ①** is load-bearing for the R3 (graph) decision: R3's value is mostly off the
Recall@K axis — graph helps temporal/multi-hop/update classes, not factoid. The only instrument
that can see R3's value is R2's per-class breakdown. **R3's go/no-go MUST wait for R2 data (C3).**

---

## 5. Overall ADR decision

- Status: `ACCEPTED — pending HITL sign-off`
- The R0 CDF spec (§2) is the binding implementation contract for Slice 5; any deviation is a
  forced-deviation to flag in `output.json`
- The R2 eval design (§3) is the binding implementation contract for Slice 25; any deviation is
  a forced-deviation to flag
- The AC-077 / R2 north-star axis per Decision ① is the committed product decision; no new AC is
  minted in this ADR
- **C3 is binding:** Slice 30 (R3 graph arm) MUST NOT begin go/no-go before R2 data exists

---

## 6. References

- Phase-1 measure definition: `dev/design/ir-recall-measure.md` (primary source for Evidence
  Recall@K definition, K-ladder, per-class structure, qrels methodology — not re-derived here)
- IR-C roadmap (C1/C2/C3 corrections, R0/R2 items): `dev/plans/runs/IR-C-roadmap.md`
- 0.8.1 slice contracts: `dev/plans/0.8.1-implementation.md` (Slice 5, Slice 25)
- Frozen corpus: `tests/corpus/corpus-card.md`; acquire scripts under `tests/corpus/scripts/`
- Gold set: `data/corpus-data/eval/ir_gold/all.gold.json`
- LongMemEval: arXiv 2410.10813; github.com/xiaowu0162/longmemeval (ICLR 2025)
- Mem0 OSS: arXiv 2504.19413; `pip install mem0ai`
- eu7/AC-075 fidelity gate (distinct from this measure): `dev/adr/ADR-0.7.0-vector-binary-quant.md`
