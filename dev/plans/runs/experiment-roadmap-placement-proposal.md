# Proposed approach — placing the OPP-1 / OPP-3 / OPP-6 experiments in the 0.8.x roadmap

> **Status: WORKING PROPOSAL (untracked).** Authored by the FathomDB-side ledger-reconcile session,
> 2026-06-29, for the Program Steward / HITL. **Not committed; not contracted.** Apply into the master
> (`dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`) + the Memex roadmap only on HITL/Steward direction.
>
> **Inputs:** the three experiment designs in the Memex repo (`dev/fathomdb/OPP-{1,3,6}-experiments.md`),
> the ledger + projection (`LEVERAGE-OPPORTUNITIES-LEDGER{,-ARCHIVE}.md`,
> `fathomdb-memex-ledger-projection.md`), the 0.8.11→0.8.15 hand-off (V-1..V-7 gate, §8b/§8c), and the
> master release plan.

---

## 1. TL;DR

The experiments map onto **existing roadmap anchors — no new releases are required**, with one already-decided
exception (the **Cause-A standalone OOB pico**). They split into **two efforts, not one monolith**, because of
one hard HITL-imposed ordering: **recall-first → V-1 (live-CE re-validation) → V-3 (iteration)**.

- **Effort A — recall/headroom evals, runnable NOW:** OPP-6 (coverage) + OPP-3 (cascade). Feed the 0.8.10
  coverage decision and the improved-recall substrate.
- **Effort B — the Pre-0.8.15 Validation Gate (already one sequential campaign):** OPP-1 = **V-3**, run in
  strict **V-1→V-7** order, after 0.8.12 (EXP-S), before 0.8.15 (dispatcher).
- **Parallel:** the **Cause-A OOB pico** (size-it-first) — gates only the real-gold *adoption* arms (+OPP-9/
  graph), **not** the academic experiments.

## 2. Dependency chain

```text
recall/coverage work ─► live-CE re-validation ─► iteration on improved recall
  OPP-6 EXP-COV-0..3        V-1 (re-run EXP-B′       V-3 = OPP-1 EXP-ITER-D/-P/-POLICY
  + EXP-A breadth           on default-reranker ON)   (A4/EXP-AF-MH deferred)

OPP-3 cascade evals ── independent; runnable now ──► final CE bearing recorded at V-7
Cause-A OOB pico ───── parallel; size-it-first ────► gates real-gold ADOPTION arms only
Memex 0.5.2 value-test harness ─► the VEHICLE for all as-Memex / real-gold arms
```

Hard edges: **OPP-6/EXP-A (recall) → V-1 → V-3/OPP-1** (HITL §8c: V-3 is *not* pulled forward).
Soft/parallel: OPP-3, Cause-A pico. Vehicle dependency: the Memex 0.5.2 harness for the as-Memex arms
(academic arms can run on FathomDB `decide_08x` without it).

## 3. Sequenced action plan

### Phase 0 — kickoff prerequisites (do before any priced run)
1. **Pin the §8b per-experiment ownership split** at kickoff (the two liaison sessions) for **OPP-1** and the
   **OPP-6 sweep**: FathomDB owns harness / corpora / at-power protocol / eval-metrics; Memex owns the
   extraction + decompose/oracle LLMs, the dependency call, and the merge (decomposer/extractor stay
   Memex-side per the cohesion seam).
2. **Set the `$` ceilings** for every priced pass — **before spend**, with the priced-run resilience
   preconditions (incremental checkpoint + verified `--resume` + 429/5xx backoff + window-fit + completeness
   guard + a running `$` ledger):
   - OPP-1: the frontier-answerer passes;
   - OPP-3: the native-gap characterization (both answerers × corpora) + answerer passes;
   - OPP-6: the C3/C4 frontier/oracle extraction passes.
   *(C0/C1 + academic arms are local/`$0` and may start without the ceiling.)*
3. **MuSiQue re-pull preserving `question_decomposition`** (FathomDB-side — we hold the corpus): modify
   `tests/corpus/scripts/acquire_musique.py` to retain the native per-hop
   `{question, answer, paragraph_support_idx}` list and re-pull `data/corpus-data/raw/musique_dev.jsonl`;
   verify all 2,417 answerable rows carry it. **Unblocks OPP-1 A3 (oracle-decompose).** Do **not** synthesize
   labels.
4. **FathomDB eval-support add (small, OOB):** expose `margin` as a *measurement* (decoupled from the V-7
   verb-shape decision) + the **distractor-injection / gold-rank-demotion** knobs + confirm per-corpus
   `decide_08x`. Can ride the Cause-A pico or a tiny eval micro. **Unblocks OPP-3.**
5. **Confirm the Memex 0.5.2 value-test harness** (the SUT driver) is the runner for the as-Memex / real-gold
   arms; academic arms are not blocked on it.

### Phase A — recall/headroom evals (runnable now; parallel)
6. **OPP-6 EXP-COV-0..3** (coverage→outcome sweep, Memex-driven, Fathom-supplies the held-fixed stack +
   extract seam + index rebuild + `decide_08x`). **EXP-COV-0 also re-measures the per-corpus relevance
   ceiling.** Output gates **0.8.10 #6 ELPS-coverage** (eval-gated: flat curve ⇒ de-prioritize; real
   multi_session/temporal lift ⇒ invest).
