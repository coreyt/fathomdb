---
title: FathomDB roadmap adjustments to align with the overall roadmap
date: 2026-07-03
status: PROPOSAL — awaiting HITL review (session "fathom-2MM-plan-refactor-0.8.x")
desc: Per-version (0.8.14–0.8.20 + 0.9.x/2.x) KEEP/INSERT/SHIFT/DEFER proposal with governance checks.
generator: model-tiered dynamic workflow (Sonnet-5 collect · Fable-5 analyze/critique/synthesize · Opus align); 11 agents, 0 errors
---

# FathomDB Roadmap Adjustment Proposal — 0.8.x Line Alignment to the Unified Roadmap

**Status: Steward PROPOSAL for HITL.** Nothing here is scheduled until HITL signs the named gate. Memex-side items are liaison PROPOSALS only (push scope: fathomdb repo). Governance invariants held throughout: even=publishable-w/HITL, odd=not, pico=label-only, `13` forbidden; **HNSW/ANN stays 2.x (F-16)**; scale gate rides **F-17** (0.9.0 soft→0.9.3 stated→1.0.0 exit-beta→1.1.0 hard); shipped query path stays **CPU-only / 1-bit / deterministic**; governed-verb baseline **16 → ~19 worst-case +5**.

**Headline:** The 0.8.x *ladder itself does not change.* Every real build stays exactly where it is. What changes is (a) three docs-only reconciliations, (b) two evidence gates that must clear *before* 0.8.14 Slice 0 or the release ships as-specified, (c) one orphaned branch that needs a home, and (d) explicit placement of OPP-12 at ≥0.9.x and E1 as a no-slot bench. HNSW is not pulled forward. The kernel is admitted to 0.8.14 only under strict droppability conditions, else it becomes a later standalone even-micro.

---

## 0.8.14 — Substrate & recall (EXP-S + BM25F/F5)

| Field | Detail |
|---|---|
| **CURRENT scope** | #2 EXP-S kind-tagged coexisting-index substrate (KEYSTONE, Slice 5) + #16 F5 fielded FTS/BM25F (Slice 10), one coordinated `SCHEMA_VERSION` bump. `#17` already struck (shipped 0.8.11). The program's long pole (I-2 → 0.8.15, one-for-one slip). F-1: resist all scope adds. |
| **PROPOSED change** | **KEEP** the specified scope as the default. **CONDITIONAL-INSERT (≤1 slice):** admit exactly one packed/SIMD exact-scan kernel slice **only if** `G-1=YES ∧ G-2=overhead-bound` clear **by Slice 0**, *and* the packed-code store is derived/rebuildable (like FTS/vec0) with **zero `SCHEMA_VERSION` coupling** (that is what makes it genuinely droppable at Slice 20 without breaking the one-bump commitment). If those conditions don't hold: kernel does NOT enter — it becomes a later standalone even-micro (M2/M3). **Also KEEP-as-decided:** the gpu-rerank home question (G-5) — see below. |
| **WHY** | Unified roadmap P0: 0.8.14 is the long pole everything on I-2 rides; F-1 forbids silent scope. The 2MM minimum-divergence path (M2) says the kernel's only cheap home is EXP-S's coexisting-index substrate (the packed store *is* another index-kind), but only under real droppability. IVF/ANN spike (E2/E3) explicitly does **not** enter here — stays 2.x contingency (M1). |
| **GOVERNANCE** | Numbering OK (even, publishable-w/HITL). **F-16 OK** — no ANN here; kernel is exact-semantics, identical results, CPU-only/1-bit/deterministic invariant preserved. **F-17 OK** — no scale claim. **API-sprawl OK** — kernel is internal, zero new verbs. One-`SCHEMA_VERSION`-bump commitment protected by the zero-coupling admissibility rule. |
| **HITL SIGN-OFF** | **YES — three gates before Slice 0:** G-1 (2MM premise), G-2 (E1 overhead attribution), G-5 (gpu-rerank home). Default on non-answer for G-1/G-2 = **NO kernel** → ship as-specified. eu7 ≥0.90 one-sided-CI hard gate at Slice 20 if vectors touched (unchanged). |

---

## 0.8.15 — In-library dispatcher / router (EXP-Fr)

