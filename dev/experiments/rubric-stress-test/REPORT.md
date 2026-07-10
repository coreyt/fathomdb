# Rubric-stress-test — final report (sealed validation run)

**Core hypothesis:** *Can we deterministically + locally collect "agentic failure"
examples from Claude Code transcripts, at ~0 LLM cost?*

**Verdict: CONFIRMED, family-dependent.** A pure-regex/structural detector suite
(no LLM at detection time) pulled **805 candidate failure-moments** out of a
held-out 1,076-file / 64,365-record split in **10.1 s on one core**. Precision is
high and generalizes for two families (self-correction, premise-falsification),
usable-with-a-filter for the dominant family (review-catch), and **overfit** for
two low-volume families (hitl-bounce, unwitnessed-premise). Net: the mechanism
works; three families need a hardening pass before they can auto-feed a rubric.

The validation split was **sealed** through Hypothesize/Implement/Review — the
detectors were designed on the experiment split only. Splits are session-coherent
and **disjoint** (0 file overlap; a session's parent transcript + all subagents
share a fold, so no held-out file is a near-duplicate sibling of a training file).

---

## 1. Yield on the sealed validation split

`run_detectors.py split-validation.txt -> out/validation_candidates.jsonl`

| | files | records | candidates | runtime | LLM tokens |
|---|---|---|---|---|---|
| validation | 1,076 | 64,365 | **805** | 10.1 s | **0** |

**Per family (yield):**

| family | candidates | % of vol | distinct sessions |
|---|---|---|---|
| C-review-catch | 756 | 94 % | 25 |
| D-self-correction | 20 | 2 % | 11 |
| B-unwitnessed-premise | 18 | 2 % | 10 |
| A-hitl-bounce | 6 | 1 % | 4 |
| E-halt-scout-falsify | 5 | 1 % | 2 |

**Per detector:** review-catch 756, self-correction 20, unwitnessed-premise 18,
halt-scout-falsify 5, hitl-soft-redirect 4, hitl-reversal-rework 2.
**Per confidence:** med 760, high 23, low 22.

The shape matches the experiment split: review-catch dominates volume (~94 %),
everything else is a long thin tail.

---

## 2. Precision on validation vs experiment (generalization gap)

Hand-labeled the **same disciplined way** used to seed the experiment set:
distinct-session stride sample (up to 10/detector; A and E fully enumerated since
they have <=6 candidates), judged from <=300-char snippets + +/-3 normalized-line
windows only — **no transcript read whole**. Labels are durable in
`validation_labels.tsv`.

| family | **validation** precision | Wilson 95 % | experiment seed | delta |
|---|---|---|---|---|
| D-self-correction | **9/10 = 0.90** | [0.60, 0.98] | 7/7 = 1.00 | holds |
| E-halt-scout-falsify | **5/5 = 1.00** | [0.57, 1.00] | 6/7 = 0.86 | holds |
| C-review-catch | **6/10 = 0.60** | [0.31, 0.83] | 3/3 = 1.00 * | drop |
| A-hitl-bounce | **2/6 = 0.33** | [0.10, 0.70] | 5/6 = 0.83 | overfit |
| B-unwitnessed-premise | **3/10 = 0.30** | [0.11, 0.60] | 3/4 = 0.75 * | overfit |
| **overall macro (labeled)** | **25/41 = 0.61** | [0.46, 0.74] | 24/27 = 0.89 | — |

By confidence tier: high 9/11 = 0.82, med 15/29 = 0.52, low 1/1. The `high` tier
is meaningfully cleaner than `med`, so the confidence heuristic carries signal —
but Ns are small and Wilson intervals overlap, so treat it as directional.

**Honest statistical caveat:** per-family Ns are 5-10; Wilson intervals are wide
and mostly overlap between experiment and validation. The point estimates are
noisy. What is *not* noisy — and is the load-bearing finding — is that validation
surfaced **concrete, nameable FP mechanisms that the experiment seed never hit**
(enumerated below). That qualitative generalization signal is stronger evidence
than the point estimates alone.

### What generalized
- **D-self-correction (0.90):** `I was wrong` / `you're right` / `good catch` /
  `I need to correct what I told you earlier`. One FP = "you're right, they're in
  separate worktrees" (affirms a fact, not a self-error admission). Robust.
