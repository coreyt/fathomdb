# Tooling-port reconciliation — the convergence map

> Proves the ported tooling is **not a parallel system**: every new agent def and
> slash command points at, and encodes, FathomDB's OWN existing prose. No memex
> domain content, no memex `dev/steward/*` text, no OPP/LEVERAGE vocabulary. The
> SHAPE of the agent defs and commands is modeled on memex's harness; every line
> of *content* is rewritten to FathomDB's docs and rules.

## New files → the FathomDB doc/rule each converges with

| New tooling file | Converges with (FathomDB source of truth) | What it encodes / points at |
|---|---|---|
| `.claude/agents/steward.md` | `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` | Program-Steward role: SoR fidelity, drift detection, cross-cutting placement, commission-and-verify, propose-first HITL interface, report format §10. Durable contract behind the `/steward` command. |
| `.claude/agents/orchestrator.md` | `dev/design/orchestration.md` + `dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` | Release-orchestrator role: three-role separation (§1), state spine (§1.5), preflight gate (§1.6), implementer spawn (§2), codex reviewer (§3), decision loop (§9), worktree cleanup (§11); §0 hard preflight (branch + worktree-base). |
| `.claude/agents/implementer.md` | `dev/design/orchestration.md` §2 / §8 | Existing local def, now tracked. Single-slice implementer in a main-thread-owned worktree; no Agent/Task; writes `output.json` (§8 schema). Copied unchanged from the main checkout. |
| `.claude/commands/steward.md` | `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` | Thin launcher → the steward hand-off (§3 cold-start). Reminds of the load-bearing rules; waits for HITL orientation ack. |
| `.claude/commands/orchestrate.md` | `dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` + `dev/design/orchestration.md` | Thin launcher → the release-orchestrator hand-off + the stable method. Notes the built-in `/goal complete 0.8.z` points at the same contract. |
| `.claude/commands/orch.md` | `.claude/commands/orchestrate.md` | Alias → `/orchestrate`. No independent content. |
| `dev/steward/README.md` | `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` §6/§7 + `dev/design/orchestration.md` §12.1 | The ledger discipline (append via `ledgerwrite`, read deltas via `ledgerwatch`, O(delta) context, name-the-decider, trust-git). |
| `dev/steward/steward-ledger.jsonl` | `dev/steward/README.md` | The append-only decision ledger (seeded with one bootstrap entry via `ledgerwrite`). |
| `dev/agent-tools/ledgerwrite/` | (domain-agnostic tool) | Append-only JSONL writer with monotonic `seq`. Copied verbatim from memex; confirmed no domain strings. Serves the ledger discipline above. |
| `dev/agent-tools/ledgerwatch/` | (domain-agnostic tool) | Delta reader with a saved cursor. Copied verbatim from memex; confirmed no domain strings. |

## Rules encoded in the agent defs → their FathomDB origin

| Rule | FathomDB origin |
|---|---|
| verify-from-git (trust git over narration) | STEWARD-HANDOFF §6; ORCHESTRATOR-HANDOFF §6; orchestration.md §1.5 (witnesses win over boards) |
| one-writer-per-worktree / one writer per checkout | orchestration.md §10 rule 10; §13 (shared-checkout incident 2026-06-29) |
| canary-first / bounded-parallel (max 3) | orchestration.md §1.6, §10; ORCHESTRATOR-HANDOFF §6 |
| no source edits on `main` (agent types omit Edit/Write) | orchestration.md §1, §10 rule 4; the `steward`/`orchestrator` agent types omit Edit/Write |
| propose-first / HITL gates | STEWARD-HANDOFF §5 decision-rights table, §6; ORCHESTRATOR-HANDOFF §1 |
| two-tier numbering (`x.y.z` real vs `x.y.z.p` pico) + `13` forbidden + label-only picos | memory `0.8.x-release-numbering-publish-governance-policy` (HITL 2026-06-29) |
| push-scope fathomdb-only (never memex) | memory `push-scope-fathomdb-only` (HITL 2026-06-29) |
| codex §9 as the review gate | orchestration.md §3, §9; ORCHESTRATOR-HANDOFF §6; memory `orchestration-execution-traps` |
| worktree-base / stale-base preflight | orchestration.md §1.6; ORCHESTRATOR-HANDOFF §0; memory `agent-worktree-stale-base-trap` |
| MAIN-tree-only maturin/GPU builds | orchestration.md §1.6; ORCHESTRATOR-HANDOFF §0/§6 |

## Tier-3 convergence (skip/defer decisions)

| Item | Decision | Rationale (converge, don't duplicate) |
|---|---|---|
| `orchestrator-guard.sh` (blanket on-`main` source-edit block) | **Skip** | The active `wake guard-check` PreToolUse hook (wired in `.claude/settings.json` on `Edit` + `Write`) already covers recorded on-`main` edit constraints. Verified present in `.claude/settings.json`. |
| Session hooks / settings | **Skip** | Already present in `.claude/settings.json` (PostCompact / Pre/PostToolUse / PreCompact `wake` hooks). |
| `archive_ledger_items.py` | **Defer** | Not needed at seed; the ledger is one bootstrap entry. Revisit when the ledger grows. |
| `preflight.sh` | **Already covered — not duplicated** | `scripts/preflight.sh` exists and is comprehensive (stale-base guard, dependency-CLOSED, mid-operation-repo, disk headroom, build-isolation reminder). `/orchestrate` + `orchestrator.md` wire to it directly. |
| `codex-review.sh` wrapper | **Not added — converge on the documented inline flow** | FathomDB's convention is the inline codex §9 invocation documented in orchestration.md §3 and ORCHESTRATOR-HANDOFF §6 (`codex exec review --dangerously-bypass-approvals-and-sandbox`; `/code-review` fallback). A wrapper script would be a new parallel surface; `/orchestrate` points at the existing documented flow instead. |
| `agent-permission-canary.sh` | **Not added — converge on the documented canary discipline** | FathomDB's method documents "canary first" as an orchestrator discipline (orchestration.md §1.6/§10; ORCHESTRATOR-HANDOFF §6), not a standalone script. Adding a memex-shaped canary script would introduce a parallel surface with no FathomDB doc behind it. `orchestrator.md` encodes the canary-first rule instead. |
