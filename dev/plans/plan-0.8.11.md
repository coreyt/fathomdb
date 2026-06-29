# FathomDB 0.8.11 — Plan (state-machine ladder) · **Planner-router experiment ladder (discharge) + agent-feedback + agent router**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.11-implementation.md` (authored at Slice 0); live state → `runs/STATUS-0.8.11.md`;
> deps/decision record → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (**F-11**). Run via `/goal complete 0.8.11`
> as an **orchestrator** session.
>
> **Why this release grew (F-11 / HITL 2026-06-28).** The original 0.8.11 plan assumed its upstream
> experiment ladder — `Gate-0/2 → EXP-A/M4 → EXP-B′ → EXP-Fr-acc base` — had already run as $0 "float"
> co-hosted on 0.8.7/0.8.9. A git-grounded steward finding (**F-11**) proved it **never ran**: 0.8.7
> closed on its GPU-embedder DoD only, 0.8.9 on CI-integrity only, the experiments-ledger has **zero**
> planner-router rows, and there are **no** `runs/` artifacts. Unowned, ungated float does not get done.
> **HITL ruling (2026-06-28):** 0.8.11 **owns and discharges the full missed ladder AND its
> originally-allocated work** — each experiment gets an explicit owning slice + DoD + gate (not floated
> again, not a separate precursor). This makes 0.8.11 a **large, multi-track release** (sizing tradeoff
> accepted, §8). The "$0 float rides the odd line" scheduling model is retired for these experiments.
>
> **Theme.** Two tracks converge on the evidence-and-artifact base the router needs. **Track E (eval
> spine)** discharges the experiment ladder end-to-end (`Gate-0/2 → EXP-A ‖ EXP-M4 → EXP-B′ → EXP-Fr-acc
> base → Fr-acc/VoI finalize → EXP-AF`) and then builds the **agent-side L2 router prototype + dispatcher
> pre-stage** on that evidence — the DP-A hedge that ships a router regardless of EXP-S's 0.8.12 outcome.
> **Track G (engineering)** ships **#17 filter-grammar** (G4↔G10 unification, IN-LIBRARY) + the **F-8b
> `record_feedback` governance** decision, independent of the eval spine.
>
> **Footprint.** Track E experiments are **EVAL-ONLY / CALLER-SIDE-BYO-LLM** (EXP-M4 has an
> **OFFLINE-BUILD/GPU** arm); the L2 prototype + dispatcher pre-stage are **CALLER-SIDE-BYO-LLM** (no
> engine changes); the filter-grammar (#17) is **IN-LIBRARY**. The library query path stays CPU-only,
> 1-bit/Hamming, deterministic throughout. Tag every technique.
>
> **Budget (HITL 2026-06-28).** Strict $0 is **no longer required** — the experiment ladder has a
> **~$20 total priced-LLM ceiling** for the release. Keep it low: most arms are $0 compute (Gate-0/2,
> EXP-A, EXP-M4 GPU, the classifier); the priced arms (EXP-B′ judge, EXP-Fr-acc, EXP-AF round-trips)
> draw on the ~$20 pool. Maintain a running `$` tally in `runs/STATUS-0.8.11.md`
> (`0.8.1-budget-discipline-cheap-validate-and-ledger`); cheap-validate each priced run before spend and
> use the resilient harness (`priced-runs-need-resilience-before-spend`). Flag to HITL if the ladder
> projects over ~$20.

---

## 1. Goal & scope

### 1a. Track E — the missed experiment ladder (now owned here; closes F-11)

These four were scheduled as 0.8.7/0.8.9 float and never ran (F-11). They are the prerequisites the
**original** 0.8.11 eval work (Fr-acc/VoI finalize, EXP-AF, the per-intent tuples) silently assumed.

- **Gate-0 / Gate-2 (foundation, $0 EVAL-ONLY).** **Gate-0** = golden-set **re-scope** (PSD §III.A):
  reuse existing LME/LOCOMO/AP-News/MuSiQue assets + the `decide_083`/`decide_084` rules; a **small,
  scoped gold-supporting-node labeling pass** only where a reused corpus lacks node-level retrieval
  labels for an intent class. **Gate-2** = the **oracle-routing upper bound** (PSD §III.B): exhaustive
  best-plan-per-query ceiling, reconciled with FathomDB's existing oracle bounds (recall@K_deep,
  per-feature CE numbers, the measured oracle +0.39 over Mem0), fed the measured per-arm cost tiers.
  **Out of scope here:** the ~269-Q F4/M6 corpus acquisition (EXP-D) stays at 0.8.17.

