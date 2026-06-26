---
name: oob-creep-vs-justified-deviation
description: How to handle out-of-bounds work in orchestrated slices — distinguish scope-creep from justified-deviation; deviations must round-trip into the spec; forced deviation = the prompt was under-specified.
metadata: 
  node_type: memory
  type: feedback
  originSessionId: f2b2ccdc-c96e-417a-bf45-7f4b7ed7ee34
---

For the orchestrated slice workflow ([[fathomdb-080-plan-approved]],
[[orchestration-execution-traps]]), out-of-bounds (OOB) work is **two different
things**, and the response differs:

- **Scope creep (forbidden):** doing more because it's *possible/nicer*, not required.
  Under-specified prompts cause *interpretation drift*, not self-clarification — the
  agent invents scope. Prevent with explicit task boundaries; flag tempting extras as
  reserved-gap and move on.
- **Justified deviation (sometimes required):** a specified path is genuinely *blocked*
  (gone anchor, false precondition, inconsistent contract, changed reality / compile
  error). Then deviate — but: (1) **smallest** change that unblocks *your* mandate, never
  expands it; (2) **loud, not silent** — `[DETECT]` on console + record in
  `output.json.additional_changes_made_in_scope` with the why (~50% of agent failures are
  silent); (3) **escalate spec-level changes** (contract / another slice / public surface)
  to the orchestrator — deviations must **round-trip into the spec**, not be absorbed.

**Why:** A 2026-06-02 forensic audit of Slices 0 & 5 found ~zero avoidable slice-agent
creep — every within-slice "OOB" was a *minimal justified deviation to make an in-scope
deliverable actually work* (release-note stub for an in-scope nav entry; nav promotion for
an in-scope guide page; re-tokenization wiring so a migration isn't a no-op on existing
DBs) or review-induced fix-1. The residue was **contract under-specification**, not
misbehavior. The one heavy OOB (corpus-line integration, `83f5156`→`c27028b`) was a
separate HITL-authorized *post-close orchestrator* action, not slice-agent creep.

**How to apply:** (orchestrator) treat every forced deviation as a *defect report on the
prompt* — name companion artifacts (nav entries, stubs) + mechanism triggers + authorized
forward-propagation up front (see the AUTHORING CHECKLIST in
`dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md`); verify end-STATE not steps; close only after
review converges (avoid post-close reconciliation commits). (slice agent) follow the
template §6 creep-vs-deviation rule. Grounded in: Anthropic multi-agent (task boundaries;
end-state eval; "think like your agents"), spec-driven development (round-trip), and
scope-validation-loop research (ambiguity→drift; silent failures).
