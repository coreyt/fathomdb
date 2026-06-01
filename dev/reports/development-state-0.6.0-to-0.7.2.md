# Development State Report — FathomDB 0.6.0 → 0.7.2

_Longitudinal history of the 0.6.x → 0.7.2 arc. One section per semver minor
line (0.6.x, 0.7.0, 0.7.1, 0.7.2). Every claim is grounded in a file path, ADR
id, AC id, or commit sha; anything not verifiable is marked `(unverified)`._

**Generated:** 2026-06-01. **Repo HEAD at writing:** `e00991f` (local `main`,
55 commits ahead of `origin/main`, unpushed).

---

## Executive summary — the arc 0.6.0 → 0.7.2

FathomDB is a local-first retrieval engine on SQLite (FTS5 + `sqlite-vec`) with
Rust, Python, and TypeScript SDKs. The 0.6.0→0.7.2 arc is the story of taking a
GA'd-but-perf-deferred engine and making both its **vector latency** and its
**default embedder** real and defensible — without ever shipping a number the
team could not stand behind.

- **0.6.0 (2026-05-19, released)** is the first stable release: the five-verb
  SDK surface, typed error hierarchy, op-store + projection scheduler + reader
  pool, two-axis versioning. It explicitly **deferred all four perf gates**
  (AC-012/013/019/020) and shipped only a `NoopEmbedder` — the default-embedder
  family (`BAAI/bge-small-en-v1.5`) was *decided* but not implemented.

- **0.6.1 (2026-05-25, released)** is a patch: surfaces `OpenReport` from both
  bindings (AC-068c/d), clears three Dependabot advisories, and carries an
  AC-012 canonical-runner re-measurement that came back **RED** (p50 140.95 ms
  vs a 20 ms budget) — the empirical fact that escalated the whole perf problem
  to a dedicated 0.7.0.

- **0.7.0 (HELD, unpushed)** is PERF-VECTOR-QUANT: the load-bearing change is
  **binary vector quantization + f32 rerank** (`ADR-0.7.0-vector-binary-quant`),
  a *data-encoding* change (not a second architectural lever — PCACHE2 keeps
  that title). It closes AC-013 **latency** but leaves AC-013b **recall** open,
  because the only available fixture (synthetic isotropic vectors) structurally
  cannot reach the 0.90 floor. The honest disposition: ship the latency win,
  surface recall as a known gap.

- **0.7.1 (HELD, unpushed)** is EMBEDDER-UNDEFER: ships the real default embedder
  `BAAI/bge-small-en-v1.5` (candle, dim 384, mean-centering, opt-in per binding),
  with a tightly-scoped first-use weight-fetch exception
  (`ADR-0.7.1-default-embedder-weight-fetch`). Real-corpus measurement (EU-7)
  put recall@10 at **0.828** — apparently below the 0.90 floor — and three
  engine defects (EU-5f: production mean-centering never pinned, projection
  workers not fault-isolated, >512-token inputs erroring) were fixed in the
  process. The floor re-derivation was deferred to 0.7.2.

- **0.7.2 (IN PROGRESS, unpushed)** is RELEASE-HARDENING — the slice that turns
  the held work into something pushable. Its pivotal finding: the 0.828 "recall
  gap" was a **measurement artifact** (exclude-after + body-string ground truth),
  not a defect; the corrected **ANN-fidelity** number is **0.937 (CI
  0.913–0.957)** and the **0.90 floor HOLDS**. This is quantization fidelity vs
  the same model's exact f32 top-10 — *not* IR relevance (the separate embedder
  IR ceiling, ~0.571, is deliberately not a gate). 0.7.2 also: hardened the
  embedder path (PR-9 — watchdog + serialization + circuit breaker); carved the
  unjustified auto-mean-drift detector out to 0.8.x while keeping the manual
  doctor verb; and reframed the latency budget as **tiered** (10k binding for
  0.x/1.x; 100k/1M tracked post-1.0) with **local-only** heavy measurement plus a
  CI read-path smoke, because real-embedder N=1M is infeasible (~166 h seed).

**Release-state truth (verify before citing):** `v0.7.0` exists as a *local*
tag (commit `38d5f4f`) but is **held/unpushed**; `v0.7.1` is **not yet tagged**;
0.7.2 is **in progress**. 0.7.2 PR-4 is the slice that writes final release
notes, creates the `v0.7.1` tag, and pushes `main` + both tags. PR-4 and
PR-5/6/7/8 have **not run**. Nothing in 0.7.x has shipped.

