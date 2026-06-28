# FathomDB 0.8.11 — Plan (state-machine ladder) · **Agent-feedback + agent router**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.11-implementation.md` (authored at Slice 0); live state → `runs/STATUS-0.8.11.md`;
> deps/decision record → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.11` as an
> **orchestrator** session.
>
> **Theme.** Four workstreams converge on a single deliverable: an evidence-backed, agent-side L2 router
> prototype that ships regardless of EXP-S's outcome. (1) **EXP-AF** tests whether the agent relevance
> signal is worth the round-trip cost. (2) **EXP-Fr-acc/VoI finalize** adds the ask-or-not policy and
> asymmetric mis-route cost to the 0.8.9 classifier work. (3) The **agent-side L2 router prototype**
> carries per-intent config tuples and hedges the EXP-S KILL path (DP-A). (4) **#17 filter-grammar**
> (G4↔G10 unification, gap-37) delivers the typed-constraint surface the router will consume, now
> de-coupled from the 0.8.12 long pole.
>
> **Footprint.** Two footprint classes coexist here: the eval tracks (EXP-AF / Fr-acc/VoI) are
> **CALLER-SIDE-BYO-LLM / EVAL-ONLY**; the filter-grammar (#17) is **IN-LIBRARY**; the L2 prototype
> and dispatcher pre-stage are **CALLER-SIDE-BYO-LLM** (no engine changes). The library query path
> stays CPU-only, 1-bit/Hamming, deterministic throughout. Tag every technique.

---

## 1. Goal & scope

- **EXP-AF — Agent-feedback value test.** Using the `record_feedback` telemetry pipeline (landed
  0.8.8 Slice 15, I-1 satisfied), measure whether an agent relevance signal beats `ce_score`-only
  routing net of round-trip cost. Scope: existing substrate, no fresh 50–100-query rebuild; one-shot
  vs iterative (within the 1–2 depth bound from PSD §II.C). **KILL path:** if the agent signal does
  not beat `ce_score`-only net of round-trip → drop the feedback loop from the prototype; router
  stays on internal `ce_score` (mirrors EXP-S's KILL discipline). Prerequisite: EXP-OBS and real-gold
  pipeline (both landed, I-1 met).

- **EXP-Fr-acc/VoI finalize.** Extend the 0.8.9 EXP-Fr-acc results (classifier accuracy + initial
  mis-route cost matrix) with the three agent-signal additions specified in PSD §III.C: (a)
  _value-of-signal_ — does agent relevance beat `ce_score`-alone on routed queries? (b) _ask-or-not
  VoI policy_ — at which (`ce_score`, route-margin) pairs does the agent round-trip break even? (c)
  _asymmetric weighting_ — does the policy preferentially suppress the high-cost needle→global
  cross-wire (−0.362 measured) over cheap same-tier misses? These thresholds feed the prototype's
  VoI escalation logic and the 0.8.15 dispatcher design.

- **Agent-side L2 router prototype + EXP-Fr dispatcher pre-stage.** Build an agent-side prototype
  (CALLER-SIDE, Python, no EXP-S substrate dependency) that: routes queries over the 5-class intent
  taxonomy (`{needle | multi_session | temporal | global | multi_hop}`) using the per-intent config
  tuples from the pre-stage; exposes a recommendation API (intent, stack, confidence, cost tier)
  without executing; and accepts an agent hint or override. The per-intent config tuple registry
  (the _dispatcher pre-stage_) is the 0.8.11 design artifact the 0.8.15 EXP-Fr build integrates
  rather than invents (DP-B mitigation). The prototype is the DP-A hedge: a working agent-side
  router ships regardless of EXP-S's 0.8.12 locus verdict.

- **#17 filter-grammar — G4↔G10 unification (gap-37, IN-LIBRARY).** Unify the G4 typed filter
  grammar (`Predicate { JsonPathEq, JsonPathCompare, ScalarValue }` on `read.list`) and the shipped
  G10 `SearchFilter { source_type, kind, created_after, status }` on `search_filtered` into
  **one typed filter contract**. The unified type serves both compilation paths (vec0 metadata
  pre-filter for G10; json-path-over-allowlist for G4). The shipped G10 surface shape is touched;
  re-express it on the new contract without behavior change. Py + TS SDK parity required (X1). This
  typed constraint surface is what the 0.8.15 router's `constraints` block (`initial-arch` §5;
  PSD §I.B) leans on.

- **F-8b — `record_feedback` governance re-classification (review + decision + execution).** The
  0.8.8 HITL ruling (F-8b, sequencing §6) mandates a reclassification review at 0.8.11 (EXP-AF).
  Determine whether `record_feedback` remains observability instrumentation or graduates to a
  **first-class governed application command** (allowlist entry in
  `src/conformance/governed-surface-allowlist.json` + Rust facade + X1 surface suites). The trigger
  criterion: does EXP-AF make `record_feedback` a load-bearing agent-facing input to a
  learned/active-feedback loop rather than passive local telemetry? If promoted, execute the
  migration in this release. `enable_telemetry` and `last_telemetry_query_id` stay instrumentation
  unless EXP-AF evidence compels otherwise.

_Why this order in the line:_ I-1 (EXP-OBS) is met; the 0.8.9 Fr-acc classifier base is done;
real-gold pipeline exists. The agent-side prototype has no EXP-S dep → builds now, hedges the 0.8.12
KILL path. Filter-grammar was de-coupled from 0.8.12 by F-10; it lands here ahead of the dispatcher
so the 0.8.15 build inherits one constraint contract.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

_Track by experiment tag / G-gap / OPP-id / TDD name. No invented AC ids; new ACs at gated slices
only per §6._

| ID | Requirement | Acceptance signal |
| --- | --- | --- |
| R-FA-1 | VoI thresholds measured and registered | Slice 5: numeric (`ce_score`, route-margin) break-even points delivered; result committed to `runs/STATUS-0.8.11.md` and sequencing §6 finding |
| R-FA-2 | Asymmetric mis-route cost matrix extended | Slice 5: per (intent-class, route) mis-route cost updated; needle→global cross-wire cost confirmed measurable |
| R-FA-3 | Value-of-signal comparison complete | Slice 5: agent-signal vs `ce_score`-only lift/delta on routed query sample; CI-bounded |
| R-AF-1 | EXP-AF KILL/GO verdict + depth-bound decision | Slice 10 HITL readout: verdict recorded in experiments ledger; one-shot vs iterative settled (within 1–2 bound) |
| R-AF-2 | EXP-AF is pre-registered before any priced run | Slice 0: hypothesis + KILL path + cost ceiling in `runs/STATUS-0.8.11.md`; no priced queries before registration |
| R-L2-1 | Agent-side L2 prototype routes all 5 intent classes | Slice 15: smoke test covers `{needle, multi_session, temporal, global, multi_hop}`; recommendation returned without execution |
| R-L2-2 | Prototype carries and exposes per-intent config tuples | Slice 15: each class has a registered `(index, retrieval, α, pool_n, MMR, recency)` tuple; tuple registry is the pre-stage artifact for 0.8.15 |
| R-L2-3 | Prototype accepts agent hint / override without error | Slice 15: override test passes; override is respected without a fallback-to-internal route |
| R-L2-4 | Prototype footprint is CALLER-SIDE; no engine changes | Slice 15 codex §9: zero changes to `fathomdb-engine` / `fathomdb-py` crates; prototype lives outside the library |
| R-FIL-1 | One unified typed filter contract covers G4 and G10 paths | Slice 20: a single type serves both `read.list` (json-path compilation) and `search_filtered` (vec0 metadata compilation); parity test: same filter expression on both paths returns consistent results on a shared fixture |
| R-FIL-2 | Shipped G10 paths re-expressed on the new contract without behavior change | Slice 20: the G10 parity test is RED-first, then GREEN; no regression on existing `SearchFilter`-using test suite |
| R-GOV-1 | F-8b classification decision committed and executed | Slice 0 HITL decision recorded; Slice 20 execution: if promoted, `record_feedback` appears in the governed allowlist + Rust facade + X1 surface assertions; if kept, ADR note commits the "stay-instrumentation" evidence |
| R-GOV-2 | Governed surface count and X1 suites updated consistently | Slice 20: if F-8b promotes, `test_surface.py` and `surface.test.ts` assertions updated; if not, no change to the allowlist |
| R-X-1 | Py + TS SDK parity on the unified filter contract (and `record_feedback` if promoted) | X1 cross-binding harness green; `test_telemetry_parity.py` + `surface.test.ts` updated |

New ACs: candidates at Slice 0 (filter-grammar contract) and at the EXP-AF/Fr-acc readout (if the
agent-signal result warrants a new gate). HITL decides.

**HITL decision points (three):**

1. **F-8b** (Slice 0) — `record_feedback` instrumentation → governed command? Before Slice 20
   execution. Owner: steward.
2. **EXP-AF KILL/GO** (Slice 10 readout) — agent signal beats `ce_score`-only net of round-trip? If
   KILL: prototype drops the feedback arm; `record_feedback` stays instrumentation (overrides F-8b
   promotion). Owner: steward.
3. **Filter-grammar unification shape** (Slice 0 ADR) — unified single type or a thin adapter that
   routes to two compilation paths? (The two backing stores — vec0 metadata columns for G10;
   json-extract over allowlist for G4 — are structurally different; the ADR must settle whether the
   public type is unified or the internal compilation is routed behind a shared facade.)

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 40
         ↑             ↑
      (eval track)  (engineering track — 15 ∥ 20 off Slice 0)
```

