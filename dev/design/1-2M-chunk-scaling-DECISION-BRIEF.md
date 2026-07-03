---
title: DECISION BRIEF — 1–2M-chunk scaling & the 0.8.x plan-refactor path
date: 2026-07-02
status: OPEN — awaiting HITL decision (session "fathom-plan-refactor-path-0.8.x")
desc: >
  Single resume-point for the 1–2M-chunk vector-latency work. Ties together the roadmap
  proposal and the divergence analysis, states the ONE decision that unlocks everything,
  and lists the contingent next actions. Read this first when you come back.
---

# DECISION BRIEF — scaling FathomDB vector retrieval to 1–2M chunks

**Read this first.** It's the resume-point for the session. Two supporting docs + a memory
entry hold the detail; this brief holds the decision.

## Artifacts produced this session

| File | What it is |
|---|---|
| `dev/design/1-2M-chunk-scaling-vector-latency-roadmap.md` | The full proposal: ranked paths (A>C>B), roadmap scaffold, experiments (E1–E6) with decision rules, trade-offs, upstream extension watch. Appendices A–E: source write-up, clean-room critique, decision arc, code-verified featureset inventory, perf data. |
| `dev/design/1-2M-chunk-scaling-roadmap-divergence.md` | How far the proposal diverges from the published schedule-of-record (plan-0.8.14…0.8.20 + F-16 + F-17). Verdict: LOW on architecture, MEDIUM on scheduling, HIGH on one premise. |
| memory `1-2M-scaling-kernel-first-not-ann.md` | Durable finding, indexed in MEMORY.md; refines `[[gpu-not-retrieval-lever-hnsw-at-0.8.20]]`. |

Provenance: produced by an independent, no-FathomDB-context ("clean-room") evaluator against a
code-verified featureset survey; recommendation is decision-support, **not** an approved slice.

## The finding in five lines

1. Shipped vector path is **O(N)** two-phase bit-KNN + f32 rescore; **measured ~1.5 s p50 @ 1M** vs a 300 ms budget (~5× over; ~10× at 2M). Crossover where the 80 ms budget breaks ≈ **50k rows**.
2. The 1.5 s is **per-row vec0 overhead, not bytes** (32× smaller binary codes bought only **1.37×** speedup; ~150–300× off the popcount floor). ⇒ **int8/Matryoshka are recall levers, not latency levers.**
3. **Ranked fix: A > C > B.** **A** = a packed/SIMD exact-scan kernel (bypass vec0 per-row overhead) + partition pruning — exact, deterministic, CPU-only, plausibly **<100 ms @ 2M**. **C** = ANN (IVF-via-partition-key), the durability track, needed only if corpus >~5–10M or A can't cover global queries. **B** = relax the budget (hold 300 ms + split CE budget; 700 ms–1 s only as an escape hatch).
4. Binary-prefilter + f32-rescore, the canonical/derived split, and a recall oracle are **already shipped** — so the fix is "make the exact scan fast," **not** "add an approximate index."
5. **Everything gates on E1** (overhead-attribution experiment). Run E1 before any build; the whole ranking is predicated on the scan being overhead-bound.

## THE decision waiting on you

> **Is 1–2M chunks a near-term target for FathomDB's own consumers (Memex / Hermes / OpenClaw)?**

This single premise is the fork. The published roadmap **deliberately** defers 1M-scale latency to
2.x / the F-17 maturity ladder (0.9.x→1.1.0), on the stated assumption that 0.x/1.x consumers run
**well under the ~50k crossover**. The proposal only makes sense if that assumption is now false.

- **If NO (consumers stay < ~50k near-term):** the published roadmap is correct; this work is
  **premature**. Park it; keep ANN at 2.x (F-16); no schedule change. (Optionally still run the cheap
  E1 to bank the diagnosis.)
- **If YES (1–2M is real, soon):** the published line has a **real gap** — no vector-latency-at-scale
  workstream before 2.x — and the *minimum-divergence path* below fills it cheaply without a roadmap rewrite.

Secondary decisions (only if YES), both deferrable until after E1:

- Where to land the kernel — ride **0.8.14**'s coexisting-index (EXP-S) substrate, or **0.8.19**'s
  `scale.rs`/benches substrate, or a standalone perf micro. (Needs Steward scheduling sign-off.)
- Whether to touch the F-17 gate at all — proposal says **no** (kernel makes the gate *reachable* early;
  the hard assert stays 0.9.x→1.1.0).

## Recommended path (if YES) — minimum divergence

Applying the proposal's own A>C>B ranking to the existing ladder shrinks the delta to additive moves
that mostly **respect** the published schedule:

1. **Run E1** (overhead attribution) on 0.8.19's `scale.rs` substrate — cheap, no new slot.
2. **Land the packed/SIMD kernel** as an **invariant-preserving** perf slice (stays CPU-only/1-bit/
   deterministic — does not fight the published query-path invariant); ride EXP-S's coexisting-index
   substrate (the contiguous-code store *is* another index-kind).
3. **Keep ANN/IVF at 2.x (F-16)** — A>C means the kernel, not an index, is the 1–2M fix, so the
   0.8.20 collision and the "defer-ANN" break both disappear.
4. **Keep the F-17 scale ladder unchanged.**
5. **Sequence vec0-internal work after/with the 0.8.20 sqlite-vec migration** (it's a predecessor, not a competitor).

The **large** divergence (pulling ANN forward, colliding with the deferred library sweep) only fires if
YES **and** E1 shows the scan is *not* overhead-bound / global-unscoped queries dominate — i.e. it's
**evidence-gated**, not assumed.

## Divergence at a glance (detail in the divergence doc)

- Architecture/compatibility: **LOW** (kernel honors the CPU-only/1-bit/deterministic invariant).
- Scheduling: **MEDIUM now → LOW** after applying A>C>B (0.8.14 re-theme + 0.8.20 repurpose collisions vanish).
- Premise: **HIGH, unresolved** — the 1–2M question above.
- Zero-divergence: ONNX@0.8.16 kept as-is; HNSW-deferral agreed by both. Already-corrected: the 0.8.18 GA/F-17 over-claim.

## When you come back — pick one

- [ ] **Premise = NO / not near-term** → I park the work; optionally run E1 to bank the diagnosis; no schedule change.
- [ ] **Premise = YES / near-term** → I (a) draft the reconciled edit to `ann-index-vec0.md` (overhead-bound, kernel-first; note the falsified <50k assumption), and (b) take the minimum-divergence path to a **Steward** session for scheduling sign-off (E1 + kernel placement).
- [ ] **Need more first** → e.g. actually run E1 now to remove the premise's dependence on a guess, or pull real consumer corpus-size projections before deciding.

Nothing is committed. No code changed. No schedule changed. This brief + the two docs + the memory
entry are the entire footprint.
