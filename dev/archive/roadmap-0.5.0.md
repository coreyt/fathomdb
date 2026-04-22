# FathomDB 0.5.0 roadmap

Captures the remaining item deferred from 0.4.x, with enough design
detail that implementation can start without re-litigating the shape.
Companion to `dev/fathom-memex-near-term-roadmap.md`.

## Scope

0.5.0 is a **ranking** release. One headline item:

- **Per-field BM25 weighting for property-FTS** (item 10).

Item 8 (`SearchBuilder.expand(slot=...).execute_grouped()`) and
item 9 (background recursive property-FTS rebuild / shadow-build)
both ship in **0.4.1**. Item 9 was originally planned for 0.5.0 but
was pulled forward on its own merits — see
`dev/notes/scope-0.4.1.md` and `dev/notes/design-0.4.1-async-rebuild.md`
for the implementation. Decoupling item 9 from item 10 lets the
async-rebuild machinery bake for a release cycle before item 10b's
storage migration depends on it.

> Historical note: an earlier revision of this doc contained the
> full design for item 9 in this section. That material has moved
> to `dev/notes/design-0.4.1-async-rebuild.md`; the short summary
> below is retained for cross-reference from the item 10b
> coordination section.

---

## Item 9: Background recursive property-FTS rebuild — **moved to 0.4.1**

Pulled forward from 0.5.0 to 0.4.1 on its own merits:

1. **Release-engineering value.** Shipping the async-rebuild
   machinery a release early lets it bake before item 10b's
   storage migration depends on it. Crash-recovery edge cases,
   background task lifecycle, and FTS5 bulk-insert cost numbers
   all benefit from a release cycle of real-world exposure
   before 10b rides on top.
2. **Breaking-change spreading.** 0.4.1 and 0.5.0 both carry
   behavior-contract shifts. Pulling item 9 forward splits them
   across two releases instead of stacking both on 0.5.0.
3. **Memex unblock is free at this point.** Memex said "no
   urgency" in their round-3 response and accepted the 0.5.0
   deferral without a preview ask. Pulling forward is a favorable
   surprise with zero cost to Memex's adoption plan.

**Full implementation design:**
`dev/notes/design-0.4.1-async-rebuild.md`.

**Short summary retained here** for cross-reference from the
item 10b coordination section below:

- `register_fts_property_schema_with_entries` becomes semi-async:
  schema persistence synchronous, rebuild queued to a background
  task, register call returns in milliseconds.
- Background task rebuilds into a non-FTS5 staging table
  (`fts_property_rebuild_staging`) in bounded batches.
- Writes during rebuild double-write: live FTS5 table (old schema,
  reads stay correct) + staging (new schema, stays caught up).
- Reads during rebuild serve the old schema. First-registration
  case falls back to a JSON scan.
