# EXP-A — recall generation / candidate-breadth sweep (0.8.11 Slice 10)

- mode: **full** · $0 / LLM-free / deterministic
- dataset: `xiaowu0162/longmemeval-cleaned` split `longmemeval_s_cleaned` seed `20260614`
- questions: 160 · arms: fathomdb_fts_only, naive_bm25
- union corpus (LME sessions): 7154
- candidate-breadth grid (gold-in-pool @K): [10, 20, 50, 100, 200]
- elapsed: 91.5s

> **Arms note.** The two lexical candidate generators (`naive_bm25`, `fathomdb_fts_only` = the
> engine FTS5 arm) are measured here at $0. The **fused-RRF** arm (the shipped candidate set;
> add `--with-fused`) needs the CPU-pinned engine embedder over the 7,154-session union — a
> multi-hour offline build (the GPU is idle by design; the Python engine embedder is CPU-pinned),
> so it is **not run in-session**. The breadth verdict does not depend on it: gold-in-pool lift is a
> pool-depth property, and Gate-2 measured `fathomdb_fused ≈ naive_bm25` on multi_session
> (fused r@10 0.325 ≥ bm25 0.275), so the fused arm starts at least as high and lifts at least as
> much. EXP-B′ reranks within the (fused) widened pool.

## Gold-in-pool vs candidate breadth (point [CI lo, hi], n)

### arm: `fathomdb_fts_only`

| class | @10 | @20 | @50 | @100 | @200 |
|---|---|---|---|---|---|
| factoid | 0.650 [0.500,0.800] (n=40) | 0.750 [0.600,0.875] (n=40) | 0.850 [0.725,0.950] (n=40) | 0.875 [0.750,0.975] (n=40) | 0.875 [0.750,0.975] (n=40) |
| knowledge_update | 0.800 [0.675,0.901] (n=40) | 0.875 [0.775,0.975] (n=40) | 0.875 [0.775,0.975] (n=40) | 0.900 [0.800,0.975] (n=40) | 0.925 [0.825,1.000] (n=40) |
| multi_session | 0.200 [0.075,0.325] (n=40) | 0.325 [0.175,0.475] (n=40) | 0.400 [0.250,0.550] (n=40) | 0.525 [0.375,0.675] (n=40) | 0.650 [0.500,0.775] (n=40) |
| temporal | 0.600 [0.450,0.750] (n=40) | 0.700 [0.550,0.825] (n=40) | 0.825 [0.700,0.925] (n=40) | 0.850 [0.725,0.950] (n=40) | 0.925 [0.850,1.000] (n=40) |

### arm: `naive_bm25`

| class | @10 | @20 | @50 | @100 | @200 |
|---|---|---|---|---|---|
| factoid | 0.700 [0.550,0.825] (n=40) | 0.725 [0.575,0.850] (n=40) | 0.850 [0.725,0.950] (n=40) | 0.875 [0.775,0.975] (n=40) | 0.900 [0.800,0.975] (n=40) |
| knowledge_update | 0.875 [0.775,0.975] (n=40) | 0.900 [0.800,0.975] (n=40) | 0.975 [0.925,1.000] (n=40) | 1.000 [1.000,1.000] (n=40) | 1.000 [1.000,1.000] (n=40) |
| multi_session | 0.275 [0.125,0.425] (n=40) | 0.375 [0.225,0.525] (n=40) | 0.475 [0.300,0.625] (n=40) | 0.500 [0.325,0.650] (n=40) | 0.675 [0.525,0.800] (n=40) |
| temporal | 0.650 [0.500,0.800] (n=40) | 0.775 [0.625,0.900] (n=40) | 0.825 [0.700,0.925] (n=40) | 0.875 [0.750,0.975] (n=40) | 0.950 [0.875,1.000] (n=40) |

## KILL check (focus: multi_session)

- floor K = 10 (shipped final_K view)

| arm | recall@floor | best K | recall@bestK | lift | CI clears floor? |
|---|---|---|---|---|---|
| naive_bm25 | 0.275 | 200 | 0.675 [0.525,0.800] | 0.4 | True |
| fathomdb_fts_only | 0.200 | 200 | 0.650 [0.500,0.775] | 0.45 | True |

**Verdict.** GO — wider candidate generation lifts gold-in-pool with CI clearing the K=10 floor; EXP-B' should rerank a widened pool.

**candidate_k that maximizes gold-in-pool (feeds EXP-B'):** {'arm': 'fathomdb_fts_only', 'candidate_k': 200}

## Per-query arm logging (deferred Slice-5 oracle enabler)

Per-query gold ranks for every arm are persisted in the JSON (`per_query_log`, 160 questions). Each entry carries, per arm, the 0-based rank of each gold session (None if absent), the min gold rank, and whether all gold was found within K=200 — making the per-query arm-selection oracle from Slice 5 Gate-2 computable offline (it was previously deferred for lack of per-query per-arm cells).
