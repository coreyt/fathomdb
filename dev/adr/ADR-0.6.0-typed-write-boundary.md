---
title: ADR-0.6.0-typed-write-boundary
date: 2026-04-25
target_release: 0.6.0
desc: Typed-write boundary at engine surface; clients never push raw SQL
blast_radius: every public write API across Rust / Python / TypeScript / CLI; engine writer architecture
status: accepted
---

# ADR-0.6.0 — Typed-write boundary

**Status:** accepted (HITL 2026-04-25, decision-recording)

## Context

0.5.x exposed a partial typed-write surface alongside several places where
raw SQL leaked through (admin paths, configure verbs taking string-typed
inputs). The result was 12+ coordinator sites with `SQLITE_SCHEMA` flooding
when admin DDL invalidated cached prepared statements (see Stop-doing
"runtime DDL on admin path" + "three parallel sources of truth for one
boolean").

Phase 1a disposition flagged `dev/archive/design-typed-write.md` as folded
input to `dev/design/engine.md`. Critic-A F8 challenged whether the
typed-vs-raw-SQL boundary was itself an open decision, or whether only the
PreparedWrite shape was open. HITL settled the boundary in this ADR; the
shape decision remains open in Phase 2 (decision-index #19).

## Decision

**Typed at the engine boundary. Clients never push raw SQL, ever.**

- All writes — application data, admin operations, configure operations, and
  recovery-tool writes — pass through typed Rust types at the engine
  boundary.
- Bindings (Python, TypeScript, CLI) accept only typed inputs that map to
  those Rust types; no `engine.execute(sql_string)` surface anywhere in any
  binding.
- Internal SQL is an implementation detail of the engine crate, not part of
  any public contract.

## Options considered

**A. Typed-only (chosen).** Engine accepts only typed payloads; SQL is
internal. Pros: removes the entire SQLITE_SCHEMA-flood failure class;
removes injection surface; removes "two shapes for one verb" stop-doing
class; lets the engine evolve its on-disk schema without breaking clients.
Cons: every write needs a Rust type; ad-hoc admin queries land as new
typed entries (cost is one-time per operation, not per call).

**B. Typed-with-raw-SQL-escape-hatch.** Default typed; accept a
`raw_sql(...)` builder for advanced use. Pros: lets advanced users do
ad-hoc work. Cons: re-introduces the exact failure mode 0.5.x had —
"escape-hatch" usage migrates into hot paths over time, and binding
authors expose the escape hatch to keep parity with Python; soon every
binding has it. Stop-doing on speculative knobs applies.

**C. SQL-first with optional typed builder.** What 0.5.x partially had.
Pros: maximum flexibility. Cons: re-introduces all of 0.5.x's typed/raw
boundary mess; injection surface; SQLITE_SCHEMA cache invalidation;
schema-leakage into client code.

## Consequences

- Phase 2 ADR #19 decides the **shape** of `PreparedWrite` (single enum,
  per-entity newtypes, builder, etc.) — the boundary itself is now closed.
- Phase 3e interfaces all derive from typed inputs; no `execute_sql` or
  `query_sql` surfaces.
- `interfaces/python.md`, `interfaces/typescript.md`, `interfaces/cli.md`
  must each enumerate their typed verb set; no string-SQL path.
- Recovery-tool CLIs (per Stop-doing "Recovery tooling is CLI-only, not
  SDK") are typed too — they take CLI flags, not SQL.
- **Op-store payload is typed-by-structural-carrier (X-2 cross-cite).**
  Op-store rows carry `OpStoreInsert { kind, payload: serde_json::Value,
  schema_id: Option<...> }` per ADR-0.6.0-op-store-same-file. The
  `serde_json::Value` is a structural carrier within a typed wrapper,
  not a raw-SQL escape: clients submit `OpStoreInsert` values, never
  SQL strings. Schema validation against `schema_id` (if present) is
  the engine's responsibility per the JSON-Schema policy followup
  (FU-M5).
- **Offline diagnostic / SQL-capable read-only binary (TWB-3): rejected.**
  An `fathomdb-inspect` style binary that opens the database and runs
  arbitrary SQL would re-introduce the schema-leakage failure mode this
  ADR closes (a tool today, a wrapper tomorrow, a binding next quarter).
  Recovery and inspection are typed CLI verbs (TWB-2 followup
  enumerates the verb set).

## Citations

- HITL decision 2026-04-25.
- `dev/archive/design-typed-write.md` (folded input).
- Stop-doing entries: runtime DDL on admin path; three parallel sources of
  truth for one boolean; layers-on-layers profile→kind→vec configure.
