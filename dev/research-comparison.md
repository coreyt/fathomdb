# Research Comparison: Deep Research on SQLite-Centric Agent Datastores

## Purpose

This document compares the findings of a deep research survey on SQLite-centric
agent datastores against fathomdb's existing design and documentation. The goal
is to identify where the research validates existing decisions, where it surfaces
risks that are already mitigated, and where it highlights gaps or new
considerations.

---

## 1. Decisions The Research Validates

The research strongly confirms several of fathomdb's core architectural choices.

### Canonical-vs-derived separation

The research emphasizes that FTS5's external content mode makes the index/content
split explicit and that rebuild is an expected lifecycle operation, not a
catastrophe. fathomdb's `ARCHITECTURE.md` (§3, §6.4) already treats FTS and
vector tables as rebuildable derived projections, with `rebuild_projections()`
and `rebuild_missing_projections()` as first-class admin primitives. The
`db-integrity-management.md` doc goes further, defining three corruption classes
(physical, logical, semantic) and mapping each to a concrete recovery workflow.

**Verdict: Full alignment.** The research confirms this is the correct pattern
and that fathomdb's framing is more mature than most comparable projects.

### Single-writer as a deliberate product feature

The research cites SQLite's own documentation, rqlite, and dqlite to argue that
single-writer serialization is inevitable and should be embraced rather than
fought. fathomdb's `ARCHITECTURE.md` (§4.5, §7.1) explicitly uses a coordinated
writer thread/actor, `BEGIN IMMEDIATE` only after pre-flight enrichment, and an
in-memory write queue. The `design-typed-write.md` doc formalizes this as a
two-stage pipeline (`WriteRequest` → `PreparedWrite`) with heavy work outside
the lock.

**Verdict: Full alignment.** The research's strongest recommendation ("treat
single-writer as a product feature with explicit operational strategy") matches
fathomdb's existing design precisely.

### Pre-flight enrichment before write lock

The research highlights that holding write locks during expensive operations
(embedding generation, chunking) is a common failure mode. fathomdb's write path
design explicitly sequences enrichment before `BEGIN IMMEDIATE`
(`ARCHITECTURE.md` §5.3, `preliminary-solution-design.md` §4.5).

**Verdict: Full alignment.**

### Shim-managed projections instead of triggers

The research notes FTS5's trigger-based synchronization pitfalls (triggers don't
backfill pre-existing content, partial trigger strategies fail silently).
fathomdb explicitly chose shim-managed projection sync over triggers
(`ARCHITECTURE.md` §3.2) and documents this as a deliberate trade-off (§8).

**Verdict: Full alignment.** The research provides additional evidence for why
this was the right call.

### sqlite-vec over sqlite-vss

The research documents the ecosystem pivot away from sqlite-vss (full index
rewrite on writes) toward sqlite-vec (incremental updates, no dependencies).
fathomdb already specifies sqlite-vec as the vector extension
(`ARCHITECTURE.md` §1 core stack) and versions vector tables by embedding
profile (§7.5).

**Verdict: Full alignment.**

### JSONB for entity properties with expression indexes for hot paths

The research recommends storing documents as blobs plus extracting indexed fields
only for hot query paths, citing Couchbase Lite's approach and Cloudflare D1's
generated-column guidance. fathomdb's `ARCHITECTURE.md` (§2.1, §7.2) uses JSONB
blobs for entity properties with expression indexes on common JSON paths, and
explicitly prefers expression indexes over generated columns as the default.

**Verdict: Full alignment.** The research's Couchbase Lite citation ("extracting
object properties from raw blob data without parsing or memory allocation") adds
useful precedent for fathomdb's existing choice.

### Provenance and undo as first-class primitives

The research cites SQLite's session extension (invertible changesets) and
trigger-based undo/redo patterns as proven building blocks. fathomdb goes beyond
these generic patterns with a structured provenance model: `source_ref` on every
canonical row, `trace_source()` and `excise_source()` admin primitives, and
append-oriented supersession that preserves full history
(`design-detailed-supersession.md`, `design-repair-provenance-primitives.md`).

**Verdict: Full alignment, with fathomdb going deeper.** The research validates
the pattern; fathomdb's agent-specific provenance model (trace → excise →
rebuild) is more targeted than the generic SQLite undo patterns the research
describes.

### Deterministic agent SDK over raw SQL

The research warns that "LLM writes SQL" turns prompt injection into
database-corruption risk, citing SQLite's `SQLITE_DBCONFIG_DEFENSIVE` guidance.
fathomdb's query compiler and fluent AST builder (`ARCHITECTURE.md` §4) were
designed specifically to prevent agents from generating raw SQL.

**Verdict: Full alignment.**

