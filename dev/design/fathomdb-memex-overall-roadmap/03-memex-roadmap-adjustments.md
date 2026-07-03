---
title: Memex roadmap adjustments to align with the overall roadmap (PROPOSAL — not pushed)
date: 2026-07-03
status: PROPOSAL — awaiting HITL review (session "fathom-2MM-plan-refactor-0.8.x")
desc: Per-version 0.5.x-tail adjustment proposal; parity + cross-product dependency checks. Memex-side changes are proposals for HITL only.
generator: model-tiered dynamic workflow (Sonnet-5 collect · Fable-5 analyze/critique/synthesize · Opus align); 11 agents, 0 errors
---

# Memex 0.5.x Roadmap — Adjustment Proposal to Align with the Unified Roadmap

**Status: PROPOSAL ONLY.** Every Memex-side change below travels as a liaison PROPOSAL to Memex HITL and is never auto-applied. Push scope stays fathomdb-only. Nothing here is scheduled or committed until the named HITL gate is signed. Governance respected throughout: Memex even micro = required/critical-path, odd micro = OOB/no-deps; FathomDB even = publishable w/HITL, odd = not, pico = label-only, '13' forbidden; ANN/HNSW = 2.x (F-16); shipped query path stays CPU-only/1-bit/deterministic.

Three cross-cutting decisions the overall roadmap turns on are answered inline where they land, and consolidated at the end: **OPP-12 ratification (G-3)**, the **2MM near-term premise (G-1)**, and the **#31 QID cross-source gate (G-4)**.

---

## 0.5.1 — FathomDB 0.8.x storage-API refit (Option B)

- **CURRENT scope:** Substantially shipped; HOLD before Phase 5 (consume) + Phase 6 (behavioral-equivalence gate B-1). Pinned to editable FathomDB build `ba80866d`; A-1 (`$.action_kind` server-side) and Cause-A `stable_id` wired but inert; shipping client-side `action_kind` split as the default fallback.
- **PROPOSED change:** **Resume Phases 5–6 now via a re-pin.** FathomDB origin/main is already ≥ `1137c572` (0.8.11.2 merged) — the dependency this HOLD waited on has been live on origin the whole time; the stall is a missing delivery signal, not missing work. Re-pin the editable build to origin/main, run capability probes for **A-1** (`$.action_kind`; correct hash on main = `9a46611b`), **A-2** (bool-eq server-executable in `read.list`), and **`stable_id`**; flip A-1 to server-side; wire `stable_id`. Then close Phase 5, and gate Phase 6/B-1 on the cutover-risk item below.
- **New blocking rider on B-1 (M20):** Before B-1 closes, each of the five cutover-design substrate risks — Python-SDK production-grade, **aarch64/Jetson**, `append_only_log` at scale, **single-writer vs Memex's TUI+service two-process architecture**, and multi-table join-query expressiveness — must be cited-as-resolved or placed on the next FathomDB↔Memex sync agenda. Single-writer/two-process is architectural and can invalidate the full-cutover destination itself; it is not a residual note.
- **WHY:** Overall roadmap names this "the joint product's proof point," stalled on an already-met dependency (**M16 ACCELERATE**, **M20 VERIFY-OR-SCHEDULE**). Consumes what FathomDB already shipped rather than waiting on new builds.
- **PARITY CHECK:** 0.5.1 even = required/critical-path. ✅ Consuming a met dependency, not adding one.
- **CROSS-PRODUCT DEPENDENCY:** FathomDB **origin/main ≥ `1137c572`** (already live) for A-1/A-2/`stable_id`. No future FathomDB version blocks Phase 5–6; only the M20 risk verify-or-schedule gates B-1.
- **[HITL — Memex]** Approve re-pin + resume. **[HITL — joint]** M20 risk disposition before B-1.

---

## 0.5.2 — Value-test harness (OPP-11 foundation)

