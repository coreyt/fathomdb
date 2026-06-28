# GA-1 — OLD-vs-NEW corpus recall A/B · track:GA · type:work

## Purpose (1–2 sentences)

Run an eu7-style real-embedder recall@10 A/B on the OLD (pre-expansion, ~0.7.2) corpus snapshot vs the
current NEW corpus (~7,667 docs) so HITL can tell **harder/larger corpus** apart from a **real regression**
or a **corpus-quality defect** at the ◆ B-1 ruling. Produces evidence only — **not** a corpus-basis recommendation.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] Step-0 item — runs **now**, no upstream gate. — verify: roadmap §"Step 0" lists GA-1 with `Depends on: nothing`
  (`grep -n "GA-1" dev/plans/0.8.0-GA-and-IR-eval-roadmap.md`).
- [ ] The real **bge-small** embedder is available for eu7 (NOT the synthetic `VaryingEmbedder`). — verify: the eu7
  path runs with the real model in `--release` (`grep -rn "bge" crates/ tests/ | head`; confirm eu7 uses it).
- [ ] The **OLD** (pre-expansion / ~0.7.2) corpus snapshot is obtainable. — verify: locate the pre-expansion corpus
  (git history of `data/corpus-data/raw/` or a 0.7.2 tag) and confirm it is the basis the 0.937 anchor was measured on.
- [ ] The **NEW** corpus is the current ~7,667-doc set. — verify: `ls data/corpus-data/raw/` (8 datasets;
  doc count ≈ 7,667 per board §1 / §7 2026-06-07).
- [ ] Measurement is `--release` + **isolated** (no concurrent workspace build/test). — verify: run via the
  canonical perf path, not `check.sh`/`cargo test --workspace` ([[perf-recall-gates-masked-and-ac013b-conflation]]).

## Work to-do (the steps)

1. Pin the **exact eu7 code path** (real bge-small, recall@10, bit-KNN+f32-rerank vs exact-f32 top-10) and confirm
   it is **byte-identical between the two runs** — only the corpus input differs.
2. Run eu7 recall@10 on the **OLD** snapshot, `--release`, isolated. Capture `recall@10`, bootstrap CI, σ, N.
3. Run eu7 recall@10 on the **NEW** corpus, same path, `--release`, isolated. Capture the same fields (expect ≈0.8710,
   CI 0.835–0.904, σ 0.018, N=7,667 per the prior Slice-40 measurement — reproduce, do not assume).
4. **Read the REAL exit code / numbers** of each `cargo test` run, not a wrapper `echo` ([[background-exit-masks-real-exit]]).
5. **Classify** the OLD→NEW delta into exactly one (or a weighted mix, with reasoning) of:
   **(a) harder/larger corpus** (more docs ⇒ more near-neighbors ⇒ lower fidelity recall, expected),
   **(b) real regression** (a code/index change degraded fidelity), or
   **(c) corpus-quality defect** (near-dups / bad docs in the expansion polluting neighbors).
6. Do **NOT** recommend a corpus-basis ruling (pin-old / adopt / snapshot) — that is the ◆ B-1 package. Provide the
   evidence + the classification only.

## Output to the orchestrator (how this session reports back)

- Artifact(s): `dev/plans/runs/GA-1-corpus-ab-<ts>.md` (narrative + methodology) **and**
  `dev/plans/runs/GA-1-corpus-ab-<ts>-output.json`.
- Schema/contract: `output.json` keys — `old_recall`, `old_ci`, `old_sigma`, `old_n`, `new_recall`, `new_ci`,
  `new_sigma`, `new_n`, `eu7_path_identical` (bool), `classification` (a|b|c|mix), `classification_rationale`,
  `discipline` (`--release`+isolated, real-exit confirmed). Numbers unknown until measured = `TBD`, never fabricated.
- Hand-off line: feeds **◆ B-1**; the orchestrator assembles the B-1 decision package from these numbers + the
  recall-eval assessment. Unblocks nothing else directly (B-1 is the gate).
- Discipline: `--release`+isolated for all measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned. **Do NOT lower the floor or weaken the eu7 assert** ([[0.8.0-ga-blocked-recall-corpus]]).

## Full prompt / next

- Authoritative prompt: none yet — **THIS scaffold is the starter; expand into a full prompt before running**
  (delegated / `perf-canonical`, never main-thread). Context: `dev/plans/prompts/0.8.0-ORCHESTRATOR-CONTINUE-GA-RECALL.md`
  (recall background) + `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md` (fidelity-vs-relevance).
- On completion → orchestrator assembles **◆ B-1 — corpus-basis ruling**.