**Through-line:** a per-query **O(N) linear scan** on the vec0 vector table (no
ANN index). Binary quant shrank the bytes-per-row and bit-KNN+rerank cut the
constant, but the asymptote is unchanged. An ANN index (HNSW/IVF/DiskANN) is the
named **post-1.0** follow-up; until it lands, the 100k/1M latency tiers are
tracked, not gated.

---

## 0.6.x — first stable release + perf-deferral patch

**Release state:** 0.6.0 released 2026-05-19; 0.6.1 released 2026-05-25 (both
tagged: `v0.6.0`, `v0.6.1`). 0.6.0-rc.2/3/4 and 0.6.1-rc.1 tags also exist.

### ADRs

0.6.x carries the large ADR-0.6.0-\* corpus (the architectural baseline; ~37
ADRs in `dev/adr/`). The ones load-bearing for this arc:

- **ADR-0.6.0-default-embedder** — status `implemented` (note: the file's status
  was flipped to "implemented in 0.7.1"; it was *accepted* 2026-04-25 as
  decision-only). Commits to `BAAI/bge-small-en-v1.5` via candle-transformers
  (`bert::BertModel`), mean-pool + L2-norm, dim 384. The model family was picked
  at 0.6.0; implementation was deferred 0.6.0 → 0.6.1 → 0.7.0 → 0.7.1.
- **ADR-0.6.0-text-query-latency-gates** — accepted 2026-04-27; set AC-012 at
  p50 ≤ 20 ms / p99 ≤ 150 ms (without a canonical-runner measurement). Later
  superseded for AC-012 by `ADR-0.7.0-text-query-latency-gates-revised`.
- **ADR-0.6.0-retrieval-latency-gates** — AC-013 anchor (later revised in 0.7.0).
- **ADR-0.6.0-vector-binary-quant** does **not** exist at 0.6.x (it is 0.7.0).
- Supporting: `ADR-0.6.0-no-shims-policy` (§54 patch-release contract — "no API
  breaks, bugfix-only" — is what forbade revising perf budgets in 0.6.x and
  forced the escalation to 0.7.0), `ADR-0.6.0-tier1-ci-platforms` (canonical
  runner shape), `ADR-0.6.0-embedder-protocol`, `ADR-0.6.0-vector-identity-
  embedder-owned`, `ADR-0.6.0-deprecation-policy-0-5-names`.

No ADRs removed in 0.6.x.

### Acceptance criteria

- **AC-012 / AC-013 / AC-019 / AC-020** — all four perf gates **DEFERRED** at
  0.6.0 (HITL re-confirmed 2026-05-17). At 0.6.1 close, an `AC012-measure` slice
  ran the canonical runner at N=1M and AC-012 came back **RED** (p50 140.95 ms,
  7.05× over; p99 458 ms, 3.05× over;
  `dev/plans/runs/0.6.1-AC012-measure-output.json`, workflow run 26346417896).
  This fired the Pack-7 un-defer trigger and escalated AC-012 (and AC-013/019/
  020) to 0.7.0.
- **AC-068c / AC-068d** — Python `engine.open_report()` / TypeScript
  `engine.openReport()` surface the structured open report; **closed in 0.6.1**
  (carry-over from the 0.6.0 GA `12-TX-OPENREPORT` deferral). See
  `dev/acceptance.md:1049–1059`.
- **AC-050c** — within-0.6.x removal-detect linter scenario
  (`dev/acceptance.md:810`); the `--base` anchor advanced `v0.6.0`→`v0.6.1` in
  the 0.6.1 BUMP (B-001 forward-retag).
- **Logical-id verbs** (`purge_logical_id`/`restore_logical_id`) re-targeted from
  0.7.x to **0.8.0** (HITL 2026-05-24).

REQ ids: REQ-010 (AC-012), REQ-011 (AC-013), REQ-017 (AC-019), REQ-018 (AC-020),
REQ-064 (AC-068a/b/c/d), REQ-033/NEED-017 (no-network-on-open).

### Architecture & design changes

- Two-axis versioning (Axis W workspace lockstep + Axis E independent
  embedder-api semver) — 0.6.1 was the first post-GA exercise of axis-E
  independence (axis-E held at 0.6.0 per Wake decision `d-001`).
