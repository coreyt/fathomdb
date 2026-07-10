# Design — operational agent-audit harness

Design of record for the `dev/agentic-rubric/` harness. It productizes the TERMINAL rubric
`dev/design/agent-harness-evaluation-rubric-v3.md` (v3.1) into a repeatable HITL tool.
Companions: `requirements.md` (what must be true) and `acceptance.md` (how we falsify it).

## Problem

The v3 rubric is complete and has been run once — a non-author judge scored release 0.8.19 by
hand (`dev/design/rubric-run-0.8.19-2026-07-10.md`, HARD gate PASS, 84.4% severity-weighted).
The deterministic detector scaffolding exists (`dev/experiments/rubric-stress-test/`). What is
missing is the **operational loop**: an automated `[L]`/`[H]` judge, a way to propose changes
to the *audited repo's* agents, a gated apply, and automated milestone re-measurement. This
subtree builds exactly those four, reusing everything else.

## Non-goals

- **Not a new rubric.** v3.1 is TERMINAL; the audit/revision line closed at v3. This tool
  consumes it. Improving the *rubric* is the existing §5 method; improving the *agents* is this
  tool.
- Not an autonomous self-modifier — apply is HITL-gated (mandate rule).
- Not a new LLM gateway — it uses the existing **airlock** (LiteLLM, OpenAI-compatible).
- Does not feed raw transcripts into an LLM — the existing detectors' bounded-memory,
  aggregates-only invariant is preserved.

## Capability map (what this tool fills)

| # | Capability | Before | This tool |
| --- | --- | --- | --- |
| 1 | Ingest transcripts + witnesses | deterministic feeders exist | thin-wrap into a typed subject |
| 2 | Airlock `[L]`/`[H]` judge | **missing** (done by hand) | **built** |
| 3 | Propose changes to audited agents/CLI/prompts | **missing** | **built** |
| 4 | HITL-gated apply | **missing** | **built** |
| 5 | Milestone re-measurement | one manual run | **automated** |

## Architecture

Isolated code under `dev/agentic-rubric/harness/` (package `harness`; run
`python -m harness.<cmd>` via `run.sh`, which puts the harness, `src/python` (for `eval.*`), and
`dev/experiments/rubric-stress-test` (for detector reuse) on `PYTHONPATH`). Pure logic is
separated from I/O so unit tests run with fakes and make **zero** network calls (the
`eval/autoe_judge.py` discipline).

```text
ingest → AuditSubject → judge(rubric v3) → scorecard/AuditReport
                                              │
                                     propose → [HITL decision package] → apply → milestone
                                              │                                     │
                                              └──────────── agent-rubric-ledger.jsonl
```

### Components

1. **`harness/ingest.py` — reuse, thin-wrap (gap 1).** Wraps
   `rubric-stress-test/parse.py` + `run_detectors.py` (bounded-memory transcript reader; never
   feeds a raw transcript to an LLM) and the repo-witness set (`dev/plans/runs/*-output.json`,
   `*-review-*.md`, STATUS boards, ledgers, `git log`) into a content-addressed `AuditSubject`
   (session id, subject archetype, release, sha, artifact paths + hashes) so every audit is
   reproducible. The 0.8.19 run showed artifacts-only scoring is often sufficient and safer.

2. **`harness/judge.py` — the airlock judge (gap 2).** Ported from `eval/autoe_judge.py` +
   `eval/gold_gen.py::_call_llm`. Dispatches per the criterion's verification class:
   - `[D]` → run the existing `detectors.py` detector; mechanical MET/UNMET, no LLM.
   - `[L]` → airlock adjudication from transcripts/diffs/ledger.
   - `[H]` → detector gathers evidence, airlock adjudicates whether it supports the *decisive*
     claim (the CR-047 trap the rubric §2.1 exists to catch — the `[H]` LLM step cannot be dropped).
   Carries v3's protocol rules as code: **judge ≠ author** (self-preference guard),
   **evidence-before-verdict** (no MET without a citation), ABSENT-safe parsing, seeded
   runs/bootstrap CIs, `custom_id` resume, and **HARD-gate-first** aggregation
   (`Σ(w·MET)/Σ(w)` using `audit/severity_vector_v3.json`; any HARD UNMET fails the subject).
   Fail-closed `--max-usd` budget (`eval/gap_decomposition_run.py`); optional Batch API path
   (`eval/p0a_batch_e2e.py`) since per-criterion calls are independent. Emits a scorecard in the
   `dev/design/rubric-run-<release>-<date>.md` shape + a `run` ledger entry.

