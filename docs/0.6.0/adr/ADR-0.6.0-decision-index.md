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
| 7 | acceptance | Single-process durability target (fsync policy, recovery time) | TBD | TBD |
| 8 | acceptance | Projection freshness SLI numerical target | TBD | TBD |
| 9 | acceptance | Retrieval p50/p99 latency gates | TBD | TBD |
| 10 | acceptance | Tier-1 CI platforms list (Linux x86_64, Linux aarch64, macOS, Windows — already required globally; ADR pins minimum platform support window) | TBD | TBD |
| 11 | architecture | Crate topology — keep `fathomdb-engine` monolith or split (storage / projection / vector / query) | TBD | TBD |
| 12 | architecture | Single-writer thread vs MVCC | resolved-by-deferral | → ADR-0.6.0-single-writer-thread.md (Phase 1 #10); MVCC deferred to 0.7+ |
| 13 | architecture | Vector index location (vec0 in same sqlite file is the keep direction; ADR records and locks) | TBD | TBD |
| 14 | architecture | Scheduler shape — Arc/async actor; client-visible "vec-not-yet-consistent" surface (per HITL F5) | TBD | TBD |
| 15 | architecture | Wire format for subprocess bridge (proto / JSON / versioned) | TBD | TBD |
| 16 | design | Projection model — pull (lazy) vs push (eager scheduler) vs hybrid | TBD | TBD |
| 17 | design | Retrieval pipeline shape — fixed stages vs composable | TBD | TBD |
| 18 | design | Error taxonomy — single crate-level error enum vs per-module | TBD | TBD |
| 19 | design | PreparedWrite shape (typed boundary itself is settled per ADR-0.6.0-typed-write-boundary.md; this decides the type) | TBD | TBD |
| 20 | interface | Python API shape — sync only / async only / both (gates on ADR-0.6.0-async-surface.md) | TBD | TBD |
| 21 | interface | TypeScript API — mirror Python 1:1 vs idiomatic TS | TBD | TBD |
| 22 | interface | CLI scope — admin-only vs full query | TBD | TBD |
| 23 | interface | Deprecation policy for 0.5.x names (rewrite-proposal anti-requirement = no shims; this ADR records and pins) | TBD | TBD |
| 24 | acceptance | Write-throughput SLI (commits/sec target under single-writer-thread; forcing function for any future MVCC re-open) | decide-now (HITL 2026-04-27) | TBD |

## Categories

acceptance | architecture | design | interface.
