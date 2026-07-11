# Agent-Audit Harness — Orchestrator hand-off (Slice 5+)

> **Entry point for an `/orchestrate` session** (or `/goal complete dev/agentic-rubric/ORCHESTRATOR-HANDOFF.md`).
> You are a **FathomDB orchestrator** building the operational agent-audit harness under
> `dev/agentic-rubric/harness/` by coordinating **`implementer`** subagents doing TDD in git
> worktrees. You do **NOT** write code yourself — you plan slices, spawn implementers, verify from
> git, run the codex §9 gate, and land to `main`. This is a **feature build on `main`, not a release**:
> no version bump, no `v*` tag, no publish. Read the cold-start set (§4) before acting; apply §0 every
> session.

---

## 0. Hard preflight (apply every session — the tree is shared with the Steward + other orchestrators)

1. **Branch check before EVERY commit/push** — `git rev-parse --abbrev-ref HEAD`. Slice work goes on the
   **slice branch** in its worktree; closure/land commits go on **`main`**. Never assume `main`; another
   session may have left a checkout on a feature branch. (`shared-checkout-branch-can-be-stale`.)
2. **Worktree-base check before spawning any implementer** — every slice worktree is cut from the **live
   tip of `main`** (`git rev-parse main`), never a stale base (`agent-worktree-stale-base-trap`). Run the
   `orchestration.md §1.6` preflight; STOP on exit 1.
3. **This build lands on `main` directly** (docs+Python, label-only). Do not create a release branch. Each
   slice is cherry-picked/merged to `main` after its §9 PASS.

## 1. Role & mission

Build the four missing capabilities that turn the TERMINAL rubric (v3.1) into an operational loop:
an **airlock `[L]`/`[H]` judge**, a **proposer** of changes to the audited repo's agents/CLI/prompts,
a **HITL-gated apply**, and **automated milestone re-measurement**. Everything else is reused, not rebuilt
(design §Reuse anchors). You orchestrate + gate; implementers write the code.

## 2. North-star — "done" for this build

Each capability lands behind its falsifiable acceptance signal (`acceptance.md`, matched by ID) with
`./scripts/agent-verify.sh` green and a codex §9 PASS. The build is complete when Slices 5→25 are landed:
the harness reproduces the 0.8.19 hand-scorecard deterministically (Slice 5), then **automatically**
scores a release, proposes subject-side fixes, applies approved ones in isolation, and re-measures deltas
with an anti-Goodhart κ (Slices 10–25).

## 3. Where things stand (verify from git before trusting this)

- **Planning docs COMPLETE on `main`:** `design.md`, `requirements.md` (incl. OR-REQ-5a budget-fallback),
  `acceptance.md`, `prompts/{judge,proposer,decision-package}.md`, this hand-off.
- **All reuse anchors present on `main`:** rubric v3.1 (`dev/design/agent-harness-evaluation-rubric-v3.md`),
  detectors + parse (`dev/experiments/rubric-stress-test/{parse,detectors,run_detectors}.py`), the new
  `[D]` detector `detect_s9_transcript.py`, audit tooling (`audit/{severity_vector_v3.json,build_audit.py,compute_irr.py}`),
  labeled packs (`audit/judge_A.jsonl`,`judge_B.jsonl`), the ground-truth scorecard
  (`dev/design/rubric-run-0.8.19-2026-07-10.md`), the ledger (`dev/steward/agent-rubric-ledger.jsonl`),
  the `eval/*` airlock/judge/batch/budget helpers, `scripts/agent-verify.sh`, airlock (`~/projects/airlock/docs/`).
- **Harness code: ZERO built.** `dev/agentic-rubric/harness/` does not exist yet. Start at Slice 5.

## 4. Cold-start reading (in order)

1. `dev/design/orchestration.md` — the method (three-role, preflight §1.6, spawn §2, codex §9 §3, decision
   loop §9, output.json §8, worktree cleanup §11). Follow literally.
2. `.claude/agents/implementer.md` — the implementer contract.
3. `dev/agentic-rubric/design.md` — the design of record (architecture, components 1–6, §Reuse anchors,
   §Build phasing, §Review resolutions R1–R5: **airlock-only judge on a non-Claude family**, budget-fallback,
   IRR honesty, the `[D]` detector).
4. `dev/agentic-rubric/{requirements,acceptance}.md` — the OR-REQ / OR-AC contract (the RED tests to write).
5. `dev/agentic-rubric/prompts/*` — the binding judge/proposer/decision-package prompts.
6. `dev/design/agent-harness-evaluation-rubric-v3.md` — the instrument the harness executes (do NOT edit it).
7. Memory index — `eval-environment-operational-notes`, `airlock-operational-notes`, budget discipline,
   worktree traps. Re-verify any file/flag a memory names.

## 5. Operating disciplines (load-bearing)