- **E-halt-scout-falsify (1.00):** every hit was a genuine scout/reviewer proving
  a load-bearing premise false (`the re-scope premise for Batch B is false`, `the
  0.8.19 claim is just wrong/stale`, `the commit's byte-for-byte claim is
  technically false`). Highest-value RCA "save-mechanism" class and the cleanest
  family. Low volume, but every candidate is worth keeping.

### What overfit / degraded (with the exact new FP mechanism)
- **B-unwitnessed-premise (0.75 -> 0.30):** three distinct new FP modes absent from
  the experiment seed —
  1. **"dead code" as a benchmark/hygiene *category*** (`Dead Code 0.84/1.00` F1
     scores; `dead code (vulture)` tool row) — 3 of 10, not a deadness premise;
  2. **cited review findings** (`"golden.py", "line": 101 ... is redundant`) whose
     `path", "line": N` quoting form slips the `path:line` witness regex — 2 of 10;
  3. **token collisions** (`no reference embedder` matched `no_callers`;
     `NO consumer-facing API` is API-surface design prose) — 2 of 10.
- **A-hitl-bounce (0.83 -> 0.33):** tiny N (6 = effectively 2 duplicated FP
  patterns): a **synthetic "Monitor poll tick" role banner** where `revert` sits in
  "Do NOT edit/apply/**revert**" (is_synthetic doesn't cover this banner), and a
  **forward instruction** "Write a durable file you can **re-read**" (bare `reread`
  is not a redirect of a proposed action). Both are fixable leaks.
- **C-review-catch (1.00 N=3 -> 0.60):** the experiment N=3 was flagged
  unrepresentative in `formalization.md`; validation **confirms the mechanism-level
  prediction** ("~50 % FP, narration-grade"). The 4 FPs: `F1` (metric score) and
  `F5` (feature/ADR label) matched by the `F-?\d` finding-id regex, and two
  `1xP1+4xP2 -> PASS` provenance summaries. The 6 TPs are real CONCERN/BLOCK/P1
  catches. So C is genuinely ~0.60 — usable but needs a secondary filter.

---

## 3. Known-positive RCA episodes — where they live

Both ground-truth RCA sessions are **entirely in the experiment split, zero files
in validation** (pinned there by the session-coherent resplit):

- **30-N plan-delta** `f57b5dee...` — 15 files, all experiment.
- **CR-047 finish-vs-delete** `2fa060bc...` — 72 files, all experiment.

So the smoke-test **cannot** be re-confirmed on validation — the positives aren't
there. On the experiment split (per `formalization.md`, re-derived from
`out/experiment_candidates.jsonl`, not memory): **CR-047 fires B x2** on the live
premises (`No live consumers found -> DELETE` L1617; `~98 call sites` L1495) **and E**
(the reversal save-mechanism); **30-N** `f57b5dee` is the rubric-*design* session
whose `no consumers`/`~N sites`/`is a duplicate` mentions are self-referential meta
describing the detector, correctly suppressed by the N2/F7-B guards (all 3 B hits
gone). Validation instead provides **fresh, never-seen** positives of the same
shapes — e.g. E's `the re-scope premise for Batch B is false` is a structurally
identical premise-falsification to the CR-047 mechanism, caught in a session the
detector never trained on.

---

## 4. Total examples collected + strongest snippets

- **Validation candidates:** 805. Estimated genuine failure examples (family
  precision x volume): **~484 of 805** — dominated by C (756 x 0.60 ~ 454) because
  C is 94 % of volume even at 0.60 precision.
- **Whole corpus so far:** experiment 1,269 + validation 805 = **2,074 candidates**
  collected at ~0 LLM cost; order-of-magnitude ~1,200 estimated true failure
  examples across both splits.
- **Candidate-weighted population precision (validation): 0.60**, set almost
  entirely by C. This is the *honest* population figure and it **refutes** the
  experiment-era candidate-weighted ~0.99, which was an artifact of an N=3
  review-catch cell scoring 1.00 (flagged indefensible in `formalization.md`).

**Strongest examples (truncated):**
- **E** `agent-a160c72b:162` — `[DETECT] The re-scope premise for Batch B is
  false. The facade FathomContentAdapter returns raw dicts ... or None/[] TODO stubs`
- **D** `42c20de3:177` — `I need to correct what I told you earlier: the 25.5 KB
  result I attributed to the architecture-fit lens was actually the misconception lens`
- **C** `agent-ad92cca6:25` — `## Verdict: CONCERN (2 new regressions introduced by
  fix-1) ... the corpus_hash/repin_hash refactor introduced two ...`
- **B (TP)** `1c0b1e0f:1190` — `inspect the two store.py duplicate-method pairs to
  see if the shadowed (earlier) copy is safe to remove`
- **A (TP)** `877994fe:155` — `Hmm. ok. re-check the proposed steps to wind/un-wind
  and get files on the right branches ... So we can have a healthy main.`

---

## 5. Token cost

- **Detection:** 0 LLM tokens. Pure stdlib regex + within-file structural indexing,
  10.1 s for 64 k records. Fully deterministic, re-runnable.
- **Labeling:** 0 LLM tokens for content ingestion — only <=300-char snippets and
  +/-3 normalized-line windows entered context; **no transcript was ever read
  whole** (the hard rule held end-to-end).

---

## 6. Formalization verdict + which families can auto-feed which rubric criteria

**The suite is a working, ~0-cost failure-example *feeder*.** Ship it as a standing
collector, but gate families by their validation-confirmed precision:

| family | rubric criterion | val precision | status -> action |
|---|---|---|---|
| **E** halt-scout-falsify | **C8** (premise falsification / RCA save) + **B7** | 1.00 | **AUTO-FEED now** — cleanest; low volume, keep every hit |
| **D** self-correction | **D3 / F** (agent self-error, self-correction quality) | 0.90 | **AUTO-FEED now** — robust across splits |
| **C** review-catch | **D5 / C7** (reviewer defect catch) | 0.60 | **AUTO-COLLECT + light filter** — 94 % of volume & ~454 TPs, but route through the section-7 finding-id/PASS-summary filter before scoring |
| **B** unwitnessed-premise | **C8** (unwitnessed plan premise) | 0.30 | **HUMAN-TRIAGE only** until hardened (section 7); volume tiny (18) |
| **A** hitl-bounce | **B7 / D1/D4/D7** (HITL correction) | 0.33 | **HUMAN-TRIAGE only** until hardened (section 7); volume tiny (6) |

D and E are **reliable enough to auto-feed today**. C is the workhorse by volume
and is worth wiring in behind a cheap deterministic secondary filter. A and B must
not auto-feed yet — but their absolute volume is so small (6 and 18) that manual
triage is trivial in the interim.

---

## 7. Next steps (ranked)

1. **Ship D + E as the first auto-feeders** into the rubric-scoring pipeline
   (D3/F and C8/B7). No further work needed to start.
2. **Add a C secondary filter** (deterministic, still 0-LLM): drop bare `F\d`
   finding-ids that collide with metric (`F1`) / feature (`F5-ADR`) tokens unless
   adjacent to `finding`/`P\d`/`CR-`/`R-...-\d`; drop `... -> PASS` provenance
   summaries. Expected to lift C from ~0.60 toward ~0.8 and unlock its ~450-example
   yield.
3. **Harden B** (three named guards): (a) treat `"file.py", "line": N` quoted form
   as a witness; (b) drop `dead code`/`Dead Code` when it's a benchmark/hygiene
   *category* (co-occurring F1 score, `(vulture)`, eval-smell list); (c) drop
   `no <noun> embedder|reference embedder` and `no consumer-facing ... API`
   (API-surface prose). Re-measure on validation.
4. **Harden A**: extend `is_synthetic` to the `Monitor poll tick` / read-only-role
   standing banner; require a real redirect/correction cue for `hitl-soft-redirect`
   (not bare `reread`/`recheck` on a forward instruction).
5. **Draw the stratified Horvitz-Thompson sample** (strata = family x confidence,
   review-catch proportional to its 94 % share) for a defensible population
   precision with CIs — the macro numbers here are directional, not population.
6. **Re-run the CR-047/30-N smoke-test after any resplit** if the RCA sessions ever
   land in a held-out fold, for a second held-out confirmation of the B/E shapes.

---

### Artifacts
- Detectors: `detectors.py`, `parse.py`, `run_detectors.py`
- Validation output: `out/validation_candidates.jsonl` (+ `.summary.json`)
- Validation hand-labels: `validation_labels.tsv` (41 labeled, +/-3-window discipline)
- Experiment baseline: `out/experiment_candidates.jsonl`, `seed_labels_fp.jsonl`,
  `make_seed_labels.py`, `formalization.md`
