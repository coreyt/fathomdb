use std::fmt::{Display, Formatter};
use std::time::Instant;

use rusqlite::Connection;

pub const SCHEMA_VERSION: u32 = 22;

/// SQLite `PRAGMA` name carrying the on-disk schema-version sentinel.
///
/// Public on-disk surface per `dev/interfaces/wire.md` § Schema-version
/// sentinel; advanced by successful migrations per `dev/design/migrations.md`.
pub const PRAGMA_USER_VERSION: &str = "user_version";

/// Suffix of the canonical SQLite database file (`<db-name>.sqlite`).
pub const SQLITE_SUFFIX: &str = ".sqlite";

/// Suffix of the SQLite write-ahead log file (`<db-name>.sqlite-wal`).
pub const WAL_SUFFIX: &str = "-wal";

/// Suffix of the sidecar lock file (`<db-name>.sqlite.lock`).
///
/// Per `dev/design/bindings.md` § 7, this sidecar flock is the load-bearing
/// cross-process exclusion layer; it surfaces lock contention before SQLite
/// I/O begins.
pub const LOCK_SUFFIX: &str = ".lock";

/// Suffix of the optional SQLite rollback journal file
/// (`<db-name>.sqlite-journal`).
pub const JOURNAL_SUFFIX: &str = "-journal";

#[must_use]
pub fn bootstrap_steps() -> &'static [&'static str] {
    &["create canonical tables", "register projection metadata", "seed rewrite-era configuration"]
}

