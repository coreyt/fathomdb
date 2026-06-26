---
name: pr3-tiered-latency-budget
description: "0.7.2 PR-3 reframe — tiered AC-013/AC-019 latency budget (10k binding), local-only canonical measurement, AC-019 synthetic report-only, recall anchor 0.937."
metadata: 
  node_type: memory
  type: project
  originSessionId: 40d29fbd-cb6d-4359-9164-540ed77b6490
---

0.7.2 PR-3 was reframed by HITL (2026-06-01) away from "dispatch real-corpus
canonical-CI at N=1M." That is **infeasible**: seeding 1M docs through the real
bge (`bge-small-en-v1.5`, dim 384) serialized projection path is ~166 h at the
PR-9-measured 1.67 docs/s vs a 240-min CI timeout — and the synthetic 1M seed
didn't even drain in 3 h locally (super-linear projection seed cost; the vec0
bit-KNN is an O(N) linear scan, no ANN index).

Decisions (the new shape of PR-3):
- **Tiered AC-013/AC-019 latency budget by N: 10k / 100k / 1M.** Only the
  **10k tier is the binding release gate for 0.x and 1.x.** 100k/1M are tracked
  targets for **post-1.0, pre-2.1** work (an ANN index on vec0: O(N)→O(log N)).
  80/300 ms is MET at 10k (real bge p50 36 / p99 49 @ N≈7667), ~147 ms @100k,
  ~1.5 s @1M.
- **Heavy measurement is LOCAL-only**; CI carries a fast always-on read-path
  smoke `perf_gates::ac_013_vector_read_path_smoke` (exact-match sentinel ranks
  1 through bit-KNN+rerank; no AGENT_LONG/feature/corpus). In-code: AC-013
  asserts the budget only at `n <= AC013_GATE_N` (10_000); above = report.
- **AC-019 synthetic `perf_gates` test is REPORT-ONLY** (no assertion). The
  synthetic isotropic data CANNOT meet the `max(baseline_p99*10,150ms)` bound —
  its instant embed gives a too-fast baseline (16–28 ms) → too-tight bound,
  while the absolute tail (~520 ms @384d / ~1050 ms @768d) matches the real
  path. Verdict signal = the real-corpus harness `eu7_real_corpus_ac.rs`
  (343 < 405 → PASS). The old EU-7 1201 ms was contention, not regression.
- **Recall (AC-013b):** pinned to the EU-7 all-real anchor recall@10 = **0.937**
  (CI 0.913–0.957, N=7667, K=192); floor stays 0.90. N=1M real recall not
  measurable on this hardware. See [[pr2a-go-recompute-split]].

HITL accepted N=7667 as "close enough" to the 10k tier. K (TOP_K_BIT_CANDIDATES)
is 192; raising it 64→192 (the recall fix) is why AC-019's stress tail rose vs
the old STATUS-PVQ 131 ms reading. Empirical data:
`dev/plans/runs/0.7.2-PR-3-perf-data.md`; closure `…/0.7.2-PR-3-output.json`;
ADR `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md` (AC-013/AC-019
sections, HITL-locked tiered; AC-012/AC-020 untouched).

**PR-3 CLOSED** (local `main`, unpushed; arc d9f9b65→68e1bf0→e00991f→df8bbb6,
then docs 9181d74). codex pass-1 BLOCK (read-path smoke could pass via the FTS
path) → fixed FTS-isolated, NOT overridden; pass-2 re-review killed per HITL →
PASS-by-inspection. Both 0.7.0 ADRs flipped to `status: locked` for their HITL-
ratified amendments (latency tiered; recall reframe), AC-012/AC-020 left per
their slices. CHANGELOG.md + `dev/reports/development-state-0.6.0-to-0.7.2.md`
drafted. **Next: PR-4** (release notes + create `v0.7.1` tag + push `main` +
both tags — the irreversible gate; needs explicit push approval). Then Phase B
PR-5/6/7/8.