---

## 2. Risks The Research Surfaces That Are Already Mitigated

### FTS drift from canonical state

The research warns that external-content FTS requires disciplined
synchronization. fathomdb mitigates this through:
- Shim-owned projection sync (no ad hoc writes to FTS tables)
- `ChunkPolicy::Replace` for atomic chunk+FTS cleanup on node replace
  (`design-detailed-supersession.md`)
- `check_semantics` to detect orphaned chunks and stale FTS rows
- `rebuild_fts` as a deterministic repair path

### Write amplification from vector indexing

The research warns about sqlite-vss's full-rewrite-on-write problem. fathomdb
mitigates this by:
- Using sqlite-vec (incremental updates)
- Treating vector projections as optional backfills for bulk ingestion
- Versioning vector tables by embedding profile instead of mutating in place

### Physical recovery destroying FTS/vector shadow tables

The research doesn't call this out, but fathomdb's `db-integrity-management.md`
explicitly addresses it: the recovery protocol recovers canonical tables only
(nodes, edges, chunks, runs, steps, actions) and rebuilds projections from
scratch. This is exactly the right approach and is more explicit than anything
the research describes.

### Extension packaging friction

The research notes that extension loading is enough of a blocker that Turso
forked SQLite to integrate vector search natively. fathomdb mitigates this by:
- Using sqlite-vec (dependency-free, portable)
- Keeping extension requirements minimal (FTS5 is built into SQLite, sqlite-vec
  is the only required extension)
- Using Rust for the engine (good FFI story for bundling native extensions)

This remains a risk to watch (see §4 below) but is not unaddressed.

---

## 3. Where The Research Reinforces Priority Of Existing Open Items

Several items already tracked in fathomdb's design docs are reinforced by the
research as high-priority:

### WAL checkpoint strategy and transaction lifetime management

The research emphasizes that long-running read transactions prevent checkpoint
progress and degrade read performance as the WAL grows. fathomdb's
`ARCHITECTURE.md` (§4.5) sets `PRAGMA journal_mode = WAL` and uses a reader
pool, but the operational monitoring of WAL size, checkpoint lag, and
long-running readers is not yet formalized.

**Recommendation:** Formalize WAL health observability as part of the admin
surface. The research's suggestion of exposing WAL size, checkpoint lag, and
last projection rebuild timestamps as "database health" signals is directly
actionable.

### Projection drift as observable state

The research recommends making "projection drift" a first-class observable
state, not just a repair trigger. fathomdb's `check_integrity()` already detects
missing FTS rows, but a proactive signal surface (e.g., "projections are N rows
behind canonical state") would be valuable for agent developers.

### Test coverage for crash consistency

The research cites SQLite's own testing culture and recommends "unusually strong
test coverage" for schema migrations, projection rebuild determinism, crash
consistency around WAL checkpoints, and semantic corruption rollback. fathomdb's
design docs use TDD throughout (`design-detailed-supersession.md` implementation
plan), but crash-consistency testing is not yet described.

---

## 4. New Considerations From The Research

### Multi-database sharding as a scale-out path

The research notes that if fathomdb eventually needs higher write concurrency,
"many databases" (sharding by agent/user/workspace) is the least disruptive
pattern, since SQLite itself frames multi-writer concurrency as the threshold
where client/server engines are better.

fathomdb's current design is single-file. This is correct for v1, but the
research's framing is worth recording as a future architecture note: if
concurrency becomes a bottleneck, shard by workspace rather than fight
single-writer limits.

### CRDT/multi-writer sync is explicitly incompatible with fathomdb's integrity model

The research documents CR-SQLite's constraint relaxation (no checked foreign
keys, no uniqueness constraints beyond primary key). fathomdb's design relies on
`PRAGMA foreign_keys = ON`, partial unique indexes for active-row invariants,
and referential integrity between canonical tables. Multi-writer merge semantics
would require fundamental changes.

This confirms that fathomdb's single-writer, single-file model is the right
choice for its integrity goals. If collaborative multi-agent use is needed,
the sharding-by-workspace approach is more compatible than CRDT merge.

### Litestream/LiteFS-style replication as a backup complement

The research describes WAL-driven replication tools (Litestream, LiteFS) that
could complement fathomdb's `safe_export()` with continuous backup. These are
not architectural dependencies but are worth noting as deployment options.
fathomdb's single-file model makes it a natural fit for WAL-streaming backup.

### The "15-minute debug" success metric

The research proposes a practical success metric: "Can an engineer debug a wrong
agent memory in under 15 minutes using only SQLite tooling plus fathomdb's admin
commands?"

