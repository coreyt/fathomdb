# IR-B (IR-1 Phase 2) — what is DEFERRED to the COR-2 corpus freeze · `[IR-eval]`

> **Status:** IR-B (Phase 2 / CODE) built the **corpus-INDEPENDENT** measure +
> schema + plumbing on branch `ir-B-phase2-*` (merged post-GA, `main`@`6d66834`).
> This note lists exactly what still waits for the **owner-paced COR-2 corpus
> freeze** before it can be done.
> Spec: `dev/design/ir-recall-measure.md` (Claude↔codex consensus-signed).
>
> **Update 2026-06-09 (HITL + tooling):** freeze target ruled = **current 8
> datasets + QAConv + QASPER ≈ 10.1K** (10K-must waived; PMC/S2ORC/ELITR
> deferred). The freeze is now a one-command flow — `tests/corpus/scripts/
> freeze_corpus.py` (verify · `--reconcile` · `--freeze` · `--reproduce`) — driven
> by the runbook `dev/plans/prompts/scaffolds/5-COR-2-corpus-freeze.md`. Known
> reproducibility blocker: the **stale qmsum manifest pin** (reconcile from real
> bytes). The data ops are owner-run where the corpus persists; the items below
> still apply, now with tooling to execute them.

## Built now (corpus-independent — landed on this branch)

- **Gold-set schema + loader + validator** — `tests/support/ir_eval.rs`
  (`GoldSet`/`GoldQuery`/`EvidenceUnit`/`Necessity`/`QueryClass`/`Locator`,
  `load_gold_set`, `parse_gold_set`, `validate_gold_set`). Additive superset of
  eu8 `ground_truth_queries` (§(b)); zero new crate deps (manual `serde_json`).
- **Evidence Recall@K measure** — strict all-of headline + graded diagnostic on
  the SINGLE `required`-only denominator (§(a)); the seed single-unit-of-relevance
  denominator with the eu8 doc-id fallback (§(f)); the K-ladder @5/@10/@20/@50,
  headline @10 (§(c)); per-class aggregation + negative-class abstention (§(d)).
- **Retrieval-mode plumbing** — `RetrievalMode` + `run_mode_bodies` reusing the
  existing engine seams: `RrfHybrid` (`Engine::search`), `VectorOnly`
  (`set_vector_stage_only_for_test`), `RerankStub` (`rerank_fused` identity).
- **Experiment-runner scaffold** — `run_experiment` (mode×K×class loop, raises
  vector fanout via `set_search_limit_for_test` to ≥ deepest K) + `experiment_to_json`.
- **Unit tests + wiring smoke** — `tests/ir_recall_eval.rs` (10 tests, GREEN on
  the DEFAULT feature set: synthetic-fixture math + the runner wired end-to-end
  against the real `Engine::search` path with the synthetic embedder).
- **Illustrative fixture** — `tests/fixtures/ir_gold/synthetic_gold.json`
  (synthetic, NOT real labels; `corpus_hash` = `TODO(COR-2-freeze)` on purpose).

## DEFERRED — blocked on the COR-2 corpus freeze (do NOT attempt before it)

1. **Real fact-level gold-set LABELING** (Phase 3 / IR-C). The synthetic fixture
   pins only the *schema*; the real `required_evidence` labels (which facts are
   required for which query, prioritizing long-doc fact-burial: enron/qmsum/cnn)
   are produced by human/HITL labeling on the **frozen** corpus, or they drift
   the way the GA recall number did. `TODO(COR-2-freeze)` in `ir_eval.rs`.
2. **The IR-C experiment RUNS + real-corpus numbers.** `run_experiment` is wired
   but must run in `--release`, isolated, on the pinned snapshot with the real
   BGE embedder + real labels. No real recall number exists yet (all TBD →
   Phase 3/4). Emit `dev/plans/runs/IR-1-ir-recall-experiments-<ts>.{md,json}`.
3. **The FTS-only retrieval modes** — `FtsWriteCursor` (production write-cursor
   order) and `Bm25Fts` (`ORDER BY bm25(search_index) ASC`). These need
   harness-level FTS5 SQL + the frozen corpus; `run_mode_bodies` returns the
   `TODO(COR-2-freeze)` marker for them today (recorded in `deferred_modes`).
4. **Pinning the corpus_hash / qrels_version** to a real frozen snapshot. The
   pinning *principle* is enforced (validator flags the placeholder); the actual
   snapshot is the downstream **B-1 corpus-basis ruling + corpus freeze**, not
   this work.
5. **AC-077 mint** (Phase 4 / IR-D) — explicitly OUT of IR-B; minted LAST,
   grounded by the experiments, thresholds still HITL-final.

## Guardrails honored

- eu7 / AC-075 fidelity gate, the engine vector path, and the ANN/fidelity
  regression: **untouched**. No schema/SDK/engine behavior change (additive new
  test files only; eu8 not modified). No AC minted. No fabricated thresholds, no
  real-corpus numbers. No commitment to a corpus snapshot.
