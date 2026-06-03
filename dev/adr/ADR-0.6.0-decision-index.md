---
title: 0.6.0 Decision Index
date: 2026-04-25
target_release: 0.6.0
desc: Triage list of candidate decisions; not itself an ADR
blast_radius: TBD
status: in-progress
---

# Decision Index (triage)

This file is a **triage list**, not a decision. Each row is a candidate decision
that may become an accepted ADR.

Workflow per row: draft → critic (`architecture-inspector`) → HITL marks
`decide-now | defer | drop`. Decide-now → ADR file
`ADR-0.6.0-<kebab-title>.md` gets full options/tradeoffs/recommendation;
HITL accepts → ADR `status: accepted`.

## Phase 1 (already-decided / decision-recording ADRs)

These were settled during Phase 1a/1b HITL. ADRs record the rationale and
preserve options-considered for posterity; they do not deliberate.

| #   | Category     | Candidate decision                                                                                        | HITL verdict                  | ADR file                                    |
| --- | ------------ | --------------------------------------------------------------------------------------------------------- | ----------------------------- | ------------------------------------------- |
| 1   | architecture | Async-surface for engine API (sync engine + Path 2 TS-binding fix + Invariants A-D)                       | accepted (deliberation)       | ADR-0.6.0-async-surface.md                  |
| 2   | design       | Default-embedder architecture (candle + tokenizers + sqlite-vec; mean-pool + L2-normalize; zerocopy BLOB) | implemented in 0.7.1          | ADR-0.6.0-default-embedder.md               |
| 3   | architecture | sqlite-vec accept-no-fallback (sole-maintainer risk)                                                      | accepted (decision-recording) | ADR-0.6.0-sqlite-vec-acceptance.md          |
| 4   | interface    | Operator config = JSON-only (toml dropped)                                                                | accepted (decision-recording) | ADR-0.6.0-operator-config-json-only.md      |
| 5   | architecture | Typed at engine boundary; no raw SQL ever from clients                                                    | accepted (decision-recording) | ADR-0.6.0-typed-write-boundary.md           |
| 6   | architecture | Operational store lives in same sqlite file (no dual-store)                                               | accepted (decision-recording) | ADR-0.6.0-op-store-same-file.md             |
| 7   | design       | Embedder protocol contract (sync, unit-norm, no-reentrancy, engine-owned-thread, per-call-timeout)        | accepted (deliberation)       | ADR-0.6.0-embedder-protocol.md              |
| 8   | architecture | Vector BLOB on-disk invariants (LE f32, alignment, byte-length, BLOB affinity)                            | accepted (decision-recording) | ADR-0.6.0-zerocopy-blob.md                  |
| 9   | interface    | No 0.5.x→0.6.0 shims; no within-0.6.x multi-release deprecation cycles                                    | accepted (decision-recording) | ADR-0.6.0-no-shims-policy.md                |
| 10  | architecture | Single-writer-thread engine for 0.6.0; MVCC explicitly out of scope (closes Phase 2 #12 by deferral)      | accepted (decision-recording) | ADR-0.6.0-single-writer-thread.md           |
| 11  | design       | Vector identity belongs to the embedder; vector configs never carry identity strings                      | accepted (decision-recording) | ADR-0.6.0-vector-identity-embedder-owned.md |

## Phase 2 (deliberation ADRs)

Decisions that are not yet settled. Drafts pending after Phase 1 ADRs land.

| #   | Category     | Candidate decision                                                                                                                                           | HITL verdict                                                                      | ADR file                                                                 |
| --- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| 7   | acceptance   | Single-process durability target (fsync policy, recovery time)                                                                                               | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-durability-fsync-policy.md                                     |
| 8   | acceptance   | Projection freshness SLI numerical target                                                                                                                    | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-projection-freshness-sli.md                                    |
| 9   | acceptance   | Retrieval p50/p99 latency gates                                                                                                                              | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-retrieval-latency-gates.md                                     |
| 10  | acceptance   | Tier-1 CI platforms list                                                                                                                                     | accepted (HITL 2026-04-27, lite batch)                                            | ADR-0.6.0-tier1-ci-platforms.md                                          |
| 11  | architecture | Crate topology — keep `fathomdb-engine` monolith or split                                                                                                    | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-crate-topology.md                                              |
| 12  | architecture | Single-writer thread vs MVCC                                                                                                                                 | resolved-by-deferral                                                              | → ADR-0.6.0-single-writer-thread.md (Phase 1 #10); MVCC deferred to 0.7+ |
| 13  | architecture | Vector index location (vec0 in same sqlite file)                                                                                                             | accepted (HITL 2026-04-27, lite batch)                                            | ADR-0.6.0-vector-index-location.md                                       |
| 14  | architecture | Scheduler shape — tokio runtime + per-job tasks                                                                                                              | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-scheduler-shape.md                                             |
| 15  | architecture | Wire format for subprocess bridge                                                                                                                            | resolved-by-deferral                                                              | ADR-0.6.0-subprocess-bridge-deferral.md (revisit 0.8.0)                  |
| 16  | design       | Projection model — push (eager)                                                                                                                              | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-projection-model.md                                            |
| 17  | design       | Retrieval pipeline shape — fixed stages                                                                                                                      | accepted (HITL 2026-04-27); composable middleware revisit 0.8.0                   | ADR-0.6.0-retrieval-pipeline-shape.md                                    |
| 18  | design       | Error taxonomy — per-module + top-level wrap                                                                                                                 | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-error-taxonomy.md                                              |
| 19  | design       | PreparedWrite shape                                                                                                                                          | accepted (HITL 2026-04-27, lite batch)                                            | ADR-0.6.0-prepared-write-shape.md                                        |
| 20  | interface    | Python API shape                                                                                                                                             | accepted (HITL 2026-04-27, lite batch)                                            | ADR-0.6.0-python-api-shape.md                                            |
| 21  | interface    | TypeScript API — idiomatic + 1:1 type names                                                                                                                  | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-typescript-api-shape.md                                        |
| 22  | interface    | CLI scope — admin + recovery + read-only query                                                                                                               | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-cli-scope.md                                                   |
| 23  | interface    | Deprecation policy for 0.5.x names                                                                                                                           | accepted (HITL 2026-04-27, lite batch)                                            | ADR-0.6.0-deprecation-policy-0-5-names.md                                |
| 24  | acceptance   | Write-throughput SLI                                                                                                                                         | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-write-throughput-sli.md                                        |
| 25  | design       | JSON Schema validation policy (FU-M5 promoted)                                                                                                               | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-json-schema-policy.md                                          |
| 26  | acceptance   | Text-query latency gates (FU-REQ-010 promoted)                                                                                                               | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-text-query-latency-gates.md                                    |
| 27  | acceptance   | Recovery rank-correlation threshold (FU-AC-PROTOCOL-BACKFILL → AC-027d)                                                                                      | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-recovery-rank-correlation.md                                   |
| 28  | design       | Provenance retention shape (FU-AC-PROTOCOL-BACKFILL → AC-033)                                                                                                | accepted (HITL 2026-04-27)                                                        | ADR-0.6.0-provenance-retention.md                                        |
| 29  | architecture | Corruption-on-open behavior (FU-VEC13-CORRUPTION + FU-RECOVERY-CORRUPTION-DETECTION resolved)                                                                | accepted (HITL 2026-04-29)                                                        | ADR-0.6.0-corruption-open-behavior.md                                    |
| 30  | architecture | Database lock mechanism (hybrid sidecar flock + PRAGMA locking_mode=EXCLUSIVE; overrides earlier "no sidecar" assertion in architecture.md § 5 + § 8 + § 11) | superseded (2026-05-02) by ADR-0.6.0-database-lock-mechanism-reader-pool-revision | ADR-0.6.0-database-lock-mechanism.md                                     |
| 31  | architecture | Database lock mechanism — reader-pool revision (drops PRAGMA locking_mode=EXCLUSIVE; retains sidecar flock; admits `-shm` as a normal WAL artifact)          | accepted (HITL 2026-05-02)                                                        | ADR-0.6.0-database-lock-mechanism-reader-pool-revision.md                |

## Phase 0.7.1 (campaign-specific ADRs)

Decisions that arose during the 0.7.1 EMBEDDER-UNDEFER campaign. Listed
separately because they post-date the 0.6.0 Phase 1/2 deliberation.

| #   | Category | Candidate decision                                                                                                                               | HITL verdict               | ADR file                                   |
| --- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------- | ------------------------------------------ |
| 32  | design   | Narrow exception to NEED-017 / REQ-033 permitting opt-in default-embedder weight fetch on first use, sha256-pinned, with `OpenReport` visibility | accepted (HITL 2026-05-28) | ADR-0.7.1-default-embedder-weight-fetch.md |

## Phase 0.8.0 (campaign-specific ADRs)

Decisions for the 0.8.0 agent-memory-fit campaign (canonical-identity substrate +
governed-surface supersession). Listed separately because they post-date the
0.7.1 campaign; several remain HITL-pending and gate Slice 15 / Slice 25.

| #   | Category     | Candidate decision                                                                                                                                                                                                                                  | HITL verdict                          | ADR file                                         |
| --- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------- | ------------------------------------------------ |
| 33  | design       | Agent-memory retrieval & identity (planning input) — reclassify hybrid fusion+rerank (G9) and vector-metadata columns (G10) to table-stakes; direct the G0 substrate to be designed bi-temporal-aware (G11, Option 2A) so supersession is not built twice | draft, HITL-required                  | ADR-0.8.0-agent-memory-retrieval-and-identity.md |
| 34  | architecture | Canonical-identity substrate (G0 keystone) — `logical_id` + `superseded_at` on `canonical_nodes` AND `canonical_edges`; transaction-time now, valid-time additive later (no reshape); authorizes the Slice-15 schema delta (`SCHEMA_VERSION` 10→11); flags `write_cursor`-as-row-id for HITL | draft, HITL-required                  | ADR-0.8.0-canonical-identity-substrate.md        |
| 35  | acceptance   | Supersede AC-057a's five-verb *scope cap* with a governed, open-but-curated SDK surface; preserve the three load-bearing guarantees (SDK parity, recovery-name denylist, typed boundary); unblocks gated read verbs G2/G3/G4/G5/G7                       | decision-ready, HITL-sign-off-pending | ADR-0.8.0-supersede-five-verb-surface-cap.md     |
| 36  | design       | Embedder identity-change workflow — deferred workflow for intentional embedder identity swaps                                                                                                                                                          | draft (deferred)                      | ADR-0.8.0-embedder-identity-change-workflow.md   |

## Categories

acceptance | architecture | design | interface.
