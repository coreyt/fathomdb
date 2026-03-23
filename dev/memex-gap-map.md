# Memex Reference-Client Gap Map for `fathomdb`

## Scope

This note evaluates `fathomdb` from the perspective of a demanding reference
client: **Memex**.

The goal is not to define `fathomdb` as "the Memex database." The goal is to
ask a better engineering question:

> What should `fathomdb` own as a reusable database substrate for local,
> personal AI agents, and what should remain the responsibility of an agent
> client such as Memex?

This review compares Memex's current datastore needs against:

- `dev/design-typed-write.md`
- `dev/design-read-execution.md`
- `dev/design-detailed-supersession.md`

## Design Boundary: Database vs Client

This is the abstraction line I would want if `fathomdb` is intended to support
multiple clients rather than one application.

### What `fathomdb` should own

`fathomdb` should own the generic database-substrate concerns:

- canonical durability and transactional correctness
- versioning, supersession, retire, restore, and retention primitives
- derived projection management for FTS/vector/graph read surfaces
- generic query execution and decoded result families
- provenance, traceability, and excision primitives
- capability detection, capability reporting, and degraded-execution semantics
- integrity checking and repair-oriented admin surfaces
- concurrency and performance discipline for local embedded use

These are database responsibilities because every serious agent client will need
them, even if their domain models differ.

### What a client like Memex should own

Memex should own application and product concerns:

- domain-specific concepts such as meetings, notifications, reminders,
  prompt-control artifacts, or planning objects
- ingestion policy and orchestration
- operator UX and product-level fallback choices
- application-specific workflows and lifecycle rules
- client-specific semantic overlays or domain meaning

These are client responsibilities because they vary substantially across
products.

### What the contract between them should look like

The contract should be explicit and well-engineered:

- `fathomdb` provides stable generic storage primitives and read/write/admin
  semantics
- clients map their domain objects onto those primitives through a documented
  extension model
- `fathomdb` reports capabilities and integrity state explicitly
- clients decide how to present or consume those states

If `fathomdb` instead absorbs one product's table names and workflows directly,
it stops being a reusable database substrate. If it stays too generic to model
real operational state safely, it stops being useful.

## Updated Bottom Line

My view of `fathomdb` improved materially after reading
`design-detailed-supersession.md`.

Before that document, I considered lifecycle and update semantics one of the
largest gaps. That is no longer true. The supersession design gives `fathomdb`
a serious substrate story for:

- append-oriented history
- atomic replace and retire operations
- chunk and FTS cleanup on text-changing replacements
- explicit runtime-row status transitions for some operational records
- transaction ordering designed for recoverability

That is the right direction for a real agent database.

But as a reference-client review, my assessment is now:

- `fathomdb` is more credible as a future database substrate for Memex
- it is not yet sufficient as a complete datastore substrate for Memex
- the remaining gaps are less about basic mutation semantics and more about:
  - abstraction clarity
  - breadth of generic result and lifecycle surfaces
  - semantic integrity guarantees
  - operational completeness
  - client-extensibility without product-specific coupling

## Why Memex Is A Useful Reference Client

Memex is a good stress test because it needs more than document storage. It
persists:

- knowledge objects: items, sources, links, trails, embeddings
- conversation and session state
- operational state: tasks, runs, intake, settings, notifications, audit
- meetings and meeting-intelligence artifacts
- planning and world-model sidecars

Evidence:

- `src/memex/migrations.py:17-244`
- `src/memex/migrations.py:293-690`
- `src/memex/migrations.py:695-1253`

This does **not** mean `fathomdb` should adopt Memex's schema. It means that a
general-purpose agent database should be able to support a client with that kind
of breadth through stable generic mechanisms.

## What The Supersession Design Fixes At The Database Layer

### 1. Replace and retire are now proper database primitives

This is a major improvement.

The supersession design explicitly distinguishes:

- insert
- replace
- retire

Evidence:

- `dev/design-detailed-supersession.md:113-180`

Why this matters at the substrate layer:

- these are not Memex-specific operations
- every long-lived agent datastore needs controlled history-preserving mutation

### 2. Projection correctness is treated as a database concern

The supersession doc correctly treats stale chunks and stale FTS rows as a
correctness problem in the storage layer, not just an application cleanup task.

Evidence:

- `dev/design-detailed-supersession.md:187-299`