7. **OPP-3 cascade evals** (Memex-driven on `eval/routing/` + held-out judge; FathomDB supplies `margin`
   measurement + knobs + per-corpus `decide_08x`). **Native-gap characterization first**, then the
   marginal-band treatment; **per-corpus, never pooled.** Final cascade/CE-default-on bearing recorded at V-7.

### Phase B — Pre-0.8.15 Validation Gate (strict V-1→V-7; after 0.8.12 EXP-S, before 0.8.15)
8. **V-1 (keystone):** re-run EXP-B′ on the **live CE engine** (default-reranker ON); fill global + multi_hop;
   add MMR + recency. *Everything downstream re-validates against this.*
9. **V-3 = OPP-1 EXP-ITER-D/-P/-POLICY** on the **improved-recall substrate** (Phase A + EXP-A), at-power on
   **MuSiQue + HotpotQA** (2Wiki deferred to the V-5 close). **A4 / EXP-AF-MH** runs as the deferred,
   lower-priority arm **under the EXP-AF cost protocol** (asymmetric + `c_rt`). Jointly owned. **Not pulled
   ahead of V-1.**
10. **V-7:** CE-default-on packaging decision — records OPP-3's cascade/CE bearing and the `margin`
    verb-shape decision (the measurement was already taken in Phase A).
    *(V-2 per-query arm oracle, V-4 real classifier, V-5 multi-corpus, V-6 competitor head-to-head run in
    their gate positions; OPP-1/OPP-3 touch V-3/V-7 specifically.)*

### Parallel track — Cause-A standalone OOB pico
11. **Size-it-first** (confirm additive-only: a field on `SearchHit` + 4 bindings + telemetry/gold keying;
    watch the `logical_id = NULL` doc-node case + the F-8a gold-id-contract revisit), then **cut the OOB
    pico.** Distinct from / lighter than the F-8a G0 `write_cursor`→`logical_id` swap. **Sequence before the
    real-gold *adoption* phase** of OPP-1/3/6 (and for OPP-9 join / graph). Does **not** block the academic
    experiment arms.

## 4. Roadmap deltas to apply (Steward / HITL)

**FathomDB master (`0.8.6-0.8.16-PROGRAM-SEQUENCING.md`):**
- **0.8.10 row:** mark **#6 ELPS coverage as eval-gated by OPP-6 EXP-COV-0..3** (build≠adopt; may
  de-prioritize).
- **Add the Cause-A OOB pico** to §4 (size-it-first → cut); note scope vs F-8a.
- **Cross-ref** the V-3 / V-7 concrete designs (Memex `OPP-1-experiments.md` / `OPP-3-experiments.md`) and
  add the small **OOB `margin`-measurement + distractor/rank-demotion** eval-support deliverable.

**Memex roadmap (`dev/ROADMAP.md`):**
- Flag the **0.5.2 value-test harness as the runner/vehicle** for Effort A's as-Memex arms + all real-gold/
  adoption arms (academic arms run on FathomDB `decide_08x` without it). OPP-3 is already Active — tie it to
  Effort A.

## 5. Exit criteria (per experiment — verdicts come from *running*, not from `AGREED`)

- **OPP-6:** coverage is the lever iff Δ(gold-in-pool) or Δ(F1) CI-lower-bound > +0.04 on ≥1 class net of
  cost, with the precision guard, confirmed on real gold; a flat curve at the per-corpus ceiling **resolves**
  OPP-6 by redirecting to embedder/recall.
- **OPP-1:** Build-GO iff A1/A2 show a significant recall + answer lift over A0 (net of read cost) on
  MuSiQue + HotpotQA, A3 confirms head-room; Adopt-GO is a separate gate (frequency audit + real personal
  gold).
- **OPP-3:** if no corpus shows answer-gap ≥ ~0.08 with a signal AUROC ≥ 0.70 → cascade stays parked
  (structurally confirmed); if a thin/personal regime clears the bar → flip-ON candidate, re-measured on real
  Memex turns before adoption. Always publish the per-corpus ce_top dominance table.

## 6. HITL flags (not for the orchestrator to decide)

- the `$` ceilings (Phase 0 #2);
- the §8b ownership split per experiment (Phase 0 #1);
- whether to bundle the OOB `margin`/knobs eval-support with the Cause-A pico or a separate eval micro;
- cross-repo mirroring so the FathomDB master + Memex roadmap carry consistent, matching entries.

## 7. One-effort vs phased — why two

Folding V-3/OPP-1 into the "run now" bucket would (a) violate HITL §8c (V-3 waits behind the V-1 live-CE
tuple re-validation) and (b) test iteration on the *unimproved* recall substrate — the exact error the
hand-off throughline warns against ("improve the recall functions first, then re-run the router/agent
experiments on the better substrate"). OPP-6 + OPP-3 *are* one parallel evals effort (shared harness +
per-corpus baseline methodology); OPP-1 belongs to the later, sequential V-gate effort.
