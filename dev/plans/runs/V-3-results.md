# V-3 (OPP-1) — Decomposition Head-room & Iterative-Solve Results

**Status:** CONCLUDED 2026-06-30. Verdict = mechanism works; **Adopt-GO HELD → Memex 0.5.5**.
**Cost:** ~$2.90 of the $75 pooled envelope (A2 priced pass only; A0/A3 were $0/local-GPU).
**Why this doc exists:** the raw eval artifacts live in `scratchpad/v3_work/` (EPHEMERAL). This is the
committed capture of the load-bearing numbers so they survive scratchpad cleanup. Companion docs:
`V-3-plan.md` (design) and `v3-decomposition-quality-research.md` (literature/SOTA digest).

---

## 1. Question & design

Can decomposing a multi-hop question into sub-questions, **solving each hop, and substituting the solved
answer forward**, manufacture supporting-fact recall over V-1's single-shot baseline — and is that lift
realistically capturable (not just an oracle artifact)?

Three arms, byte-identical retrieval stack to V-1 (per-question paragraph pool → BM25 + dense bge-small CLS
→ RRF fuse k=60 → CE rerank via `fathomdb.rerank` on live TinyBERT-L-2 → blend α=0.3, pool_n=20):

- **A0** — single-shot blended ranking of the full question (the V-1 floor).
- **A3 (oracle)** — native MuSiQue `question_decomposition` with `#N` placeholders replaced by **gold**
  sub-answers (perfect decomposition AND perfect intermediates); round-robin union of per-sub-query
  rankings under the same total doc budget k. **The ceiling.**
- **A2 (real iterative solve)** — a realistic loop: flash-lite decomposes, `gemini-3.1-pro` reads/solves
  each hop from retrieved evidence, the solved entity is substituted into the next sub-query (least-to-most
  / IRCoT-style). **The capturable arm.**

Primary metric: fractional supporting-fact recall@k = |gold ∩ top-k| / |gold|, paired per qid, per-corpus,
never pooled. Gap-recovered = (A2 − A0) / (A3 − A0).

---

## 2. A3 oracle head-room — full MuSiQue (n = 2,417 answerable, avg 2.65 sub-Qs) — $0/local GPU

Fractional recall@k, paired-bootstrap 95% CI (2000 resamples, seed 20260630):

| k | A0 | A3 (oracle) | lift (95% CI) |
|---|------|------|------------------------|
| 2 | 0.476 | 0.674 | **+0.198 [0.188, 0.209]** |
| 3 | 0.559 | 0.827 | **+0.268 [0.257, 0.279]** |
| 4 | 0.612 | 0.895 | **+0.284 [0.273, 0.295]** |
| 5 | 0.652 | 0.928 | **+0.276 [0.265, 0.286]** |
| 10 | 0.795 | 0.983 | **+0.188 [0.178, 0.197]** |

Strict all-or-nothing recall@5: A0 0.310 → A3 0.801 (**+0.492**). **Lift grows with hop depth**
(lift@5: 2-hop +0.256 → 3-hop +0.271 → 4-hop +0.344) — the signature of compounding per-hop error that a
real solver must overcome. Every CI lower-bound is well above 0.

**HotpotQA harness cross-check (A0 only; no native decomposition; n=1,000 of 7,405):** nDCG@10 0.902 /
MRR 0.940 reproduce V-1's independently-reported 0.902 / 0.947 → the analyzer/harness is validated.

**Read:** oracle decomposition manufactures large, depth-growing recall head-room. The priced A2 phase has
a real ceiling worth chasing → proceed past the D-1 gate (not a cheap KILL).

---

## 3. A2 real iterative-solve — MuSiQue (n = 200; hops 2/3/4 = 104/62/34) — priced $2.897, 813 calls

Config: α=0.3, pool_n=20, k_reader=10, k_solve=5, decomposer `gemini-flash-lite`, reader `gemini-3.1-pro`,
seed 20260630. F1/EM measured on the n=100 answer subset.

| Metric | A0 (floor) | A2 (real) | A3 (ceiling) | **gap recovered** |
|--------|-----------|-----------|--------------|-------------------|
| recall@5 | 0.612 | 0.845 | 0.922 | **75.2%** |
| recall@10 | 0.770 | 0.920 | 0.980 | 71.7% |
| F1 (n=100) | 0.460 | 0.630 | 0.665 | **83.4%** |
| EM (n=100) | 0.360 | 0.490 | 0.550 | 68.4% |

**By hop (recall@5 gap-recovered):** 2-hop 90.6% → 3-hop 64.3% → 4-hop 50.0% — recovery decays with depth,
exactly as compounding error predicts, but stays material even at 4 hops.

**Read:** the solve-and-substitute loop is **real and effective** — it captures ~¾ of the oracle recall
head-room and ~⁵⁄₆ of the F1 head-room at ~$0.0145/question, decisively beating the naive flash-lite
splitter (which captured ~0%). The winning mechanism is *solving each hop and feeding the resolved entity
forward*, not merely emitting sub-question text.

---

## 4. Verdict

- **Build-GO — MET.** Decomposition-with-iterative-solve works: A2 recovers 75.2% recall@5 / 83.4% F1 of
  the oracle head-room on MuSiQue for ~$2.9.
- **Adopt-GO — HELD → Memex 0.5.5** (NOT 0.5.3). Two reasons: (1) the winning mechanism (solve-and-
  substitute) is **not** in Memex's current `run_decomposed`, which emits sub-question *titles* only —
  adopting means Memex must **build** the iterative solve-loop; (2) insufficient **frequency** data on how
  often real Memex traffic is genuinely multi-hop to justify the per-query cost/latency. Gate adoption on
  `$0` intent_hint frequency telemetry + Cause-A real-gold dependency labels first.
- **Guidance for the 0.5.5 build** is in the two Memex-committed docs: `dev/fathom/
  v3-decomposition-quality-research.md` (d802fe4) and `dev/fathom/decomposer-0.5.5-optimization-briefing.md`
  (e7f46cc). P0 levers from the literature: interleaved solve-and-retrieve with forward-substitution of the
  *resolved entity*; per-hop verification (self-consistency + grounding) since error compounds as p^k;
  when-to-decompose / when-to-stop gating; best-of-N over chains for the deep tail.

**Cross-refs:** memory `opp1-v3-decomposition-verdict-adopt-hold-0.5.5`; `0.8.x-remaining-todos.md`
(OPP-1 adoption tail). Config, per-qid traces, and CIs: `scratchpad/v3_work/{a3-analysis.gpu.json,
a2-run.json,A0-A3-headroom.md}` (ephemeral).
