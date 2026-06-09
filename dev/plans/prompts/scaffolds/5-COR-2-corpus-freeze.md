# COR-2 — corpus freeze / versioned SHA-256 snapshot · track:Corpus · type:work

## Purpose (1–2 sentences)
Assemble the agreed Version-B source set, reconcile its checksums, **freeze a versioned, reproducible,
SHA-256-pinned snapshot**, and record it so the IR-eval fact-level gold set (IR-B) is built against an immutable
corpus. Owner-managed, out-of-band; **does not gate GA** (already shipped) — it gates IR-C (the experiments).

## HITL freeze-target ruling (2026-06-09, coreyt — supersedes the "~10K must" lock)
- The **~10K doc target is WAIVED.** Freeze the **current 8 datasets + QAConv + QASPER ≈ 10.1K docs**.
- **QAConv (BSD-3) + QASPER (CC-BY) MUST be loaded before the freeze** — they are already scripted and
  commit-eligible, QASPER is the **only `paper`-class source**, and QASPER+QAConv ship ~10,296 eval-QA pairs
  that are the richest fuel for the IR gold set. Freezing without them would leave the gold set with an empty
  paper class and lose the best labeling source — see board §7.
- **PMC OA · S2ORC · ELITR stay DEFERRED** (not in this freeze; revisit post-0.8.1 if needed).
- Rationale: the doc count was never the real gate; the real gates are **reproducibility (to freeze)** and
  **gold-set labeling (to measure)**.

## ⚠️ Network requirement (why this is owner/CI work, not sandbox work)
The acquisitions pull from **HuggingFace** (cnn_dailymail, enronqa), **CMU** (enron), and **AWS S3** (qasper).
GitHub-only egress policies **403** on those hosts — verified 2026-06-09: 5/9 sources build in a GitHub-allowlisted
sandbox (qmsum, qaconv, synthetic_notes, landes_todos, bahmutov — all reproduce their pins exactly), but
cnn_dailymail/enronqa/enron/qasper cannot. **Run the freeze where network is open:** the
`.github/workflows/corpus-freeze.yml` workflow (GitHub-hosted runner) is the turnkey path, or a local/owner box
with open egress.

## Prerequisites (verify ALL before freezing — do not freeze if any is unmet)
- [ ] **Open network egress** to huggingface.co + www.cs.cmu.edu + qasper-dataset.s3 (see above), and
  `pip install "datasets>=3.0,<4.0"` (the only non-stdlib dep; used by the two HF scripts).
- [ ] Corpus data present on disk (`data/corpus-data/` is gitignored and EMPTY in a fresh checkout). — verify:
  `ls data/corpus-data/raw/*.jsonl` after acquisition.
- [ ] **QAConv + QASPER produced** — verify: `ls data/corpus-data/raw/{qaconv,qasper}.jsonl` exist. They were
  added to the manifest 2026-06-02 but never loaded into the measured corpus.
- [ ] Corpus **ingests cleanly + the eval harness runs** against the candidate set (green smoke).
- [ ] GA-1 corpus-quality finding read — it classified the recall drop as **(b) code/measurement-path, NOT a
  corpus-quality defect** (`dev/plans/runs/GA-1-corpus-ab-20260608T012503Z-output.json`), so **no near-dup /
  bad-doc refine is owed**. (The qmsum "stale checksum" GA-1 flagged is NOT a bad pin — re-acquiring reproduces
  the manifest pin `19a2e5b4…` exactly; the on-disk `717e9fb5…` was stale leftover data. Re-acquire fixes it.)

## Work to-do (the steps — all via `tests/corpus/scripts/freeze_corpus.py`)
Run these where the corpus data actually lives (owner machine / CI runner), `--release`/isolated for any measure:

0. **Easiest path: dispatch `.github/workflows/corpus-freeze.yml`** (Actions → corpus-freeze → Run). It does
   steps 1–5 on an open-network runner and commits `snapshot.json`. The manual steps below are for a local/owner box.
