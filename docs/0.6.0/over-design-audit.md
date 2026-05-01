---
title: "0.6.0 Over-Design Audit"
status: draft
date: 2026-04-30
scope: ADRs + architecture.md + requirements.md + acceptance.md + design/bindings.md + interfaces/*
method: 6-wave architecture-inspector audit, falsification-test per finding
precedent: ENG6 (OpenReport _ns siblings — DELETE, see hitl-queue.md)
adjudicated_skip: ENG6, ENG3, E3, E5, E7, X4, X6
---

# 0.6.0 Over-Design Audit

Revised read-only audit of the 0.6.0 design corpus for two distinct failure
classes:

1. **Over-design** — surface area added on speculative future need with no
   current user-visible forcing function.
2. **Incomplete / contradictory contract** — required 0.6.0 surface whose
   semantics are missing, stub-cited, or contradicted across docs.

The revision below was checked against FathomDB's stated purpose as an
**embedded SQLite datastore for persistent AI agents** with graph, vector,
FTS, provenance, and operational-state capabilities. Several original ODs were
valid as "extra surface" catches; several others were misclassified because
they targeted core product surface rather than speculative extensions.

Note: the first bucket of contract-fix work (CLI, cursor semantics, missing
design-owner files) has since been applied in the corpus. A second bucket has
also now been applied for OD-03, OD-04, OD-07, OD-10, OD-11, OD-14, OD-15,
OD-16, and OD-20. A third bucket is now applied for OD-06, OD-08, OD-09,
OD-19, OD-23, OD-24, OD-29, and OD-30. A fourth bucket is now applied for
OD-01 and OD-02. Findings in those buckets are retained here as audit history,
not as a claim that the current tree still exhibits every listed contradiction.
OD-15 is complete for 0.6.0 by removal and is now tracked as a 0.8.0 deferral.

## Summary table

| ID | Title | Revised verdict | Action |
|---|---|---|---|
| OD-01 | scheduler `tasks_in_flight` orphan metric | acted: orphan metric removed from scheduler ADR | **DONE** |
| OD-02 | bindings.md § 8 winston/pino default adapter | acted: brand-specific logger adapter clause removed | **DONE** |
| OD-03 | op-store `op_kind` 4-variant enum (`put/delete/increment`) | acted: narrowed to append-only op-store mutation verb | **DONE** |
| OD-04 | op-store `latest_state` kind + `operational_current` table | acted: keep `latest_state`, drop derived `operational_current` | **DONE** |
| OD-05 | `scheduler_runtime_threads` binding knob | not over-design | **LEAVE** |
| OD-06 | `PreparedWrite::AdminSchema` provisional variant | acted: provenance and locked-vs-provisional status documented | **DONE** |
| OD-07 | op-store `disabled_at` column | acted: removed from accepted op-store schema | **DONE** |
| OD-08 | binding knob-symmetry (CLI included) inconsistency | acted: SDK symmetry, CLI boundary, and binding-runtime exception boundary documented | **DONE** |
| OD-09 | error taxonomy 9-module decomposition (5 unbacked) | acted: `design/errors.md` now owns roots, module taxonomy, and mapping boundaries | **DONE** |
| OD-10 | `embedder_pool_size` per-engine knob | not over-design after rationale; operator-facing contention control | **DONE** |
| OD-11 | `Search.rerank` + `Search.expand` Option fields | acted: `expand` kept, `rerank` deferred | **DONE** |
| OD-12 | bindings.md `--json` NDJSON wire commitment (stub-cited) | acted: CLI JSON contract aligned to verb-owned shapes | **DONE** |
| OD-14 | projection retry policy `3 / 1s/4s/16s` tunable | acted: retry policy kept as fixed 0.6.0 constants | **DONE** |
| OD-15 | `accept_identity_change` flag at `Engine.open` | acted: removed from 0.6.0 and deferred to 0.8.0 | **DONE** |
| OD-16 | projection batch B=64 "tunable per Engine.open" | acted: batch size fixed at constant `B=64` | **DONE** |
| OD-17 | `interfaces/cli.md` stub as locked artifact | acted: load-bearing CLI owner now exists in draft form | **DONE** |
| OD-19 | embedder Invariant 2 `engine_in_call` runtime guard | acted: documented as debug-only deadlock tripwire | **DONE** |
| OD-20 | vector-identity `(profile, dimension)` MAY cache | acted: removed as unnecessary consequence | **DONE** |
| OD-22 | `c_w` vs `projection_cursor` two-name-one-value | acted: write and read cursor semantics were separated again | **DONE** |
| OD-23 | `projection_failures` + `regenerate` AC gap | acted: REQ/AC coverage added for durable failures + explicit regenerate workflow | **DONE** |
| OD-24 | `&[PreparedWrite]` slice with deferred batch semantics | acted: single-batch transactional semantics documented | **DONE** |
| OD-25 | architecture locked while load-bearing design docs are absent | acted: missing design owners were created | **DONE** |
| OD-26 | CLI verb set diverges across ADR / REQ / AC | acted: corpus aligned to the same two-root CLI surface | **DONE** |
| OD-27 | `--json` output mode diverges across bindings / REQ / AC / HITL | acted: `check-integrity` object contract reconciled | **DONE** |
| OD-28 | write cursor semantics diverge across architecture / REQ / AC | acted: cursor semantics reconciled across architecture / REQ / AC | **DONE** |
| OD-29 | op-store core surface lacks direct REQ traceability | acted: direct REQ traceability added for op-store surface | **DONE** |
| OD-30 | op-store and projection-failure workflows lack AC coverage | acted: AC coverage added for op-store semantics and projection repair | **DONE** |

Findings still rated `not over-designed` and left unchanged: bindings § 3
`RecoveryHint.code`, `EngineError::{Overloaded, Closing}`,
typed-write-boundary itself, per-profile-lazy `C′` deferred option,
durability fsync `synchronous=NORMAL`, REQ-007 / AC-009 stress-failure schema,
ADR-recovery-rank-correlation tau gate, ADR-subprocess-bridge Option-A residue.

## Top priority revisions

1. **OD-25 — architecture locked while named design owners did not exist.**
   The design tree lacked the load-bearing files named by `architecture.md`.
   This was a design-corpus incompleteness, not over-design.
2. **OD-26 + OD-27 — CLI contract was contradictory.** The accepted CLI ADR,
   requirements, acceptance criteria, and bindings doc were not describing the
   same root shape or JSON posture.
3. **OD-28 — write-cursor semantics conflicted across docs.** `architecture.md`
   named `c_w` and `projection_cursor` as distinct values while REQ-055 and
   AC-059b had collapsed them.
4. **OD-03 / OD-04 / OD-29 / OD-30 — op-store is core but under-specified.**
   The fix is not "drop op-store"; it is either narrow it to the actually
   consumed subset or add REQ/AC coverage for the promised surface.
5. **OD-01 / OD-02 are now complete clean deletions.** They were the final
   highest-confidence net-negative-LoC wins and are now removed from the live
   corpus.

## Cross-cutting patterns

- **Clean deletions.** OD-01 and OD-02 were the clearest "remove speculative
  surface now, add later if a user actually needs it" cases and are now
  complete.
- **Speculative knobs.** OD-10, OD-14, and OD-16 were the strongest examples
  of "constant would satisfy the contract"; the corpus now documents the
  operator rationale for `embedder_pool_size`, fixes retry policy as constants,
  and fixes projection batch size at `B=64`.
- **Misclassified core surface.** OD-03, OD-04, OD-06, and OD-11 were
  originally treated as speculative, but they trace to
  FathomDB's core embedded-database product promise or to accepted REQs/ADRs.
- **Contract conflicts presented as over-design.** OD-08, OD-12, OD-17, OD-22,
  plus new OD-25..OD-30, are really documentation contradictions or missing
  owners. Those items are now fixed in the corpus rather than merely
  documented.

## OD-by-OD adjudication

- **OD-01 — DONE.** `tasks_in_flight` was only cited inside the scheduler ADR;
  the actual overload surface continues to use `projection_queue_depth` and
  `embedder_saturation`.
- **OD-02 — DONE.** Brand-specific default logger adapter language was
  removed from `design/bindings.md` § 8.
- **OD-03 — DONE.** The accepted op-store ADR now narrows
  `operational_mutations.op_kind` to `append`; the subsystem remains core.
- **OD-04 — DONE.** `latest_state` remains part of the product surface, but
  the derived `operational_current` table is gone. 0.6.0 keeps only
  authoritative regular op-store tables; performance-style rollups are
  calculated at query time.
- **OD-05 — REVISE TO LEAVE.** This knob is ADR-set and tied to the chosen
  scheduler implementation. It may still be a questionable public API, but it
  is not strong enough to classify as over-design in this audit.
- **OD-06 — DONE.** `PreparedWrite::AdminSchema` is now documented as the
  required engine-side carrier for accepted `admin.configure` work; the docs
  distinguish the locked variant existence from the still-owner-local internal
  field shape.
- **OD-07 — DONE.** `disabled_at` had no visible 0.6.0 consumer and has been
  removed from the accepted op-store schema.
- **OD-08 — DONE.** `design/bindings.md` now separates three boundaries:
  SDK verb symmetry, CLI as a separate operator surface, and engine-config
  knobs versus binding-runtime mechanics.
- **OD-09 — DONE.** `design/errors.md` now names the top-level roots, the
  module taxonomy, the validation split, and the binding / owner boundaries.
- **OD-10 — DONE.** `embedder_pool_size` remains accepted, and the corpus now
  states the operator rationale: embedded deployments need an explicit
  throughput-vs-contention lever for local embedding.
- **OD-11 — DONE / SPLIT.** `expand` stays; `rerank` does not. `expand`
  was already supported pre-0.6.0 in real codepaths and tests
  (`crates/fathomdb/src/search.rs`, `python/fathomdb/_query.py`,
  `crates/fathomdb/tests/grouped_query_reads.rs`, `python/fathomdb/tests/test_grouped_expand.py`)
  and is part of the README query story. 0.6.0 keeps `expand` as
  carried-forward core surface and defers `rerank`.
- **OD-12 — DONE.** The blanket NDJSON commitment was removed; CLI JSON output
  is now verb-owned and `check-integrity` is a single JSON object.
- **OD-14 — DONE.** Bounded retry remains, but the public tunables are gone:
  0.6.0 now treats `3` retries and `1s/4s/16s` backoff as fixed constants.
- **OD-15 — DONE.** `accept_identity_change` is removed from 0.6.0 and moved
  to a 0.8.0 deferred workflow draft.
- **OD-16 — DONE.** Projection batch size is fixed as a constant at `B=64`;
  it is no longer treated as an open-time tuning surface in 0.6.0.
- **OD-17 — DONE.** A load-bearing `interfaces/cli.md` draft now exists and
  owns the CLI contract instead of remaining a stub.
- **OD-19 — DONE.** `design/engine.md` now documents `engine_in_call` as a
  debug-only deadlock tripwire rather than a stable user-facing runtime
  contract.
- **OD-20 — DONE.** The MAY-cache note was removable and has been deleted from
  the vector-identity ADR.
- **OD-22 — DONE.** The docs now distinguish write-commit cursor from
  projection-visibility cursor again.
- **OD-23 — DONE.** `requirements.md`, `acceptance.md`, `design/projections.md`,
  `design/recovery.md`, and `interfaces/cli.md` now cover durable
  `projection_failures` plus the explicit regenerate workflow implemented by
  `recover --rebuild-projections`.
- **OD-24 — DONE.** `design/engine.md` now states the single-batch,
  single-transaction semantics of `&[PreparedWrite]`.

## Appended ODs — discovered incompletes

- **OD-25 — architecture locked while load-bearing design docs are absent.**
  At audit time, `architecture.md` assigned ownership to many `design/*.md`
  files that did not exist yet.
- **OD-26 — CLI verb set diverges across ADR / REQ / AC.**
  At audit time, the accepted CLI ADR and the later REQ/AC set were not
  describing the same two-root surface.
- **OD-27 — `--json` output mode diverges across docs.**
  At audit time, the bindings doc stated a blanket NDJSON rule that conflicted
  with the `check-integrity` object contract.
- **OD-28 — write cursor semantics diverge across docs.**
  At audit time, `architecture.md` distinguished `c_w` from `projection_cursor`
  while REQ-055 / AC-059b had collapsed them.
- **OD-29 — DONE.** `requirements.md` and `architecture.md` now give op-store
  direct traceability through REQ-057 and REQ-058, with REQ-059 covering the
  projection-failure workflow that depends on durable op-store rows.
- **OD-30 — DONE.** `acceptance.md` now covers `append_only_log`,
  `latest_state`, the absence of `operational_current`, collection-lifecycle
  schema boundaries, durable `projection_failures`, restart behavior, and the
  explicit regenerate workflow.

## Deferred — stubs, not primary findings

`interfaces/{python,typescript,wire}.md` remain short `not-started` stubs. They
do not yet expose enough surface for a deep over-design pass. Re-audit after
Phase 3e drafts land. The CLI interface file is no longer safely ignorable
because the rest of the corpus already depends on it; that problem is captured
by OD-17 / OD-26 / OD-27 instead of being waived here.

## Suggested triage

1. **Already applied corpus repairs:** OD-03, OD-04, OD-07, OD-10, OD-11,
   OD-12, OD-14, OD-15, OD-16, OD-17, OD-20, OD-22, OD-25, OD-26, OD-27,
   OD-28.
2. **High-confidence OD / deletion candidates still not yet acted:** none.
3. **Doc completion still pending:** none in this bucket.
4. **Acceptance / traceability completion:** none in this bucket.

## Method notes

- Original method retained: 6 read-only `architecture-inspector` subagents,
  parallel; falsification per finding via codebase grep → cited docs.
- Revision pass added an explicit product-purpose check against repo README
  before adjudicating whether a surface was speculative.
- Adjudicated entries (ENG6/ENG3/E3/E5/E7/X4/X6) remain excluded.