- **EXP-A — recall generation (`$0` EVAL-ONLY).** Wider candidate-generation recall lift for F2
  (recall@K_deep, gold-in-pool). Feeds EXP-B′ and the Gate-2 bound.

- **EXP-M4 — embedder-ceiling escape (`$0` / GPU; OFFLINE-BUILD/EVAL).** **Measure** the embedder
  ceiling (swap-candidate → re-whiten → eu7 re-clear → re-tune α; OD-7) on the 0.8.7 GPU seam. **Scope
  guard:** this is a **ceiling measurement**, not a productized swap — the standing decision is **keep
  CLS-corrected bge-small** (`0.8.3-mem0-parity-closed`); any productized embedder swap remains a
  separately-gated decision, **out of 0.8.11**. **KILL path:** if no candidate beats bge-small net of
  re-whiten/re-clear cost → register the ceiling and move on.

- **EXP-B′ — 3-stage joint tuning (`$0` + `$` judge; EVAL-ONLY).** The `candidate_k × pool_n × α ×
  final_K` joint optimization (PSD §II.C crux: α=1.0 @ pool_n=50 *drops* r@10 0.548→0.498). Hard-blocked
  by EXP-A ∧ EXP-M4. **Output = the per-intent `(index, retrieval, α, pool_n, MMR, recency)` config
  tuples** + the **EXP-B′.5 router-stack joint-regression guard** (a config for feature X must not
  regress feature Y). These tuples are what the dispatcher pre-stage registers (DP-B mitigation).

- **EXP-Fr-acc base (`$0` + small `$`; EVAL-ONLY).** The work that should have been 0.8.9: 5-class
  intent classifier accuracy over `{needle | multi_session | temporal | global | multi_hop}` + the
  **initial asymmetric mis-route cost matrix** (needle→`C` map-reduce = −0.362 + an LLM call). This is
  the **base** that the original-0.8.11 "finalize" (Slice 25) extends.

### 1b. The originally-allocated 0.8.11 work (unchanged in intent; now built on the real base)

- **EXP-Fr-acc/VoI finalize.** Extend the EXP-Fr-acc base (Slice 20) with PSD §III.C's three additions:
  (a) _value-of-signal_ — does agent relevance beat `ce_score`-alone on routed queries? (b) _ask-or-not
  VoI policy_ — at which (`ce_score`, route-margin) pairs does the agent round-trip break even? (c)
  _asymmetric weighting_ — does the policy preferentially suppress the high-cost needle→global cross-wire
  over cheap same-tier misses? These thresholds feed the prototype and the 0.8.15 dispatcher.

- **EXP-AF — agent-feedback value test.** Using the `record_feedback` telemetry pipeline (landed 0.8.8
  Slice 15; I-1 satisfied, F-6), measure whether an agent relevance signal beats `ce_score`-only net of
  round-trip cost on the **existing** substrate (no fresh 50–100-query rebuild); one-shot vs iterative
  within the 1–2 depth bound (PSD §II.C). **KILL path:** if the agent signal does not beat `ce_score`
  net of round-trip → drop the feedback loop from the prototype; router stays on internal `ce_score`.

- **Agent-side L2 router prototype + dispatcher pre-stage.** A CALLER-SIDE Python prototype (no engine
  changes) that routes the 5-class taxonomy using the EXP-B′ per-intent tuples; exposes a recommendation
  API (intent, stack, confidence, cost_tier, rationale) without executing; and accepts an agent hint /
  override. The **per-intent config tuple registry** is the 0.8.15 design artifact (DP-B). The prototype
  is the **DP-A hedge** — a working agent-side router ships regardless of EXP-S's 0.8.12 locus verdict.