- Final swap: short IMMEDIATE tx bulk-inserts staged rows into
  `fts_node_properties` and clears state. Still O(rows) lock
  hold, but only FTS5-insert cost, not JSON-walking cost —
  major improvement over the current 5–10 minute window. Zero-
  stall swap is a post-0.4.1 follow-up (and will be handled
  naturally by item 10b's per-kind table model).
- `RebuildMode::Eager` escape hatch preserves today's
  transactional semantics for tests and small kinds.

---

## Item 10: Per-field BM25 weighting for property-FTS

### Problem

Today every indexed path in a property-FTS schema contributes
equal-weight to BM25. For kinds like Memex's `WMKnowledgeObject`
(where `$.title` is ~5 tokens and `$.payload` recursive is thousands),
a query that literally matches the title can score lower than a kind
whose payload mentions the phrase in passing. The symptom Memex
reports is "obviously-relevant record ranks below incidental
mentions."

The current storage model (`crates/fathomdb-schema/src/bootstrap.rs:390-394`)
is a single FTS5 virtual table `fts_node_properties` with columns
`node_logical_id UNINDEXED, kind UNINDEXED, text_content`. Recursive
paths are concatenated into `text_content` at write time by
`extract_property_fts` in `crates/fathomdb-engine/src/writer.rs:1176`.
A position sidetable `fts_node_property_positions` records which path
each token came from, but nothing in the query path uses that for
scoring — only for fused filter routing.

### Design: three coordinated changes

The 0.5.0 release ships three changes together. None of them alone
fully solves the problem; together they do.

#### 10a. `matched_paths` attribution on `SearchHit` (independent, ships first)

**Already partially wired.** `SearchHit.attribution:
Option<HitAttribution>` exists at
`crates/fathomdb-query/src/search.rs:68-79,148-149` as Phase 5
placeholder, currently always `None`. The `matched_paths: Vec<String>`
field is reserved inside `HitAttribution`.

Work: light up `attribution` for property-FTS hits by joining the
`fts_node_property_positions` sidetable during query execution and
returning the set of paths that contained any matching token. The
data is already there — this is plumbing, not architecture.

Value as a standalone change: callers can do their own per-path
reranking without any engine-side weighting. It is **not** a
replacement for 10b; it is a different tool. A rerank multiplier at
query time cannot change which candidates survive BM25's top-N cutoff
— only engine-side weighting can do that.

Ships as a self-contained PR early in 0.5.0. Unblocks Memex's
ranking work before the heavier 10b lands.

#### 10b. FTS5 per-column weights on property-FTS

Move property-FTS from a single-column FTS5 table to a **per-kind
dynamic FTS5 table with one column per registered path**, where each
scalar path is its own column and the recursive payload (if any) is
one column containing all recursive leaves concatenated. Weights
attach to `FtsPropertyPathSpec` as a `weight: f32` field (default
`1.0`, preserves current behavior).

Target shape (Memex-provided, validated):

```rust
engine.admin().register_fts_property_schema_with_entries(
    "WMKnowledgeObject",
    entries: vec![
        FtsPropertyPathSpec::scalar("$.title").weight(10.0),
        FtsPropertyPathSpec::scalar("$.knowledge_type").weight(2.0),
        FtsPropertyPathSpec::scalar("$.source_url").weight(1.0),
        FtsPropertyPathSpec::scalar("$.canonical_key").weight(1.0),
        FtsPropertyPathSpec::recursive("$.payload").weight(1.0),
    ],
)?;
```

**Why per-kind dynamic tables, not a single global table with more
columns:** different kinds register different path sets. A global
table would need one column per (kind, path) pair, which explodes
column count and wastes storage. Per-kind tables naturally bound
column count to "paths registered on that kind."

**Why one column for all recursive leaves, not one per leaf:**
recursive paths by definition have an open-ended leaf set. One column
per leaf is not expressible — the leaf set is discovered at write
time. One column for the whole recursive subtree matches both
Memex's mental model and the storage reality.

**Migration:** 0.5.0 ships a bootstrap migration that creates per-kind
tables on first use for kinds with registered schemas. Existing
`fts_node_properties` remains for backward compat during the
migration window; new registrations write to per-kind tables, and the
0.5.0 upgrade walks existing schemas and creates/backfills per-kind
tables. Uses the 0.5.0 async rebuild path from item 9 — so the
upgrade itself doesn't stall writes.

**BM25 scoring:** SQLite FTS5 supports per-column weights via the
`bm25(table, weight1, weight2, ...)` rank function. The weights
registered on the schema get passed to BM25 at query time in the
same order as the columns. No custom scoring code; SQLite does the
weighting natively.

**Fused filters:** the named fused JSON filters from 0.4.0 continue
to work. The fusion routing (`fts_node_property_positions`) needs
updating to reference the per-kind table, but the user-facing
contract is unchanged.

#### 10c. Snippet format stability statement

Explicitly document that `SearchHit.snippet` format is unstable and
callers must not parse it. Add to `docs/reference/types.md` and
`docs/guides/querying.md`. Trivial docs change but closes a future
footgun — Memex's interim workaround (parsing snippet substrings to
approximate title match) is exactly the anti-pattern this docs note
prevents from calcifying.

