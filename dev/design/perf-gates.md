# Perf gates — the two-tier model

FathomDB has **two** perf-gate tiers that exercise the *same* production read
path (`Engine::search` → FTS5 MATCH + two-phase bit-KNN + f32 rerank) at
different scales, with different jobs:

| Tier | File | Runs | N | Job |
|---|---|---|---|---|
| **Devloop** | `crates/fathomdb-engine/tests/perf_gates_devloop.rs` | always, in `cargo test` | ≈1000 | fast inner-loop signal (seconds) |
| **Canonical** | `crates/fathomdb-engine/tests/perf_gates.rs` | `AGENT_LONG=1` only | 10k binding / 100k / 1M | ship verdict |

The devloop tier is the subject of 0.7.2 PR-6. The canonical tier is the
ADR-locked release verdict (PR-3); this doc explains how the two relate so a
green devloop run is never mistaken for a ship verdict (or vice versa).

---

## Honest per-AC gate map (where each gate runs, and what it asserts)

> Added in **0.8.9 (R-PG-1)** so the board never implies a floor CI does not
> actually check. The recall-floor masking it documents was *exposed and fixed*
> at **0.8.0 Slice 40 (AC-075)** — this table records the post-fix reality, not
> a TODO. Verify against the cited test before trusting any "0.90 floor holds"
> claim elsewhere (`perf-recall-gates-masked-and-ac013b-conflation`).

| AC | Test (file::fn) | Runs | Embedder / corpus | Asserts? | Catch / notes |
|---|---|---|---|---|---|
| **013** (text+vector latency, devloop) | `perf_gates_devloop.rs` | **per-push** (`cargo test`) | synthetic `VaryingEmbedder`, ≈1000-doc `CorpusFixture` | **BLOCK** on catastrophic ceiling (p50>500ms / p99>1500ms); soft p50≤50/p99≤150 **notify-only** | synthetic isolates *retrieval* latency; catches the `53a270d` scanner regression |
| **013** (latency, canonical) | `perf_gates.rs::ac_013_vector_retrieval_latency` | **`AGENT_LONG` only** (release) | synthetic, 10k binding / 100k / 1M | tiered ADR budget (10k binding) | the ship verdict; real-embed N=1M is infeasible on CI (~166 h) |
| **013b** (recall@10 floor, synthetic) | `perf_gates.rs::ac_013b_recall_at_10_floor` | `AGENT_LONG` only | synthetic *isotropic* `VaryingEmbedder` | **REPORT-ONLY** (prints `RECALL_FIDELITY_INFO`; **no hard assert**) since AC-075 | isotropic noise is the worst case for sign-bit ANN (~0.35–0.89 < 0.90) — a fixture property, not a product defect. The floor constant `AC013B_RECALL_FLOOR=0.90` and its sentinel `ac_013b_floor_matches_adr` are retained |
| **013b** (recall@10 floor, **asserting**) | `eu7_real_corpus_ac.rs` (real BGE, vector-stage) | `AGENT_LONG` + `default-embedder` (release) | real bge-small, real eu7 corpus | **BLOCK** — one-sided CI: PASS iff `recall_ci_hi ≥ 0.90` (`recall_gate::recall_ci_clears_floor`) | the actual recall verdict (AC-075). N=7667: point 0.8960, CI [0.8640, 0.9250] → ci_hi 0.925 ≥ 0.90 ⇒ PASS |
| **013b** (catch logic, **per-push**) | `recall_gate_predicate.rs` | **per-push** (`cargo test`) | none (pure predicate) | **BLOCK** — asserts the predicate FAILS below floor, PASSES within-uncertainty | 0.8.9 R-PG-2 demonstrate-the-catch: a regression that makes the floor predicate a tautology RED-fails here every push, not once-per-release |
| **013b** (recall WARN, devloop) | `perf_gates_devloop.rs` (`DEVLOOP_REAL_EMBEDDER=1`) | opt-in per-push | real BGE | **notify-only** recall@10 ≥ 0.85 WARN; synthetic recall **report-only** | inner-loop fidelity-drift signal; the binding verdict is the eu7 gate above |
| **012** (FTS5 text-query latency) | `perf_gates.rs::ac_012_text_query_latency_on_fts5_path` | `AGENT_LONG` only | synthetic English-like, tiered N | tiered ADR budget | canonical latency on the FTS path |
| **019** (concurrency stress) | `perf_gates.rs` (AC-019) + `perf_gates_devloop.rs` | canonical `AGENT_LONG`; devloop per-push (stress-tail **report-only**) | synthetic | canonical asserts; devloop report-only | `DEVLOOP_AC019_DETAIL` line carries thread shape |
| **020** (latency under load) | `perf_gates.rs` (AC-020) | `AGENT_LONG` only | synthetic | tiered budget | release-only |
| **037** (netns-deny-egress) | `scripts/security/check-netns-deny-egress.sh` | **per-push** in CI `security` job (`ubuntu-22.04`, userns-permissive) | n/a (strace `connect()` trace) | **BLOCK**; `STRICT=1` makes a toolchain blocker hard-fail (no vacuous pass) | catch demonstrated by `scripts/security/check-netns-deny-egress-catch.sh` (0.8.9 R-037-2) |

