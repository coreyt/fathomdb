# Default embedder — decision register

**Purpose.** Single-page index of every parameter decision behind the
fathomdb default embedder. For each: what was considered, what was
picked, why, where the research lives, where the empirical data lives,
and the lock status.

**Audience.** Future orchestrators, reviewers, and operators trying to
understand or revisit a parameter without trawling six ADRs and a
research notebook.

**Companion documents.**
- `dev/design/embedder.md` — the architectural design (dispatch pool,
  warmup, timeout, loader EMB-5 sub-design).
- `dev/adr/ADR-0.6.0-default-embedder.md` — the model + runtime
  decision.
- `dev/adr/ADR-0.6.0-embedder-protocol.md` — the trait invariants
  every impl (including this default) must satisfy.
- `dev/adr/ADR-0.6.0-vector-identity-embedder-owned.md` — identity
  belongs to the embedder; profile-pinning is fail-closed.
- `dev/adr/ADR-0.7.0-vector-binary-quant.md` — the binary-quant +
  rerank pipeline this embedder feeds.
- `dev/adr/ADR-0.7.1-default-embedder-weight-fetch.md` *(pending)* —
  the NEED-017/REQ-033 download-exception ADR (0.7.1 EU-1).
- `dev/notes/0.7.1-default-embedder-research.md` — EU-0 empirical
  research report (recall@10 K-sweep + mean-centering ablation).
- `dev/plans/runs/STATUS-perf-vector-quant.md` — 0.7.0 PVQ STATUS
  including the isotropic-vs-real direction correction.

**Lock status legend.**
- 🔒 **Locked** — set in an accepted ADR or via explicit HITL decision;
  changing requires an ADR amendment with HITL sign-off.
- 🟡 **Recommended** — current best understanding; HITL has reviewed
  but the value may shift inside its bound on next measurement.
- ⏳ **TBD** — explicit deferred decision; the owning slice is named.

---

## 1. Model selection

### 1.1 Embedder family

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | `BAAI/bge-small-en-v1.5` |
| **HF snapshot** | `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a` |
| **Locked by** | `dev/adr/ADR-0.6.0-default-embedder.md` § 2 (Accepted 2026-04-27) + EU-0 empirical confirmation 2026-05-27 |
| **Empirical anchor** | `dev/notes/0.7.1-default-embedder-research.md` § 2.1 |

**Considered alternatives** (EU-0 sweep):

| Candidate | Dim | recall@10 K=64 | Disposition |
|---|---|---|---|
| **bge-small-en-v1.5** | 384 | **0.793** | **Picked** |
| `intfloat/e5-small-v2` | 384 | 0.448 | Rejected — collapses under sign-bit quant; consistent with HF blog (e5-base-v2 retention 74.77%) |
| `BAAI/bge-base-en-v1.5` | 768 | 0.885 | Held as fallback — recall +9 pp over bge-small at K=64 but 2× storage, 2.8× embed wall time, ~2.5–3× search latency. Pareto-inferior once K is allowed to move (bge-small+mc at K=128 = 0.907 hits the same band at half cost). |

**Considered but not benchmarked** (EU-0 § 5 escalation options):
`mxbai-embed-xsmall-v1`, `nomic-embed-text-v1.5-distilled`. Skipped
because bge-small already cleared the floor with mean-centering and
escalation cost was not justified.

**Why bge-small wins**:
- Empirical Pareto-best on (recall, latency, storage) once K can move.
- Cleanest binary-quant retention of the 384d candidates tested.
- BGE family's MTEB Retrieval score (51.68) is close to bge-base
  (53.25) — gap is small and the on-device cost asymmetry is large.
- Fits the candle-transformers `bert::BertModel` surface already
  selected in `dev/deps/candle-transformers.md`.

### 1.2 Dimension

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | 384 |
| **Locked by** | Intrinsic to bge-small-en-v1.5; flows through to `bit[384]` schema |

**Considered**: 384 (bge-small / e5-small / MiniLM family) and 768
(bge-base / e5-base / nomic-embed). 768 rejected per § 1.1.

### 1.3 Runtime

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | candle-transformers `bert::BertModel`, pure-Rust, in-process |
| **Locked by** | `dev/adr/ADR-0.6.0-default-embedder.md` § 2; `dev/adr/ADR-0.6.0-subprocess-bridge-deferral.md` |

**Considered**:
- candle in-process *(picked)*
- sentence-transformers in-process — dropped from default path per
  `dev/deps/sentence-transformers.md`; available to callers via their
  own `Embedder` impl.
