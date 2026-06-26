# Design Docs

> **Current vs historical (2026-06-26 ledger-prune).** The **live** design surface is the
> cross-cutting *topic* specs (`engine.md`, `retrieval.md`, `vector.md`, `op-store.md`,
> `migrations.md`, `errors.md`, `embedder.md`, `lifecycle.md`, `scheduler.md`, `recovery.md`,
> `projections.md`, `bindings.md`, `orchestration.md`, `perf-gates.md`, `ann-index-vec0.md`,
> `release.md`) plus the live experiment decision-tree (`0.8.x-portfolio-features-and-experiment-tree.md`,
> `0.8.x-parity-portfolio-strategy.md`) and the in-flight `0.8.5-*` slice design.
> The per-slice memos (`slice-*-design.md`, `0.6.x/0.7.x/0.8.0–0.8.4-*`) are **historical
> records of closed slices — may be STALE**; their results are distilled in
> `dev/experiments-ledger.md` and their decisions live in `dev/adr/`. Frozen pre-registrations
> (`0.8.2-m1-multihop-harness.md`, `0.8.3-mem0-parity.md`) and `ir-recall-measure.md` are kept
> as load-bearing REFERENCE. See `dev/DOC-INDEX.md` for the authoritative per-doc map.

## Purpose

Detailed subsystem design documents that elaborate on accepted requirements and
ADRs.

## Keep here

- engine, retrieval, scheduler, vector, recovery, and release design docs
- cross-cutting design constraints

## Do not keep here

- public user/operator guidance
- architectural decision records
- disposable ideation notes

## Canonicality

Canonical for internal subsystem design, subject to ADR and requirement owners.

## Lifecycle

Living until design freeze; update when contracts materially change.
