---
title: Cross-product priorities + misalignment matrix (FathomDB ⇄ Memex)
date: 2026-07-03
status: PROPOSAL — awaiting HITL review (session "fathom-2MM-plan-refactor-0.8.x")
desc: Fable-5 priority ranking across both products and the full misalignment matrix (type/severity/root-cause/resolution).
generator: model-tiered dynamic workflow (Sonnet-5 collect · Fable-5 analyze/critique/synthesize · Opus align); 11 agents, 0 errors
---

# CROSS-PRODUCT PLAN — PRIORITY RANKING + MISALIGNMENT MATRIX

**Verified from git during this pass (supersedes stale digest caveats):** local fathomdb `main` == `origin/main` (`83ef13fc`, 0 ahead); 0.8.11.2 (`1137c572`) IS on origin/main; A-1 `$.action_kind` allowlist IS on origin/main (as `9a46611b`; the `9e0a3459` hash Memex cites was rebased); corpus `source_type` 6→8 + `entity_ids` + WEC-Eng ARE on origin/main. The "may not be pushed to origin yet" caveats in the Memex handoff are RESOLVED — Memex is waiting on nothing that isn't already published to origin.

---

## 1. PRIORITY RANKING

Ranked most-important first. Axes: **V**=value to joint product, **U**=urgency, **D**=dependency weight (what blocks on it), **E**=evidence-gating status.

**1. FathomDB 0.8.14 (EXP-S substrate + BM25F/F5)** — V: keystone substrate for router, F9 schema, and (if premise=YES) the kernel's packed-code store. U: next-up, "single most schedule-critical build" (F-1). D: 0.8.15 slips one-for-one (I-2); 0.8.16 F9 schema waits; indirectly Memex 0.5.3.1. E: only the F5 ADR pre-condition check at Slice 0; not experiment-blocked. Nothing outranks the long pole.

**2. 2MM premise resolution + minimal E1 (a DECISION package, not a build)** — V: forks the whole vector-latency roadmap. U: artificially high — NOT because the kernel is urgent, but because the minimum-divergence landing spot for the kernel is 0.8.14's EXP-S substrate; deciding after 0.8.14 closes forfeits the cheap path and forces a later standalone slot. D: gates any Path-A work; blocks the `ann-index-vec0.md` reconciling edit. E: doubly gated — the HITL premise (1–2M near-term for Memex/Hermes/OpenClaw? UNKNOWN — needs HITL/scout for consumer corpus projections + E-A2 filter-rate telemetry) AND E1 (never run). **Mis-prioritized as a dependency**: its measurement substrate (`scale.rs`) sits at 0.8.19, end of line — see M3.

**3. Cause-A / SearchHit typed-identity reconciliation (F-8a)** — V: single highest dependency fan-in in the portfolio — gates OPP-1 graph path, OPP-9 join, OPP-11 hit-level data-fitness, Memex `fathom_store.py` hack deletion, and OPP-12's `SearchHit` shrink. U: high — two contradictory placements exist (OOB pico vs ≥0.9.x breaking pair, M4) and no owner/date. D: heaviest. E: none — it's a scheduling/reconciliation decision; the additive `stable_id` half already shipped (0.8.11.2, on origin/main). **Mis-prioritized: dependency weight vastly exceeds its current scheduling attention.**

**4. Memex 0.5.1 Phases 5–6 (consume + behavioral-equivalence gate, close B-1)** — V: makes Memex a real governed-surface consumer — the joint product's proof point and the substrate every OPP value-test runs on. U: high and **currently under-prioritized because its stated blocker is stale**: A-1 and `stable_id` are live on origin/main today (verified above); Memex only needs a re-pin. D: OPP-5/7/10/1 consumption, OPP-11 harness realism. E: none engineering-side; awaits re-baseline/re-SIGN of plan-0.5.1.

**5. Memex 0.5.5 track B — #31 QID entity-linking + re-probe** — V: gates the entire cross-source bench (the only path to at-power multi_session/temporal/global evidence FathomDB's router tuning needs). U: active slice, in flight, HITL-ratified. D: bench slices B→C→D and #30 corpus utility all gate on it. E: correctly evidence-gated — "do not build questions until this passes." Keep gated; also owns the enrichment-location decision (fathomdb-durable vs memex-ephemeral — a scope-creep watch item).

**6. OPP-12 ratification close-out (Memex `agree` + ledger text apply)** — V: freezes the lifecycle contract both products' cleanup depends on; resolves CR-056/057/060 direction. U: one message away (seq-12) — cheap, do now. D: 0.5.5-A retrievability-declaration path and the M4/M7 reconciliations key off the frozen text. E: none. **The BUILD is a different item: ~90% net-new, placement TBD ≥0.9.x — do not let ratification urgency bleed into build urgency.** Memex-side action = PROPOSAL for HITL only (push-scope).

