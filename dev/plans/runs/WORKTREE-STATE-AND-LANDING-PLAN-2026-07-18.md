# Worktree state + landing plan — 2026-07-18

> Snapshot captured from `git` (fetched), **not** memory. `origin/main` = `d526d15c` (unchanged since
> 2026-07-11). Purpose: map every worktree/branch, and give a toe-safe sequence to land the 0.8.20 plan work.

## 1. Source of truth

- **`origin/main` = `d526d15c`** — "docs(steward): HITL decisions — guardrail ADOPTED (TC-RUBRIC-5) + 0.8.20
  erasure Slice-0 inputs". Every non-experiment branch below is already merged into it (ahead=0).

## 2. Worktrees

| Worktree | Branch | Head | Uncommitted | State / purpose |
|---|---|---|---|---|
| `fathomdb/` (**primary**) | detached | `d4b5cd90` | **15 files** | ⚠ **Contended, detached, contaminated.** Holds a MIX of two workstreams (see §4). This is the anti-pattern TC-RUBRIC-5 was adopted to end. Do **not** land from here. |
| `fathomdb-worktrees/0.8.20-plan` | `plan/0.8.20-reconcile` | `d526d15c` | **9 files** | ✅ **THE 0.8.20 deliverable.** Cut clean off `origin/main`. Docs/plans only. Ready to commit. |
| `fathomdb-worktrees/main` | `main` | `ed2419be` | clean | Local `main`, **behind origin/main by 1** — fast-forward it. |
| `fathomdb-worktrees/steward-main` | `steward-docs-0710` | `d526d15c` | clean | == `origin/main` (already merged). **Prunable.** |
| `fathomdb-worktrees/rubric-eval-v3-terminal` | `rubric-eval-v3-terminal` | `c063f699` | clean | Merged (behind 25). **Prunable.** |
| `fathomdb-worktrees/0.8.11.2` | `docs/0.8.x-renumber-reconcile` | `20f53ffb` | 1 untracked | Merged (behind 225). 1 stray file (`0.8.x-renumber-memex-handoff.md`). **Prunable after rescuing the stray.** |
| `fathomdb-worktrees/0.5.1-memex-build` | detached | `1137c572` | clean | Memex-side build sandbox — **separate concern, leave alone.** |

## 3. Branches carrying unmerged work

- **`plan/0.8.20-reconcile`** — my deliverable; commits pending (§5). ahead=0 only because not yet committed.
- **`exp/subagent-persistence`** — 12 unique commits, 390 behind. Parked experiment; **out of scope**, do not land.
- Everything else (`docs/0.8.x-renumber-reconcile`, `rubric-eval-v3-terminal`, `rubric-h7-0.8.20-fold`,
  `steward-docs-0710`, `0.8.11.2-pico-umbrella`) is **ahead=0 → already in `origin/main`**.
- ~40 `origin/dependabot/*` branches exist untracked — the deferred dependency queue (napi-3, rusqlite-0.40,
  sqlite-vec-0.1.9, etc.). These are **0.8.22** work (F-19/F-20), not now.

## 4. The primary checkout's 15 uncommitted files — ownership

**Mine (0.8.20 / TC-11 workstream) — all REDUNDANT with `plan/0.8.20-reconcile`:**
- `dev/requirements.md`, `record-lifecycle-protocol/{README,api-surface,structural-lifecycle-contract}.md`
  (tracked, M) — **byte-identical** to the worktree copies (verified). Superseded.
- `0.8.20-erasure-and-h-end-state-v4.md` — the design of record; also on the worktree.
- `0.8.20-erasure-and-h-end-state-v3.md`, `0.8.20-tc11-...-v2.md`, `0.8.20-tc11-...axis.md` — **superseded
  drafts**, stamped ⛔ SUPERSEDED. Review-trail only.
- 3 codex logs (`0.8.20-*erasure*`) — my review trail.

**NOT mine (rubric / failure-mode workstream) — do NOT sweep into a 0.8.20 commit:**
- `agent-harness-evaluation-rubric-v3.md`, `agent-rubric-ledger.jsonl` — **already committed in `origin/main`**;
  the working copies are redundant.
- `opus-claude-failure-modes-2026-07-11.md` — **genuinely uncommitted, NOT in origin/main.** Belongs to the
  failure-mode/rubric workstream. Leave for its owner.
- `failure-modes-rubric-hardening-codex-consult-*.log` — same workstream.

**Net:** nothing unique-and-valuable is trapped in the primary checkout except the failure-mode workstream's
artifacts, which are not mine to land.

## 5. Landing sequence (toe-safe) — CAN be done cleanly

The 0.8.20 work is **fully isolated** on `plan/0.8.20-reconcile` off `origin/main`, is **docs/plans-only** (no
code), and the only "contention" is my own duplicate copies in the primary checkout. No other live workstream
touches these files. So yes — this lands without stepping on anyone.

1. **Fast-forward local `main`.** In `fathomdb-worktrees/main`: `git pull --ff-only` → `main` = `d526d15c`.
2. **Commit the deliverable** on `plan/0.8.20-reconcile` (9 files): `git add -A && git commit`. Suggested subject:
   `docs(0.8.20): de-stale + author plan-0.8.20 (OPP-12 Phase-2 + erasure); TC-11 pin ratified; F-25`.
3. **Land to main.** Docs-only + `main` not branch-protected (CI advisory for docs) → PR then merge, or a
   direct `--ff-only` merge from the worktree. Either is fine; PR preferred for the audit trail.
4. **Decontaminate the primary checkout** — carefully, preserving the non-mine files:
   - `git checkout -- dev/requirements.md dev/design/record-lifecycle-protocol/` (discard my redundant tracked edits).
   - Delete my redundant untracked drafts/logs (`0.8.20-*` designs + codex logs) — they are safely on the branch/worktree.
   - **Leave** `opus-claude-failure-modes-2026-07-11.md` + the failure-modes log + the two redundant rubric files
     for their owner to reconcile. Do not touch.
   - Re-attach the primary to a branch (`git switch main`) so it stops being a detached shared checkout.
5. **Prune merged worktrees/branches** (housekeeping, optional): `steward-main`, `rubric-eval-v3-terminal`, and
   the `0.8.11.2` renumber worktree — after rescuing `0.8.x-renumber-memex-handoff.md` from the last.
6. **Do NOT touch:** `exp/subagent-persistence` (parked experiment), `0.5.1-memex-build` (memex sandbox), the
   `dependabot/*` queue (0.8.22).

## 6. Open items that survive the landing (not blockers)

- **OPP-12 sub-ledger seq 10** (Memex notification of the §2(ii) overrule) — drafted, **not appended**; needs
  HITL content approval + `ledgerwrite` tooling (absent this session). Do this on a fresh full-tail read against
  `origin/main` — the base was `.seq`=9 there, so the next append is **seq 10**, not 9.
- **Publish** of 0.8.20 is a separate per-`x.y.z` HITL gate (F-21). Plan authoring ≠ publish.
