# Dev Docs Index

`dev/` is the internal engineering corpus for FathomDB. Keep requirements,
architecture, ADRs, subsystem designs, interface contracts, planning notes, and
other team-working material here.

Do not keep public product guides, public API reference prose, or MkDocs source
content here. Those belong in `docs/`.

## Where the live work is

Read these first — this is where the current release actually lives.

- `plans/` release planning and execution. Contains:
  - `plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` — **master schedule-of-record**
    for the 0.8.x line (despite the filename, it sequences through 0.8.24)
  - `plans/runs/` — per-release boards and run results;
    `plans/runs/STATUS-0.8.20.md` is the **live board**
  - `plans/release-state-<version>.json` — machine state for the release; one
    writer only
  - `plans/prompts/` — commission prompts and hand-offs, incl.
    `plans/prompts/0.8.x-STEWARD-HANDOFF.md`
- `steward/` Program Steward's append-only decision ledger and its discipline
- `agent-tools/` executables agents invoke directly —
  `agent-tools/ledgerwrite`, `agent-tools/ledgerwatch`,
  `agent-tools/codex-nostdin.sh`

Agent operating rules are not in `dev/` — they are in `AGENTS.md` at the repo
root.

## Engineering reference

- `adr/` architectural decisions
- `design/` subsystem design docs
- `interfaces/` internal interface contracts
- `interface-inventory/` interface inventory artifacts
- `roadmap/` implementation sequencing and planning
- `release/` internal release process, checklists, and release-gate fixtures
- `deps/` dependency audit and evaluation material
- `doc-index/` long-form per-doc notes backing `DOC-INDEX.md`
- `agents/` agent-oriented working prompts and outputs
- `notes/` supporting notes that are not canonical on their own
- `templates/` document templates
- `tools/` dev tooling (markdown guards, mermaid, ONNX export)
- `scripts/` one-off analysis scripts

## Evaluation and measurement

- `experiments/` evaluation and experiment code
- `research/` literature, competitor, and market research (large; mostly
  fetched material)
- `corpus-creation/` how evaluation corpora are built
- `corpus-survey/` survey and map of available corpora
- `perf-history/` performance baselines as JSON, keyed by AC and commit
- `profiling/` per-subsystem profiling write-ups
- `acceptance-rowsets/` frozen rowsets backing specific acceptance criteria
- `turso-watch/` competitive watch on SQLite/Turso divergence

## Historical / low-traffic

- `progress/` **historical — frozen at 0.6.x.** Superseded by `plans/runs/`
- `releases/` early per-release notes (0.8.0 only); current release material is
  in `plans/`
- `prototypes/` throwaway prototypes kept for reference
- `archive/` retired material, incl. `archive/hitl-queue.md` (resolved Tier 1–4
  HITL queue)

## Top-level canonical docs

- `DOC-INDEX.md` — full path → purpose map of the doc tree; every slice updates it
- `needs.md`
- `requirements.md`
- `acceptance.md`
- `architecture.md`
- `test-plan.md`
- `traceability.md`
- `security-review.md`
- `learnings.md`
- `experiments-ledger.md`
