# EXP-Fr-acc base — classifier accuracy + asymmetric mis-route cost (0.8.11 Slice 20)

> Pre-registration: `dev/plans/0.8.11-implementation.md` §1 (EXP-Fr-acc base);
> PSD `dev/design/planner-router-psd-0.8.x.md` §II.A / §II.D / §III.D.
> Script: `src/python/eval/fracc_classifier_run.py` (base mode).
> Output JSON: `dev/plans/runs/fracc-base-output.json`. **Real measured numbers, never fabricated.**

## TL;DR

- **Classifier (deliverable 1, $0):** the internal-fallback intent classifier is **usable** —
  macro accuracy **0.768 [0.732, 0.802]** over the 5 classes, **every class above the 0.20
  chance floor**. **KILL check: NO KILL** (0 classes at chance). `needle` is the weakest
  (0.500 [0.40, 0.60]) — it leaks into the other memory/compositional classes.
- **Mis-route matrix (deliverable 2, priced):** the asymmetry is **directionally confirmed** —
  `needle` is the **only** intent where routing to `C` (map-reduce/QFS) is **negative**; the
  three other measured classes are neutral-to-slightly-positive. The **magnitude scales with
  map-reduce breadth**: needle→C = **−0.080 [−0.28, +0.12]** at 3 distractors (CI crosses 0) →
  **−0.300 [−0.47, −0.10]** at 8 distractors (**CI excludes 0; ≈ the prior −0.362**). The
  router-isolation rule (C forbidden on `needle`) is supported.
- **Models + spend:** classifier = $0 (pure-numpy lexical, no LLM). Mis-route arm =
  **`gemini-flash-lite`** (local vLLM `qwen3.6-27b`/`gemma-4` were down — HTTP 500 — so the cheap
  hosted tier was used). **Total spend ≈ $0.054** (committed runs $0.041 + a discarded
  non-deterministic first attempt $0.013), far under the **$3** ceiling.

## 1. Intent-classifier accuracy ($0, no network)

