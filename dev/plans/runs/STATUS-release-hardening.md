# STATUS — 0.7.2 RELEASE-HARDENING

_Last updated: 2026-06-01 — **CAMPAIGN CLOSING — PR-8 (campaign closure) IN PROGRESS.**
All prior slices CLOSED on `main`: Phase A (PR-1, PR-2 family, PR-9, PR-3, PR-4) +
Phase B (PR-5, PR-6, PR-7). **fathomdb 0.7.1 is PUBLISHED** (crates.io + PyPI + npm +
GitHub release `v0.7.1`, PR-4 `c893d8b`); `v0.7.0` is a local historical marker only
(never published, per HITL). PR-8 has finalized + dated the CHANGELOG 0.7.2 section
(Phase B documented; PR-7 batch-collapse honesty preserved), bumped the workspace
`0.7.1 → 0.7.2` (Axis W; embedder-api held 0.6.0), and refreshed `Cargo.lock`.
**REMAINING: codex review → HITL push gate → push `main` + `v0.7.2` → publish
pipeline.** Local `main` is 11 commits ahead of `origin/main` (Phase B) before the
PR-8 commits; nothing new pushed yet._

Orchestrator: main-thread Claude Code session. Pattern per `dev/design/orchestration.md`
(per-slice prompt → informed subagent implementer/orchestrator (TDD) → codex review →
cherry-pick/ff to `main` on PASS). **No push without explicit HITL OK.**

## Handoff / sources of truth

- **Plan-of-record (spec):** `dev/plans/prompts/0.7.2-RELEASE-HARDENING-HANDOFF.md`
  (ordered PR-N sections + Definition of Done + HITL-gates table).
- **This file:** live execution ledger / ordered run-sheet. The scoreboard below is
  the to-do list; each slice prompt reports its outcome back here.
- PR-2 family resolution: `dev/plans/runs/0.7.2-PR-2bc-decision.md` (RATIFIED).
- Root-cause evidence: `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`,
  `dev/plans/runs/0.7.2-EU-8-ir-recall-results.md`.
- Inherited STATUS: `dev/plans/runs/STATUS-embedder-undefer.md` (0.7.1),
  `dev/plans/runs/STATUS-perf-vector-quant.md` (0.7.0).
- Deferred to 0.8.x: `dev/plans/prompts/0.8.x-auto-mean-drift-DEFERRED.md`.

## Baseline

- Branch: `main` (slices work directly on `main` per the PR-2bc precedent, or in
  per-slice worktrees per orchestration.md § 2 — orchestrator's choice per slice).
- Pre-0.7.2 anchor: `v0.7.0` (held, unpushed). PR-1+ branch from `main` HEAD.
- Current `main` HEAD: `63bb7f3`. Ahead of `origin/main` by 46 commits.

## Slice scoreboard (ordered run-sheet — this is the to-do list)

Sequence is top-to-bottom within phase. "Gate" = HITL gate that must clear before the
slice's irreversible/costly step. "Prompt" = per-slice execution prompt (authored
as-needed from the handoff section; — = not yet authored).

### Phase A — release closure (must finish before Phase B)

