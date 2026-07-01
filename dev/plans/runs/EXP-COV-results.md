# OPP-6 EXP-COV Phase-A — extraction-coverage census (`$0` results)

> **Discharges the parked OPP-6 Phase-A eval** (STATUS-0.8.11.2 board had EXP-COV `PENDING`; folded into
> 0.8.12 Slice 5 by HITL 2026-07-01). **All arms `$0`/local** — no new LLM call (the ELPS-baseline arm is
> scored from *pre-computed* extractions already on disk). Pre-registration:
> `dev/design/0.8.12-coverage-probe-and-value-test.md` §A. Arm design (authoritative): Memex
> `dev/fathomdb/OPP-6-experiments.md` (EXP-COV-0, §4 metric, §7 rule). Harness:
> `src/python/eval/exp_cov_census.py` (deterministic; `python -m eval.exp_cov_census [--gliner]`).
> **No raw licensed payloads committed** — the gold/baseline inputs are gitignored EVAL-ONLY; only these
> derived metrics are persisted.

**Date:** 2026-07-01 · **Author:** 0.8.12 release orchestrator (Slice 5).

---

## 1. What ran (and what is HELD)

| Arm | Extractor | Footprint | Ran? |
|-----|-----------|-----------|------|
| **C0-floor** | heuristic (capitalized-span entities + intra-sentence co-occurrence edges) | CPU, `$0` | ✅ |
| **ELPS-baseline** | current ELPS extractor = pre-computed `anthropic/claude-haiku-4-5` (prompt `elps-prompt-2`), scored `$0` | no new spend | ✅ |
| **C1-gliner** | GLiNER `urchade/gliner_small-v2.1` NER (entity-only) | CPU/GPU local, `$0` | ✅ |
| **EXP-COV-0 ceiling** | per-corpus relevance ceiling | — | ✅ (cited, see §5) |
| **C2 / C3 / C4** | cheap-LLM / frontier-LLM / oracle extraction | network-LLM (priced) | **HELD — needs explicit HITL go** |

**Frozen corpus:** `personal.gold` (memex-elps) — **60 docs, 246 gold entities, 163 gold edges**, 6 doc
kinds (`chat, email, note, text_message, todo, voice_memo`), domain `personal`. This is a curated
oracle-grade fact set that already exists, so it is the OPP-6 §4 gold reference **without a new frontier
oracle run**.

**Coverage metric (OPP-6 §4, pre-registered):** fact-coverage = **recall of gold facts**, reported
separately for entities and edges, each with a **precision guard**. Edges reported two ways: **strict**
(canonical `(from, relation, to)` triple) and **pair** (relation-agnostic endpoint pair) — the gap
between them isolates relation-*label* disagreement from fully-missed facts. Endpoints are resolved
through the gold alias map so surface/alias variants do not falsely penalize.

---

## 2. Headline result (overall, N=60 docs)

| Arm | Entity recall | Entity precision | Edge recall (strict) | Edge recall (pair) | Edge precision |
|-----|:-------------:|:----------------:|:--------------------:|:------------------:|:--------------:|
| C0-floor (heuristic) | 0.658 | 0.531 | **0.000** | 0.264 | 0.000 |
| **ELPS-baseline (haiku)** | **0.854** | 0.933 | **0.227** | 0.577 | 0.237 |
| C1-gliner (entity-only) | 0.854 | 0.942 | 0.000 (n/a) | 0.000 (n/a) | 0.000 (n/a) |

**Doc-level paired bootstrap CI95 (ELPS-baseline, B=2000, seeded):**

- entity recall **0.854** — CI95 **[0.812, 0.897]**
- edge recall strict **0.227** — CI95 **[0.157, 0.306]**
- edge recall pair **0.577** — CI95 **[0.463, 0.684]**

The entity and edge-strict CIs are **fully disjoint** — the coverage gap is real and on the **edge/relation
axis**, not the entity axis.

---

## 3. Per-class breakdown (ELPS-baseline — the per-class half of OPP-6 D-3)

| Doc kind | gold edges | Entity recall | Edge recall (strict) | Edge recall (pair) | Edge precision | Powered? |
|----------|:----------:|:-------------:|:--------------------:|:------------------:|:--------------:|:--------:|
| chat | 22 | 0.95 | 0.55 | 0.86 | 0.43 | yes |
| email | 13 | 0.96 | 0.46 | 0.85 | 0.33 | yes (marginal) |
| note | 32 | 0.71 | **0.22** | **0.28** | 0.21 | yes |
| text_message | 24 | 1.00 | 0.25 | 0.58 | 0.32 | yes |
| todo | 53 | 0.76 | **0.04** | 0.58 | 0.05 | yes |
| voice_memo | 19 | 1.00 | 0.21 | 0.53 | 0.31 | yes |

(Power floor = 10 gold edges; all classes clear it, `email`=13 is marginal.)

C0-floor per class: entity recall 0.51–0.83, edge-strict **0.00** everywhere (a rule-only baseline cannot
label relations); edge-pair 0.02–0.64. C1-gliner per class: entity recall 0.71–1.00 (tracks ELPS
closely), no edges by construction.

