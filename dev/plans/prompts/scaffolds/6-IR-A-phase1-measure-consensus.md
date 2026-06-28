# IR-A — IR-1 Phase 1 (measure + Claude↔codex consensus) · track:IR-eval · type:work

## Purpose (1–2 sentences)

Define the IR/agentic-**relevance** measure (the product-value recall axis FathomDB lacks — distinct from the eu7
fidelity floor) and reach a **Claude↔codex consensus** on the methodology, producing a signed
`dev/design/ir-recall-measure.md`. Pure design; runs now with no prerequisites.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] Step-0 item — runs **now**, blocks nothing on GA. — verify: roadmap §"Step 0" lists IR-A with `Depends on:
  nothing` (`grep -n "IR-A" dev/plans/0.8.0-GA-and-IR-eval-roadmap.md`).
- [ ] codex is runnable here (consensus reviewer). — verify: `codex exec review` is available with
  `--dangerously-bypass-approvals-and-sandbox` ([[orchestration-execution-traps]]).
- [ ] The fidelity-vs-relevance framing is on hand. — verify: `ls dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`
  and [[fathomdb-recall-fidelity-vs-relevance]].

## Work to-do (the steps)

**Follow the authoritative prompt `dev/plans/prompts/0.8.x-IR-1-phase1-measure-consensus.md` — this scaffold is the
launcher.** Key reminders:

1. **Definition + methodology only** — define the IR/agentic-relevance measure (evidence/task recall); reach a
   genuine **Claude↔codex consensus** on it.
2. **No AC, no gold set, no experiments** in this phase (those are IR-B/C/D) — and **don't commit to a specific corpus
   snapshot** (the corpus basis is ruled at B-1, frozen at COR-2).
3. Keep the **fidelity axis (eu7/AC-075) separate** from this relevance axis — they are different gates
   ([[fathomdb-recall-fidelity-vs-relevance]]).

## Output to the orchestrator (how this session reports back)

- Artifact(s): signed `dev/design/ir-recall-measure.md` + the codex consult/consensus log
  (`dev/plans/runs/` review artifact) + merge to local `main` (no push).
- Schema/contract: the measure doc = {definition of IR/agentic-relevance recall, methodology (K-ladder / pooling /
  reranker seam shape), what's in/out of scope for Phase 1, the Claude↔codex consensus record}. Thresholds = **TBD**
  (decided post-experiment at IR-D/IR-gate) — no fabricated numbers.
- Hand-off line: the signed measure **feeds IR-B** (Phase 2 code builds to it) **and IR-E** (IR-2 analyzes against it).
  Blocks nothing on GA.
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned.

## Full prompt / next

- Authoritative prompt: `dev/plans/prompts/0.8.x-IR-1-phase1-measure-consensus.md`.
- On completion → orchestrator codex-reviews the measure; output feeds **IR-B/C/D** (after ◆ B-1 + ⬛ COR-2) and **IR-E**.
