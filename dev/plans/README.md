# Plans

`dev/plans/` is the home for **per-release implementation planning and
execution artifacts** across every shipped and in-flight FathomDB release line
(0.6.0 → 0.6.1 → 0.7.0 → 0.7.1 → 0.7.2 → 0.8.x). It is an **append-only
historical record**, not a workspace that gets pruned per release.

## Layout

- `<version>-implementation.md` — per-release single source of truth for the
  implementation sequence, execution posture, and phase/pack status
  (`0.6.0-implementation.md`, `0.7.0-implementation.md`,
  `0.7.1-implementation.md`, …).
- `../progress/<version>.md` — chronological work log per release line.
- `prompts/` — execution prompts used for slice/packet work, one file per
  slice (e.g. `0.7.2-PR-8-campaign-closure.md`).
- `runs/` — slice logs, reviewer notes, and status boards
  (e.g. `STATUS-0.8.4.md`, `0.8.4-gating-rerun-RESULT.md`). Distilled experiment
  results of record now live in `dev/experiments-ledger.md`; transient per-run
  artifacts (raw `*-output.json`, codex `*-review-*` logs, `.log`) have been pruned
  and are recoverable from git history.

Directory split:

- `dev/plans/` = release execution planning + run artifacts (all versions).
- `dev/roadmap/` = forward-looking, not-yet-scheduled backlog and deferrals.

## Archive convention (in place — do NOT relocate)

Completed-release prompts and run artifacts are **archived in place**, not moved
to a subdirectory. This is deliberate: artifacts are cross-referenced **by path**
from ADRs, design docs, implementation docs, and prior run logs (≈120 distinct
prompt paths are referenced from ≈140 files as of 0.7.2). Physically relocating
a completed prompt would break those references or force rewrites of immutable
historical run logs. Instead:

- A prompt/run artifact is "archived" when its release line has shipped; its
  status lives in that release's `STATUS-*.md` / `*-implementation.md` ledger,
  not in its filesystem location.
- The active to-do surface is always the current campaign's `STATUS-*.md`
  scoreboard (e.g. `runs/STATUS-release-hardening.md` for 0.7.2), not a scan of
  `prompts/`.
- Do not delete or move shipped-release artifacts. If a path must change, update
  every inbound reference in the same change and leave a note in the ledger.

## Rules

- Active source-of-truth and in-flight slice artifacts: keep in this directory.
- Completed-release prompts and run logs: keep in place (see archive convention).
- Future-release backlog or deliberate deferral beyond the active line:
  `dev/roadmap/`.
