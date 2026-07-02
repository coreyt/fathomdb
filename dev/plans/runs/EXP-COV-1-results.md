# OPP-6 EXP-COV-1 — coverage->outcome SUFFICIENCY sweep (priced; HITL-approved $20)

> **The keystone OPP-6 arm (D-1, D-3).** Authoritative design: Memex
> `dev/fathomdb/OPP-6-experiments.md` (EXP-COV-1, §2 sweep, §4 metric, §7 rule). Prior
> `$0` census (Slice 5, `EXP-COV-results.md`): **entity coverage is solved; the gap is
> edges/relations.** This arm asks the *sufficiency* question the census could not:
> **does closing the edge/relation coverage gap move a DOWNSTREAM retrieval metric above
> the ~0.571 embedder ceiling, holding the retrieval stack FIXED?**
>
> **Corpus: LOCOMO** (`multi_session` / `temporal` — the classes where relations matter).
> **CC-BY-NC, gitignored, EVAL-ONLY — no raw payloads committed; only derived metrics.**
> **Date:** 2026-07-02 · **Author:** 0.8.12 EXP-COV-1 implementer (downstream GPU re-run appended 2026-07-02).

---

## 0. STATUS — priced work COMPLETE; downstream verdict CLOSED (GPU re-run) → CEILING-ABSORBED

- **VERDICT: CEILING-ABSORBED.** On the *full* held-fixed GPU stack (dense edge_facts +
  FTS(docs) + graph-arm ON + CE-rerank depth 50, α=1.0, pool_n=50), a coverage increase
  does **not** move the downstream retrieval metric above the embedder ceiling on any
  powered class. Every paired-bootstrap Δ vs the **same-stack `C-none`** is **negative**
  (coverage *degrades* retrieval — the graph-arm coverage-consumer dilutes the pool), so
  the pre-registered rule (CI-lo > +0.04 on ≥1 powered class) is not met, decisively.
  See §4 (GPU re-run).
- **The priced lever (relation-focused extraction) is fully executed and clean:**
  **272/272 LOCOMO sessions, `$4.79` of the `$20` ceiling, 0 failures.** The GPU re-run
  **reused the preserved cache at `$0` additional spend** (completeness guard 272/272, no
  re-extraction). Resilience was proven under spend (a real mid-run crash + `--resume`
  recovery, killed at 229/272, resumed to 272 from `$4.09`).
- The environmental blocker (§6, CPU embedder) is **RESOLVED**: a GPU engine wheel
  (`embed-cuda,rerank-cuda`, RTX 3090 idx 0; K620 excluded) restored the dense + CE
  stack, embedding a ~700-token body in **13.9 ms** (vs ~0.95 s–13 s CPU) with **CPU↔CUDA
  ~1-bit parity held** (max abs diff 1.1e-7, cosine 1.000000000000). The verdict is on
  the *actual* fixed stack, not a degraded-stack artifact.

---

## 1. Design — coverage is the independent variable, the retrieval stack is held FIXED