- **#17 filter-grammar — G4↔G10 unification (gap-37, IN-LIBRARY).** Unify the G4 typed filter grammar
  (`Predicate { JsonPathEq, JsonPathCompare, ScalarValue }` on `read.list`) and the shipped G10
  `SearchFilter { source_type, kind, created_after, status }` on `search_filtered` into **one typed
  filter contract** serving both compilation paths (vec0 metadata pre-filter for G10; json-path-over-
  allowlist for G4). The shipped G10 surface is touched; re-express without behavior change. Py + TS SDK
  parity (X1). This is the typed-constraint surface the 0.8.15 router's `constraints` block leans on.

- **F-8b — `record_feedback` governance re-classification.** Per the 0.8.8 HITL ruling (F-8b), decide
  at Slice 0 and execute at Slice 40 whether `record_feedback` stays observability instrumentation or
  graduates to a **governed application command** (allowlist + Rust facade + X1 surface suites). Trigger
  criterion: does EXP-AF make it a load-bearing agent-facing input to an active-feedback loop?
  `enable_telemetry` / `last_telemetry_query_id` stay instrumentation unless EXP-AF compels otherwise.

_Why this is the right order:_ I-1 (EXP-OBS) is met (F-6, on `origin/main`). The eval spine discharges
the ladder in dependency order so each result is real before the next consumes it; the agent-side
prototype has no EXP-S dep → it builds here and hedges the 0.8.12 KILL path; filter-grammar (Track G)
is independent and runs concurrently from Slice 0.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

_Track by experiment tag / G-gap / OPP-id / TDD name. No invented AC ids; new ACs at gated slices only
per §6. Every Track-E experiment result is committed to **both** `dev/experiments-ledger.md` (closing
the F-11 "zero rows" gap) and `runs/STATUS-0.8.11.md`._

| ID | Requirement | Acceptance signal |
| --- | --- | --- |
| R-G0-1 | Gate-0 golden-set re-scope delivered | Slice 5: reused-asset map + decide_083/084 rule adoption + any scoped node-label pass recorded; ledger row written |
| R-G2-1 | Gate-2 oracle-routing upper bound delivered | Slice 5: best-plan-per-query ceiling + per-arm cost tiers, reconciled with existing oracle bounds; ledger row written |
| R-EXPA-1 | EXP-A recall-generation result registered | Slice 10: recall@K_deep / gold-in-pool lift for F2 with CI; ledger row written |
| R-M4-1 | EXP-M4 embedder-ceiling measured; swap KILL/GO recorded | Slice 10: ceiling vs bge-small with CI; **no productized swap**; ledger row + explicit "keep bge-small unless…" verdict |
| R-BP-1 | EXP-B′ joint-tuning result → per-intent config tuples | Slice 15: `(index, retrieval, α, pool_n, MMR, recency)` tuple per intent class + EXP-B′.5 joint-regression guard result; ledger row written |
| R-FRACC-1 | EXP-Fr-acc **base**: 5-class classifier accuracy + asymmetric mis-route cost matrix | Slice 20: per-class accuracy + per (intent, route) mis-route cost (incl. needle→C −0.362) with CI; ledger row written |
| R-FA-1 | VoI thresholds measured and registered | Slice 25: numeric (`ce_score`, route-margin) break-even points; committed to STATUS + sequencing §6 finding |
| R-FA-2 | Asymmetric mis-route cost matrix extended | Slice 25: matrix extended from the Slice-20 base; needle→global cross-wire cost confirmed measurable |
| R-FA-3 | Value-of-signal comparison complete | Slice 25: agent-signal vs `ce_score`-only lift on routed sample; CI-bounded |
| R-AF-1 | EXP-AF KILL/GO verdict + depth-bound decision | Slice 30 HITL readout: verdict in experiments ledger; one-shot vs iterative settled (1–2 bound) |
| R-AF-2 | Every priced run pre-registered before spend; ladder stays under the ~$20 ceiling | Slice 0: hypothesis + KILL path + per-experiment cost ceiling (EXP-B′, Fr-acc, EXP-AF) in STATUS; running `$` tally maintained; no priced run before registration; HITL flag if projected total > ~$20 |
| R-L2-1 | Agent-side L2 prototype routes all 5 intent classes | Slice 35: smoke test covers all 5; recommendation returned without execution |
| R-L2-2 | Prototype carries and exposes per-intent config tuples | Slice 35: each class has a registered EXP-B′ tuple; tuple registry is the 0.8.15 pre-stage artifact |
| R-L2-3 | Prototype accepts agent hint / override without error | Slice 35: override respected without fallback-to-internal route |
| R-L2-4 | Prototype footprint is CALLER-SIDE; no engine changes | Slice 35 codex §9: zero changes to `fathomdb-engine` / `fathomdb-py`; prototype lives outside the library |
| R-FIL-1 | One unified typed filter contract covers G4 and G10 paths | Slice 40: single type serves both `read.list` (json-path) and `search_filtered` (vec0 metadata); parity test on a shared fixture |
| R-FIL-2 | Shipped G10 paths re-expressed on the new contract without behavior change | Slice 40: G10 parity test RED-first → GREEN; no regression on existing `SearchFilter` suite |
| R-GOV-1 | F-8b classification decision committed and executed | Slice 0 HITL decision recorded; Slice 40 execution: if promoted → allowlist + Rust facade + X1; if kept → ADR note with evidence |
| R-GOV-2 | Governed surface count + X1 suites updated consistently | Slice 40: if promoted, `test_surface.py` + `surface.test.ts` updated; if not, no allowlist change |
| R-X-1 | Py + TS SDK parity on the unified filter contract (and `record_feedback` if promoted) | X1 harness green; `test_telemetry_parity.py` + `surface.test.ts` updated |
| R-LEDGER-1 | F-11 closed: every Track-E experiment has a ledger row + reproducible script | Slice 45: `dev/experiments-ledger.md` carries Gate-0/2, EXP-A, EXP-M4, EXP-B′, EXP-Fr-acc, EXP-AF rows (no longer zero) |