| Field | Detail |
|---|---|
| **CURRENT scope** | Two governed verbs `route_recommend()` + `search_routed()`; 5-class taxonomy → F1–F5; rule-based (not LLM), CPU-only default; `search()` byte-identical. Hard-gated I-2 on EXP-S (0.8.14) GO. §10–14 are adjacency pointers, NOT this release. |
| **PROPOSED change** | **KEEP** the build. **INSERT into Slice-0 agenda (no new build scope):** (1) F-8b `record_feedback` reclassification must resolve here (also DP-0.17-B); (2) explicit disposition of §10–14 adjacencies — decide which (if any) ride this window vs. move under the OPP-12 lifecycle channel (M7), so Memex stops version-label-gating on the string "~0.8.15." |
| **WHY** | M8/M7: today four Memex asks (§10 FTS, §11 read.state, §12 touch, §13 neighbors) and §14 purge are soft-pinned to "~0.8.15" with no committed slice. Re-keying them to capability probes + routing lifecycle-shaped asks through OPP-12 prevents phantom scope on the dispatcher and stops Memex 0.5.3.1 from blocking on a version label (M15). OPP-3 `margin` (V-7) lands here → unblocks cascade re-measurement on real turns. |
| **GOVERNANCE** | Numbering OK (odd — but a REAL engine build, not OOB; correctly documented). +2 verbs are the dispatcher's own committed surface, not lifecycle sprawl. **F-16/F-17 OK.** API-sprawl guarded: §12 touch becomes a projection-registry entry not a verb; §14 purge ≡ OPP-12 `purge` (never two verbs). |
| **HITL SIGN-OFF** | **YES:** locus decision (in-library vs agent-side vs both-layered) — the release's first HITL call at Slice 0; route-accuracy AC from EXP-Fr-acc; F-8b resolution. Adjacency-disposition is Steward+HITL at Slice 0. |

---

## 0.8.16 — Ranking signal (F9) + embedder reach (ONNX)

| Field | Detail |
|---|---|
| **CURRENT scope** | #15 F9 importance/confidence (KEYSTONE) + #4 cross-vendor `OrtBgeEmbedder` (ort: CUDA/ROCm/DirectML/OpenVINO/CPU) via `EmbedderChoice::Caller`, zero engine diff. Slice-15 candle↔ONNX Δ measurement feeds 0.8.18 #5 tolerance calibration. |
| **PROPOSED change** | **KEEP** F9 + ONNX unchanged (the unified roadmap explicitly marks this release zero-divergence). **CONDITIONAL-INSERT (G-5):** if HITL re-homes the orphaned `0.8.14-gpu-rerank` branch (`ce_blend_enabled` flip, `embed_batch_cls`) here — this is the **recommended** home ("ranking signal & embedder reach" is the natural fit). **RECONCILE (docs-only, M18):** make edge **I-7** machine-visible in the master edge table: 0.8.16 Slice-15 Δ → 0.8.18 #5 calibration (physically hard). **NOTE:** F9 completes OPP-12's `rankable` signal algebra — a *contract input*, not a build trigger for OPP-12. |
| **WHY** | M19/G-5: the gpu-rerank branch is rebased/green/unmerged with **no home in any plan** and Memex's retrieval-quality plan homes CE-blend at the string "0.8.14" — an unacknowledged F-1 collision. 0.8.16 is where embedder/ranking work legitimately lives. M18: the cross-release Δ→calibration coupling is real but absent from I-1..I-6. |
| **GOVERNANCE** | Numbering OK (even). **F-16/F-17 OK.** ONNX deliberately scheduled non-early because it manufactures the cross-backend divergence hazard 0.8.18 #5 exists to catch. API-sprawl OK — ONNX is `EmbedderChoice::Caller`, zero engine diff; gpu-rerank flip is a config flag, not a verb. |
| **HITL SIGN-OFF** | **YES if G-5 lands here** (admit gpu-rerank as explicit 0.8.16 scope). Otherwise no new sign-off beyond the existing eu7 ≥0.90 embedder/index-touch gate. |

---

## 0.8.17 — Router hardening / forks (OOB)

