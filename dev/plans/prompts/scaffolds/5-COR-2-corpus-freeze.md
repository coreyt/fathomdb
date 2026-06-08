# COR-2 — corpus freeze / versioned SHA-256 snapshot · track:Corpus · type:work

## Purpose (1–2 sentences)
Finish Version B, refine it per the GA-1 / quality findings, test it, and **freeze a versioned, reproducible,
SHA-256-pinned snapshot** so the IR-eval fact-level gold set (IR-B) is built against an immutable corpus. Owner-managed,
out-of-band; **does not gate GA** (GA pins a snapshot at B-1) — its deadline is **before IR-B**.

## Prerequisites (verify ALL before starting — do not start if any is unmet)
- [ ] Version B expansion is **finished** (~7.6K → ~10K target) — verify: `ls data/corpus-data/raw/` doc count at the
  target; the `corpus-work` worktree/branch (`.claude/worktrees/corpus-work`, branch `corpus-work`) is at its final state.
- [ ] Version B has been **refined** per the GA-1 corpus-quality findings (near-dup / bad-doc cleanup if GA-1
  classified defects). — verify: GA-1 `classification` was read and any (c) corpus-quality defects addressed
  (`ls dev/plans/runs/GA-1-corpus-ab-*-output.json`).
- [ ] Version B is **tested** (ingests cleanly; eu7/eval harness runs against it). — verify: a green ingest +
  smoke run on the candidate snapshot.
- [ ] The corpus-card format is on hand. — verify: `ls tests/corpus/corpus-card.md` (per-source SHA-256 convention).

## Work to-do (the steps)
1. Assemble the final Version B corpus set (post-refine, post-test).
2. Compute a **per-source SHA-256** manifest per `tests/corpus/corpus-card.md`; record each source's hash + size + count.
3. **Bump the corpus version** and assign a stable **snapshot id**; document it in the corpus card.
4. **Confirm reproducibility:** re-fetch / re-assemble from the manifest and verify the result is **bit-identical**
   (hashes match exactly). A snapshot that won't reproduce bit-identically is not frozen — HALT + escalate.
5. Record the frozen snapshot id + corpus-card version + SHA manifest where IR-B can consume them.

## Output to the orchestrator (how this session reports back)
- Artifact(s): the updated `tests/corpus/corpus-card.md` (versioned + SHA manifest) + the **frozen snapshot id**;
  a freeze record on the board (`STATUS-0.8.0.md` §7).
- Schema/contract: snapshot record = {snapshot_id, corpus_version, per_source_sha256[], total_docs, reproduced_bit_identical
  (bool)}. No fabricated hashes — every hash is computed from the real artifact.
- Hand-off line: the ⬛ COR-2 freeze is **consumed by IR-B** (the fact-level gold set must be built on a frozen corpus);
  it does **not** gate GA (GA pins its own snapshot at B-1). Deadline: **before IR-B**.
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned. Owner-paced (corpus track is out-of-band; [[fathomdb-consumer-agents]]).

## Full prompt / next
- Authoritative prompt: none yet — **THIS scaffold is the starter; expand into a full prompt before running**
  (corpus owner). Informed by GA-1's corpus-quality classification.
- On completion → ⬛ frozen snapshot **unblocks IR-B** (together with ◆ B-1 ruled + IR-A merged).