New ACs: candidates at Slice 0 (filter-grammar contract) and at the EXP-AF/Fr-acc readout (if the
agent-signal result warrants a new gate). HITL decides.

**HITL decision points (four):**

1. **F-8b** (Slice 0) — `record_feedback` instrumentation → governed command? Before Slice 40 execution.
2. **EXP-M4 swap KILL/GO** (Slice 10 readout) — does any candidate beat bge-small? Default = keep
   bge-small; a productized swap is out of 0.8.11 and separately gated.
3. **EXP-AF KILL/GO** (Slice 30 readout) — agent signal beats `ce_score`-only net of round-trip? If
   KILL: prototype drops the feedback arm; `record_feedback` stays instrumentation (overrides F-8b promote).
4. **Filter-grammar unification shape** (Slice 0 ADR) — unified single type vs thin adapter over two
   compilation paths (vec0 metadata columns for G10; json-extract over allowlist for G4).

Owner of all four: steward.

---

## 3. Slice ladder (mod-5)

```text
Track E (eval spine — sequential):   0 → 5 → 10 → 15 → 20 → 25 → 30 → 35
Track G (engineering — ∥ from 0):                                   40
Verification:                                                          45
```

| Slice | Title | Track | Work-type | Footprint | Depends-on |
| ---: | --- | :---: | --- | --- | --- |
| **0** | Setup + ADRs — pre-register the **whole** ladder (Gate-0/2, EXP-A, EXP-M4, EXP-B′, EXP-Fr-acc, EXP-AF: hypothesis/KILL/cost-ceiling each); filter-grammar unification ADR; F-8b decision; L2 prototype scope; stand up `runs/STATUS-0.8.11.md` | — | design-adr | — | — |
| **5** | **Gate-0 + Gate-2 (eval FOUNDATION)** — golden-set re-scope + oracle-routing upper bound; reconcile with existing oracle bounds; per-arm cost tiers | E | eval ($0 EVAL-ONLY) | EVAL-ONLY | 0 |
| **10** | **EXP-A ‖ EXP-M4** — recall generation (F2) ‖ embedder-ceiling measurement (GPU); EXP-M4 swap KILL/GO readout (keep bge-small unless beaten) | E | eval ($0 / GPU) | EVAL-ONLY / OFFLINE-BUILD | 5 |
| **15** | **EXP-B′ — 3-stage joint tuning (KEYSTONE)** — `candidate_k × pool_n × α × final_K`; per-intent config tuples + EXP-B′.5 joint-regression guard; HITL-gated `$` judge | E | eval ($0 + $ judge) | EVAL-ONLY | 10 (EXP-A ∧ EXP-M4) |
| **20** | **EXP-Fr-acc base** — 5-class classifier accuracy + asymmetric mis-route cost matrix (the missed 0.8.9 work) | E | eval ($0 + small $) | EVAL-ONLY / CALLER-SIDE-BYO-LLM | 5 |
| **25** | **EXP-Fr-acc/VoI finalize** — value-of-signal + ask-or-not VoI thresholds + asymmetric-weighting extension; uses EXP-OBS telemetry (I-1) | E | eval ($0 + small $) | EVAL-ONLY / CALLER-SIDE-BYO-LLM | 20 |
| **30** | **EXP-AF value test** — agent relevance signal vs `ce_score`-only; round-trip VoI break-even; one-shot vs iterative; KILL/GO → HITL readout | E | eval ($0 + small $) | EVAL-ONLY / CALLER-SIDE-BYO-LLM | 25 |
| **35** | **Agent-side L2 router prototype + dispatcher pre-stage** — Python CALLER-SIDE; 5-class taxonomy; per-intent tuple registry (from EXP-B′); recommendation API; hint/override; pre-stage artifact committed | E | implementation (CALLER-SIDE) | CALLER-SIDE-BYO-LLM | 15 (tuples) ∧ 25 (VoI) ∧ 30 (AF verdict) |
| **40** | **#17 filter-grammar unification + F-8b execution** — unified typed filter contract on `read.list` (G4) + `search_filtered` (G10); RED→GREEN parity; re-express shipped G10; execute F-8b; Py + TS parity (X1). **∥ from Slice 0 — independent of the eval spine** | G | implementation (IN-LIBRARY + governance) | IN-LIBRARY | 0 (NOT gated by 5–35) |
| **45** | **Verification + Release Readiness (0.8.11)** — X1/X2/X3 + full AC gate; experiments-ledger carries all Track-E rows (F-11 closed); dispatcher pre-stage artifact reviewed; hand-off note for 0.8.15 | — | verification | — | 5,10,15,20,25,30,35,40 |

