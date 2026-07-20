# Plan — next-phase operational agent-audit harness (builds on rubric v3) + branch landing

## Context

The `rubric-eval-v3-terminal` branch already holds a **mature, mostly-complete project** — I
discovered this only after the first push was (correctly) rejected as non-fast-forward. My
initial plan assumed a greenfield and defined a *parallel* rubric; that was wrong and is being
corrected here. Nothing was pushed, so the remote is uncontaminated.

**What already exists on the branch (retain, do not duplicate):**

- **The rubric, TERMINAL** — `dev/design/agent-harness-evaluation-rubric-v3.md` (v3.1): 62
  criteria across 8 dimensions (A–H), MET/UNMET binary scoring, 12 HARD-gate invariants, a
  severity vector (`critical/high/med/low`), and every criterion tagged `[D]` deterministic /
  `[L]` LLM-judge / `[H]` hybrid. The judge is already specified as "a harness, not a single
  prompt" (tool-invoked, read-only, non-author, codex-§9 shape). Companion report + method:
  `agent-harness-evaluation-rubric-report-v3.md`, `dev/design/rubric-audit-and-revision-method.md`.
- **A real run** — `dev/design/rubric-run-0.8.19-2026-07-10.md` (non-author judge, HARD gate
  PASS, 84.4% severity-weighted), which drove the v3→v3.1 amendment.
- **Deterministic scaffolding** — `dev/experiments/rubric-stress-test/` (`parse.py`,
  `detectors.py`, `run_detectors.py`, `probes/`, `coverage/`, `audit/`) plus
  `dev/experiments/code-markers-eval/`. All deterministic; **no LLM/airlock call anywhere**.
- **Ledger** — `dev/steward/agent-rubric-ledger.jsonl` (kinds `milestone|decision|tc|run|
  amendment|confirmation`; every entry names a `decider`; gap items are `TC-RUBRIC-N`).

**The genuine gap the "next phase" fills** (measured capability map):

| # | Capability | Status today |
| --- | --- | --- |
| 1 | Ingest transcripts | PARTIAL — deterministic feeders exist (`parse.py`, `run_detectors.py`) |
| 2 | **LLM judge via airlock (`[L]`/`[H]`)** | **MISSING** — pilot adjudication was done by hand |
| 3 | **Propose changes to the audited repo's agents/CLI/prompts** | **MISSING** — only the rubric self-amends |
| 4 | **HITL-gated apply** | **MISSING** — explicitly "tracked, not built" |
| 5 | Milestone re-measurement | PARTIAL — one manual pilot; `build_audit.py`/`phase_a.py` make the audit reproducible, but no automation |

Intended outcome: productize the existing v3 rubric into a **repeatable, HITL-driven operational
tool** that runs `[D]` detectors + an airlock `[L]`/`[H]` judge against the 62 criteria, emits a
scorecard in the existing shape, and closes the **propose → apply → measure-at-milestone** loop
against the *audited repo's* agents/CLI/prompts — reusing (never re-deriving) the rubric,
detectors, and ledger.

### Decisions

- **First bullet (git landing)** — the "work" (the v3 rubric project) is already on
  `origin/rubric-eval-v3-terminal`. **Retain it**: rebase my planning commit onto
  `origin/rubric-eval-v3-terminal` (never force-push over it), then push. My earlier local
  branch was cut from `origin/main` before I knew the remote branch existed.
- **Build on v3, do not redefine** — delete my parallel `rubrics/agent-performance-v1.md`; the
  rubric of record is `dev/design/agent-harness-evaluation-rubric-v3.md`.
- **Inputs** — both raw session logs (via the existing `parse.py`/detectors) and repo witnesses
  (the 0.8.19 run showed artifacts-only scoring is often sufficient and safer).
- **Autonomy** — propose-first; apply requires an explicit HITL approval manifest (mandate rule).
- **Home** — *recommended:* keep one isolated dir `dev/agentic-rubric/` for the NEW operational
  tool + its plan (honors the earlier isolation ask), while **referencing** the v3 rubric and
  **reusing** the existing detectors + `agent-rubric-ledger.jsonl`. Alternative if preferred:
  adopt existing conventions (`dev/experiments/agentic-rubric-harness/` + `dev/design/agent-
  harness-*` + continue the ledger). Open for redirection at approval — the AskUserQuestion tool
  failed twice, so this defaults rather than blocks.

---

## Phase 0 — retain the branch + land the (reconciled) planning deliverables

Runs after approval (plan mode forbids mutations). One-writer-per-checkout via the worktree.