1. **Acquire every source** (all 9 must be present to freeze):
   run `acquire_cnn_dailymail.py · acquire_enronqa.py · acquire_qmsum.py · acquire_enron.py ·
   acquire_bahmutov_dailylogs.py · acquire_landes_todos.py · acquire_qaconv.py · acquire_qasper.py`, then
   `generate_synthetic_notes.py` and (last, it anchors on real docs) `generate_chain_corpus.py`.
2. **Verify** every raw source against the manifest:
   `python tests/corpus/scripts/freeze_corpus.py`
   — sources built from a clean acquisition reproduce their pins (`MATCH`); `chain_connectives` shows
   `UNMANIFESTED` (expected — synthetic, not a manifest contract).
3. **Reconcile** only if a source genuinely drifted upstream (NEVER hand-edit a hash):
   `python tests/corpus/scripts/freeze_corpus.py --reconcile`  → re-run step 2 until **VERIFY OK**. (A `MISMATCH`
   from stale on-disk data is fixed by re-acquiring, not reconciling — reconcile is for a real upstream change.)
4. **Freeze** the snapshot:
   `python tests/corpus/scripts/freeze_corpus.py --freeze --corpus-version 0.8.x-B`
   → writes `tests/corpus/snapshot.json` (snapshot_id + per-source SHA-256 + total_docs + corpus_hash).
5. **Prove determinism** (COR-2's hard gate — a snapshot that won't reproduce is NOT frozen):
   `python tests/corpus/scripts/freeze_corpus.py --reproduce tests/corpus/snapshot.json`
   → on PASS it stamps `reproduced_bit_identical=true`. On FAIL: re-assemble from manifest pins; if still
   divergent, **HALT + escalate** (do not freeze a non-reproducible corpus).
6. **Pin the gold set:** copy the printed `corpus_hash` into the gold-set fixture(s), replacing the
   `TODO(COR-2-freeze)` placeholder (`tests/fixtures/ir_gold/*.json` `corpus_hash`, and `qrels_version` once labels exist).
7. **Commit** `tests/corpus/snapshot.json` + the reconciled `manifest.json` (both tracked; the data stays
   gitignored). Report the snapshot record to the board.

## Output to the orchestrator (how this session reports back)
- Artifact(s): `tests/corpus/snapshot.json` (committed) + the reconciled `manifest.json`; a freeze record on the
  board (`STATUS-0.8.0.md` §7).
- Schema: snapshot record = {snapshot_id, corpus_version, corpus_hash, total_docs, source_count,
  per_source_sha256[], reproduced_bit_identical(bool)} — emitted by `freeze_corpus.py`. No fabricated hashes;
  every hash is computed from the real artifact.
- Hand-off line: the ⬛ COR-2 freeze is **consumed by IR-B/IR-C** (the fact-level gold set + experiments must
  run on a frozen corpus). It does **not** gate GA (already shipped).
- Discipline: read the REAL exit/numbers ([[background-exit-masks-real-exit]]); no fabricated numbers; no push/tag
  of release artifacts; board is orchestrator-owned. Owner-paced ([[fathomdb-consumer-agents]]).

## After the freeze → what unblocks (the real IR critical path)
The freeze unblocks, but is NOT, the measurement. Next, in order:
1. **Gold-set labeling (IR-C / Phase 3)** — fact-level `required_evidence` labels on the FROZEN corpus
   (human/HITL; QASPER+QAConv eval-QA are the source material). Must be post-freeze — labeling on an unfrozen
   corpus is the label-drift that moved the GA recall number.
2. **IR-C experiment runs** — `run_experiment` `--release`/isolated on the pinned snapshot with the real BGE
   embedder + real labels → `dev/plans/runs/IR-1-ir-recall-experiments-<ts>.{md,json}`. (RRF-hybrid / vector-only
   / rerank-stub modes run today; the FTS-only modes still need harness FTS5 SQL.)
3. **IR-D** mint AC-077 (grounded by the experiments) → **IR-2 / IR-gate** (HITL thresholds).

## Full prompt / next
- This runbook IS the starter; the freeze itself is mechanical via `freeze_corpus.py`. The gold-set labeling
  (step 1 above) is the substantial human/HITL pole and gets its own IR-C prompt.
