# ◆ B-1 — corpus-basis ruling for the FIDELITY floor · track:GA · type:HITL-gate

## Purpose (1–2 sentences)

The GA-critical HITL decision: pick the **corpus basis** the eu7/AC-075 fidelity floor (≥0.90) is measured against,
so the floor is honest **and** GA can proceed. This scaffold is a **decision package the orchestrator assembles**
from GA-1 — it is **not** an agent to spawn.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] **GA-1 output exists** (the OLD-vs-NEW A/B numbers + classification). — verify:
  `ls dev/plans/runs/GA-1-corpus-ab-*-output.json` and confirm `old_recall`/`new_recall`/`classification` are real (not TBD).
- [ ] The recall-eval assessment is on hand (fidelity ≠ relevance framing). — verify:
  `ls dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`.
- (This is type:HITL-gate — the orchestrator assembles the package; HITL decides. No worktree, no work agent.)

## Work to-do (the steps)

(Decision package the orchestrator assembles for HITL — not work steps.)

1. **GA-1 evidence:** old_recall vs new_recall + CIs/σ/N + the harder-corpus-vs-regression-vs-defect classification.
2. **The three options** (per the roadmap Gate B-1):
   (1) **pin the floor to the old/defined snapshot** — *recommended*; decouples GA from the out-of-band expansion, GA proceeds;
   (2) **adopt the expanded corpus** → retrieval/engine work before GA — *lowest-leverage* per the assessment
       (fidelity already ≫ the 0.571 relevance ceiling, so 0.87→0.95 engine work buys ≈0 product value);
   (3) **pin to a frozen, versioned snapshot** — clean and reproducible.
3. **Recommendation:** pin to a versioned snapshot (option 1/3 family), per the assessment + the
   [[pr2a-go-recompute-split]] precedent (a recall scare was once a measurement artifact).
4. **Non-negotiable:** **never lower the floor** / weaken the eu7 assert
   ([[0.8.0-ga-blocked-recall-corpus]], [[perf-recall-gates-masked-and-ac013b-conflation]]).

## Output to the orchestrator (how this session reports back)

- Artifact(s): the **B-1 ruling recorded on the board** (`dev/plans/runs/STATUS-0.8.0.md` §7) + the ADR amendment it
  implies (`ADR-0.7.0-vector-binary-quant.md` corpus-basis clause), authored downstream at GA-2.
- Schema/contract: the decision-package contents = {GA-1 numbers, 3 options, recommendation (pin-snapshot),
  "never lower the floor"}; the HITL ruling = which option + the pinned snapshot id/basis.
- Hand-off line: a ruled B-1 **unblocks GA-2 / Slice-40** (pin eu7's corpus, finalize AC-075) **AND IR-B** (the
  corpus basis the fact-level gold set pins to).
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned.

## Full prompt / next

- Authoritative prompt: none — this is a **HITL gate**; the orchestrator prepares the package, HITL decides.
  Context: `dev/plans/prompts/0.8.0-ORCHESTRATOR-CONTINUE-GA-RECALL.md` §"B-1".
- On completion → **GA-2 / Slice-40** (GA track) **∥** unblocks **IR-B** (IR-eval track, once corpus is also frozen).
