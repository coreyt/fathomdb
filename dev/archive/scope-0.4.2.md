# FathomDB 0.4.2 scope

Two headline items, one foundational migration:

1. **Per-kind FTS5 tables with per-column BM25 weights** — roadmap
   item 10b, pulled forward from 0.5.0. Replaces the single global
   `fts_node_properties` table with one FTS5 table per registered
   kind. Unlocks per-column weight configuration and is the storage
   foundation that 0.4.5's user-picked tokenizer selection builds on.
2. **`matched_paths` attribution on `SearchHit`** — roadmap item 10a,
   pulled forward from 0.5.0. Lights up `HitAttribution::matched_paths`
   using the existing `fts_node_property_positions` sidecar. Independent
   of item 1; can be implemented in parallel.

Item 10c (snippet stability docs) ships with 0.4.2 as a documentation
deliverable.

> **Unblocking note:** Pack A (schema + per-kind table creation) must
> land first. It defines the interface — `create_fts_kind_table(kind,
> tokenizer)` + `projection_profiles` table + tokenizer lookup — that
> the 0.4.5 implementer plugs into. The 0.4.5 work (user-picked
> tokenizers) can begin once Pack A's schema and Rust interface are
> merged, even before Packs B/C land.

## Critical path

**Item 10b (Pack A → Pack B)** is the structural migration. Pack A is
the unblocking dependency for 0.4.5. Pack B adds the user-visible
weight feature on top of the per-kind infrastructure Pack A creates.

**Item 10a (Pack C)** is independent — can be implemented in parallel
with Packs A and B.

No Memex schema changes are gated on 0.4.2. The adoption for item 10a
is a pure caller-side rerank opportunity; item 10b is storage-layer
only (no public API change on the search surface).

## What ships

### Pack A: Schema foundations (unblocking pack)

**Schema migration 20** adds two things:

1. `projection_profiles` table:

```sql
CREATE TABLE projection_profiles (
    kind  TEXT NOT NULL,   -- node kind (e.g. 'WMKnowledgeObject') or '*' for global
    facet TEXT NOT NULL,   -- 'fts' | 'vec'
    config_json TEXT NOT NULL,
    active_at   INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (kind, facet)
);
```

2. Per-kind FTS5 table creation for each existing entry in
   `fts_property_schemas`:

```sql
-- one per kind, created by migration 20
CREATE VIRTUAL TABLE fts_props_<sanitized_kind> USING fts5(
    node_logical_id UNINDEXED,
    <path_col_1>,
    <path_col_2>,
    ...
    tokenize = '<tokenizer_string>'
);
```

The migration creates the tables empty and enqueues an async rebuild
for each kind (inserts PENDING rows into `fts_property_rebuild_state`).
`fts_node_properties` is **dropped** in this migration. During the
rebuild window, the existing JSON-scan fallback (first-registration
path) serves FTS queries — same behavior as a brand-new kind.

#### Kind-name sanitization rule (canonical, used everywhere)

Table names are derived deterministically from the node kind:
1. Lowercase the kind string.
2. Replace any character that is not `[a-z0-9]` with `_`.
3. Collapse consecutive underscores to one.
4. Prefix with `fts_props_`.
5. If the result exceeds 63 characters, truncate to 55 characters and
   append `_` + first 7 chars of the SHA-256 hex of the original kind.

Examples: `WMKnowledgeObject` → `fts_props_wmknowledgeobject`,
`WMExecutionRecord` → `fts_props_wmexecutionrecord`.

This rule must be implemented as a single shared function
(`fts_kind_table_name(kind: &str) -> String`) in `fathomdb-schema`
and used by every call site — never inlined.

#### Tokenizer lookup rule (canonical for 0.4.5 wiring)

When creating a per-kind FTS5 table, the tokenizer string is resolved
as follows:

1. Query `projection_profiles WHERE kind = ?1 AND facet = 'fts'`.
2. If a row exists, parse `config_json` for `"tokenizer"` string.
3. If no row exists, default to `porter unicode61 remove_diacritics 2`.

This lookup is implemented in `bootstrap.rs` as
`resolve_fts_tokenizer(conn, kind) -> String`. The 0.4.5 work wires
`configure_fts` to write a profile row before this lookup is called;
in 0.4.2 the table is always empty so the default applies everywhere.

#### Writer update

All `INSERT INTO fts_node_properties` call sites in `admin.rs` (and
`writer.rs`) must be updated to route to the per-kind table using
`fts_kind_table_name(kind)`. Count as of schema audit: ~8 in
`admin.rs`, ~1 in `rebuild_actor.rs` (swap step). The same applies
to `DELETE FROM fts_node_properties WHERE kind = ?` — becomes
`DELETE FROM fts_props_<kind>`.

The query path in `coordinator.rs` that currently targets
`bm25(fts_node_properties)` must be updated to use the per-kind table
name dynamically.

### Pack B: Per-column BM25 weights (depends on Pack A)

`FtsPropertyPathSpec` gains a `weight` builder method:

```rust
FtsPropertyPathSpec::scalar("$.title").weight(10.0)
FtsPropertyPathSpec::recursive("$.payload").weight(1.0)
// default weight: 1.0
```

Weights are stored in `fts_property_schemas.property_paths_json`
alongside the path string. At per-kind table creation time, paths
become FTS5 columns (one per `FtsPropertyPathSpec`). Recursive paths
use a single column for all their leaf content.

BM25 scoring in `coordinator.rs` is updated from:

```sql
ORDER BY bm25(fts_node_properties)
```

to:

```sql
ORDER BY bm25(fts_props_<kind>, <w1>, <w2>, ...)
```

where weights are read from the schema at query build time.

Weight validation at registration: `0.0 < weight <= 1000.0`. Error at
registration time if out of range, not at query time.