| Slice | Title | Work-type | Footprint | Depends-on |
| ---: | --- | --- | --- | --- |
| **0** | Setup + ADRs — EXP-AF pre-registration; VoI extension design; filter-grammar unification ADR (compilation routing vs unified type — HITL F-8b decision before Slice 20); L2 prototype scope; stand up `runs/STATUS-0.8.11.md` | design-adr | — | — |
| **5** | **EXP-Fr-acc/VoI finalize (KEYSTONE eval)** — value-of-signal + ask-or-not VoI thresholds + asymmetric mis-route cost matrix extension; uses EXP-OBS telemetry + real-gold from 0.8.8 (I-1); HITL-gated cost spend | eval (EVAL-ONLY / CALLER-SIDE-BYO-LLM, small $) | CALLER-SIDE-BYO-LLM / EVAL-ONLY | 0 |
| **10** | **EXP-AF value test** — agent relevance signal vs `ce_score`-only; round-trip cost + VoI break-even; one-shot vs iterative; KILL/GO verdict → HITL readout; mis-route reduction | eval (EVAL-ONLY / CALLER-SIDE-BYO-LLM, small $) | CALLER-SIDE-BYO-LLM / EVAL-ONLY | 5 (VoI framework needed for comparison baseline) |
| **15** | **Agent-side L2 router prototype + dispatcher pre-stage** — Python CALLER-SIDE prototype; 5-class intent taxonomy; per-intent config tuple registry; recommendation API; hint/override contract; pre-stage artifact committed | implementation (CALLER-SIDE) | CALLER-SIDE-BYO-LLM | 0 (can start in parallel with eval track; soft dep on Slice 5 for VoI escalation thresholds — see note) |
| **20** | **#17 filter-grammar unification + F-8b execution** — unified typed filter contract on `read.list` (G4) + `search_filtered` (G10); RED→GREEN parity test; re-express shipped G10 paths; execute F-8b classification decision; Py + TS parity (X1) | implementation (IN-LIBRARY + governance) | IN-LIBRARY | 0 (F-8b HITL decision from Slice 0; filter-grammar ADR from Slice 0) |
| **40** | **Verification + Release Readiness (0.8.11)** — X1/X2/X3 + R-FA/R-AF/R-L2/R-FIL/R-GOV AC gate; experiments-ledger update (EXP-AF + Fr-acc/VoI); dispatcher pre-stage artifact reviewed; hand-off note for 0.8.15 committed | verification | — | 5, 10, 15, 20 |