| Field | Detail |
|---|---|
| **CURRENT scope** | Track A (config-tuple registry, forbidden-composition validator) + Track B (EXP-AF GO/NO-GO in code) = backbone; Track C (EXP-C/D/E forks) reserved-gap, corpus/spend-gated, non-blocking. Runs **after 0.8.18** (needs the publish pipeline for its Slice-40 dry-run). |
| **PROPOSED change** | **KEEP** entirely. No scope change. |
| **WHY** | Unified roadmap P2 places 0.8.17 after 0.8.18 per the documented numeric-vs-dependency inversion (now promoted to the master edge list under M10). Track C stays corpus/spend-gated — correctly deferred, non-blocking to Slice-40 closure. No misalignment touches this release's scope. |
| **GOVERNANCE** | Numbering OK (odd/OOB). **F-16/F-17 OK.** The out-of-numeric-order run (after 0.8.18) is explicitly documented, not a violation. API-sprawl OK (registry + validator, no new governed verbs). |
| **HITL SIGN-OFF** | **NO new sign-off** beyond existing: DP-0.17-A (EXP-AF go/no-go) + DP-0.17-B (`record_feedback` state, now pulled forward to resolve at 0.8.15 Slice 0); DP-0.17-C (EXP-D spend) + DP-0.17-D (Fork-E) remain HITL but non-blocking. |

---

## 0.8.18 — Production-safety & CI hardening capstone (release-engineering GA ONLY)

| Field | Detail |
|---|---|
| **CURRENT scope** | #5 vector-equivalence self-check (KEYSTONE, tolerance calibrated against 0.8.16 Slice-15 Δ) + #11-full publish pipeline + the real HITL-gated `v*` tag. `#13` struck → moved to 0.8.19. Explicitly **release-engineering GA only** — makes NO scale/stability claim (F-17). |
| **PROPOSED change** | **KEEP** unchanged. Formalize incoming edge **I-7** (from 0.8.16, M18) as the hard calibration input for #5. |
| **WHY** | Unified roadmap: zero-divergence release. The critical guard is that "GA" here is **not** a maturity/scale guarantee — the unified roadmap and F-17 both insist the scale bound is staged 0.9.0→1.1.0. Any 2MM/kernel work must NOT be read as making 0.8.18 assertable as scale-GA. |
| **GOVERNANCE** | Numbering OK (even, and the release that cuts the real line-wide `v*` tag). **F-16 OK** — no ANN. **F-17 OK — load-bearing: this release makes no scale-envelope claim; that is the whole point.** API-sprawl OK (self-check probe-set is internal `_fathomdb_embed_probe`, no verb). |
| **HITL SIGN-OFF** | **YES:** the Slice-40 real `v*` tag fires actual crates.io/PyPI/npm publish — separate explicit HITL call. R-VEQ-2 two-sided test is a hard requirement. |

---

## 0.8.19 — Free-threading (EXP-FT) + benchmark/robustness harness (#13, scale.rs)

| Field | Detail |
|---|---|
| **CURRENT scope** | Track A EXP-FT ladder (FT-4 concurrency-safety = HARD GATE); Track B #13 `benches/`, `scale.rs`, `tracing` feature, weekly workflow. End of 0.8.x odd line. FT productization (`gil_used=false` flip) is explicitly NOT in this release. |
| **PROPOSED change** | **KEEP** the build. **DESIGNATE (M3):** `scale.rs` / #13 is the documented long-term home that **absorbs the minimal E1 bench** run in Phase 0. E1 runs *now* as a $0 scratch/label-only bench so its answer beats 0.8.14 Slice 0; it is marked "absorbed by 0.8.19 #13" so it never becomes a second harness. **RECONCILE (docs-only, M10):** reword the stale "#11-full publish matrix DONE @0.8.18 (already satisfied 2026-06-28)" prereq, which is date-inconsistent with the renumbered sequence. |
| **WHY** | Unified roadmap's single most important sequencing insight: E1's natural substrate (`scale.rs`) sits at end-of-line while its answer is needed before 0.8.14. Fix by decoupling — extract minimal E1 now (no slot), let 0.8.19 absorb it later. Prevents inventing a parallel measurement harness. |
| **GOVERNANCE** | Numbering OK (odd/OOB, mostly-$0). **F-16/F-17 OK** — the harness *measures* scale, makes no guarantee. **API-sprawl OK.** FT productization stays a post-0.8.19 steward-elected slot outside the odd line (unchanged). |
| **HITL SIGN-OFF** | **NO new sign-off** for the harness/E1-absorption. FT productization decision remains a Slice-40 HITL readout (unchanged). |

