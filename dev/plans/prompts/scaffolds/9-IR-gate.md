# ◆ IR-gate — set AC-077 binding thresholds · track:IR-eval · type:HITL-gate

## Purpose (1–2 sentences)

The final IR-eval HITL gate: take IR-2's recommendation and **set AC-077's binding thresholds** (and flip
gated-vs-tracked per-class/per-mode) → **AC-077 LIVE**. A decision package the orchestrator assembles; **not** an
agent to spawn.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] **IR-E (IR-2) recommendation exists.** — verify: the IR-2 recommendation doc is merged on `main`
  (per IR-E's output contract) and carries per-class/per-mode achievability + proposed thresholds.
- [ ] **AC-077 exists** (structure, TBD thresholds). — verify: `git show main:dev/acceptance.md | grep -n "AC-077"`.
- (type:HITL-gate — orchestrator assembles; HITL decides.)

## Work to-do (the steps)

(Decision package the orchestrator assembles for HITL — not work steps.)

1. **IR-2's recommendation** — per-class/per-mode achievability vs the ≈0.571 ceiling + the proposed thresholds +
   gate-now-vs-track call.
2. **Set AC-077's binding thresholds** — HITL fixes the numbers (only at/below the measured ceiling; a bar above it
   is a permanently-red gate — reject/track instead).
3. **Flip gated/tracked** — decide which AC-077 sub-metrics are binding gates vs report-only for 0.8.x.
4. STOP → present the package + recommendation; **wait for HITL**.

## Output to the orchestrator (how this session reports back)

- Artifact(s): AC-077's binding thresholds recorded in `dev/acceptance.md` + the ruling on the board
  (`STATUS-0.8.0.md` §7 / the 0.8.x board).
- Schema/contract: package = {IR-2 recommendation, proposed thresholds + reachability evidence, gated/tracked
  proposal}; the HITL ruling = the fixed thresholds + gated/tracked per metric.
- Hand-off line: the ruling makes **AC-077 LIVE** (gated/tracked per the decision) — ★ the IR product-recall gate exists.
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned. **Never set a threshold above the measured ceiling** ([[fathomdb-recall-fidelity-vs-relevance]]).

## Full prompt / next

- Authoritative prompt: none — **HITL gate**; orchestrator prepares the package from IR-2, HITL decides.
  Context: `dev/plans/prompts/0.8.0-MASTER-ORCHESTRATOR-HANDOFF.md` §"Per-gate decision packages".
- On completion → **★ AC-077 LIVE** (the IR-eval product-recall gate). Campaign complete.
