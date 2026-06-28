# IR-B/C/D — IR-1 Phases 2–4 (code → experiments → mint AC-077) · track:IR-eval · type:work

## Purpose (1–2 sentences)

Build the IR-eval harness + fact-level gold set on the frozen corpus (Ph2 CODE), run the measure across mode×K×class
(Ph3 EXPERIMENTS), then **mint AC-077 grounded by the Phase-3 data** (Ph4 — structure + measured/proposed numbers,
binding thresholds still TBD). The IR-eval long pole.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] **IR-A merged** (signed measure on `main`). — verify: `git show main:dev/design/ir-recall-measure.md`
  resolves and is signed.
- [ ] **◆ B-1 ruled** (corpus basis pinned). — verify: board §7 "B-1 RULED" entry
  (`grep -n "B-1" dev/plans/runs/STATUS-0.8.0.md`).
- [ ] **⬛ COR-2 frozen** (versioned SHA-256 snapshot exists). — verify: `tests/corpus/corpus-card.md` carries the
  frozen snapshot id + SHA manifest; freeze recorded on the board.
- [ ] **◆ Slice-40 merged** (AC-075/076 on `main`) — required only by **Ph4** so AC-077 is the next free id. — verify:
  `git show main:dev/acceptance.md | grep -E "AC-076"` resolves; AC-077 is the next unused id.

## Work to-do (the steps)

**Follow the authoritative prompt `dev/plans/prompts/0.8.x-IR-1-recall-measure.md` (Phases 2–4) — this scaffold is the
launcher.** Key reminders:

1. **Order = CODE → EXPERIMENTS → AC** (Ph2 → Ph3 → Ph4). Build the harness + **fact-level gold set** on the
   **frozen** corpus first (eu8 K-ladder, pooling, reranker seam); then run experiments; mint the AC **last**.
2. **Mint AC-077 LAST and grounded** — structure + measured/proposed numbers from the Ph3 data; **binding thresholds
   stay TBD** (set at ◆ IR-gate). A blind threshold = a permanently-red gate (eu8 IR ceiling ≈0.571).
3. All measurement **`--release` + isolated**; **pin the eval set**; **leave eu7 untouched** (fidelity axis is separate).
4. Read REAL exits ([[background-exit-masks-real-exit]]); no fabricated numbers.

## Output to the orchestrator (how this session reports back)

- Artifact(s): the experiments outputs file (structured Ph3 results) + the eval harness + **AC-077** (structure, TBD
  thresholds) in `dev/acceptance.md` + merge to local `main` (no push).
- Schema/contract: Ph3 outputs = per (mode × K × class) measured relevance recall + CIs; AC-077 = {id, measure ref
  (IR-A doc), measured/proposed numbers, thresholds=TBD, gated-vs-tracked=TBD}. No fabricated thresholds.
- Hand-off line: orchestrator gates the merge (codex §9 over the AC + harness diff); the experiment outputs + AC-077
  **feed IR-E** (IR-2 analysis).
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned.

## Full prompt / next

- Authoritative prompt: `dev/plans/prompts/0.8.x-IR-1-recall-measure.md` (Phases 2–4, DEFERRED header).
- On completion → orchestrator codex-reviews → merge; → **IR-E (IR-2)**.
