# FathomDB 0.8.11.2 — Plan (pico umbrella) · **OPP-1 / OPP-3 / OPP-6 + Cause-A**

> **Plan-as-pico-umbrella.** This is a **calendar-decoupling landing vehicle**, not an engine release and
> **not a sequencing override.** It gathers four already-decided work items — **OPP-1, OPP-3, OPP-6, and
> Cause-A** — under one label so each can start the moment its OWN Phase-0 prerequisites are met, instead
> of waiting for the 0.8.10 / 0.8.12 / V-gate calendar. Read first:
> `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (findings **F-13** + **F-14** — the disposition of
> record), `dev/plans/runs/experiment-roadmap-placement-proposal.md` (the source placement rationale,
> esp. §3 sequenced action plan + §5 exit criteria), and the concrete V-3 / V-7 experiment designs in the
> Memex repo (`dev/fathomdb/OPP-1-experiments.md` / `OPP-3-experiments.md`).
>
> **Label note (HITL 2026-06-29; two-tier model, F-13/F-14).** `0.8.11.2` is a **normal pico** under the
> standing two-tier numbering model: `x.y.z` = real, publishable releases (manifest bump + `v*` tag +
> registry publish; publishing is always a separate explicit HITL call), and `x.y.z.p` **picos** =
> label-only, **never-published** work-completion increments for OOB/transitory work. This umbrella is
> therefore **transitory + label-only**: NO manifest version bump, NO `v*` tag, NO publish. Cause-A's code
> lands on `main` but **Memex consumes it via a local build**; a publishable `x.y.z` is a separate later
> HITL call. (`13` stays forbidden as minor and micro — no `0.13.x`, no `0.8.13`.)
>
> **Distinct from `0.8.11.1`.** `0.8.11.2` is a **different pico off the same 0.8.11 parent** than
> `0.8.11.1` (the dependency **Library Sweep**, F-12). The two do not share scope; this plan does **not**
> touch, merge, or re-scope `0.8.11.1`.
>
> **Sequencing PRESERVED (the load-bearing caveat).** The pico is a landing vehicle, **not** a sequencing
> override. The hard dependency order is unchanged: **Phase A (runnable now) → V-1 (live-CE
> re-validation) → OPP-1 = V-3 (held behind V-1) → OPP-3 bears at V-7**; Cause-A runs parallel and gates
> only the real-gold *adoption* arms. The acceleration is **decoupling from the release cadence**, not
> pulling any gate forward.
>
> **Footprint.** Experiments produce **verdicts / eval artifacts, not shipped features.** Cause-A is
> additive code (a stable-id field on `SearchHit` + bindings + telemetry/gold keying), default-off and
> behavior-neutral on the existing query path. The priced arms (frontier answerer / oracle extraction)
> are EVAL-ONLY and run under a pre-set `$` ceiling with the priced-run resilience preconditions.

---

## 0. START HERE — Phase-0 prerequisites (do before any priced run)

The pico decouples from the **calendar**, not from the **prerequisites**: each item starts when its OWN
Phase-0 conditions are met. Stand up `runs/STATUS-0.8.11.2.md` and clear these first (placement proposal
§3 Phase 0):

1. **Pin the §8b per-experiment ownership split** at kickoff (the two liaison sessions) for **OPP-1** and
   the **OPP-6 sweep**: FathomDB owns harness / corpora / at-power protocol / eval-metrics; Memex owns the
   extraction + decompose/oracle LLMs, the dependency call, and the merge (decomposer/extractor stay
   Memex-side per the cohesion seam).
2. **Set the `$` ceilings** for every priced pass — **before spend**, with the priced-run resilience
   preconditions (incremental checkpoint + verified `--resume` + 429/5xx backoff + window-fit +
   completeness guard + a running `$` ledger):
   - OPP-1: the frontier-answerer passes;
   - OPP-3: the native-gap characterization (both answerers × corpora) + answerer passes;
   - OPP-6: the C3/C4 frontier/oracle extraction passes.

   (C0/C1 + academic arms are local/`$0` and may start without the ceiling.)
3. **MuSiQue re-pull preserving `question_decomposition`** (FathomDB-side — we hold the corpus): modify
   `tests/corpus/scripts/acquire_musique.py` to retain the native per-hop
   `{question, answer, paragraph_support_idx}` list and re-pull
   `data/corpus-data/raw/musique_dev.jsonl`; verify all 2,417 answerable rows carry it. **Unblocks OPP-1
   A3 (oracle-decompose).** Do **not** synthesize labels.
4. **FathomDB eval-support add (small, OOB):** expose `margin` as a *measurement* (decoupled from the V-7
   verb-shape decision) + the **distractor-injection / gold-rank-demotion** knobs + confirm per-corpus
   `decide_08x`. Rides the Cause-A pico or a tiny eval micro. **Unblocks OPP-3.**
5. **Confirm the Memex 0.5.2 value-test harness** is the runner/vehicle for the as-Memex / real-gold
   arms; the academic arms are **not** blocked on it (they run on FathomDB `decide_08x`).

---

## 1. Goal & scope

In scope — the **four** decided items, gathered under one label-only pico:

- **OPP-6 — ELPS extraction-coverage sweep (EXP-COV-0..3).** Coverage→outcome sweep; EXP-COV-0 also
  re-measures the per-corpus relevance ceiling. **Output gates the 0.8.10 #6 ELPS-coverage
  invest/de-prioritize decision** (the dependency is intact; only the execution venue moved here).
- **OPP-3 — cascade / CE-default-on evals.** Native-gap characterization first, then the marginal-band
  treatment; **per-corpus, never pooled.** Final cascade/CE bearing is **recorded at V-7**.
- **OPP-1 — agent self-iteration experiments (EXP-ITER-D/-P/-POLICY).** Runs at the **V-3** gate position
  on the improved-recall substrate, **held strictly behind V-1** (NOT pulled forward). A4 / EXP-AF-MH is
  the deferred, lower-priority arm under the EXP-AF cost protocol.
- **Cause-A — stable hit-id for real-gold adoption.** **Size-it-first → cut**: additive stable-id field
  on `SearchHit` + 4 bindings + telemetry/gold keying. **Gates only the real-gold *adoption* arms (+
  OPP-9 join / graph), NOT the academic experiment arms.** Distinct from / lighter than the F-8a G0
  `write_cursor`→`logical_id` swap (additive, not a carrier reshape).

**Out of scope:** anything in `0.8.11.1` (the Library Sweep — do not touch); the F-8a G0 identity-substrate
swap (Cause-A is the lighter, additive alternative); any publishable `x.y.z` cut (a separate later HITL
call); pulling any V-gate forward (the calendar is decoupled, the sequencing is not).

---

## 2. Phased sequencing (the dependency order — UNCHANGED)

```text
Phase A (runnable now) ─► V-1 (keystone) ─► OPP-1 = V-3 ─► OPP-3 bears @ V-7
  OPP-6 EXP-COV-0..3      re-run EXP-B'     EXP-ITER-D/-P    cascade/CE
  OPP-3 cascade evals     on live CE        /-POLICY on the  default-on
  Cause-A (size→cut)      (default-reranker improved-recall  packaging
  OOB margin/rank knobs   ON)               substrate        decision

