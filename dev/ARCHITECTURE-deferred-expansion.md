# ARCHITECTURE-deferred-expansion.md

## Purpose

This file preserves architecture and schema ideas that are intentionally **not**
part of the tight v1 engine scope in [ARCHITECTURE.md](./ARCHITECTURE.md), but
are still valuable as likely future expansion paths.

The main architecture document now optimizes for implementation focus. This
document keeps the broader design space so it is not lost.

## 1. Broader Typed Semantic Table Candidates

Before narrowing the v1 runtime schema to `runs`, `steps`, and `actions`, the
architecture carried a wider set of typed semantic tables. These remain useful
candidates once the engine shape stabilizes:

- `intent_frames`
- `actions`
- `observations`
- `knowledge_objects`
- `meeting_artifacts`
- `control_artifacts`
- `evaluation_records`
- `approvals`
- `scheduler_runs`

The likely progression is:

1. start with the narrow runtime triad in `ARCHITECTURE.md`
2. keep world entities as generic `nodes` with explicit `kind` values
3. promote only the semantic families that prove stable and performance-critical
   into dedicated relational tables

## 2. Deferred Approval And Proposal Model

The broader design previously included proposal and approval states such as:

- `proposed`
- `approved`
- `rejected`
- `superseded`

These states are still relevant to the product and user-needs model, but are
deferred from the v1 core engine because they complicate base visibility rules
and temporal semantics.

Near-term guidance:

- keep the engine-level state model simple: active, superseded, deleted
- represent proposal/review state in JSON properties or SDK-level models
- revisit dedicated approval tables only after the query/compiler core is stable

## 3. Deferred Persistent Queue Table

An earlier design path allowed for a persistent SQLite queue table such as
`embedding_jobs` for optional semantic backfills.

This remains a plausible later evolution if the product needs:

- crash-resilient background job recovery
- multiple long-running background workers
- explicit introspection of queued and failed semantic backfills

It is deferred from v1 because a durable queue inside the same SQLite file can
fight the interactive writer path for the single write lock.

Current v1 direction:

- use an in-memory async queue for optional semantic projection work
- repair missing projections via startup rebuild checks

## 4. Future Table Promotion Guidance

The following semantic families are the most plausible candidates to graduate
from generic `nodes` to typed tables later:

### 4.1 Prompt-Control And Governance

- `intent_frames`
- `control_artifacts`
- `validation_results`
- `response_contracts`

Promote these if:

- explainability queries become hot
- offline replay/evaluation becomes a primary workflow
- JSON-based filtering on these records becomes a bottleneck

### 4.2 Meeting Intelligence

- `meeting_artifacts`
- `meeting_decisions`
- `meeting_commitments`
- `meeting_promotions`

Promote these if:

- meeting workflows become central to the product
- meeting-specific joins dominate retrieval or audit paths

### 4.3 Evaluation And Benchmarking

- `evaluation_records`
- `rubric_scores`
- `failure_labels`
- `comparison_runs`

Promote these if:

- controlled experimentation and version comparisons become routine
- evaluation workloads justify stricter schemas than generic nodes/actions

## 5. Broader Runtime Schema Possibility

If `runs`, `steps`, and `actions` eventually prove too narrow, the next natural
expansion is:

- `runs` for session/scheduler containers
- `steps` for control and LLM stages
- `actions` for concrete operations
- `observations` for normalized result records
- `control_artifacts` for parser/router/policy outputs

That progression preserves the v1 core while allowing more specialized read
paths later.

## 6. Why This File Exists

This file exists to separate two concerns cleanly:

- **Main architecture:** what should be built first
- **Deferred expansion:** what should not be forgotten

Use [ARCHITECTURE.md](./ARCHITECTURE.md) for implementation-driving decisions.
Use this file when revisiting schema expansion, approvals, background job
durability, and richer semantic-family promotion.