Column naming in the FTS5 table: sanitized path strings. The
sanitization rule (`$` → `p`, `.` → `_`, `[` → `_`, `]` → empty,
collapse underscores) must be in the same shared `fathomdb-schema`
function used for table name resolution. For the recursive path case
(`FtsPropertyPathSpec::recursive`), the column is named `payload_all`
(or the sanitized path of the recursive root + `_all`).

#### Non-goals for Pack B

- Per-leaf weights within a recursive path. One weight per
  `FtsPropertyPathSpec` entry.
- Runtime weight tuning without re-registering. Weights are part of
  the schema; changing them requires `register_fts_property_schema`
  (which triggers an async rebuild).

### Pack C: `matched_paths` attribution (independent)

`HitAttribution::matched_paths: Vec<String>` at
`crates/fathomdb-query/src/search.rs:68-79` is currently always `None`.

Work: at query execution time, after BM25 retrieval, join
`fts_node_property_positions` to find which paths contributed matching
tokens for each hit. Return the set of distinct `leaf_path` values as
`matched_paths`.

The join is a secondary lookup (not in the BM25 critical path) — run
it only for hits that are returned in the final result set, not for
all BM25 candidates.

Bindings: `SearchHit` in Python and TypeScript gains `matched_paths:
list[str] | None` / `matchedPaths: string[] | null`. `None`/`null`
when the hit came from the vector path (no position data) or when
attribution was not requested.

### Pack D: Item 10c + docs

- Add to `docs/reference/types.md` and `docs/guides/querying.md`:
  explicit statement that `SearchHit.snippet` format is unstable.
  Callers must not parse snippet substrings. Introduce a new structured
  field (`snippet_fields`) if stable snippet surface is needed in
  future — do not bless the current string format.
- Update `docs/guides/property-fts.md` to document per-column weights
  and the new per-kind table architecture.
- Changelog entry with **"Breaking change"** section for the
  `fts_node_properties` removal and **"New"** sections for 10a and 10b.

## Interface contract for 0.4.5

Pack A defines the interface the 0.4.5 implementer depends on:

| Symbol | Location | Contract |
|---|---|---|
| `fts_kind_table_name(kind)` | `fathomdb-schema` | Canonical kind → table name mapping |
| `resolve_fts_tokenizer(conn, kind)` | `fathomdb-schema` | Profile lookup with default fallback |
| `projection_profiles` table | schema v20 | Exists, empty after migration; `configure_fts` in 0.4.5 writes rows |
| `FtsPropertyPathSpec::weight(f32)` | `fathomdb-engine` | Path-level weight; serialized in `property_paths_json` |

The 0.4.5 implementer may begin Phase 2 (configuration persistence)
and Phase 3 (Python admin surface) work immediately. Phase 1 completion
(wiring tokenizer from profile into per-kind table creation) requires
Pack A to be merged first, but the function signature and lookup
contract are locked above.

## Test coverage required before ship

**Pack A:**
- Migration 20 from schema v19: existing kinds get per-kind tables
  created, `fts_property_rebuild_state` shows PENDING rows for each.
- Query during rebuild window returns JSON-scan fallback results, not
  empty results.
- After rebuild completes, per-kind FTS query returns correct results.
- `fts_node_properties` does not exist after migration.
- `fts_kind_table_name` round-trip: kinds with uppercase, hyphens,
  Unicode all produce valid SQLite identifiers.

**Pack B:**
- `register_fts_property_schema_with_entries` with explicit weights
  creates per-kind table with correct column count.
- BM25 scoring produces different orderings with non-unit weights
  (regression test: title-matching record outranks payload mention
  when `$.title` weight = 10.0).
- Weight out-of-range error at registration time.
- Schema re-registration with changed weights triggers async rebuild.

**Pack C:**
- `matched_paths` is non-null for property-FTS hits.
- `matched_paths` is null for vector-only hits.
- Paths returned are a subset of the registered schema paths.
- Cross-binding smoke tests (Python, TypeScript).

## Memex impact

On ship:
- Memex `m004_register_fts_property_schemas_v2.py` can add weight
  fields to the four hot kinds (`WMKnowledgeObject`, `WMExecutionRecord`,
  `WMAction`, `WMGoal`) — no required change, opt-in improvement.
- `matched_paths` available for reranking in `score_search_rows`.
- No caller changes required to maintain existing search behavior;
  the per-kind table migration is transparent to the public API.

## What 0.4.2 does not touch

- User-picked tokenizer selection (`configure_fts` CLI / `AdminClient`).
  The `projection_profiles` table ships in 0.4.2 but is empty; writing
  to it is a 0.4.5 feature.
- `matched_paths`-based reranking recipes in docs (0.4.5 guide).
- Python admin CLI (`fathomdb admin`). Existing `AdminClient` gains no
  new public methods in 0.4.2.
- Embedding adapter suite. All `QueryEmbedder` optimizations are 0.4.5.
- Write-priority / foreground-read isolation (item 11 / post-0.5.0).

## Release gating

1. All Pack A migration tests pass on a database upgraded from schema
   v19 (existing 0.4.1 data).
2. All Pack B weight tests pass; existing schema registrations without
   weights continue to work with implicit weight 1.0.
3. All Pack C matched_paths tests pass; Python and TypeScript bindings
   expose the field.
4. Pack D docs written: snippet instability note + per-kind weights
   guide + changelog.
5. No regression in existing `admin.rs` / `writer.rs` / `rebuild_actor.rs`
   tests.
6. `fts_kind_table_name` and `resolve_fts_tokenizer` exported from
   `fathomdb-schema` crate (not just `fathomdb-engine`) so 0.4.5 can
   call them from Python admin CLI path.
