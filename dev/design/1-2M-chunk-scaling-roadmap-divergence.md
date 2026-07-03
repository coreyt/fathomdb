---
title: Divergence — proposed 1–2M scaling roadmap vs the published schedule-of-record
date: 2026-07-02
status: analysis (companion to 1-2M-chunk-scaling-vector-latency-roadmap.md)
desc: >
  Quantifies how far the proposed 1–2M-chunk vector-latency roadmap diverges from the
  published forward schedule (plan-0.8.14 … plan-0.8.20 + F-16 + F-17). Verdict: divergence
  is LOW on architecture/compatibility, MODERATE-to-HIGH on scheduling/placement, and turns
  almost entirely on ONE unresolved premise — whether 1–2M chunks is a near-term target.
---

# Divergence: proposed 1–2M scaling roadmap vs the published roadmap

Companion to `dev/design/1-2M-chunk-scaling-vector-latency-roadmap.md` (the proposal).
This doc measures the gap against the **published schedule-of-record** and says where the
two genuinely conflict versus where they can co-exist.

## What "the published roadmap" is (sources)

The authoritative forward line (post 2026-07-01 renumber), from the plan files + governance findings:

| Slot | Published theme | Scope (plan file) |
|---|---|---|
| **0.8.14** | **Substrate & recall features** — "the schema-migration release" | #2 kind-tagged **coexisting-index substrate** (EXP-S: row-kinds leaf/coverage/graph, "one store, many indexes", incremental multi-index write + determinism check) + #16 **fielded FTS / BM25F** (F5). One coordinated `SCHEMA_VERSION` bump. |
| **0.8.16** | **Ranking signal & embedder reach** | #15 F9 importance/confidence + #4 **cross-vendor ONNX embedder** (`ort`: CUDA/ROCm/DirectML/OpenVINO). |
| **0.8.18** | **Production-safety & CI hardening capstone** | #5 vector-equivalence self-check + #11-full publish pipeline + the **GA tag**. Explicitly *release-engineering* GA; **pre-1.0.0 is beta; scale/stability guarantees staged 0.9.x → 1.1.0 (F-17)**. |
| **0.8.19** | Benchmark & robustness harness | #13: `benches/`, `scale.rs`, `tracing` feature, weekly workflow. |
| **0.8.20** | **Library Sweep (major deps)** | napi 2→3 + **rusqlite 0.31→0.40 + sqlite-vec** migration. **Deferred-with-trigger; Slice-0 HITL timing gate; timing NOT confirmed (F-12).** |
| **2.x** | ANN / HNSW (**F-16**) | Replace the O(N) vec0 scan. **Not in the 0.8.x line at all.** |
| **0.9.x → 1.1.0** | Scale/latency *hard gate* (**F-17**) | 0.9.0 soft → 0.9.3 stated → 1.0.0 exit-beta → 1.1.0 hard. |

**Load-bearing invariant, stated across 0.8.14 and 0.8.18:** *"the shipped query path stays
CPU-only/1-bit/deterministic."* And the 1M latency tier is **measured-but-not-gated on purpose** —
the published line treats 1M-scale latency as a **2.x/0.9.x maturity concern, not a 0.8.x deliverable.**

## Overall verdict

> **Divergence is LOW on architecture/compatibility, MODERATE on the F-17 gate, and HIGH on
> slot placement — but it collapses to "insert one experiment + one perf slice" once you apply
> the proposal's OWN A>C>B ranking. The entire gap hinges on one unresolved premise: is 1–2M a
> near-term target?**

Three things make the divergence smaller than it first looks:

1. **The recommended path (A) is *compatible* with the published invariant.** The packed/SIMD
   exact-scan kernel keeps the query path **exact, 1-bit, deterministic, CPU-only** — it optimizes
   the *existing* path, it does not add an approximate index. So Path A does **not** break the
   "CPU-only/1-bit/deterministic" rule the published line protects.
