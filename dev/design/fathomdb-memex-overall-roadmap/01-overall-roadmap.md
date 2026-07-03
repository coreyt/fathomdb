---
title: Overall unified FathomDB + Memex roadmap
date: 2026-07-03
status: PROPOSAL — awaiting HITL review (session "fathom-2MM-plan-refactor-0.8.x")
desc: The single sequenced cross-product roadmap: open gates, unified sequence, priority tiers, misalignment resolutions, API-surface discipline, risks.
generator: model-tiered dynamic workflow (Sonnet-5 collect · Fable-5 analyze/critique/synthesize · Opus align); 11 agents, 0 errors
---

# UNIFIED FATHOMDB + MEMEX ROADMAP — PLAN OF RECORD PROPOSAL

**Status: PROPOSAL FOR HITL. Nothing in this document is approved, scheduled, or committed until HITL signs the relevant gate. All Memex-side actions are PROPOSALS relayed via the liaison channel — never auto-applied (push scope: fathomdb repo only).**

Date: 2026-07-02 · Inputs: five workstream digests, priority+misalignment analysis (git-verified), adversarial critique (top-3 must-fixes incorporated: gpu-rerank orphan, Cause-A probe-then-retire, cutover-risk ownership).

---

## EXECUTIVE SUMMARY