- `ort` (ONNX Runtime) — rejected for 0.6.0 per
  `dev/deps/candle-core.md:47` ("not net win" given wheel-build and
  abi3 cost).
- Sidecar / subprocess — rejected by
  `dev/adr/ADR-0.6.0-subprocess-bridge-deferral.md`; reserved as a 0.7+
  fallback only if wheel-size or compile-time pain forces it.

**Why candle in-process**: only pure-Rust path to in-process embedding;
no Python or external process; matches the local-first single-process
posture; satisfies `EmbedderIdentity` invariants 2 (no engine callbacks)
and 4 (engine-owned thread pool) without any IPC seam.

---

## 2. Embedding-time pipeline

### 2.1 Tokenization

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | `tokenizers::Tokenizer` (WordPiece for BGE) |
| **Locked by** | `dev/deps/tokenizers.md:6` |
| **Max tokens** | 512 (BGE config); truncation `True` |

**Considered**: HF `tokenizers` vs reimplementation. Reimplementation
rejected — `tokenizers` is the canonical Rust impl and is already a
candle ecosystem dep.

### 2.2 Pooling

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | Mean-pool over attention mask: `(hidden_states * mask).sum(dim=seq) / mask.sum(dim=seq)` |
| **Locked by** | `dev/adr/ADR-0.6.0-default-embedder.md` § HITL-override of critic-EMB-1 |
| **Research anchor** | EU-0 § 1.3 step 1 ("not unconditional mean across pad tokens") |

**Considered alternatives**:
- **CLS pooling** — critic-EMB-1 originally argued for CLS. HITL
  overrode because "fathomdb's vectors store canonical information for
  agentic search; mean-pool empirically gives better search accuracy
  for that use case than CLS pooling on BGE-class models" (quoted from
  ADR-0.6.0-default-embedder).
- **Max pooling**, **last-token pooling** — not considered; BGE family
  is mean-pool-trained.

### 2.3 Normalization

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | L2-norm, applied **after** pool, **before** sign-quantization |
| **Locked by** | `dev/adr/ADR-0.6.0-default-embedder.md`; `dev/adr/ADR-0.6.0-embedder-protocol.md` Invariant 1 |
| **Tolerance** | `(‖v‖ − 1.0).abs() < 1e-5` debug-asserted |
| **Research anchor** | EU-0 § 6.7 self-review checklist |

**Why L2-norm post-pool pre-quant**:
- Embedder protocol Invariant 1 requires unit-norm output.
- Sign-bit quantization is rotation-invariant on the hypersphere, so
  pre-quant normalization is the canonical form expected by the rerank
  step (`vec_distance_l2` on unit vectors recovers cosine ordering).
- L2 pre-quant also lets the centered variant (§ 3) subtract a single
  corpus-mean unit vector cleanly.

### 2.4 Query / passage prefix

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | **NONE** for BGE family — neither queries nor documents get a prefix |
| **Locked by** | Family standard; EU-0 § 1.3 step 3 |
| **Research anchor** | EU-0 § 3 literature cross-check (e5 collapse partially attributed to missing `"passage: "` prefix; BGE has no equivalent) |

**Considered**: applying E5-style `"query: "` / `"passage: "` prefixes
on the off chance they help. Rejected — BGE is not trained with this
asymmetry; literature confirms it would hurt, not help.

---

## 3. Mean-centering

| | |
|---|---|
| **Status** | 🔒 Locked **ON** |
| **Picked** | Subtract corpus-mean f32 vector from each doc and query vector **before** sign-quantization. Rerank step still uses original (uncentered) L2-normed f32 vectors. |
| **Locked by** | EU-0 § 4 recommendation; orchestrator+HITL acceptance 2026-05-28 |
| **Research anchor** | EU-0 § 2.2 (ablation) + § 2.3 (paired bootstrap on diff: +0.050, 95% CI +0.024 … +0.078) |

### 3.1 Why ON

Cheapest fix on the table for the recall gap. Single-axis ablation on
bge-small at K=64 shows a statistically significant +5.0 pp lift
(paired bootstrap 95% CI excludes zero). Mechanism: real BGE
embeddings live on a narrow cone of the hypersphere (Ethayarajh 2019;
Gao 2019 "representation degeneration"); centering removes the cone's
shift before sign-quantization, restoring per-bit entropy.

### 3.2 Per-K lift (measured)

