# Slice 40 / GA-2 — gate restructure (AC-075/076) + GA verification design memo

**Type:** `[design]` · **Slice:** 40 / GA-2 (0.8.0 GA terminus) · **Baseline:** local `main` @ `7316746`
**Worktree:** `/tmp/fdb-ga2-20260608T145205Z` · **Branch:** `slice-40-ga2-20260608T145205Z`
**Authoritative scope:** `dev/plans/prompts/0.8.0-slice-40-B1-application.md` (the ◆ B-1 ruling, which
OVERRIDES the eu7/AC-075 framing of `0.8.0-slice-40.md`) + the B2/B3/Q3 HITL rulings in the base prompt.

This memo records the designs **before** the engine/harness edits, per the slice prompt §3.0. It is the
falsifiable bar the codex §9 reviewer will check the diff against.

---

## 0. The ◆ B-1 correction (why this GA-2 differs from the halted GA-1 attempt)

The first Slice-40 attempt (branch `slice-40-20260607T145013Z`) HALTED at `recall@10 = 0.8710 < 0.90`.
GA-1 (`dev/plans/runs/GA-1-corpus-ab-20260608T012503Z.{md,json}`) + orchestrator verification proved that
HALT was **not** a corpus move and **not** a confirmed ANN regression:

1. The corpus is gitignored and **byte-identical** across both anchor runs (no OLD≠NEW corpus exists;
   all 8 raw files mtime 2026-05-27, N=7,667 for both the 0.937 anchor and the 0.8710 run).
2. `eu7::measure_recall` builds ground truth as an **exact-f32 VECTOR top-10** but compared it against
   `.search()`, which Slice 10 (`d28d204`) made **unconditional RRF-hybrid** (vector ⊕ FTS5). So the
   measured "recall@10" was dominated by **fusion divergence**, conflated with the ANN-quantization
   FIDELITY the 0.90 floor is *defined* to measure (`ADR-0.7.0-vector-binary-quant.md` § 2 point 4;
   `[[fathomdb-recall-fidelity-vs-relevance]]`). The 0.937 anchor (2026-05-31) predates the
   unconditional-RRF change, so it was effectively a vector-stage number.

**HITL ◆ B-1 ruling (Option 1, 2026-06-08):** correct `eu7` to measure ANN-quantization FIDELITY on the
**vector stage in isolation**, then assert the unchanged 0.90 floor. This **RESTORES** eu7's documented
semantics — a *correction, NOT a weakening*. Do NOT lower the floor, do NOT weaken/remove the assert, do
NOT redefine the floor as end-to-end hybrid fidelity (rejected Option 3). The prior branch's
`eu7`-on-`search()` assert (asserting 0.90 against the fused SUT → 0.871, RED) is **the bug this slice
corrects**; its perf_gates / acceptance.md / ADR / CI gate-restructure work is reused unchanged.

---

## 1. The fix in three parts (RED → GREEN)

### 1a. Engine — TEST-ONLY vector-stage measurement seam (additive plumbing only)

`read_search_in_tx` (`crates/fathomdb-engine/src/lib.rs`) already computes the vector branch
(`vector_results` = bit-KNN K=192 Hamming + f32 rerank) separately, *before* `fuse_rrf` folds in the FTS5
(`text_results`) branch. The seam exposes that pre-fusion ranking:

- New `ProjectionRuntimeShared::vector_stage_only_for_test: AtomicBool` (default `false`), mirroring
  `recency_reweight_enabled`. Read in `search_inner`, carried on `ReaderRequest::Search`, passed to
  `read_search_in_tx`.
- New seam `Engine::set_vector_stage_only_for_test(&self, enabled: bool)`, mirroring
  `set_recency_reweight_enabled_for_test` (release-available — eu7 runs in `--release`).
- In `read_search_in_tx`: `let results = if vector_stage_only { vector_results } else { rerank_fused(apply_recency_reweight(fuse_rrf(vector_results, text_results), recency_enabled)) };`

