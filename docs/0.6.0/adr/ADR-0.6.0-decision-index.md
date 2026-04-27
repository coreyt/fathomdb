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

| # | Category | Candidate decision | HITL verdict | ADR file |
|---|----------|-------------------|--------------|----------|
| 1 | architecture | Async-surface for engine API (sync engine + Path 2 TS-binding fix + Invariants A-D) | accepted (deliberation) | ADR-0.6.0-async-surface.md |
| 2 | design | Default-embedder architecture (candle + tokenizers + sqlite-vec; mean-pool + L2-normalize; zerocopy BLOB) | accepted (decision-recording) | ADR-0.6.0-default-embedder.md |
| 3 | architecture | sqlite-vec accept-no-fallback (sole-maintainer risk) | accepted (decision-recording) | ADR-0.6.0-sqlite-vec-acceptance.md |
| 4 | interface | Operator config = JSON-only (toml dropped) | accepted (decision-recording) | ADR-0.6.0-operator-config-json-only.md |
| 5 | architecture | Typed at engine boundary; no raw SQL ever from clients | accepted (decision-recording) | ADR-0.6.0-typed-write-boundary.md |
| 6 | architecture | Operational store lives in same sqlite file (no dual-store) | accepted (decision-recording) | ADR-0.6.0-op-store-same-file.md |
| 7 | design | Embedder protocol contract (sync, unit-norm, no-reentrancy, engine-owned-thread, per-call-timeout) | accepted (deliberation) | ADR-0.6.0-embedder-protocol.md |
| 8 | architecture | Vector BLOB on-disk invariants (LE f32, alignment, byte-length, BLOB affinity) | accepted (decision-recording) | ADR-0.6.0-zerocopy-blob.md |
| 9 | interface | No 0.5.x→0.6.0 shims; no within-0.6.x multi-release deprecation cycles | accepted (decision-recording) | ADR-0.6.0-no-shims-policy.md |
| 10 | architecture | Single-writer-thread engine for 0.6.0; MVCC explicitly out of scope (closes Phase 2 #12 by deferral) | accepted (decision-recording) | ADR-0.6.0-single-writer-thread.md |
| 11 | design | Vector identity belongs to the embedder; vector configs never carry identity strings | accepted (decision-recording) | ADR-0.6.0-vector-identity-embedder-owned.md |

## Phase 2 (deliberation ADRs)

Decisions that are not yet settled. Drafts pending after Phase 1 ADRs land.

| # | Category | Candidate decision | HITL verdict | ADR file |
|---|----------|-------------------|--------------|----------|
| 7 | acceptance | Single-process durability target (fsync policy, recovery time) | accepted (HITL 2026-04-27) | ADR-0.6.0-durability-fsync-policy.md |
| 8 | acceptance | Projection freshness SLI numerical target | accepted (HITL 2026-04-27) | ADR-0.6.0-projection-freshness-sli.md |
| 9 | acceptance | Retrieval p50/p99 latency gates | accepted (HITL 2026-04-27) | ADR-0.6.0-retrieval-latency-gates.md |
| 10 | acceptance | Tier-1 CI platforms list | TBD (lite — defer to batch) | TBD |
| 11 | architecture | Crate topology — keep `fathomdb-engine` monolith or split | accepted (HITL 2026-04-27) | ADR-0.6.0-crate-topology.md |
| 12 | architecture | Single-writer thread vs MVCC | resolved-by-deferral | → ADR-0.6.0-single-writer-thread.md (Phase 1 #10); MVCC deferred to 0.7+ |
| 13 | architecture | Vector index location (vec0 in same sqlite file) | TBD (lite — defer to batch) | TBD |
| 14 | architecture | Scheduler shape — tokio runtime + per-job tasks | accepted (HITL 2026-04-27) | ADR-0.6.0-scheduler-shape.md |
| 15 | architecture | Wire format for subprocess bridge | resolved-by-deferral | ADR-0.6.0-subprocess-bridge-deferral.md (revisit 0.8.0) |
| 16 | design | Projection model — push (eager) | accepted (HITL 2026-04-27) | ADR-0.6.0-projection-model.md |
| 17 | design | Retrieval pipeline shape — fixed stages | accepted (HITL 2026-04-27); composable middleware revisit 0.8.0 | ADR-0.6.0-retrieval-pipeline-shape.md |
| 18 | design | Error taxonomy — per-module + top-level wrap | accepted (HITL 2026-04-27) | ADR-0.6.0-error-taxonomy.md |
| 19 | design | PreparedWrite shape | TBD (lite — defer to batch) | TBD |
| 20 | interface | Python API shape | TBD (lite — defer to batch; gates on async-surface) | TBD |
| 21 | interface | TypeScript API — idiomatic + 1:1 type names | accepted (HITL 2026-04-27) | ADR-0.6.0-typescript-api-shape.md |
| 22 | interface | CLI scope — admin + recovery + read-only query | accepted (HITL 2026-04-27) | ADR-0.6.0-cli-scope.md |
| 23 | interface | Deprecation policy for 0.5.x names | TBD (lite — defer to batch; closes against no-shims-policy) | TBD |
| 24 | acceptance | Write-throughput SLI | accepted (HITL 2026-04-27) | ADR-0.6.0-write-throughput-sli.md |
| 25 | design | JSON Schema validation policy (FU-M5 promoted) | accepted (HITL 2026-04-27) | ADR-0.6.0-json-schema-policy.md |

## Categories

acceptance | architecture | design | interface.
