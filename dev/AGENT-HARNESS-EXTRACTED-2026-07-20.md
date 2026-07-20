# NOTICE — the agent-harness / rubric work was extracted to `~/projects/agent-aware` (2026-07-20)

**Read this if you are working on `fathomdb` `main` and expected to find the agent-harness
evaluation rubric, the operational audit harness, or the rubric stress-test experiment.**

They are no longer in this repo. The entire agent-harness / rubric scope was **split out into its
own standalone project** at `~/projects/agent-aware`, with **full git history preserved**
(`git filter-repo` from `fathomdb` `origin/main` `c82feb80`). This was an intentional,
HITL-directed extraction — not a loss.

## What moved (removed from fathomdb in this commit)

| Old fathomdb path | New home in `agent-aware` |
| --- | --- |
| `dev/agentic-rubric/` | `harness/` |
| `dev/experiments/rubric-stress-test/` | `stress-test/` |
| `dev/design/agent-harness-evaluation-rubric*.md` | `rubric/rubric-*.md` |
| `dev/design/rubric-audit-*.md`, `rubric-run-0.8.19-2026-07-10.md` | `rubric/` |
| `dev/steward/agent-rubric-ledger.jsonl(.seq)` | `ledger/` |

## What did NOT move (still here, unchanged)

Shared orchestration tooling was **copied** into agent-aware (it lives in both repos):
`dev/agent-tools/{ledgerwrite,ledgerwatch,codex-nostdin.sh}`, `scripts/{agent-verify,preflight}.sh`,
`.claude/agents/*`, `dev/design/orchestration.md`,
`dev/design/dedicated-checkout-per-orchestration-guard-proposal.md`,
`dev/agent-harness-bootstrap-prompt.md`, `dev/steward/steward-ledger.jsonl`. Fathomdb's own release
orchestration continues to use these here.

## Dangling references are intentional

~14 fathomdb docs (release plans, steward hand-offs, ledgers, `TC-RUBRIC-N` items) still *mention*
the moved paths. Those are historical ledger/plan entries and are **deliberately not rewritten** —
rewriting the historical record to erase a real path would itself be a drift/laundering hazard. Treat
any such reference as a pointer to `~/projects/agent-aware`.

## How to get to a clean git state

1. **Fetch and re-baseline your branch/worktree onto the new `main`:**

   ```sh
   git fetch origin
   git rebase origin/main        # or: git merge origin/main
   ```

   The removed files simply disappear. That is expected — do not treat their absence as damage.
2. **If you have local edits to any removed path** (a rebase/merge conflict on a deleted file):
   the work belongs in `~/projects/agent-aware` now. **Accept the deletion here**
   (`git rm <path>` to resolve), and re-apply your change in the agent-aware repo instead.
   Do **not** re-add the removed paths to fathomdb.
3. **If you were mid-slice building the harness** (the paused `dev/agentic-rubric/ORCHESTRATOR-HANDOFF.md`
   Slice 5–25 ladder): that build now happens in `~/projects/agent-aware` (`harness/ORCHESTRATOR-HANDOFF.md`),
   not here.
4. **Do not resurrect** `dev/agentic-rubric/`, `dev/experiments/rubric-stress-test/`, or the moved
   `dev/design/*rubric*` files in fathomdb. If something here truly needs one, copy it read-only from
   agent-aware and keep it gitignored, or reference the agent-aware path.

Questions about provenance: `agent-aware` `README.md` records the extraction sha and method.