2. **The divergent part (ANN/IVF, Path C) is explicitly the *deferred contingency* in the proposal.**
   The proposal's own ranking is A > C > B; C only fires if E1/E-A1/E-A2 fail. So the proposal does
   **not** actually demand pulling ANN into 0.8.x for the 1–2M case — it agrees ANN stays deferred
   unless the corpus grows past ~5–10M.
3. **Real architectural adjacency already exists in the published line:** EXP-S (0.8.14) is literally
   "one store, many coexisting indexes" — the substrate a packed-code shadow / IVF-centroid index
   would plug into; 0.8.19 already stands up `scale.rs`/`benches/` — the natural home for the E1/E-A1
   latency measurements; and 0.8.20 already carries the **sqlite-vec** migration the deeper vec0 work
   depends on.

What genuinely diverges: the proposal introduces a **vector-latency-at-scale workstream** into a line
that **deliberately excluded it**, and (in its first-draft placements) collided with two already-claimed
slots and mis-stated the GA/F-17 boundary (now corrected in the proposal doc).

## Per-item divergence

| Item | Published | Proposed | Divergence | Note |
|---|---|---|---|---|
| **Query-path invariant** | CPU-only / 1-bit / deterministic | **Preserved** by Path A (kernel) | **NONE** | Kernel is a perf optimization of the exact path, not a new index type |
| **1M-scale latency posture** | measured-but-not-gated; a 2.x/0.9.x concern | Treat as a **near-term deliverable** IF 1–2M is real | **HIGH (premise-dependent)** | This is the core disagreement; it's a product/HITL premise, not an engineering fact |
| **E1 overhead-attribution experiment** | not on the schedule | Prereq; ride 0.8.14 EXP-S or 0.8.19 `scale.rs` | **LOW** | Cheap; has a natural home in existing substrate |
| **Packed/SIMD exact-scan kernel (Path A)** | not on the schedule | Primary latency fix, proposed at 0.8.14 | **MEDIUM** | New scope, but invariant-compatible; can ride EXP-S's coexisting-index substrate rather than displace it |
| **0.8.14 theme** | EXP-S coexisting-index substrate + BM25F (fully scoped, one schema migration) | Insert kernel + oracle-ext + IVF spike | **MEDIUM-HIGH** | Scope *expansion* / re-theme of a release already fully specified; needs HITL. Synergy: kernel/IVF-index = "another coexisting index" |
| **ONNX embedder (0.8.16)** | #4 ONNX + #15 F9 | **Keep as-is** (+ contingency note) | **NONE** | Proposal explicitly retains it |
| **0.8.18 GA meaning** | release-eng GA; scale gate is F-17 (0.9.x→1.1.0) | (draft) "gate assertable at 1M/2M" → **corrected** to F-17 ladder | **LOW (residual)** | First draft over-claimed; already reconciled in the proposal doc |
| **Benchmark/scale harness (0.8.19)** | #13 `scale.rs`/`benches/` | (not originally leveraged) → natural home for E-A1/E-A2 | **LOW** | Proposal *should* lean on 0.8.19 rather than invent measurement |
| **0.8.20** | napi3 + rusqlite/**sqlite-vec** migration; deferred-with-trigger | (draft) repurpose for ANN productionization | **HIGH (collision)** | Two different owners for one slot; and the sqlite-vec bump is a *dependency* of deep vec0 work, not a substitute for it |
| **ANN / IVF (Path C)** | **2.x (F-16)** | Contingent; (draft) pulled to 0.8.20 | **HIGH if pulled; LOW if left at 2.x** | Proposal's own A>C ranking says C isn't needed at 1–2M ⇒ leaving it at 2.x reconciles cleanly |
| **HNSW/DiskANN sidecar** | eventual 2.x default | Demoted below in-engine IVF | **LOW/semantic** | Both defer it; proposal just reranks the eventual flavor |

## The three axes of divergence

1. **Premise (HIGH, unresolved).** The published line assumes 0.x/1.x consumers run **well under the
   ~50k crossover** (`ann-index-vec0.md`), so 1M latency is a 2.x/0.9.x concern. The proposal assumes
   **1–2M is a near-term target**, which *falsifies* that assumption. **Neither is an engineering
   question — it's a product decision the HITL must settle.** Everything downstream depends on it.
2. **Placement (MEDIUM-HIGH, reconcilable).** First-draft placements collided with 0.8.14 (fully-scoped
   schema-migration release) and 0.8.20 (deferred library sweep). Both collisions are avoidable — see
   the minimum-divergence path below.
3. **Gate policy (LOW, already corrected).** The draft implied 0.8.18 GA would assert the scale gate;
   the ledger reserves that for F-17 (0.9.x→1.1.0). The proposal doc now states this correctly.

## Minimum-divergence reconciliation (recommended)

Applying the proposal's **own** A>C>B ranking to the published ladder shrinks the divergence to a few
additive moves that mostly **respect** the existing schedule:

- **Keep ANN/IVF at 2.x (F-16).** Do *not* pull to 0.8.20. The A>C ranking says the kernel, not an
  index, is the 1–2M fix — so the biggest divergence (0.8.20 collision + defer-ANN break) simply
  **disappears**.
- **Keep the F-17 scale ladder unchanged.** The hard 1M/2M gate stays 0.9.x→1.1.0; the kernel just makes
  it *reachable* early. No change to 0.8.18's GA meaning.
- **Run E1 as a cheap experiment** on the 0.8.19 `scale.rs`/`benches/` substrate (or as an EXP-S
  determinism-adjacent probe in 0.8.14). No new slot.
- **Land the packed/SIMD kernel as an invariant-preserving perf slice** — either riding 0.8.14's
  coexisting-index substrate (the kernel's contiguous-code store *is* another index-kind) or as a small
  perf micro on the odd line. It keeps CPU-only/1-bit/deterministic, so it does not fight the published
  invariant.
- **Sequence after / with the 0.8.20 sqlite-vec migration** where the work touches vec0 internals — the
  published bump is a *predecessor*, not a competitor.

Under this reconciliation the residual divergence is: **(a) accept 1–2M as a near-term target (premise,
HITL), (b) insert E1 + a kernel perf slice into the existing 0.8.14/0.8.19 substrate, (c) leave ANN and
the F-17 gate exactly where they are.** That is a **small** delta — not a roadmap rewrite.

The **large** divergence only materializes if the HITL both (i) confirms 1–2M as near-term **and**
(ii) E1 shows the scan is *not* overhead-bound or global-unscoped queries dominate — at which point ANN
(Path C) is genuinely forced forward from 2.x, colliding with the deferred library line. That is the
scenario the proposal flags as needing the biggest schedule change, and it is **evidence-gated**, not
assumed.

## Bottom line

- **Architecture/compatibility divergence: LOW.** The recommended fix (kernel) honors the published
  CPU-only/1-bit/deterministic invariant; ANN (the divergent part) is a deferred contingency both plans agree to park.
- **Scheduling divergence: MEDIUM now, LOW after reconciliation.** The only hard collisions (0.8.14 re-theme,
  0.8.20 repurpose) vanish once the proposal's own A>C>B ranking is applied to the existing ladder.
- **Premise divergence: HIGH and unresolved.** Whether 1–2M is near-term is the single fork. If NO, the
  published roadmap is correct and the proposal is premature. If YES, the published line has a real gap
  (no vector-latency-at-scale workstream before 2.x) that the minimum-divergence path fills cheaply.
- **Net:** this is **not** a competing roadmap. It is a **premise challenge + one cheap experiment (E1) +
  an invariant-preserving perf slice**, with a genuinely divergent ANN branch that stays **evidence-gated
  at 2.x** unless the premise and E1 both push it forward. **Settle the 1–2M premise with the HITL first;
  everything else follows from E1.**