/// Canonical tables owned by the rewrite-era schema, in stable display
/// order. Excludes FTS, vec0, and projection shadow tables (re-derivable
/// from canonical state) and internal `_fathomdb_*` metadata.
///
/// `doctor dump-row-counts` enumerates this set; `doctor dump-schema`
/// uses it to order canonical tables ahead of derived/internal ones.
pub const CANONICAL_TABLES: &[&str] = &[
    "canonical_nodes",
    "canonical_edges",
    "operational_collections",
    "operational_mutations",
    "operational_state",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration {
    pub step_id: u32,
    pub sql: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStepReport {
    pub step_id: u32,
    pub duration_ms: Option<u64>,
    pub failed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReport {
    pub schema_version_before: u32,
    pub schema_version_after: u32,
    pub migration_steps: Vec<MigrationStepReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationFailureReport {
    pub schema_version_before: u32,
    pub schema_version_current: u32,
    pub migration_steps: Vec<MigrationStepReport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    IncompatibleSchemaVersion { seen: u32, supported: u32 },
    MigrationError(MigrationFailureReport),
    Storage { message: &'static str },
}

impl Display for MigrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleSchemaVersion { seen, supported } => {
                write!(f, "database schema version {seen} is incompatible with supported version {supported}")
            }
            Self::MigrationError(report) => write!(
                f,
                "schema migration failed at step {}",
                report.migration_steps.last().map_or(0, |step| step.step_id)
            ),
            Self::Storage { message } => write!(f, "schema storage error: {message}"),
        }
    }
}

impl std::error::Error for MigrationError {}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        step_id: 1,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_schema_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    },
    Migration {
        step_id: 2,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_migrations(step_id INTEGER PRIMARY KEY, applied_at_ms INTEGER NOT NULL);
              CREATE TABLE IF NOT EXISTS canonical_nodes(write_cursor INTEGER NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS canonical_edges(write_cursor INTEGER NOT NULL, kind TEXT NOT NULL, from_id TEXT NOT NULL, to_id TEXT NOT NULL);",
    },
    Migration {
        step_id: 3,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_embedder_profiles(profile TEXT PRIMARY KEY, name TEXT NOT NULL, revision TEXT NOT NULL, dimension INTEGER NOT NULL)",
    },
    Migration {
        step_id: 4,
        sql: "CREATE TABLE IF NOT EXISTS operational_collections(
                  name TEXT PRIMARY KEY,
                  kind TEXT NOT NULL CHECK(kind IN ('append_only_log', 'latest_state')),
                  schema_json TEXT NOT NULL,
                  retention_json TEXT NOT NULL,
                  format_version INTEGER NOT NULL,
                  created_at INTEGER NOT NULL
              );
              CREATE TABLE IF NOT EXISTS operational_mutations(
                  id INTEGER PRIMARY KEY AUTOINCREMENT,
                  collection_name TEXT NOT NULL,
                  record_key TEXT NOT NULL,
                  op_kind TEXT NOT NULL CHECK(op_kind = 'append'),
                  payload_json TEXT NOT NULL,
                  schema_id TEXT,
                  write_cursor INTEGER NOT NULL
              );
              CREATE TABLE IF NOT EXISTS operational_state(
                  collection_name TEXT NOT NULL,
                  record_key TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  schema_id TEXT,
                  write_cursor INTEGER NOT NULL,
                  PRIMARY KEY(collection_name, record_key)
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_open_state(key TEXT PRIMARY KEY, value TEXT NOT NULL);
              INSERT OR IGNORE INTO operational_collections(
                  name, kind, schema_json, retention_json, format_version, created_at
              ) VALUES (
                  'projection_failures',
                  'append_only_log',
                  '{\"type\":\"object\"}',
                  '{}',
                  1,
                  0
              );",
    },
    Migration {
        step_id: 5,
        sql: "CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
                  body,
                  kind UNINDEXED,
                  write_cursor UNINDEXED
              );",
    },
    Migration {
        step_id: 6,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_projection_state(
                  kind TEXT PRIMARY KEY,
                  last_enqueued_cursor INTEGER NOT NULL DEFAULT 0,
                  updated_at INTEGER NOT NULL DEFAULT 0
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_vector_kinds(
                  kind TEXT PRIMARY KEY,
                  profile TEXT NOT NULL,
                  created_at INTEGER NOT NULL DEFAULT 0
              );
              CREATE TABLE IF NOT EXISTS _fathomdb_vector_rows(
                  rowid INTEGER PRIMARY KEY,
                  kind TEXT NOT NULL,
                  write_cursor INTEGER NOT NULL UNIQUE
              );",
    },
    Migration {
        step_id: 7,
        sql: "CREATE TABLE IF NOT EXISTS _fathomdb_projection_terminal(
                  write_cursor INTEGER PRIMARY KEY,
                  state TEXT NOT NULL CHECK(state IN ('failed', 'up_to_date'))
              );",
    },
    // Phase 9 Pack B — REQ-026 / AC-028a/b/c / AC-042 recovery seam.
    // `source_id` is nullable; existing canonical rows back-fill to NULL,
    // so reads from older callers stay schema-stable. REQ-045 accretion
    // offset is documented in `migrations/008_source_id.sql` as inherently
    // impossible for this slice (every existing canonical column is
    // load-bearing for replay / projections / recovery locators); the
    // next schema-touching pack carries the offset budget for two adds.
    Migration {
        step_id: 8,
        sql: "ALTER TABLE canonical_nodes ADD COLUMN source_id TEXT;
              ALTER TABLE canonical_edges ADD COLUMN source_id TEXT;
              CREATE INDEX IF NOT EXISTS canonical_nodes_source_id_idx
                  ON canonical_nodes(source_id);
              CREATE INDEX IF NOT EXISTS canonical_edges_source_id_idx
                  ON canonical_edges(source_id);",
    },
    // 0.7.0 Pack 1 — Vector binary-quantization data encoding.
    // Per `dev/design/0.7.0-vector-quant-pack1.md` D4 (fix-3). Stages
    // the existing f32 corpus + kind mapping into a regular SQL table,
    // drops + recreates `vector_default` with the new schema (sibling
    // `embedding_bin bit[768]`, `source_type TEXT partition key`,
    // `kind TEXT`, `created_at INTEGER`), then repopulates with
    // SQL-side `vec_quantize_binary` and the D3 CASE mapping. A
    // prefix CHECK-constraint preflight aborts the migration if any
    // `_fathomdb_vector_rows.kind` is outside the locked vocabulary.
    //
    // `<dim>=768` is hardcoded against the default profile
    // (`load_default_profile` -> `DEFAULT_EMBEDDER_DIMENSION` in
    // fathomdb-engine). The design notes this constraint and defers
    // a runtime-dim migration to 0.7.1.
    Migration {
        step_id: 9,
        // SQL-side: D4 fix-3.1 preflight only. The vec0 reshape itself is
        // dim-aware and lives in the engine crate's
        // `ensure_vector_partition_pack1` (called by `ensure_vector_partition`
        // immediately after `migrate_with_event_sink` returns). Splitting the
        // preflight (SQL, in-tx with `apply_one`) from the reshape (Rust,
        // dim-driven by `_fathomdb_embedder_profiles.dimension`) is required
        // because `fathomdb-schema::Migration` is a `&'static str` with no
        // runtime parameterization, and the existing dim=8 / dim=384 test
        // suite must stay GREEN. The reshape is idempotent across crashes:
        // if open fails between this step's commit (user_version=9) and the
        // Rust reshape, the next open re-detects the old shape and replays
        // the reshape. See dev/plans/runs/0.7.0-PVQ-P1-IMPL-output.json
        // for the design-memo deviation note.
        sql: "CREATE TEMP TABLE _vec0_migration_assertion(
                  check_passes INTEGER NOT NULL CHECK(check_passes = 1)
              );
              INSERT INTO _vec0_migration_assertion(check_passes)
                  SELECT CASE WHEN EXISTS (
                      SELECT 1 FROM _fathomdb_vector_rows
                      WHERE kind NOT IN ('email','article','paper','meeting','note','todo','doc')
                  ) THEN 0 ELSE 1 END;
              DROP TABLE _vec0_migration_assertion;",
    },
    // 0.7.1 EU-5a2 — mean-centering schema column.
    // Per `dev/design/embedder.md` §0.2: nullable BLOB holding the
    // pinned per-workspace mean vector for the default profile. Byte
    // length, when non-NULL, MUST equal `4 * dimension` (f32 little-endian).
    // Pure additive ALTER; SQLite stores NULL for the pre-existing row.
    // Lifecycle (compute-once-on-first-ingest threshold-pin) is in the
    // engine crate, not the schema layer.
    Migration {
        step_id: 10,
        sql: "ALTER TABLE _fathomdb_embedder_profiles ADD COLUMN mean_vec BLOB",
    },
    // 0.8.0 Slice 5 (G1) — global FTS5 tokenizer-default upgrade.
    // Per `dev/plans/0.8.0-implementation.md` § "Slice 5" and the design
    // memo `dev/design/0.8.0-slice-5-G1-design.md`. Migrations are
    // forward-only and immutable, and FTS5 has no `ALTER … tokenize`, so the
    // tokenizer default is upgraded by dropping and recreating the
    // `search_index` virtual table rather than editing the step-5 DDL (which
    // would change the tokenizer for new DBs only). The drop+recreate leaves
    // the FTS index empty on a migrated DB; the engine re-tokenizes from the
    // canonical source rows immediately after this step lands (open path,
    // `reproject_search_index_after_tokenizer_upgrade`) — projection-only, no
    // source-record migration. `DROP TABLE` already satisfies the accretion
    // guard's `names_removal` branch; the exemption marker is carried to
    // document intent and match the established pattern.
    Migration {
        step_id: 11,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: tokenizer-default upgrade (drop+recreate FTS5 projection; no source-record migration)
              DROP TABLE IF EXISTS search_index;
              CREATE VIRTUAL TABLE search_index USING fts5(
                  body,
                  kind UNINDEXED,
                  write_cursor UNINDEXED,
                  tokenize = 'porter unicode61 remove_diacritics 2'
              );",
    },
    // 0.8.0 Slice 15 (G0 KEYSTONE) — transaction-time canonical-identity
    // substrate. Per `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`
    // (SIGNED 2026-06-03) and `dev/design/slice-15-g0-design.md`. Two additive
    // nullable columns on BOTH canonical tables: `logical_id TEXT` (stable
    // cross-re-ingestion identity; NULL = legacy/own-identity row) and
    // `superseded_at INTEGER` (transaction-time tombstone; NULL = active row).
    // A partial UNIQUE INDEX `(logical_id) WHERE superseded_at IS NULL` per table
    // enforces one active version per logical id — scoped to `logical_id` ALONE
    // (Decision 5, HITL-SIGNED 2026-06-05; `kind` is payload/classification on
    // nodes and relationship-type on edges, NEVER an identity-scope component).
    // NULL-safe, so the many legacy NULL-logical_id rows never collide (SQLite
    // treats each NULL as distinct; load-bearing). The folded G4/G5 read indexes
    // (`canonical_nodes(kind)`, `canonical_edges(from_id)/(to_id)`) ride this one
    // accretion offset budget. Pure additive ALTER (no DROP) → the exemption
    // marker is REQUIRED (the accretion guard rejects ADD COLUMN without it);
    // legacy rows read NULL with no data migration / re-open (in-place ALTER).
    // Step-12 amended IN PLACE (Slice 31, no SCHEMA_VERSION bump): already-migrated
    // local v12 DBs keep the old compound index until rebuilt (HITL: disposable).
    Migration {
        step_id: 12,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: G0 transaction-time identity substrate
              ALTER TABLE canonical_nodes ADD COLUMN logical_id TEXT;
              ALTER TABLE canonical_nodes ADD COLUMN superseded_at INTEGER;
              ALTER TABLE canonical_edges ADD COLUMN logical_id TEXT;
              ALTER TABLE canonical_edges ADD COLUMN superseded_at INTEGER;
              CREATE UNIQUE INDEX IF NOT EXISTS canonical_nodes_logical_active_idx
                  ON canonical_nodes(logical_id) WHERE superseded_at IS NULL;
              CREATE UNIQUE INDEX IF NOT EXISTS canonical_edges_logical_active_idx
                  ON canonical_edges(logical_id) WHERE superseded_at IS NULL;
              CREATE INDEX IF NOT EXISTS canonical_nodes_kind_idx
                  ON canonical_nodes(kind);
              CREATE INDEX IF NOT EXISTS canonical_edges_from_id_idx
                  ON canonical_edges(from_id);
              CREATE INDEX IF NOT EXISTS canonical_edges_to_id_idx
                  ON canonical_edges(to_id);",
    },
    // 0.8.0 Slice 33 (G3 / F4-READ) — op-store paginated read-back hardening.
    // Per `dev/design/slice-33-cursor-hardening-design.md`. The governed
    // `read.collection` / `read.mutations` SELECT is
    // `WHERE collection_name = ?1 AND id > ?2 ORDER BY id LIMIT ?3`. Without an
    // index on `collection_name`, SQLite rides the `id` PRIMARY KEY (EXPLAIN:
    // `SEARCH … USING INTEGER PRIMARY KEY (rowid>?)`), scanning the id-ordered
    // log and filtering `collection_name` row-by-row — O(rows-scanned) for a
    // small collection inside a large multi-collection log. The composite
    // `(collection_name, id)` index makes the plan index-driven (EXPLAIN:
    // `SEARCH … USING INDEX operational_mutations_collection_id_idx
    // (collection_name=? AND id>?)`): the leading equality on `collection_name`
    // fixes the prefix, the trailing `id` serves BOTH the after-id cursor range
    // and `ORDER BY id` with no temp B-tree — O(page). Pure additive
    // `CREATE INDEX` (no table/column add, no DROP, no table reshape), so the
    // accretion guard does not flag it and no exemption marker is required.
    Migration {
        step_id: 13,
        sql: "CREATE INDEX IF NOT EXISTS operational_mutations_collection_id_idx
                  ON operational_mutations(collection_name, id);",
    },
    // 0.8.1 Slice 15 (G11) — fact-on-edge enrichment + edge projectability.
    // Per `dev/adr/ADR-0.8.1-graph-substrate-g11-migration.md` (HITL-SIGNED
    // 2026-06-13). Five additive nullable columns on `canonical_edges`:
    //   `body`              — the fact/relationship text for FTS + vector projection
    //   `t_valid`           — event valid-time (ISO-8601); NULL = "still valid"
    //   `t_invalid`         — event invalid-time (ISO-8601); NULL = "still valid"
    //   `confidence`        — extraction confidence ∈ [0.0, 1.0] from the harness
    //   `extractor_model_id`— opaque model id from BYO-LLM harness `ready.model`
    // All five are nullable; pre-G11 rows read NULL (no data migration required).
    // Also creates `search_index_edges` FTS5 virtual table for edge-body FTS
    // projection (Option B: separate table, no modification to the existing
    // `search_index` path). MIGRATION-ACCRETION-EXEMPTION required for ADD COLUMN.
    Migration {
        step_id: 14,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: G11 edge enrichment (5 additive nullable columns + edge FTS table)
              ALTER TABLE canonical_edges ADD COLUMN body TEXT;
              ALTER TABLE canonical_edges ADD COLUMN t_valid TEXT;
              ALTER TABLE canonical_edges ADD COLUMN t_invalid TEXT;
              ALTER TABLE canonical_edges ADD COLUMN confidence REAL;
              ALTER TABLE canonical_edges ADD COLUMN extractor_model_id TEXT;
              CREATE VIRTUAL TABLE IF NOT EXISTS search_index_edges USING fts5(
                  body,
                  kind UNINDEXED,
                  write_cursor UNINDEXED,
                  tokenize = 'porter unicode61 remove_diacritics 2'
              );",
    },
    // 0.8.1 Slice 30 (R3) SCHEMA-GATE-1 — temporal_fallback provenance flag.
    // HITL-SIGNED 2026-06-13: approved additive schema bump.
    // Edges whose `t_valid` was defaulted to `created_at` by the ELPS extractor
    // (not text-grounded) carry this flag so the graph-arm BFS can exclude them
    // from temporal queries. NULL = not a fallback (pre-column rows and edges
    // written without the flag are treated as NOT temporal_fallback — safe default
    // since they were written before provenance tracking existed or via a direct
    // write where the caller owns the t_valid).
    // MIGRATION-ACCRETION-EXEMPTION required for ADD COLUMN.
    Migration {
        step_id: 15,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: R3 temporal_fallback provenance flag (additive nullable BOOLEAN column)
              ALTER TABLE canonical_edges ADD COLUMN temporal_fallback INTEGER;",
    },
    // 0.8.14 Slice 5 (EXP-S KEYSTONE) — kind-tagged coexisting-index substrate.
    // Per `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` D1
    // (RATIFIED 2026-07-03) and `dev/plans/plan-0.8.14.md` §2 (R-SUB-1/R-SUB-3).
    // Adds `row_kind` — a SEPARATE structural-role axis on `canonical_nodes`,
    // orthogonal to the doc-type `kind` (email/article/paper/meeting/note/todo/
    // doc/edge_fact). Vocabulary: `leaf` (normal record — the DEFAULT, which
    // preserves current behavior for every existing/normal row), `coverage`
    // (coverage/summary rows), `graph` (graph structural rows). D1 is explicit:
    // this must NOT overload the doc-type `kind` vocabulary or touch its three
    // hard-locked sites (engine `resolve_source_type` / `KIND_TO_SOURCE_TYPE_CASE_SQL`
    // / this crate's migration-9 preflight CHECK). NOT NULL DEFAULT 'leaf' is a
    // constant default, so pre-existing rows back-fill to `leaf` in-place (no data
    // migration / re-open) and the migration is forward-only. Additive ADD COLUMN
    // (no DROP) → the accretion guard REQUIRES the exemption marker. No vec0
    // embedding/quant/pooling change (ADR §D6): this step does NOT rewrite vec0
    // rows, so the eu7 fidelity gate stays a documented no-op at Slice 20.
    Migration {
        step_id: 16,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: EXP-S row_kind structural-role tag (additive NOT NULL DEFAULT 'leaf' column; separate axis from doc-type kind)
              ALTER TABLE canonical_nodes ADD COLUMN row_kind TEXT NOT NULL DEFAULT 'leaf';",
    },
    // 0.8.14 Slice 10 (F5 — fielded FTS / BM25F) — multi-column FTS5 index.
    // Per `dev/adr/ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md` §3.1 and
    // `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` §D4
    // (RATIFIED 2026-07-03; F5 co-lands by the §D8 HITL override) and
    // `dev/plans/plan-0.8.14.md` §2 (R-F5-1 / R-SUB-3). Creates a NEW FTS5 virtual
    // table `search_index_v2` over the doc-type node fields `kind` / `body` /
    // `status`, so a BM25F query can weight each field independently
    // (`bm25(search_index_v2, W_kind, W_body, W_status)`), riding the EXP-S
    // substrate. This is ADDITIVE and coexists with the single-column body-only
    // `search_index` (which is RETAINED, byte-unchanged — the existing RRF/lexical
    // query path keeps using it, so its determinism pins are untouched): the new
    // table is a second coexisting index in the "one store, many indexes"
    // substrate, exactly like `search_index_edges` (step 14, Option B). FTS5 has
    // no in-place column-add, so BM25F requires a new virtual table + an O(N)
    // re-index; the co-land with step 16 means an old DB pays ONE re-index window
    // (`SCHEMA_VERSION` 15 -> 17 in one open). The `status` field is derived from
    // the JSON body's `$.status`, guarded by `json_valid` so non-JSON bodies
    // index an empty status; this is F5's own `$.status`-derived field, NOT
    // the value the shipped G10 SearchFilter reads (G10 reads vec0 `status`,
    // still the empty sentinel). The
    // `write_cursor` UNINDEXED column mirrors `search_index` for the
    // canonical-row join (rowid==write_cursor identity is preserved by the
    // engine write path; the vec0 corpus is NOT touched, so the eu7 fidelity gate
    // stays a documented no-op at Slice 20 — ADR-0.8.14 §D6). `CREATE VIRTUAL
    // TABLE` does not trip the accretion guard (it fires only on `CREATE TABLE` /
    // `ADD COLUMN`), but the exemption marker is carried to document the additive
    // re-index intent and match the step-11/step-14 virtual-table precedent.
    Migration {
        step_id: 17,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: F5 fielded FTS (new multi-column search_index_v2 FTS5 table + O(N) re-index; search_index retained)
              CREATE VIRTUAL TABLE IF NOT EXISTS search_index_v2 USING fts5(
                  kind,
                  body,
                  status,
                  write_cursor UNINDEXED,
                  tokenize = 'porter unicode61 remove_diacritics 2'
              );
              INSERT INTO search_index_v2(kind, body, status, write_cursor)
                  SELECT
                      kind,
                      body,
                      CASE WHEN json_valid(body)
                           THEN COALESCE(json_extract(body, '$.status'), '')
                           ELSE '' END,
                      write_cursor
                  FROM canonical_nodes;",
    },
    // 0.8.16 Slice 5 (F9 KEYSTONE) — node-level importance ranking scalar.
    // Per `dev/adr/ADR-0.8.16-f9-importance-confidence-ranking.md` §2.1
    // (SIGNED 2026-07-08) and `dev/plans/plan-0.8.16.md` §2 (R-F9-1/R-F9-4).
    // Adds `importance REAL` on `canonical_nodes` — a caller-supplied ranking
    // scalar, symmetric with the existing genuine-NULL `canonical_edges.confidence`
    // (step-14). 3-way sentinel (frozen): `NULL` = never assigned (graceful-absent,
    // ranks NEUTRAL — the OPP-12 Q6a graceful-absent state, load-bearing for
    // R-F9-4); `0.0` = explicit floor/de-weight; `(0.0, 1.0]` = explicit importance.
    // Nullable, so pre-existing rows read NULL in-place (no data migration / re-open):
    // the graceful-absent default preserves current ranking for every existing row.
    // Additive `ADD COLUMN` (no DROP) → the accretion guard REQUIRES the exemption
    // marker. This step does NOT rewrite vec0 / vector rows (ADR §4 eu7 no-op basis):
    // it adds a scalar column only, so the eu7 fidelity gate stays a documented no-op.
    Migration {
        step_id: 18,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: F9 importance ranking scalar (additive nullable REAL; 3-way sentinel, NULL=graceful-absent)
              ALTER TABLE canonical_nodes ADD COLUMN importance REAL;",
    },
    // 0.8.18 Slice 5 (#5 vector-equivalence probe KEYSTONE) — the
    // `_fathomdb_embed_probe` self-check substrate. Per
    // `dev/adr/ADR-0.8.18-vector-equivalence-self-check.md` (SIGNED 2026-07-09)
    // and `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md` §U1
    // (R-VEQ-1). Creates a new internal table holding the 45 committed
    // equivalence probes, each with its **UN-centered f32 reference vector**
    // (`4 * dim` little-endian bytes) and the embedder identity that produced it.
    // The engine populates the 45 rows at first vector-kind registration (open
    // path, adjacent to `ensure_vector_partition`); this migration only creates
    // the empty table. **Store f32 ONLY — the Phase-1 mean-centered bits are
    // NEVER persisted** (they are recomputed at check time from the un-centered
    // reference + the live pinned `mean_vec`, U1-d). This step does NOT rewrite
    // vec0 / vector rows (eu7 no-op basis): it creates a fresh sidecar table only,
    // so the eu7 fidelity gate stays a documented no-op. `CREATE TABLE` adds
    // schema (no DROP) → the accretion guard REQUIRES the exemption marker.
    Migration {
        step_id: 19,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: #5 vector-equivalence probe substrate (new internal _fathomdb_embed_probe table; UN-centered f32 references only, NEVER persists P1 bits)
              CREATE TABLE IF NOT EXISTS _fathomdb_embed_probe(
                  probe_ordinal INTEGER PRIMARY KEY,
                  probe_text TEXT NOT NULL,
                  reference_vec BLOB NOT NULL,
                  embedder_name TEXT NOT NULL,
                  embedder_revision TEXT NOT NULL,
                  dim INTEGER NOT NULL
              );",
    },
    // 0.8.19 Slice 5 (OPP-12 record-lifecycle Phase-1 KEYSTONE) — the existence
    // axis. Per `dev/design/0.8.19-slice-0-opp12-phase1-design.md` §5 (the ONE
    // 19→20 migration) and `dev/plans/plan-0.8.19.md` §2 (R-EX-1/R-MIG-1). Adds
    // the two existence columns on `canonical_nodes`:
    //   `state`  — the `LifecycleState` enum, stored as TEXT. `NOT NULL DEFAULT
    //              'active'` so EVERY pre-existing row back-fills to `active`
    //              in-place (no data migration / re-open); the shipped corpus is
    //              wholly `active`, so the new default-read exclusion
    //              (`AND state = 'active'` co-located with `superseded_at IS NULL`
    //              at each retrieval site) is a documented NO-OP on it (eu7 no-op
    //              basis, design §9).
    //   `reason` — nullable advisory cause for the CURRENT state (quarantine cause
    //              for `pending`; delete cause for the delete-family). Engine never
    //              interprets it.
    // Plus `canonical_nodes_state_active_idx` — a PARTIAL index over active rows
    // keyed by `write_cursor` (the dominant retrieval/join key), serving the
    // active-only default-read hot path.
    // Scoped per F-23 ruling 1a: existence columns ONLY — NO surrogate-`logical_id`
    // backfill (anonymous rows keep `logical_id = NULL`; surrogate minting is
    // Phase-2/0.8.20). One migration per release (I-6). This step does NOT rewrite
    // vec0 / vector rows (eu7 no-op basis). Additive `ADD COLUMN` (no DROP) → the
    // accretion guard REQUIRES the exemption marker.
    Migration {
        step_id: 20,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: OPP-12 Phase-1 existence axis (state NOT NULL DEFAULT 'active' + nullable reason on canonical_nodes + active-only partial index; no surrogate backfill — F-23 ruling 1a)
              ALTER TABLE canonical_nodes ADD COLUMN state TEXT NOT NULL DEFAULT 'active';
              ALTER TABLE canonical_nodes ADD COLUMN reason TEXT;
              CREATE INDEX IF NOT EXISTS canonical_nodes_state_active_idx
                  ON canonical_nodes(write_cursor) WHERE state = 'active';",
    },
    // Step 21 (0.8.20 Slice 5c) — legacy provenance backfill, per
    // `dev/design/0.8.20-slice0-erasure-design.md` §4 work item 7 and
    // `dev/plans/plan-0.8.20.md` R-20-E8.
    //
    // Erasure runs through provenance: `excise_source` addresses rows BY
    // `source_id`, so a stored row with `source_id IS NULL` is reachable by no
    // erasure call at all — it is un-erasable. Pre-0.8.20 the public write type
    // carried `source_id: Option<String>` and a `None` landed NULL, so shipped
    // databases hold such rows. R-20-E3 closes the write path going forward
    // (`SourceId` makes the absence inexpressible); this step repairs the rows
    // already on disk by stamping them with the reserved
    // `_legacy:pre-0.8.20`, after which an operator can erase them.
    //
    // THE GATE IS EXACT, LOAD-BEARING AND **NODE-ONLY**: on `canonical_nodes`
    // the predicate is `WHERE source_id IS NULL AND logical_id IS NULL`; on
    // `canonical_edges` it is `WHERE source_id IS NULL` alone. The asymmetry is
    // deliberate, and the reason is that the gate's rationale holds for one
    // table and not the other.
    //
    // The rationale comes from the TC-11 pin (CLOSED): a GOVERNED row — one
    // carrying a `logical_id` — is addressable in its own right, because `purge`
    // reaches it BY `logical_id`. Stamping it with a shared `_legacy:`
    // provenance would make it collateral of an
    // `excise_source('_legacy:pre-0.8.20')` call aimed at anonymous rows, which
    // is precisely the over-erasure the pin forbids. That argument is sound FOR
    // NODES: governed nodes keep NULL `source_id` by design, and that is not a
    // gap.
    //
    // It is FALSE FOR EDGES. `purge` resolves its lifecycle target exclusively
    // through `canonical_nodes` (`SELECT state FROM canonical_nodes WHERE
    // logical_id = ?1 AND superseded_at IS NULL`) and then erases edges by
    // ENDPOINT (`from_id`/`to_id`) — it never resolves an edge by edge
    // `logical_id`. An edge `logical_id` is only a SUPERSESSION identity; it
    // confers no purge-addressability whatsoever. Applying the node gate to
    // edges therefore left legacy edges with `source_id IS NULL AND logical_id
    // IS NOT NULL` skipped by this backfill (⇒ unreachable by
    // `excise_source`/`erase_source`) AND not purge-addressable (⇒ unreachable
    // by `purge`), so they were erasable by NO verb and could only disappear
    // incidentally when a connected node happened to be purged. That defeats
    // R-20-E8, whose entire purpose is that legacy NULL-provenance rows become
    // erasable. (codex §9 P1; `legacy_backfill_covers_governed_edges`.)
    //
    // Back-filling an edge's `source_id` does NOT touch the TC-11 pin: the pin
    // forbids populating `logical_id` on an existing row and forbids re-deriving
    // a stored row's id-space, and this writes neither.
    //
    // The pin's enforcing invariant is also respected: this statement READS
    // `logical_id` as its predicate and NEVER writes one. No row transitions
    // `logical_id` NULL -> NOT NULL, and no stored row's id-space is re-derived
    // (`s21_backfill_populates_no_logical_id` asserts both).
    //
    // Rows that already carry provenance are untouched (`source_id IS NULL`
    // half of the predicate), so caller-supplied ids are never overwritten.
    //
    // No accretion exemption marker: this is a pure data `UPDATE` with no
    // `CREATE TABLE` / `ADD COLUMN`, so the guard does not fire (cf. step 13).
    // One migration per release (I-6).
    Migration {
        step_id: 21,
        sql: "UPDATE canonical_nodes
                 SET source_id = '_legacy:pre-0.8.20'
               WHERE source_id IS NULL AND logical_id IS NULL;
              UPDATE canonical_edges
                 SET source_id = '_legacy:pre-0.8.20'
               WHERE source_id IS NULL;",
    },
    // Step 22 (0.8.20 Slice 10b) — R-20-NV node validity window, per
    // `dev/plans/plan-0.8.20.md` §3 (R-20-NV). Adds the two world-time validity
    // columns on `canonical_nodes`:
    //   `valid_from`  — inclusive lower bound of the window.
    //   `valid_until` — EXCLUSIVE upper bound of the window.
    // The interval is HALF-OPEN: `[valid_from, valid_until)`. A node is valid at
    // instant `t` iff `(valid_from IS NULL OR valid_from <= t) AND (valid_until
    // IS NULL OR valid_until > t)`. NULL means UNBOUNDED on that side, so
    // NULL/NULL is "valid for all time". This convention is stated once here and
    // is the same one `ReadView::valid_as_of` compiles to at every read site.
    //
    // **UNITS: INTEGER epoch SECONDS (UTC).** This DELIBERATELY DIVERGES from
    // `canonical_edges.t_valid`/`t_invalid` (step 14), which are ISO-8601 TEXT
    // compared through `datetime(...)`. The divergence is intentional and the
    // edge columns are NOT changed and NOT unified with these:
    //   (a) the release contract for R-20-NV specifies INTEGER windows;
    //   (b) INTEGER windows are directly comparable/indexable with no `datetime()`
    //       conversion per row, so the validity conjunct stays sargable against
    //       `canonical_nodes_validity_idx`;
    //   (c) the node-validity instant is a BOUND PARAMETER (`:now` seam), never a
    //       `datetime('now')` SQL literal, so node validity is deterministically
    //       testable — whereas the shipped EDGE path still inlines
    //       `datetime('now')`. Unifying the two would either re-litigate the
    //       shipped edge semantics or force a data rewrite of `canonical_edges`,
    //       both out of scope for this slice.
    // The divergence is a KNOWN, FLAGGED inconsistency in the temporal model and
    // is escalated with this slice rather than silently resolved.
    //
    // Existing rows get NULL/NULL on both columns (SQLite `ADD COLUMN` with no
    // DEFAULT back-fills NULL in place, no table rewrite), i.e. unbounded ⇒
    // always valid ⇒ EVERY pre-existing row's default-view visibility is
    // UNCHANGED (asserted by `s22_preexisting_rows_stay_visible_in_default_view`
    // and, at the engine level, by the R-20-NV suite). This step does NOT rewrite
    // vec0 / vector rows (eu7 no-op basis).
    //
    // Crash-safety + idempotence come from the runner, exactly as for step 20:
    // `apply_one` wraps the batch AND the `PRAGMA user_version` bump in a single
    // `BEGIN IMMEDIATE`/`COMMIT`, so a crash mid-step rolls back to 21 and the
    // step re-runs whole; and `migrate_with_event_sink` only applies steps with
    // `step_id > user_version`, so a completed step never re-runs (which matters
    // because `ALTER TABLE ... ADD COLUMN` has no `IF NOT EXISTS` form).
    // One migration per release (I-6). Additive `ADD COLUMN` (no DROP) → the
    // accretion guard REQUIRES the exemption marker.
    Migration {
        step_id: 22,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: R-20-NV node validity window (valid_from/valid_until INTEGER epoch-seconds on canonical_nodes; NULL = unbounded; half-open [valid_from, valid_until); deliberately INTEGER, diverging from the ISO-8601 TEXT canonical_edges.t_valid/t_invalid, which are unchanged)
              ALTER TABLE canonical_nodes ADD COLUMN valid_from INTEGER;
              ALTER TABLE canonical_nodes ADD COLUMN valid_until INTEGER;
              CREATE INDEX IF NOT EXISTS canonical_nodes_validity_idx
                  ON canonical_nodes(valid_from, valid_until)
                  WHERE superseded_at IS NULL AND state = 'active';",
    },
];