**HARD CONSTRAINTS held:** measurement seam ONLY. Production `search()` output is byte-unchanged for any
non-test caller (flag defaults off, never set outside eu7). It does NOT reintroduce a `fusion_mode` knob
(RRF stays unconditional) and does NOT alter `fuse_rrf` / `rerank_fused` / recency (the production fusion
expression is preserved verbatim inside the `else` branch). `git diff` on the fusion logic is additive
plumbing only. Recovery suites + governed-surface allowlist byte-frozen.

### 1b. Harness — repoint `eu7` recall@10 at the vector stage; report the fused delta

`eu7::measure_recall` currently collects the SUT from `engine.search()` (fused). Repoint:

- The **verdict** SUT becomes the vector-stage ranking: enable the seam, `engine.search()` then returns
  the pre-fusion vector top-(10+slack), compared against the unchanged exact-f32 VECTOR top-10 GT. This is
  ANN+ vector top-10 vs exact-f32 vector top-10 — the quantization-FIDELITY axis.
- The **delta** SUT (report-only): with the seam off, `engine.search()` returns the production RRF-fused
  result; measured with the *same* exclude-before+dedup method and printed as `EU7_RECALL_FUSED` so the
  fused-vs-vector delta is legible. This is the load-bearing RED→GREEN demonstration: fused ≈0.871 (would
  RED the 0.90 assert) vs vector-stage ≈0.937 (GREEN) — proving the repoint is not cosmetic.
- The GT construction (`~:400-413`), the `RECALL_SEARCH_SLACK=5` fanout via `set_search_limit_for_test`,
  the bootstrap CI/σ/N, and the exclude-target convention all stay. Only the SUT changes.

### 1c. The assert (the AC-075 verdict)

Promote the eu7 verdict loop from `assert!(recall ≥ SANITY_FLOOR 0.55)` to also
`assert!(recall ≥ CURRENT_FLOOR 0.90)` on the **vector-stage** number, for each all-real (non-padded) N.
Keep `SANITY_FLOOR` as the lower wiring-bug diagnostic. `perf_gates::ac_013b_recall_at_10_floor`
(synthetic isotropic `VaryingEmbedder`) is demoted to REPORT-ONLY (`RECALL_FIDELITY_INFO`, no hard 0.90
assert); the `AC013B_RECALL_FLOOR = 0.90` constant + its `ac_013b_floor_matches_adr` sentinel are kept.

**Measurement model (mirrors AC-072/073):** the eu7 real-embedder recall is the LOCAL once-per-release /
`perf-canonical` dispatch verdict (real bge-small + real corpus + `default-embedder` + `AGENT_LONG`,
~1.5 h seed). Per-push CI runs only the fixture-independent `ac_013_vector_read_path_smoke`. NO
real-embedder run is wired into per-push CI (infeasible — AC-072 ~166 h at canonical N).

## 2. AC-076 design (ac_012 tier) — mirror ac_013 (reused from prior branch unchanged)

- Add `const AC012_GATE_N: usize = 10_000;` next to `AC013_GATE_N`; wrap the p50≤20/p99≤150 asserts in
  `if n <= AC012_GATE_N { assert } else { eprintln!("AC012_TIER_INFO …") }` — copying `ac_013`'s branch
  shape. `AC012_DEFAULT_N` stays `100_000`. **Tokenizer + migration 011 unchanged** (Slice 6: porter ≈
  unicode61 within noise; the latency is O(N) FTS-scan).

## 3. acceptance.md — AC-075 + AC-076 (mirror AC-072/073)

- AC-075 wording is **corrected per B-1**: the measured quantity is the **ANN+ vector-stage recall@10**
  (1-bit sign-quant K=192 Hamming + f32 rerank) **vs the exact-f32 VECTOR top-10 of the same embedder
  ≥ 0.90 — an ANN-quantization FIDELITY gate, measured on the vector stage in isolation (NOT the
  RRF-fused `search()` output)**. Note it is complementary to, not a substitute for, the IR/relevance
  axis (eu8 / the IR-1 `ir-recall-measure.md`).