Cause-A ── parallel; size-it-first ──► gates real-gold ADOPTION arms only (+ OPP-9 join / graph)
Memex 0.5.2 value-test harness ─► the VEHICLE for all as-Memex / real-gold arms
```

Hard edges (placement proposal §2): **OPP-6/EXP-A (recall) → V-1 → V-3/OPP-1** (HITL §8c: V-3 is *not*
pulled forward). Soft/parallel: OPP-3, Cause-A. Vehicle dependency: the Memex 0.5.2 harness for the
as-Memex arms (academic arms run on FathomDB `decide_08x` without it).

| Phase | Work | Runs when | Gates / consumed by |
|-------|------|-----------|---------------------|
| **A (now)** | OPP-6 EXP-COV-0..3 · OPP-3 cascade evals · Cause-A size-it-first→cut · OOB `margin`/distractor-rank eval-support | Phase-0 prereqs met (no calendar wait) | OPP-6 → 0.8.10 #6 decision; OPP-3 → V-7; recall substrate → V-1 |
| **V-1 (keystone)** | Re-run EXP-B′ on the **live CE engine** (default-reranker ON); fill global + multi_hop; add MMR + recency | after Phase A recall work | everything downstream re-validates against this |
| **V-3 = OPP-1** | EXP-ITER-D/-P/-POLICY on the improved-recall substrate; at-power on MuSiQue + HotpotQA (2Wiki → V-5) | **after V-1** (NOT pulled forward) | the agent self-iteration build/adopt gates |
| **V-7** | CE-default-on packaging decision — records OPP-3's cascade/CE bearing + the `margin` verb-shape decision | gate position | the CE-default-on product decision |
| **Cause-A (parallel)** | additive `SearchHit` id field + 4 bindings + telemetry/gold keying | Phase A; parallel | sequences before the real-gold **adoption** phase of OPP-1/3/6 (+ OPP-9 join / graph) |

(V-2 per-query arm oracle, V-4 real classifier, V-5 multi-corpus, V-6 competitor head-to-head run in
their own gate positions; OPP-1/OPP-3 touch V-3/V-7 specifically.)

---

## 3. Requirements + acceptance criteria (DoD)

| ID | Requirement | Acceptance signal (falsifiable) |
|----|-------------|---------------------------------|
| R-U-1 | Each of the four items is dispositioned with a landed verdict/artifact, not an `AGREED` | per-item result doc + repro script recorded in `runs/STATUS-0.8.11.2.md` |
| R-U-2 | Sequencing preserved — no V-gate pulled forward | V-3/OPP-1 starts only after V-1 lands; recorded in the status board |
| R-U-3 | OPP-6 output gates the 0.8.10 #6 decision | the EXP-COV curve + invest/de-prioritize recommendation is recorded against 0.8.10 #6 |
| R-U-4 | Label-only: no manifest bump / tag / publish | `git diff` shows no `version =`/`"version":` change in shipped manifests; no `v*` tag; Memex builds Cause-A locally |
| R-U-5 | Cause-A is additive + behavior-neutral on the existing path | default query path byte-unchanged; the new id field is opt-in/telemetry-only |
| R-U-6 | `0.8.11.1` untouched | no edit to `plan-0.8.11.1.md` or the Library Sweep scope |
| R-U-7 | Priced runs respect the `$` ceiling + resilience preconditions | running `$` ledger ≤ ceiling; checkpoint/resume/backoff verified before spend |

---

## 4. Per-item exit criteria (lifted from the placement proposal §5)

Verdicts come from **running**, not from `AGREED`:

- **OPP-6.** Coverage is the lever iff Δ(gold-in-pool) or Δ(F1) CI-lower-bound > +0.04 on ≥1 class net of
  cost, with the precision guard, confirmed on real gold; a **flat curve at the per-corpus ceiling
  resolves** OPP-6 by redirecting to embedder/recall.
- **OPP-1.** Build-GO iff A1/A2 show a significant recall + answer lift over A0 (net of read cost) on
  MuSiQue + HotpotQA, A3 confirms head-room; **Adopt-GO is a separate gate** (frequency audit + real
  personal gold).
- **OPP-3.** If no corpus shows answer-gap ≥ ~0.08 with a signal AUROC ≥ 0.70 → cascade **stays parked**
  (structurally confirmed); if a thin/personal regime clears the bar → flip-ON candidate, re-measured on
  real Memex turns before adoption. Always publish the per-corpus `ce_top` dominance table.
- **Cause-A.** Size-it-first confirms additive-only (a field on `SearchHit` + 4 bindings + telemetry/gold
  keying; watch the `logical_id = NULL` doc-node case + the F-8a gold-id-contract revisit), then cut the
  OOB pico; it **gates only the real-gold adoption arms**, not the academic arms.

---

## 5. Cross-cutting DoD (bind every item)

- **X1 — SDK parity.** Cause-A's `SearchHit` field + bindings must keep Py↔TS parity; no SDK drift.
- **X2 — `mkdocs build` stays green** if any `docs/` touched (none expected).
- **X3 — docs/changelog.** Label-only umbrella ⇒ a single changelog/maintenance note (no version bump);
  update `runs/STATUS-0.8.11.2.md` per item.

---

## 6. Prerequisites (before any work opens)

1. **`main` is clean and current** (`git rev-parse --abbrev-ref HEAD` = `main`; `== origin/main`).
2. **Worktree hygiene:** any code work (Cause-A) gets a unique worktree cut from a verified `origin/main`
   tip; never the shared/primary checkout. One `maturin develop` at a time for any binding rebuild.
3. **0.8.11 is complete + merged** (PR #122); this baselines off current `origin/main`.
4. The **Phase-0 prerequisites** in §0 are cleared for the specific item being started (they are
   per-item, not a single global gate).

---

## 7. Decisions taken (recorded)

- 2026-06-29 — `0.8.11.2` pico umbrella established for OPP-1 / OPP-3 / OPP-6 + Cause-A; calendar-decoupled,
  sequencing preserved (Phase A → V-1 → OPP-1@V-3 → OPP-3@V-7; Cause-A parallel) · HITL, F-14.
- 2026-06-29 — `0.8.11.2` is a **normal pico** under the two-tier model (label-only, never-published);
  publish = a separate later HITL `x.y.z`; Memex consumes Cause-A via local build · HITL, F-13/F-14.
- 2026-06-29 — `0.8.11.2` is **distinct from `0.8.11.1`** (the Library Sweep); the latter's scope is
  untouched · F-12/F-14.

---

## 8. Open questions for the human (raise at Phase 0)

1. **`$` ceilings** per priced pass (Phase 0 #2) — confirm the per-experiment caps before any spend.
2. **§8b ownership split** per experiment (Phase 0 #1) — confirm the FathomDB/Memex boundary.
3. **OOB `margin`/knobs eval-support** — bundle with the Cause-A pico, or a separate tiny eval micro?
4. **Label-only confirmation** — confirm no publish/tag for this umbrella; Cause-A reaches Memex via a
   local build until a later HITL-cut publishable `x.y.z`.

---

## 9. Immediate next step

Stand up `runs/STATUS-0.8.11.2.md`; clear the §0 Phase-0 prerequisites for the first item to start
(Phase A is runnable now once its prereqs are met); **post the §8 questions to HITL and PAUSE** on the
`$` ceilings + ownership split before any priced run. Phase A (OPP-6 + OPP-3 + Cause-A + the OOB
eval-support) may begin in parallel as each item's prerequisites clear; V-1 → V-3 → V-7 follow in strict
order (sequencing preserved).