- **TDD, RED→GREEN.** Every slice starts from a failing test matching its `OR-AC-*` signal. Pure logic is
  separated from I/O so unit tests run with **fakes and zero network calls** (`eval/autoe_judge.py` pattern).
- **Airlock-only judge on a non-Claude family** (R2). The `[L]`/`[H]` judge reaches models ONLY via
  `http://localhost:4000/v1` (OpenAI-compatible, master key from `dev/.env.eval`). The judge family MUST
  differ from the audited Claude family — self-preference guard **fails closed** otherwise (OR-AC-3).
- **Fail-closed budget + tiered fallback** (OR-REQ-5/5a). `--max-usd` refuses before any call; on exhaustion,
  fall back to a cheaper/local airlock route, then **stop cleanly** with an INCOMPLETE-marked partial
  scorecard + a stop-reason ledger entry + non-zero exit. Never silent truncation.
- **Raw transcript never enters an LLM context** — only detector-extracted bounded windows.
- **Evidence-before-verdict; HARD-gate-first** aggregation (`Σ(w·MET)/Σw` via `severity_vector_v3.json`).
- **Mandate rule** — `apply` is HITL-gated (approval manifest); apply runs in a **fresh worktree**
  (one-writer-per-checkout) behind `agent-verify`.
- **No agent-generated oracles** — κ is measured against the human/independent packs, labeled honestly as
  inter-judge (R3).
- **Ledger everything** — each run/proposal/apply/milestone appends via `ledgerwrite` (kinds
  `run`/`proposal`/`apply`/`milestone`), each stamped with a `decider`.

## 6. Orchestration mechanics (per slice)

Per `orchestration.md`: preflight §1.6 → `git worktree add "$WT" -b "harness-slice-<N>-<ts>" $(git rev-parse main)`
→ spawn one `implementer` with the slice's RED-testable `OR-AC-*` signals + the reuse anchors it may import
→ implementer does RED→GREEN, runs `agent-verify`, writes `output.json` → you verify from git (head advanced,
witnesses present, real exit codes) → **codex §9 review gate** (`codex-nostdin.sh`; BLOCK never overridden,
CONCERN needs written rationale, fix-N to a terminal verdict) → cherry-pick/merge to `main` → append a ledger
entry → `orchestration.md §11` worktree cleanup. One implementer per worktree; ≤3 concurrent.

## 7. What NOT to do

- **Do NOT edit any `dev/design/agent-harness-evaluation-rubric*` file** — the rubric is TERMINAL (v3.1).
  Improving the *rubric* is the closed §5 method; this build improves the *agents/tool*.
- **Do NOT publish / bump versions / tag** — feature build, lands label-only on `main`.
- **Do NOT let the judge use a Claude family**, feed a raw transcript to an LLM, skip §9, blend fallback
  verdicts silently with primary, or auto-apply a proposal without an approval manifest.

## 8. First actions — Slice 5 ($0, no network)

Build `harness/ingest.py` (thin-wrap `parse.py`+`run_detectors.py`+the repo-witness set into a
content-addressed `AuditSubject`) + scorecard assembly + a `FakeJudge`, and **reproduce the 0.8.19
scorecard deterministically from repo artifacts** — per-dimension %MET (A 60.9 · B 91.3 · C 100 · D 95.2 ·
E 75.0 · F 86.7 · G 73.3 · H 86.7) + HARD-gate PASS (`OR-AC-4a`). Wire the `[D]` layer including
`detect_s9_transcript.py` for the B1/B2 sub-check. **DoD:** `OR-AC-1`, `OR-AC-2`, `OR-AC-4a` pass as tests;
`agent-verify` green; a `run` ledger entry recorded. No airlock, no spend until Slice 10.

## Slice ladder (RED tests = the `OR-AC-*` signals in `acceptance.md`)

| Slice | Capability | Lands | Gates on |
|---|---|---|---|
| **5** | `$0` infra: ingest + scorecard + `FakeJudge`; reproduce 0.8.19 | `harness/{ingest,scorecard,fakes}.py` + `run.sh` | OR-AC-1, OR-AC-2, OR-AC-4a |
| **10** | airlock `[L]`/`[H]` judge + protocol rules + budget/fallback | `harness/judge.py` | OR-AC-3, OR-AC-4b, OR-AC-5, **OR-AC-5a** — plus a small priced airlock pilot vs the hand run |
| **15** | proposer + HITL decision package | `harness/propose.py` | OR-AC-6 |
| **20** | HITL-gated apply (fresh worktree, agent-verify) + ledger `apply` | `harness/apply.py` | OR-AC-7, OR-AC-9 |
| **25** | milestone automation + inter-judge κ (anti-Goodhart) | `harness/milestone.py` | OR-AC-8, OR-AC-10 |

Slice 10 requires a HITL cost-projection + `--max-usd` before its priced pilot. Everything before Slice 10
is $0/no-network. Record progress on `dev/plans/runs/STATUS-harness.md` (stand it up at Slice 5) and in the
agent-rubric ledger; witnesses over boards.