| K | plain | centered | lift | significance |
|---|---|---|---|---|
| 64 | 0.793 | 0.843 | **+5.0 pp** | significant |
| 96 | 0.849 | 0.880 | +3.1 pp | significant |
| 128 | 0.882 | 0.907 | +2.5 pp | significant |
| 256 | 0.933 | ~0.945 | +1.2 pp | **n.s.** |

Lift attenuates geometrically with K (mc's job is correcting tight
candidate sets; larger K dilutes the benefit). Practical ceiling for
bge-small+mc is ~0.945.

### 3.3 Storage

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | One f32 corpus-mean vector per embedder profile, stored in `_fathomdb_embedder_profiles` (new column) |
| **Migration** | step 10 (Pack 1 was step 9) |
| **Owning slice** | 0.7.1 EU-2 (loader sub-design) + EU-5 (engine wiring) |

**Considered alternatives**:
- Per-source-type mean (one mean per `source_type` partition_key) —
  rejected as scope creep; corpus-wide mean is the standard approach.
- Don't store; recompute on every open — rejected because corpus-mean
  computation is O(N) and adds startup latency proportional to corpus
  size.

### 3.4 Recomputation trigger

| | |
|---|---|
| **Status** | ⏳ TBD — owning slice **0.7.1 EU-2** |
| **Recommended** | Recompute when corpus row count changes by **> 25%** since last write, OR on explicit operator command |
| **Open questions** | (a) Is row-count the right signal vs e.g. distribution-shift metric? (b) Should the recomputation happen in `Engine.open` (blocks startup) or via a background job? (c) What surface does "explicit operator command" use — CLI verb, Engine API, both? |

**Why 25% threshold**: ad hoc, derived from "small enough to catch
meaningful corpus growth, large enough to avoid every-write
recomputation." Not empirically validated. EU-2 design should sanity-
check against a corpus-shift sensitivity study or accept that the
threshold is a heuristic and pin it as such.

---

## 4. Quantization + reranking pipeline

### 4.1 Quantization method

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | Sign-bit quantization via sqlite-vec `vec_quantize_binary(embedding)` |
| **Locked by** | `dev/adr/ADR-0.7.0-vector-binary-quant.md` § 2 point 1 |
| **Bit column** | `embedding_bin bit[384]` (dim-parameterized via `migrate_vector_partition_to_pack1`) |

**Considered alternatives** (per the same ADR § 3):
- Vectorlite HNSW SQLite extension — rejected (stale upstream, no 1M
  benchmark, additional architectural lever).
- sqlite-vec ANN alpha — rejected (alpha-tagged).
- Rust-side `usearch` / `instant-distance` — rejected (blast radius too
  large; data-encoding lever closes the gap without it).
- Embedder dim reduction — rejected by HITL Q2 lock.
- Partitioning alone — kept as bundled deliverable but not load-bearing
  for the gate (AC-013 fixture is single-kind so partitioning yields
  zero benefit on the gate).

### 4.2 Rerank distance function

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | `vec_distance_l2` over the retained f32 `embedding` column |
| **Locked by** | `dev/adr/ADR-0.7.0-vector-binary-quant.md` § 2 point 2; Pack 2 SQL at `lib.rs:2348-2362` |

**Considered alternatives**:
- `vec_distance_cosine` — equivalent on unit vectors (which we
  guarantee per § 2.3); L2 chosen because it matches the pre-Pack-2
  implicit MATCH distance default and minimizes behavior change for
  rank-order-sensitive tests.

### 4.3 Bit-KNN candidate count (K)

| | |
|---|---|
| **Status** | 🔒 Locked at **K=192** (2026-05-28) |
| **Picked** | `TOP_K_BIT_CANDIDATES = 192` |
| **Locked by** | Orchestrator+HITL decision 2026-05-28; supersedes the K=64 default in `dev/adr/ADR-0.7.0-vector-binary-quant.md` § 2 point 2 (ADR-amendment slice owns the cross-cite) |
| **Research anchor** | EU-0 § 2.1 K-sweep; EU-0 mean-centering K-extended ablation 2026-05-28 |
| **Empirical basis** | K=128 mc = 0.907 (measured); K=256 mc ≈ 0.945 (measured, mc lift n.s.); K=192 interpolated estimate ≈ 0.925 — ~2.5 pp cushion above the 0.90 floor |

**Considered alternatives**:

| K | bge-small + mc recall@10 | Pros | Cons |
|---|---|---|---|
| 64 | 0.843 | Cheapest rerank; matches PVQ-default | Below 0.90 floor |
| 96 | 0.880 | Marginal cost over 64 | Below 0.90 floor |
| 128 | 0.907 | Cheapest K that clears 0.90 | Thin 0.7 pp cushion; sensitive to corpus-shape variance |
| **192** | **~0.925** (interp) | **2.5 pp cushion above floor; small absolute rerank cost vs K=128** | Marginal CPU vs K=128 |
| 256 | ~0.945 | Maximum recall on bge-small+mc | Diminishing returns; mc lift n.s.; rerank cost grows |

**Why K=192 over K=128**:
- Measured K=128 clears 0.90 by only 0.7 pp. EU-0 § 3 flags two
  measurement biases that push real-corpus recall *down* from the
  research number: (a) synthetic queries are noisier than relevance-
  judged sets, (b) the 7,667-doc corpus is smaller than canonical-CI
  N=1M. A 2 pp cushion is prudent against both.
- Cost asymmetry: K is cheap (linear in Phase-2 rerank only; embed +
  Hamming are K-independent). On bge-small (384d) the K=64→192
  rerank-cost delta is much smaller than the storage/embed cost of
  switching to bge-base.

**Why not K=256**:
- mc lift at K=256 is non-significant (+1.2 pp); past K=192 we're
  buying rerank cost for noise.
- ~0.945 is the practical ceiling for bge-small+mc; if the team
  needs >0.945 the answer is switching to bge-base, not raising K.

### 4.4 Engine returns top-N

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | 10 (fixed) |
| **Locked by** | Pack 2 SQL `LIMIT 10` at `lib.rs:2360` |

Not currently configurable. Out of scope for 0.7.x.

---

## 5. Recall floor

### 5.1 `AC013B_RECALL_FLOOR`

| | |
|---|---|
| **Status** | ⏳ TBD — owning slice **0.7.2 PR-2** |
| **Current value** | 0.90 (calibrated for isotropic synthetic, no longer the right shape) |
| **Recommended derivation** | `R_canonical_CI - 2σ_bootstrap`, rounded down to 0.01 |
| **Locked by (eventually)** | ADR amendment to `dev/adr/ADR-0.7.0-vector-binary-quant.md` § 2 point 4 |

**Why the current 0.90 is wrong-shape**:
- 0.90 was set when the AC-013 fixture used `VaryingEmbedder` (sparse
  6-coord, then dense isotropic post-2026-05-27 Option 1). Isotropic
  random vectors are the noise-limited case for sign-bit ANN (see
  `dev/plans/runs/STATUS-perf-vector-quant.md` § Fixture-replacement
  evaluation, post-correction). The 0.90 number doesn't translate to
  real-embedder data.

**Decision process for the new floor**:
1. EU-7 measures recall@10 on real bge-small+mc at K=192, canonical-CI
   N=1M. Call this `R_canonical`.
2. PR-2 derives the floor as `R_canonical - 2σ_bootstrap` (one-sided
   lower bound; ~95% confidence production lands ≥ floor).
3. PR-2 rounds down to the nearest 0.01.
4. ADR amendment + HITL sign-off.

**Honesty constraint**: per the 0.7.0 ship precedent (commit `38d5f4f`
message), if `R_canonical < 0.90` the floor drops to match the honest
measurement — **the floor is not gerrymandered to fit a desired pass**.

---

## 6. Identity + profile-pinning

### 6.1 Identity revision string

| | |
|---|---|
| **Status** | ⏳ TBD — owning slice **0.7.1 EU-5** |
| **Recommended** | Either `"0.7.1"` (release-axis) or the HF snapshot SHA prefix `5c38ec7c` (full model provenance) |
| **Locked by (eventually)** | `default_embedder_identity()` in `fathomdb-engine/src/lib.rs:3507` |

**Trade-off**:
- Release-axis (`"0.7.1"`) — bumps every release; any 0.7.x→0.7.y open
  fails-closed on identity mismatch even when weights are unchanged.
  Cleaner narrative for "this was shipped in 0.7.1".
- HF SHA (`5c38ec7c`) — only bumps when weights actually change.
  Better matches the load-bearing fact (the model is the model
  regardless of release line). Slightly opaque for humans.

**Recommendation**: HF SHA prefix as `revision`, release line in
`name` (`"fathomdb-bge-small-en-v1.5"`). Lets the identity reflect
"weights changed" without re-pinning on every release.

EU-5 owns the final call.

### 6.2 Profile-pinning behavior