3. **`harness/propose.py` — propose changes to the audited repo (gap 3).** Turns UNMET findings
   into typed `ChangeProposal`s against `.claude/agents/*.md`, `scripts/agent-*.sh`,
   `dev/plans/prompts/*`, `AGENTS.md`, `dev/design/orchestration.md`. **Distinct from rubric
   self-amendment** — this changes the *subject*, not the *instrument*. Fields: `target_path,
   rationale, criteria_addressed` (v3 ids), `finding_refs, patch, risk_tier, acceptance_signal,
   rollback`. Ties each proposal to the relevant `TC-RUBRIC-N` where one exists (e.g. TC-RUBRIC-5
   → an A2 PreToolUse deny guard). Rendered as a `◆ HITL decision package` (see
   `prompts/decision-package.md`). No auto-apply.

4. **`harness/apply.py` — HITL-gated apply (gap 4).** Requires an explicit HITL approval
   manifest; applies only approved proposal ids in a fresh git worktree
   (one-writer-per-checkout); runs `scripts/agent-verify.sh`; aborts and leaves the target
   untouched on failure. Writes `apply` ledger entries (`decider=hitl` for the approval,
   `decider=harness` for the mechanical apply).

5. **`harness/milestone.py` — automate the re-measure loop (gap 5).** A milestone binds a target
   session/release set + rubric version + expected deltas. The `reopen` triggers already on the
   `TC-RUBRIC-N` items ("next release scored under the rubric") are the natural milestones.
   Re-runs the audit and reports per-dimension/per-criterion deltas with CIs; records a
   `milestone` ledger entry. **Anti-Goodhart:** reuse `audit/compute_irr.py` to track judge↔human
   agreement (κ) per rubric version against the existing labeled packs
   (`audit/judge_A.jsonl`/`judge_B.jsonl`) — honors "no agent-generated oracles." A rubric
   version whose mean rises while agreement falls is flagged.

6. **CLI + prompt gists.** `python -m harness.{run,propose,apply,milestone,report}` via `run.sh`,
   emitting structured JSON like `agent-*.sh`. Prompts in `prompts/` cite v3 criteria + the
   `[D]/[L]/[H]` class and front-load invariants (`AGENTS.md §8`).

## Reuse anchors (do not reinvent)

| Need | Reuse | Path |
| --- | --- | --- |
| Rubric of record | v3.1 TERMINAL | `dev/design/agent-harness-evaluation-rubric-v3.md` |
| Transcript parse + `[D]` detectors | stress-test | `dev/experiments/rubric-stress-test/{parse,detectors,run_detectors}.py` |
| Severity vector + adjudication + IRR | audit tooling | `dev/experiments/rubric-stress-test/audit/{severity_vector_v3.json,build_audit.py,compute_irr.py}` |
| Scorecard shape | 0.8.19 run | `dev/design/rubric-run-0.8.19-2026-07-10.md` |
| Ledger (append, O(delta) read) | agent-rubric-ledger | `dev/steward/agent-rubric-ledger.jsonl`, `dev/agent-tools/{ledgerwrite,ledgerwatch}` |
| Airlock judge structure + bias controls | `autoe_judge` / `gold_gen` | `src/python/eval/autoe_judge.py`, `eval/gold_gen.py` |
| Batch API + fail-closed budget | `p0a_batch_e2e` / `gap_decomposition_run` | `src/python/eval/` |
| Codex review wrapper (apply-verify) | `codex-nostdin.sh` | `dev/agent-tools/codex-nostdin.sh` |

## Guardrails honored

TDD (RED→GREEN, pure logic + fakes); no agent-generated oracles (IRR vs human packs); mandate
rule (apply HITL-gated); one-writer-per-checkout (apply in a worktree); fail-closed budgeting +
ABSENT-safety + resume; judge ≠ author; evidence-before-verdict; HARD-gate-first aggregation;
raw transcript never enters an LLM context.

## Build phasing (mod-5 ladder)

- **Slice 5 — `$0` infra, no network.** `harness/ingest` (wrap existing parse/detectors) +
  scorecard assembly + `FakeJudge`. Reproduce the 0.8.19 artifacts-only scorecard deterministically.
- **Slice 10 — airlock judge (gap 2).** `[L]`/`[H]` adjudication + protocol rules + budget guard;
  pilot-first cost projection to the HITL. Re-score 0.8.19 and compare to the hand run.
- **Slice 15 — propose + HITL decision package (gap 3).**
- **Slice 20 — HITL-gated apply (gap 4)** + ledger `apply` kind.
- **Slice 25 — milestone automation + IRR agreement (gap 5).**

## Open items to confirm at Slice-5 kickoff

- Raw Claude Code session JSONL location for gap-2+ ingest (existing probes read a host path
  like `~/transcript-data/…`); witnesses alone suffice for Slice 5.
- Default judge model family for the self-preference guard (a non-Claude airlock alias vs. an
  explicit cross-family corroboration run, matching the pilot's Opus-tier + Fable-High pair).
- Whether to lift the shared airlock/budget helpers out of `eval/` into a small importable
  module, or import `eval.*` directly via `PYTHONPATH`.