| Order | ID | Subject | Status | Dep | Gate | Prompt | Notes |
|---|---|---|---|---|---|---|---|
| — | PR-0 | Inherited-state reconciliation | **FOLDED** | — | — | — | Never run as a discrete slice; its facts are captured here + in the decision memo. Tags/version confirmed (`v0.7.0` only; ws 0.7.0). Treat as satisfied. |
| — | PR-2a | Mean-centering recall investigation | **CLOSED (GO, later reframed)** | — | done | `…/prompts/0.7.2-PR-2a-recall-investigation.md` | GO verdict; later shown to address a measurement artifact. |
| — | PR-2(bc) | Recall floor + mean recompute family | **CLOSED / RESOLVED** | PR-2a | ratified | `…/prompts/0.7.2-PR-2bc-{reassessment,S1,S2,S3}.md` | S1 land-harness + S2 carve-auto-drift + S3 floor-reframe landed (`5b69568`/`2ef8c3d..d2c0bf4`/`78164b9`); PR-2c SHELVED. Floor HOLDS 0.90. See decision memo. |
| **1** | PR-1 | Architecture/design doc drift sweep | **CLOSED** | PR-0 | drift-list approved 2026-05-31 | `…/prompts/0.7.2-PR-1-doc-drift-sweep.md` | Audit→drift list (10 items, `4beca5b`)→HITL approved all→corrections `aebf959` + closure `10a0e24` on `main`. Codex **PASS** (`…/runs/0.7.2-PR-1-review-20260531T165936Z.md`). Docs-only; nothing pushed. |
| **2** | PR-9 | Embedder robustness (concurrent-embed safety + Invariant-5 watchdog) | **✅ CLOSED (`21f4df6`, local main, unpushed)** | PR-1 | diff+tests ✅ (HITL 2026-05-31) | `…/prompts/0.7.2-PR-9-embedder-robustness.md`; closure `…/runs/0.7.2-PR-9-output.json`; review `…/runs/0.7.2-PR-9-review-20260531T205810Z.md` | Watchdog (Invariant 5) + engine-side embed **serialization** (re-justified on SAFETY — throughput-neutral, candle global rayon pool; false "~13×" withdrawn) + **circuit breaker** keyed on concurrent **live embed threads** (bounds abandoned-thread leak for persistent AND intermittent hangs). RED→GREEN each. **codex 5 passes → PASS** (pass-4 BLOCK on the original consecutive-timeout breaker design was NOT overridden — redesigned to live-thread-count + intermittent regression test; pass-5 PASS). Tests: serialization 1/1, watchdog 5/5, eu5f 6/6, projection 12/12; release e2e seed N=2000 complete+correct. Uncommitted; **no push**. (`ac_007b` flake is PRE-EXISTING — fails at baseline `ff7b008`, unrelated to PR-9.) |
| **3** | PR-3 | Real-corpus latency/recall measurement + tiered ADR | **✅ CLOSED (`e00991f`, local main, unpushed)** | PR-2(bc), PR-9 | dispatch reframed to local (HITL 2026-06-01); budget HITL-locked tiered | `…/prompts/0.7.2-PR-3-canonical-ci-dispatch.md`; review `…/runs/0.7.2-PR-3-review-20260601T122419Z.md` | Commits `d9f9b65`→`68e1bf0`→`e00991f`. Codex pass-1 **BLOCK** (smoke could pass via FTS path) → FIXED (FTS-isolated smoke), NOT overridden; pass-2 re-review skipped per HITL → **PASS-by-inspection**. **Reframed (HITL):** real-embedder N=1M is infeasible on CI (~166 h seed) → heavy measurement is **local-only**; CI gets a fast read-path smoke (`perf_gates::ac_013_vector_read_path_smoke`). Budget is now **tiered (10k binding for 0.x/1.x; 100k/1M tracked post-1.0 ANN work)**. **10k tier MET** (AC-013 real bge p50 36/p99 49 @ N≈7667; recall@10 0.937 ≥ 0.90; AC-019 real 343 < 405 bound). Data: `…/runs/0.7.2-PR-3-perf-data.md`; ADR amended (AC-013/AC-019 tiered, HITL-locked); closure `…/runs/0.7.2-PR-3-output.json`. AC-019 idle-box item resolved (real 1201 ms = contention; clean real run 343 < 405). **Synthetic AC-019 is report-only** (HITL: synthetic data can't meet the baseline-relative bound; real-corpus path is the verdict). AC-013 keeps its hard 10k gate. Nothing pushed. |
| **4** | PR-4 | Release notes + **push v0.7.0 + v0.7.1** | **✅ CLOSED — fathomdb 0.7.1 PUBLISHED (crates.io + PyPI + npm + GitHub release), `c893d8b`** | PR-1, PR-2(bc), PR-3 | push approved (HITL 2026-06-01T17:20Z); recovery re-fire approved | `…/prompts/0.7.2-PR-4-release-notes-and-push.md`; review `…/runs/0.7.2-PR-4-review-20260601T154644Z.md`; closure `…/runs/0.7.2-PR-4-output.json` | Commits `4773845` (docs) + `e24b7a1` (version bump). CHANGELOG v0.7.0 + v0.7.1 finalized + dated 2026-06-01; `docs/embedder.md` recall caveat reframed to corrected 0.937; workspace bumped 0.7.0→0.7.1 via `set-version.sh` (Axis W lockstep — `[workspace.package]` + 5 sibling pins + python + ts; Axis E embedder-api stays 0.6.0); `Cargo.lock` refreshed. `v0.7.1` tag on `e24b7a1`; `v0.7.0` unchanged `38d5f4f`. Removal linter PASS; `--check-files` ok. **codex pass-1 BLOCK** (Axis-W sibling pins left at 0.7.0) → FIXED via set-version.sh, NOT overridden; **pass-2 CONCERN** (cut-before-push publish wording) → ADDRESSED (publish claims tied to the push). **HITL push gate (2026-06-01):** decided push main-only + dry-run rehearse v0.7.1 publish; **do NOT publish v0.7.0** (tag points at old tree `38d5f4f` that fails release-gate preflight — stays a local historical marker; 0.7.1 is the first published 0.7.x). **`main` pushed** (`c8c7d43..484042b`); no tags pushed → no publish fired. Pre-push hook surfaced a pre-existing clippy `doc_lazy_continuation` regression in PR-9 test docs (`pr9_concurrent_embed.rs`/`pr9_embed_watchdog.rs`) → FIXED (`484042b`, doc-only) + pushed. **Dry-run release dispatch (run 26768536981) FAILED at `verify-release`** → all publish jobs skipped (nothing published). Root cause: `actionlint` rejects `perf-canonical.yml` (13 `workflow_dispatch` inputs > GitHub max 10) — pre-existing, unrelated to PR-4, blocks release preflight. **FIXED (HITL 2026-06-01): consolidated the 6 SQLite env-knob inputs into one `sqlite_perf_env` string (13→8); actionlint PASS local.** Folded into PR-4 as release-unblockers. Dry-run release (run 26769746003) **GREEN** — verify-release + 5-platform builds + cargo-publish dry-runs t1–t7 + npm dry-run all pass; publish-pypi/post-publish-smoke/github-release correctly skipped under dry_run=true. **`v0.7.1` re-cut onto release-consistent main tip `365bc54`** (was `e24b7a1`, which predated the CI fixes and would have re-failed the tag-tree preflight). Real v0.7.1 tag pushed (HITL OK 2026-06-01T17:20Z, `074a3f4`). **Release run 26770446544 PARTIAL-FAILED at publish tier order:** crates.io now has `fathomdb-schema 0.7.1` + `fathomdb-query 0.7.1` (published, irreversible) + `fathomdb-embedder-api 0.6.0` (pre-existing, skipped); `fathomdb-engine` FAILED (`needs fathomdb-embedder ^0.7.1` not yet on crates.io) → embedder/facade/cli, PyPI, npm, github-release all skipped. **Root cause:** `release.yml` tier order had embedder (T5) AFTER engine (T4), but engine depends on embedder (edge added in 0.7.1 embedder-undefer). **FIXED:** reordered T4=embedder/T5=engine + rewired facade/pypi/npm `needs:` → engine (T5); `dev/design/release.md` tier table corrected. Recovery = force-move `v0.7.1` to the fix commit + re-push to re-fire (idempotency guard skips schema/query already published). Recovery (HITL-approved): force-moved `v0.7.1` → `c893d8b` + re-push → **re-fire run 26771399059 fully GREEN** (cargo t1–t7 correct order, PyPI, npm, 3× post-publish smoke, co-tagging, github-release). **fathomdb 0.7.1 published** on crates.io (schema/query/embedder/engine/facade/cli; embedder-api held 0.6.0), PyPI (latest), npm (latest), and GitHub release `v0.7.1` (not draft). `v0.7.0` NOT published (local marker only). `origin/main == c893d8b`. CLOSED 2026-06-01T18:01Z. |

### Phase B — testing/perf hardening (after PR-4)

| Order | ID | Subject | Status | Dep | Gate | Prompt | Notes |
|---|---|---|---|---|---|---|---|
| **5** | PR-5 | Corpus-driven test harness (`tests/support/corpus_harness.rs`) | **✅ CLOSED (`c605a18`, local main, unpushed)** | PR-4 | diff+tests ✅; cache-dir confirmed (HITL 2026-06-01) | `…/prompts/0.7.2-PR-5-corpus-harness.md`; closure `…/runs/0.7.2-PR-5-output.json`; review `…/runs/0.7.2-PR-5-review-20260601T203321Z.md` | `CorpusFixture` (small/medium/full + per_source + from_docs), synthetic/real embedder toggle, one-line `ingest_into` (reuses `corpus_subset::ingest` — 4a95cfd batched pattern), `query_set` (EU-0 §1.2), 3 assert helpers. **Per-(model,subset) embedding cache** under `data/corpus-data/.cache/embeddings/` (gitignored; `$FATHOMDB_CORPUS_CACHE_DIR`-overridable): key=sha256(identity+label+doc-manifest), byte-deterministic blob, atomic write, hit-path re-verifies identity+manifest, **every miss (cold/partial/stale) is loud** (`CORPUS_CACHE_MISS` + `IngestReport.cache_miss_reason`). Pack-4 `corpus_vector`/`corpus_fts`/`corpus_graph` migrated **no behavior change**; eu7 migration deferred (PR-6/7). Added `VaryingEmbedder::with_identity` test seam (prod `EmbedderIdentity` untouched). **codex BLOCK→BLOCK→CONCERN→PASS** (no BLOCK overridden — loud-miss contract + both stale-cache branches locked by tests). Matrix GREEN: default & `--features default-embedder`, corpus-present & -absent; clippy clean. AGENT_LONG-gated real-embedder smoke added. **No push.** |
| **6** | PR-6 | Dev-loop perf gates (`perf_gates_devloop.rs`) | **✅ CLOSED (`e2c886d`, local main, unpushed)** | PR-5 | budget shape HITL-locked 2026-06-01 (perf NOTIFIES, structural BLOCKS, +1 catastrophic hard assert) | `…/prompts/0.7.2-PR-6-devloop-perf-gates.md`; closure `…/runs/0.7.2-PR-6-output.json`; review `…/runs/0.7.2-PR-6-review-20260601T221430Z.md` | 3 always-on devloop ACs (NOT AGENT_LONG-gated) at N≈1000 via PR-5 `CorpusFixture::medium`, same production read path. **Structural invariants HARD** (vec0 row count + FTS) catch batch-collapse `4a95cfd`; **soft latency** p50≤50/p99≤150ms (synthetic) + **recall floor** 0.85 (real) NOTIFY-only; **one hard catastrophic ceiling** (10× soft = p50>500/p99>1500ms, synthetic) catches scanner-throughput `53a270d`. **Signal split:** synthetic isolates LATENCY (instant embed) + recall report-only (~0.35 @ N=1000, sparse-vector artifact); real carries RECALL + latency report-only. **RED-shows verified** (throwaway, reverted): batch-collapse→structural panic (1 row vs 1000), scanner→catastrophic panic (p50=616>500ms). Synthetic warm wall ≈16s ≤30s ✓; clippy clean (default + `default-embedder`). Stable `DEVLOOP_NUMBERS` line for PR-7. **codex pass-1 BLOCK→pass-2 PASS** (no BLOCK overridden: AC-019 schema normalized, real cold-cache reconciled as allowed + latency report-only, latency comment corrected). Doc `dev/design/perf-gates.md`. **No push.** |
| **6** | PR-7 | Perf regression detection (`dev/perf-history/` + check bin) | **✅ CLOSED (ff to local `main`, unpushed)** | PR-5 | thresholds **15% lat / 0.03 recall** (HITL 2026-06-01); diff+tests ✅ (HITL 2026-06-01) | `…/prompts/0.7.2-PR-7-perf-regression-detection.md`; closure `…/runs/0.7.2-PR-7-output.json`; review `…/runs/0.7.2-PR-7-review-20260601T230046Z.md` | `perf-regression-check` `[[bin]]` under `fathomdb-cli` (layout A). Groups by `(ac_id,n)`; single-latest vs rolling median of prior ≤10; flags p50/p99 >15% or recall >0.03 abs. **Thresholds conservative** (handoff default 10%/0.02; HITL chose 15%/0.03 ≈ 2.4σ recall, ~1% jitter FP). Strict fixed-width RFC3339→epoch (dep-free; unparseable ⇒ exit 2; parsed epoch is sort key). Exit 0/1/2; `--json`; append-only (read-only bin). CI → `$GITHUB_STEP_SUMMARY` (workflow_dispatch-safe; dropped dead PR-comment + `pull-requests:write`). Backfill v0.6.x→0.7.2 (9 rows); most groups single-point ⇒ honest `insufficient-history`. **Batch-collapse honesty (HITL option A):** the 2026-05-27 bug masqueraded as an *improvement* (degenerate recall 1.0), so a degradation detector flags the regression-shaped **correction** (1.0→0.1572 @`4a95cfd`), not the bug itself (true bug-catch = row-count/anomaly rule, logged future work). Committed store CLEAN (exit 0). Tests: integration 12/12, bin-unit (rfc3339) 5/5; clippy/fmt/actionlint clean. **codex BLOCK→BLOCK→BLOCK→PASS** (chronology+CI+honesty → strict-value → fixed-width; no BLOCK overridden). No push. |
| **7** | PR-8 | Campaign closure | **🔶 IN PROGRESS** (CHANGELOG 0.7.2 finalized + dated; workspace bumped 0.7.1→0.7.2; v0.7.2 tag + push pending HITL) | PR-7, PR-9 | v0.7.2 push (HITL — PENDING) | `…/prompts/0.7.2-PR-8-campaign-closure.md` | Final scoreboard here; CHANGELOG 0.7.2 section sealed; Phase B (PR-5/6/7) documented; PR-7 batch-collapse honesty preserved; box-3 reframe noted (not retconned). Awaiting codex + HITL push OK. |

## Open items (carried; not gating their own slice)

- ~~**AC-019 idle-box re-run**~~ — **RESOLVED (PR-3, 2026-06-01).** The EU-7/PR-2bc
  dev-box stress MISS (p99 1201 ms vs 499 ms bound) is confirmed a **contention
  artifact, not a regression**: the clean idle run passes (343 ms < 405 ms bound at
  N≈7667; AC-013 also PASSes p50 36 / p99 49). See `…/runs/0.7.2-PR-3-perf-data.md`
  (AC-019 table) and the tiered AC-019 budget in `ADR-0.7.0-text-query-latency-gates-revised.md`.
- ~~**EU-7 harness follow-up**~~ — **RESOLVED (already landed `f5bd686`, PR-2bc S1).**
  The `EU7_GT_EMBED_PROGRESS` periodic log line is wired at
  `eu7_real_corpus_ac.rs:725`; this entry was stale.
- **Doc-archive hygiene** (out of campaign scope) — ~100 completed-release prompts in
  `dev/plans/prompts/` + run artifacts have no archive convention; `dev/plans/README.md`
  is itself stale (claims the dir is "0.6.0 only"). Decide a convention before moving
  tracked files (cross-refs by path exist).

## Honesty report

- The recall "gap" (0.828) was a **measurement artifact**, not a defect. The corrected
  ANN-fidelity number is **0.937** and the 0.90 floor was always defensible; the ADR was
  corrected to cite the right measurement, NOT re-worded to retcon a pass.
- The automatic mean-drift detector was built and ratified, then **carved out** because
  its sole justification (recall) collapsed and its benefit is unmeasured. It is parked
  for 0.8.x behind a RED guard, not silently dropped.
- Nothing is pushed. `v0.7.1` is intentionally untagged until PR-4.

## Pointer forward

PR-3 CLOSED (`e00991f`, local `main`, unpushed; codex pass-1 BLOCK fixed →
PASS-by-inspection, pass-2 re-review skipped per HITL). Latency budget is now
**tiered** (10k binding for 0.x/1.x; 100k/1M tracked post-1.0 ANN-index work);
real-embedder canonical N=1M is infeasible so measurement is local-only + a CI
read-path smoke; recall anchor 0.937 (floor 0.90 holds); AC-019 synthetic is
report-only (real-corpus is the verdict). Data: `…/runs/0.7.2-PR-3-perf-data.md`.

**PR-4 CLOSED (`c893d8b`) — fathomdb 0.7.1 PUBLISHED** to crates.io + PyPI + npm
+ GitHub release `v0.7.1` (re-fire run 26771399059 green after a first-attempt
partial failure on a publish-tier-order bug — engine published before its
`fathomdb-embedder` dep; fixed T4=embedder/T5=engine in `release.yml` + corrected
`dev/design/release.md`). `v0.7.0` NOT published (local historical marker only,
per HITL). Three pre-existing CI regressions were fixed as release-unblockers
(clippy doc-lists, perf-canonical 13→8 inputs, publish tier order). Closure:
`…/runs/0.7.2-PR-4-output.json`. **`origin/main` now public at `c893d8b`** — the
entire held 0.7.x line is pushed.

Phase A (release closure) COMPLETE. Next actionable slice: **PR-5 (corpus-driven
test harness, `tests/support/corpus_harness.rs`)** — begins Phase B
(testing/perf hardening): PR-5 → PR-6 (dev-loop perf gates) / PR-7 (perf-
regression detection, parallelizable) → PR-8 (campaign closure + 0.7.2 release).
Update this scoreboard on landing.

**PR-6 CLOSED (`e2c886d`, local `main`, unpushed; codex pass-1 BLOCK → pass-2
PASS, no BLOCK overridden).** Dev-loop perf gates (`tests/perf_gates_devloop.rs`)
+ two-tier doc (`dev/design/perf-gates.md`). HITL-locked disposition: **perf
NOTIFIES, structural BLOCKS**, with one hard catastrophic latency ceiling (10×
soft) so an orders-of-magnitude scanner regression still RED-fails. Both named
regressions RED-shows-verified (batch-collapse via structural assert;
scanner-throughput via catastrophic ceiling). Synthetic isolates latency + real
carries recall (each path's off-signal report-only). Stable `DEVLOOP_NUMBERS`
line is the **PR-7 ingestion contract**. Synthetic warm wall ≈16s ≤30s; clippy
clean. Closure `…/runs/0.7.2-PR-6-output.json`. **Next: PR-7 (perf-regression
detection) — now unblocked; then PR-8 (campaign closure + 0.7.2 push).** No push.

**PR-7 CLOSED (ff to local `main`, unpushed; codex BLOCK→BLOCK→BLOCK→PASS, no
BLOCK overridden).** Perf-regression detection: `perf-regression-check` `[[bin]]`
under `fathomdb-cli` (layout A) reading append-only `dev/perf-history/`, grouping
by `(ac_id,n)`, flagging latest-vs-rolling-median (>15% latency / >0.03 abs
recall — HITL-locked, conservative vs the 10%/0.02 default). Strict fixed-width
RFC3339→epoch (dep-free; bad timestamp ⇒ exit 2). CI → `$GITHUB_STEP_SUMMARY`
(workflow_dispatch-safe; no PR-comment/permission over-grant). **Honesty (HITL
option A):** the 2026-05-27 batch-collapse masqueraded as an *improvement*
(degenerate recall 1.0), so the detector flags the regression-shaped
**correction** (1.0→0.1572 @`4a95cfd`), not the bug itself — the true bug-catch
(row-count/anomaly invariant) is logged as future work. Most history groups are
single-point ⇒ honest `insufficient-history`; committed store CLEAN. Landed
second of the PR-6/PR-7 sibling pair — rebased onto PR-6 (`ee7ba9f`), STATUS the
only collision, both rows kept. Closure `…/runs/0.7.2-PR-7-output.json`. **Next:
PR-8 (campaign closure + 0.7.2 push) — the only remaining Phase-B slice.** No push.
