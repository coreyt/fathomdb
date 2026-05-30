# STATUS — 0.7.1 EMBEDDER-UNDEFER

_Last updated: 2026-05-30 — campaign CLOSED (docs done; tag deferred). EU-0..EU-6 CLOSED on `origin/main`; EU-5f (engine fixes A/B/C) + EU-7 (measurement) codex-PASS; EU-8 (docs + release prep) CLOSED. EU-7: AC-013 latency + AC-019 stress GREEN (dev-box); AC-013b recall@10 = 0.828 — RED vs the 0.90 floor, surfaced to HITL (floor re-pin = 0.7.2 PR-2). HITL (2026-05-30) deferred the version bump + `v0.7.1` tag entirely to 0.7.2 PR-4; workspace stays at 0.7.0. NOT pushed (9 commits ahead of origin/main)._

Orchestrator: main thread (Claude Code session). Pattern per `dev/design/orchestration.md`.

## Handoff

- Master plan: `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md`
- Decision register: `dev/design/embedder-decision.md` (every locked parameter)
- Loader / mean-centering design: `dev/design/embedder.md`
- EU-0 research: `dev/notes/0.7.1-default-embedder-research.md`
- EU-7 dispatch: `dev/plans/prompts/0.7.1-EU-7-launch.md`
- EU-7 raw measurements: `dev/plans/runs/0.7.1-EU-7-measurements.json`
- EU-7 closure JSON: `dev/plans/runs/0.7.1-EU-7-output.json`
- EU-7 run log: `dev/plans/runs/0.7.1-EU-7-fullrun.log`
- Downstream campaign: `dev/plans/prompts/0.7.2-RELEASE-HARDENING-HANDOFF.md`

## Baseline

- Branch: `main` (slices worked directly on `main`; EU-7 per its launch prompt)
- Pre-campaign HEAD: `fe1d10f` (parent of the first 0.7.1 commit; PVQ closure was `v0.7.0` = `e543c26`)
- Post-EU-7 HEAD: the closure commit that adds this file + `0.7.1-EU-7-output.json` (see `git log --oneline v0.7.0..HEAD | head -1`)

## Slice scoreboard

Commit SHAs from `git log --oneline v0.7.0..HEAD`. RED/GREEN/FIX chain per
`dev/design/orchestration.md`.

| ID | Subject | Status | Key commit(s) | Codex |
|---|---|---|---|---|
| EU-0 | Research: bge-small under sign-bit + f32 rerank; K-sweep + mc ablation | **CLOSED** | `e5d1298`, `d3f7c49`, `d0d2678`, `8e8454c` (K=128→192) | PASS |
| EU-1 | ADR + NEED-017/REQ-033 weight-fetch exception | **CLOSED** | `b99c203` | PASS |
| EU-2 | EMB-5 loader + mean-centering sub-design | **CLOSED** | `8fa4c75`, `fae2799`, `3ee3775` | PASS (HITL-signed) |
| EU-3 | Default-embedder loader impl | **CLOSED** | `af2e6e7` (GREEN), `dc70704`/`b77798f`/`6c2a2b1` (FIX-1/2/3) | PASS |
| EU-4 | CandleBgeEmbedder (candle-transformers BertModel) | **CLOSED** | `a18b1bf` (GREEN), `a97b6a6` (FIX-1) | PASS |
| EU-5a1 | EmbedderChoice enum + OpenReport plumbing | **CLOSED** | `c572228` (GREEN), `f61c88e` (FIX-1) | PASS |
| EU-5a2 | Schema migration step 10 + mean-centering pipeline + K=192 | **CLOSED** | `49cdcf4` (GREEN), `9587af3` (FIX-1) | PASS |
| EU-5b | Default-embedder identity lock-flip + CLI warm-cache | **CLOSED** | `1c0b760` (GREEN), `4770b11` (FIX-1) | PASS |
| EU-5c | CI BGE cache pre-warm + network-test gate | **CLOSED** | `30d68ce`, `b3e5025` (FIX-1) | PASS |
| EU-5d | Post-review follow-ups (ADR + clippy + endianness compile_error) | **CLOSED** | `ea57fdf` | PASS |
| EU-6 | Python + TS `use_default_embedder` + wheel-size gate | **CLOSED** | `c27712f` (GREEN), `ed20816` (FIX-1), `63886fc` (FIX-2 EmbedderEvent union), `c8c7d43` (FIX-2 follow) | PASS |
| EU-5f | Production mean-centering pin + projection fault-isolation + 512-token truncation (engine fixes surfaced by EU-7; see findings A/B/C) | **CLOSED** | `fbdd613` (findings), `2270520` (RED), `574ef28` (GREEN A+B), `c719a12` (Finding C truncation) | **PASS** (`...review-20260530T130438Z.md`) |
| EU-7 | Real-corpus AC validation + recall@10 anchor | **MEASURED (AC-013b RED vs 0.90, surfaced)** | harness in `2270520`; results JSON in `aff565f` | **PASS** (HITL owns floor via PR-2) |
| EU-8 | Docs + release prep (CHANGELOG, docs/embedder.md, ADR flip, closure doc); tag deferred to 0.7.2 PR-4 | **CLOSED** | `9cb44c3` (prompt), `2916626` (CHANGELOG+docs), `2776164` (ADR+closure), closure in this commit | optional |