- Design docs baseline: `dev/design/{engine,retrieval,projections,scheduler,
  op-store,recovery,vector,embedder,lifecycle,migrations,errors,bindings}.md`.
- `dev/roadmap/0.8.0.md` positions 0.8.0 as the knowledge-store/retrieval anchor
  for the Memex consumer.

### Key code changes

- 0.6.0 is the rewrite (`0.6.0-rewrite` baseline; no 0.5.x→0.6.0 shims). Engine,
  op-store, scheduler, reader pool, five-verb surface, PyO3 + napi-rs bindings.
- 0.6.1: `OpenReport` accessors on both bindings; `pyo3` 0.22.6→0.24.1
  (RUSTSEC-2025-0020, 24 `*_bound` callsite renames); `js-yaml` 4.1.0→4.1.1
  (GHSA-mh29-5h37-fv8m); CVE-2024-3651 confirmed false-positive.

### Key test changes

- AC-012 canonical-runner measurement harness (`perf-canonical.yml`) exercised at
  N=1M; closure JSON `dev/plans/runs/0.6.1-AC012-measure-output.json`.
- Removal-detect linter (AC-050c) + `scripts/tests/test_set_version.sh` axis-E
  independence sentinel (#13).

---

## 0.7.0 — PERF-VECTOR-QUANT (binary quant + f32 rerank)

**Release state:** workspace bumped to 0.7.0 (`38d5f4f`); local tag `v0.7.0`
exists at `38d5f4f` but is **HELD / unpushed** (pushed by 0.7.2 PR-4). Commit arc
`v0.6.1..v0.7.0`. Ledger: `dev/plans/runs/STATUS-perf-vector-quant.md`.

### ADRs

- **ADR-0.7.0-vector-binary-quant** (new; date 2026-05-27) — **status `locked`**
  (flipped 2026-06-01 after the 0.7.2 PR-2 recall reframe; § 2 cites the
  corrected ANN-fidelity 0.937, floor kept 0.90). Decision: binary quantization
  to a sibling
  `bit[768]` column + two-phase bit-KNN (K=64) + f32 rerank; `source_type`
  partition_key + metadata columns; recall floor ≥ 0.90 recall@10; sqlite-vec
  pin stays `=0.1.7`; embedder unchanged; single-writer/projection-cursor
  contracts preserved. Explicitly a **data-encoding change, not a second
  architectural lever**.
- **ADR-0.7.0-text-query-latency-gates-revised** (new; date 2026-05-25) — **status
  `locked` for AC-013/AC-019** (tiered budget, HITL 2026-06-01, 0.7.2 PR-3);
  AC-012/AC-020 remain as drafted (owned by their own slices). Supersedes
  ADR-0.6.0-text-query-latency-gates for AC-012; pins budgets against
  canonical-runner measurement. Of the proposed AC ids AC-071..AC-075, **AC-072
  (revised AC-013) and AC-073 (revised AC-019) were appended to
  `dev/acceptance.md`** (2026-06-01); AC-071/AC-074/AC-075 remain proposed (their
  ACs not yet locked / not built).
- **ADR-0.7.0-ac020-architectural-lever** (new) — status `draft, HITL-required`.
  Names **PCACHE2** as the single 0.7.0 architectural lever; AC-020 speedup gap
  (measured 3.530× vs ≥ 5.33× bound) is this lever's job.

No ADRs removed.

### Acceptance criteria

- **AC-013 latency** — re-pinned to **80 / 300 ms** (`AC013_BUDGET_P50/P99`,
  `d468999`). Dev-box MET (p50 12 / p99 16 ms at N=10K post-fix). Canonical N=1M
  lock-flip deferred.
- **AC-013b recall@10 ≥ 0.90** — **held OPEN** at ship; **NOT MET** on the only
  available fixture. New test `ac_013b_recall_at_10_floor` (`d468999`).
  Sparse `VaryingEmbedder` = 0.1572; dense isotropic (Option 1, `38f5e3a`) =
  **0.5124** (isotropic noise floor — real embeddings are *easier* for sign-bit
  ANN, so this bounds from below, not above). Real-embedder validation deferred
  to 0.7.1 EU-7. The ADR recall claim was deliberately **not** retconned at 0.7.0
  (`STATUS-perf-vector-quant.md` "Critical finding").
- **AC-019 stress** — dev-box MET (p99 131 ms post-Pack-2). AC-020/AC-012/
  AC-017/AC-018 unchanged.
- New AC ids in the revised-budgets ADR: **AC-072** (AC-013 revised) and
  **AC-073** (AC-019 revised) were **appended to `dev/acceptance.md`** (2026-06-01,
  resolved later in the 0.7.2 line) with supersede pointers on AC-013/AC-019 and
  traceability + coverage-trace rows. **AC-071** (AC-012 revised), **AC-074**
  (AC-020 revised), **AC-075** (top-K LIMIT) remain *proposed* — their underlying
  ACs are owned by other slices / not yet built.

### Architecture & design changes

- **Design memo** `dev/design/0.7.0-vector-quant-pack1.md` (D1–D8 resolved
  against code anchors; codex PASS after 3 BLOCK iterations on P1-DESIGN).
- **Schema**: `fathomdb-schema` migration step 9
  (`migrations/009_vector_binary_quant.sql`).
- Vector search remains a per-query **O(N) linear scan** over vec0 (no ANN
  index); ANN is the named post-0.7.0 follow-up
  (`dev/notes/pcache2-followups.md`, `ADR-0.7.0-vector-binary-quant` §4).

### Key code changes (commits)

- `4a95cfd` — fix `engine.write` batch-collapse (one write cursor per row;
  previously a batch collapsed to ~1 unique vec0 row, masking recall + scanner
  issues). Regression test `tests/batch_write_per_row_cursor.rs`.
- `9b9f840`/`f5da3e4`/`7d4aa2c`/`d96c4b0` — Pack 1 schema migration + writer
  double-write (`embedding`, `embedding_bin`, `source_type`, `kind`,
  `created_at`); `resolve_source_type` 6-value lock with `doc→article` coercion;
  dim-aware reshape.
- `26ef3dc` — Pack 2 read path: two-phase bit-KNN `TOP_K_BIT_CANDIDATES`=64 +
  f32 rerank via `vec_distance_l2` in `read_search_in_tx`.
- `53a270d` — projection scanner enqueues the full inflight budget per cycle;
  `PROJECTION_INFLIGHT_LIMIT` 8→32 (dev-box seed ~11× faster).
- `38f5e3a` — Option 1 dense isotropic fixture for AC-013b.
- CORPUS-1..4 (`5c1e92a`..`d9a219d`) — ~7,667-doc multi-source real corpus under
  `data/corpus-data/` + chain generator + ingest harness + validation gates.

### Key test changes

- `ac_013b_recall_at_10_floor` (recall ≥ 0.90 vs in-test f32 brute-force GT).
- AC-013 budget constants re-pinned 80/300 ms (`tests/perf_gates.rs:149-150`).
- `tests/batch_write_per_row_cursor.rs` locks the per-row-cursor invariant.

---

## 0.7.1 — EMBEDDER-UNDEFER (real default embedder)

**Release state:** workspace stays at **0.7.0**; `v0.7.1` **NOT tagged** (HITL
2026-05-30 deferred the bump + tag to 0.7.2 PR-4). Campaign CLOSED (docs done,
tag deferred); 9 commits ahead of origin at close. Ledger:
`dev/plans/runs/STATUS-embedder-undefer.md`. Commit arc starts after `v0.7.0`
(parent `fe1d10f`).

### ADRs

- **ADR-0.7.1-default-embedder-weight-fetch** (new; date 2026-05-28) — status
  **accepted** (HITL 2026-05-28). Narrow exception to NEED-017 / REQ-033:
  default embedder MAY fetch its single pinned weight set from a fixed URL set on
  first use **only when the caller opts in**, cached + sha256-verified, visible
  in `OpenReport.embedder_events`. Scope guardrails: no arbitrary model fetch, no
  implicit fetch, no trust-on-first-use, no user-controllable URL; weight-fetch
  only (no telemetry).
- **ADR-0.6.0-default-embedder** — status changed to **implemented** ("in 0.7.1")
  via EU-8 (`2776164`).
- No ADRs removed.

### Acceptance criteria

- **AC-013b recall@10** — measured (EU-7) at **0.828** (95% CI 0.796–0.858,
  σ=0.0165) at N=7,667 on the real corpus; **RED vs the 0.90 floor**, surfaced to
  HITL. The floor constant and ADR language were **not** re-pinned (EU-7 is the
  measurement, not the lock-flip; deferred to 0.7.2 PR-2). `R_canonical_anchor`
  recorded for PR-2. **This 0.828 was later shown to be a measurement artifact
  (corrected to 0.937 in 0.7.2).**
- **AC-013 latency** — dev-box GREEN (real bge: p50 25 / p99 40 ms at N=7,667).
- **AC-019 stress** — dev-box GREEN (343 ms vs 405 ms bound at N=7,667).
- Canonical N=1M validation deferred to 0.7.2 PR-3.
- NEED-017 / REQ-033 amended with the opt-in weight-fetch exception clause.

### Architecture & design changes

- `dev/design/embedder.md` — EMB-5 loader sub-design + mean-centering sub-design
  (cache layout, atomic rename, sha verification, concurrent-load file lock,
  failure taxonomy, `embedder_events` shape). Mean pinned once at the first
  `MEAN_VEC_PIN_THRESHOLD`=256 docs, never silently recomputed (§0.3).
- `dev/design/embedder-decision.md` — decision register (every locked parameter:
  model, dim 384, K=192, mean-centering ON).
- `docs/embedder.md` — user guide (opt-in, first-use download, cache, offline,
  migration).

### Key code changes (commits)

- `af2e6e7` (EU-3) — default-embedder loader impl (cache + sha256 + atomic
  rename), `default-embedder` Cargo feature gates the network surface.
- `a18b1bf` (EU-4) — `CandleBgeEmbedder` via candle-transformers `BertModel`
  (mean-pool → L2-norm; dim 384).
- `c572228` (EU-5a1) — `EmbedderChoice` enum + `OpenReport` plumbing.
- `49cdcf4` (EU-5a2) — schema migration **step 10** (nullable `mean_vec BLOB`) +
  mean-centering pipeline + **K=64→192**.
- `1c0b760` (EU-5b) — default-embedder identity lock-flip
  (`fathomdb-bge-small-en-v1.5`) + CLI warm-cache.
- `c27712f`/`63886fc` (EU-6) — Python `use_default_embedder` / TypeScript
  `useDefaultEmbedder` open flags + `EmbedderEvent` typed-union surface +
  per-platform wheel-size gate.
- **EU-5f engine fixes** (surfaced while building the EU-7 harness;
  `dev/plans/runs/0.7.1-EU-7-findings.md`):
  - `574ef28` (GREEN A+B) — (A) projection workers fault-isolated with a
    `catch_unwind` guard mirroring the reader pool's `LiveGuard` (a faulting
    worker previously wedged `drain` into `EngineError::Scheduler`); (B)
    **production mean-centering now pins in `commit_projection_outcomes`**
    (commit-gate-serialized) + an open-time recovery pin — previously the pin
    only ran via the `write_vector_for_test` seam, so real ingests were
    sign-quantized un-centered.
  - `c719a12` (Finding C) — truncate embedder input to 512 tokens (long docs
    previously errored `index-select invalid index 512`).
  - HITL sign-off on the EU-5f slice recorded 2026-05-30 (memory:
    `eu5f-hitl-signoff.md`).