**Keystones / hard gates.**

- **Slice 15 (EXP-B′) is the eval keystone** — its per-intent tuples are the dispatcher pre-stage and
  the L2 prototype's routing table. Hard-blocked by EXP-A ∧ EXP-M4 (Slice 10).
- **Slice 20 (EXP-Fr-acc base) hard-gates Slice 25** — there is no VoI "finalize" without the classifier
  accuracy + mis-route base. (This is the edge the old plan got wrong — F-11.)
- **Slice 30 (EXP-AF KILL/GO) hard-gates the L2 prototype's feedback arm** — if KILL, the prototype drops
  the agent-signal loop before Slice 45 (reserved-gap patch if needed).
- **F-8b HITL decision (Slice 0) gates Slice 40's governance execution.**
- **All priced eval runs use the resilient harness** (incremental checkpoint, `--resume`, 429/5xx
  backoff, completeness guard, `$` ledger) — `priced-runs-need-resilience-before-spend`. EXP-B′ /
  EXP-Fr-acc / EXP-AF are the priced ones; pre-register each (R-AF-2) before any spend and draw on the
  shared **~$20 ceiling** (Budget, above) with a running tally — flag HITL before exceeding it.

**Tracks (parallelizable after Slice 0).**

- **Track E (eval spine):** strictly sequential `5 → 10 → 15 → 20 → 25 → 30 → 35` (each result feeds the
  next). Slice 20 (Fr-acc base) and Slices 10/15 (A/M4/B′) both branch off Slice 5 and can overlap until
  they converge at Slice 25/35.