**EU-5f context.** EU-7 could not validly measure the locked config until three
engine defects surfaced during harness construction were fixed (full writeup:
`dev/plans/runs/0.7.1-EU-7-findings.md`):
- **A** — projection workers had no panic/fault guard, so a faulting worker
  wedged `drain` into `EngineError::Scheduler`. Fixed with a `catch_unwind`
  guard mirroring the reader pool's `LiveGuard`.
- **B** — mean-centering (locked ON) only pinned via the `write_vector_for_test`
  test seam; the production `engine.write`→projection path never pinned, so
  real ingests were quantized un-centered. Fixed by porting the pin into
  `commit_projection_outcomes` (commit-gate-serialized) + an open-time recovery
  pin.
- **C** (the actual EU-7 stall) — `CandleBgeEmbedder` did not truncate to 512
  tokens, so any >512-token doc errored (`index-select invalid index 512`);
  long cnn_dailymail docs error-retried past the per-batch drain timeout. Fixed
  with tokenizer `with_truncation(512)`.

## Per-AC scoreboard

Dev-box = this 24-core runner against the real `bge-small-en-v1.5` embedder
over `data/corpus-data/raw/*.jsonl` (7,667 real docs). **Canonical N=1M is
0.7.2 PR-3's job** — dev-box numbers here are scouting, not the verdict.

| AC | Synthetic-fixture (perf_gates.rs) | EU-7 real-corpus dev-box (N=7667) | Budget / floor | Status |
|---|---|---|---|---|
| AC-013 p50 latency | ~12-16 ms (isotropic) | 25 ms | ≤ 80 ms | **GREEN** (dev-box) |
| AC-013 p99 latency | ~16 ms (isotropic) | 40 ms | ≤ 300 ms | **GREEN** (dev-box) |
| AC-013b recall@10 | 0.5124 @ N=10K isotropic (noise floor) | 0.828 (95% CI 0.796–0.858, σ=0.0165) | current floor 0.90 (re-pin = PR-2) | **RED vs 0.90** — measured + surfaced; see honesty report |
| AC-019 stress p99 | ~131 ms (isotropic) | 343 ms (bound 405 ms) | ≤ max(10×baseline, 150 ms) | **GREEN** (dev-box) |

At N=1000 the same query set gives recall@10 0.831 (CI 0.803–0.856), p50 16 ms /
p99 30 ms, stress p99 205 ms — recall is nearly flat across N (fixed query set;
the extra cnn_dailymail docs are topically distinct distractors). Latency/stress
are dev-box scouting; canonical N=1M is 0.7.2 PR-3.

## What landed (EU-7)

**Measurement harness**: `src/rust/crates/fathomdb-engine/tests/eu7_real_corpus_ac.rs`
(feature-gated `default-embedder`, `AGENT_LONG`-gated). Constructs the real
`CandleBgeEmbedder` and supplies it via `EmbedderChoice::Caller` so the
identity-gated mean-centering apply paths engage exactly as for
`EmbedderChoice::Default` (mean pins at `MEAN_VEC_PIN_THRESHOLD`=256;
retrieval runs the locked K=192 bit-KNN + f32 rerank). Holding the embedder
`Arc` lets the harness compute the f32 ground truth with the **same model**
(measuring quantization loss, not cross-model quality). Queries are
synthesized per EU-0 §1.2 (title-or-lead-sentence) with the synthesis-target
doc excluded from both ground truth and production results. Incremental
seeding grows one engine across the N sweep so each doc is embedded once.

- Print line `EU7_NUMBERS n=… p50_ms=… p99_ms=… recall_at_10=… recall_ci_lo=…
  recall_ci_hi=… sigma=…` per N.
- Raw artifact: `dev/plans/runs/0.7.1-EU-7-measurements.json` (regenerable).
- Closure artifact: `dev/plans/runs/0.7.1-EU-7-output.json` (orchestration §8).

**Production-path fidelity caveat**: per `dev/design/embedder.md` §0.3 the
corpus mean is pinned once at the 256th doc and never recomputed. EU-0's
Python pipeline centered on the full-corpus mean; the engine centers on the
pinned-early mean — the production behaviour EU-7 measures. Any gap between
EU-7's number and EU-0's 0.933 partly reflects this (and the synthetic-query
+ smaller-corpus biases EU-0 §3 already flagged).

