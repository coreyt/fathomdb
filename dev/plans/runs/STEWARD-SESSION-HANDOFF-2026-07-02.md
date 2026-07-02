# FathomDB — Steward Session Handoff (2026-07-02)

> **Boot:** run **`/steward`** (now a real command — reads `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` for the role),
> then read THIS doc for current state, then return an orientation paragraph and **WAIT for HITL** before mutating.
> You are the **Primary Development Steward (PDS)**: keep the schedule-of-record true to git, place cross-cutting
> work, commission + verify orchestrators/implementers, propose-first to HITL. **Do not implement code or hand-drive
> a release ladder** — delegate; spend your context on judgment + verify-from-git.

## ⚠️ Git state — read first
- **Local `main` = `6176055a`, 12 commits AHEAD of `origin/main` (`20f53ffb`), UNPUSHED.** Two local-only merges landed this session: `corpus/crosssource-acquire` (ff, eval/corpus infra) and `tooling/steward-orchestration-port` (merge commit). **Decision owed: push `main` to origin, or keep local-only?** Until pushed, worktrees cut from `origin/main` won't see these 12 commits.
- Main checkout `/home/coreyt/projects/fathomdb` is on `main`, clean. This is the integration point (where `main` lives) — merges INTO main happen here, not in a worktree.

## Primary in-flight: 0.8.12 (Memory-quality) — PAUSED at a finish plan
- Branch **`0.8.12-memory-quality` @ `8a2a1006`** (worktree `fathomdb-worktrees/0.8.12`), **NOT merged to main; label-only; manifests stay `0.8.9`.** All agents stood down.
- Done: Slices **0/5/15/20 CLOSED** (codex §9); Slice 40 PARTIAL. Consolidation provider (#7) built, value-test verdict **STAY-OFF/opt-in** (+ named default-ON blocker `t_invalid` FTS/vec filter). OPP-6 census discharged (entity solved; gap = edges 0.227).
- **Resume from `dev/plans/plan-0.8.12-finish.md`** (HITL-approved): Phase 1 = EXP-COV-1 **GPU verdict re-run** ($0, reuses the preserved extraction) → Phase 2 record verdict (productization decided ONLY after Phase 5) → Phase 3 `t_invalid` fix + live TS X1 → Phase 4 Slice-40 DoD → Phase 5 label-only merge to main.
- **EXP-COV-1 asset preserved:** `$4.79` extraction, 272/272, on branch `0.8.12-expcov1-sweep` (`6daf2d94`); cache gitignored in the MAIN tree (`data/corpus-data/eval-cache/exp-cov1/…`), sha256 verified against committed manifest. GPU downstream **replan** is written (`runs/EXP-COV-1-downstream-GPU-replan.md`) and PDS-endorsed with **3 required additions**: (1) GPU-alloc hygiene (3090 cuda:0/1, exclude K620, vLLM/GPU mutex); (2) build FROM `0.8.14-gpu-rerank` branch (`embed_batch_cls`/`rerank-cuda` exist); (3) same-stack C-none re-run (degraded 0.468 is a prior point only).
- Env-block finding: CPU-embedder `.so` defect blocks dense/CE eval — a GPU build is the unblock (`dev/notes/0.8.12-cpu-embedder-defect-blocks-dense-eval.md`).

## Worktrees + cleanup
- `fathomdb-worktrees/tooling-steward-port` (`tooling/steward-orchestration-port`) — **mine, merged, executor stood down → safe to remove** (`git worktree remove` + `git branch -d`).
- `fathomdb-worktrees/corpus-crosssource` (`corpus/crosssource-acquire`) — **another session's; merged**; remove only once HITL confirms that session is done.
- `fathomdb-worktrees/0.8.12` (`8a2a1006`) + `0.8.12-expcov1` (`6daf2d94`) — keep (0.8.12 resume).
- `0.8.12-gpu-rerank` (`d9e61c66` on branch `0.8.14-gpu-rerank`) — parked/green, folds into 0.8.14. `0.5.1-memex-build` (`1137c572`) — memex local build. `0.8.11.2` (`docs/0.8.x-renumber-reconcile`) — stale, cleanup candidate.

## Parked backlog (Steward-tracked)
V-7 (OPP-3) **held** · OPP-1 adoption tail → **Memex 0.5.5** · GPU-rerank refile/merge → **0.8.14** (#20) · #29 vector-arm Mean-vs-CLS · deferred fixes (#24 R7, #25 m1 pin, #26 actionlint, corpus_graph skip) · roadmap markers (0.8.14 EXP-S · 0.8.15 dispatcher · 0.8.16 F9/ONNX · 0.8.18 vec-equiv/GA · 0.8.20 HNSW+majors · ≥0.9.x multi-field FTS) · **publish + Memex push (HITL hard-stops)**.

## Open HITL decisions
1. **Push local `main`** (12 ahead) to origin, or hold local-only?
2. **Resume 0.8.12** now (Phase 1 GPU re-run) — needs a GPU-embedder build; spawn a fresh orchestrator/session.
3. Clean up the merged worktrees (tooling now; corpus on confirm).

## Standing constraints
Label-only (manifests `0.8.9`, no tag/publish unless a HITL publishable `x.y.z` cut) · **fathomdb-only push, NEVER memex** (per-push directive required each time) · `13` forbidden as minor+micro · two-tier numbering (`x.y.z` real / `x.y.z.p` pico) · **V-7 held** · don't rewrite history · **verify from git before narrating** · codex §9 is the review gate · **one writer per worktree** (never 2 file-mutating agents in one checkout) · **background-agent SPEND needs the USER's OWN direct message, not a PDS relay** — pre-authorize spend envelopes in the spawn prompt.

## New tooling (use it)
`/steward`, `/orchestrate`, `/orch` are live. **Use the steward ledger** to keep context O(delta): `python dev/agent-tools/ledgerwrite/ledgerwrite.py dev/steward/steward-ledger.jsonl --kind decision --summary "…"` to append; `python dev/agent-tools/ledgerwatch/ledgerwatch.py dev/steward/steward-ledger.jsonl` to read deltas — instead of re-reading/re-narrating state.

## Memory pointers
`[[steward-orchestration-tooling-ported-from-memex]]` · `[[0.8.x-release-numbering-publish-governance-policy]]` (renumber) · `[[0.8.11.2-launched-autonomous-envelope]]` (0.8.12 lineage) · `[[opp1-v3-decomposition-verdict-adopt-hold-0.5.5]]` · `[[push-scope-fathomdb-only]]` · `[[orchestration-execution-traps]]` (incl. #8 spend-relay) · `[[steward-delegate-dont-hand-do]]` · `[[gpu-not-retrieval-lever-hnsw-at-0.8.20]]`.