- **Track G (engineering):** Slice 40 (filter-grammar + F-8b) is independent — it unblocks at Slice 0
  and runs concurrently with the entire eval spine; its number reflects ladder position, not a dependency
  on 5–35.

---

## 4. Reserved-gap policy

Carried unchanged from `0.8.1-plan.md §Numbering`. Likely insertion points this release:

- If the Slice 0 filter-grammar ADR reveals the two compilation paths cannot share a single type cleanly,
  a reserved-gap slice is inserted between 40 and 45 (separate migration tests), off a fresh `main`.
- If **EXP-M4** surfaces a swap candidate that beats bge-small, that does **not** trigger in-release
  productization (out of scope, §1a) — it is registered and escalated to HITL as a separate gated decision.
- If **EXP-AF KILL** triggers a prototype feedback-arm removal, a reserved-gap patch slice handles the
  delta before Slice 45.
- Every reserved-gap slice is fully orchestrated off a fresh `main` baseline — never an ad-hoc patch.

---

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + harnesses (Py↔TS) · X2 `mkdocs build --strict` green · X3 docs + DOC-INDEX entry per
slice. `runs/STATUS-0.8.11.md` carries the per-slice X column. For the eval slices (5/10/15/20/25/30) the
"shippable" DoD is the **landed result doc + reproducible script + a row in `dev/experiments-ledger.md`**
(not a published artifact); X1/X2/X3 still apply to any surface change those slices incidentally touch.

The filter-grammar slice (40) triggers a DOC-INDEX update for: `docs/reference/python-api.md`,
`docs/reference/typescript-api.md`, `docs/guides/hybrid-search-filtering.md`, and
`src/conformance/governed-surface-allowlist.json` (if F-8b promotes).

---

## 6. Acceptance-criteria policy

`dev/acceptance.md` is locked (max AC-074); do not invent AC ids. Track by:

- Experiment tags (`Gate-0`, `Gate-2`, `EXP-A`, `EXP-M4`, `EXP-B′`, `EXP-Fr-acc`, `EXP-AF`), OPP-id
  (`OPP-9` telemetry/real-gold), G-gap (`G4`, `G10`, gap-37), and TDD names (`test_gold_pipeline.py`,
  `test_surface.py`, `surface.test.ts`, `test_telemetry_parity.py`).
- New ACs (if warranted) proposed at Slice 0 (filter-grammar contract) or the EXP-AF/Fr-acc readout.
  HITL decides; none are pre-issued here.

---

## 7. Prerequisites

1. **I-1 satisfied (verified on `origin/main`).** EXP-OBS landed at 0.8.8 Slices 0/1/5 (commits
   `170c5109`, `8c938bb7`, `eec4ddb0`, `5c7b9f31`); `Explanation`/`QueryTrace`/`PerHitExplain` exist on
   main. The per-arm provenance + score breakdown the agent-signal track judges against physically exists.
2. **Real-gold pipeline operational (OPP-9 / 0.8.8 Slice 15).** `record_feedback` appending to a local
   JSONL sink; `enable_telemetry` + `last_telemetry_query_id` tested in `test_gold_pipeline.py`. EXP-AF
   relies on real-gold rows; if the local sink lacks a sufficient sample, a short cheap run at Slice 5/20
   generates it (gated by R-AF-2 pre-registration).
3. **0.8.7 GPU seam present.** `parse_device_request` / `resolve_device()` shipped (0.8.7, `02297cb3`);
   EXP-M4's GPU sweeps ride it. EXP-0 CE-rerank α/pool_n knobs are in-engine (landed 2026-06-25) — the
   EXP-B′ joint-tuning surface.
4. **~~0.8.9 EXP-Fr-acc base complete — FALSE (F-11).~~** **CORRECTED:** the EXP-Fr-acc base, plus
   Gate-0/2, EXP-A, EXP-M4, and EXP-B′, were scheduled as 0.8.7/0.8.9 float and **never ran** (zero
   ledger rows, no `runs/` artifacts). **0.8.11 now OWNS building all of them** (Track E, Slices 5–20).
   This is the change that grew the release (F-11 / HITL 2026-06-28).
