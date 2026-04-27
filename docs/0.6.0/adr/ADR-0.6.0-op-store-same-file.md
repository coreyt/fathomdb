---
title: ADR-0.6.0-op-store-same-file
date: 2026-04-25
target_release: 0.6.0
desc: Operational store lives in the same sqlite file as primary entities; no dual-store
blast_radius: design/engine.md operational store section; dev/design-add-operational-store-feature.md (folded); docs/concepts/operational-store.md (folded); design-operational-payload-schema-validation.md (folded); design-operational-secondary-indexes.md (folded)
status: accepted
---

# ADR-0.6.0 — Op-store in same sqlite file

**Status:** accepted (HITL 2026-04-25, decision-recording)

## Context

0.5.x carried four design documents proposing an "operational store" — a
logical store for application state distinct from primary entities
(nodes, edges, chunks). Critic-A F8 flagged the cluster as four docs
riding one undecided architectural ADR; the question was whether 0.6.0
should:

(a) ship op-store as a separate logical store / file,
(b) drop op-store from 0.6.0 core entirely, or
(c) defer to Phase 2.

## Decision

**No dual-store. FathomDB operational-store needs live in the same
sqlite file as primary entities. Clients keep their own storage for
whatever else they need.**

The four folded docs (op-store feature, op-store concept, payload schema
validation, secondary indexes) all describe primitives that survive as
**sections of the same file's logical surface** — they are not a
separate store, separate database, or separate file.

## Options considered

**A. Same sqlite file (chosen).** Pros: single-file invariant preserved
(matches `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17); single
backup target; transactional consistency between primary entities and
op-store rows; one schema migration story. Cons: schema namespace must
keep op-store tables distinct from primary tables (already the case in
the 0.5.x folded design).

**B. Separate sqlite file alongside primary file.** Pros: cleaner
isolation. Cons: breaks single-file invariant; doubles backup/restore
surface; cross-file transactions are not real → consistency story
weakens; `safe_export` becomes a manifest of two files.

**C. Drop op-store from 0.6.0; clients persist application state
themselves.** Pros: smallest engine surface. Cons: every agentic client
re-implements the same primitives (run/step/action provenance, opt-in
schema validation, bounded secondary indexes); the rewrite proposal's
"thin-plus" thesis includes operational primitives because they are
load-bearing for agentic workflows.

## Consequences

- `dev/design-add-operational-store-feature.md` folds into
  `design/engine.md` (operational-store section) — same-file constraint.
- `docs/concepts/operational-store.md` folds same place.
- `dev/design-operational-payload-schema-validation.md` folds as
  engine-design input (opt-in payload validation contract).
- `dev/design-operational-secondary-indexes.md` folds as engine-design
  input (bounded secondary-index contract).
- Clients are free to keep their own state outside the FathomDB file.
  FathomDB does not document, depend on, or reach into client storage.

## Citations

- HITL decision 2026-04-25 (critic-A F8 cluster resolution).
- `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17 (single-file
  invariant).
- Folded inputs: dev/design-add-operational-store-feature.md;
  docs/concepts/operational-store.md;
  dev/design-operational-payload-schema-validation.md;
  dev/design-operational-secondary-indexes.md.