**Keystones / hard gates.**

- **Slice 5 is the eval keystone.** Its VoI thresholds are the comparison baseline for EXP-AF (Slice
  10) and the escalation logic the L2 prototype (Slice 15) encodes. Slice 5 does not hard-block 15
  (the prototype can be built to provisional thresholds and updated), but the codex §9 review of
  Slice 15 must confirm VoI thresholds are consistent with Slice 5's output.
- **Slice 10 EXP-AF KILL/GO is a hard gate on the L2 prototype's feedback arm.** If KILL, the
  prototype drops the agent-signal loop before Slice 40 (a reserved-gap patch slice if needed).
- **F-8b HITL decision at Slice 0 gates Slice 20's governance execution.** Do not implement
  reclassification before the decision is recorded.
- **All priced eval runs use the resilient harness** (incremental checkpoint, `--resume`, 429/5xx
  backoff, completeness guard, $ ledger) — `priced-runs-need-resilience-before-spend`.

**Tracks (parallelizable after Slice 0).**

- Eval track: **5 → 10** (Fr-acc/VoI finalize → EXP-AF). Sequential within the track.
- Engineering track: **15 ∥ 20** (L2 prototype and filter-grammar are independent; both unblock at
  Slice 0). Slice 15 has a _soft_ dep on Slice 5's thresholds; run both tracks concurrently and
  reconcile at Slice 40 if the prototype precedes the eval readout.

---

## 4. Reserved-gap policy