### Key test changes

- `tests/eu7_real_corpus_ac.rs` — real-corpus AC harness (feature-gated
  `default-embedder`, `AGENT_LONG`-gated): constructs the real `CandleBgeEmbedder`,
  computes f32 ground truth with the **same model** (so it measures quantization
  loss, not cross-model quality), excludes the query-source doc, prints
  `EU7_NUMBERS`. Raw: `dev/plans/runs/0.7.1-EU-7-measurements.json`.
- EU-5f RED tests (`2270520`) lock the production-path mean pin + fault isolation.

---

## 0.7.2 — RELEASE-HARDENING (in progress)

**Release state:** **IN PROGRESS, unpushed.** Workspace still `0.7.0`. Phase A
slices PR-1, PR-2(family), PR-9, PR-3 landed on local `main`; **PR-4 (release
notes + create `v0.7.1` tag + push `main` + both tags) NOT started**;
PR-5/6/7/8 NOT started. Local `main` ~55 commits ahead of `origin/main`. Ledger:
`dev/plans/runs/STATUS-release-hardening.md`. Plan-of-record:
`dev/plans/prompts/0.7.2-RELEASE-HARDENING-HANDOFF.md`.

### ADRs

- **ADR-0.7.0-vector-binary-quant** — **amended** (PR-2 S3, `78164b9`): § 2
  point 4 now cites the corrected real-embedder ANN-fidelity recall@10 = **0.937
  (CI 0.913–0.957, σ=0.0116)**; the 0.90 floor is kept (a mechanical R−2σ gives
  0.914). Explicitly distinguishes ANN/quantization fidelity (the gate) from IR
  relevance (the embedder ceiling ~0.571, NOT a gate). Front-matter **flipped to
  `status: locked`** 2026-06-01 (HITL-ratified recall reframe).
