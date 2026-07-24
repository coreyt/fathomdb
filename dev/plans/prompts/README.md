# `dev/plans/prompts/` — execution-prompt archive

This directory holds the **execution prompts** used to drive per-slice / per-packet /
per-release implementation work across every FathomDB release line (0.6.0 → 0.8.20+).
As of DOC-HYGIENE-1 (2026-07-24) it holds **~216 files / ~2.8 MB**.

## Archive-in-place convention (and why)

Prompts are **archived in place, never relocated or renamed**, per the standing rule in
`dev/plans/README.md` § "Archive convention": completed prompts are cross-referenced
**by path** from ADRs, design docs, implementation docs, `dev/DOC-INDEX.md`, and prior
run logs — roughly **120 distinct prompt paths are referenced from ~140 files**.
Physically moving a completed prompt would break those references or force rewrites of
immutable historical run logs (`dev/plans/runs/*`). A prompt's status lives in its
release's `STATUS-*.md` / `*-implementation.md` ledger, not in its filesystem location.

**Do not `git mv` or rename anything in this directory.** If a path genuinely must
change, update every inbound reference in the same change and leave a note in the
ledger — this is the rare exception, not the normal path.

## What's actually in here

The great majority of this directory — well over 90% of the ~216 files — is
**closed-line, one-shot execution prompts**: per-slice prompts (`0.8.0-slice-*.md`),
per-packet/per-fix handoffs (`0.7.0-PVQ-P1-DESIGN-fix-1.md`), and per-release launch/GA
prompts (`0.6.1-GA.md`, `0.7.0-RC1.md`) for release lines that have **already shipped or
closed**. Each was written to be read exactly once, by the subagent it was addressed to,
for a slice/packet that is now done. Reading them at cold-start is almost never useful —
treat them as historical record, not as live guidance (see the staleness index in
`dev/plans/README.md`).

## Live prompts an agent should actually read

Only a handful of files here are **live, generic entry points** meant to be read by a
new session rather than archived history:

| Prompt | Role |
|--------|------|
| `0.8.x-STEWARD-HANDOFF.md` | Program Steward hand-off — role/mandate (canonical, cross-release). |
| `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` | Release Orchestrator hand-off — per-release entry point (sibling role, not the Steward). |
| `DOC-HYGIENE-1-HANDOFF.md` | This cross-cutting docs/tooling-hygiene effort's own commissioning brief. |
| `LIBRARY-BUMP-STEWARD.md` | Library Bump Steward — recurring dependency-sweep role brief. |
| `LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md` | Per-sweep Library Bump Orchestrator template. |
| `PLAN-TEMPLATE.md` | Per-release plan-authoring template (fill-in skeleton for a new `plan-<release>.md`). |

Everything else in this directory is closed-line and archived-in-place. For the current
program state, read `dev/DOC-INDEX.md` and the live `dev/plans/runs/STATUS-0.8.20.md`
board, not a scan of this directory.