| | |
|---|---|
| **Status** | 🔒 Locked |
| **Picked** | Fail-closed on identity mismatch |
| **Locked by** | `dev/adr/ADR-0.6.0-vector-identity-embedder-owned.md`; `lib.rs:3602-3607` |
| **0.6.x → 0.7.1 migration** | Existing workspaces with `fathomdb-noop` pinned will fail-closed on re-open with the default embedder. Intentional per `dev/adr/ADR-0.8.0-embedder-identity-change-workflow.md`. Release-notes must document. |

---

## 7. Loader behavior (download + cache)

| | |
|---|---|
| **Status** | ⏳ TBD — owning slice **0.7.1 EU-2** (sub-design) → **EU-3** (impl) |
| **Legal basis** | `dev/adr/ADR-0.7.1-default-embedder-weight-fetch.md` *(EU-1)* — NEED-017 / REQ-033 exception |
| **Sub-design** | Will live in `dev/design/embedder.md` post-EU-2 (10 sections enumerated in `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` § EU-2) |

Headlines (subject to EU-2 confirmation):
- Transport: `ureq` blocking client; HF resolve URL pattern; 302 →
  CloudFront handling.
- Cache: `<dirs::cache_dir>/fathomdb/embedders/<model-sha-prefix>/<file>`.
- Verification: sha256 against pinned constants; no "trust on first
  use."
- Concurrency: `fs2::FileExt::lock_exclusive` on a `.lock` sibling.
- Visibility: `default_embedder_download` event in
  `OpenReport.embedder_events`.

---

## 8. Bindings

| | |
|---|---|
| **Status** | 🔒 Locked (surface shape); ⏳ TBD (parameter names) — owning slice **0.7.1 EU-6** |
| **Picked** | Binary toggle: caller opts in via a `bool` flag; no callable embedder bridge in 0.7.1 |
| **Locked by** | `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` § EU-6 |

**Recommended ergonomics** (per EU-6):
- Python: `use_default_embedder: bool = False` on `Engine.open(...)`.
- TypeScript: `useDefaultEmbedder?: boolean` on `OpenOptions`.

Custom caller-supplied Py/TS embedders are explicitly **deferred to
0.8.x** (require PyO3 callback bridges + protocol Invariant 3
guarantees; out of scope for 0.7.1).

---

## 9. Embedder-protocol invariants (carry-forward from ADR-0.6.0-embedder-protocol)

These are not negotiable per impl — every embedder must satisfy them,
the default included. Listed here for completeness.

| # | Invariant | Why |
|---|---|---|
| 1 | Unit-norm output (debug-asserted ±1e-5) | Lets rerank step use cosine via L2; centering math stays clean |
| 2 | No engine callbacks from inside `embed()` | Prevents re-entrancy deadlocks |
| 3 | No `pyo3-log` emission during `embed()` | Killed a 0.5.x GIL deadlock; see `dev/archive/pyo3-log-gil-deadlock-evidence.md` |
| 4 | Engine-owned thread pool, sized `num_cpus::get()` default | Caller doesn't choose where the embedder runs |
| 5 | 30s per-call timeout | Caller doesn't choose how long the embedder can hang |

---

## 10. Performance / cost profile (informational)

Empirical anchors from EU-0 § 6.4 (24-core CPU, 7,667-doc corpus,
N=100 queries):

| Operation | Cost |
|---|---|
| Corpus embed (one-time) | ~289 s for bge-small (vs ~800 s for bge-base) |
| Query embed | ~9 ms / query (bge-small CPU); ~22-28 ms / query (bge-base) |
| Per-query Hamming Phase 1 (N=7.7K) | sub-ms |
| Per-query f32 rerank Phase 2 (K=192) | sub-ms |

Canonical-CI N=1M projections (PR-3 will measure):
- Corpus embed: ~10 hours on a 24-core CPU without GPU. CI step "warm
  embedder cache before AC-013 runs" amortizes this across all
  perf-canonical dispatches.
- Per-query end-to-end: dominated by embed (~9 ms) + Hamming
  (~N×384/8 = 48 MB scan, ~10-30 ms depending on cache) + rerank
  (~K×384 = 74 K ops, sub-ms) ≈ 25–50 ms / query CPU. Well within
  the 80/300 ms AC-013 budget.

---

## Change log

- **2026-05-29** — Initial decision register. Locks K=192, real
  embedder for AC-013/AC-019, mean-centering ON. Defers floor,
  identity revision string, recomputation trigger, loader headlines.

Future amendments: append a dated bullet; each bullet must cite the
HITL message-id or commit SHA that authorized the change.
