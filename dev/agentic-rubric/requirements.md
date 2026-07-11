# Requirements — operational agent-audit harness

Needs → requirements for the four missing capabilities (the rubric itself is out of scope — it
is TERMINAL at v3.1). Every requirement has a falsifiable acceptance signal in `acceptance.md`
(matched by ID) and traces to v3 criteria and/or an existing `TC-RUBRIC-N` gap item. IDs are
stable.

## Needs (the "why")

- **OR-NEED-1** — The rubric can only be applied by hand today; there is no repeatable tool, so
  each release re-scoring is expensive and non-reproducible.
- **OR-NEED-2** — Gaps found by a run become `TC-RUBRIC-N` prose; nothing turns them into
  concrete, reviewed changes to the agents/CLI/prompts, or measures whether the change helped.
- **OR-NEED-3** — Priced LLM-judge runs must be safe to spend on (budgeted, resumable, unbiased,
  non-author) and must never leak raw transcripts into a model context.
- **OR-NEED-4** — Improvement must be provable, not asserted, and resistant to Goodharting the
  metric.

## Requirements (the "what")

### Ingest + judge

- **OR-REQ-1** — Ingest raw transcripts (via the existing `parse.py`/`run_detectors.py`) and repo
  witnesses into one typed, content-addressed `AuditSubject`; identical inputs → identical
  subject hash. *(→ OR-NEED-1)*
- **OR-REQ-2** — Dispatch each of the 62 v3 criteria by its verification class: `[D]` runs the
  existing detector with no LLM; `[L]` calls the airlock judge; `[H]` gathers evidence
  deterministically then has the airlock judge adjudicate the decisive claim. The `[H]` LLM step
  is never dropped. *(→ OR-NEED-1; rubric §2.1)*
- **OR-REQ-3** — Enforce v3 protocol rules in code: judge ≠ author (self-preference guard),
  evidence-before-verdict (no MET without a citation), ABSENT-safe parsing (unparseable →
  excluded, never a silent MET/UNMET). *(→ OR-NEED-3; rubric §2 rules 2/3)*
- **OR-REQ-4** — Aggregate HARD-gate-first: any HARD UNMET fails the subject; otherwise severity-
  weighted `Σ(w·MET)/Σ(w)` over applicable criteria using `audit/severity_vector_v3.json`.
  Emit a scorecard in the `rubric-run-<release>-<date>.md` shape. *(→ OR-NEED-1; rubric §2)*
- **OR-REQ-5** — Priced runs are fail-closed budgeted (`--max-usd` preflight refusal), idempotently
  resumable via `custom_id`, and never place a raw transcript in an LLM context. *(→ OR-NEED-3)*
- **OR-REQ-5a** — **Budget-exhaustion is detected mid-run and handled by a tiered fallback, then a clean
  stop.** When spend reaches `--max-usd`, or the airlock returns a budget/quota signal (429 / virtual-key
  limit / provider quota), the harness (1) **detects** it (does not blow past the cap), (2) **tries a
  configured fallback route** — a cheaper/local airlock model (e.g. a self-hosted vLLM/Ollama alias,
  ~$0) for the remaining un-judged criteria, and (3) if the fallback is also unavailable or exhausted,
  **stops cleanly**: checkpoints the completed `custom_id`s, emits a **partial scorecard explicitly marked
  INCOMPLETE** (which criteria are un-judged), records the stop reason to the ledger, and exits non-zero —
  never a silent truncation that reads as "fully scored." The fallback tier and its model are HITL-set;
  a run may disable fallback (`--no-fallback`) to hard-stop at the cap. *(→ OR-NEED-3)*

### Propose + apply

- **OR-REQ-6** — Turn UNMET findings into typed `ChangeProposal`s against the audited repo's
  agents/CLI/prompts, each with `target_path`, `criteria_addressed`, `finding_refs`, a patch, a
  `risk_tier`, an `acceptance_signal`, and a `rollback`; distinct from rubric self-amendment.
  *(→ OR-NEED-2)*
- **OR-REQ-7** — Nothing is applied without an explicit HITL approval manifest; apply runs in a
  fresh worktree and gates on `scripts/agent-verify.sh`, aborting cleanly on failure.
  *(→ OR-NEED-2; mandate rule)*

### Measure

- **OR-REQ-8** — Milestones re-audit a subsequent/held-out set (the `TC-RUBRIC-N` `reopen`
  triggers) and report per-dimension/per-criterion deltas with CIs per rubric version.
  *(→ OR-NEED-4)*
- **OR-REQ-9** — Every run/proposal/apply/milestone appends to `dev/steward/agent-rubric-ledger.jsonl`
  via `ledgerwrite` (new `kind`s `proposal`/`apply`; existing `run`/`milestone`), each stamped
  with a `decider`; O(delta)-readable via `ledgerwatch`. *(→ OR-NEED-2)*
- **OR-REQ-10** — Track judge↔human agreement (κ, via `audit/compute_irr.py`) against the existing
  labeled packs per rubric version, to flag Goodharting even when the mean score rises.
  *(→ OR-NEED-4)*