5. **EXP-S is NOT a 0.8.11 prerequisite.** EXP-S (kind-tag index substrate) is 0.8.12; EXP-B′'s hard
   blockers are EXP-A ∧ EXP-M4 only. The agent-side L2 prototype is deliberately substrate-free (DP-A).
6. **Worktrees off `$(git rev-parse main)`.** Every implementation slice uses a pre-created worktree off
   a fresh `main` baseline; no `maturin develop` from a worktree (`agent-worktree-stale-base-trap`).
7. **0.8.10 not a gate.** 0.8.11 is OOB; no I-5 edge to it.

**What 0.8.11 hands to 0.8.15 (the dispatcher build) — now backed by real experiments:**

| Artifact | Consumer at 0.8.15 |
| --- | --- |
| Gate-2 oracle ceiling + per-arm cost tiers | EXP-Fr routing-value justification |
| EXP-A recall + EXP-M4 ceiling | the recall/embedder envelope the stacks tune within |
| EXP-B′ per-intent config tuples + joint-regression guard | EXP-Fr integrates rather than invents (DP-B); forbidden-composition validator |
| EXP-Fr-acc accuracy + asymmetric mis-route matrix | EXP-Fr classifier-accuracy gate + escalation logic |
| Fr-acc/VoI thresholds (`ce_score`, route-margin break-even) | EXP-Fr ask-or-not escalation |
| EXP-AF KILL/GO verdict + depth bound (1 vs 2) | whether 0.8.15 integrates the agent-signal loop |
| Agent-side L2 prototype (working code) | if EXP-S KILL at 0.8.12, this _is_ the 0.8.15 router (DP-A hedge) |
| Unified filter contract (#17, IN-LIBRARY) | the `constraints` block the dispatcher leans on |
| F-8b classification (committed) | whether `record_feedback` is governed before wiring it |

---

## 8. Dependencies / sequencing

**Upstream (must be done before 0.8.11 can start):**

- I-1 edge: EXP-OBS (@0.8.8) — **satisfied** (F-6).
- ~~0.8.9 EXP-Fr-acc base~~ — **NOT satisfied (F-11)**; absorbed into Track E here.

**Within-release ordering (hard):**

1. Slice 0 before all others (pre-registration gates every priced run; ADRs gate Slice 40 + Slice 5 scope).
2. Track E is strictly sequential `5 → 10 → 15 → 20 → 25 → 30 → 35`.
3. Slice 0 F-8b HITL decision before Slice 40 execution.
4. All of 5/10/15/20/25/30/35/40 before Slice 45.

**Parallelizable:**

- Track G (Slice 40, filter-grammar + F-8b) runs concurrently with the entire eval spine after Slice 0.
- Within Track E, the Fr-acc base (Slice 20) branch and the A/M4/B′ branch both start off Slice 5 and
  can overlap until Slice 25/35.

**Downstream (0.8.11 must complete before these consume its artifacts):**

- **0.8.12 EXP-S readout** uses the Fr-acc/VoI thresholds + L2 prototype for the formal locus decision
  (agent-side vs in-library). 0.8.11 supplies the evidence; the locus call is made at 0.8.12.
- **0.8.15 EXP-Fr** is hard-gated on EXP-B′ ∧ EXP-S ∧ Fr-acc ∧ EXP-OBS — **0.8.11 now closes EXP-B′,
  Fr-acc, the pre-stage, and (with 0.8.8) EXP-OBS**; only EXP-S (0.8.12) remains. _This is the broader
  impact of F-11: before this release, the 0.8.15 dispatcher rested on experiments that had never run._
- **0.8.17 router hardening** builds on the 0.8.15 dispatcher + EXP-AF productization (GO → productize
  the agent-signal loop; KILL → skip that arm).

**Interlock with master sequencing (F-11 / F-10):**

0.8.11 is a net-new OOB slot (F-10). The odd line skips 13. **F-11 ruling:** the missed experiment ladder
is discharged here as owned, gated slices; the "$0 float rides the odd line" model is retired for these
experiments. The master §4 0.8.11 row + §5b by-when table + the F-11 disposition are reconciled by the
steward (this plan is the owning artifact).

**Size flag — HONEST.** This release is **no longer "well-sized"** — folding the full experiment ladder
into 0.8.11 (HITL ruling on F-11) makes it the **largest release in the 0.8.x line**: five owned
experiments (Gate-0/2, EXP-A, EXP-M4, EXP-B′, EXP-Fr-acc base) + three original eval workstreams
(Fr-acc/VoI finalize, EXP-AF, L2 prototype) + one IN-LIBRARY engine slice (filter-grammar) + a governance
action — ~10 slices across two tracks. **The tradeoff was accepted deliberately:** these are low-cost
experiments (a **~$20** priced-LLM ceiling for the whole ladder; the failure was ownership, not cost),
discharging them in one owned release is cheaper than re-floating them, and it de-risks the entire 0.8.15
router track in a single place. **Scope-creep
hotspots to watch:** (a) EXP-M4 — keep it a *measurement*; a productized embedder swap is out of scope
and separately gated; (b) Gate-0 — keep the gold-labeling pass *scoped to gaps*, not a fresh golden set
(the ~269-Q F4 build stays at 0.8.17); (c) filter-grammar — if the two compilation paths can't share one
type, a reserved-gap slice is inserted (§4). Flag any of these to HITL if they grow.

---

## 9. Immediate next slice

**Slice 0 — ADRs + full ladder pre-registration + HITL F-8b.** Stand up `runs/STATUS-0.8.11.md`. In parallel:

1. **Pre-register the whole Track-E ladder** (R-AF-2, generalized): for each of Gate-0/2, EXP-A, EXP-M4,
   EXP-B′, EXP-Fr-acc, EXP-AF — hypothesis, KILL criteria, corpus/assets, cost ceiling (`$0` vs priced),
   and the reproducible script. **Block every priced run** (EXP-B′ judge, Fr-acc, EXP-AF) until its
   registration is in `STATUS-0.8.11.md`. Read PSD §III + the master §5b by-when table as inputs.

2. **Author the filter-grammar unification ADR** (#17, Track G): settle whether the unified `Filter`
   type is a single enum routing internally to the two compilation paths, or a thin adapter over two
   distinct types. Inputs: `dev/adr/ADR-0.8.0-filter-grammar.md`, `dev/design/slice-10-design.md`. State
   (a) the new type's shape/name, (b) vec0-metadata (G10) vs json-extract (G4) compilation, (c) the
   parity-test fixture, (d) whether `SearchFilter` is renamed/extended or replaced. Any `[TBD]` blocks Slice 40.

3. **Gate-0 re-scope plan**: enumerate the reusable LME/LOCOMO/AP-News/MuSiQue assets + the applicable
   `decide_083`/`decide_084` rules; identify exactly which intent classes lack FathomDB-node-level
   retrieval labels (→ the scoped labeling pass). Confirm the ~269-Q F4/M6 build is **excluded** (0.8.17).

4. **EXP-B′ / Fr-acc / EXP-AF extension design**: specify the per-intent tuple output format (feeds the
   L2 prototype + 0.8.15 pre-stage), the EXP-B′.5 joint-regression guard, the three Fr-acc/VoI additions
   (value-of-signal, VoI break-even, asymmetric weighting), and the EXP-AF comparison method.

5. **L2 prototype scope spec**: confirm CALLER-SIDE Python harness (no crate changes); recommendation API
   shape per `initial-arch` §5 (intent, stack, confidence, cost_tier, rationale); which tuples are pinned
   from EXP-0 vs to-be-filled from EXP-B′.

6. **F-8b HITL decision**: bring the reclassification checklist from
   `dev/plans/runs/NOTE-0.8.8-to-orchestrator-record-feedback-reclassify.md` to HITL; record in
   `STATUS-0.8.11.md` before Slice 40 begins.

After Slice 0, fan out: **Track E** runs the sequential eval spine (5 → 10 → 15 → 20 → 25 → 30 → 35);
**Track G** runs Slice 40 (filter-grammar + F-8b) concurrently. Converge at Slice 45.