**The throughline:** the 0.90 recall floor is a **release-tier** gate (real BGE
embed = minutes–hours; it *cannot* run per-push), enforced by the eu7
real-embedder one-sided-CI test — **not** by the synthetic `ac_013b`, which is
report-only. The per-push half is the devloop recall WARN (notify) plus the
`recall_gate_predicate` catch-logic unit test (block). Any board that quotes
"0.90 floor enforced in per-push CI" is wrong: per-push enforces the *catch
logic* and the *catastrophic latency ceiling*; the *floor value* is a
once-per-release real-corpus verdict.

---

## Why two tiers

The canonical gates are slow: the real-embedder N=1M measurement is infeasible
on CI (~166 h of serialized BGE seed; see
`dev/plans/runs/0.7.2-PR-3-output.json`), so they are `AGENT_LONG`-gated and run
as a once-per-release local exercise. That leaves the inner dev loop with **no
perf signal at all** between releases — a latency or vector-wiring regression
could land and sit unnoticed until the next canonical run.

The devloop tier fills that gap: a ≈1000-doc subset (via PR-5's `CorpusFixture`)
that runs on every `cargo test`, exercises the identical production read SQL,
and surfaces a perf + recall + structural signal in ~16 s.

It does **not** replace the canonical verdict. Small-N latency is noisy and the
synthetic embedder cannot meet the real-data recall/stress bounds, so the
devloop tier deliberately does **not** assert the canonical budgets.

---

## Gate disposition: notify vs block (HITL-locked 2026-06-01)

The governing rule for the devloop tier is **perf signals NOTIFY, structural
invariants BLOCK**:

| Signal | Disposition | Catches | Why this disposition |
|---|---|---|---|
| `assert_vec0_row_count_matches_ingest` | **hard assert (BLOCK)** | batch-collapse (`4a95cfd`) | deterministic correctness — a collapsed batch leaves `vector_default` short of the doc count; no flap risk |
| `assert_fts_index_populated` | **hard assert (BLOCK)** | FTS path not wired | deterministic correctness |
| latency p50 ≤ 50 ms / p99 ≤ 150 ms (synthetic) | **notify-only WARN** | gradual latency drift | small-N sample noise should not fail `cargo test`; PR-7 turns the trend into a CI gate |
| **catastrophic ceiling** p50 > 500 ms / p99 > 1500 ms (10× soft; synthetic) | **hard assert (BLOCK)** | scanner-throughput (`53a270d`) | an orders-of-magnitude inflation is unambiguously a regression, not noise |
| latency (real embedder) | **report-only** | — | `search` latency on the real path is candle-embed-influenced, not a clean retrieval signal; synthetic isolates retrieval |
| recall@10 ≥ 0.85 (real embedder) | **notify-only WARN** | quantization-fidelity drift | noisy at small N; PR-7 owns the trend gate |
| recall@10 (synthetic embedder) | **report-only** | — | synthetic vectors quantize poorly (~0.35 @ N≈1000) — a property of the DATA, not a regression (mirrors AC-019) |

**The two embedders carry complementary signals:** synthetic isolates
**latency** (instant embed → measured time is pure retrieval); real makes
**recall** meaningful (dense vectors → ANN fidelity is real). Each path's
off-signal is report-only.

### Why a single hard catastrophic ceiling

Notify-only latency alone would mean the scanner-throughput regression
(`53a270d`, whose symptom is orders-of-magnitude p50/p99 inflation) no longer
fails `cargo test`. The catastrophic ceiling at 10× the soft budget restores a
hard RED for that regression while leaving routine noise to the notify path: a
regression that inflates p50 from ~14 ms to hundreds of ms clears 500 ms; normal
run-to-run wobble never does.

### RED-shows evidence

Both named regressions were re-introduced (symptom-injected at the gate seam) in
a throwaway run and confirmed to RED-fail — see
`dev/plans/runs/0.7.2-PR-6-output.json` (`red_shows_experiment`):

- batch-collapse → `assert_vec0_row_count_matches_ingest` panics
  (*"vector_default has 1 rows but 1000 … docs were ingested"*).
- scanner-throughput → `enforce_catastrophic_ceiling` panics
  (*"DEVLOOP CATASTROPHIC (013): p50=616ms > 500ms"*).

---

## Embedder: synthetic default, real opt-in

- **Synthetic (`VaryingEmbedder`) — always-runs default.** Its embed is instant,
  so the measured latency isolates *retrieval* (same rationale as canonical
  AC-013). This is the inner-loop signal and the only path bound by the ≤30 s
  budget. It carries the latency gates; its recall is report-only (see above).
- **Real BGE — opt-in.** Enabled by `DEVLOOP_REAL_EMBEDDER=1` **and** the
  `default-embedder` feature; without the feature the fixture SKIPs gracefully.
  It carries the 0.85 recall-floor WARN; its latency is report-only. Cold-cache
  is **allowed** (the first run warms PR-5's on-disk doc-body cache and is slow);
  a warm re-run skips re-embedding doc bodies. Held-out *query* texts are not in
  the doc-body cache, so the warmup pass embeds them live once (the in-memory
  cache then serves the measure pass) and the recall ground-truth pass embeds
  them again — so the real path's WALL time is candle-bound and **not** ≤30 s
  regardless of the doc cache. It is an occasional end-to-end exercise, not the
  inner-loop signal.

The PR-5 embedding cache (`data/corpus-data/.cache/embeddings/`) caches embed
*inputs*, not query *results*: warm runs skip re-embedding doc bodies but the
production bit-KNN + f32 rerank read path runs in full every time. The devloop
tests therefore measure the real read path, never a cached short-circuit.

---

## Knob inventory (`DEVLOOP_*`)

The devloop tier keeps its own knob surface, disjoint from the canonical
`AC013_*` / `AC019_*` knobs, so neither contract bleeds into the other.

| Knob | Effect | Default |
|---|---|---|
| `DEVLOOP_REAL_EMBEDDER=1` | use real BGE (requires `default-embedder`; cold-cache allowed, warm re-run faster) | unset → synthetic |
| `FATHOMDB_CORPUS_CACHE_DIR` | override the PR-5 embed-cache dir (shared with the harness) | `data/corpus-data/.cache/embeddings/` |

In-code budget constants (not env): `DEVLOOP_BUDGET_P50` (50 ms),
`DEVLOOP_BUDGET_P99` (150 ms), `DEVLOOP_RECALL_FLOOR` (0.85),
`DEVLOOP_CATASTROPHIC_MULT` (10×), `DEVLOOP_SAMPLES` (100), `DEVLOOP_SEED`.

Devloop tests are **not** gated behind `AGENT_LONG` — that flag is the canonical
tier's switch and the devloop tier must run unconditionally.

---

## The `DEVLOOP_NUMBERS` line (PR-7 consumes this)

Each test emits one stable, parseable trend line on stderr:

```
DEVLOOP_NUMBERS ac=013 n=1000 samples=100 p50_ms=14 p99_ms=18 recall_at_10=NA cache=na embedder=synthetic
DEVLOOP_NUMBERS ac=013b n=1000 samples=100 p50_ms=0 p99_ms=0 recall_at_10=0.3470 cache=na embedder=synthetic
DEVLOOP_NUMBERS ac=019 n=1000 samples=200 p50_ms=12 p99_ms=154 recall_at_10=NA cache=na embedder=synthetic
DEVLOOP_AC019_DETAIL ac=019 threads=4 per_thread=50 stress_ms=5728 baseline_p99_ms=16 stress_p50_ms=12 stress_p99_ms=154 disposition=report_only
```

All three ACs share the same keyed `DEVLOOP_NUMBERS` schema
(`ac/n/samples/p50_ms/p99_ms/recall_at_10/cache/embedder`); for AC-019 the
percentiles are the stress-tail. AC-019's extra detail (baseline, thread shape,
wall time) is on a separate `DEVLOOP_AC019_DETAIL` line so the common contract
stays uniform.

Breach notifications (notify-only, do not fail):

```
DEVLOOP_PERF_WARN ac=013 metric=p50 value_ms=60 budget_ms=50 status=OVER
DEVLOOP_PERF_WARN ac=013b metric=recall_at_10 value=0.83 floor=0.85 status=UNDER   # real embedder only
DEVLOOP_PERF_INFO ac=013b metric=recall_at_10 disposition=report_only embedder=synthetic
```

The `ac=`, `p50_ms=`, `p99_ms=`, `recall_at_10=` fields are the stable contract
**PR-7** (perf-regression detection, `dev/perf-history/`) parses to build the
per-(ac, embedder) trend and apply its 10 %-latency / 0.02-recall thresholds.
Adding fields is safe; renaming or reordering the keyed fields is a breaking
change to that contract.

---

## See also

- `dev/adr/ADR-0.7.0-text-query-latency-gates-revised.md` — the tiered canonical
  budget (10k binding; 100k/1M post-1.0 ANN work).
- `crates/fathomdb-engine/tests/support/corpus_harness.rs` — PR-5 `CorpusFixture`
  + embed cache the devloop tier consumes.
- `dev/perf-history/` + the perf-regression check (PR-7) — turns the
  `DEVLOOP_NUMBERS` trend into a regression gate.