pub fn migrate(conn: &Connection) -> Result<MigrationReport, MigrationError> {
    migrate_with_steps(conn, MIGRATIONS)
}

pub fn migrate_with_steps(
    conn: &Connection,
    migrations: &[Migration],
) -> Result<MigrationReport, MigrationError> {
    migrate_with_event_sink(conn, migrations, |_| {})
}

pub fn migrate_with_event_sink(
    conn: &Connection,
    migrations: &[Migration],
    mut emit: impl FnMut(&MigrationStepReport),
) -> Result<MigrationReport, MigrationError> {
    let before = user_version(conn)?;
    if before > SCHEMA_VERSION {
        return Err(MigrationError::IncompatibleSchemaVersion {
            seen: before,
            supported: SCHEMA_VERSION,
        });
    }

    let mut current = before;
    let mut reports = Vec::new();

    for migration in migrations.iter().filter(|migration| migration.step_id > before) {
        if migration.step_id != current.saturating_add(1) {
            return Err(MigrationError::Storage {
                message: "migration registry is not contiguous",
            });
        }

        let started = Instant::now();
        if let Err(_err) = apply_one(conn, migration) {
            reports.push(MigrationStepReport {
                step_id: migration.step_id,
                duration_ms: Some(duration_ms(started)),
                failed: true,
            });
            emit(reports.last().expect("failed step report was just pushed"));
            let schema_version_current = user_version(conn).unwrap_or(current);
            return Err(MigrationError::MigrationError(MigrationFailureReport {
                schema_version_before: before,
                schema_version_current,
                migration_steps: reports,
            }));
        }

        current = migration.step_id;
        reports.push(MigrationStepReport {
            step_id: migration.step_id,
            duration_ms: Some(duration_ms(started)),
            failed: false,
        });
        emit(reports.last().expect("successful step report was just pushed"));
    }

    Ok(MigrationReport {
        schema_version_before: before,
        schema_version_after: user_version(conn)?,
        migration_steps: reports,
    })
}

fn apply_one(conn: &Connection, migration: &Migration) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        conn.execute_batch(migration.sql)?;
        conn.pragma_update(None, PRAGMA_USER_VERSION, migration.step_id)?;
        Ok(())
    })();

    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

fn user_version(conn: &Connection) -> Result<u32, MigrationError> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .map_err(|_| MigrationError::Storage { message: "could not read schema version" })
}

fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationAccretionError {
    pub offender: String,
}

impl Display for MigrationAccretionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "migration accretion guard rejected {}", self.offender)
    }
}

impl std::error::Error for MigrationAccretionError {}

pub fn check_migration_accretion(name: &str, sql: &str) -> Result<(), MigrationAccretionError> {
    let upper = sql.to_ascii_uppercase();
    let adds_schema = upper.contains("CREATE TABLE") || upper.contains("ADD COLUMN");
    let names_removal = upper.contains("DROP TABLE") || upper.contains("DROP COLUMN");
    let has_exemption = sql.contains("-- MIGRATION-ACCRETION-EXEMPTION: ");

    if adds_schema && !names_removal && !has_exemption {
        return Err(MigrationAccretionError { offender: name.to_string() });
    }

    Ok(())
}
