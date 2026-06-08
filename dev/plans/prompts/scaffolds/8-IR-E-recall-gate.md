# IR-E — IR-2 (analysis → HITL recommendation) · track:IR-eval · type:work

## Purpose (1–2 sentences)
Independently analyze the IR-1 outputs (experiment data + AC-077 structure) — achievability vs the ≈0.571 ceiling,
per-class/per-mode, gate-now-vs-track — and produce a **HITL gate recommendation** for AC-077's binding thresholds.
Recommends, does not decide.

## Prerequisites (verify ALL before starting — do not start if any is unmet)
- [ ] **IR-1 Ph4 merged** (AC-077 + experiment outputs on `main`). — verify:
  `git show main:dev/acceptance.md | grep -n "AC-077"` resolves **and** the Ph3 experiment outputs file exists
  (`ls dev/plans/runs/*ir*experiment*` or per the IR-B/C/D output contract).
- [ ] The signed measure is available (the analysis frame). — verify: `git show main:dev/design/ir-recall-measure.md`.

## Work to-do (the steps)
**Follow the authoritative prompt `dev/plans/prompts/0.8.x-IR-2-recall-gate.md` — this scaffold is the launcher.**
Key reminders:
1. **Achievability vs the ≈0.571 IR ceiling** — assess whether any proposed threshold is reachable; a bar above the
   measured ceiling = a permanently-red, useless gate (escalate, don't propose it).
2. **Per-class / per-mode** breakdown — don't collapse to one number; relevance varies by mode×K×class.
3. **Gate-now-vs-track** — recommend whether AC-077 is a binding gate now or a tracked/report-only metric for 0.8.x.
4. **Recommend, don't decide** — the binding thresholds are set by HITL at ◆ IR-gate.

## Output to the orchestrator (how this session reports back)
- Artifact(s): the `g9`/IR-2 recommendation doc (per the prompt) + merge to local `main` (no push).
- Schema/contract: recommendation = {per-class/per-mode achievability vs ceiling, proposed thresholds (with
  reachability evidence), gate-now-vs-track recommendation, risks}. Thresholds are **proposals** — TBD until the gate.
- Hand-off line: the recommendation **feeds ◆ IR-gate** (HITL sets AC-077's binding thresholds + flips gated/tracked).
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); no push/tag; board is
  orchestrator-owned. If any proposed threshold exceeds the measured ceiling → HALT + escalate
  ([[fathomdb-recall-fidelity-vs-relevance]]).

## Full prompt / next
- Authoritative prompt: `dev/plans/prompts/0.8.x-IR-2-recall-gate.md`.
- On completion → orchestrator routes the recommendation → **◆ IR-gate**.