**Joint thesis.** FathomDB and Memex are one product system with two repos: FathomDB owns mechanism (indexes, lifecycle, ranking substrate), Memex owns judgment (semantics, policy, LLM). The 0.8.x line's remaining value is concentrated in one long pole (0.8.14 EXP-S) that everything downstream rides, while Memex's remaining value is concentrated in *consuming what has already shipped* (0.5.1 Phases 5–6) and in evidence gates (#31 QID probe) that decide where joint eval investment goes. The portfolio's biggest inefficiencies right now are not missing builds — they are stale cross-repo state (dependencies already met that nobody consumed), unowned decisions (2MM premise, gpu-rerank home), and one mechanism scheduled twice (Cause-A / OPP-12 identity).

**The 2–3 decisions that unlock everything:**

1. **The 2MM premise + minimal E1, decided BEFORE 0.8.14 Slice 0.** Is 1–2M chunks a near-term consumer target? This forks the entire vector-latency track. Default on non-answer = **NO** (published roadmap stands: ANN stays 2.x/F-16, 0.8.20 stays deps-only, CPU-only/1-bit/deterministic invariant untouched). A minimal $0 E1 overhead-attribution bench is banked either way so a later YES costs only a slot, not lost measurement.
2. **OPP-12 ratification close-out (seq-12) + OPP-11 final SIGN.** Two one-message governance closes that freeze the lifecycle contract and the adjudicator framework every adoption gate depends on. Ratification schedules **nothing** — the build (~90% net-new) gets its own ≥0.9.x scheduling call.
3. **The gpu-rerank re-homing call.** A Memex-expected deliverable (`ce_blend_enabled` flip, `embed_batch_cls`, branch `0.8.14-gpu-rerank`, rebased/green, unmerged) currently has **no home in any plan** — plan-0.8.14 contains zero rerank scope and F-1 forbids adding it silently. HITL must either admit the merge as explicit 0.8.14 scope or re-home it (0.8.16 is the natural fit: ranking signal & embedder reach) and tell Memex which.

**Single most important sequencing insight.** *Decision inputs are sequenced backwards from their decision points, and two of them are inverted today:* E1's natural substrate (`scale.rs`) sits at 0.8.19 — end of line — while E1's answer is needed before 0.8.14 (the earliest cheap landing spot for the kernel). And Memex 0.5.1's "blocker" (0.8.11.2 merge, A-1) has been on origin/main since the merge — verified this pass — meaning the joint product's proof point is stalled on a delivery signal that was never sent, not on missing work. Fix both by decoupling: extract a minimal E1 now (no slot), and send the re-pin liaison proposal now (no build).

---

## OPEN GATES

Premises/ratifications HITL must settle before dependent work proceeds. Ordered by decision deadline.

| Gate | Question | Owner | Deadline | Unblocks | Default on non-answer |
|---|---|---|---|---|---|
| **G-1: 2MM premise** | Is 1–2M chunks near-term for Memex/Hermes/OpenClaw? Inputs missing: consumer corpus-size projections (scout — cheap, in-repo/public) + E-A2 filter-rate telemetry (**unowned instrumentation** — needs a named Memex owner/slot via liaison PROPOSAL, or HITL explicitly accepts deciding on projections alone) | HITL | Before 0.8.14 Slice 0 | Kernel-slice admission decision (M2); `ann-index-vec0.md` reconciling edit; 0.8.20 Slice-0 agenda item | **NO** — published roadmap stands; bank E1 anyway |
| **G-2: E1 overhead attribution** | Is the O(N) scan vec0-overhead-bound (Path A viable) or bandwidth/compute-bound? | FathomDB (minimal standalone bench, $0, no release slot; absorbed later by 0.8.19 `scale.rs`) | Before any kernel build; ideally before 0.8.14 Slice 0 | A>C>B ranking confirmation; the entire Path-A case. Until run, A>C>B is a *prediction*, not a fact | No kernel work anywhere |
| **G-3: OPP-12 ratification** | Memex posts `agree` at seq-12 (or objections) + applies prepared prose-ledger text | Memex HITL (liaison PROPOSAL) | ASAP — one message | M7 verb consolidation; C-1 registration-flow checkpoint; the ≥0.9.x scheduling call; §14 purge timing re-affirmation | OPP-12 stays PENDING; **nothing schedules against it** (M5 rule); §14 purge commitment stands on its own |
| **G-4: #31 QID entity-linking re-probe** | Non-zero cross-source edge-density / giant-component after QID linking? | Memex 0.5.5-B (in flight) | Before any bench build spend | Bench slices B→C→D; #30 corpus utility; enrichment-location decision (fathomdb-durable vs memex-ephemeral) | No bench build — correctly evidence-gated |
| **G-5: gpu-rerank home** | Admit `0.8.14-gpu-rerank` merge as explicit 0.8.14 scope, or re-home to 0.8.16? | HITL | 0.8.14 Slice 0 | Memex CE-blend adoption path (their retrieval-quality plan homes it at "0.8.14"); `embed_batch_cls` availability | Branch stays parked; Memex must be told its expectation is unhomed |
| **G-6: Cause-A Stage-1 sufficiency probe** | Does shipped `stable_id` (0.8.11.2, on origin/main) suffice for OPP-11 hit-level data-fitness? | Memex probe (liaison PROPOSAL, small) | Before retiring the Cause-A pico | PASS → retire the standalone pico as discharged; FAIL → pico stays the live interim vehicle (per OPP-11's size-it-first resolution) | Pico stays live — **probe-then-retire, never retire-then-probe** |
| **Cheap closes (do with G-3):** OPP-11 final HITL SIGN (all axes resolved, feeds every adoption gate); F-8b `record_feedback` reclassification resolved before 0.8.15 Slice 15 (also DP-0.17-B) | | | | | |

---

## THE UNIFIED SEQUENCE

Governance kept intact throughout: FathomDB even=publishable w/HITL, odd=not, pico=label-only, '13' forbidden; HNSW/ANN = 2.x (F-16); scale gate rides F-17 (0.9.0 soft → 0.9.3 stated → 1.0.0 exit-beta → 1.1.0 hard); shipped query path stays CPU-only/1-bit/deterministic. Memex even micro = required/critical-path, odd = OOB.

### Phase 0 — NOW (all parallel; no build dependencies among them)

| Lane | Action |
|---|---|
| FathomDB build | **Start 0.8.14 as specified** (EXP-S keystone + BM25F/F5, single `SCHEMA_VERSION` bump, #17 already struck). No kernel scope unless G-1+G-2 clear by Slice 0. |
| FathomDB measure | **Minimal E1 bench** — $0, scratch/label-only, on existing `ir_recall_eval` + eval-layer substrate. Marked "absorbed by 0.8.19 #13" so it never becomes a second harness. |
| Scout | Consumer corpus-size projections (Memex in-repo; Hermes/OpenClaw public) → G-1 input package for HITL. |
| Liaison → Memex HITL (PROPOSALS, one bundle) | (a) OPP-12 seq-12 `agree` ask + prose-ledger text apply (G-3). (b) **Re-pin unblock**: fathomdb origin/main ≥ `1137c572` is live — re-pin editable build, run capability probes for A-1 (`$.action_kind`, correct hash on main = `9a46611b`), **A-2 (bool-eq server-executable in `read.list`)**, and `stable_id`; flip A-1 server-side; wire `stable_id`. (c) Cause-A Stage-1 sufficiency probe (G-6). (d) E-A2 filter-rate telemetry ask with named-owner requirement (G-1). (e) Note: gpu-rerank home is **undecided** (G-5) — do not plan CE-blend adoption against "0.8.14" until FathomDB answers. (f) Corpus-schema/WEC-Eng origin visibility CONFIRMED — #30/#31 may pin `manifest_sha256` against origin/main now. |
| Docs-only reconciliation (commit straight to main per "don't gate trivial changes on CI") | M1 (strike ANN from 2MM roadmap §5 table), M10 (plan-0.8.19 stale prereq wording), M18 (F-11a rename + add edge I-7: 0.8.16 Slice-15 Δ → 0.8.18 #5 calibration, physically hard), 0.8.20 Slice-0 agenda addition (M11), `tests/corpus/README.md` 6→8 source_types, **annotate memory `1-2M-scaling-kernel-first-not-ann.md`: "A>C>B conditional on E1 + premise, both OPEN"**. Do NOT edit `ann-index-vec0.md` until G-1 resolves. |
| Memex (PROPOSALS) | 0.5.5-B #31 QID probe continues (in flight, THE gate). 0.5.4 partial kickoff per M12: gauntlet groups not touching the 4 open round-3 gates launch; rest wait for gate ratification. 0.5.1 Phase 5–6 resume after re-pin. |

### Phase 1 — FathomDB 0.8.14 window

- **0.8.14 ships EXP-S + F5.** If G-1=YES ∧ G-2=overhead-bound: HITL may admit exactly ONE kernel slice, admissible **only if** the packed-code store is a derived/rebuildable structure (like FTS/vec0) with **no `SCHEMA_VERSION` coupling** — that is what makes it genuinely droppable at Slice 20 without violating the one-bump commitment. IVF spike (E2/E3) goes to the 2.x contingency track regardless — never into 0.8.14.
- **Memex 0.5.1 Phases 5–6 close** (consume + behavioral-equivalence gate B-1), *contingent on* the cutover-risk verify-or-schedule item (M20 below): the five unresolved substrate risks — Python-SDK production-grade, **aarch64/Jetson**, append_only_log at scale, **single-writer vs Memex's TUI+service two-process architecture**, join-query expressiveness — must each be cited-as-resolved or placed on the next FathomDB↔Memex sync agenda **before B-1 closes**. Single-writer/two-process is architectural: it can invalidate the cutover destination itself.
- Memex 0.5.4 completes gates + gauntlet. → hard-unblocks 0.5.5-A (adopt the tail-INDEX hard reading; state precisely in plan-0.5.5 — **PROPOSAL to Memex HITL**).
- Caveat carried on all value-test claims: 0.5.2's harness is RETRIEVE-only by default (full-loop seam is a stub; `data_fitness` not on all reflect variants) — engine-fitness A/B verdicts are retrieval-only until that debt is paid.

**Cross-product arrow:** 0.8.14 EXP-S → 0.8.15 dispatcher (I-2, one-for-one slip — the program's hardest edge).

### Phase 2 — FathomDB 0.8.15 window

- **0.8.15 dispatcher/router.** Slice-0 agenda (all pre-staged): locus decision; route-accuracy AC from EXP-Fr-acc; **F-8b resolution** (blocks Slice 15); explicit disposition of §10–14 adjacencies — which (if any) ride this window vs move under OPP-12 (M7), so Memex stops version-label-gating on "~0.8.15."
- **Memex 0.5.3.1 gate re-keyed** from "FathomDB ~0.8.15" to named capability probes (read.state present; FTS-drift counterpart scheduled) — PROPOSAL to Memex HITL. The FTS half is **measurement-only** (M15): the 0.8.x extension door is closed (R-I4); a proven unrecoverable miss produces a ≥0.9.x ask via HITL, nothing sooner.
- OPP-3 `margin` lands here (V-7) → cascade re-measurement can proceed on real turns.
- Memex 0.5.5-A (entity registry) runs; **binding checkpoint (M14):** before R-A freezes the persisted `EntityTypeSpec` schema, read-only review against OPP-12's `ProjectionSpec` field set (no build dependency; HITL breaking-waiver bounds the downside).

### Phase 3 — FathomDB 0.8.16 → 0.8.18 → 0.8.17 → 0.8.19 → 0.8.20

Strictly ordered where arrows exist; explicitly out-of-numeric-order where documented:

- **0.8.16** (F9 + ONNX; + gpu-rerank if G-5 re-homed it here). Slice-15 candle↔ONNX Δ → **hard input** (new edge I-7) → 0.8.18 #5 tolerance calibration. F9 also completes OPP-12's `rankable` signal algebra (contract input, not a build trigger).
- **0.8.18** (publish pipeline + vector-equivalence; release-engineering GA ONLY — F-17: no scale claim). Real `v*` tag = separate HITL.
- **0.8.17** runs **after 0.8.18** (documented inversion — its Slice-40 dry-run needs the publish pipeline). Track C stays corpus/spend-gated, non-blocking.
- **0.8.19** (EXP-FT ladder + #13 harness; absorbs the minimal E1 bench). FT productization = post-0.8.19 steward-elected slot, per plan.
- **0.8.20** Library Sweep — deferred-with-trigger, Slice-0 HITL gate; agenda now includes: even/OOB classification (F-12 reconcile), publish-vs-label, equivalence bar, **and (new)** "if G-1=YES, the sqlite-vec 0.1.7→0.1.9 migration is a predecessor of any vec0-internal kernel work."
- OPP-1/3/6 experiment ladders run in strict V-1→V-7 order throughout — no pull-forward, no pre-built abstraction (OPP-1's anti-abstraction stance holds).

### Phase 4 — ≥0.9.x / 2.x horizon

- **OPP-12 build**: coordinated breaking FathomDB-0.9.x ↔ Memex-0.5.x pair; dedicated steward/HITL scheduling call with build-cost estimate (design is CONVERGED; build is ~90% net-new — the README gets an explicit "ratified ≠ scheduled ≠ cheap" line, M6). Cause-A **Stage 2** (typed `{space,value}` SearchHit.id subsumption + anonymous-node surrogate minting) rides this pair — one mechanism, never two work items.
- **ANN/IVF stays 2.x (F-16).** Reopens early ONLY via the written escalation path: G-1=YES ∧ E1 shows not-overhead-bound ∧ kernel can't hit budget on global/unscoped queries.
- F-17 scale ladder: 0.9.0 soft → 0.9.3 stated → 1.0.0 exit-beta → 1.1.0 hard. Untouched by everything above.

---

## PRIORITY TIERS

**P0 — do now / next-up (blocks the most, or is one message away):**

- FathomDB **0.8.14** — the long pole; everything on I-2 rides it. No silent scope adds (F-1).
- **G-1+G-2 decision package** (scout + minimal E1 + HITL premise call) — forks the vector roadmap; must beat 0.8.14 Slice 0 to keep the cheap landing option.
- **G-5 gpu-rerank re-homing** — a consumer-expected deliverable with no plan home; second unacknowledged F-1 collision if left ambiguous.
- **Memex 0.5.1 Phase 5–6 via re-pin** (PROPOSAL) — stalled on an already-met dependency; the joint product's proof point.
- **G-3 + OPP-11 SIGN + G-6 probe** — three cheap governance/probe closes with portfolio-wide fan-out.
- **Memex 0.5.5-B #31** — in flight; gates all cross-source bench spend.

**P1 — next in line (real value, properly gated):**

- FathomDB **0.8.15** dispatcher (after 0.8.14; Slice-0 agenda includes F-8b + adjacency disposition).
- Memex **0.5.4** completion (partial-launch now, round-3 gates before the rest) → hard-unblocks 0.5.5-A.
- **M20 cutover-risk verify-or-schedule** — must land before 0.5.1 B-1 closes; owns the five orphaned substrate risks.
- FathomDB **0.8.16** (F9 + ONNX; I-7 feed-forward to 0.8.18).
- **M7 verb consolidation** — executes only after G-3 ratifies (see API-Surface Discipline).
- Memex **0.5.5-A** entity registry (with the M14 checkpoint).

**P2 — sequenced later / correctly deferred:**

- FathomDB **0.8.18 → 0.8.17 → 0.8.19** (order per documented dependencies), **0.8.20** (trigger-gated; do not migrate for novelty).
- Memex **0.5.3.1** (capability-probe-gated, measurement-only FTS scope).
- **OPP-1/3/6 ladders** (V-1→V-7; OPP-3 paused below its own 0.70 AUROC bar pending `margin`).
- **OPP-12 build placement call** (≥0.9.x) and **ANN/2.x contingency** — deliberately last; design-complete ≠ build-scheduled.

---

## MISALIGNMENT RESOLUTIONS

| ID | Resolution verb | Net effect on the sequence |
|---|---|---|
| M1 | RECONCILE (docs-only) | None — removes a phantom 0.8.20 collision; escalation path retained in the brief |
| M2 | GATE-ON-EVIDENCE | Kernel enters 0.8.14 only if G-1+G-2 clear by Slice 0 **and** packed store has zero `SCHEMA_VERSION` coupling (real droppability); else later standalone even micro |
| M3 | SPLIT | Minimal E1 runs now, no slot; 0.8.19 unmoved and later absorbs it |
| M4 | RECONCILE (amended: **probe-then-retire**) | G-6 probe first; PASS → pico retired as discharged, FAIL → pico stays the live interim vehicle; Stage-2 rides the OPP-12 ≥0.9.x pair — one mechanism on both ledgers |
| M5 | SEQUENCE | Nothing schedules against OPP-12 until seq-12 exists; one liaison PROPOSAL closes it |
| M6 | DEFER | "Ratified ≠ scheduled ≠ cheap" line added; build gets its own scheduling call — no sequence change |
| M7 | RECONCILE (**conditional on G-3**) | After seq-12 only: §14 purge ≡ OPP-12 `purge` (one verb); §12 touch → projection registry, not a verb; §11 read.state stays separate+evidence-gated. **Blocking step, not residual risk:** HITL re-affirms §14's committed timing (if HITL wants purge pre-0.9.x it ships forward-compatible with the transition table). Net **+5** verbs worst-case vs +7 naive |
| M8 | RECONCILE (Memex PROPOSAL) | 0.5.3.1 re-keys from version-label to capability probes; 0.8.15 Slice-0 states which adjacencies ride its window |
| M9 | GATE-ON-EVIDENCE | G-1 package commissioned now; **E-A2 telemetry named as unowned work needing a Memex owner or an explicit HITL waiver**; default NO |
| M10 | RECONCILE (docs-only) | plan-0.8.19 prereq reworded; 0.8.17↔0.8.18 inversion promoted to the master edge list |
| M11 | DEFER + ADD agenda item | 0.8.20 Slice-0 gains the sqlite-vec-predecessor consideration; no slot change |
| M12 | RECONCILE (Memex PROPOSAL) | 0.5.4 partial-launch: gate-untouched gauntlet groups now, rest after round-3 |
| M13 | RECONCILE (**Memex PROPOSAL** — label corrected) | Hard reading adopted: 0.5.5-A waits on 0.5.4 gates touching track-A scope |
| M14 | SEQUENCE | R-A freeze gains a read-only `ProjectionSpec` cross-review checkpoint; DECOUPLE placement otherwise intact |
| M15 | RECONCILE | ~0.8.15/0.5.3.1 joint FTS item = MEASUREMENT ONLY; extension door closed for 0.8.x |
| M16 | ACCELERATE (Memex PROPOSAL) | Re-pin to origin/main; probes include **A-1 AND A-2** + `stable_id`; unstalls 0.5.1 Phase 5 immediately |
| M17 | RECONCILE (docs-only + liaison note) | #30/#31 may pin against origin/main now; corpus README fixed |
| M18 | RECONCILE (docs-only) | F-11a rename; new hard edge I-7 (0.8.16 Δ → 0.8.18 calibration) made machine-visible |
| **M19 (new)** | **DECIDE (HITL, G-5)** | Orphaned `0.8.14-gpu-rerank`/`ce_blend_enabled`/`embed_batch_cls`: admit as explicit 0.8.14 scope or re-home (0.8.16 recommended); Memex informed either way — the "re-pin unblocks everything" claim is incomplete without this |
| **M20 (new)** | **VERIFY-OR-SCHEDULE** | Cutover doc's five unresolved risks (SDK maturity, aarch64/Jetson, append-log scale, **single-writer vs two-process**, join expressiveness): each cited-as-resolved or on the next joint-sync agenda **before 0.5.1 Phase 6 closes B-1** |
| **M21 (new)** | **ANNOTATE (docs-only)** | Memory entry `1-2M-scaling-kernel-first-not-ann.md` marked "conditional on E1 + premise, both OPEN" — prevents future sessions treating A>C>B as settled |

---

## API-SURFACE DISCIPLINE

Baseline: **16 governed verbs.** Joint goal: serve Memex without ballooning the SDK.

- **One lifecycle channel.** All lifecycle/purge/touch-shaped asks route through the OPP-12 contract once ratified: OPP-12's `transition` / `purge` / `configure_projections` (+3, → ~19 verbs). §14's committed physical-purge verb is **the same verb** as OPP-12 `purge` — never two. §12's touch/last-accessed becomes a projection-registry entry (`filterable`/`rankable` last-accessed), **not a verb**, still gated on its Memex value test.
- **Worst-case ceiling: +5** (OPP-12's +3, plus §11's op-store `read.state` + latest-state scan — op-store is explicitly outside OPP-12 scope — and only if the 0.5.3.1 latency measurement proves the client-side collapse workaround inadequate). Anything beyond +5 requires a fresh HITL case.
- **2MM adds zero surface.** Path A is an internal kernel behind existing verbs — exact semantics, identical results, no new API. Path C (if ever, at 2.x) is in-engine per the E6 rule. E1/E-A1/E-A2 are measurement, not surface.
- **Identity is a type change, not a verb**: `SearchHit.id` typed newtype (C-2) *shrinks* the hit struct (subsumes `stable_id`, retires `write_cursor`), lands only with the OPP-12 pair.
- **Standing denials hold**: no multi-field FTS, no custom tokenizers, no per-column BM25 weights in 0.8.x (R-I4); recovery denylist stays five names (`undelete`, never `restore`); SDKs stay thin pass-throughs with X1 parity — exposure, never logic.

---

## RISKS & CONTINGENCIES

**G-1 (2MM premise) = NO** *(default)*: Published roadmap stands unchanged — ANN at 2.x, 0.8.20 deps-only, F-17 ladder intact. Minimal E1 is banked anyway (~$0); memory annotation (M21) prevents A>C>B fossilizing. Cost of a later reversal: one standalone even-micro perf slot. **YES**: exactly one invariant-preserving kernel slice may enter 0.8.14 under M2's admissibility conditions; IVF stays a 2.x contingency; F-17 gate untouched (kernel makes it reachable early; hard assert still 1.1.0). **YES + E1 says not-overhead-bound**: the only path that legitimately reopens ANN-earlier — escalate to HITL with E2/E3 designs; even then the 0.8.20 sweep is a *predecessor*, not a slot to repurpose.

**G-3 (OPP-12) not ratified / Memex amends**: No build was scheduled against it (M5 rule), so slippage costs only the M7 consolidation and the M14 checkpoint reverting to watch-status. §14's purge commitment survives independently (HITL-committed); if ratification stalls long, HITL may ship purge standalone, forward-compatible with the draft transition table. Memex 0.5.5-A write-side registry proceeds regardless (DECOUPLE placement).

**G-6 probe FAILS** (`stable_id` insufficient for hit-level data-fitness): the Cause-A pico is NOT retired — it remains the sized-first interim vehicle, now evidence-backed; OPP-11's harness runs retrieval-only fitness until it lands.

**G-4 (#31) FAILS again**: no bench build (slices B–D cancelled/deferred); fall back to path (c) synthetic-bridge *only if HITL forces it*; #30 acquisitions already made retain standalone eval value.

**G-5 unresolved by 0.8.14 Slice 0**: branch stays parked; liaison tells Memex CE-blend adoption has no near-term FathomDB home — Memex plans around client-side blend consumption. Worst outcome is the silent one (Memex assuming "0.8.14 has it"); this document's Phase-0 liaison note forecloses that.

**0.8.14 slips**: 0.8.15 slips one-for-one (I-2); 0.8.16/0.8.18 cascade; Memex 0.5.3.1 unaffected (capability-gated, not date-gated after M8); Memex 0.5.4/0.5.5 tracks unaffected (no FathomDB dependency). This asymmetry is why nothing else is allowed onto the long pole without a gate.

**M20 risks verify red** (e.g., single-writer incompatible with two-process): 0.5.1 Phase 6 B-1 does not close; the full-cutover destination is re-scoped at a joint sync before further adapter investment — cheaper than discovering it post-cutover.

---

*Prepared as decision-support by the cross-product chief architect role. Approval path: HITL signs gates G-1..G-6 individually; Memex-side items travel as liaison PROPOSALS; docs-only reconciliations (M1, M10, M11, M17, M18, M21) may commit to fathomdb main immediately per standing policy.*