That is the right separation of concerns. A client should not have to remember
to clean up internal projection rows manually.

### 3. Some operational-state transitions are becoming substrate-level writes

Adding `upsert: bool` to `RunInsert`, `StepInsert`, and `ActionInsert` is a
useful move, not because those exact structs are sacred, but because it admits
that agent databases must support operational state transitions, not only
knowledge inserts.

Evidence:

- `dev/design-detailed-supersession.md:303-367`

### 4. Transaction ordering is designed for recovery

The sequence for retires, replacements, chunk changes, and FTS row updates is
trying to preserve detectable invariants under failure.

Evidence:

- `dev/design-detailed-supersession.md:454-486`

That is a strong substrate signal.

## Requirement Matrix: Substrate Expectations vs Memex Manifestation

| Requirement class | Database-substrate expectation | Memex manifestation | Current `fathomdb` coverage | Assessment |
|---|---|---|---|---|
| Canonical durability | One canonical durable authority with transactional mutation | SQLite-primary store with rebuildable projections | Typed write path keeps canonical rows primary: `dev/design-typed-write.md:17-30` | Strong fit |
| Concurrency discipline | Single-writer plus WAL-friendly readers for embedded local use | Memex already relies on WAL-backed SQLite | Explicit in write/read designs: `dev/design-typed-write.md:19-29`, `dev/design-read-execution.md:22-30` | Strong fit |
| Projection derivation | Database owns derived FTS/vector projection maintenance | Memex already treats search surfaces as derivative | FTS derivation is owned by engine; FTS cleanup on supersession is now designed: `dev/design-typed-write.md:96-105`, `dev/design-detailed-supersession.md:221-243` | Strong fit for FTS |
| Projection lifecycle completeness | Database should keep all derived surfaces consistent through replace/retire | Memex needs both text and vector correctness | Vector cleanup remains deferred: `dev/design-detailed-supersession.md:541-544` | Partial fit |
| History-preserving mutation | Replace/retire/restore/purge must be explicit, safe, and auditable | Memex uses version/history and delete lifecycle today | Replace/retire now exist; restore/purge do not | Partial fit |
| Durable correction trail | Why something changed or was retired should be queryable later | Memex needs operator trust and auditability | Retire provenance is warning-oriented, not a durable event model: `dev/design-detailed-supersession.md:419-428` | Missing |
| Semantic graph integrity | Relationships should remain semantically coherent or be diagnosable | Memex depends on linked knowledge and world-model records | Replace semantics are good; retire can leave dangling edges, detection deferred | Partial fit |
| Generic operational state support | Substrate should support durable operational records without becoming product-specific | Memex stores tasks, runs, intake, settings, notifications, meetings, planning state | Some runtime primitives exist, but coverage is still narrow | Partial fit |
| Client extensibility model | Database should support multiple client schemas/workflows via generic primitives or sanctioned extension seams | Memex has much broader state than current `fathomdb` runtime structs | Not yet clearly defined in current docs | Missing / unclear |
| Rich read model | Database should return more than node rows when clients need broader state families | Memex reads many heterogeneous record types | Read execution remains node-shaped first: `dev/design-read-execution.md:63-79`, `dev/design-read-execution.md:198-204` | Missing |
| Capability negotiation | Database should report capabilities and enable degraded or alternate execution plans explicitly | Memex often wants continued operation without vectors | Current read plan prefers capability error when vector support is absent: `dev/design-read-execution.md:123-134` | Needs better abstraction |
| Integrity and admin surface | Database should expose rebuild, diagnostics, and repair-oriented semantics | Memex relies on rebuild/recovery/admin flows | Directionally aligned, but current docs do not yet provide full contract | Partial fit |

## What Memex Needs Clearly, Without Requiring Memex-Specific Coupling

From the database point of view, Memex needs the following classes of support:

### 1. Knowledge-state storage

Memex needs durable support for:

- entities/objects
- relationships
- text search
- vector search
- provenance
- temporal history

This is generic agent-database territory. `fathomdb` is increasingly credible
here.

### 2. Operational-state storage

Memex needs durable support for:

- task and scheduler state
- intake and replay state
- operator-visible artifacts
- long-running workflow state

This is still database territory, but the substrate should express it through
generic operational-record primitives, not by absorbing Memex's table names.

### 3. Semantic-memory integrity

Memex needs guarantees that:

- active search projections reflect active canonical state
- relationship traversal does not silently drift into broken semantics
- recovery and integrity tools can diagnose semantic corruption, not only
  physical corruption

This must be owned primarily by the database substrate.

### 4. Client-visible capability and health reporting

Memex needs to know:

- which capabilities are currently available
- which are degraded
- when results are partial
- when rebuild or repair is needed

The database should expose these states clearly. The client should decide how to
surface them to users or operators.

## Revised View By Area

### 1. Canonical knowledge storage

Updated view:

- `fathomdb` now looks like a serious substrate candidate.

Good:

- append-oriented node history
- explicit replace/retire semantics
- chunk cleanup on replace
- FTS cleanup on replace and retire

Still missing for a world-class database substrate:

- vector cleanup parity
- chunk archival/history option
- fuller audit trail on retire/excise

### 2. Lifecycle and retention

Updated view:

- lifecycle is no longer a weak point at the level of basic replace/retire
- lifecycle is still incomplete as a database contract

Still needed:

- restore semantics
- purge/retention semantics
- durable event history for retire/excise/correction

This is substrate work, not client work.

### 3. Graph integrity and semantic truth

Updated view:

- replace semantics for edges are better than expected because traversal is by
  `logical_id`
- retire semantics still leave too much burden on the caller

For a world-class substrate, semantic graph damage should be:

- hard to create accidentally
- easy to detect automatically
- explicit in diagnostics

That means the integrity model still needs to grow.

### 4. Operational-state substrate

Updated view:

- current runtime primitives are a useful start
- they are still too narrow to demonstrate a true multi-client operational
  substrate

The key requirement is not "add Memex tables." It is:

- define a generic, extensible operational-state model that can support clients
  like Memex without bespoke database changes for each application

### 5. Read models and query surface

Updated view:

- this remains one of the biggest gaps

No matter how good writes become, the database substrate is incomplete if it can
only return node-shaped rows while real clients need:

- knowledge-state reads
- operational-state reads
- history and active-state views
- diagnostics and explainability

This is a substrate gap, not a Memex-specific request.

## What I Would Now Ask `fathomdb` To Build Next

These asks are phrased at the right layer: they are database responsibilities
that would help Memex, but they are not Memex-specific coupling requests.

### 1. Write down the substrate/client extension contract

Define clearly:

- what kinds of record families `fathomdb` natively supports
- what extension points clients can rely on
- what invariants the engine guarantees across those extensions

Without this, breadth discussions collapse into "add more tables."

### 2. Finish lifecycle semantics as a full database contract

Add:

- restore semantics
- retention/purge semantics
- durable retire/excise/correction event records
- vector lifecycle cleanup alongside FTS lifecycle cleanup

### 3. Make semantic integrity a first-class product promise

Expand integrity tooling so it can detect at minimum:

- dangling active edges after retire
- stale vector rows after chunk replacement
- rows retired without usable provenance
- mismatches between active canonical text and active search projections

### 4. Define a generic operational-state model

Rather than chasing one client's tables, provide a reusable model for durable:

- runs
- steps
- actions
- workflow/task state
- operator-visible artifacts

Then validate it against Memex-like workloads.

### 5. Widen the read surface into stable result families

Add result families for:

- canonical knowledge objects
- operational objects
- historical views
- diagnostics/explain plans

This is required for real clients to treat `fathomdb` as a database rather than
just a storage kernel.

### 6. Revisit capability reporting and degraded execution

The database should expose:

- capability availability
- degraded execution choices
- partial-result semantics

The client should choose policy, but the database should not force a single
hard-fail posture where degraded execution is possible.

## Final Judgment

As a Memex reference-client review, I would now describe `fathomdb` this way:

- It is becoming a credible database substrate for local, personal AI agents.
- The detailed supersession work is a real quality inflection point.
- The next step is not to become more Memex-specific.
- The next step is to become more explicit and more complete as a reusable
  substrate:
  - better abstraction boundaries
  - broader generic read and lifecycle surfaces
  - stronger semantic-integrity guarantees
  - clearer extension seams for multiple clients

Memex remains a useful proving ground because its datastore needs are broad and
operationally real. But the correct engineering target is:

- `fathomdb` should satisfy Memex-like demands
- without becoming a hardcoded Memex schema or workflow engine

That is the separation of concerns I would want from a well-engineered agent
database.