**7. FathomDB 0.8.15 (dispatcher/router)** — V: the config-carrying-tuning lever (0.8.11 verdict: arm-switching≈0, tuples are the win). U: medium — hard-gated on 0.8.14 (I-2). D: 0.8.17 backbone, OPP-3 `margin`/V-7, OPP-10 production target, Memex 0.5.3.1's timing anchor (but see M8 — its §10–14 adjacencies are NOT commitments). E: locus decision + route-accuracy AC at Slice 0 from EXP-Fr-acc (in hand from 0.8.11).

**8. Memex 0.5.4 kickoff (gauntlet + behavior-model gates)** — V: prerequisite decisions (Gate ②, ratified) constrain the entity model that OPP-12's C-1 must mesh with. U: medium — parked awaiting HITL kickoff; 4 round-3 sub-gates still open (M12). D: hard-blocks 0.5.5-A (Commission C). E: internal gates only.

**9. FathomDB 0.8.16 (F9 importance/confidence + ONNX)** — V: F9 unblocks OPP-12's `rankable` signal algebra (contractually incomplete until F9); ONNX Slice-15 Δ is 0.8.18's calibration input. U: after 0.8.14. D: 0.8.18 #5 hard-depends on Slice-15 output (M18 — coupling under-flagged in the I-edge table). E: eu7 ≥0.90 gate on embedder/index touches.

**10. FathomDB 0.8.18 (production-safety GA capstone)** — V: publish pipeline + vector-equivalence = the release-engineering trust layer; prerequisite for 0.8.17's Slice-40 dry-run. U: sequenced after 0.8.16. D: 0.8.17 close; real `v*` publish of the line. E: R-VEQ-2 two-sided test; HITL tag gate. Keep F-17 discipline: it claims NO scale envelope.