Carried unchanged from `0.8.1-plan.md §Numbering`. If the Slice 0 filter-grammar ADR reveals the
unification is heavier than a single slice (e.g., the two compilation paths diverge enough to require
separate migration tests), a reserved-gap slice is inserted between 20 and 40 as a fully-orchestrated
follow-on off a fresh `main` baseline — never an ad-hoc patch. Similarly, if EXP-AF KILL triggers a
prototype feedback-arm removal, a reserved-gap patch slice handles the delta before Slice 40.

---

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + harnesses (Py↔TS) · X2 `mkdocs build --strict` green · X3 docs + DOC-INDEX entry per
slice. `runs/STATUS-0.8.11.md` carries the per-slice X column. For the eval slices (5, 10) the
"shippable" DoD is the **landed result doc + reproducible script** (not a published artifact);
X1/X2/X3 still apply to any surface change those slices incidentally touch.

The filter-grammar slice (20) triggers a DOC-INDEX update for: `docs/reference/python-api.md`,
`docs/reference/typescript-api.md`, `docs/guides/hybrid-search-filtering.md`, and
`src/conformance/governed-surface-allowlist.json` (if F-8b promotes).

---

## 6. Acceptance-criteria policy

`dev/acceptance.md` is locked (max AC-074); do not invent AC ids. Track by:

- Experiment tags (`EXP-AF`, `EXP-Fr-acc`), OPP-id (`OPP-9` for telemetry/real-gold), G-gap (`G4`,
  `G10`, gap-37), and TDD test names (`test_gold_pipeline.py`, `test_surface.py`,
  `surface.test.ts`, `test_telemetry_parity.py`).
- New ACs (if warranted) proposed at Slice 0 (filter-grammar contract) or Slice 10 readout (EXP-AF
  gate). HITL decides; none are pre-issued here.

---

## 7. Prerequisites

1. **I-1 satisfied (verified on `origin/main`).** EXP-OBS landed at 0.8.8 Slices 0/1/5 (commits
   `170c5109`, `8c938bb7`, `eec4ddb0`, `5c7b9f31`); field set ratified; `Explanation`/`QueryTrace`/
   `PerHitExplain` exist on main. The per-arm provenance + score breakdown the agent-signal track
   judges against physically exists.
2. **0.8.9 EXP-Fr-acc base complete.** The 5-class classifier accuracy + initial mis-route cost
   matrix landed in 0.8.9. 0.8.11 _extends_ this with agent-signal + VoI; it does not re-derive the
   classifier from scratch.
3. **Real-gold pipeline operational (OPP-9 / 0.8.8 Slice 15).** `record_feedback` appending to a
   local JSONL sink, `enable_telemetry` + `last_telemetry_query_id` tested in
   `test_gold_pipeline.py`. EXP-AF relies on real-gold rows captured during the 0.8.9 eval runs.
   Confirm local gold sink has a sufficient query sample before Slice 10; if not, a short cheap
   eval run at Slice 5 generates the sample (gated by R-AF-2 pre-registration).
4. **Worktrees off `$(git rev-parse main)`.** Every implementation slice uses a pre-created worktree
   off a fresh `main` baseline; see `agent-worktree-stale-base-trap`.
5. **0.8.10 not a gate.** 0.8.11 is OOB; it does not depend on 0.8.10 closing (odd/even
   tracks are independent except at hard edges, and there is no I-5 edge to 0.8.11).

**What 0.8.11 hands to 0.8.15 (the dispatcher build):**