- AC-076: p50≤20/p99≤150 BINDING at 10k, 100k/1M TRACKED. Supersedes AC-012's unconditional budget.
- Coverage trace: REQ-010 → AC-012, AC-076; REQ-011 → AC-013, AC-072, AC-075.

## 4. ADR amendment — `ADR-0.7.0-vector-binary-quant.md`

§ 2 point 4 / status: record that the 0.90 floor is now GATED on the real-embedder `eu7` **vector-stage**
fidelity (AC-075), and WHY: the unconditional-RRF change (Slice 10, `d28d204`) made fused-`search()` the
wrong SUT for a quantization-fidelity gate — cite GA-1. The synthetic `ac_013b` is report-only. No
numeric change to the 0.90 floor. This is the single ADR amendment the slice authorizes.

## 5. CI wiring (Q3/META) — reused from prior branch

- `ci.yml`: `security` job on **`ubuntu-22.04`** (userns-permissive for AC-037 netns-deny-egress),
  `STRICT=1 bash scripts/agent-security.sh`. `actionlint` clean.
- `perf-canonical.yml`: annotate AC-075 (real-embedder eu7 vector-stage recall) + AC-076 (ac_012@10k) as
  the once-per-release `--release` verdict. No per-push real-embedder run.

## 6. Release docs (C) + X1/X2/X3

- `dev/releases/0.8.0.md` + `docs/release-notes/0.8.0.md` + CHANGELOG `## 0.8.0`. Three behavior-compat
  events: RRF ordering (no knob); `SearchHit` reshape (`Eq` dropped); AC-057a → AC-074 governed-surface
  supersession. Plus additive `WriteReceipt` fields + the latency-neutral FTS5 tokenizer upgrade.
- X2: `mkdocs build --strict` green after nav. X3: `dev/DOC-INDEX.md` rows for AC-075/076, the ADR
  amendment, release notes, this memo. Every release-note claim traces to a §7 measured result.

## 7. Falsifiable bar (the §3.3 battery, `--release` + isolated)

Bars (a)–(n) green in `--release`, perf/recall isolated. Specifically: **eu7 vector-stage recall ≥ 0.90
(asserting, PASS ≈0.937) with the fused delta (≈0.871) printed**; ac_012@10k green + 100k reported;
ac_013b report-only; ac_020 green+isolated (B3); recovery + governed-surface allowlist **byte-unchanged**;
migration steps 11+12+13 idempotent; Py+TS parity; `mkdocs build --strict` green; `actionlint` clean;
gate (n) present (AC-037 env-deferred locally, CI-wired). A non-AC-037 gate RED in release+isolated for a
real reason → STOP+report (no gate patched to green).

**GA-2 environment note (recover-out-loud):** the real bge-small weights are NOT in the local HF cache and
the gitignored corpus is not materialized in this worktree, so the ~1.5 h real-embedder eu7 run is
**infeasible in this worktree** — exactly the LOCAL/perf-canonical-dispatch verdict AC-072/073 already
document as out-of-band. The fused-vs-vector delta (0.871 vs 0.937) is evidenced on-record: 0.8710 is the
fused-`search()` number from the prior slice-40 `output.json`; 0.937 is the 0.7.1 vector-stage anchor
(measured pre-unconditional-RRF). The code structurally encodes the RED→GREEN demonstration; the live
numeric re-confirmation is the once-per-release perf-canonical verdict, not a per-push gate.

## 8. Scope guard (what this slice does NOT touch)

No tokenizer / migration 011 / engine read-write / schema / SDK-surface change beyond the additive
test-only seam; no re-opened signed ADR (except the authorized `ADR-0.7.0-vector-binary-quant.md`
amendment); ACs limited to AC-075 + AC-076; no `STATUS-0.8.0.md` edit (orchestrator finalizes the
scoreboard); no version bump / push / tag / publish; no Slice 46 / gap-37. Recovery suites +
governed-surface allowlist byte-frozen. **No self-merge — the orchestrator runs codex §9 and merges.**