- **CURRENT scope:** ✅ SHIPPED (PR #101). SUT harness driver, `data_fitness(turn)` signal, no-egress capture with a `source_id` slot for Cause-A.
- **PROPOSED change:** No scope change. **Two annotations only:** (1) Record explicitly that engine-fitness A/B verdicts are **RETRIEVE-only** until the tracked debt is paid — `_full_loop_seam` is still a `NotImplementedError` stub and `data_fitness` is not emitted on all reflect variants; every downstream OPP-11 adoption claim built on this harness inherits that caveat. (2) The `source_id` slot activates via the Cause-A Stage-1 probe (G-6) below, not automatically.
- **WHY:** Overall roadmap carries this caveat on all value-test claims so OPP-11 adjudication isn't over-read as full-loop fitness. No misalignment beyond the honesty annotation.
- **PARITY CHECK:** 0.5.2 even = required. ✅ Shipped as such.
- **CROSS-PRODUCT DEPENDENCY:** None new. `source_id` → Cause-A `stable_id` (on origin/main; probe-gated per G-6).

---

## 0.5.3 — Architectural cruft refactor (OOB)

- **CURRENT scope:** ✅ SHIPPED (PR #104). 38/61 findings; behavior-preserving; no FathomDB dependency (FathomDB-joint items were deliberately split to 0.5.3.1 to keep bare 0.5.3 dependency-free).
- **PROPOSED change:** None. Confirm the split held.
- **WHY:** Already aligned — the deliberate 0.5.3-vs-0.5.3.1 split is exactly the parity discipline the overall roadmap wants preserved.
- **PARITY CHECK:** 0.5.3 odd = OOB/no-deps. ✅ This is the canonical example of the rule — the reason the external-dependency work was pushed to 0.5.3.1.
- **CROSS-PRODUCT DEPENDENCY:** None.

---

## 0.5.3.1 — FathomDB-joint deferred workstream

- **CURRENT scope:** Placeholder/unscoped; hard-blocked on "FathomDB ~0.8.15." Carries FTS value-tests + ranking/coverage drift, the FTS recovery decision, and op-store latest-state recovery.
- **PROPOSED change (M8 + M15):** **Re-key the gate from a version label ("~0.8.15") to named capability probes.** FathomDB consumption is capability-probe-gated, not version-string-gated (0.8.11.x picos never bump the floor). Split the two halves by disposition:
  - **FTS drift half → MEASUREMENT ONLY.** The 0.8.x FTS-extension door is closed (R-I4, HITL 2026-06-30): no multi-field FTS, no custom tokenizers, no per-column BM25 weights in the 0.8.x line. This workstream measures recall/coverage + ranking drift and **records** gaps; a proven unrecoverable miss produces a **≥0.9.x** ask through HITL — nothing sooner. Do not frame the extension option as "still open/joint."
  - **op-store `read.state` half → gated on the actual `read.state`/latest-state verb** being present in a probed build, and only justified if the 0.5.3.1 latency measurement shows the client-side log-collapse workaround inadequate.
- **WHY:** Overall roadmap M8 (re-key to capability probes) + M15 (FTS = measurement-only). Prevents the "~0.8.15" label from acting as a false hard block and correctly bounds the FTS ask. Ties to the 0.8.15 Slice-0 agenda item that states which §10–14 adjacencies actually ride that window.
- **PARITY CHECK:** The `.1` suffix is correctly NOT the odd/OOB identifier — this is required work with an external dependency, deliberately kept off the 0.5.3 odd-micro contract. ✅ Preserve that distinction.
- **CROSS-PRODUCT DEPENDENCY:** FathomDB **0.8.15** window for the op-store `read.state` verb (if scoped there); the FTS half needs no FathomDB build (measurement against the existing governed FTS surface). The extension escape hatch, if ever triggered, is **≥0.9.x**.
- **[HITL — Memex]** Approve the re-key + the measurement-only reframe. **[HITL — joint]** Whether `read.state` rides 0.8.15 (stated at 0.8.15 Slice-0).

---

## 0.5.4 — Fathom-gauntlet decompositions + behavior-model gates

- **CURRENT scope:** PARKED (awaits HITL kickoff). 18 findings in 4 gauntlet groups + 6 decision gates. Gate ② (liveness/version) and typed-vs-dict CRUD RATIFIED 2026-07-02; 4 round-3 gates STILL OPEN (scoring-golden re-pin, provenance read-surface, plan-exec state, session-lifetime). No FathomDB dependency.
- **PROPOSED change (M12):** **Partial launch now.** Kick off the gauntlet groups that do **not** touch the 4 open round-3 gates; hold the remainder until those gates ratify. Resolves the tail-INDEX-vs-plan-doc tension (INDEX says "Launchable now"; plan doc shows 4 gates open) by making the split explicit rather than treating 0.5.4 as monolithically blocked or monolithically clear.
- **WHY:** Overall roadmap M12 — unblock the gate-independent work immediately so 0.5.4 completion (which hard-unblocks 0.5.5-A) isn't held hostage to the round-3 gate schedule. Internal-only work; no cross-product cost.
- **PARITY CHECK:** 0.5.4 even = required/critical-path. ✅ No external dependency introduced by partial launch.
- **CROSS-PRODUCT DEPENDENCY:** None on FathomDB. **Note the OPP-12 coupling:** Gate ②'s ratified "single canonical liveness predicate" and "provenance links store ids-only, resolve at read" decisions must stay consistent with OPP-12's engine-owned liveness axes and typed `SearchHit.id` — flagged so 0.5.4's liveness modeling and the OPP-12 contract don't diverge.
- **[HITL — Memex]** Approve partial-launch split + resolve the 4 round-3 gates on their own track.

---

## 0.5.5 track A — Legacy convergence + runtime entity schema-registry

- **CURRENT scope:** PARKED-planned. 5 convergence findings + entity schema-registry build (runtime schema-registry entity facades, greenlit over build-time codegen). Binding gates R-A (`EntityTypeSpec` persist + boot-reload) and R-B (generic entity+EAV read projection, the CR-015 keystone). No FathomDB dependency; write-side proceeds unaffected (OPP-12 DECOUPLE placement).
- **PROPOSED change (M13 + M14):**
  - **M13 — adopt the HARD reading of the 0.5.4 dependency:** 0.5.5-A waits on the 0.5.4 gates that touch track-A scope (Gate ②, typed-vs-dict CRUD). Resolve the plan-doc "typically 0.5.4 first" (soft) vs tail-INDEX "Blocked on 0.5.4 gates" (hard) disagreement in favor of the hard reading; state it precisely in plan-0.5.5.
  - **M14 — add a read-only cross-review checkpoint:** Before R-A **freezes** the persisted `EntityTypeSpec` schema, review it against OPP-12's `ProjectionSpec` field set (C-1 makes `EntityTypeSpec` drive `ProjectionSpec` in one registration flow). This is a read-only checkpoint, **no build dependency** — the HITL breaking-waiver bounds the downside if they later diverge, but a cheap look now avoids a two-registry sync problem.
- **WHY:** Overall roadmap M13 (label-corrected hard reading) + M14 (SEQUENCE the R-A freeze against the OPP-12 contract). The C-1 requirement means these two registries are contractually the same declaration flow; freezing `EntityTypeSpec` blind to `ProjectionSpec` reintroduces the CR-057 denormalization drift OPP-12 exists to kill.
- **PARITY CHECK:** 0.5.5 is odd = OOB/no-deps. Track A has **no FathomDB dependency** (registry is Memex-internal; DECOUPLE placement keeps write-side moving). ✅ The M14 checkpoint is read-only and adds no external build dependency, so the odd-micro contract holds.
- **CROSS-PRODUCT DEPENDENCY:** None hard. Soft co-design coupling to the **OPP-12 contract** (design-level, ≥0.9.x build) via C-1 — a checkpoint, not a blocker.
- **[HITL — Memex]** Approve hard-reading label + the R-A `ProjectionSpec` cross-review checkpoint.

---

## 0.5.5 track B — Decomposer / retrieval-quality / cross-source-bench

- **CURRENT scope:** IN FLIGHT (HITL-ratified 2026-07-02, additive). Decomposition RETAINED opt-in (no conversational-domain lift); L1 CE-rerank adopt-candidate GATED PER-CLASS; L2/L3 DROPPED. **#31 QID entity-linking + re-probe is THE GATE** before any bench build; Slice-A probe was NO-GO on the existing corpus (5/15 pairs genuinely disjoint). #30 corpus acquisition and #27/#32 housekeeping queued.
- **PROPOSED change (M16/M17 confirmations; no re-scope):**
  - Confirm **#31 stands exactly as THE GATE** — bench slices B→C→D do not proceed until re-probe shows non-zero cross-source edge-density/giant-component. This is correct evidence-gating (G-4); preserve it, do not pre-build the bench.
  - **M17 confirmation:** FathomDB's corpus-schema bump (`source_type` 6→8 + additive `entity_ids`) and WEC-Eng acquisition are now **origin-visible** — #30/#31 may pin `corpus.manifest_sha256` against origin/main now (previously blocked on "may not be pushed to origin yet"). The `tests/corpus/README.md` 6→8 staleness fix lands docs-only on fathomdb main.
  - **CE-blend adoption caveat (G-5):** The retrieval-quality plan homes the `ce_blend_enabled` flip at "FathomDB 0.8.14," but the `0.8.14-gpu-rerank` branch (`ce_blend_enabled`, `embed_batch_cls`) is currently **orphaned — no home in any FathomDB plan** (plan-0.8.14 has zero rerank scope; F-1 forbids silent scope-add). **Do NOT plan CE-blend adoption against "0.8.14" until FathomDB answers G-5** (admit to 0.8.14 or re-home to 0.8.16, recommended). The worst outcome is the silent one — Memex assuming 0.8.14 ships it.
  - **Enrichment-location decision:** whether QID linking lives durably in fathomdb (`enrich_entities.py`) or ephemerally in the Memex probe is an open sub-decision that #31 forces; it determines who owns the QID-linking artifact going forward.
- **WHY:** Overall roadmap G-4 (#31 correctly evidence-gated) + M17 (origin-visibility unblocks manifest pinning) + G-5 (gpu-rerank orphan must not be silently assumed). Ties to FathomDB corpus-schema work already on origin/main and to the unresolved gpu-rerank home.
- **PARITY CHECK:** 0.5.5 odd = OOB/no-deps. Track B is additive research; its FathomDB dependencies (schema bump, WEC-Eng) are **already merged**, so no forward-build dependency is created — the odd-micro contract holds. Corpus acquisition (#30) happens **in the fathomdb repo on an isolated branch, coordinated with its Steward, never pushed to its active main** by Memex-side agents.
- **CROSS-PRODUCT DEPENDENCY:** FathomDB **origin/main** (schema 6→8 + `entity_ids` + WEC-Eng, already live). CE-blend consumption waits on **G-5** resolution (0.8.14 vs 0.8.16). Enrichment-location is a joint decision, no version attached.
- **[HITL — joint]** G-5 gpu-rerank home (blocks CE-blend adoption planning). **[HITL — joint]** enrichment-location (fathomdb-durable vs memex-ephemeral). **[Evidence gate, not HITL]** #31 re-probe result gates bench spend.

---

## Cross-cutting: Cause-A pico (standalone OOB)

- **CURRENT scope:** OPP-11 resolved Cause-A as a standalone OOB **pico** (`x.y.z.p` label revived, N-6), "size-it-first." But OPP-12's converged design says the SearchHit identity fix "lands-together" with the anonymous-node surrogate minting as part of the ≥0.9.x breaking pair. **Two placements, unreconciled.**
- **PROPOSED change (M4 — probe-then-retire):** Stage the mechanism explicitly:
  - **Stage 1** = the additive `stable_id` **already shipped on origin/main (0.8.11.2).** Run the **G-6 sufficiency probe** (small Memex probe, liaison PROPOSAL): does shipped `stable_id` suffice for OPP-11 hit-level data-fitness attribution? **PASS → retire the standalone pico as discharged** (the ≥0.9.x pair carries only the typed-newtype subsumption). **FAIL → the pico stays the live, now-evidence-backed interim vehicle.** Never retire-then-probe.
  - **Stage 2** = the typed `SearchHit.id {space,value}` subsumption + anonymous-node surrogate minting → **rides the OPP-12 ≥0.9.x breaking pair.** One mechanism, one work item on both ledgers — not double-scheduled.
- **WHY:** Overall roadmap M4. Cause-A appears as a separate ask across OPP-1/9/11 and is mechanically identical to OPP-12's typed identity — the biggest double-counting risk in the portfolio.
- **PARITY CHECK:** Pico = label-only/never-published, OOB — correct vehicle for a standalone substrate fix. The Stage-2 subsumption is breaking → correctly deferred to the coordinated ≥0.9.x pair.
- **CROSS-PRODUCT DEPENDENCY:** Stage 1 = FathomDB origin/main (live). Stage 2 = FathomDB **≥0.9.x** (OPP-12 pair).
- **[HITL — Memex]** Approve the G-6 probe before any retire decision.

---

## OPP-12 ratification — what Memex must agree to, by when

**This is the single governance close-out with the widest fan-out (G-3).**

- **What Memex must do (two concrete actions):**
  1. **Post an `agree` at seq-12** in `memex/dev/fathomdb/enum-discussion-ledger.jsonl` (or post objections). The thread is at seq-11, materially converged, FathomDB-HITL-approved-pending-Memex. FathomDB will not edit the Memex repo.
  2. **Apply the prepared replacement prose** (`OPP-12-leverage-ledger-update.md`) into `memex/dev/fathomdb/LEVERAGE-OPPORTUNITIES-LEDGER.md` line 93, which is **stale** (still shows `DISCUSSING`).
- **What agreement commits Memex to (the frozen, adversarially-reviewed contract):** engine-owned three-axis lifecycle (existence/admission, version-currency via the **already-shipped** `superseded_at` G0 index — NOT a new `is_latest` column, temporal-validity extending edge `t_valid`/`t_invalid` to nodes); mechanism(FathomDB)/policy(Memex) seam; **C-1** — Memex's `EntityTypeSpec` drives the engine `ProjectionSpec` in one registration flow (HARD requirement — couples directly to 0.5.5-A's R-A); **C-2** — `SearchHit.id` becomes a typed `{space,value}` newtype; **+3 verbs** (`transition`, `purge`, `configure_projections`) / **+5 governed types**; **op-store liveness stays OUT of scope** (`__memex_deleted__` remains a permanent op-store convention — resolves 2 of 3 CR-056 encodings); `undelete` naming (never `restore`); Memex deletes its 3 hand-rolled not-live encodings (minus op-store) and stops re-deriving liveness client-side in `fathom_store.py`.
- **By when:** **ASAP — one message** (G-3). This is the gate with the most downstream items waiting.
- **Critical framing (M5 + M6):** **Ratification schedules NOTHING.** The build is ~90% net-new engine work with a dedicated steward/HITL scheduling call, likely a coordinated breaking **FathomDB-0.9.x ↔ Memex-0.5.x pair**. "Ratified ≠ scheduled ≠ cheap." Nothing schedules against OPP-12 until seq-12 exists (M5); if it stalls, only the M7 verb-consolidation and the M14 checkpoint revert to watch-status — Memex 0.5.5-A write-side proceeds regardless (DECOUPLE), and FathomDB's §14 physical-purge commitment survives independently.
- **[HITL — Memex]** Post seq-12 `agree` (or objections) + apply prose text. **[HITL — Memex]** OPP-11 final SIGN travels in the same bundle (all axes resolved; feeds every adoption gate).

---

## The 2MM near-term premise (G-1) — Memex's answer

**The overall FathomDB 2MM vector-latency gate turns entirely on one question Memex must answer: is 1–2M chunks a near-term corpus target for Memex?**

- **Memex-side read: almost certainly NO for the near term** — consistent with the roadmap's default. Memex is a single-user, local-first agent memory (runs on Jetson/Tegra; the OPP-12 op-store framing is explicitly "latency, not corruption, at single-user-local scale"). Chunk accumulation is turn/message/document-paced: even heavy multi-year personal use lands in the low-hundreds-of-thousands, well under the ~50k→ (and far under 1M) crossover the published roadmap assumes. The current cross-source-bench corpus is ~9,135 docs / 8 source types — three orders of magnitude below 1M.
- **The one caveat that could flip it:** the new `kb` source_type (ingesting external knowledge bases) or bulk external-corpus ingestion could inflate a single store past 1M independent of conversational pace. And **Hermes/OpenClaw trajectories are not Memex's to answer** — they need their own projection scout.
- **What this means for the gate:** Memex should supply its **in-repo corpus-size projection** as the G-1 input package (cheap scout), with the honest default read = **NO, not near-term**. This lets FathomDB hold the published roadmap (ANN stays 2.x/F-16, 0.8.20 stays deps-only, CPU-only/1-bit/deterministic invariant untouched) while still banking the minimal $0 E1 overhead-attribution bench so a later YES costs only a slot, not lost measurement.
- **The unowned instrumentation flag (M9):** E-A2 filter-rate telemetry (global/unscoped-query fraction on real consumer traffic) is **unowned work** — if HITL wants it as a G-1 input rather than deciding on projections alone, it needs a **named Memex owner and slot** via liaison PROPOSAL, or an explicit HITL waiver to decide on projections only.
- **[HITL — joint]** G-1 premise call (default NO). **[HITL — Memex]** whether Memex owns the E-A2 filter-rate telemetry, or HITL waives it.

---

## The #31 QID cross-source-bench gate (G-4)

Covered under 0.5.5-B above; restated as a standalone gate because the overall roadmap treats it as portfolio-level:

- **#31 (QID entity-linking + re-probe) is THE GATE** — the bench build (slices B→C→D: join+composed index → STaRK-modeled question generator → answer-F1 with distraction on/off) proceeds **only if** the re-probe shows non-zero cross-source edge-density/giant-component after QID linking. The first probe was NO-GO on the existing corpus.
- **Correctly evidence-gated — preserve it.** Do not pre-build the bench (mirrors OPP-1's anti-abstraction stance). #30 corpus acquisitions (CompMix, MultiHop-RAG, S2ORC/STaRK-MAG) retain standalone eval value even if #31 fails again; fall back to synthetic-bridge path (c) only if HITL forces it.
- **Enrichment-location** (fathomdb-durable `enrich_entities.py` vs memex-ephemeral probe-time) is the open sub-decision #31 forces.
- **[Evidence gate]** #31 re-probe result — not a HITL call, but gates all bench spend. **[HITL — joint]** enrichment-location ownership.

---

## HITL agreement — consolidated flag list

| Item | Gate | Owner | Urgency |
|---|---|---|---|
| Post OPP-12 seq-12 `agree` + apply stale prose-ledger text | G-3 | Memex HITL | ASAP, one message |
| OPP-11 final SIGN (bundle with G-3) | — | Memex HITL | With G-3 |
| G-1 2MM premise call + Memex corpus-size projection (default NO) | G-1 | Joint HITL | Before FathomDB 0.8.14 Slice 0 |
| E-A2 filter-rate telemetry: name a Memex owner or HITL-waive | M9/G-1 | Memex HITL | With G-1 |
| Cause-A Stage-1 sufficiency probe (probe-then-retire) | G-6 | Memex probe | Before retiring the pico |
| gpu-rerank home (blocks CE-blend adoption planning) | G-5 | Joint HITL | FathomDB 0.8.14 Slice 0 |
| 0.5.1 re-pin + resume Phase 5–6 | M16 | Memex HITL | Now |
| M20 cutover-risk verify-or-schedule (5 risks) before B-1 closes | M20 | Joint sync | Before 0.5.1 B-1 |
| 0.5.3.1 re-key to capability probes + FTS = measurement-only | M8/M15 | Memex HITL | Before 0.5.3.1 start |
| 0.5.4 partial-launch split + 4 round-3 gates | M12 | Memex HITL | Now (partial) |
| 0.5.5-A hard-reading label + R-A `ProjectionSpec` checkpoint | M13/M14 | Memex HITL | At 0.5.5-A kickoff |
| #31 enrichment-location (fathomdb vs memex) | G-4 | Joint HITL | With #31 |

**Governance invariants preserved:** no new FathomDB verb proposed here beyond the OPP-12 +3 (worst-case +5 with op-store `read.state`); no FTS extension / custom tokenizer / per-column BM25 (R-I4 door closed for 0.8.x); Memex odd/even micro contract intact (0.5.3/0.5.5 odd = no forward-build deps; 0.5.1/0.5.2/0.5.4 even = required; 0.5.3.1 keeps its non-odd identifier for required-with-external-dep work); Cause-A/OPP-12 identity treated as **one** mechanism, never double-scheduled.