---

## 0.8.20 — Library Sweep (napi 2→3, rusqlite 0.31→0.40 + sqlite-vec)

| Field | Detail |
|---|---|
| **CURRENT scope** | Deferred-with-trigger; Slice-0 mandatory PAUSE-for-HITL timing gate. **F-16: stays deps-only — HNSW/ANN is a 2.x item, NOT scheduled here.** napi 2→3 + rusqlite/sqlite-vec migration + action-gh-release 2→3. |
| **PROPOSED change** | **KEEP deps-only.** **DEFER + ADD agenda item (M11):** 0.8.20 Slice-0 agenda gains one consideration — *"if G-1=YES, the sqlite-vec 0.1.7→0.1.9 migration is a **predecessor** of any future vec0-internal kernel/ANN work, not a competitor for this slot."* This removes the phantom collision where the 2MM roadmap's §5 draft table tried to repurpose 0.8.20 for ANN productization. **RECONCILE (docs-only, M1):** strike ANN from the 2MM roadmap §5 table. |
| **WHY** | M1/M11: the 2MM proposal's first-draft placement pulled ANN to a "contingent 0.8.20," a HIGH-divergence collision with the already-scheduled dependency migration. The minimum-divergence path keeps ANN at 2.x → the collision "simply disappears." The sqlite-vec bump is sequenced as a predecessor, never a repurpose. |
| **GOVERNANCE** | Numbering OK (even; even/OOB classification F-12 reconcile is itself a Slice-0 agenda item — treat as OPEN, not settled). **F-16 OK — deps-only, ANN stays 2.x.** **F-17 OK.** API-sprawl OK (equivalence-preserving migration, no new surface). |
| **HITL SIGN-OFF** | **YES:** Slice-0 is a hard PAUSE — trigger-present-per-migration? publish-vs-label-only? equivalence bar? no collision with in-flight engine release? Do not migrate for novelty. |

---

## 0.9.x — Scale-bound ladder + OPP-12 lifecycle build (Cause-A Stage 2)

- **F-17 scale ladder** (0.9.0 soft → 0.9.3 stated → 1.0.0 exit-beta → 1.1.0 hard) — **KEEP, untouched** by everything above. A future 0.9.x roadmap doc carries it.
- **OPP-12 record-lifecycle-protocol build lands here** — a coordinated **breaking FathomDB-0.9.x ↔ Memex-0.5.x pair**, its own dedicated steward/HITL scheduling call. Design is **CONVERGED (ratification pending Memex `agree`, G-3)** but the build is **~90% net-new** engine work — README gets an explicit *"ratified ≠ scheduled ≠ cheap"* line (M6). Nothing schedules against OPP-12 until seq-12 exists (M5).
  - Verbs: `transition` / `purge` / `configure_projections` = **+3 → ~19 governed verbs**. §14 committed physical-purge verb **is the same verb** as OPP-12 `purge` (never two, M7). §12 touch/last-accessed = projection-registry entry, **not a verb**.
  - **Cause-A Stage 2** rides this pair: typed `SearchHit.id {space,value}` newtype subsumption + anonymous-node surrogate `logical_id` minting — **one mechanism, never two work items** (M4). Stage 1 = shipped additive `stable_id` (0.8.11.2, on origin/main); G-6 probe decides whether the interim pico retires (probe-then-retire, never retire-then-probe).