Every condition ingests the **same 272 LOCOMO sessions** with the **same embedder** and
is queried with the **same query set** through the **same search config**; the **only**
thing that changes is which entity/edge facts populate the fact-graph — i.e. extraction
**coverage**. This isolates coverage from the embedder ceiling (OPP-6 §2). The graph arm
(`use_graph_arm=True`) is the **consumer** that converts coverage into retrieval (BFS
over fact-edges from the lexical/dense seed hits; an edge's `source_id` resolves back to
its source session). It is held ON for **all** conditions (coverage's best shot).

| Condition | Extractor | Coverage delivered | Footprint |
|---|---|---|---|
| **C-none** | none (docs only) | 0 facts | `$0` (baseline) |
| **C0-floor** | heuristic (caps + intra-sentence co-occurrence) | 8,888 ent / 5,685 edges | `$0` (anchor) |
| **C-relation** | `claude-haiku` + a **relation-maximizing prompt** (`cov1-relation-1`) | **2,966 ent (10.9/doc) / 5,756 edges (21.2/doc)** | **priced lever ($4.79)** |

`C-relation` holds the **model** at the census-baseline family (`claude-haiku-4-5`) and
changes only the **prompt** to a relation-maximizing one, so the lever under test is
*relation coverage*, not a model swap (EXP-M4 is separate). C0-floor and C-none are `$0`
— the cheapest valid comparison; the full C0..C4 matrix was not needed for sufficiency.

**Downstream read (dependent variable):** per intent class — **gold-in-pool@10 /
recall@k** and **MRR** — with a **paired bootstrap** (B=2000, seeded) of the per-query
delta vs `C-none`, and the pre-registered decision rule. (An EM/F1 answerer arm was
**deferred**: priced per query×condition, and the `$0` gold-in-pool metric already
discharges the decision rule.)

## 2. Pre-registered decision rule (OPP-6 §7 — no goalpost moving)

- **SUFFICIENT** (coverage IS the lever; a scoped Slice-10 run is justified) iff a
  coverage increase yields Δ(gold-in-pool) or Δ(MRR) with paired-bootstrap **CI lower
  bound > +0.04** on **≥1 powered class** (power floor = 30 scored questions), precision
  guard intact.
- **CEILING-ABSORBED** (redirect; resolve OPP-6 #6) iff the curve is flat at the ceiling
  (all powered CIs span ≤ the noise floor, i.e. CI-lo ≤ +0.04).

**VERDICT (GPU re-run, 2026-07-02): CEILING-ABSORBED.** No powered class shows
Δ(gold-in-pool@10) or Δ(MRR) with paired-bootstrap CI-lo > +0.04 vs the same-stack
`C-none`; all powered deltas are **negative** (§4). Paired bootstrap B=2000, seed=0xC0F1;
power floor 30 met on both classes (multi_session n=269, temporal n=321).

## 3. Coverage delivered by the priced lever (`C-relation`)

The relation-maximizing prompt yields a **dense** fact-graph: **10.9 entities and 21.2
directed fact-edges per session** (272 docs → 2,966 entities / 5,756 edges), vs the
census ELPS-baseline's much sparser edge set (strict edge recall 0.227). This is a large
coverage increase over both `C-none` (0 facts) and `C0-floor` (co-occurrence noise), so
the lever is genuinely exercised — the sufficiency question is well-posed.

## 4. Downstream verdict (GPU re-run, full held-fixed stack) — CEILING-ABSORBED

Run 2026-07-02 on a GPU engine wheel (`embed-cuda,rerank-cuda`, RTX 3090 idx 0; Quadro
K620 excluded via `CUDA_VISIBLE_DEVICES`). **Held-fixed stack:** CLS-corrected bge-small
dense (edge_facts) + FTS(docs) + graph-arm BFS ON for all conditions + CE-rerank depth 50
(α=1.0, pool_n=50) — the *full* stack the CPU run had to drop. Coverage (the extractor)
is the only variable. Paired bootstrap B=2000, seed=0xC0F1, k-grid {5,10,20}, focus_k=10.

**Same-stack `C-none` baseline (this sweep — the pre-registered comparator):**

| class | n | gold-in-pool@10 | MRR |
|---|--:|--:|--:|
| multi_session | 269 | 0.3978 | 0.6131 |
| temporal | 321 | 0.8723 | 0.6705 |

**`C-relation` (priced relation coverage, 5,036 canonical edges) vs same-stack `C-none`:**

| class | n | gip@10 (C-rel / C-none) | **Δ gip@10 [95% CI]** | MRR (C-rel / C-none) | **Δ MRR [95% CI]** | verdict |
|---|--:|--:|--:|--:|--:|--|
| multi_session | 269 | 0.2751 / 0.3978 | **−0.1227 [−0.1673, −0.0781]** | 0.3857 / 0.6131 | **−0.2274 [−0.2629, −0.1908]** | CEILING-ABSORBED |
| temporal | 321 | 0.8037 / 0.8723 | **−0.0685 [−0.0967, −0.0436]** | 0.4265 / 0.6705 | **−0.2439 [−0.2780, −0.2138]** | CEILING-ABSORBED |

**`C0-floor` anchor (heuristic co-occurrence, 2,468 canonical edges) vs same-stack `C-none`:**

| class | n | Δ gip@10 [95% CI] | Δ MRR [95% CI] | verdict |
|---|--:|--:|--:|--|
| multi_session | 269 | −0.0632 [−0.0967, −0.0297] | −0.1219 [−0.1493, −0.0951] | CEILING-ABSORBED |
| temporal | 321 | −0.0374 [−0.0592, −0.0187] | −0.1124 [−0.1379, −0.0890] | CEILING-ABSORBED |

**Reading.** Every powered Δ is **negative** and every CI sits well below the +0.04 floor:
adding relation/entity coverage *lowers* both pool membership (gold-in-pool@10) and rank
precision (MRR) versus the embedder-only baseline. The graph-arm — the consumer that
turns coverage into retrieval — pulls topically-adjacent-but-wrong sessions into the
pool, diluting precision (the MRR loss of −0.23/−0.24 is the sharpest signal). Coverage
is therefore **not** the lever at this operating point; the binding constraint is the
embedder/retrieval ceiling (consistent with the Mem0-parity finding that precision, not
formation, is the cause, and with the graph-arm-does-not-beat-BM25 pivot). **Verdict:
CEILING-ABSORBED** — redirect to OPP-6 #6 (resolve the ceiling); do **not** fund a
full-corpus relation-extraction pass (~$340+, §8) on a coverage-lift premise.

**Precision guard.** No recall/precision masking: coverage did not trade precision for
pool membership — both moved down together, so the negative verdict is not an artifact of
a recall-inflating pool. MRR (the precision-sensitive rank metric) degrades monotonically
with coverage density (C-none 0.613/0.671 → C0-floor 0.491/0.558 → C-relation 0.386/0.427
for multi_session/temporal), directly evidencing precision loss from the consumer.

> **Historical (superseded) baseline — do NOT use as the comparator.** The prior CPU
> screening measured a *degraded* FTS-only + structural-graph `C-none` (CE + dense OFF):
> multi_session gip@10 = 0.468, temporal 0.913, factoid 0.979. Per the PDS same-stack
> requirement, the verdict above contrasts `C-relation` against the **full-GPU-stack**
> `C-none` recomputed in *this* sweep (multi_session 0.3978), not that degraded number.
> (factoid was not re-run on GPU: already near-ceiling with no relation headroom.)

## 5. Graph-arm latency signal on GPU (feeds OPP-6 D-5 sequencing, not the verdict)

Measured end-to-end per condition over the 590 powered queries (embed + FTS + graph-arm
BFS + CE-rerank@50; GPU embed/CE on an RTX 3090):

| condition | canonical edges | retrieve time (590 q) | per-query |
|---|--:|--:|--:|
| C-none | 0 | 37.2 s | ~63 ms |
| C0-floor | 2,468 | 197.7 s | ~335 ms |
| C-relation | 5,036 | 377.9 s | ~640 ms |

Even with GPU embed + CE, richer coverage carries a **real ~10× query-time cost** — and
the cost lives in the CPU/SQLite **graph-arm BFS** over the denser fact-graph, not the
embedder (GPU embed of a ~700-token body is 13.9 ms; standalone CE-rerank@50 ~a few ms
after warmup). So the coverage consumer is *more* expensive **and** *less* accurate at
this operating point — a strong input to the coverage-first-vs-consumer-first sequencing
decision (OPP-6 D-5).

## 6. Environmental blocker (RESOLVED by the GPU re-run — historical record)

The installed native `_fathomdb.abi3.so` in the shared venv is a **CPU-only candle
build** (no `embed-cuda` feature; `FATHOMDB_EMBED_DEVICE=cuda` falls back to CPU), and it
has a pathological embed/rerank latency profile that makes the *fixed* retrieval+CE stack
intractable at LOCOMO scale:

- a ~700-token doc-body embed takes **~13 s** (measured), so dense-embedding 272 doc
  bodies is ~1 h/condition;
- the embed scheduler **intermittently STALLS** mid edge_fact-queue (spins at ~1500% CPU
  with **zero** write progress; reproduced twice at ~5 k nodes) — even though all fact
  bodies are <300 chars, so it is not a data problem;
- CE-rerank over LOCOMO-sized bodies is **~8 s/query** (rerank_depth=20) → hours/condition.

To stay within a screening budget the stack was progressively degraded (doc-dense off →
CE off → fact-dense off), landing on **FTS + structural graph-arm BFS**. That still
produced the `C-none` baseline (§4) but the `C-relation` retrieval at 590 queries did not
complete in a reasonable window (graph-BFS ~2-3 s/query, §5). Rather than ship a verdict
on a triply-degraded stack, the downstream read is **replanned onto a GPU embedder
build** where dense + CE are restored and tractable (§7 / the replan doc). This is a
justified deviation (environmental), documented per the OOB protocol, escalated to HITL,
and resolved by the replan — not a silent spec change.

## 7. $ ledger (actual spend) + resilience (the priced mandate — COMPLETE)

**Actual priced spend: `$4.79` of the `$20` ceiling** (C-relation extraction only; all
retrieval/scoring is `$0`/local). Per-doc measured cost: **`$0.0179/doc`** (`claude-haiku`,
relation prompt, ACTUAL response-usage tokens × a pinned price map; mean 1,406 in /
3,299 out tokens/doc). Auto-stop never triggered (spend well under ceiling).

All §4 resilience preconditions were built into `eval.exp_cov1_extract` and proven on
`$0`/stub conditions **before** any spend, and then validated by a real crash:
- **atomic checkpoint** — every unit via temp-file + `os.replace` (≤1 unit lost on crash);
- **verified `--resume`** — a real kill at 229/272 resumed cleanly to 272, ledger
  continuing from `$4.09` (0 re-extraction, 0 double-charge);
- **429/5xx exponential backoff + cap**; deterministic JSON-decode failures fail-fast
  (retrying a `temperature=0` call is futile) — surfaced a `max_tokens` truncation in the
  pilot, fixed to 8192;
- **per-doc window-fit** (char budget);
- **completeness guard** — a failed extraction is recorded `status=failed`; the sweep
  **REFUSES** to score `C-relation` until every unit is present-or-explicitly-failed
  (272/272 ok here);
- **running `$` ledger with HARD auto-stop** at `$20` (projects next-call cost from the
  running mean and stops *before* a breach).

The preserved priced cache (gitignored, on-machine, EVAL-ONLY) lets the GPU re-run
`--resume` at **`$0` additional spend**:
`data/corpus-data/eval-cache/exp-cov1/relation.claude-haiku.cov1-relation-1.ndjson`.

## 8. Full-Slice-10 relation-targeted extraction cost estimate

Extrapolated from the measured **`$0.0179/doc`** (relation-focused `claude-haiku`):

| corpus | ~docs | est. relation-extraction cost |
|---|--:|--:|
| LOCOMO (this corpus) | 272 | **$4.87** (actual: $4.79) |
| personal.gold slice | 60 | $1.07 |
| LongMemEval sessions | 19,195 | **$343.61** |
| LOCOMO + LME | 19,467 | $348.48 |

A full personal-agent-scale relation extraction is therefore **~$340+** at cheap-model
rates — a material line item for the OPP-6 coverage-first-vs-consumer-first sequencing
decision, to be weighed against whatever downstream lift the GPU sweep measures.

## 9. Reproduction

```bash
cd src/python
# 1) priced extraction (resumable; auto-stops at the ceiling) — ALREADY DONE ($4.79)
python -m eval.exp_cov1_extract --corpus locomo --model claude-haiku \
    --cache <cache.ndjson> --ledger <ledger.ndjson> --ceiling 20
# 2) the downstream sweep (holds retrieval fixed; per-condition ranks checkpointed)
#    GPU: build the engine with the embed-cuda feature, then FATHOMDB_EMBED_DEVICE=cuda
python -m eval.exp_cov1_sweep --conditions C-none,C0-floor,C-relation \
    --relation-model claude-haiku --relation-cache <cache.ndjson> \
    --use-embedder --classes multi_session,temporal \
    --db-dir <db> --ranks <ranks.json> --out-json <out.json>
```

Inputs (gitignored, on-machine): `data/corpus-data/raw/locomo10.json` (CC-BY-NC) and the
preserved extraction cache under `data/corpus-data/eval-cache/exp-cov1/`. Only derived
metrics are persisted in-repo — never the corpus payload or the extracted fact spans.