- **ADR-0.7.0-text-query-latency-gates-revised** — **amended** (PR-3): AC-013 and
  AC-019 sections filled and **HITL-locked (2026-06-01)** as a **tiered**
  budget (10k binding for 0.x/1.x; 100k/1M tracked post-1.0). Front-matter
  flipped to `status: locked` for AC-013/AC-019; AC-012/AC-020 remain as drafted
  (their own slices).
- **ADR-0.8.0-agent-memory-retrieval-and-identity** (new; status `draft,
  HITL-required`) and **ADR-0.8.0-embedder-identity-change-workflow** (new;
  status `draft`) — seeded under 0.7.2's tail (`25ab3ee`) but belong to the
  0.8.0 line; listed here only because they were committed during the 0.7.2
  window.
- No ADRs removed. **Auto-mean-drift policy** was built then **carved out** to
  0.8.x (`dev/plans/prompts/0.8.x-auto-mean-drift-DEFERRED.md`).

### Acceptance criteria

- **AC-013b recall@10 ≥ 0.90 — MET / floor HOLDS** (PR-2 family). Corrected
  ANN-fidelity **0.937 (CI 0.913–0.957)** on real bge, N=7,667, K=192,
  mean-centering. The earlier 0.828 was a **measurement artifact** (exclude-after
  + body-string GT over a corpus with ~5.6 % duplicate bodies; +7.8 pp recovered
  by exclude-before + dedup-by-id GT). Sentinel `ac_013b_floor_matches_adr`
  (`a154037`) pins the test constant to the ADR. Root-cause:
  `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`.