If we later decide snippet format *should* be stable for some
use case, introduce a new structured field (`snippet_fields:
Vec<SnippetFragment>` or similar) rather than blessing the current
string format.

### Coordination with item 9

Item 10b's migration uses item 9's async rebuild path. With item 9
shipping in 0.4.1, the machinery is available by the time 0.5.0
development starts. The 0.5.0 upgrade's FTS-schema migration walks
existing schemas, creates per-kind tables, and enqueues a shadow
rebuild for each via the 0.4.1 async-rebuild path — **the upgrade
does not stall writers.** Shipping 10b on the current eager-rebuild
path (as would have been the case before item 9 was pulled forward)
would have stalled every existing engine for 5–10 minutes at upgrade
time, which is the footgun item 9 fixed.

A bonus: item 10b's per-kind FTS5 tables make item 9's swap cost
effectively zero. Today the final swap in item 9's design is a bulk
INSERT into `fts_node_properties` scoped to one kind, which is
O(rows) lock hold. Under item 10b's per-kind-table model, the swap
becomes a `DROP TABLE` + `ALTER TABLE RENAME` on the shadow table,
which is O(1). This is not a reason to couple the two items — item 9
ships first and stands alone — but it is a reason the two items
compose naturally.

10a does not depend on item 9 and can ship independently early in
0.5.0.

### Open implementation questions

- **Position sidetable scope.** `fts_node_property_positions` is
  currently keyed by `(node_logical_id, path)`. Under per-kind tables
  this sidetable either moves per-kind or gains a `kind` column. The
  fused filter query path touches this — check cost impact before
  deciding.
- **Dynamic column naming.** SQLite FTS5 column names are fixed at
  `CREATE VIRTUAL TABLE` time. The engine needs to derive stable
  column names from path strings. Candidate: sanitized path hash
  (e.g. `path_a1b2c3`). Not user-visible.
- **Weight range validation.** SQLite BM25 accepts any f32, but
  extreme values produce degenerate ranking. Validate at registration
  time (e.g. `0.0 <= weight <= 1000.0`) with a clear error.
- **Query path update.** `coordinator.rs:1441` hardcodes
  `-bm25(fts_node_properties)`. This becomes dynamic based on kind —
  either a per-kind query template or a JOIN through a dispatch
  table. Implementation detail.

### Non-goals

- Per-leaf weights within a recursive path. Out of scope; Memex's
  mental model is "one weight for the whole recursive subtree" and
  that matches storage reality.
- Runtime weight tuning without re-registering the schema. 0.5.0
  treats weights as part of the schema — changing them requires a
  new registration (which goes through async rebuild).
- Cross-kind weight normalization. Each kind's weights are
  independent; the per-dispatcher rerank layer (Memex's
  `score_search_rows`) handles cross-kind fusion.

### Memex impact

On ship:
- Memex's `m004_register_fts_property_schemas_v2.py` gains weight
  fields on the four hot kinds (`WMKnowledgeObject`,
  `WMExecutionRecord`, `WMAction`, `WMGoal`).
- `score_search_rows` at `src/memex/memory/ranking.py:229` can drop
  any snippet-parsing heuristic it added in the interim.
- If 10a ships early, Memex can consume `matched_paths` immediately
  without waiting for 10b.

---

## Out of scope for 0.5.0

Write-priority / foreground-read isolation (item 11) stays deferred
pending Memex scheduler-burst instrumentation. No change to that
sequencing.

## Critical path

1. Ship **0.4.1** (item 8: grouped expand + item 9: shadow-build
   async rebuild) — Memex is waiting on item 8; item 9 ships on
   its own merits and bakes the machinery 10b depends on.
2. 0.5.0 development:
   - 10a (`matched_paths` attribution) — independent, ships first.
   - 10b (per-kind FTS5 tables + per-column weights) — uses
     item 9's async rebuild machinery (already shipped in 0.4.1)
     for the upgrade migration. Also drives item 9's swap cost
     to O(1) as a side benefit.
   - 10c (snippet stability docs) — can ship any time.
3. 0.5.0 release.