---

## 4. Findings (what the census settles)

1. **Entity coverage is NOT the binding gap.** The current extractor recovers **85%** of gold entities at
   **93%** precision. A **cheap local CPU NER (GLiNER)** matches it — **0.854 recall / 0.942 precision** —
   so entity coverage is solved and does **not** need a frontier lever. This directly answers the OPP-6
   D-2 hypothesis that GLiNER ("the one clean real-gold win to date") is the cheap lever: it *is* — but
   only for the axis that is already solved.
2. **Edge / relation coverage IS the gap.** ELPS-baseline strict edge recall is **0.227** (CI95
   [0.157, 0.306]); heuristic and GLiNER produce **0** relational edges. This is where headroom lives.
3. **~⅓–½ of the edge miss is relation-*label* disagreement, not missing facts.** Strict 0.227 vs pair
   0.577 means the extractor often finds the right *endpoints* but emits a different relation label than
   gold. The clearest case is **`todo`** (largest class, 53 edges): **0.04 strict / 0.58 pair** — almost
   purely label mismatch. **`note`** misses both endpoints and labels (0.22 / 0.28) — a genuine
   fact-coverage hole.
4. **Precision guard fires (with a caveat).** Edge precision is low overall (**0.237**; `todo` 0.05).
   Part is real over-extraction, but a large part is **gold-incompleteness** (the gold is a curated,
   sparse subset — not every extractor-emitted true fact is in gold) **plus** the same relation-label
   divergence penalizing precision symmetrically. So low edge precision is a **flag to carry into any
   priced run**, not a verdict that the extractor is mostly noise. A raw "more edges" lever without a
   relation-canonicalization / gold-alignment guard risks the 0.8.3 garbage-edge failure mode.

---

## 5. Per-corpus relevance ceiling (EXP-COV-0)

The `personal.gold` slice is an **extraction-coverage** corpus (fact labels), not a retrieval-QA corpus,
so it carries no query set to measure a retrieval-relevance ceiling directly. The established per-corpus
relevance ceiling of record is the **embedder-bound ceiling ≈ 0.571** (eu8 IR-relevance on the LME
workhorse; memory `fathomdb-recall-fidelity-vs-relevance`, `0.8.3-mem0-parity-closed`), held fixed across
all coverage conditions per OPP-6 §2 (CLS-corrected bge-small; no embedder swap). This matters for the
gate: coverage lift only converts to downstream value **below** this ceiling — the coverage→outcome
sensitivity curve (EXP-COV-1) is the arm that reads it, and that arm is **priced/HELD**. A fresh at-power
per-corpus ceiling re-measure on LOCOMO/AP-News is scoped with the priced sweep, not this `$0` census.

---

## 6. Gate recommendation (pre-registered rule → HARD-STOP #1)

Applying the §A.5 decision rule to the `$0` arms:

- **GATE = OPEN-BUT-NARROWED.** There is a **real coverage gap with headroom** — but it is confined to
  the **edge/relation axis** (entity coverage is solved, incl. by a cheap local model). So:
  - A priced Slice-10 extraction run **is justified in principle** (headroom exists on ≥1 powered class —
    `note`, `todo`, `text_message`, `voice_memo` all miss most gold edges strict).
  - It should be **scoped to relation/edge coverage + relation-label canonicalization**, NOT entity
    volume (spending on entities buys ≈0), and **must carry the precision / gold-alignment guard** (§4.4)
    so a coverage gain is not garbage edges.
- **Crucial unresolved question the `$0` arms cannot answer:** whether closing the edge-coverage gap
  actually **moves a downstream retrieval/answer metric above noise** (OPP-6 D-1), or whether the
  embedder ceiling (§5) absorbs it. That is exactly the **priced coverage→outcome sweep (EXP-COV-1)** —
  HELD. The census establishes *necessary* headroom; it cannot establish *sufficiency*.

**Recommendation to HITL (HARD-STOP #1):** the coverage headroom is genuine but narrow; before
committing frontier extraction spend, decide between (a) a **scoped priced Slice-10 run** targeting
relation coverage with the precision guard + the EXP-COV-1 downstream-outcome read (to test sufficiency),
or (b) **redirect** per OPP-6 §7 (entity coverage solved; edge-relation lift may be absorbed by the
embedder ceiling — spend the budget on recall/embedder instead). This census does not authorize spend on
its own (build ≠ adopt); the priced arm needs the explicit go + the resilience preconditions.

---

## 7. Reproduction

```bash
# $0, deterministic; reads gitignored EVAL-ONLY inputs by path (env-overridable)
cd src/python && python -m eval.exp_cov_census            # C0-floor + ELPS-baseline
cd src/python && python -m eval.exp_cov_census --gliner   # + C1-gliner (downloads the NER model once)
```

Inputs (gitignored, on-machine): `data/corpus-data/external/memex-elps/personal.gold.jsonl`,
`personal.baseline_outputs.jsonl`. Override the dir with `EXP_COV_CORPUS_DIR`.