- **GOVERNANCE:** Numbering OK (≥0.9.x, breaking-OK under HITL migration waiver). API-sprawl bounded: **worst-case +5** (OPP-12's +3, plus §11 op-store `read.state` + latest-state scan *only if* 0.5.3.1 latency measurement proves the client-side workaround inadequate). Identity change *shrinks* the hit struct. Standing denials hold (no multi-field FTS / custom tokenizers / per-column BM25 in 0.8.x; recovery denylist stays five names).
- **HITL SIGN-OFF: YES** — G-3 (Memex ratification, one message) unblocks the ≥0.9.x scheduling call; the build itself gets a fresh scheduling call with a build-cost estimate. Purge timing (§14) may ship forward-compatible pre-0.9.x if HITL wants it, per the transition table.

## 2.x — ANN / HNSW (F-16) + IVF contingency

- **ANN/HNSW stays 2.x (F-16) — KEEP, do NOT pull forward.** Not in the 0.8.x line anywhere.
- **Path C (IVF-via-`centroid_id` partition key)** is the 2.x durability track — gated on E2 (recall/latency sweep) + E3 (filtered-IVF collapse). If ever built, must be **in-engine** (inside sqlite-vec/vec0, not a sidecar) per the E6 rule. RAM-HNSW sidecar and libSQL/DiskANN both demoted.
- **The ONLY reopen path** (written escalation, M4/G-1): `G-1=YES ∧ E1 shows NOT overhead-bound ∧ kernel can't hit budget on global/unscoped queries`. Even then the 0.8.20 sweep is a *predecessor*, not a slot to repurpose. Absent all three, published roadmap stands.
- **GOVERNANCE:** F-16 honored (2.x). F-17 hard scale assert stays 1.1.0 — the Path-A kernel, if it lands in 0.8.14, makes the gate *reachable early* but does not move the hard assertion.
- **HITL SIGN-OFF: YES** to reopen ANN earlier (escalation with E2/E3 designs) — otherwise no action; this is deliberately last.

---

## Where the two contested pieces land (explicit)

- **OPP-12:** design converges now (G-3, one Memex message); **build is ≥0.9.x** as a breaking FathomDB↔Memex pair. It touches **no 0.8.x release scope** — only F9 at 0.8.16 feeds its `rankable` algebra as a contract input. Cause-A Stage 2 is inside this pair.
- **2MM E1 / kernel:** **E1 runs now** as a $0 no-slot bench, later absorbed by **0.8.19** `scale.rs`. **The kernel** rides **0.8.14** *only* under G-1=YES ∧ G-2=overhead-bound ∧ zero-`SCHEMA_VERSION`-coupling; otherwise a later standalone even-micro. **IVF/ANN never enters 0.8.x** — it is a 2.x contingency. Default on non-answer: NO kernel, published roadmap stands.

## Every change flagged for HITL

| Gate/ID | Decision | Blocks |
|---|---|---|
| **G-1** | 2MM premise (is 1–2M near-term?) | 0.8.14 kernel admission; before Slice 0; default NO |
| **G-2** | E1 overhead attribution | any kernel work; before Slice 0 |
| **G-3** | OPP-12 Memex `agree` (seq-12) | OPP-12 ≥0.9.x scheduling; M7 verb consolidation |
| **G-5 (M19)** | gpu-rerank home: admit to 0.8.14 or re-home to 0.8.16 (recommended) | Memex CE-blend adoption path; before 0.8.14 Slice 0 |
| **G-6 (M4)** | Cause-A Stage-1 sufficiency probe | retire vs keep the interim pico (probe-then-retire) |
| 0.8.15 Slice 0 | locus decision; F-8b; §10–14 adjacency disposition | dispatcher build |
| 0.8.18 Slice 40 | real `v*` tag (fires publish) | line-wide release |
| 0.8.20 Slice 0 | trigger / publish-vs-label / equivalence bar / F-12 classification | Library Sweep |
| ≥0.9.x | OPP-12 build scheduling (own call, build-cost estimate) | lifecycle build |
| 2.x | reopen-ANN-early escalation (only if G-1=YES ∧ E1 not-overhead-bound ∧ global-query fail) | ANN pull-forward |

**Docs-only, commit straight to main (no HITL, per "don't gate trivial changes on CI"):** M1 (strike ANN from 2MM §5 table), M10 (plan-0.8.19 prereq wording), M11 (0.8.20 Slice-0 agenda add), M17 (`tests/corpus/README.md` 6→8), M18 (F-11a rename + edge I-7), M21 (annotate memory `1-2M-scaling-kernel-first-not-ann.md`: "A>C>B conditional on E1 + premise, both OPEN"). **Do NOT edit `ann-index-vec0.md` until G-1 resolves.**