## EU-7 measurement results

Real `bge-small-en-v1.5` + mean-centering ON + K=192 over the real corpus
(`data/corpus-data/raw/*.jsonl`), dev-box scale, 100 synthesized queries,
bootstrap 1000 resamples. Run: 6127 s. Raw:
`dev/plans/runs/0.7.1-EU-7-measurements.json`; closure:
`dev/plans/runs/0.7.1-EU-7-output.json`.

| N | recall@10 | 95% CI | σ_bootstrap | p50 ms | p99 ms | stress p99 ms (bound) |
|---|---|---|---|---|---|---|
| 1000 | 0.831 | 0.803–0.856 | 0.0140 | 16 | 30 | 205 (307) |
| 7667 | **0.828** | 0.796–0.858 | 0.0165 | 25 | 40 | 343 (405) |

**`R_canonical_anchor` (for 0.7.2 PR-2): recall@10 = 0.828 at N=7667**, σ=0.0165.
Floor by the locked formula `R - 2σ` = 0.795, rounded down to **0.79**. PR-2
re-derives against canonical N=1M (PR-3) before the lock-flip.

## Honesty report

AC-013b recall@10 **does NOT meet the current 0.90 floor**: 0.828 at the full
real corpus (entire 95% CI 0.796–0.858 sits below 0.90; gap ~0.072). AC-013
latency and AC-019 stress both PASS (dev-box). The recall sits in EU-0 §3's
predicted real-BGE band (~79–90%) but is materially below EU-0's synthetic-
pipeline estimate of 0.933. Likely drivers, in order: (1) production mean-
centering pins the mean on the **first 256 ingested docs** (all
`bahmutov_dailylogs`, a single non-representative source) rather than the full-
corpus mean EU-0 used — the documented topic-drift limitation
(`embedder-decision.md` §3.4); (2) 512-token truncation (EU-5f Finding C) drops
content from long articles; (3) query-synthesis + production-path differences
vs EU-0's Python harness. Per the EU-7 mandate this slice does **not** re-pin
`AC013B_RECALL_FLOOR` or re-word the ADR. The mean-pin-on-first-256
representativeness issue is itself a finding for HITL beyond the floor number.

## Open items

- **EU-8 (docs + release prep)** — CLOSED (2026-05-30). 0.7.1 CHANGELOG
  section; `docs/embedder.md` user guide (+ install opt-in pointers + nav);
  `ADR-0.6.0-default-embedder.md` status → "implemented in 0.7.1";
  `dev/plans/0.7.1-implementation.md` closure doc. NEED-017/REQ-033 cross-cites
  verified. The version bump + `v0.7.1` tag were **deferred to 0.7.2 PR-4** by
  HITL (2026-05-30) rather than tagged locally (recall RED vs 0.90; floor lock +
  canonical validation are 0.7.2); workspace stays at 0.7.0.
- **`AC013B_RECALL_FLOOR` re-pinning** — DEFERRED to **0.7.2 PR-2**. PR-2
  derives the new floor as `R_canonical - 2*sigma_bootstrap` (rounded down
  to 0.01) via an ADR amendment to
  `dev/adr/ADR-0.7.0-vector-binary-quant.md` § 2 point 4 with HITL sign-off.
  EU-7 is the measurement, not the lock-flip — the current `0.90` constant
  in `tests/perf_gates.rs` and the ADR recall language are untouched.
- **`v0.7.1` tag push** — DEFERRED to **0.7.2 PR-4** push sequence.
- **Canonical-CI N=1M AC validation** — owned by **0.7.2 PR-3** (real
  corpus + synthetic replicates to N=1M; the verdict-quality signal that
  confirms or overrides EU-7's dev-box scouting numbers).

## Pointer forward

The 0.7.2 RELEASE-HARDENING campaign starts at PR-0 (reconciliation), per
`dev/plans/prompts/0.7.2-RELEASE-HARDENING-HANDOFF.md`. PR-0 reads this
STATUS file; PR-2 consumes `r_canonical_anchor` from
`dev/plans/runs/0.7.1-EU-7-output.json`; PR-3 dispatches canonical-CI.

## Compaction-resume checklist

1. `git log --oneline v0.7.0..HEAD | grep EU-` — confirm the scoreboard SHAs.
2. EU-7 numbers live in `dev/plans/runs/0.7.1-EU-7-measurements.json` and
   `0.7.1-EU-7-output.json`; regenerate via
   `AGENT_LONG=1 cargo test --release -p fathomdb-engine --features default-embedder --test eu7_real_corpus_ac -- --nocapture`.
3. Next slice is **EU-8** (docs + release prep). Floor re-pin is **not** EU-8;
   it is 0.7.2 PR-2.
