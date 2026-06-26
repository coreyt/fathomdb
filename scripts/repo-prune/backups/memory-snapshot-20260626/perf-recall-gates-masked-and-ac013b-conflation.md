---
name: perf-recall-gates-masked-and-ac013b-conflation
description: "0.8.0 Slice 40 GA verification exposed that the heavy perf/recall gates never run in per-push CI and ac_013b's 0.90 floor was conflated with eu7's real-embedder number"
metadata: 
  node_type: memory
  type: project
  originSessionId: 857bb76f-928c-49c7-a858-4ecfbb197057
---

Slice 40 (0.8.0 GA verification, 2026-06-06) forced `AGENT_LONG=1` and surfaced what
per-push CI had masked all campaign. **All five heavy perf/recall gates in
`perf_gates.rs` (ac_012/013/013b/019/020) are `long_run_enabled()`-gated (early-return
unless `AGENT_LONG` is set), and the per-push CI `verify` job never sets it.** They run
only in `perf-canonical.yml` — which is `workflow_dispatch`-only and AC-012-filtered by
default. The per-push guard is only the lightweight `ac_013_vector_read_path_smoke`. So
these budget/floor asserts were **vacuously green** (skipped), the trap class from
[[conformance-rewrite-vacuous-green-trap]] + [[background-exit-masks-real-exit]].

Three findings (all verified from test source, not just the agent's word):
- **B2 (NOT a regression — CORRECTED by Slice 6, 2026-06-07):** the Slice-5 tokenizer is
  **exonerated**. Engine A/B (release, isolated, same x86_64 box): `porter unicode61
  remove_diacritics 2` = 21ms ≈ v0.7.2 `unicode61` = 20ms @100k — latency-neutral (porter
  is a no-op on the digit-bearing synthetic AC-012 vocab). The Slice-40 49.9ms was a
  **debug-build + concurrent-workspace** artifact: `check.sh` = `AGENT_LONG=1 cargo test
  --workspace` (debug, all crates concurrent); Slice 6 used `cargo test --release` isolated.
  **Lesson: perf gates MUST build `--release` + run isolated** (as `perf-canonical.yml`
  does); don't wire `cargo test --workspace` perf into per-push CI. Real issue = `ac_012`
  asserts @100k *unconditionally* while siblings AC-013/AC-019 were tiered (AC-072/AC-073:
  10k binding, 100k/1M tracked). **Fix = tier AC-012 (mint AC-076, mirror `ac_013`'s
  `AC013_GATE_N`), KEEP the porter tokenizer** (it buys +3/8 morphological recall for ~0ms).
- **B1 (gate mis-calibration, pre-existing, NOT a quality regression):**
  `ac_013b_recall_at_10_floor` ASSERTS the 0.90 floor but runs the SYNTHETIC
  `VaryingEmbedder` → ~0.73 (v0.7.2-identical). **The 0.937 figure everyone cited is
  `eu7_real_corpus_ac.rs` (REAL bge-small, report-only), NOT ac_013b.** There is NO formal
  `AC-013b` row in acceptance.md (informal sub-gate). Real product retrieval quality is
  fine (0.937, CI L-bound 0.913 > 0.90); the gate is wrong. The Slice-40 *contract* itself
  carried the conflation ("ac_013b observed ~0.937") and it propagated into my prompt —
  corrected. **Don't repeat "the 0.90 recall floor holds via ac_013b" — it never did.**
- **B3 (runner-pinned):** `ac_020` parallel-read scaling fails on the aarch64 dev box AND
  on v0.7.2; perf gates are canonical-x86_64-pinned. Confirm on the canonical runner.

**B1 resolution is precedented:** AC-072 (latency) and AC-073 (stress) already did the
same move — real-corpus `eu7` = asserting verdict, synthetic `perf_gates` = REPORT-ONLY,
via `ADR-0.7.0-text-query-latency-gates-revised`. B1 = mint **AC-075** (recall verdict;
eu7 asserts ≥0.90 real-embedder, ac_013b→report-only) at a gated slice, amending
`ADR-0.7.0-vector-binary-quant.md`. AC changes only at gated slices (acceptance.md max =
AC-074; [[acceptance-md-locked-no-feature-acs]]).

HITL ruled 2026-06-06: B1 elevate-eu7/demote-ac_013b; B2 experiment slice (Slice 6) first;
Q3 wire perf+recall to GA (Slice 40 re-scope). Slice 40 HALTED, not merged; main unmoved.
B1+Q3 fold into the Slice-40 re-scope after B2 resolves. See [[fathomdb-080-plan-approved]].