**11. Memex 0.5.5 track A (entity schema-registry + legacy convergence)** — V: high (KEYSTONE R-B unblocks CR-015; supplies `EntityTypeSpec` for OPP-12 C-1). U: low now — blocked on 0.5.4 gates (resolve M13's soft/hard discrepancy as hard). D: OPP-12's single-registration-flow requirement. E: R-A/R-B binding gates.

**12. OPP-1/3/6 experiment ladders** — V: they decide adoption of decomposition, cascade, and coverage investment. U: low-medium; strict V-1→V-7 order stands, OPP-3 paused below its own 0.70 AUROC bar pending `margin` (0.8.15/V-7). E: correctly and deliberately gated — do not pull forward, do not prebuild (OPP-1's anti-abstraction stance).

**13. FathomDB 0.8.17 (router hardening/forks)** — V: productizes 0.8.15's tuples. U: low — runs after 0.8.18 by its own dependency (documented inversion, M10). D: nothing external blocks on it. E: Track C corpus-gated/HITL-spend-gated (non-blocking).

**14. FathomDB 0.8.19 (EXP-FT + benchmark harness `scale.rs`)** — V: measurement infrastructure + FT ladder. U: nominally end-of-line, **but conditionally mis-prioritized: it owns E1's natural substrate while E1's answer is needed before 0.8.14** (M3). Resolution: extract minimal E1, don't move 0.8.19. E: FT-4 hard gate; productization explicitly post-0.8.19.

**15. Memex 0.5.3.1 (FathomDB-joint deferrals)** — V: real (FTS drift measurement, op-store read-back). U: none — correctly deferred, but its gate is mis-keyed to a version number instead of capabilities (M8). E: value-tests by design.

**16. FathomDB 0.8.20 (Library Sweep)** — V: dependency hygiene + the sqlite-vec migration that any future vec0-internal kernel work must sequence with. U: none — deferred-with-trigger, Slice-0 HITL gate; "do not migrate for novelty." E: trigger-gated. Gains one new Slice-0 agenda item if premise=YES (M2/M11).

**Standing mis-prioritization callouts:** (a) E1's substrate at rank 14 vs its decision needed before rank 1 — the sharpest inversion in the portfolio; (b) Cause-A's fan-in vs no owner; (c) Memex 0.5.1 Phase 5–6 stalled on a blocker that is already cleared on origin/main; (d) OPP-12 ratification (cheap, do now) vs OPP-12 build (large, TBD) must not be conflated.

---

## 2. MISALIGNMENT MATRIX

**M1 — ANN still listed in 0.8.20 inside the 2MM roadmap doc's own table**

- BETWEEN: 2MM roadmap §5 version-mapping table ↔ F-16 ("0.8.20 stays deps-only, ANN=2.x") ↔ the same proposal's divergence doc + DECISION-BRIEF minimum-divergence path.
- TYPE: version-slot-collision (internal doc inconsistency).
- SEVERITY: med (docs-only, but a downstream reader of §5 alone re-imports a settled-away collision).
- ROOT CAUSE: roadmap doc's table was not updated after its own reconciliation section resolved the collision.
- RESOLUTION: **RECONCILE** — edit the roadmap §5 table to match the minimum-divergence path: ANN/IVF stays 2.x (F-16); 0.8.20 row reads deps-only. Docs-only change; per "don't gate trivial changes on CI," commit directly.
- RESIDUAL RISK: if premise=YES AND E1 shows not-overhead-bound, the ANN pull-forward question legitimately reopens — that escalation path must stay written down (it is, in the brief), not deleted with the table row.

**M2 — Kernel-into-0.8.14 vs F-1 "resist adding scope to 0.8.14"**

- BETWEEN: 2MM proposal (kernel + partition pruning + oracle-ext + parallel IVF spike into 0.8.14) ↔ PROGRAM-SEQUENCING F-1 ↔ divergence doc's own "fully-scoped, MEDIUM-HIGH divergence, needs HITL."
- TYPE: ordering/sequencing + governance-conflict.
- SEVERITY: high — 0.8.14 is the program's long pole; scope added here slips everything one-for-one.
- ROOT CAUSE: the packed-code store genuinely IS "another index kind," making EXP-S the technically natural home — technical fit is colliding with schedule-criticality.
- RESOLUTION: **GATE-ON-EVIDENCE** — premise + minimal-E1 must both resolve before 0.8.14 Slice 0. If both YES: HITL admits exactly ONE invariant-preserving kernel slice (exact semantics, zero recall risk) into 0.8.14 and pushes the IVF spike (E2/E3) out entirely to the 2.x contingency track. If either is unresolved at Slice 0: 0.8.14 ships as-specified and the kernel takes a later standalone perf slot (steward-elected even micro). Never re-theme 0.8.14 silently.
- RESIDUAL RISK: even one slice grows the long pole; mitigate by making the kernel slice hard-droppable at 0.8.14's Slice-20 gate without cascading (it shares only the schema bump, not EXP-S semantics).

**M3 — E1's substrate (0.8.19 `scale.rs`) lands after E1's answer is needed (pre-0.8.14)**

- BETWEEN: 2MM minimum-divergence step 1 ("run E1 on 0.8.19's scale.rs — no new slot") ↔ step 2 (kernel rides 0.8.14) ↔ forced order 0.8.14 → … → 0.8.19.
- TYPE: ordering/sequencing.
- SEVERITY: high if premise=YES; low if NO.
- ROOT CAUSE: the proposal reused an existing slot for measurement without checking that slot's position in the interleave order.
- RESOLUTION: **SPLIT** — extract a minimal standalone E1 overhead-attribution bench ($0, label-only pico or scratch harness on the existing `ir_recall_eval`/eval-layer substrate, no new release slot) runnable now; 0.8.19's full `scale.rs`/#13 harness later absorbs/supersedes it. Do NOT move 0.8.19 forward.
- RESIDUAL RISK: throwaway bench code duplicating #13; explicitly mark it as absorbed-by-0.8.19 so it doesn't become a second harness.

**M4 — Cause-A scheduled twice: standalone OOB pico vs OPP-12's ≥0.9.x breaking pair**

- BETWEEN: OPP-11 Q1 resolution ("standalone OOB pico, off the trains") ↔ OPP-12 C-2 ("typed `{space,value}` SearchHit.id lands-together with anonymous-node logical_id minting in the coordinated ≥0.9.x pair") ↔ Cause-A appearing as differently-worded asks in OPP-1/9/11.
- TYPE: duplication + ordering/sequencing.
- SEVERITY: high — risk of double-scheduling one mechanism, or of Memex waiting on 0.9.x for something it half-has today.
- ROOT CAUSE: OPP-11's pico decision predates the OPP-12 convergence that revealed the identity fix and Cause-A are the SAME mechanism.
- RESOLUTION: **RECONCILE** — declare a two-stage single work item on both ledgers: Stage 1 (DONE) = additive `stable_id` shipped 0.8.11.2, on origin/main, satisfies the OPP-11/OPP-9 join-key need now — retire the "standalone Cause-A pico" as already-discharged; Stage 2 = the breaking typed-newtype subsumption lands ONLY with the OPP-12 pair (≥0.9.x, placement at the steward scheduling call). Verify Stage-1 sufficiency for OPP-11 hit-level data-fitness: UNKNOWN — needs a small Memex-side probe (scout).
- RESIDUAL RISK: if `stable_id` proves insufficient for data-fitness attribution, an interim ask reappears before 0.9.x — acceptable, it would arrive evidence-backed.

**M5 — OPP-12 status stale on the Memex side; no seq-12 `agree` exists**

- BETWEEN: FathomDB README/design docs ("CONVERGED — pending MEMEX agree") ↔ Memex prose ledger line 93 ("DISCUSSING") ↔ JSONL at seq 11.
- TYPE: unmet-cross-product-dependency + governance-conflict (stale cross-repo state).
- SEVERITY: med — any Memex-side planner reading the prose ledger badly under-estimates OPP-12's state; any FathomDB planner reading "CONVERGED" over-estimates it (no agree exists).
- ROOT CAUSE: push-scope constraint (FathomDB never edits the memex repo) + the discussion moved to a JSONL side-channel the prose ledger doesn't mirror.
- RESOLUTION: **SEQUENCE** — one liaison PROPOSAL to Memex HITL (never auto-applied): (1) post `agree` at seq 12 or state objections, (2) apply the prepared OPP-12 replacement text to the prose ledger. Until seq-12 exists, both products' plans must carry OPP-12 as PENDING, and nothing may schedule against it.
- RESIDUAL RISK: Memex raises late objections and part of the frozen scope reopens — cheaper now than after any build starts.

**M6 — "CONVERGED" framing invites premature slotting of a ~90%-net-new build**

- BETWEEN: OPP-12 design completeness (fully specified, 4 review rounds) ↔ code-grounded audit (~90% net-new) ↔ placement "TBD, likely ≥0.9.x, ratification schedules nothing."
- TYPE: premise-gap.
- SEVERITY: low-med.
- ROOT CAUSE: design maturity and build maturity diverged; the artifact reads more shipped than it is.
- RESOLUTION: **DEFER** — add one explicit line to the OPP-12 README status block: "Ratified ≠ scheduled ≠ cheap: ~90% net-new engine work; placement requires a dedicated steward/HITL scheduling call with a build-cost estimate; no 0.8.x slot exists for it." Fold the F9-algebra dependency (0.8.16) into that scheduling call's inputs.
- RESIDUAL RISK: none material; slight over-caution delay.

**M7 — Verb duplication + surface sprawl: OPP-12's `purge` vs plan-0.8.15 §14's committed physical-purge verb; §11 read.state / §12 touch as free-standing verb candidates**

- BETWEEN: OPP-12 (+3 verbs incl. `purge(logical_id)`) ↔ plan-0.8.15 §14 (physical-purge verb COMMITTED, timing at a later sync) ↔ §11 (+2 op-store read-back verbs) ↔ §12 (touch verb) ↔ the joint sprawl-containment goal (16 verbs today).
- TYPE: duplication + api-surface-sprawl.
- SEVERITY: high — these are the same problem-space being granted verbs through two independent channels; naive union is 16→~22+.
- ROOT CAUSE: §11–14 adjacency asks predate OPP-12's convergence; nobody has folded them under the lifecycle contract.
- RESOLUTION: **RECONCILE** — declare §14's committed purge verb IDENTICAL to OPP-12's `purge(logical_id)` (one verb, one contract; §14's commitment stands, its mechanism is OPP-12's — timing inherits the OPP-12 scheduling call, and Memex's purge/forget xfails re-key to that). Route §12's touch/last-accessed through OPP-12's projection-registry framing (a `rankable`/`filterable` last-accessed projection, not a new verb) — still gated on its Memex value test. §11's op-store read.state stays a separate candidate (op-store is explicitly OUT of OPP-12 scope) but remains evidence-gated on the 0.5.3.1 latency measurement. Net: at most +4 verbs across both initiatives instead of +7.
- RESIDUAL RISK: §14's "committed, 0.8.15-ish window" timing expectation slips to the OPP-12 pair — needs an explicit HITL re-affirmation since HITL committed the verb; if HITL wants purge earlier than 0.9.x, it must ship forward-compatible with OPP-12's transition table.

**M8 — Memex 0.5.3.1 hard-gated on "FathomDB ~0.8.15" whose plan does NOT contain the deliverables**

- BETWEEN: plan-0.5.3.1 ("Not startable before FathomDB ~0.8.15") ↔ plan-0.8.15's actual ladder (dispatcher only; §10–12 items are explicitly "adjacent, NOT this release," candidate/off-default, no committed slice).
- TYPE: unmet-cross-product-dependency + premise-gap.
- SEVERITY: med-high — Memex believes a version number delivers capabilities FathomDB has only listed as non-commitments; 0.5.3.1 would "unblock" at 0.8.15 and find nothing to consume.
- ROOT CAUSE: version-label gating used where Memex's own doctrine is capability-probe gating.
- RESOLUTION: **RECONCILE** — re-key 0.5.3.1's start gate from "~0.8.15" to named capability probes (read.state verb present; FTS-drift measurement counterpart scheduled), consistent with Memex's probe-gated consumption model (PROPOSAL for Memex HITL). FathomDB-side: the 0.8.15 Slice-0 locus decision should state explicitly which §10–14 adjacencies (if any) ride the 0.8.15 window vs move under OPP-12/M7.
- RESIDUAL RISK: 0.5.3.1 floats indefinitely if no capability ever ships — acceptable; the FTS value-test half doesn't actually need new engine surface and could start earlier as measurement-only.

**M9 — The premise contradiction: "consumers run well under ~50k rows" vs the 1–2M target — with the deciding data never requested**

- BETWEEN: published roadmap's ANN-deferral assumption (`ann-index-vec0.md`) ↔ the entire 2MM proposal ↔ the absent inputs (consumer corpus-size projections; E-A2 filter-rate from real Memex/Hermes/OpenClaw traffic).
- TYPE: conflicting-assumption + premise-gap.
- SEVERITY: high — it forks the roadmap, and both docs punt to HITL while nobody has commissioned the data the HITL call needs.
- ROOT CAUSE: the ask exists only as prose inside decision-support docs; no scout/liaison task was ever cut.
- RESOLUTION: **GATE-ON-EVIDENCE** — commission the missing inputs NOW (cheap, parallel to 0.8.14 prep): a scout for corpus-size projections from Memex (readable in-repo), Hermes/OpenClaw (public repos), plus a Memex liaison PROPOSAL for filter-rate telemetry; then put THE premise question to HITL before 0.8.14 Slice 0. Default on non-answer = NO (published roadmap stands; bank minimal E1 per M3 anyway). Do not edit `ann-index-vec0.md` until the premise resolves.
- RESIDUAL RISK: projections are guesses — mitigated by the NO-default and by E1 being banked either way, so a later YES costs only the kernel slot, not lost measurement.

**M10 — 0.8.19's stale prerequisite claim + 0.8.17/0.8.18 numeric-order inversion**

- BETWEEN: plan-0.8.19 ("#11-full publish matrix — DONE @0.8.18, already satisfied at plan date 2026-06-28") ↔ 0.8.18 not yet run ↔ plan-0.8.17's after-0.8.18 dependency vs its numeric position.
- TYPE: ordering/sequencing (doc inconsistency; the 0.8.17 inversion is acknowledged, the 0.8.19 claim is just wrong/stale).
- SEVERITY: low-med.
- ROOT CAUSE: plan-0.8.19 written pre-renumber and never reconciled against the new sequence.
- RESOLUTION: **RECONCILE** — one docs-only pass on plan-0.8.19: reword the #11-full prerequisite as "owned by 0.8.18, must be closed before FT-5's wheel-matrix extension" (soft, FT-1..4 unaffected). Leave the 0.8.17↔0.8.18 inversion as-is — it's explicit and intentional; add it to PROGRAM-SEQUENCING's edge list so it's machine-visible, not just prose.
- RESIDUAL RISK: none material.

**M11 — 0.8.20's even/OOB classification self-contradiction (+ new sequencing input from 2MM)**

- BETWEEN: master §4 ("OOB, net-new" F-12) ↔ two-tier rule (even = real/publishable) ↔ plan-0.8.20's own "reconcile at Slice 0" ↔ 2MM step 5 (vec0-internal kernel work must sequence after/with the sqlite-vec migration).
- TYPE: governance-conflict.
- SEVERITY: low (a Slice-0 gate already exists to resolve it).
- ROOT CAUSE: F-12 filed the sweep before the two-tier model (F-13) fully settled even/odd semantics.
- RESOLUTION: **DEFER** to the existing Slice-0 HITL gate, but ADD one agenda item now: "if the 2MM premise=YES, the sqlite-vec 0.1.7→0.1.9 migration becomes a predecessor of any vec0-internal kernel work — factor into trigger evaluation." No slot or classification change today.
- RESIDUAL RISK: if premise=YES arrives late, 0.8.20's trigger evaluation may need re-running.

**M12 — Memex 0.5.4: tail-INDEX "Launchable now" vs 4 round-3 gates STILL OPEN**

- BETWEEN: `0.5.x-tail-INDEX.md` ↔ plan-0.5.4's own gate table (scoring re-pin CR-038/054, provenance read-surface CR-055, plan-exec CR-044, session-lifetime CR-032 open).
- TYPE: conflicting-assumption (doc-level).
- SEVERITY: med — an orchestrator kicked off from the INDEX alone would run into unratified gates mid-flight.
- ROOT CAUSE: INDEX summarizes at commission granularity; gate state lives one level down and moved after the INDEX was written.
- RESOLUTION: **RECONCILE** (Memex-side PROPOSAL) — INDEX marks 0.5.4 "launchable for gauntlet groups whose findings don't touch the 4 open gates; round-3 gate required before the rest." Kickoff prompt must enumerate the open gates.
- RESIDUAL RISK: partial-launch bookkeeping complexity; findings misclassified against gates.

**M13 — 0.5.5 track A start condition: "typically 0.5.4 first" (soft) vs "Blocked on 0.5.4 gates" (hard)**

- BETWEEN: plan-0.5.5 ↔ tail-INDEX Commission C.
- TYPE: conflicting-assumption.
- SEVERITY: low.
- ROOT CAUSE: hedged prose in one doc, categorical status column in the other.
- RESOLUTION: **RECONCILE** — adopt the hard reading (tail-INDEX): Gate ② + typed-vs-dict decisions materially constrain the goal/task/entity model track A builds on, and both are now ratified (2026-07-02), so the practical residue is only 0.5.4's remaining round-3 gates that touch track-A scope. State that precisely in plan-0.5.5.
- RESIDUAL RISK: slight over-serialization if the remaining open gates turn out orthogonal to track A.

**M14 — OPP-12 C-1 single-registration-flow vs Memex 0.5.5-A registry proceeding pre-ratification**

- BETWEEN: OPP-12 C-1 (`EntityTypeSpec` drives `ProjectionSpec`, HARD, single flow) ↔ 0.5.5-A R-A/R-B (registry build greenlit, PLACEMENT=DECOUPLE: write-side/DISPLAY proceeds now, retrievability-declaration waits).
- TYPE: ordering/sequencing + duplication risk (two registries diverging = exactly what C-1 forbids).
- SEVERITY: med.
- ROOT CAUSE: track A's build cadence is decoupled from an unratified contract that constrains its keystone schema.
- RESOLUTION: **SEQUENCE** — keep the DECOUPLE placement, but insert one binding checkpoint: before 0.5.5-A's R-A gate freezes the persisted `EntityTypeSpec` schema, it is reviewed against OPP-12's `ProjectionSpec` field set (a read-only cross-repo review, no build dependency, PROPOSAL to Memex).
- RESIDUAL RISK: if OPP-12 ratification amends `ProjectionSpec` after R-A freezes, a bounded schema migration on the Memex side — acceptable given HITL's breaking-changes waiver.

**M15 — FTS extension: door "closed" (R-I4) vs still framed as open/joint (~0.8.15, 0.5.3.1, MEMORY "revisit ~0.8.15")**

- BETWEEN: plan-0.5.1 decisions log ("will NOT except on recall-test-proven unrecoverable miss"; tokenizer "likely denied until post-1.x") ↔ plan-0.5.3.1 / plan-0.8.15 §10 ("adopt-as-is vs extension — joint decision") ↔ multifield-FTS memory note.
- TYPE: conflicting-assumption.
- SEVERITY: med — Memex could spend 0.5.3.1 effort preparing an ask FathomDB has effectively pre-denied, or FathomDB could pre-build for an ask that never clears its own bar.
- ROOT CAUSE: "measure at ~0.8.15" (the value-test) got conflated with "decide the extension at ~0.8.15" (mostly already decided).
- RESOLUTION: **RECONCILE** — restate in both plans: the ~0.8.15/0.5.3.1 joint item is MEASUREMENT ONLY (recall/coverage + ranking drift); the extension door for 0.8.x is closed; a proven unrecoverable miss produces a ≥0.9.x ask via HITL, nothing sooner. This also honors sprawl containment.
- RESIDUAL RISK: a genuine unrecoverable miss then waits for 0.9.x — the escape valve (HITL) exists.

**M16 — Memex refit still treats A-1 + `stable_id` as inert pending a merge that has already happened**

- BETWEEN: memex-side-refit-design §5 ("reaches Memex's build only when 0.8.11.2 merges to main"; pin `ba80866d`) ↔ verified reality (0.8.11.2 AND A-1 on origin/main today).
- TYPE: unmet-cross-product-dependency (stale — actually a MET dependency nobody consumed).
- SEVERITY: med — a free unblock for 0.5.1 Phase 5 (server-side `action_kind` filter, `stable_id` activation) sitting idle.
- ROOT CAUSE: no delivery signal flowed through the liaison channel after the 0.8.11.2 merge; probe-gated design means Memex won't notice until it re-pins.
- RESOLUTION: **ACCELERATE** — liaison PROPOSAL to Memex: re-pin the editable build to fathomdb origin/main (≥`1137c572`), run the capability probes, flip A-1 to server-side and wire `stable_id`; folds directly into ranking item #4. Note the correct A-1 hash on main is `9a46611b` (the refit doc's `9e0a3459` was rebased).
- RESIDUAL RISK: none beyond normal re-pin regression risk, covered by 0.5.1's own equivalence gate.

**M17 — "Corpus schema merged locally, may not be pushed" caveat is stale**

- BETWEEN: 0.5.5 steward handoff caveat ↔ verified reality (`entity_ids`/`source_type` 8 + WEC-Eng on origin/main; local main == origin/main, 0 ahead).
- TYPE: unmet-cross-product-dependency (resolved-in-fact, stale-in-docs).
- SEVERITY: low (was med until verified this pass).
- ROOT CAUSE: handoff written mid-flight before the push happened.
- RESOLUTION: **RECONCILE** — liaison note to Memex 0.5.5-B: origin visibility confirmed, #30/#31 may pin `corpus.manifest_sha256` against origin/main now. Also fix the flagged `tests/corpus/README.md` staleness (6→8 source_types) as a trivial docs commit (fathomdb-side, no CI gate needed).
- RESIDUAL RISK: none.

**M18 — Doc-hygiene pair: duplicate F-11 finding IDs; 0.8.16→0.8.18 calibration coupling missing from the I-edge table**

- BETWEEN: PROGRAM-SEQUENCING §6 (two "F-11" entries) ↔ §2a edge list (I-1..I-6 omits the Slice-15 Δ → #5 tolerance feed-forward).
- TYPE: governance-conflict (record integrity) + ordering (unmodeled hard edge).
- SEVERITY: low.
- ROOT CAUSE: superseded draft retained under the same ID; the calibration edge was documented in plans but never promoted to the master edge table.
- RESOLUTION: **RECONCILE** — rename the superseded entry F-11a (historical, superseded-by-F-11) and add edge I-7: 0.8.16 Slice-15 (candle↔ONNX Δ) → 0.8.18 Slice-0 (#5 tolerance calibration), physically hard. Docs-only.
- RESIDUAL RISK: none.

---

**Top 5 actions this ordering implies (all governance-respecting):** (1) start 0.8.14 as specified — no kernel scope without M2's gate clearing first; (2) cut the M9 scout + minimal-E1 (M3) now, in parallel, $0; (3) send the two liaison PROPOSALS — OPP-12 seq-12 ask (M5) and the re-pin/origin-visibility unblocks (M16/M17); (4) execute the M4 Cause-A two-stage reconciliation on both ledgers; (5) run the M1/M7/M10/M18 docs-only reconciliation commits straight to main.

---

## APPENDIX — ADVERSARIAL CRITIQUE (Fable-5 gap-check)

Verification note before findings: I re-ran the analysis's git claims — they hold (local main == origin/main `83ef13fc`; `1137c572` is an ancestor; `9a46611b` on origin/main, `9e0a3459` not; WEC-Eng/corpus commits on origin/main). The M16/M17 hinge is sound. I also verified two things the analysis did NOT check, which produce the biggest finding below.

## CORRECTIONS & GAPS

- **MISSED CROSS-PRODUCT DEPENDENCY + SLOT COLLISION: the `0.8.14-gpu-rerank` branch / `ce_blend_enabled` flip is orphaned.** Memex's retrieval-quality plan (bridge digest, ask #4) homes CE-rerank adoption at "FathomDB 0.8.14, branch `0.8.14-gpu-rerank`, houses the `ce_blend_enabled` flip" — but I verified `dev/plans/plan-0.8.14.md` contains **zero** mentions of rerank/ce_blend/gpu-rerank (scope is EXP-S + F5 only, F-1 forbids scope adds), the branch exists locally **unmerged**, and `ce_blend_enabled` is not on main. Memex also flags `embed_batch_cls` absent/CPU-only in its resolved build — same branch. Why it matters: a Memex-expected deliverable has no FathomDB home; ranking item 4 tells Memex to re-pin to origin/main and "fold into ranking #4," but re-pinning delivers neither. This is exactly the (a)/(d)/(f) failure the analysis was supposed to catch, and it's a second, unacknowledged scope-vs-F-1 collision on the long pole beyond the kernel (M2). Fix: add a matrix item — HITL must either admit the (rebased/green) gpu-rerank merge as 0.8.14 scope or explicitly re-home it (0.8.15/0.8.16) and say so to Memex via the same M16 liaison proposal. Corollary: the header claim "Memex is waiting on nothing that isn't already published to origin" is overbroad — this branch is a live counterexample.

- **INTERNAL CONTRADICTION in M4: the pico is retired before the sufficiency probe runs.** M4 says "retire the standalone Cause-A pico as already-discharged" AND "Verify Stage-1 sufficiency for OPP-11 hit-level data-fitness: UNKNOWN — needs a probe." Why it matters: if the probe finds `stable_id` insufficient, the pico was retired on a false premise and OPP-11's Q1 blocker silently reopens with no scheduled vehicle. Fix: sequence it — probe first; retire the pico only on a PASS; on FAIL the pico stays the live interim option (which is exactly what OPP-11's size-it-first resolution anticipated).

- **INTERNAL CONTRADICTION between M5 and M7: M7 schedules verb consolidation against an unratified OPP-12 while M5 forbids scheduling against OPP-12 until seq-12 exists.** M7 declares §14's purge "IDENTICAL to OPP-12's `purge`" and re-times an explicit HITL COMMITMENT (§14, committed 2026-06-30) to inherit OPP-12's ≥0.9.x window — treating ratification as settled (failure mode (e)). Why it matters: if Memex's seq-12 response amends the transition table or purge semantics, the §14 commitment has been re-keyed to a moving target; and demoting an HITL commitment's timing is not a "residual risk," it's a blocking pre-condition. Fix: make M7 explicitly conditional on seq-12 `agree`, and elevate the §14 HITL timing re-affirmation from residual-risk to a required step. Also fix the arithmetic: OPP-12 (+3) + §11 read.state + latest-state list-scan (+2) = **+5** verbs, not "at most +4."

- **M2's "hard-droppable kernel slice" understates schema coupling.** 0.8.14's commitment is ONE coordinated `SCHEMA_VERSION` bump; if the packed-code store rides that bump and the kernel slice is then dropped at Slice-20, you ship dead schema or need a second migration — violating the one-bump commitment the divergence doc itself calls out. Fix: the admissibility condition for the kernel slice must include "packed store is a derived/rebuildable structure (like FTS/vec0) with NO `SCHEMA_VERSION` coupling" — then droppability is real; otherwise the slice is not droppable and M2's mitigation is fiction.

- **MISSING MATRIX ITEM: the cutover doc's five unresolved substrate risks have no home anywhere.** The bridge digest explicitly flags that neither current doc revisits: Python-SDK production-grade, **aarch64/Jetson support**, append_only_log at-scale, **single-writer vs Memex's TUI+service two-process architecture**, and join-query expressiveness. Why it matters: 0.5.1 (rank 4) is the on-ramp to the full cutover; single-writer/two-process is architectural and could invalidate the destination, and aarch64 is a Memex ask with no FathomDB 0.8.x home (build-target gap, (a)+(d)). Fix: add a matrix item — verify-or-schedule: either cite where each risk was resolved (e.g., in the refit re-baseline) or put them on the next FathomDB↔Memex sync agenda before 0.5.1 Phase 6 closes B-1.

- **A-2 (bool-eq server-executable in `read.list`) is never mentioned.** It's the companion pending ask to A-1 (memex-roadmap ask #2, same urgency). Fix: one line in M16's liaison proposal — include A-2 confirmation in the capability probes at re-pin. Cheap, but its omission means the "stale blocker" story in M16 is incomplete.

- **OPP-11's final HITL SIGN is missing from the ranking and matrix.** The ledger digest says OPP-11 is `AGREED`, all axes resolved, "awaiting HITL sign to SIGNED" — and OPP-11 feeds *every* OPP's adoption gate plus underpins ranking item 4's value claim. It's the same shape as OPP-12 seq-12: one cheap action closing a governance loop. Fix: add to the "cheap, do now" set (top-action 3).

- **E-A2 filter-rate telemetry is treated as a data pull; it's actually unowned instrumentation work.** M9's "liaison PROPOSAL for filter-rate telemetry" implies Memex just hands over numbers, but no Memex slice owns capturing query-filter rates on real traffic (0.5.2's harness telemetry is SUT-side, not production-traffic-side). Why it matters: the premise decision's second input may silently never arrive, leaving HITL deciding on corpus projections alone. Fix: the M9 proposal must name it as a scoped ask needing a Memex owner/slot, or explicitly accept scout-level approximation and say the decision proceeds without it.

- **The already-committed durable memory `1-2M-scaling-kernel-first-not-ann.md` is an (e)-class hazard the analysis skips.** The 2MM digest flags it as the only committed artifact; it records A>C>B while E1 is unrun. Future sessions reading MEMORY.md may treat the ranking as settled fact. Fix: docs-only — annotate the memory entry (or its index line) "conditional on E1 + premise, both OPEN," alongside the M1 table fix.

- **M13's resolution edits a Memex-repo doc without the PROPOSAL label.** "State that precisely in plan-0.5.5" — plan-0.5.5.md is a memex file; per push-scope every Memex-side change must be labeled a PROPOSAL for HITL (the analysis does this correctly in M8/M12/M14/M16 but not M13). Fix: add the label. Minor but it's the exact governance slip category (b) asks about.

- **Ranking item 4 overstates the value-test substrate's readiness.** 0.5.2's harness runs RETRIEVE-only by default; the full classify→respond→reflect loop is a `NotImplementedError` stub and `data_fitness` isn't emitted on all reflect variants. "The substrate every OPP value-test runs on" is therefore partially aspirational — engine-fitness A/B verdicts (OPP-11's core) are currently retrieval-only. Fix: note the 0.5.2 debt items as a soft dependency of ranks 4/12.

- **F-8b (`record_feedback` reclassification) is a dangling gate absent from the matrix.** Verified it's a named pre-Slice-15 blocker in plan-0.8.15 and DP-0.17-B in 0.8.17 Track B. Minor, but it belongs on the same 0.8.15 Slice-0 agenda the analysis builds in M8.

**Where the analysis is sound (briefly):** the git verification is accurate (re-confirmed independently); F-16 (ANN=2.x), F-17 ladder, CPU-only/1-bit invariant, and push-scope are respected in every resolution I checked except the M13 label slip; M3's split-E1 resolution is the right call and correctly refuses to move 0.8.19; M9's default-NO is the correct conservative fork; M1/M10/M11/M18 are accurate and correctly sized; the E1-substrate-vs-E1-answer inversion callout is real and well-argued; treating OPP-12 ratification vs build as separate items is exactly right.

## TOP 3 THE SYNTHESIS MUST FIX

1. **Add the orphaned `0.8.14-gpu-rerank`/`ce_blend_enabled` deliverable as a first-class matrix item** — a Memex-expected capability with no home in plan-0.8.14, an unmerged branch, and a second unacknowledged F-1 scope collision on the long pole; the "re-pin unblocks Memex" story (M16 / rank 4) is incomplete without it.
2. **Repair the internal contradictions in the Cause-A/OPP-12/purge cluster**: M4 must probe-then-retire (not retire-then-probe); M7 must be conditional on OPP-12 seq-12 per M5's own rule, with §14's HITL-committed purge timing re-affirmed as a blocking step; correct the verb count to +5.
3. **Give the cutover doc's five unresolved risks (esp. single-writer vs two-process, aarch64/Jetson) a verify-or-schedule matrix item** before 0.5.1 Phase 6 closes — they gate the architectural destination the whole rank-4 workstream is driving toward, and nothing anywhere currently owns them.
