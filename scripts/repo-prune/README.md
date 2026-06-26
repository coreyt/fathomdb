# repo-prune — documentation & memory ledger pruning (mini-project)

Tooling to keep FathomDB's **engineering docs** (`dev/`) and the **agent memory**
(`~/.claude/.../memory`) lean: separate *current state* from *historical ledger*, preserve
experiment results with fidelity, archive or delete the rest, and **measure** the gain. Two
pipelines share one design; each is a two-phase, HITL-gated, snapshot/recover-safe prune with
a machine-checked gate and before/after metrics.

> Status: both prunes executed 2026-06-26. Doc-prune: runs/ 660→168 files, dev/ −53% bytes,
> live `.md` tokens −51%. Memory-prune: 45→35 files, MEMORY.md −18% tokens/session, 0 broken
> links / 0 dead refs. See `measurements/*/DELTA-*.md`.

## Layout

```
scripts/repo-prune/
├── README.md                         ← this file: requirements + design
├── prompts/
│   ├── prune-docs.md                 the doc-ledger prune prompt (for Claude) + epoch stamp
│   └── prune-memory.md               the memory-ledger prune prompt (DRAFT-style; executed)
├── tests/
│   ├── prune-docs-acceptance-tests.md   5 escalating tests the doc prompt must pass
│   └── prune-memory-acceptance-tests.md 5 escalating tests the memory prompt must pass
├── bin/
│   ├── context-clarity.sh            measure dev/ doc-tree clarity (files/tokens/DOC-INDEX/search noise)
│   ├── memory-clarity.sh             measure memory clarity (index tokens/redirects/staleness/links)
│   └── memory-prune-verify.sh        memory invariant GATE (INV-1..5; exit 0 = pass)
├── measurements/
│   ├── context-clarity/{baseline,post}.{json,md} + DELTA-2026-06-26.md
│   └── memory-clarity/{baseline,post}.{json,md}  + DELTA-2026-06-26.md
├── runs/
│   └── doc-prune-CLEANUP-MAP.md       Phase-1 classification map of the first doc prune
└── backups/                          local-only prune snapshots (git-ignored; see Safety)
    └── memory-snapshot-20260626/      pre-prune memory copy (the ONLY recovery path; NOT committed)
```

Durable outputs that live **outside** this project (they are repo state, not tooling):
`dev/experiments-ledger.md` (distilled results of record), `dev/archive/` (relocated historical
docs + manifest), and the per-doc map `dev/DOC-INDEX.md`.

## Requirements

**Functional**
- R1. Classify every doc/memory into exactly one verdict and act on it: doc prune =
  CURRENT / REFERENCE / ARCHIVE / DELETE; memory prune = KEEP / CONSOLIDATE / REPOINT / RETIRE.
- R2. Preserve every experiment result with fidelity — distil into `dev/experiments-ledger.md`
  **before** deleting any raw artifact; numbers verified against source.
- R3. Two phases: Phase 1 emits a classification map and **stops for HITL sign-off**; Phase 2
  executes only after approval.
- R4. Quantify the gain: before/after measurement (not just MB — also DOC-INDEX size, search
  signal-to-noise, memory index tokens/session, redirects, staleness, link health).

**Safety invariants (hard)**
- S1. No source/test code touched (`git status --porcelain src/` clean).
- S2. Reversibility: doc prune uses `git mv`/`git rm` (history recovers). Memory dir is **not
  git** → a full snapshot to `backups/` is mandatory before any memory delete/rewrite (gate INV-5).
- S3. Index integrity at every commit: `dev/DOC-INDEX.md` and `MEMORY.md` stay true 1:1 maps —
  no dangling rows, no unindexed files (the "gate-m" discipline).
- S4. Link integrity: never break a by-path reference (docs) or a `[[wikilink]]` (memory);
  rewrite inbound links in the same change. Accepted ADRs are never edited/moved.
- S5. Locked IDs (`dev/acceptance.md`/`requirements.md`) are never renumbered.

## Design

- **Distill-before-delete.** Build the durable ledger first (read-only subagents return verified
  entries; one writer assembles), then delete raw artifacts whose results it captures.
- **Archive modes.** Relocate to `dev/archive/<release>/` + banner + manifest by default; for
  trees cross-referenced by path (e.g. `dev/plans/prompts/`, ~120 refs) archive **in place** and
  record stale status in the existing index — never a second index.
- **Untracked = HOLD.** Untracked/git-ignored trees (`dev/research/`) are distilled but not
  deleted this pass; deferred decisions recorded in the ledger's Deferred section.
- **Memory specifics.** `MEMORY.md` loads every session → the win is index tokens + correctness,
  not MB. `feedback`/`user` entries are KEEP-always. Findings REPOINT to the ledger; same-topic
  chains CONSOLIDATE; fully-duplicated entries RETIRE. Filenames use dots, `name:` fields use
  dashes, `[[wikilinks]]` are mostly dotted — these are distinct identifiers; do NOT mass-rename.

## Learnings (carried between the two prunes)

- A failed `git rm` + `git add -A` silently **un-deleted** staged deletions and swept in
  pre-existing files → verify the real diff before committing; never `git add -A` over staged deletions.
- Relocation breaks by-path refs — an accepted ADR referenced `dev/progress/` by path, forcing an
  abort of that move. Memory analog: `[[wikilinks]]`. Always scan inbound refs before moving/retiring.
- One regex is not enough — an over-broad pattern deleted the wrong file twice; the gate re-checks.
- A measurement tool that misses a case is worse than none: the memory link-checker's regex
  excluded dotted `[[links]]`; fixed before any retirement.

## How to run

```sh
# Measure (read-only, before/after a prune):
scripts/repo-prune/bin/context-clarity.sh baseline      # dev/ doc tree
scripts/repo-prune/bin/memory-clarity.sh   baseline      # agent memory
#   …prune… then:  …context-clarity.sh post / …memory-clarity.sh post ; diff the JSON.

# Memory prune gate (run before = audit debt, after = must exit 0):
scripts/repo-prune/bin/memory-prune-verify.sh
# during a memory prune, enforce the snapshot:
MEMORY_PRUNE_ACTIVE=1 MEMORY_SNAPSHOT=scripts/repo-prune/backups/memory-snapshot-<ts> \
  scripts/repo-prune/bin/memory-prune-verify.sh
```

Scripts resolve the repo root via `git rev-parse`, so they run from anywhere. Override the
memory dir with `CLAUDE_MEMORY_DIR`. Token counts are estimates (`ceil(bytes/4)`), constant
across runs so deltas are valid.

## Safety / backups

`backups/memory-snapshot-20260626/` is the **only** recovery path for the executed memory
prune (the memory dir is not under git). It is **local-only / git-ignored** (`backups/.gitignore`)
— snapshots are never committed. Keep it on disk until you're confident in the pruned state, then
delete it. Future snapshots go here too (timestamped, also ignored). The doc prune is fully
recoverable from git history at/below baseline `25541d88`.