1. In the worktree `../fathomdb-rubric-eval`, **rebase** my planning commit onto the real branch:
   `git fetch origin rubric-eval-v3-terminal` then
   `git rebase origin/rubric-eval-v3-terminal` (my commit replays on top; the 97-file v3 project
   is preserved untouched). Resolve the fact that `.markdownlint-cli2.jsonc` already carries a
   `+6` change on the branch — take theirs.
2. **Replace the naive deliverables** authored earlier with the reconciled set (below): delete
   `dev/agentic-rubric/rubrics/agent-performance-v1.md`; rewrite `design.md`, `requirements.md`,
   `acceptance.md`, `prompts/*`, `README.md` to consume v3 and target only gaps 2–5.
3. Lint the subtree (`markdownlint-cli2`, already in the branch's glob scope) — my files clean;
   the 9 pre-existing `dev/research/*` findings are not mine and pre-date the branch.
4. Commit, then `git push -u origin rubric-eval-v3-terminal` (backoff retries). No PR unless asked.

Reconciled deliverables under `dev/agentic-rubric/` (docs only — no harness code yet):

- `README.md` — positions this subtree as the **operational tool** that runs the v3 rubric;
  points at `dev/design/agent-harness-evaluation-rubric-v3.md` as the instrument of record and
  `dev/experiments/rubric-stress-test/` as the reused detector layer.
- `design.md` — the operational-harness design (below).
- `requirements.md` + `acceptance.md` — requirements/acceptance for gaps 2–5 only, traceable to
  v3 criteria and to the existing `TC-RUBRIC-N` items.
- `prompts/{judge,proposer,decision-package}.md` — prompt gists that cite v3 criteria + the
  `[D]/[L]/[H]` class of the criterion being judged.

---

## The operational harness design (the "next phase")

Isolated code under `dev/agentic-rubric/harness/` (package `harness`, run
`python -m harness.<cmd>` with `PYTHONPATH` including the harness, `src/python` for `eval.*`
reuse, and `dev/experiments/rubric-stress-test` for detector reuse). Pure logic separated from
I/O; **zero** network in unit tests (the `eval/autoe_judge.py` discipline). It **consumes** v3,
it does not restate it.

### Components (each maps to a gap)

1. **`harness/ingest.py` — reuse, thin-wrap.** Wraps the existing
   `rubric-stress-test/parse.py` + `run_detectors.py` (bounded-memory transcript reader, never
   feeds a raw transcript into an LLM) and the repo-witness set (`dev/plans/runs/*-output.json`,
   `*-review-*.md`, STATUS boards, ledgers, git log) into a content-addressed `AuditSubject`.
   *(gap 1 — hardening, not greenfield)*

2. **`harness/judge.py` — the missing airlock judge (gap 2).** Ported from
   `eval/autoe_judge.py` + `eval/gold_gen.py::_call_llm`. Dispatches per the criterion's
   verification class: `[D]` → run the existing detector, no LLM; `[L]` → airlock adjudication;
   `[H]` → detector gathers evidence, airlock adjudicates whether it supports the decisive claim
   (the CR-047 trap the rubric §2.1 exists to catch). Carries v3's protocol rules as code: **judge
   ≠ author** (self-preference guard), **evidence-before-verdict** (no score without a citation),
   ABSENT-safe parsing, seeded runs/CIs, `custom_id` resume, and the HARD-gate-first aggregation
   (`Σ(w·MET)/Σ(w)` with the severity vector from
   `rubric-stress-test/audit/severity_vector_v3.json`). Fail-closed `--max-usd` budget
   (`eval/gap_decomposition_run.py`). Emits a scorecard in the
   `dev/design/rubric-run-<release>-<date>.md` shape and a `run` ledger entry.

3. **`harness/propose.py` — propose changes to the audited repo (gap 3).** Turns UNMET findings
   into typed `ChangeProposal`s against `.claude/agents/*.md`, `scripts/agent-*.sh`,
   `dev/plans/prompts/*`, `AGENTS.md`, `dev/design/orchestration.md`. **Distinct from rubric
   self-amendment** — this changes the *subject*, not the *instrument*. Fields: `target_path,
   rationale, criteria_addressed (v3 ids), finding_refs, patch, risk_tier, acceptance_signal,
   rollback`. Rendered as a `◆ HITL decision package`. Ties each proposal to the relevant
   `TC-RUBRIC-N` where one exists.

4. **`harness/apply.py` — HITL-gated apply (gap 4).** Requires an explicit approval manifest;
   applies only approved proposal ids in a fresh worktree; runs `scripts/agent-verify.sh`; aborts
   and leaves the target untouched on failure. Writes `apply` ledger entries (`decider=hitl` for
   approval, `decider=harness` for the mechanical apply).

5. **`harness/milestone.py` — automate the re-measure loop (gap 5).** A milestone binds a target
   session/release set + the rubric version + expected deltas (naturally: the `reopen` triggers
   already on the `TC-RUBRIC-N` items, e.g. "next release scored under the rubric"). Re-runs the
   audit and reports per-dimension/per-criterion deltas with CIs; records a `milestone` ledger
   entry. **Anti-Goodhart:** reuse the existing `audit/compute_irr.py` κ/agreement machinery to
   track judge↔human agreement per rubric version against the existing labeled packs
   (`judge_A.jsonl`/`judge_B.jsonl`) — honors "no agent-generated oracles."

6. **CLI + prompt gists.** `python -m harness.{run,propose,apply,milestone,report}` via a
   `run.sh` wrapper emitting structured JSON like `agent-*.sh`. Prompts in `prompts/` cite v3
   criteria and front-load invariants (`AGENTS.md §8`).

### Reuse anchors (do not reinvent)

| Need | Reuse | Path |
| --- | --- | --- |
| Rubric of record (62 criteria, `[D]/[L]/[H]`, HARD, Q-SEV) | v3.1 TERMINAL | `dev/design/agent-harness-evaluation-rubric-v3.md` |
| Transcript parse + deterministic detectors | stress-test | `dev/experiments/rubric-stress-test/{parse,detectors,run_detectors}.py` |
| Severity vector + adjudication + IRR | audit tooling | `dev/experiments/rubric-stress-test/audit/{severity_vector_v3.json,build_audit.py,compute_irr.py}` |
| Scorecard shape | 0.8.19 run | `dev/design/rubric-run-0.8.19-2026-07-10.md` |
| Ledger (append, O(delta) read) | agent-rubric-ledger | `dev/steward/agent-rubric-ledger.jsonl`, `dev/agent-tools/{ledgerwrite,ledgerwatch}` |
| Airlock LLM-judge structure + bias controls | `autoe_judge` / `gold_gen` | `src/python/eval/autoe_judge.py`, `eval/gold_gen.py` |
| Batch API + fail-closed budget | `p0a_batch_e2e` / `gap_decomposition_run` | `src/python/eval/` |
| Codex review wrapper (for apply-verify) | `codex-nostdin.sh` | `dev/agent-tools/codex-nostdin.sh` |

### Guardrails honored

TDD (RED→GREEN, pure logic + fakes); no agent-generated oracles (IRR vs human packs); mandate
rule (apply HITL-gated); one-writer-per-checkout (apply in a worktree); fail-closed budgeting +
ABSENT-safety + resume; judge ≠ author; evidence-before-verdict; HARD-gate-first aggregation;
raw transcript never enters an LLM context (existing detector invariant).

### Build phasing (mod-5 ladder)

- **Slice 5 — `$0` infra, no network.** `harness/ingest` (wrap existing parse/detectors) +
  scorecard assembly + `FakeJudge`. Reproduce the 0.8.19 artifacts-only scorecard deterministically.
- **Slice 10 — airlock judge (gap 2).** `[L]`/`[H]` adjudication + protocol rules + budget guard;
  pilot-first cost projection to the HITL. Re-score 0.8.19 and compare to the hand run.
- **Slice 15 — propose + HITL decision package (gap 3).**
- **Slice 20 — HITL-gated apply (gap 4)** + ledger `apply` kind.
- **Slice 25 — milestone automation + IRR agreement (gap 5).**

---

## Verification (end-to-end)

- **Unit:** `dev/agentic-rubric/harness/tests/test_*.py` with `FakeJudge`, zero network; a socket
  guard asserts no call in tests. Run via `dev/agentic-rubric/run.sh test`.
- **Dry-run:** reproduce the 0.8.19 scorecard from repo artifacts under `FakeJudge` and diff the
  per-dimension %MET against `rubric-run-0.8.19-2026-07-10.md` (the ground-truth hand run).
- **Priced pilot:** one small airlock run with `--max-usd`; confirm budget refusal + idempotent
  resume; confirm the self-preference guard rejects a same-family judge.
- **Gate:** `./scripts/agent-verify.sh` green before every commit.

## Open items

- Home/naming decision above (defaulted to isolated `dev/agentic-rubric/`; redirect at approval).
- Raw Claude Code session JSONL location for gap-2+ ingest (the existing probes read a host path
  `~/transcript-data/…`; witnesses alone suffice for Slice 5).
- Default judge model family for the self-preference guard (non-Claude airlock alias vs. an
  explicit cross-family corroboration run, matching the pilot's Opus-tier + Fable-High pair).