**What is measured.** The *internal classifier fallback* (PSD §II.A preference #3 — the
agent-passed label is preferred #1, provider-callback #2). The preferred paths are not measured
here; this is the lower-tier fallback whose usability gates the KILL check.

**Method.** Pure-numpy **lexical TF-IDF nearest-centroid (Rocchio)** classifier, **stratified
5-fold cross-validation**, balanced **100/class** (chance = 0.20; `global` caps the balanced N at
100 — AP-News global pool). Ground-truth labels are the Gate-0 corpus map: LME
factoid+knowledge_update + LOCOMO factoid → `needle`; LME/LOCOMO `multi_session`/`temporal` 1:1;
AP-News activity_global+data_global → `global`; MuSiQue answerable → `multi_hop`. Bootstrap CI
(2000×, seed 0). *No torch/sklearn in this env → the embedding-based variant is unavailable; the
lexical classifier is the honest $0 fallback proxy (reported as such, a likely lower bound).*

| Intent | Accuracy (recall) | 95% CI | n |
| --- | ---: | :---: | ---: |
| needle | 0.500 | [0.400, 0.600] | 100 |
| multi_session | 0.630 | [0.540, 0.720] | 100 |
| temporal | 0.770 | [0.690, 0.850] | 100 |
| global | 1.000 | [1.000, 1.000] | 100 |
| multi_hop | 0.940 | [0.890, 0.980] | 100 |
| **macro** | **0.768** | **[0.732, 0.802]** | 500 |

**Confusion (row = true class).** `global` ("Across the dataset…") is perfectly separable;
`multi_hop` nearly so. The confusable cluster is the three memory/compositional classes — `needle`
splits into `multi_session` (20), `multi_hop` (16), `temporal` (14):

```
needle        : needle 50 | multi_session 20 | multi_hop 16 | temporal 14
multi_session : multi_session 63 | needle 24 | temporal 5 | multi_hop 6 | global 2
temporal      : temporal 77 | needle 10 | multi_hop 9 | multi_session 4
global        : global 100
multi_hop     : multi_hop 94 | temporal 3 | multi_session 2 | needle 1
```

**KILL check.** Rule: accuracy at chance (CI lo ≤ 0.20) for ≥2 classes. **Result: NO KILL** —
0 classes at chance (the weakest, `needle`, has CI lo 0.40 ≫ 0.20). The internal classifier is a
usable fallback. (The agent-passed label still ranks #1 by design; this only establishes the
fallback is not at chance.)

## 2. Asymmetric mis-route cost matrix (priced)

**What is measured.** Per (intent, chosen-route) **answer-quality** accuracy, and the delta of the
**C (map-reduce/QFS)** route vs the **correct retrieval route**. **Oracle context** isolates the
*route* effect from retrieval noise: both arms receive identical raw chunks (gold evidence +
distractors); only `C` inserts the lossy per-chunk query-focused-summarize → reduce → answer
bottleneck. The same `gemini-flash-lite` judge grades both arms (paraphrase-tolerant CORRECT/
INCORRECT vs gold). A deterministic containment match corroborates. Paired bootstrap CI (2000×).
Corpora: LOCOMO (needle/multi_session/temporal, short gold answers + same-conv distractor
sessions); MuSiQue (multi_hop, `is_supporting` gold paragraphs + non-supporting distractors).
Base arm: **25/class, 3 distractors**.

| Intent | correct route | acc(retrieval) | acc(C) | **Δ (C − retrieval)** | n |
| --- | --- | ---: | ---: | :---: | ---: |
| **needle** | retrieval | 0.640 [0.44, 0.84] | 0.560 [0.36, 0.76] | **−0.080 [−0.28, +0.12]** | 25 |
| multi_session | retrieval | 0.680 [0.52, 0.84] | 0.720 [0.56, 0.88] | +0.040 [−0.12, +0.24] | 25 |
| temporal | retrieval | 0.160 [0.04, 0.32] | 0.200 [0.08, 0.36] | +0.040 [−0.08, +0.20] | 25 |
| multi_hop | retrieval | 0.160 [0.04, 0.32] | 0.200 [0.04, 0.36] | +0.040 [−0.12, +0.20] | 25 |

**Asymmetry holds.** `needle` is the **only** class with a negative Δ — exactly the predicted
high-cost cross-wire (summarize-away). The other classes are neutral-to-slightly-positive (note
`temporal`/`multi_hop` sit on a low absolute floor — hard for a cheap reader either way, which can
mask a C penalty).

### Load-bearing cell: needle → C (the −0.362 claim)

The base −0.080 has a CI that crosses 0 at n=25 — directionally right, not individually
significant, and far from the prior −0.362. **The penalty scales with how much the map-reduce
distills.** A sensitivity arm (needle only, **n=40, 8 distractors** = a QFS that "reads more"):

| Arm | acc(retrieval) | acc(C) | **Δ (judge)** | Δ (contains) |
| --- | ---: | ---: | :---: | :---: |
| base (3 distractors, n=25) | 0.640 | 0.560 | −0.080 [−0.28, +0.12] | −0.12 |
| **deep (8 distractors, n=40)** | 0.750 | 0.450 | **−0.300 [−0.47, −0.10]** | −0.225 |

At 8 distractors the **CI excludes 0** and the magnitude **−0.300 ≈ the prior −0.362**.
Interpretation: the prior −0.362 was flagged in the 0.8.3 ledger as a *weak-distiller
(gpt-5-nano) artifact*; we **reproduce it with a competent cheap summarizer once the map-reduce
reads enough chunks** — the cost is real and grows with QFS breadth. This supports the
**router-isolation rule**: `map_reduce_qfs` / `community_summary` are forbidden on `needle`
(EXP-B′.5 `forbidden_ops`).

### Deferred / not-measured cells

- **`global` row (both routes).** AP-News sensemaking is **reference-free** — graded by
  `decide_084` LLM-judge **win-rate**, not gold-answer accuracy. `global×C` (the correct route)
  and `global×retrieval` (the mis-route) belong to the `decide_084` axis (EXP-B′ priced judge,
  Slice 15), not this gold-answer harness → **deferred** to that axis.
- **Within-retrieval config cells** (e.g. `needle→multi_session` *stack*). These differ by
  **config** (`alpha`/`pool_n`), not by route — a low-cost same-tier difference owned by the
  **EXP-B′.5 forbidden-composition matrix**, not a route mis-wire → not measured here.

## 3. Models, budget, reproducibility

- **Classifier:** $0 (pure-numpy lexical; no LLM call).
- **Mis-route arm:** `gemini-flash-lite` (priced (0.05, 0.20)/1M). Local vLLM `qwen3.6-27b` /
  `gemma-4` returned HTTP 500 (server down) at run time → cheap hosted tier used; both arms +
  judge use the same model so the delta stays fair.
- **Spend:** base arm $0.0236 (886 calls, 441K in / 7.9K out tok) + needle-deep $0.0173 =
  **$0.0409 committed**; + ~$0.013 on a discarded non-deterministic first attempt + $0.0004
  cheap-validate = **≈ $0.054 total**. Ceiling $3 — **far under**. (`BudgetLedger` `--max-usd 3.0`
  guard armed throughout; per-item checkpoint + idempotent `--resume`.)
- **Cheap-validate** (mandated before spend): 2 needle queries, $0.0004, confirmed prompt/parse/
  grader end-to-end before the full batch.

**Reproduce:**

```bash
set -a; . dev/.env.eval; set +a        # R2_JUDGE_* → airlock proxy; R2_RUN=1
# classifier only ($0):
python -m eval.fracc_classifier_run --skip-misroute
# full base (classifier + mis-route, gemini-flash-lite, ≤$3):
python -m eval.fracc_classifier_run --misroute-n 25 --distractors 3 --max-usd 3.0 \
  --checkpoint dev/plans/runs/fracc-base.checkpoint.json --out dev/plans/runs/fracc-base-output.json
# needle-deep sensitivity arm:
python -m eval.fracc_classifier_run --classes needle --misroute-n 40 --distractors 8
```

## 4. Verdict

- **Classifier usable (NO KILL).** Internal fallback macro 0.768; all classes above chance.
  The prototype keeps the preference order (agent label #1 → provider-callback #2 → this
  classifier #3); the classifier is a legitimate low-confidence fallback, not at chance.
- **Mis-route asymmetry confirmed; needle→C is the high-cost cross-wire.** Negative only for
  `needle`, and −0.300 [−0.47, −0.10] ≈ the prior −0.362 once the map-reduce reads enough chunks.
  Encode `map_reduce_qfs`/`community_summary` as `forbidden_ops` on `needle`
  (and the other non-`global` retrieval paths) in the EXP-B′.5 forbidden-composition matrix.
- **Feeds Slice 25 (EXP-Fr-acc/VoI):** this matrix is the asymmetric mis-route cost the VoI
  ask-or-not policy weights — be far more willing to pay an agent round-trip when the candidate
  route risks needle→C than a cheap same-tier miss.
