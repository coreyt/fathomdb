# Template — HITL decision package

The `◆ HITL-gate` artifact `harness/propose.py` emits for the human to approve, reject, or edit
before `harness/apply.py` may touch anything. Rendered as this markdown plus a sibling JSON
manifest the apply step consumes. No change is applied without an approval manifest
(`AGENTS.md` mandate rule). Every proposal targets the audited *subject*, never the rubric.

## Header

```text
Decision package: {package_id}
Subject: {subject_id}  ({archetype}, release {release}, sha {sha})
Rubric: agent-harness-evaluation v{rubric_version} (TERMINAL)
Result: HARD gate {PASS|FAIL} · severity-weighted {weighted_overall}%  (MET {n_met} / UNMET {n_unmet} / N-A {n_na})
Projected re-audit cost: {projected_usd} USD (cap {max_usd})
Prepared by: harness   Decider: HITL (coreyt)
```

## Findings → proposals

One block per proposal, each approvable independently:

```text
[{risk_tier}] {proposal_id} → {target_path}
Addresses: {criteria_addressed}   TC: {tc_ref}   (findings {finding_refs})
Why: {rationale}
Change:
{patch}
Acceptance signal (how the next audit confirms it worked): {acceptance_signal}
Rollback: {rollback}
Decision: [ ] approve  [ ] reject  [ ] edit → ______
```

## Approval manifest (the machine-readable gate)

`apply.py` refuses to run without this and applies **only** the approved proposal ids.

```json
{
  "package_id": "",
  "decider": "hitl",
  "approved": ["proposal_id_1"],
  "rejected": ["proposal_id_2"],
  "edited": { "proposal_id_3": "revised patch or note" },
  "milestone": "optional milestone id / TC-RUBRIC-N reopen this apply advances"
}
```

## After apply

`apply.py` records the applied set and the post-apply `agent-verify` result to
`dev/steward/agent-rubric-ledger.jsonl` (`kind: apply`; `decider=hitl` for the approval,
`decider=harness` for the mechanical apply). If a `milestone` (or a `TC-RUBRIC-N` `reopen`
trigger) was named, it schedules the re-audit that measures the criterion delta.