- **AC-013 latency — MET at the 10k tier** (binding for 0.x/1.x). Real bge p50
  **36** / p99 **49** ms at N≈7,667; synthetic 384-d 15/17 ms. 100k = 147/198 ms
  (synthetic 384-d, p50 MISS by 1.8×); 1M ≈ 1.5 s (O(N) extrapolation; 0.7.0
  W4.1 f32-brute anchor 2,048 ms) — **tracked, not gated**. Data:
  `dev/plans/runs/0.7.2-PR-3-perf-data.md`.
- **AC-019 stress — MET on the real-corpus path at the 10k tier** (clean run
  343 ms < 405 ms bound). The earlier 1,201 ms real-path number was concurrent-
  CPU contention, not a regression (resolves the carried "AC-019 idle-box
  re-run" item). The **synthetic `perf_gates` AC-019 is REPORT-ONLY** — the
  synthetic isotropic data cannot meet the baseline-relative bound (instant
  embed → too-fast baseline → too-tight 10× bound).
- **Separate IR ceiling (NOT a gate):** EU-8 measured the embedder's
  IR-relevance ceiling at recall@10 ≈ **0.571** (CI 0.530–0.614) on 301 labeled
  queries (`dev/plans/runs/0.7.2-EU-8-ir-recall-results.md`). ANN fidelity (0.937)
  exceeds it by ~37 pp → quantization is not the bottleneck; the lever for
  end-to-end quality is a better embedder or the graph, not K/ANN tuning.
- **AC-040a doctor verb set** — the manual mean-recompute doctor verb ships
  (`dev/acceptance-rowsets/AC-040a-doctor-verb-set.md`); the automatic in-ingest
  drift detector is deferred.

### Architecture & design changes

- **PR-1 doc-drift sweep** (`aebf959`, closure `10a0e24`): 10 HITL-approved
  corrections aligning design/architecture/ADR docs to shipped 0.7.x reality.
  Drift list: `dev/plans/runs/0.7.2-RHC-PR-1-drift-list.md`.
- **Tiered latency policy** + local-measurement posture: `dev/notes/
  ac013-ac019-canonical-scale-policy.md` (real-corpus is the verdict; synthetic
  dev-box is scouting).
- Vector search remains a per-query **O(N) linear scan** (no ANN index); the ANN
  index is the named post-1.0 (pre-2.1) follow-up, tracked in
  `dev/design/ann-index-vec0.md`.

### Key code changes (commits)

- **PR-9 embedder robustness** (`21f4df6`): Invariant-5 per-embed watchdog
  (`embed_with_watchdog`, 30 s deadline → `RuntimeEmbedderError::Timeout`,
  panic-transparent); engine-side embed serialization (`embed_serialize` guard,
  justified on **safety** not throughput — throughput-neutral on the candle
  default; the earlier "~13×" claim withdrawn); circuit breaker on
  `live_embed_threads` (latches at threshold 8; keyed on *concurrent* live
  threads, not a consecutive-timeout streak, so it bounds intermittent hangs).
  codex 5 passes, BLOCK→PASS (pass-4 BLOCK on the original consecutive-timeout
  design redesigned, not overridden). Release N=2000 real-corpus serialized seed
  clean at ~1.67 docs/s.
- **PR-2 family** (`64f72e0` build / `2ef8c3d` RED guard) — auto in-ingest
  mean-recompute detector **carved out** to 0.8.x behind a RED guard; manual
  doctor verb kept. Floor reframe `78164b9` (ADR) + `a154037` (sentinel + comment).
- **PR-3** (`d9f9b65` test/reframe, `68e1bf0` codex BLOCK fix — FTS-isolate the
  read-path smoke, `e00991f` closure sync): new `ac_013_vector_read_path_smoke`
  (always-on canary); tiered budget (`AC013_DEFAULT_N` 50000→10000;
  `AC013_GATE_N`=10000; assert only at `n ≤ gate`, report above);
  `ac_019_…` made REPORT-ONLY (`AC019_REPORT_ONLY`).
- **`ac_007b` timing-flake fix** (`7ae8979`) — runtime CTE calibration for a
  hardware-pinned timing flake (PRE-EXISTING, unrelated to PR-9).

### Key test changes (what they lock)

- `perf_gates::ac_013_vector_read_path_smoke` — fixture-independent per-push
  canary: an exact-match sentinel must rank 1 through the two-phase bit-KNN + f32
  rerank path. Replaces the infeasible canonical N=1M perf run in CI.
- `ac_013b_floor_matches_adr` — sentinel pinning the recall-floor constant to the
  ADR's corrected 0.937/0.90 language.
- AC-013 gate split: asserts the 80/300 ms budget only at `n ≤ AC013_GATE_N`
  (10,000); reports (`AC013_TIER_INFO`) above it.
- `ac_019_mixed_retrieval_stress_workload_tail` → report-only; the asserting
  AC-019 signal now lives in the real-corpus harness `eu7_real_corpus_ac.rs`.
- PR-9 RED-first tests: `pr9_embed_watchdog` (watchdog + persistent/intermittent
  breaker), `pr9_embed_serialization` (peak in-flight == 1), `pr9_concurrent_embed`
  (real-corpus seed guard, `AGENT_LONG`), `pr9_embed_microbench` (diagnostic).
- 0.7.2 EU-8 IR-recall harness (`519079c`) — orthogonal to ANN recall; produces
  the 0.571 IR ceiling.

---

## Cross-cutting open gaps (stated plainly)

- **Nothing in 0.7.x is shipped.** `v0.7.0` is a held local tag; `v0.7.1` is
  untagged; 0.7.2 is in progress. 0.7.2 PR-4 is the push gate and has not run.
- **Latency above ~50k is not gated and not met.** O(N) vec0 scan; 100k ~147 ms
  p50, 1M ~1.5 s. ANN index is the named post-1.0 (pre-2.1) follow-up, tracked in
  `dev/design/ann-index-vec0.md` (no dated milestone yet — target window only).
- **1M real-corpus recall/latency never freshly measured** (~166 h seed). 0.937 @
  N=7,667 is treated as an upper-ish bound; the 1M latency tier is an O(N)
  extrapolation.
- **End-to-end retrieval quality is embedder/graph-bound, not quantization-
  bound.** IR ceiling ~0.571 « ANN fidelity 0.937; raising K buys no user-visible
  gain. The lever is a better embedder or the graph (0.8.x territory).
- **Auto mean-drift deferred to 0.8.x** behind a RED guard; a workspace whose
  first 256 docs are unrepresentative can under-center until manual reindex.