| Artifact | Consumer at 0.8.15 |
| --- | --- |
| Fr-acc/VoI thresholds (`ce_score`, route-margin break-even) | EXP-Fr escalation logic |
| Asymmetric mis-route cost matrix (updated) | EXP-Fr forbidden-composition validator |
| EXP-AF KILL/GO verdict + depth bound (1 vs 2) | Whether 0.8.15 integrates agent-signal loop |
| Per-intent config tuple registry (pre-stage artifact) | EXP-Fr integrates rather than invents these; DP-B mitigation |
| Agent-side L2 prototype (working code) | If EXP-S KILL at 0.8.12, this _is_ the 0.8.15 router (DP-A hedge) |
| Unified filter contract (#17, IN-LIBRARY) | The `constraints` block the 0.8.15 dispatcher leans on |
| F-8b classification (committed) | 0.8.15 knows whether `record_feedback` is a governed command before wiring it |

---

## 8. Dependencies / sequencing

**Upstream (must be done before 0.8.11 can start):**

- I-1 edge: EXP-OBS (@0.8.8) — **satisfied** (F-6 in sequencing doc).
- 0.8.9 EXP-Fr-acc base — **satisfied** (0.8.9 closed, F-9 confirmed in sequencing doc).

**Within-release ordering (hard):**

1. Slice 0 before all others (ADRs gate Slice 20's execution and set the pre-registration for Slice 5).
2. Slice 5 before Slice 10 (VoI framework is the comparison baseline).
3. Slice 0 F-8b HITL decision before Slice 20 execution.
4. Slices 5/10/15/20 all before Slice 40.

**Parallelizable:**

- Slice 15 and Slice 20 are independent and can run concurrently after Slice 0.
- Slice 15 and Slice 5 can run concurrently (with the soft reconciliation note above).

**Downstream (0.8.11 must complete before these consume its artifacts):**

- **0.8.12 EXP-S readout** uses the Fr-acc/VoI thresholds + L2 prototype to make the formal locus
  decision (agent-side vs in-library). The locus decision is _not_ made in 0.8.11; 0.8.11 supplies
  the evidence.
- **0.8.15 EXP-Fr** is hard-gated on EXP-B′ ∧ EXP-S ∧ Fr-acc ∧ EXP-OBS — 0.8.11 closes the
  Fr-acc and pre-stage legs of that conjunction. EXP-B′ deadline is 0.8.12 (by-when table).
- **0.8.17 router hardening** builds on the 0.8.15 dispatcher and on EXP-AF productization. If
  EXP-AF is GO, 0.8.17 productizes the agent-signal loop; if KILL, it skips that arm.

**Interlock with master sequencing (F-10):**

0.8.11 is a net-new OOB slot (F-10, sequencing §6). The odd line skips 13; no x.y.13 release.
EXP-B′ was to be done by 0.8.12 (sequencing §5b); 0.8.11 does not execute EXP-B′ (that belongs in
the 0.8.9/0.8.11 window; confirm at Slice 0 whether it is already in the ledger or needs a
reserved-gap slot here). If EXP-B′ is not yet landed, it must be done before Slice 40 as a
pre-condition for the dispatcher pre-stage tuples to be valid across joint-tuning stacks.

**Size flag.** This release is assessed as _well-sized_: two eval slices (EVAL-ONLY, no engine
changes), one CALLER-SIDE prototype slice, one IN-LIBRARY engine slice (filter-grammar, moderate
complexity), and one governance action folded across Slices 0 and 20. The only scope-creep risk is
the filter-grammar unification: if the Slice 0 ADR reveals the two compilation paths cannot share a
single type cleanly, a reserved-gap slice is inserted (see §4). Flag to HITL if that occurs.

---

## 9. Immediate next slice

**Slice 0 — ADRs + pre-registration + HITL F-8b.** Stand up `runs/STATUS-0.8.11.md`. In parallel:

1. Author the **filter-grammar unification ADR** (the Slice 0 deliverable for #17): settle whether
   the unified `Filter` type is a single enum that routes internally to the two compilation paths, or
   a thin adapter over the two distinct types. Read `dev/adr/ADR-0.8.0-filter-grammar.md` and
   `dev/design/slice-10-design.md` as inputs. The ADR must state: (a) the new type's shape and name,
   (b) how it compiles to vec0 metadata columns (G10) vs json-extract (G4), (c) the parity test
   fixture, and (d) whether the existing `SearchFilter` struct is renamed/extended or replaced. Any
   `[TBD]` in the ADR is a blocker for Slice 20.

2. **Pre-register EXP-AF**: hypothesis, KILL criteria (`ce_score`-only is the baseline; the agent
   signal must beat it net of round-trip cost), corpus (real-gold from 0.8.8 OPP-9 pipeline), cost
   ceiling ($), and the reproducible script. Block any priced run until this is in `STATUS-0.8.11.md`.

3. **EXP-Fr-acc/VoI extension design**: specify the three additions (value-of-signal comparison
   method, VoI break-even calculation, asymmetric-weighting test) that extend the 0.8.9 Fr-acc
   results. State the output format that will feed the L2 prototype's VoI escalation logic.

4. **L2 prototype scope spec**: confirm the prototype is a Python CALLER-SIDE harness (no crate
   changes); specify the recommendation API shape consistent with `initial-arch` §5 (intent, stack,
   confidence, cost_tier, rationale); list which per-intent config tuples are provisional (from
   EXP-B′ if landed) vs pinned from EXP-0.

5. **F-8b HITL decision**: bring the reclassification checklist from
   `dev/plans/runs/NOTE-0.8.8-to-orchestrator-record-feedback-reclassify.md` to HITL; record the
   decision in `STATUS-0.8.11.md` before Slice 20 begins.

After Slice 0, fan out: eval track (Slice 5) and engineering track (Slices 15 ∥ 20) in parallel.
