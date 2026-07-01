# Planner-router — release sequencing, dependencies, and the by-when

> ⚠ SUPERSEDED on all release-number scheduling by `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (master) + `dev/plans/runs/0.8.x-renumber-reconciliation.md`. Dispatcher = 0.8.15, EXP-S = 0.8.14. Numbers retained below for design rationale.
>
> **⚠ SUPERSEDED for scheduling (2026-06-26).** This doc's sequencing is now folded into the **sole
> 0.8.x master release plan** — [`../plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`](../plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md)
> (§1b planner-router track · §2a hard-dependency table I-1…I-6 · §5 the by-when · §6 integration
> findings). Use that file as the schedule of record; this doc is retained as the design-rationale source.

**Status:** `Superseded-for-scheduling (rationale retained)` · **Date:** 2026-06-26 · **Owner:** [TBD: steward]
**Companions:** `planner-router-psd-0.8.x.md` (the solution design / planning input — this doc schedules its §VI),
`initial-arch-planner-router-0.8.x.md` (the contract/stance layer),
`0.8.x-portfolio-features-and-experiment-tree.md` (the experiment tree this sequences: EXP-0/A/M4/S/B′/Fr-acc/Fr/OBS/AF/C/D/E).
**Convention:** `[TBD: …]` = unresolved. *Provisional:* = a position for review. Release numbers are slots, not dates.
**Question this answers:** *when* do the planner-router experiments and the dispatcher build happen, given that the
non-planner-router todo already fixes the router's two prerequisites in the even-release plan; and *by-when* must the
implementation land due to dependencies?

---

## 0. The result in one box

> The planner-router work is **almost entirely out-of-band** ($0 eval/analysis, landable into the repo anytime) — with
> one exception: the **dispatcher build (EXP-Fr)**, which is real engine code hard-gated on the **kind-tagged substrate
> (EXP-S)** that lands at **0.8.14**. So the experiments run OOB across the odd slots **0.8.7 → 0.8.11 and must be *done*
> by 0.8.14**, and the dispatcher lands at **0.8.15**. **EXP-S (0.8.14) is the long pole.**
> Two of the router's hard prerequisites — **EXP-OBS (0.8.8)** and **EXP-S (0.8.14)** — already live in the *even*-release
> (non-planner-router) plan; the odd-release planner-router work is sequenced *around* those two landings.

---

## 1. The two interlocks that fix all the timing

The router's prerequisites are listed in the non-planner-router todo (items #1, #2) and are built in the **even** releases.
Everything else router-shaped is $0 eval and rides the **odd** releases around them.

| Router prerequisite | Lands (non-PR, even) | What it gates |
|---|---|---|
| **EXP-OBS** (#1 — retrieval-`EXPLAIN`: per-arm provenance + rrf/ce score breakdown + opt-in `explain=True`) | **0.8.8** | A *transparent* router (`initial-arch` §6); **and** the telemetry the **agent-relevance-signal** judges against + the reward log (`psd` §I.D/§V.E). Nothing agent-feedback can run before this. |
| **EXP-S** (#2 — kind-tagged coexisting indexes: schema/engine migration + multi-index write + determinism check) | **0.8.14** | The *in-library* dispatcher (EXP-Fr). Its **KILL path** — "router stays agent-side, indexes stay eval-side" (`tree` §4 EXP-S; `initial-arch` §8) — literally decides the router's **locus**. The in-library build cannot precede it. |

> **Watch item on EXP-OBS@0.8.8:** the agent-signal/VoI work needs the **per-arm provenance + score breakdown**, not just
> the already-shipped `ce_score`. If the 0.8.8 increment ships only `ce_score`, EXP-Fr-acc.6 / EXP-AF slip. Make the
> per-arm provenance part of the 0.8.8 scope, not a later increment.

---

## 2. Dependency graph

```text
  EVEN releases (non-planner-router plan)            ODD releases (planner-router work)
  ─────────────────────────────────────             ──────────────────────────────────
  0.8.5  EXP-0  CE α/pool_n/ce_score  ── LANDED ───►  (banked precision win)

  0.8.7  GPU embedder (#3) ───────────────────────►  0.8.7  EXP-A (recall)   EXP-M4 (embedder ceiling)
            │ 27h→min embed sweeps                            │                 │  (rides the GPU)
            │                                                 │                 │  OD-7: if swap feasible →
            │                                                 │                 │  re-whiten → eu7 re-clear
            │                                                 ▼                 ▼
            │                                          ┌──────────────────────────────┐
  0.8.8  EXP-OBS (#1) ── provenance+breakdown ──┐      │  Gate 0 re-scope + Gate 2     │ (PSD Phase-1, $0 analysis)
         real-gold/telemetry (#10) ─────────────┤      └──────────────────────────────┘
            │                                    │             │
            │                                    │             ▼
  0.8.9  CI micro (#12,#14) ──────────────┐      │      0.8.9  EXP-B′ (joint tuning)   EXP-Fr-acc (accuracy+mis-route)
            │                             │      └────────────►  + EXP-Fr-acc agent-signal half (now OBS exists)
            │                             │                          │
  0.8.12 OPP-2 recency (#7) ──────────────┼────► (F3 config the router will carry)
            │                             │                          ▼
            │                             └──────►  0.8.11  EXP-AF (agent-feedback value test)
            │                                                 + AGENT-SIDE L2 router prototype  ← no substrate dep
            │                                                          │
  0.8.14 EXP-S (#2) substrate ════════ KILL? ════════════════════════ │ ══► locus decision finalized here
            │   (#16 BM25F, #17 filter-grammar)                        ▼
            │                                          0.8.15  EXP-Fr  BUILD THE IN-LIBRARY DISPATCHER
  0.8.16 #15 importance, #4 ONNX embedder ───────►            (gates met: B′ ∧ S ∧ Fr-acc ∧ OBS)
            │                                                          │
  0.8.18 #5 vec-equiv, #13 bench, #11-full ──────►  0.8.15  router hardening · per-feature config wiring ·
                                                            EXP-AF productization · F4/F5 forks (EXP-C/D/E)
```

`════` = hard gate (cannot cross until the upstream lands). `───►` = feeds/informs. `OD-n` = order dependency from
`tree` §4.

---

## 3. Odd-release schedule

| Release | Existing OOB content | Planner-router work added | Why here |
|---|---|---|---|
| **0.8.7** | GPU embedder (#3) | **EXP-A** (recall gen) · **EXP-M4** (embedder ceiling) · PSD **Gate 0 re-scope + Gate 2** oracle bound | EXP-M4 rides the GPU landing (sweeps 27h→min); both feed EXP-B′. Gate 0/2 are $0 analysis on existing corpora. |
| **0.8.9** | CI micro (#12, #14) | **EXP-B′** (3-stage joint tuning) · **EXP-Fr-acc** (classifier accuracy + asymmetric mis-route matrix) | EXP-A/M4 done; OBS landed 0.8.8 → the **agent-signal** half of Fr-acc can start too. |
| **0.8.11** | *(free)* | **EXP-AF** (agent-feedback value test) · finalize Fr-acc agent-signal/VoI · **agent-side L2 router prototype** | Real-gold (#10) + OBS both exist. An *agent-side* router needs **no** substrate → buildable now (de-risks §5.1). |
| **0.8.15** | *(free)* | **EXP-Fr — build the in-library dispatcher** · finalize the **locus decision** per EXP-S's outcome | First odd slot after substrate (0.8.14). All gates met: EXP-B′ ∧ EXP-S ∧ EXP-Fr-acc ∧ EXP-OBS. |
| **0.8.15** | *(free)* | Router hardening · per-feature config-tuple wiring · EXP-AF productization (if it beat `ce_score`) · F4/F5 forks (EXP-C/D/E) as corpus lands | Capstone / overflow. |

---

## 4. What is out-of-band, and the one dependency that gives each a deadline

OOB = $0 eval/analysis, no engine surgery, landable into the repo anytime as scripts + result docs. Each has exactly one
downstream consumer that sets its **by-when**.

| Planner-router item | Cost | Blocked-to-START by | **Consumed by → deadline** | Land in |
|---|---|---|---|---|
| Gate 0 re-scope + Gate 2 oracle bound | $0 | — | foundational (informs all) | **0.8.7** |
| EXP-A (recall generation) | $0 | — | EXP-B′ | **by 0.8.9** |
| EXP-M4 (embedder ceiling) | $0 / GPU | wants GPU #3 (0.8.7) for speed | EXP-B′ — OD-7 serial chain (swap → re-whiten → eu7 re-clear → re-tune α) | **by 0.8.9** |
| EXP-B′ (3-stage joint tuning) | $0 + $ judge | EXP-A, EXP-M4 | EXP-Fr | **by 0.8.14** |
| EXP-Fr-acc (accuracy + mis-route matrix) | $0 + small $ | — | EXP-Fr | **by 0.8.14** |
| EXP-Fr-acc agent-signal + EXP-AF | small $ | **EXP-OBS (0.8.8) + real-gold #10 (0.8.8)** | agent-loop productization | **by 0.8.15** |
| EXP-Fr-acc **locus decision** | $0 | **EXP-S outcome (0.8.14)** | the *form* of EXP-Fr | **resolves at 0.8.14** |
| **EXP-Fr — dispatcher build** *(NOT OOB)* | engine build | **EXP-B′ ∧ EXP-S (0.8.14) ∧ Fr-acc ∧ EXP-OBS** | the router / product | **lands 0.8.15** |

**The only non-OOB, must-be-sequenced piece is the dispatcher build (EXP-Fr).** EXP-OBS and EXP-S are also real builds,
but they are already placed in the even (non-planner-router) plan; the router merely consumes them.

---

## 5. By-when (bottom line)

- **The dispatcher (EXP-Fr) lands at 0.8.15 and cannot come earlier** — EXP-S at 0.8.14 is the long pole. It
  lands at **0.8.15**.
- **All feeding experiments must be *done by 0.8.14*** so 0.8.15 builds with no idle wait and no stale re-runs:
  EXP-A/M4 by 0.8.9 → EXP-B′ + Fr-acc by 0.8.11 → locus finalized at 0.8.14 with EXP-S.
- **Long pole = EXP-S (0.8.14).** Everything router-shaped is OOB and finished before it; **if EXP-S slips, the router
  slips one-for-one.** Same one-for-one coupling holds for EXP-OBS@0.8.8 → the agent-signal track.

---

## 6. Contingencies — explicit decision points

### Decision point A — EXP-S KILL path (at 0.8.14): in-library vs agent-side router

**Trigger:** EXP-S's determinism/perf check fails — the coexisting kind-tagged indexes are not deterministic or not fast
enough in-product (`tree` §4 EXP-S KILL; `initial-arch` §8).

**If KILL fires:**

- The **in-library dispatcher is off the table** for now — "router stays agent-side, indexes stay eval-side."
- **0.8.15 ships the agent-side router + L1 hardening instead**, not the in-library dispatcher.
- The in-library version becomes a later upgrade, re-gated on a fixed substrate.

**Pre-mitigation (why 0.8.11 builds an agent-side prototype):** the **agent-side L2 router has no substrate dependency**
— it orchestrates the existing L1 arms agent-side over the eval substrate. Building it at **0.8.11** means **a router
ships either way**; EXP-S only decides whether the *in-library* locus is also available. This is the "both-layered"
recommendation (`initial-arch` §2) expressed as a schedule hedge.

**Decision owner / when:** [TBD: steward], at the 0.8.14 EXP-S readout, jointly with EXP-Fr-acc's locus decision.

### Decision point B — EXP-B′ divergence (at ~0.8.9–0.8.11): does the router move onto the parity critical path?

**Trigger:** EXP-B′ joint-tuning shows the per-feature stacks **cannot unify under one config** — recall⇄precision is
Pareto-blocked, so F1 and F2 need *different* `(index, retrieval, α, pool_n, MMR, recency)` tuples (`tree` §4 EXP-B′:
"Pareto-blocked → per-intent stacks diverge → Fr").

**If divergence fires:**

- Per-intent **parity-or-better for F1/F2 can no longer ship without the router** — a single global config can't realize
  both wins, so the router stops being a "capstone" and becomes the **gate for the parity product claim**.
- EXP-Fr's **priority rises** (it's now blocking a headline competitor result), **but it still cannot precede EXP-S
  (0.8.14)** — so the effect is to *raise the cost of any router slip*, not to pull the date earlier.
- Mitigation: if this is foreseeable from EXP-B′ early signal, make the **agent-side router at 0.8.11** carry the
  per-intent tuples so the parity claim can be demonstrated agent-side before the in-library build.

**If stacks unify instead:** F1/F2 register at parity under one config (`tree` §4 "stacks unify → register"); the router
reverts to a capstone/convenience and the 0.8.15 date is comfortable.

**Decision owner / when:** [TBD: steward], at the EXP-B′ readout (~0.8.9–0.8.11).

---

## 7. Open items for the steward

1. **Ratify the long pole.** Accept EXP-S@0.8.14 → EXP-Fr@0.8.15 as the binding sequence, with all experiments done by
   0.8.14? (§5)
2. **Agent-side prototype at 0.8.11** — fund it as the EXP-S-KILL hedge and the EXP-B′-divergence parity vehicle? (§6 A/B)
3. **EXP-OBS@0.8.8 scope** — confirm the 0.8.8 increment includes per-arm provenance + score breakdown (not just
   `ce_score`), else the agent-signal track slips. (§1 watch item)
4. **Odd-slot crowding** — 0.8.7 and 0.8.9 already carry OOB non-planner-router items (GPU, CI micro); confirm the $0
   eval work co-resides there rather than needing its own slots. (§3)
5. **EXP-AF placement `[TBD]`** — does EXP-AF gate EXP-Fr or ride parallel into 0.8.15 productization? (`psd` Open-Q 10)