This is a good framing for evaluating the admin surface. fathomdb's
`trace_source` → `excise_source` → `rebuild_projections` workflow is designed
for exactly this scenario, and the Go CLI's `trace`, `excise`, `apply`, and
`repair` commands map to it. The `db-integrity-management.md` doc already
describes this workflow in detail (export → trace → excise → apply patch →
rebuild).

### Graceful degradation when vector capability is absent

The research doesn't address this directly, but the `memex-gap-map.md` flags it
as a mismatch: fathomdb currently prefers explicit capability errors when vector
support is unavailable, while real-world deployments (like Memex) often want
degraded-but-usable behavior (fall back to FTS/structured retrieval).

This is already tracked as a gap and is reinforced by the research's emphasis on
operational simplicity.

---

## 5. What The Research Does NOT Cover That fathomdb Already Addresses

The research survey is broad but does not go as deep as fathomdb's existing
design in several areas:

### Structured supersession semantics

The research mentions undo/replay patterns generically (session extension,
trigger-based undo logging). fathomdb's `design-detailed-supersession.md`
defines a complete supersession model: Insert/Replace/Retire operations,
chunk lifecycle policies, FTS consistency guarantees, cascade rules, runtime
table status transitions, and transaction ordering. This is substantially
more mature than anything the research describes.

### Agent-specific provenance model

The research cites source tracking and correction history as desirable. fathomdb
goes further with `source_ref` as a mandatory-enough field on every canonical
row, structured `TraceReport` output, provenance-aware excision that restores
prior active versions, and `provenance_warnings` on write receipts.

### Three-layer corruption taxonomy

The research mentions physical and logical corruption in passing. fathomdb's
`db-integrity-management.md` formalizes three corruption classes (physical,
logical, semantic) with distinct detection and recovery workflows for each.
The Go integrity tool design (`fathom-integrity-recovery.md`) maps these to
concrete CLI operations with three detection layers (SQLite storage, engine
invariants, application semantics).

### Typed write pipeline with preparation stages

The research discusses write-path concerns generically. fathomdb's
`design-typed-write.md` and `design-detailed-supersession.md` define a concrete
two-stage pipeline (WriteRequest → PreparedWrite) with typed structs, engine-
owned projection derivation, and explicit backfill accounting. This is more
operational than the research's pattern-level recommendations.

---

## 6. Summary Matrix

| Topic | Research Position | fathomdb Position | Assessment |
|---|---|---|---|
| Canonical-vs-derived separation | Strong recommendation | Core architecture | Validated |
| Single-writer model | Embrace, don't fight | Coordinated writer actor | Validated |
| Pre-flight enrichment | Best practice | Explicit design requirement | Validated |
| Shim-managed sync (no triggers) | Implied by FTS5 pitfalls | Explicit architectural choice | Validated |
| sqlite-vec over sqlite-vss | Ecosystem consensus | Already adopted | Validated |
| JSONB + expression indexes | Best practice | Already adopted | Validated |
| Provenance/undo primitives | Generic SQLite patterns | Agent-specific model (deeper) | fathomdb ahead |
| Agent SDK over raw SQL | Security recommendation | Core design motivation | Validated |
| WAL checkpoint observability | Operational recommendation | Pragmas set, monitoring not formalized | Gap to close |
| Projection drift observability | Operational recommendation | Detection exists, proactive signals missing | Gap to close |
| Crash consistency testing | Strong recommendation | TDD discipline, crash tests not yet described | Gap to close |
| Multi-DB sharding for scale | Future architecture note | Not addressed (correctly for v1) | Record for future |
| CRDT incompatibility | Documented constraint trade-offs | Confirms single-writer choice | Validated |
| WAL-streaming backup | Deployment option | Compatible with single-file model | Nice-to-have |
| 15-minute debug metric | Proposed success criterion | Admin workflow supports it | Adopt as metric |
| Graceful vector degradation | Not directly addressed | Known gap (memex-gap-map.md) | Existing open item |

---

## 7. Recommendations

1. **Adopt the "15-minute debug" metric** as a concrete success criterion for
   the admin surface. The trace → excise → rebuild workflow is already designed
   for it.

2. **Formalize WAL health observability** (WAL size, checkpoint lag,
   long-running reader detection) as part of the admin/operational surface.

3. **Add crash-consistency test scenarios** to the test plan, particularly
   around WAL checkpoints, interrupted projection sync, and partial write
   recovery.

4. **Record multi-database sharding** as the preferred future scale-out
   strategy if single-file write throughput becomes a bottleneck.

5. **Continue treating CRDT/multi-writer** as out of scope; the research
   confirms this is incompatible with fathomdb's integrity model.

6. **Consider Litestream-style WAL streaming** as a deployment-time backup
   complement to `safe_export()`, but not as an architectural dependency.
