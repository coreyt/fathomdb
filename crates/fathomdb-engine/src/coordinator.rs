use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use fathomdb_query::{
    BindValue, ComparisonOp, CompiledGroupedQuery, CompiledQuery, CompiledRetrievalPlan,
    CompiledSearch, CompiledSearchPlan, CompiledVectorSearch, DrivingTable, ExpansionSlot,
    FALLBACK_TRIGGER_K, HitAttribution, Predicate, RetrievalModality, ScalarValue, SearchBranch,
    SearchHit, SearchHitSource, SearchMatchMode, SearchRows, ShapeHash, render_text_query_fts5,
};
use fathomdb_schema::SchemaManager;
use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Value};

use crate::embedder::QueryEmbedder;
use crate::telemetry::{SqliteCacheStatus, TelemetryCounters, read_db_cache_status};
use crate::{EngineError, sqlite};

/// Maximum number of cached shape-hash to SQL mappings before the cache is
/// cleared entirely.  A clear-all strategy is simpler than partial eviction
/// and the cost of re-compiling on a miss is negligible.
const MAX_SHAPE_CACHE_SIZE: usize = 4096;

/// Maximum number of root IDs per batched expansion query.  Kept well below
/// `SQLITE_MAX_VARIABLE_NUMBER` (default 999) because each batch also binds
/// the edge-kind parameter.  Larger root sets are chunked into multiple
/// batches of this size rather than falling back to per-root queries.
const BATCH_CHUNK_SIZE: usize = 200;

/// A pool of read-only `SQLite` connections for concurrent read access.
///
/// Each connection is wrapped in its own [`Mutex`] so multiple readers can
/// proceed in parallel when they happen to grab different slots.
struct ReadPool {
    connections: Vec<Mutex<Connection>>,
}

impl fmt::Debug for ReadPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadPool")
            .field("size", &self.connections.len())
            .finish()
    }
}

impl ReadPool {
    /// Open `pool_size` read-only connections to the database at `path`.
    ///
    /// Each connection has PRAGMAs initialized via
    /// [`SchemaManager::initialize_connection`] and, when the `sqlite-vec`
    /// feature is enabled and `vector_enabled` is true, the vec extension
    /// auto-loaded.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if any connection fails to open or initialize.
    fn new(
        db_path: &Path,
        pool_size: usize,
        schema_manager: &SchemaManager,
        vector_enabled: bool,
    ) -> Result<Self, EngineError> {
        let mut connections = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let conn = if vector_enabled {
                #[cfg(feature = "sqlite-vec")]
                {
                    sqlite::open_readonly_connection_with_vec(db_path)?
                }
                #[cfg(not(feature = "sqlite-vec"))]
                {
                    sqlite::open_readonly_connection(db_path)?
                }
            } else {
                sqlite::open_readonly_connection(db_path)?
            };
            schema_manager
                .initialize_reader_connection(&conn)
                .map_err(EngineError::Schema)?;
            connections.push(Mutex::new(conn));
        }
        Ok(Self { connections })
    }

    /// Acquire a connection from the pool.
    ///
    /// Tries [`Mutex::try_lock`] on each slot first (fast non-blocking path).
    /// If every slot is held, falls back to a blocking lock on the first slot.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Bridge`] if the underlying mutex is poisoned.
    fn acquire(&self) -> Result<MutexGuard<'_, Connection>, EngineError> {
        // Fast path: try each connection without blocking.
        for conn in &self.connections {
            if let Ok(guard) = conn.try_lock() {
                return Ok(guard);
            }
        }
        // Fallback: block on the first connection.
        self.connections[0].lock().map_err(|_| {
            trace_error!("read pool: connection mutex poisoned");
            EngineError::Bridge("connection mutex poisoned".to_owned())
        })
    }

    /// Return the number of connections in the pool.
    #[cfg(test)]
    fn size(&self) -> usize {
        self.connections.len()
    }
}

/// Execution plan returned by [`ExecutionCoordinator::explain_compiled_read`].
///
/// This is a read-only introspection struct. It does not execute SQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryPlan {
    pub sql: String,
    pub bind_count: usize,
    pub driving_table: DrivingTable,
    pub shape_hash: ShapeHash,
    pub cache_hit: bool,
}

/// A single node row returned from a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRow {
    /// Physical row ID.
    pub row_id: String,
    /// Logical ID of the node.
    pub logical_id: String,
    /// Node kind.
    pub kind: String,
    /// JSON-encoded node properties.
    pub properties: String,
    /// Optional URI referencing external content.
    pub content_ref: Option<String>,
    /// Unix timestamp of last access, if tracked.
    pub last_accessed_at: Option<i64>,
}

/// A single run row returned from a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRow {
    /// Unique run ID.
    pub id: String,
    /// Run kind.
    pub kind: String,
    /// Current status.
    pub status: String,
    /// JSON-encoded run properties.
    pub properties: String,
}

/// A single step row returned from a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepRow {
    /// Unique step ID.
    pub id: String,
    /// ID of the parent run.
    pub run_id: String,
    /// Step kind.
    pub kind: String,
    /// Current status.
    pub status: String,
    /// JSON-encoded step properties.
    pub properties: String,
}

/// A single action row returned from a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRow {
    /// Unique action ID.
    pub id: String,
    /// ID of the parent step.
    pub step_id: String,
    /// Action kind.
    pub kind: String,
    /// Current status.
    pub status: String,
    /// JSON-encoded action properties.
    pub properties: String,
}

/// A single row from the `provenance_events` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenanceEvent {
    pub id: String,
    pub event_type: String,
    pub subject: String,
    pub source_ref: Option<String>,
    pub metadata_json: String,
    pub created_at: i64,
}

/// Result set from executing a flat (non-grouped) compiled query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryRows {
    /// Matched node rows.
    pub nodes: Vec<NodeRow>,
    /// Runs associated with the matched nodes.
    pub runs: Vec<RunRow>,
    /// Steps associated with the matched runs.
    pub steps: Vec<StepRow>,
    /// Actions associated with the matched steps.
    pub actions: Vec<ActionRow>,
    /// `true` when a capability miss (e.g. missing sqlite-vec) caused the query
    /// to degrade to an empty result instead of propagating an error.
    pub was_degraded: bool,
}

/// Expansion results for a single root node within a grouped query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpansionRootRows {
    /// Logical ID of the root node that seeded this expansion.
    pub root_logical_id: String,
    /// Nodes reached by traversing from the root.
    pub nodes: Vec<NodeRow>,
}

/// All expansion results for a single named slot across all roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpansionSlotRows {
    /// Name of the expansion slot.
    pub slot: String,
    /// Per-root expansion results.
    pub roots: Vec<ExpansionRootRows>,
}

/// Result set from executing a grouped compiled query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupedQueryRows {
    /// Root node rows matched by the base query.
    pub roots: Vec<NodeRow>,
    /// Per-slot expansion results.
    pub expansions: Vec<ExpansionSlotRows>,
    /// `true` when a capability miss caused the query to degrade to an empty result.
    pub was_degraded: bool,
}

/// Manages a pool of read-only `SQLite` connections and executes compiled queries.
pub struct ExecutionCoordinator {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    pool: ReadPool,
    shape_sql_map: Mutex<HashMap<ShapeHash, String>>,
    vector_enabled: bool,
    vec_degradation_warned: AtomicBool,
    telemetry: Arc<TelemetryCounters>,
    /// Phase 12.5a: optional read-time query embedder. When present,
    /// [`Self::execute_retrieval_plan`] invokes it via
    /// [`Self::fill_vector_branch`] after compile to populate
    /// `plan.vector`. When `None`, the Phase 12 v1 vector-dormancy
    /// invariant on `search()` is preserved: the vector slot stays empty
    /// and the coordinator's stage-gating check skips the vector branch.
    query_embedder: Option<Arc<dyn QueryEmbedder>>,
}

impl fmt::Debug for ExecutionCoordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionCoordinator")
            .field("database_path", &self.database_path)
            .finish_non_exhaustive()
    }
}

impl ExecutionCoordinator {
    /// # Errors
    /// Returns [`EngineError`] if the database connection cannot be opened or schema bootstrap fails.
    pub fn open(
        path: impl AsRef<Path>,
        schema_manager: Arc<SchemaManager>,
        vector_dimension: Option<usize>,
        pool_size: usize,
        telemetry: Arc<TelemetryCounters>,
        query_embedder: Option<Arc<dyn QueryEmbedder>>,
    ) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        #[cfg(feature = "sqlite-vec")]
        let conn = if vector_dimension.is_some() {
            sqlite::open_connection_with_vec(&path)?
        } else {
            sqlite::open_connection(&path)?
        };
        #[cfg(not(feature = "sqlite-vec"))]
        let conn = sqlite::open_connection(&path)?;

        let report = schema_manager.bootstrap(&conn)?;

        // ----- Open-time rebuild guards for derived FTS state -----
        //
        // `fts_node_properties` and `fts_node_property_positions` are both
        // derived from the canonical `nodes.properties` blobs and the set
        // of registered `fts_property_schemas`. They are NOT source of
        // truth, so whenever a schema migration or crash leaves either
        // table out of sync with the canonical state we repopulate them
        // from scratch at open() time.
        //
        // The derived-state invariant is: if `fts_property_schemas` is
        // non-empty, then `fts_node_properties` must also be non-empty,
        // and if any schema is recursive-mode then
        // `fts_node_property_positions` must also be non-empty. If either
        // guard trips, both tables are cleared and rebuilt together — it
        // is never correct to rebuild only one half.
        //
        // Guard 1 (P2-1, FX pack, v15→v16 migration): detects an empty
        // `fts_node_properties` while schemas exist. This covers the
        // Phase 2 tokenizer swap (migration 16 drops the FTS5 virtual
        // table before recreating it under `porter unicode61`) and the
        // crash-recovery case where a prior open applied migration 16 but
        // crashed before the subsequent property-FTS rebuild committed.
        //
        // Guard 2 (P4-P2-3, FX2 pack, v16→v17 migration, extended to
        // v17→v18 by P4-P2-4): detects an empty
        // `fts_node_property_positions` while any recursive-mode schema
        // exists. This covers migration 17 (which added the sidecar table
        // but left it empty) and migration 18 (which drops and recreates
        // the sidecar to add the UNIQUE constraint, also leaving it
        // empty). Both migrations land on the same guard because both
        // leave the positions table empty while recursive schemas remain
        // registered.
        //
        // Both guards are no-ops on an already-consistent database, so
        // running them on every open is safe and cheap.
        let needs_property_fts_rebuild = {
            let schema_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM fts_property_schemas", [], |row| {
                    row.get(0)
                })?;
            if schema_count == 0 {
                false
            } else {
                let fts_count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM fts_node_properties", [], |row| {
                        row.get(0)
                    })?;
                fts_count == 0
            }
        };
        // Guard 2 (see block comment above): recursive schemas registered
        // but `fts_node_property_positions` empty. Rebuild regenerates
        // both the FTS blob and the position map from canonical state, so
        // it is safe to trigger even when `fts_node_properties` is
        // already populated.
        let needs_position_backfill = {
            // NOTE: This LIKE pattern assumes `property_paths_json` is
            // serialized with compact formatting (no whitespace around
            // `:`). All current writers go through `serde_json`'s compact
            // output so this holds. If a future writer emits pretty-
            // printed JSON (`"mode": "recursive"` with a space), this
            // guard would silently fail. A more robust check would use
            // `json_extract(property_paths_json, '$[*].mode')` or a
            // parsed scan, at the cost of a per-row JSON walk.
            let recursive_schema_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM fts_property_schemas \
                 WHERE property_paths_json LIKE '%\"mode\":\"recursive\"%'",
                [],
                |row| row.get(0),
            )?;
            if recursive_schema_count == 0 {
                false
            } else {
                let position_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM fts_node_property_positions",
                    [],
                    |row| row.get(0),
                )?;
                position_count == 0
            }
        };
        if needs_property_fts_rebuild || needs_position_backfill {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM fts_node_properties", [])?;
            tx.execute("DELETE FROM fts_node_property_positions", [])?;
            crate::projection::insert_property_fts_rows(
                &tx,
                "SELECT logical_id, properties FROM nodes \
                 WHERE kind = ?1 AND superseded_at IS NULL",
            )?;
            tx.commit()?;
        }

        #[cfg(feature = "sqlite-vec")]
        let mut vector_enabled = report.vector_profile_enabled;
        #[cfg(not(feature = "sqlite-vec"))]
        let vector_enabled = {
            let _ = &report;
            false
        };

        if let Some(dim) = vector_dimension {
            schema_manager
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", dim)
                .map_err(EngineError::Schema)?;
            // Profile was just created or updated — mark as enabled.
            #[cfg(feature = "sqlite-vec")]
            {
                vector_enabled = true;
            }
        }

        // Drop the bootstrap connection — pool connections are used for reads.
        drop(conn);

        let pool = ReadPool::new(&path, pool_size, &schema_manager, vector_enabled)?;

        Ok(Self {
            database_path: path,
            schema_manager,
            pool,
            shape_sql_map: Mutex::new(HashMap::new()),
            vector_enabled,
            vec_degradation_warned: AtomicBool::new(false),
            telemetry,
            query_embedder,
        })
    }

    /// Returns the filesystem path to the `SQLite` database.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Returns `true` when sqlite-vec was loaded and a vector profile is active.
    #[must_use]
    pub fn vector_enabled(&self) -> bool {
        self.vector_enabled
    }

    fn lock_connection(&self) -> Result<MutexGuard<'_, Connection>, EngineError> {
        self.pool.acquire()
    }

    /// Aggregate `SQLite` page-cache counters across all pool connections.
    ///
    /// Uses `try_lock` to avoid blocking reads for telemetry reporting.
    /// Connections that are currently locked by a query are skipped — this
    /// is acceptable for statistical counters.
    #[must_use]
    pub fn aggregate_cache_status(&self) -> SqliteCacheStatus {
        let mut total = SqliteCacheStatus::default();
        for conn_mutex in &self.pool.connections {
            if let Ok(conn) = conn_mutex.try_lock() {
                total.add(&read_db_cache_status(&conn));
            }
        }
        total
    }

    /// # Errors
    /// Returns [`EngineError`] if the SQL statement cannot be prepared or executed.
    #[allow(clippy::expect_used)]
    pub fn execute_compiled_read(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<QueryRows, EngineError> {
        let row_sql = wrap_node_row_projection_sql(&compiled.sql);
        // FIX(review): was .expect() — panics on mutex poisoning, cascading failure.
        // Options: (A) into_inner() for all, (B) EngineError for all, (C) mixed.
        // Chose (C): shape_sql_map is a pure cache — into_inner() is safe to recover.
        // conn wraps a SQLite connection whose state may be corrupt after a thread panic,
        // so we propagate EngineError there instead.
        {
            let mut cache = self
                .shape_sql_map
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if cache.len() >= MAX_SHAPE_CACHE_SIZE {
                trace_debug!(evicted = cache.len(), "shape cache full, clearing");
                cache.clear();
            }
            cache.insert(compiled.shape_hash, row_sql.clone());
        }

        let bind_values = compiled
            .binds
            .iter()
            .map(bind_value_to_sql)
            .collect::<Vec<_>>();

        // FIX(review) + Security fix M-8: was .expect() — panics on mutex poisoning.
        // shape_sql_map uses into_inner() (pure cache, safe to recover).
        // conn uses map_err → EngineError (connection state may be corrupt after panic;
        // into_inner() would risk using a connection with partial transaction state).
        let conn_guard = match self.lock_connection() {
            Ok(g) => g,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(e);
            }
        };
        let mut statement = match conn_guard.prepare_cached(&row_sql) {
            Ok(stmt) => stmt,
            Err(e) if is_vec_table_absent(&e) => {
                if !self.vec_degradation_warned.swap(true, Ordering::Relaxed) {
                    trace_warn!("vector table absent, degrading to non-vector query");
                }
                return Ok(QueryRows {
                    was_degraded: true,
                    ..Default::default()
                });
            }
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(EngineError::Sqlite(e));
            }
        };
        let nodes = match statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                Ok(NodeRow {
                    row_id: row.get(0)?,
                    logical_id: row.get(1)?,
                    kind: row.get(2)?,
                    properties: row.get(3)?,
                    content_ref: row.get(4)?,
                    last_accessed_at: row.get(5)?,
                })
            })
            .and_then(Iterator::collect)
        {
            Ok(rows) => rows,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(EngineError::Sqlite(e));
            }
        };

        self.telemetry.increment_queries();
        Ok(QueryRows {
            nodes,
            runs: Vec::new(),
            steps: Vec::new(),
            actions: Vec::new(),
            was_degraded: false,
        })
    }

    /// Execute a compiled adaptive search and return matching hits.
    ///
    /// Phase 2 splits filters: fusable predicates (`KindEq`, `LogicalIdEq`,
    /// `SourceRefEq`, `ContentRefEq`, `ContentRefNotNull`) are injected into
    /// the `search_hits` CTE so the CTE `LIMIT` applies after filtering,
    /// while residual predicates (JSON path filters) stay in the outer
    /// `WHERE`. The chunk and property FTS
    /// indexes are `UNION ALL`-ed, BM25-scored (flipped so larger values mean
    /// better matches), ordered, and limited. All hits return
    /// `match_mode = Strict` — the relaxed branch and fallback arrive in
    /// later phases.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the SQL statement cannot be prepared or executed.
    pub fn execute_compiled_search(
        &self,
        compiled: &CompiledSearch,
    ) -> Result<SearchRows, EngineError> {
        // Build the two-branch plan from the strict text query and delegate
        // to the shared plan-execution routine. The relaxed branch is derived
        // via `derive_relaxed` and only fires when strict returned fewer than
        // `min(FALLBACK_TRIGGER_K, limit)` hits. With K = 1 this collapses to
        // "relaxed iff strict is empty," but the routine spells the rule out
        // explicitly so raising K later is a one-line constant bump.
        let (relaxed_query, was_degraded_at_plan_time) =
            fathomdb_query::derive_relaxed(&compiled.text_query);
        let relaxed = relaxed_query.map(|q| CompiledSearch {
            root_kind: compiled.root_kind.clone(),
            text_query: q,
            limit: compiled.limit,
            fusable_filters: compiled.fusable_filters.clone(),
            residual_filters: compiled.residual_filters.clone(),
            attribution_requested: compiled.attribution_requested,
        });
        let plan = CompiledSearchPlan {
            strict: compiled.clone(),
            relaxed,
            was_degraded_at_plan_time,
        };
        self.execute_compiled_search_plan(&plan)
    }

    /// Execute a two-branch [`CompiledSearchPlan`] and return the merged,
    /// deduped result rows.
    ///
    /// This is the shared retrieval/merge routine that both
    /// [`Self::execute_compiled_search`] (adaptive path) and
    /// `Engine::fallback_search` (narrow two-shape path) call into. Strict
    /// runs first; the relaxed branch only fires when it is present AND the
    /// strict branch returned fewer than `min(FALLBACK_TRIGGER_K, limit)`
    /// hits. Merge and dedup semantics are identical to the adaptive path
    /// regardless of how the plan was constructed.
    ///
    /// Error contract: if the relaxed branch errors, the error propagates;
    /// strict hits are not returned. This matches the rest of the engine's
    /// fail-hard posture.
    ///
    /// # Errors
    /// Returns [`EngineError`] if either branch's SQL cannot be prepared or
    /// executed.
    pub fn execute_compiled_search_plan(
        &self,
        plan: &CompiledSearchPlan,
    ) -> Result<SearchRows, EngineError> {
        let strict = &plan.strict;
        let limit = strict.limit;
        let strict_hits = self.run_search_branch(strict, SearchBranch::Strict)?;

        let fallback_threshold = FALLBACK_TRIGGER_K.min(limit);
        let strict_underfilled = strict_hits.len() < fallback_threshold;

        let mut relaxed_hits: Vec<SearchHit> = Vec::new();
        let mut fallback_used = false;
        let mut was_degraded = false;
        if let Some(relaxed) = plan.relaxed.as_ref()
            && strict_underfilled
        {
            relaxed_hits = self.run_search_branch(relaxed, SearchBranch::Relaxed)?;
            fallback_used = true;
            was_degraded = plan.was_degraded_at_plan_time;
        }

        let mut merged = merge_search_branches(strict_hits, relaxed_hits, limit);
        // Attribution runs AFTER dedup so that duplicate hits dropped by
        // `merge_search_branches` do not waste a highlight+position-map
        // lookup.
        if strict.attribution_requested {
            let relaxed_text_query = plan.relaxed.as_ref().map(|r| &r.text_query);
            self.populate_attribution_for_hits(
                &mut merged,
                &strict.text_query,
                relaxed_text_query,
            )?;
        }
        let strict_hit_count = merged
            .iter()
            .filter(|h| matches!(h.match_mode, Some(SearchMatchMode::Strict)))
            .count();
        let relaxed_hit_count = merged
            .iter()
            .filter(|h| matches!(h.match_mode, Some(SearchMatchMode::Relaxed)))
            .count();
        // Phase 10: no vector execution path yet, so vector_hit_count is
        // always zero. Future phases that wire a vector branch will
        // contribute here.
        let vector_hit_count = 0;

        Ok(SearchRows {
            hits: merged,
            strict_hit_count,
            relaxed_hit_count,
            vector_hit_count,
            fallback_used,
            was_degraded,
        })
    }

    /// Execute a compiled vector-only search and return matching hits.
    ///
    /// Phase 11 delivers the standalone vector retrieval path. The emitted
    /// SQL performs a vec0 KNN scan over `vec_nodes_active`, joins to
    /// `chunks` and `nodes` (active rows only), and pushes fusable filters
    /// into the candidate CTE. The outer `SELECT` applies residual JSON
    /// predicates and orders by score descending, where `score = -distance`
    /// (higher is better) per addendum 1 §Vector-Specific Behavior.
    ///
    /// ## Capability-miss handling
    ///
    /// If the `sqlite-vec` capability is absent (feature disabled or the
    /// `vec_nodes_active` virtual table has not been created because the
    /// engine was not opened with a `vector_dimension`), this method returns
    /// an empty [`SearchRows`] with `was_degraded = true`. This is
    /// **non-fatal** — the error does not propagate — matching the addendum's
    /// §Vector-Specific Behavior / Degradation.
    ///
    /// ## Attribution
    ///
    /// When `compiled.attribution_requested == true`, every returned hit
    /// carries `attribution: Some(HitAttribution { matched_paths: vec![] })`
    /// per addendum 1 §Attribution on vector hits (Phase 5 chunk-hit rule
    /// extended uniformly).
    ///
    /// # Errors
    /// Returns [`EngineError`] if the SQL statement cannot be prepared or
    /// executed for reasons other than a vec-table capability miss.
    #[allow(clippy::too_many_lines)]
    pub fn execute_compiled_vector_search(
        &self,
        compiled: &CompiledVectorSearch,
    ) -> Result<SearchRows, EngineError> {
        use std::fmt::Write as _;

        // Short-circuit zero-limit: callers that pass `limit == 0` expect an
        // empty result rather than a SQL error from `LIMIT 0` semantics in
        // the inner vec0 scan.
        if compiled.limit == 0 {
            return Ok(SearchRows::default());
        }

        let filter_by_kind = !compiled.root_kind.is_empty();
        let mut binds: Vec<BindValue> = Vec::new();
        binds.push(BindValue::Text(compiled.query_text.clone()));
        if filter_by_kind {
            binds.push(BindValue::Text(compiled.root_kind.clone()));
        }

        // Build fusable-filter clauses, aliased against `src` inside the
        // candidate CTE. Same predicate set the text path fuses.
        let mut fused_clauses = String::new();
        for predicate in &compiled.fusable_filters {
            match predicate {
                Predicate::KindEq(kind) => {
                    binds.push(BindValue::Text(kind.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                      AND src.kind = ?{idx}"
                    );
                }
                Predicate::LogicalIdEq(logical_id) => {
                    binds.push(BindValue::Text(logical_id.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                      AND src.logical_id = ?{idx}"
                    );
                }
                Predicate::SourceRefEq(source_ref) => {
                    binds.push(BindValue::Text(source_ref.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                      AND src.source_ref = ?{idx}"
                    );
                }
                Predicate::ContentRefEq(uri) => {
                    binds.push(BindValue::Text(uri.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                      AND src.content_ref = ?{idx}"
                    );
                }
                Predicate::ContentRefNotNull => {
                    fused_clauses
                        .push_str("\n                      AND src.content_ref IS NOT NULL");
                }
                Predicate::JsonPathEq { .. } | Predicate::JsonPathCompare { .. } => {
                    // JSON predicates are residual; compile_vector_search
                    // guarantees they never appear here, but stay defensive.
                }
            }
        }

        // Build residual JSON clauses, aliased against `h` in the outer SELECT.
        let mut filter_clauses = String::new();
        for predicate in &compiled.residual_filters {
            match predicate {
                Predicate::JsonPathEq { path, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(scalar_to_bind(value));
                    let value_idx = binds.len();
                    let _ = write!(
                        filter_clauses,
                        "\n  AND json_extract(h.properties, ?{path_idx}) = ?{value_idx}"
                    );
                }
                Predicate::JsonPathCompare { path, op, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(scalar_to_bind(value));
                    let value_idx = binds.len();
                    let operator = match op {
                        ComparisonOp::Gt => ">",
                        ComparisonOp::Gte => ">=",
                        ComparisonOp::Lt => "<",
                        ComparisonOp::Lte => "<=",
                    };
                    let _ = write!(
                        filter_clauses,
                        "\n  AND json_extract(h.properties, ?{path_idx}) {operator} ?{value_idx}"
                    );
                }
                Predicate::KindEq(_)
                | Predicate::LogicalIdEq(_)
                | Predicate::SourceRefEq(_)
                | Predicate::ContentRefEq(_)
                | Predicate::ContentRefNotNull => {
                    // Fusable predicates live in fused_clauses above.
                }
            }
        }

        // Bind the outer limit as a named parameter for prepare_cached
        // stability across calls that vary only by limit value.
        let limit = compiled.limit;
        binds.push(BindValue::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
        let limit_idx = binds.len();

        // sqlite-vec requires the LIMIT/k constraint to be visible directly
        // on the vec0 KNN scan, so we isolate it in a sub-select. The vec0
        // LIMIT overfetches `base_limit` = limit (Phase 11 keeps it simple;
        // Phase 12's planner may raise this to compensate for fusion
        // narrowing the candidate pool).
        let base_limit = limit;
        let kind_clause = if filter_by_kind {
            "\n                      AND src.kind = ?2"
        } else {
            ""
        };

        let sql = format!(
            "WITH vector_hits AS (
                SELECT
                    src.row_id AS row_id,
                    src.logical_id AS logical_id,
                    src.kind AS kind,
                    src.properties AS properties,
                    src.source_ref AS source_ref,
                    src.content_ref AS content_ref,
                    src.created_at AS created_at,
                    vc.distance AS distance,
                    vc.chunk_id AS chunk_id
                FROM (
                    SELECT chunk_id, distance
                    FROM vec_nodes_active
                    WHERE embedding MATCH ?1
                    LIMIT {base_limit}
                ) vc
                JOIN chunks c ON c.id = vc.chunk_id
                JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                WHERE 1 = 1{kind_clause}{fused_clauses}
            )
            SELECT
                h.row_id,
                h.logical_id,
                h.kind,
                h.properties,
                h.content_ref,
                am.last_accessed_at,
                h.created_at,
                h.distance,
                h.chunk_id
            FROM vector_hits h
            LEFT JOIN node_access_metadata am ON am.logical_id = h.logical_id
            WHERE 1 = 1{filter_clauses}
            ORDER BY h.distance ASC
            LIMIT ?{limit_idx}"
        );

        let bind_values = binds.iter().map(bind_value_to_sql).collect::<Vec<_>>();

        let conn_guard = match self.lock_connection() {
            Ok(g) => g,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(e);
            }
        };
        let mut statement = match conn_guard.prepare_cached(&sql) {
            Ok(stmt) => stmt,
            Err(e) if is_vec_table_absent(&e) => {
                // Capability miss: non-fatal — surface as was_degraded.
                if !self.vec_degradation_warned.swap(true, Ordering::Relaxed) {
                    trace_warn!("vector table absent, degrading vector_search to empty result");
                }
                return Ok(SearchRows {
                    hits: Vec::new(),
                    strict_hit_count: 0,
                    relaxed_hit_count: 0,
                    vector_hit_count: 0,
                    fallback_used: false,
                    was_degraded: true,
                });
            }
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(EngineError::Sqlite(e));
            }
        };

        let attribution_requested = compiled.attribution_requested;
        let hits = match statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                let distance: f64 = row.get(7)?;
                // Score is the negated distance per addendum 1
                // §Vector-Specific Behavior / Score and distance. For
                // distance metrics (sqlite-vec's default) lower distance =
                // better match, so negating yields the higher-is-better
                // convention that dedup_branch_hits and the unified result
                // surface rely on.
                let score = -distance;
                Ok(SearchHit {
                    node: fathomdb_query::NodeRowLite {
                        row_id: row.get(0)?,
                        logical_id: row.get(1)?,
                        kind: row.get(2)?,
                        properties: row.get(3)?,
                        content_ref: row.get(4)?,
                        last_accessed_at: row.get(5)?,
                    },
                    written_at: row.get(6)?,
                    score,
                    modality: RetrievalModality::Vector,
                    source: SearchHitSource::Vector,
                    // Vector hits have no strict/relaxed notion.
                    match_mode: None,
                    // Vector hits have no snippet.
                    snippet: None,
                    projection_row_id: row.get::<_, Option<String>>(8)?,
                    vector_distance: Some(distance),
                    attribution: if attribution_requested {
                        Some(HitAttribution {
                            matched_paths: Vec::new(),
                        })
                    } else {
                        None
                    },
                })
            })
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        {
            Ok(rows) => rows,
            Err(e) => {
                // Some SQLite errors surface during row iteration (e.g. when
                // the vec0 extension is not loaded but the table exists as a
                // stub). Classify as capability-miss when the shape matches.
                if is_vec_table_absent(&e) {
                    if !self.vec_degradation_warned.swap(true, Ordering::Relaxed) {
                        trace_warn!(
                            "vector table absent at query time, degrading vector_search to empty result"
                        );
                    }
                    drop(statement);
                    drop(conn_guard);
                    return Ok(SearchRows {
                        hits: Vec::new(),
                        strict_hit_count: 0,
                        relaxed_hit_count: 0,
                        vector_hit_count: 0,
                        fallback_used: false,
                        was_degraded: true,
                    });
                }
                self.telemetry.increment_errors();
                return Err(EngineError::Sqlite(e));
            }
        };

        drop(statement);
        drop(conn_guard);

        self.telemetry.increment_queries();
        let vector_hit_count = hits.len();
        Ok(SearchRows {
            hits,
            strict_hit_count: 0,
            relaxed_hit_count: 0,
            vector_hit_count,
            fallback_used: false,
            was_degraded: false,
        })
    }

    /// Execute a unified [`CompiledRetrievalPlan`] (Phase 12 `search()`
    /// entry point) and return deterministically ranked, block-ordered
    /// [`SearchRows`].
    ///
    /// Stages, per addendum 1 §Retrieval Planner Model:
    ///
    /// 1. **Text strict.** Always runs (empty query short-circuits to an
    ///    empty branch result inside `run_search_branch`).
    /// 2. **Text relaxed.** Runs iff the plan carries a relaxed branch AND
    ///    the strict branch returned fewer than `min(FALLBACK_TRIGGER_K,
    ///    limit)` hits — same v1 (`K = 1`) zero-hits-only trigger as the
    ///    Phase 6 text-only path.
    /// 3. **Vector.** Runs iff text retrieval (strict + relaxed combined)
    ///    returned zero hits AND `plan.vector` is `Some`. **In v1 the
    ///    planner never wires a vector branch through `search()`, so this
    ///    code path is structurally present but dormant.** A future phase
    ///    that wires read-time embedding into `compile_retrieval_plan` will
    ///    immediately light it up.
    /// 4. **Fusion.** All collected hits are merged via
    ///    [`merge_search_branches_three`], which produces strict ->
    ///    relaxed -> vector block ordering with cross-branch dedup
    ///    resolved by branch precedence.
    ///
    /// `was_degraded` covers only the relaxed-branch cap miss in v1. The
    /// addendum's "vector capability miss => `was_degraded`" semantics
    /// applies to `search()` only when the unified planner actually fires
    /// the vector branch, which v1 never does.
    ///
    /// # Errors
    /// Returns [`EngineError`] if any stage's SQL cannot be prepared or
    /// executed for a non-capability-miss reason.
    pub fn execute_retrieval_plan(
        &self,
        plan: &CompiledRetrievalPlan,
        raw_query: &str,
    ) -> Result<SearchRows, EngineError> {
        // Phase 12.5a: we work against a local owned copy so
        // `fill_vector_branch` can mutate `plan.vector` and
        // `plan.was_degraded_at_plan_time`. Cloning is cheap (the plan is
        // a bounded set of predicates + text AST nodes) and avoids
        // forcing callers to hand us `&mut` access.
        let mut plan = plan.clone();
        let limit = plan.text.strict.limit;

        // Stage 1: text strict.
        let strict_hits = self.run_search_branch(&plan.text.strict, SearchBranch::Strict)?;

        // Stage 2: text relaxed. Same K=1 zero-hits-only trigger the Phase 6
        // path uses.
        let fallback_threshold = FALLBACK_TRIGGER_K.min(limit);
        let strict_underfilled = strict_hits.len() < fallback_threshold;
        let mut relaxed_hits: Vec<SearchHit> = Vec::new();
        let mut fallback_used = false;
        let mut was_degraded = false;
        if let Some(relaxed) = plan.text.relaxed.as_ref()
            && strict_underfilled
        {
            relaxed_hits = self.run_search_branch(relaxed, SearchBranch::Relaxed)?;
            fallback_used = true;
            was_degraded = plan.was_degraded_at_plan_time;
        }

        // Phase 12.5a: fill the vector branch from the configured
        // read-time query embedder, if any. Option (b) from the spec:
        // only pay the embedding cost when the text branches returned
        // nothing, because the three-branch stage gate below only runs
        // the vector stage under exactly that condition. This keeps the
        // hot path (strict text matched) embedder-free.
        let text_branches_empty = strict_hits.is_empty() && relaxed_hits.is_empty();
        if text_branches_empty && self.query_embedder.is_some() {
            self.fill_vector_branch(&mut plan, raw_query);
        }

        // Stage 3: vector. Runs only when text retrieval is empty AND a
        // vector branch is present. When no embedder is configured (Phase
        // 12.5a default) `plan.vector` stays `None` and this stage is a
        // no-op, preserving the Phase 12 v1 dormancy invariant.
        let mut vector_hits: Vec<SearchHit> = Vec::new();
        if let Some(vector) = plan.vector.as_ref()
            && strict_hits.is_empty()
            && relaxed_hits.is_empty()
        {
            let vector_rows = self.execute_compiled_vector_search(vector)?;
            // `execute_compiled_vector_search` returns a fully populated
            // `SearchRows`. Promote its hits into the merge stage and lift
            // its capability-miss `was_degraded` flag onto the unified
            // result, per addendum §Vector-Specific Behavior.
            vector_hits = vector_rows.hits;
            if vector_rows.was_degraded {
                was_degraded = true;
            }
        }
        // Phase 12.5a: an embedder-reported capability miss surfaces as
        // `plan.was_degraded_at_plan_time = true` set inside
        // `fill_vector_branch`. Lift it onto the response so callers see
        // the graceful degradation even when the vector stage-gate never
        // had the chance to fire (the embedder call itself failed and
        // the vector slot stayed `None`).
        if text_branches_empty
            && plan.was_degraded_at_plan_time
            && plan.vector.is_none()
            && self.query_embedder.is_some()
        {
            was_degraded = true;
        }

        // Stage 4: fusion.
        let strict = &plan.text.strict;
        let mut merged = merge_search_branches_three(strict_hits, relaxed_hits, vector_hits, limit);
        if strict.attribution_requested {
            let relaxed_text_query = plan.text.relaxed.as_ref().map(|r| &r.text_query);
            self.populate_attribution_for_hits(
                &mut merged,
                &strict.text_query,
                relaxed_text_query,
            )?;
        }

        let strict_hit_count = merged
            .iter()
            .filter(|h| matches!(h.match_mode, Some(SearchMatchMode::Strict)))
            .count();
        let relaxed_hit_count = merged
            .iter()
            .filter(|h| matches!(h.match_mode, Some(SearchMatchMode::Relaxed)))
            .count();
        let vector_hit_count = merged
            .iter()
            .filter(|h| matches!(h.modality, RetrievalModality::Vector))
            .count();

        Ok(SearchRows {
            hits: merged,
            strict_hit_count,
            relaxed_hit_count,
            vector_hit_count,
            fallback_used,
            was_degraded,
        })
    }

    /// Phase 12.5a: populate `plan.vector` from the configured read-time
    /// query embedder, if any.
    ///
    /// Preconditions (enforced by the caller in `execute_retrieval_plan`):
    /// - `self.query_embedder.is_some()` — no point calling otherwise.
    /// - Both the strict and relaxed text branches already ran and
    ///   returned zero hits, so the existing three-branch stage gate
    ///   will actually fire the vector stage once the slot is populated.
    ///   This is option (b) from the Phase 12.5a spec: skip the embedding
    ///   cost entirely when text retrieval already won.
    ///
    /// Contract: never panics, never returns an error. On embedder error
    /// it sets `plan.was_degraded_at_plan_time = true` and leaves
    /// `plan.vector` as `None`; the coordinator's normal error-free
    /// degradation path then reports `was_degraded` on the result.
    fn fill_vector_branch(&self, plan: &mut CompiledRetrievalPlan, raw_query: &str) {
        let Some(embedder) = self.query_embedder.as_ref() else {
            return;
        };
        match embedder.embed_query(raw_query) {
            Ok(vec) => {
                // `CompiledVectorSearch::query_text` is a JSON float-array
                // literal at the time the coordinator binds it (see the
                // `CompiledVectorSearch` docs). `serde_json::to_string`
                // on a `Vec<f32>` produces exactly that shape — no
                // wire-format change required.
                let literal = match serde_json::to_string(&vec) {
                    Ok(s) => s,
                    Err(err) => {
                        trace_warn!(
                            error = %err,
                            "query embedder vector serialization failed; skipping vector branch"
                        );
                        let _ = err; // Used by trace_warn! when tracing feature is active
                        plan.was_degraded_at_plan_time = true;
                        return;
                    }
                };
                let strict = &plan.text.strict;
                plan.vector = Some(CompiledVectorSearch {
                    root_kind: strict.root_kind.clone(),
                    query_text: literal,
                    limit: strict.limit,
                    fusable_filters: strict.fusable_filters.clone(),
                    residual_filters: strict.residual_filters.clone(),
                    attribution_requested: strict.attribution_requested,
                });
            }
            Err(err) => {
                trace_warn!(
                    error = %err,
                    "query embedder unavailable, skipping vector branch"
                );
                let _ = err; // Used by trace_warn! when tracing feature is active
                plan.was_degraded_at_plan_time = true;
            }
        }
    }

    /// Execute a single search branch against the underlying FTS surfaces.
    ///
    /// This is the shared SQL emission path used by
    /// [`Self::execute_compiled_search_plan`] to run strict and (when
    /// present) relaxed branches of a [`CompiledSearchPlan`] in sequence.
    /// The returned hits are tagged with `branch`'s corresponding
    /// [`SearchMatchMode`] and are **not** yet deduped or truncated — the
    /// caller is responsible for merging multiple branches.
    #[allow(clippy::too_many_lines)]
    fn run_search_branch(
        &self,
        compiled: &CompiledSearch,
        branch: SearchBranch,
    ) -> Result<Vec<SearchHit>, EngineError> {
        use std::fmt::Write as _;
        // Short-circuit an empty/whitespace-only query: rendering it would
        // yield `MATCH ""`, which FTS5 rejects as a syntax error. Callers
        // (including the adaptive path when strict is Empty and derive_relaxed
        // returns None) must see an empty result, not an error. Each branch
        // is short-circuited independently so a strict-Empty + relaxed-Some
        // plan still exercises the relaxed branch.
        // A top-level `TextQuery::Not` renders to an FTS5 expression that
        // matches "every row not containing X" — a complement-of-corpus scan
        // that no caller would intentionally want. Short-circuit to empty at
        // the root only; a `Not` nested inside an `And` is a legitimate
        // exclusion and must still run.
        if matches!(
            compiled.text_query,
            fathomdb_query::TextQuery::Empty | fathomdb_query::TextQuery::Not(_)
        ) {
            return Ok(Vec::new());
        }
        let rendered = render_text_query_fts5(&compiled.text_query);
        // An empty `root_kind` means "unkind-filtered" — the fallback_search
        // helper uses this when the caller did not add `.filter_kind_eq(...)`.
        // The adaptive `text_search()` path never produces an empty root_kind
        // because `QueryBuilder::nodes(kind)` requires a non-empty string at
        // the entry point.
        let filter_by_kind = !compiled.root_kind.is_empty();
        let mut binds: Vec<BindValue> = if filter_by_kind {
            vec![
                BindValue::Text(rendered.clone()),
                BindValue::Text(compiled.root_kind.clone()),
                BindValue::Text(rendered),
                BindValue::Text(compiled.root_kind.clone()),
            ]
        } else {
            vec![BindValue::Text(rendered.clone()), BindValue::Text(rendered)]
        };

        // P2-5: both fusable and residual predicates now match against the
        // CTE's projected columns (`u.kind`, `u.logical_id`, `u.source_ref`,
        // `u.content_ref`, `u.properties`) because the inner UNION arms
        // project the full active-row column set through the
        // `JOIN nodes src` already present in each arm. The previous
        // implementation re-joined `nodes hn` at the CTE level and
        // `nodes n` again at the outer SELECT, which was triple work on
        // the hot search path.
        let mut fused_clauses = String::new();
        for predicate in &compiled.fusable_filters {
            match predicate {
                Predicate::KindEq(kind) => {
                    binds.push(BindValue::Text(kind.clone()));
                    let idx = binds.len();
                    let _ = write!(fused_clauses, "\n                  AND u.kind = ?{idx}");
                }
                Predicate::LogicalIdEq(logical_id) => {
                    binds.push(BindValue::Text(logical_id.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND u.logical_id = ?{idx}"
                    );
                }
                Predicate::SourceRefEq(source_ref) => {
                    binds.push(BindValue::Text(source_ref.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND u.source_ref = ?{idx}"
                    );
                }
                Predicate::ContentRefEq(uri) => {
                    binds.push(BindValue::Text(uri.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND u.content_ref = ?{idx}"
                    );
                }
                Predicate::ContentRefNotNull => {
                    fused_clauses.push_str("\n                  AND u.content_ref IS NOT NULL");
                }
                Predicate::JsonPathEq { .. } | Predicate::JsonPathCompare { .. } => {
                    // Should be in residual_filters; compile_search guarantees
                    // this, but stay defensive.
                }
            }
        }

        let mut filter_clauses = String::new();
        for predicate in &compiled.residual_filters {
            match predicate {
                Predicate::JsonPathEq { path, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(scalar_to_bind(value));
                    let value_idx = binds.len();
                    let _ = write!(
                        filter_clauses,
                        "\n  AND json_extract(h.properties, ?{path_idx}) = ?{value_idx}"
                    );
                }
                Predicate::JsonPathCompare { path, op, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(scalar_to_bind(value));
                    let value_idx = binds.len();
                    let operator = match op {
                        ComparisonOp::Gt => ">",
                        ComparisonOp::Gte => ">=",
                        ComparisonOp::Lt => "<",
                        ComparisonOp::Lte => "<=",
                    };
                    let _ = write!(
                        filter_clauses,
                        "\n  AND json_extract(h.properties, ?{path_idx}) {operator} ?{value_idx}"
                    );
                }
                Predicate::KindEq(_)
                | Predicate::LogicalIdEq(_)
                | Predicate::SourceRefEq(_)
                | Predicate::ContentRefEq(_)
                | Predicate::ContentRefNotNull => {
                    // Fusable predicates live in fused_clauses; compile_search
                    // partitions them out of residual_filters.
                }
            }
        }

        // Bind `limit` as an integer parameter rather than formatting it into
        // the SQL string. Interpolating the limit made the prepared-statement
        // SQL vary by limit value, so rusqlite's default 16-slot
        // `prepare_cached` cache thrashed for paginated callers that varied
        // limits per call. With the bind the SQL is structurally stable for
        // a given filter shape regardless of `limit` value.
        let limit = compiled.limit;
        binds.push(BindValue::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
        let limit_idx = binds.len();
        // P2-5: the inner UNION arms project the full active-row column
        // set through `JOIN nodes src` (kind, row_id, source_ref,
        // content_ref, content_hash, created_at, properties). Both the
        // CTE's outer WHERE and the final SELECT consume those columns
        // directly, which eliminates the previous `JOIN nodes hn` at the
        // CTE level and `JOIN nodes n` at the outer SELECT — saving two
        // redundant joins on the hot search path. `src.superseded_at IS
        // NULL` in each arm already filters retired rows, which is what
        // the dropped outer joins used to do.
        let (chunk_fts_bind, chunk_kind_clause, prop_fts_bind, prop_kind_clause) = if filter_by_kind
        {
            (
                "?1",
                "\n                      AND src.kind = ?2",
                "?3",
                "\n                      AND fp.kind = ?4",
            )
        } else {
            ("?1", "", "?2", "")
        };
        let sql = format!(
            "WITH search_hits AS (
                SELECT
                    u.row_id AS row_id,
                    u.logical_id AS logical_id,
                    u.kind AS kind,
                    u.properties AS properties,
                    u.source_ref AS source_ref,
                    u.content_ref AS content_ref,
                    u.created_at AS created_at,
                    u.score AS score,
                    u.source AS source,
                    u.snippet AS snippet,
                    u.projection_row_id AS projection_row_id
                FROM (
                    SELECT
                        src.row_id AS row_id,
                        c.node_logical_id AS logical_id,
                        src.kind AS kind,
                        src.properties AS properties,
                        src.source_ref AS source_ref,
                        src.content_ref AS content_ref,
                        src.created_at AS created_at,
                        -bm25(fts_nodes) AS score,
                        'chunk' AS source,
                        snippet(fts_nodes, 3, '[', ']', '…', 32) AS snippet,
                        f.chunk_id AS projection_row_id
                    FROM fts_nodes f
                    JOIN chunks c ON c.id = f.chunk_id
                    JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                    WHERE fts_nodes MATCH {chunk_fts_bind}{chunk_kind_clause}
                    UNION ALL
                    SELECT
                        src.row_id AS row_id,
                        fp.node_logical_id AS logical_id,
                        src.kind AS kind,
                        src.properties AS properties,
                        src.source_ref AS source_ref,
                        src.content_ref AS content_ref,
                        src.created_at AS created_at,
                        -bm25(fts_node_properties) AS score,
                        'property' AS source,
                        substr(fp.text_content, 1, 200) AS snippet,
                        CAST(fp.rowid AS TEXT) AS projection_row_id
                    FROM fts_node_properties fp
                    JOIN nodes src ON src.logical_id = fp.node_logical_id AND src.superseded_at IS NULL
                    WHERE fts_node_properties MATCH {prop_fts_bind}{prop_kind_clause}
                ) u
                WHERE 1 = 1{fused_clauses}
                ORDER BY u.score DESC
                LIMIT ?{limit_idx}
            )
            SELECT
                h.row_id,
                h.logical_id,
                h.kind,
                h.properties,
                h.content_ref,
                am.last_accessed_at,
                h.created_at,
                h.score,
                h.source,
                h.snippet,
                h.projection_row_id
            FROM search_hits h
            LEFT JOIN node_access_metadata am ON am.logical_id = h.logical_id
            WHERE 1 = 1{filter_clauses}
            ORDER BY h.score DESC"
        );

        let bind_values = binds.iter().map(bind_value_to_sql).collect::<Vec<_>>();

        let conn_guard = match self.lock_connection() {
            Ok(g) => g,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(e);
            }
        };
        let mut statement = match conn_guard.prepare_cached(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(EngineError::Sqlite(e));
            }
        };

        let hits = match statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                let source_str: String = row.get(8)?;
                // The CTE emits only two literal values here: `'chunk'` and
                // `'property'`. Default to `Chunk` on anything unexpected so a
                // schema drift surfaces as a mislabelled hit rather than a
                // row-level error.
                let source = if source_str == "property" {
                    SearchHitSource::Property
                } else {
                    SearchHitSource::Chunk
                };
                let match_mode = match branch {
                    SearchBranch::Strict => SearchMatchMode::Strict,
                    SearchBranch::Relaxed => SearchMatchMode::Relaxed,
                };
                Ok(SearchHit {
                    node: fathomdb_query::NodeRowLite {
                        row_id: row.get(0)?,
                        logical_id: row.get(1)?,
                        kind: row.get(2)?,
                        properties: row.get(3)?,
                        content_ref: row.get(4)?,
                        last_accessed_at: row.get(5)?,
                    },
                    written_at: row.get(6)?,
                    score: row.get(7)?,
                    // Phase 10: every branch currently emits text hits.
                    modality: RetrievalModality::Text,
                    source,
                    match_mode: Some(match_mode),
                    snippet: row.get(9)?,
                    projection_row_id: row.get(10)?,
                    vector_distance: None,
                    attribution: None,
                })
            })
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        {
            Ok(rows) => rows,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(EngineError::Sqlite(e));
            }
        };

        // Drop the statement so `conn_guard` is free (attribution is
        // resolved after dedup in `execute_compiled_search_plan` to avoid
        // spending highlight lookups on hits that will be discarded).
        drop(statement);
        drop(conn_guard);

        self.telemetry.increment_queries();
        Ok(hits)
    }

    /// Populate per-hit attribution for the given deduped merged hits.
    /// Runs after [`merge_search_branches`] so dropped duplicates do not
    /// incur the highlight+position-map lookup cost.
    fn populate_attribution_for_hits(
        &self,
        hits: &mut [SearchHit],
        strict_text_query: &fathomdb_query::TextQuery,
        relaxed_text_query: Option<&fathomdb_query::TextQuery>,
    ) -> Result<(), EngineError> {
        let conn_guard = match self.lock_connection() {
            Ok(g) => g,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(e);
            }
        };
        let strict_expr = render_text_query_fts5(strict_text_query);
        let relaxed_expr = relaxed_text_query.map(render_text_query_fts5);
        for hit in hits.iter_mut() {
            // Phase 10: text hits always carry `Some(match_mode)`. Vector
            // hits (when a future phase adds them) have `None` here and
            // are skipped by the attribution resolver because attribution
            // is meaningless for vector matches.
            let match_expr = match hit.match_mode {
                Some(SearchMatchMode::Strict) => strict_expr.as_str(),
                Some(SearchMatchMode::Relaxed) => {
                    relaxed_expr.as_deref().unwrap_or(strict_expr.as_str())
                }
                None => continue,
            };
            match resolve_hit_attribution(&conn_guard, hit, match_expr) {
                Ok(att) => hit.attribution = Some(att),
                Err(e) => {
                    self.telemetry.increment_errors();
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// # Errors
    /// Returns [`EngineError`] if the root query or any bounded expansion
    /// query cannot be prepared or executed.
    pub fn execute_compiled_grouped_read(
        &self,
        compiled: &CompiledGroupedQuery,
    ) -> Result<GroupedQueryRows, EngineError> {
        let root_rows = self.execute_compiled_read(&compiled.root)?;
        if root_rows.was_degraded {
            return Ok(GroupedQueryRows {
                roots: Vec::new(),
                expansions: Vec::new(),
                was_degraded: true,
            });
        }

        let roots = root_rows.nodes;
        let mut expansions = Vec::with_capacity(compiled.expansions.len());
        for expansion in &compiled.expansions {
            let slot_rows = if roots.is_empty() {
                Vec::new()
            } else {
                self.read_expansion_nodes_chunked(&roots, expansion, compiled.hints.hard_limit)?
            };
            expansions.push(ExpansionSlotRows {
                slot: expansion.slot.clone(),
                roots: slot_rows,
            });
        }

        Ok(GroupedQueryRows {
            roots,
            expansions,
            was_degraded: false,
        })
    }

    /// Chunked batched expansion: splits roots into chunks of
    /// `BATCH_CHUNK_SIZE` and runs one batched query per chunk, then merges
    /// results while preserving root ordering.  This keeps bind-parameter
    /// counts within `SQLite` limits while avoiding the N+1 per-root pattern
    /// for large result sets.
    fn read_expansion_nodes_chunked(
        &self,
        roots: &[NodeRow],
        expansion: &ExpansionSlot,
        hard_limit: usize,
    ) -> Result<Vec<ExpansionRootRows>, EngineError> {
        if roots.len() <= BATCH_CHUNK_SIZE {
            return self.read_expansion_nodes_batched(roots, expansion, hard_limit);
        }

        // Merge chunk results keyed by root logical_id, then reassemble in
        // root order.
        let mut per_root: HashMap<String, Vec<NodeRow>> = HashMap::new();
        for chunk in roots.chunks(BATCH_CHUNK_SIZE) {
            for group in self.read_expansion_nodes_batched(chunk, expansion, hard_limit)? {
                per_root
                    .entry(group.root_logical_id)
                    .or_default()
                    .extend(group.nodes);
            }
        }

        Ok(roots
            .iter()
            .map(|root| ExpansionRootRows {
                root_logical_id: root.logical_id.clone(),
                nodes: per_root.remove(&root.logical_id).unwrap_or_default(),
            })
            .collect())
    }

    /// Batched expansion: one recursive CTE query per expansion slot that
    /// processes all root IDs at once. Uses `ROW_NUMBER() OVER (PARTITION BY
    /// source_logical_id ...)` to enforce the per-root hard limit inside the
    /// database rather than in Rust.
    fn read_expansion_nodes_batched(
        &self,
        roots: &[NodeRow],
        expansion: &ExpansionSlot,
        hard_limit: usize,
    ) -> Result<Vec<ExpansionRootRows>, EngineError> {
        let root_ids: Vec<&str> = roots.iter().map(|r| r.logical_id.as_str()).collect();
        let (join_condition, next_logical_id) = match expansion.direction {
            fathomdb_query::TraverseDirection::Out => {
                ("e.source_logical_id = t.logical_id", "e.target_logical_id")
            }
            fathomdb_query::TraverseDirection::In => {
                ("e.target_logical_id = t.logical_id", "e.source_logical_id")
            }
        };

        // Build a UNION ALL of SELECT literals for the root seed rows.
        // SQLite does not support `VALUES ... AS alias(col)` in older versions,
        // so we use `SELECT ?1 UNION ALL SELECT ?2 ...` instead.
        let root_seed_union: String = (1..=root_ids.len())
            .map(|i| format!("SELECT ?{i}"))
            .collect::<Vec<_>>()
            .join(" UNION ALL ");

        // The `root_id` column tracks which root each traversal path
        // originated from. The `ROW_NUMBER()` window in the outer query
        // enforces the per-root hard limit.
        let sql = format!(
            "WITH RECURSIVE root_ids(rid) AS ({root_seed_union}),
            traversed(root_id, logical_id, depth, visited, emitted) AS (
                SELECT rid, rid, 0, printf(',%s,', rid), 0
                FROM root_ids
                UNION ALL
                SELECT
                    t.root_id,
                    {next_logical_id},
                    t.depth + 1,
                    t.visited || {next_logical_id} || ',',
                    t.emitted + 1
                FROM traversed t
                JOIN edges e ON {join_condition}
                    AND e.kind = ?{edge_kind_param}
                    AND e.superseded_at IS NULL
                WHERE t.depth < {max_depth}
                  AND t.emitted < {hard_limit}
                  AND instr(t.visited, printf(',%s,', {next_logical_id})) = 0
            ),
            numbered AS (
                SELECT t.root_id, n.row_id, n.logical_id, n.kind, n.properties
                     , n.content_ref, am.last_accessed_at
                     , ROW_NUMBER() OVER (PARTITION BY t.root_id ORDER BY n.logical_id) AS rn
                FROM traversed t
                JOIN nodes n ON n.logical_id = t.logical_id
                    AND n.superseded_at IS NULL
                LEFT JOIN node_access_metadata am ON am.logical_id = n.logical_id
                WHERE t.depth > 0
            )
            SELECT root_id, row_id, logical_id, kind, properties, content_ref, last_accessed_at
            FROM numbered
            WHERE rn <= {hard_limit}
            ORDER BY root_id, logical_id",
            edge_kind_param = root_ids.len() + 1,
            max_depth = expansion.max_depth,
        );

        let conn_guard = self.lock_connection()?;
        let mut statement = conn_guard
            .prepare_cached(&sql)
            .map_err(EngineError::Sqlite)?;

        // Bind root IDs (1..=N) and edge kind (N+1).
        let mut bind_values: Vec<Value> = root_ids
            .iter()
            .map(|id| Value::Text((*id).to_owned()))
            .collect();
        bind_values.push(Value::Text(expansion.label.clone()));

        let rows = statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?, // root_id
                    NodeRow {
                        row_id: row.get(1)?,
                        logical_id: row.get(2)?,
                        kind: row.get(3)?,
                        properties: row.get(4)?,
                        content_ref: row.get(5)?,
                        last_accessed_at: row.get(6)?,
                    },
                ))
            })
            .map_err(EngineError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Sqlite)?;

        // Partition results back into per-root groups, preserving root order.
        let mut per_root: HashMap<String, Vec<NodeRow>> = HashMap::new();
        for (root_id, node) in rows {
            per_root.entry(root_id).or_default().push(node);
        }

        let root_groups = roots
            .iter()
            .map(|root| ExpansionRootRows {
                root_logical_id: root.logical_id.clone(),
                nodes: per_root.remove(&root.logical_id).unwrap_or_default(),
            })
            .collect();

        Ok(root_groups)
    }

    /// Read a single run by id.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the query fails or if the connection mutex
    /// has been poisoned.
    pub fn read_run(&self, id: &str) -> Result<Option<RunRow>, EngineError> {
        let conn = self.lock_connection()?;
        conn.query_row(
            "SELECT id, kind, status, properties FROM runs WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(RunRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    status: row.get(2)?,
                    properties: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(EngineError::Sqlite)
    }

    /// Read a single step by id.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the query fails or if the connection mutex
    /// has been poisoned.
    pub fn read_step(&self, id: &str) -> Result<Option<StepRow>, EngineError> {
        let conn = self.lock_connection()?;
        conn.query_row(
            "SELECT id, run_id, kind, status, properties FROM steps WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(StepRow {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    kind: row.get(2)?,
                    status: row.get(3)?,
                    properties: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(EngineError::Sqlite)
    }

    /// Read a single action by id.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the query fails or if the connection mutex
    /// has been poisoned.
    pub fn read_action(&self, id: &str) -> Result<Option<ActionRow>, EngineError> {
        let conn = self.lock_connection()?;
        conn.query_row(
            "SELECT id, step_id, kind, status, properties FROM actions WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(ActionRow {
                    id: row.get(0)?,
                    step_id: row.get(1)?,
                    kind: row.get(2)?,
                    status: row.get(3)?,
                    properties: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(EngineError::Sqlite)
    }

    /// Read all active (non-superseded) runs.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the query fails or if the connection mutex
    /// has been poisoned.
    pub fn read_active_runs(&self) -> Result<Vec<RunRow>, EngineError> {
        let conn = self.lock_connection()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, kind, status, properties FROM runs WHERE superseded_at IS NULL",
            )
            .map_err(EngineError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RunRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    status: row.get(2)?,
                    properties: row.get(3)?,
                })
            })
            .map_err(EngineError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Sqlite)?;
        Ok(rows)
    }

    /// Returns the number of shape→SQL entries currently indexed.
    ///
    /// Each distinct query shape (structural hash of kind + steps + limits)
    /// maps to exactly one SQL string.  This is a test-oriented introspection
    /// helper; it does not reflect rusqlite's internal prepared-statement
    /// cache, which is keyed by SQL text.
    ///
    /// # Panics
    /// Panics if the internal shape-SQL-map mutex is poisoned.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn shape_sql_count(&self) -> usize {
        self.shape_sql_map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Returns a cloned `Arc` to the schema manager.
    #[must_use]
    pub fn schema_manager(&self) -> Arc<SchemaManager> {
        Arc::clone(&self.schema_manager)
    }

    /// Return the execution plan for a compiled query without executing it.
    ///
    /// Useful for debugging, testing shape-hash caching, and operator
    /// diagnostics. Does not open a transaction or touch the database beyond
    /// checking the statement cache.
    ///
    /// # Panics
    /// Panics if the internal shape-SQL-map mutex is poisoned.
    #[must_use]
    pub fn explain_compiled_read(&self, compiled: &CompiledQuery) -> QueryPlan {
        let cache_hit = self
            .shape_sql_map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&compiled.shape_hash);
        QueryPlan {
            sql: wrap_node_row_projection_sql(&compiled.sql),
            bind_count: compiled.binds.len(),
            driving_table: compiled.driving_table,
            shape_hash: compiled.shape_hash,
            cache_hit,
        }
    }

    /// Execute a named PRAGMA and return the result as a String.
    /// Used by Layer 1 tests to verify startup pragma initialization.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the PRAGMA query fails or if the connection
    /// mutex has been poisoned.
    #[doc(hidden)]
    pub fn raw_pragma(&self, name: &str) -> Result<String, EngineError> {
        let conn = self.lock_connection()?;
        let result = conn
            .query_row(&format!("PRAGMA {name}"), [], |row| {
                // PRAGMAs may return TEXT or INTEGER; normalise to String.
                row.get::<_, rusqlite::types::Value>(0)
            })
            .map_err(EngineError::Sqlite)?;
        let s = match result {
            rusqlite::types::Value::Text(t) => t,
            rusqlite::types::Value::Integer(i) => i.to_string(),
            rusqlite::types::Value::Real(f) => f.to_string(),
            rusqlite::types::Value::Blob(_) => {
                return Err(EngineError::InvalidWrite(format!(
                    "PRAGMA {name} returned an unexpected BLOB value"
                )));
            }
            rusqlite::types::Value::Null => String::new(),
        };
        Ok(s)
    }

    /// Return all provenance events whose `subject` matches the given value.
    ///
    /// Subjects are logical node IDs (for retire/upsert events) or `source_ref`
    /// values (for excise events).
    ///
    /// # Errors
    /// Returns [`EngineError`] if the query fails or if the connection mutex
    /// has been poisoned.
    pub fn query_provenance_events(
        &self,
        subject: &str,
    ) -> Result<Vec<ProvenanceEvent>, EngineError> {
        let conn = self.lock_connection()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, event_type, subject, source_ref, metadata_json, created_at \
                 FROM provenance_events WHERE subject = ?1 ORDER BY created_at",
            )
            .map_err(EngineError::Sqlite)?;
        let events = stmt
            .query_map(rusqlite::params![subject], |row| {
                Ok(ProvenanceEvent {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    subject: row.get(2)?,
                    source_ref: row.get(3)?,
                    metadata_json: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(EngineError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Sqlite)?;
        Ok(events)
    }
}

fn wrap_node_row_projection_sql(base_sql: &str) -> String {
    format!(
        "SELECT q.row_id, q.logical_id, q.kind, q.properties, q.content_ref, am.last_accessed_at \
         FROM ({base_sql}) q \
         LEFT JOIN node_access_metadata am ON am.logical_id = q.logical_id"
    )
}

/// Returns `true` when `err` indicates the vec virtual table is absent
/// (sqlite-vec feature enabled but `vec_nodes_active` not yet created).
pub(crate) fn is_vec_table_absent(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => {
            msg.contains("vec_nodes_active") || msg.contains("no such module: vec0")
        }
        _ => false,
    }
}

fn scalar_to_bind(value: &ScalarValue) -> BindValue {
    match value {
        ScalarValue::Text(text) => BindValue::Text(text.clone()),
        ScalarValue::Integer(integer) => BindValue::Integer(*integer),
        ScalarValue::Bool(boolean) => BindValue::Bool(*boolean),
    }
}

/// Merge strict and relaxed search branches into a single block-ordered,
/// deduplicated, limit-truncated hit list.
///
/// Phase 3 rules, in order:
///
/// 1. Each branch is sorted internally by score descending with `logical_id`
///    ascending as the deterministic tiebreak.
/// 2. Within a single branch, if the same `logical_id` appears twice (e.g.
///    once from the chunk surface and once from the property surface) the
///    higher-score row wins, then chunk > property > vector, then declaration
///    order (chunk first).
/// 3. Strict hits form one block and relaxed hits form the next. Strict
///    always precedes relaxed in the merged output regardless of per-hit
///    score.
/// 4. Cross-branch dedup is strict-wins: any relaxed hit whose `logical_id`
///    already appears in the strict block is dropped.
/// 5. The merged output is truncated to `limit`.
fn merge_search_branches(
    strict: Vec<SearchHit>,
    relaxed: Vec<SearchHit>,
    limit: usize,
) -> Vec<SearchHit> {
    merge_search_branches_three(strict, relaxed, Vec::new(), limit)
}

/// Three-branch generalization of [`merge_search_branches`]: orders hits as
/// (strict block, relaxed block, vector block) per addendum 1 §Fusion
/// Semantics, with cross-branch dedup resolved by branch precedence
/// (strict > relaxed > vector). Within each block the existing
/// [`dedup_branch_hits`] rule applies (score desc, `logical_id` asc, source
/// priority chunk > property > vector).
///
/// Phase 12 (the unified `search()` entry point) calls this directly. The
/// two-branch [`merge_search_branches`] wrapper is preserved as a
/// convenience for the text-only `execute_compiled_search_plan` path; both
/// reduce to the same code.
fn merge_search_branches_three(
    strict: Vec<SearchHit>,
    relaxed: Vec<SearchHit>,
    vector: Vec<SearchHit>,
    limit: usize,
) -> Vec<SearchHit> {
    let strict_block = dedup_branch_hits(strict);
    let relaxed_block = dedup_branch_hits(relaxed);
    let vector_block = dedup_branch_hits(vector);

    let mut seen: std::collections::HashSet<String> = strict_block
        .iter()
        .map(|h| h.node.logical_id.clone())
        .collect();

    let mut merged = strict_block;
    for hit in relaxed_block {
        if seen.insert(hit.node.logical_id.clone()) {
            merged.push(hit);
        }
    }
    for hit in vector_block {
        if seen.insert(hit.node.logical_id.clone()) {
            merged.push(hit);
        }
    }

    if merged.len() > limit {
        merged.truncate(limit);
    }
    merged
}

/// Sort a branch's hits by score descending + `logical_id` ascending, then
/// dedup duplicate `logical_id`s within the branch using source priority
/// (chunk > property > vector) and declaration order.
fn dedup_branch_hits(mut hits: Vec<SearchHit>) -> Vec<SearchHit> {
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.node.logical_id.cmp(&b.node.logical_id))
            .then_with(|| source_priority(a.source).cmp(&source_priority(b.source)))
    });

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    hits.retain(|hit| seen.insert(hit.node.logical_id.clone()));
    hits
}

fn source_priority(source: SearchHitSource) -> u8 {
    // Lower is better. Chunk is declared before property in the CTE; vector
    // is reserved for future wiring but comes last among the known variants.
    match source {
        SearchHitSource::Chunk => 0,
        SearchHitSource::Property => 1,
        SearchHitSource::Vector => 2,
    }
}

/// Sentinel markers used to wrap FTS5-matched terms in the `highlight()`
/// output so the coordinator can recover per-term byte offsets in the
/// original `text_content` column.
///
/// Each sentinel is a single `U+0001` ("start of heading") / `U+0002`
/// ("start of text") byte. These bytes are safe for all valid JSON text
/// *except* deliberately escape-injected `\u0001` / `\u0002` sequences: a
/// payload like `{"x":"\u0001"}` decodes to a literal 0x01 byte in the
/// extracted blob, making the sentinel ambiguous for that row. Such input
/// is treated as out of scope for attribution correctness — hits on those
/// rows may have misattributed `matched_paths`, but no panic or query
/// failure occurs. A future hardening step could strip bytes < 0x20 at
/// blob-emission time in `RecursiveWalker::emit_leaf` to close this gap.
///
/// Using one-byte markers keeps the original-to-highlighted offset
/// accounting trivial: every sentinel adds exactly one byte to the
/// highlighted string.
const ATTRIBUTION_HIGHLIGHT_OPEN: &str = "\x01";
const ATTRIBUTION_HIGHLIGHT_CLOSE: &str = "\x02";

/// Load the `fts_node_property_positions` sidecar rows for a given
/// `(logical_id, kind)` ordered by `start_offset`. Returns a vector of
/// `(start_offset, end_offset, leaf_path)` tuples ready for binary search.
fn load_position_map(
    conn: &Connection,
    logical_id: &str,
    kind: &str,
) -> Result<Vec<(usize, usize, String)>, EngineError> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT start_offset, end_offset, leaf_path \
             FROM fts_node_property_positions \
             WHERE node_logical_id = ?1 AND kind = ?2 \
             ORDER BY start_offset ASC",
        )
        .map_err(EngineError::Sqlite)?;
    let rows = stmt
        .query_map(rusqlite::params![logical_id, kind], |row| {
            let start: i64 = row.get(0)?;
            let end: i64 = row.get(1)?;
            let path: String = row.get(2)?;
            // Offsets are non-negative and within blob byte limits; on the
            // off chance a corrupt row is encountered, fall back to 0 so
            // lookups silently skip it rather than panicking.
            let start = usize::try_from(start).unwrap_or(0);
            let end = usize::try_from(end).unwrap_or(0);
            Ok((start, end, path))
        })
        .map_err(EngineError::Sqlite)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(EngineError::Sqlite)?);
    }
    Ok(out)
}

/// Parse a `highlight()`-wrapped string, returning the list of original-text
/// byte offsets at which matched terms begin. `wrapped` is the
/// highlight-decorated form of a text column; `open` / `close` are the
/// sentinel markers passed to `highlight()`. The returned offsets refer to
/// positions in the *original* text (i.e. the column as it would be stored
/// without highlight decoration).
fn parse_highlight_offsets(wrapped: &str, open: &str, close: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let bytes = wrapped.as_bytes();
    let open_bytes = open.as_bytes();
    let close_bytes = close.as_bytes();
    let mut i = 0usize;
    // Number of sentinel bytes consumed so far — every marker encountered
    // subtracts from the wrapped-string index to get the original offset.
    let mut marker_bytes_seen = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(open_bytes) {
            // Record the original-text offset of the term following the open
            // marker.
            let original_offset = i - marker_bytes_seen;
            offsets.push(original_offset);
            i += open_bytes.len();
            marker_bytes_seen += open_bytes.len();
        } else if bytes[i..].starts_with(close_bytes) {
            i += close_bytes.len();
            marker_bytes_seen += close_bytes.len();
        } else {
            i += 1;
        }
    }
    offsets
}

/// Binary-search the position map for the leaf whose `[start, end)` range
/// contains `offset`. Returns `None` if no leaf covers the offset.
fn find_leaf_for_offset(positions: &[(usize, usize, String)], offset: usize) -> Option<&str> {
    // Binary search for the greatest start_offset <= offset.
    let idx = match positions.binary_search_by(|entry| entry.0.cmp(&offset)) {
        Ok(i) => i,
        Err(0) => return None,
        Err(i) => i - 1,
    };
    let (start, end, path) = &positions[idx];
    if offset >= *start && offset < *end {
        Some(path.as_str())
    } else {
        None
    }
}

/// Resolve per-hit match attribution by introspecting the FTS5 match state
/// for the hit's row via `highlight()` and mapping the resulting original-
/// text offsets back to recursive-leaf paths via the Phase 4 position map.
///
/// Chunk-backed hits carry no leaf structure and always return an empty
/// `matched_paths` vector. Property-backed hits without a `projection_row_id`
/// (which should not happen — the search CTE always populates it) also
/// return empty attribution rather than erroring.
fn resolve_hit_attribution(
    conn: &Connection,
    hit: &SearchHit,
    match_expr: &str,
) -> Result<HitAttribution, EngineError> {
    if !matches!(hit.source, SearchHitSource::Property) {
        return Ok(HitAttribution {
            matched_paths: Vec::new(),
        });
    }
    let Some(rowid_str) = hit.projection_row_id.as_deref() else {
        return Ok(HitAttribution {
            matched_paths: Vec::new(),
        });
    };
    let rowid: i64 = match rowid_str.parse() {
        Ok(v) => v,
        Err(_) => {
            return Ok(HitAttribution {
                matched_paths: Vec::new(),
            });
        }
    };

    // Fetch the highlight-wrapped text_content for this hit's FTS row. The
    // FTS5 MATCH in the WHERE clause re-establishes the match state that
    // `highlight()` needs to decorate the returned text.
    let mut stmt = conn
        .prepare_cached(
            "SELECT highlight(fts_node_properties, 2, ?1, ?2) \
             FROM fts_node_properties \
             WHERE rowid = ?3 AND fts_node_properties MATCH ?4",
        )
        .map_err(EngineError::Sqlite)?;
    let wrapped: Option<String> = stmt
        .query_row(
            rusqlite::params![
                ATTRIBUTION_HIGHLIGHT_OPEN,
                ATTRIBUTION_HIGHLIGHT_CLOSE,
                rowid,
                match_expr,
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(EngineError::Sqlite)?;
    let Some(wrapped) = wrapped else {
        return Ok(HitAttribution {
            matched_paths: Vec::new(),
        });
    };

    let offsets = parse_highlight_offsets(
        &wrapped,
        ATTRIBUTION_HIGHLIGHT_OPEN,
        ATTRIBUTION_HIGHLIGHT_CLOSE,
    );
    if offsets.is_empty() {
        return Ok(HitAttribution {
            matched_paths: Vec::new(),
        });
    }

    let positions = load_position_map(conn, &hit.node.logical_id, &hit.node.kind)?;
    if positions.is_empty() {
        // Scalar-only schemas have no position-map entries; attribution
        // degrades to an empty vector rather than erroring.
        return Ok(HitAttribution {
            matched_paths: Vec::new(),
        });
    }

    let mut matched_paths: Vec<String> = Vec::new();
    for offset in offsets {
        if let Some(path) = find_leaf_for_offset(&positions, offset)
            && !matched_paths.iter().any(|p| p == path)
        {
            matched_paths.push(path.to_owned());
        }
    }
    Ok(HitAttribution { matched_paths })
}

fn bind_value_to_sql(value: &fathomdb_query::BindValue) -> Value {
    match value {
        fathomdb_query::BindValue::Text(text) => Value::Text(text.clone()),
        fathomdb_query::BindValue::Integer(integer) => Value::Integer(*integer),
        fathomdb_query::BindValue::Bool(boolean) => Value::Integer(i64::from(*boolean)),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Arc;

    use fathomdb_query::{BindValue, QueryBuilder};
    use fathomdb_schema::SchemaManager;
    use rusqlite::types::Value;
    use tempfile::NamedTempFile;

    use crate::{EngineError, ExecutionCoordinator, TelemetryCounters};

    use fathomdb_query::{
        NodeRowLite, RetrievalModality, SearchHit, SearchHitSource, SearchMatchMode,
    };

    use super::{
        bind_value_to_sql, is_vec_table_absent, merge_search_branches, merge_search_branches_three,
        wrap_node_row_projection_sql,
    };

    fn mk_hit(
        logical_id: &str,
        score: f64,
        match_mode: SearchMatchMode,
        source: SearchHitSource,
    ) -> SearchHit {
        SearchHit {
            node: NodeRowLite {
                row_id: format!("{logical_id}-row"),
                logical_id: logical_id.to_owned(),
                kind: "Goal".to_owned(),
                properties: "{}".to_owned(),
                content_ref: None,
                last_accessed_at: None,
            },
            score,
            modality: RetrievalModality::Text,
            source,
            match_mode: Some(match_mode),
            snippet: None,
            written_at: 0,
            projection_row_id: None,
            vector_distance: None,
            attribution: None,
        }
    }

    #[test]
    fn merge_places_strict_block_before_relaxed_regardless_of_score() {
        let strict = vec![mk_hit(
            "a",
            1.0,
            SearchMatchMode::Strict,
            SearchHitSource::Chunk,
        )];
        // Relaxed has a higher score but must still come second.
        let relaxed = vec![mk_hit(
            "b",
            9.9,
            SearchMatchMode::Relaxed,
            SearchHitSource::Chunk,
        )];
        let merged = merge_search_branches(strict, relaxed, 10);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].node.logical_id, "a");
        assert!(matches!(
            merged[0].match_mode,
            Some(SearchMatchMode::Strict)
        ));
        assert_eq!(merged[1].node.logical_id, "b");
        assert!(matches!(
            merged[1].match_mode,
            Some(SearchMatchMode::Relaxed)
        ));
    }

    #[test]
    fn merge_dedup_keeps_strict_over_relaxed_for_same_logical_id() {
        let strict = vec![mk_hit(
            "shared",
            1.0,
            SearchMatchMode::Strict,
            SearchHitSource::Chunk,
        )];
        let relaxed = vec![
            mk_hit(
                "shared",
                9.9,
                SearchMatchMode::Relaxed,
                SearchHitSource::Chunk,
            ),
            mk_hit(
                "other",
                2.0,
                SearchMatchMode::Relaxed,
                SearchHitSource::Chunk,
            ),
        ];
        let merged = merge_search_branches(strict, relaxed, 10);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].node.logical_id, "shared");
        assert!(matches!(
            merged[0].match_mode,
            Some(SearchMatchMode::Strict)
        ));
        assert_eq!(merged[1].node.logical_id, "other");
        assert!(matches!(
            merged[1].match_mode,
            Some(SearchMatchMode::Relaxed)
        ));
    }

    #[test]
    fn merge_sorts_within_block_by_score_desc_then_logical_id() {
        let strict = vec![
            mk_hit("b", 1.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
            mk_hit("a", 2.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
            mk_hit("c", 2.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
        ];
        let merged = merge_search_branches(strict, vec![], 10);
        assert_eq!(
            merged
                .iter()
                .map(|h| &h.node.logical_id)
                .collect::<Vec<_>>(),
            vec!["a", "c", "b"]
        );
    }

    #[test]
    fn merge_dedup_within_branch_prefers_chunk_over_property_at_equal_score() {
        let strict = vec![
            mk_hit(
                "shared",
                1.0,
                SearchMatchMode::Strict,
                SearchHitSource::Property,
            ),
            mk_hit(
                "shared",
                1.0,
                SearchMatchMode::Strict,
                SearchHitSource::Chunk,
            ),
        ];
        let merged = merge_search_branches(strict, vec![], 10);
        assert_eq!(merged.len(), 1);
        assert!(matches!(merged[0].source, SearchHitSource::Chunk));
    }

    #[test]
    fn merge_truncates_to_limit_after_block_merge() {
        let strict = vec![
            mk_hit("a", 2.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
            mk_hit("b", 1.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
        ];
        let relaxed = vec![mk_hit(
            "c",
            9.0,
            SearchMatchMode::Relaxed,
            SearchHitSource::Chunk,
        )];
        let merged = merge_search_branches(strict, relaxed, 2);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].node.logical_id, "a");
        assert_eq!(merged[1].node.logical_id, "b");
    }

    /// P12 architectural pin: the generalized three-branch merger must
    /// produce strict -> relaxed -> vector block ordering with cross-branch
    /// dedup resolved by branch precedence (strict > relaxed > vector).
    /// v1 `search()` policy never fires the vector branch through the
    /// unified planner because read-time embedding is deferred, but the
    /// merge helper itself must be ready for the day the planner does so —
    /// otherwise wiring the future phase requires touching the core merge
    /// code as well as the planner.
    #[test]
    fn search_architecturally_supports_three_branch_fusion() {
        let strict = vec![mk_hit(
            "alpha",
            1.0,
            SearchMatchMode::Strict,
            SearchHitSource::Chunk,
        )];
        let relaxed = vec![mk_hit(
            "bravo",
            5.0,
            SearchMatchMode::Relaxed,
            SearchHitSource::Chunk,
        )];
        // Synthetic vector hit with the highest score. Three-block ordering
        // must still place it last.
        let mut vector_hit = mk_hit(
            "charlie",
            9.9,
            SearchMatchMode::Strict,
            SearchHitSource::Vector,
        );
        // Vector hits actually carry match_mode=None per the addendum, but
        // the merge helper's ordering is mode-agnostic; we override here to
        // pin the modality field for the test.
        vector_hit.match_mode = None;
        vector_hit.modality = RetrievalModality::Vector;
        let vector = vec![vector_hit];

        let merged = merge_search_branches_three(strict, relaxed, vector, 10);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].node.logical_id, "alpha");
        assert_eq!(merged[1].node.logical_id, "bravo");
        assert_eq!(merged[2].node.logical_id, "charlie");
        // Vector block comes last regardless of its higher score.
        assert!(matches!(merged[2].source, SearchHitSource::Vector));

        // Cross-branch dedup: a logical_id that appears in multiple branches
        // is attributed to its highest-priority originating branch only.
        let strict2 = vec![mk_hit(
            "shared",
            0.5,
            SearchMatchMode::Strict,
            SearchHitSource::Chunk,
        )];
        let relaxed2 = vec![mk_hit(
            "shared",
            5.0,
            SearchMatchMode::Relaxed,
            SearchHitSource::Chunk,
        )];
        let mut vshared = mk_hit(
            "shared",
            9.9,
            SearchMatchMode::Strict,
            SearchHitSource::Vector,
        );
        vshared.match_mode = None;
        vshared.modality = RetrievalModality::Vector;
        let merged2 = merge_search_branches_three(strict2, relaxed2, vec![vshared], 10);
        assert_eq!(merged2.len(), 1, "shared logical_id must dedup to one row");
        assert!(matches!(
            merged2[0].match_mode,
            Some(SearchMatchMode::Strict)
        ));
        assert!(matches!(merged2[0].source, SearchHitSource::Chunk));

        // Relaxed wins over vector when strict is absent.
        let mut vshared2 = mk_hit(
            "shared",
            9.9,
            SearchMatchMode::Strict,
            SearchHitSource::Vector,
        );
        vshared2.match_mode = None;
        vshared2.modality = RetrievalModality::Vector;
        let merged3 = merge_search_branches_three(
            vec![],
            vec![mk_hit(
                "shared",
                1.0,
                SearchMatchMode::Relaxed,
                SearchHitSource::Chunk,
            )],
            vec![vshared2],
            10,
        );
        assert_eq!(merged3.len(), 1);
        assert!(matches!(
            merged3[0].match_mode,
            Some(SearchMatchMode::Relaxed)
        ));
    }

    /// P12-N-3: production-realistic vector-only fusion. The v1 planner
    /// never fires this shape today (read-time embedding is deferred), but
    /// when it does the merger will see empty strict + empty relaxed + a
    /// non-empty vector block. The three-branch merger must pass that
    /// block through unchanged, preserving `RetrievalModality::Vector`,
    /// `SearchHitSource::Vector`, and `match_mode == None` semantics.
    ///
    /// Note: the review spec asked for `vector_hit_count == 1` /
    /// `strict_hit_count == 0` assertions. Those are fields on
    /// `SearchRows`, which is assembled one layer up in
    /// `execute_compiled_retrieval_plan`. The merger returns a bare
    /// `Vec<SearchHit>`, so this test asserts the corresponding invariants
    /// directly on the returned vec (block shape + per-hit fields).
    #[test]
    fn merge_search_branches_three_vector_only_preserves_vector_block() {
        let mut vector_hit = mk_hit(
            "solo",
            0.75,
            SearchMatchMode::Strict,
            SearchHitSource::Vector,
        );
        vector_hit.match_mode = None;
        vector_hit.modality = RetrievalModality::Vector;

        let merged = merge_search_branches_three(vec![], vec![], vec![vector_hit], 10);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].node.logical_id, "solo");
        assert!(matches!(merged[0].source, SearchHitSource::Vector));
        assert!(matches!(merged[0].modality, RetrievalModality::Vector));
        assert!(
            merged[0].match_mode.is_none(),
            "vector hits carry match_mode=None per addendum 1"
        );
    }

    /// P12-N-3: limit truncation must preserve block precedence — when the
    /// strict block alone already exceeds the limit, relaxed and vector
    /// hits must be dropped entirely even if they have higher raw scores.
    ///
    /// Note: the review spec asked for `strict_hit_count == 2` /
    /// `relaxed_hit_count == 0` / `vector_hit_count == 0` assertions, which
    /// are `SearchRows` fields assembled one layer up. Since
    /// `merge_search_branches_three` only returns a `Vec<SearchHit>`, this
    /// test asserts the corresponding invariants directly: the returned
    /// vec contains exactly the top two strict hits, with no relaxed or
    /// vector hits leaking past the limit.
    #[test]
    fn merge_search_branches_three_limit_truncates_preserving_block_precedence() {
        let strict = vec![
            mk_hit("a", 3.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
            mk_hit("b", 2.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
            mk_hit("c", 1.0, SearchMatchMode::Strict, SearchHitSource::Chunk),
        ];
        let relaxed = vec![mk_hit(
            "d",
            9.0,
            SearchMatchMode::Relaxed,
            SearchHitSource::Chunk,
        )];
        let mut vector_hit = mk_hit("e", 9.5, SearchMatchMode::Strict, SearchHitSource::Vector);
        vector_hit.match_mode = None;
        vector_hit.modality = RetrievalModality::Vector;
        let vector = vec![vector_hit];

        let merged = merge_search_branches_three(strict, relaxed, vector, 2);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].node.logical_id, "a");
        assert_eq!(merged[1].node.logical_id, "b");
        // Neither relaxed nor vector hits made it past the limit.
        assert!(
            merged
                .iter()
                .all(|h| matches!(h.match_mode, Some(SearchMatchMode::Strict))),
            "strict block must win limit contention against higher-scored relaxed/vector hits"
        );
        assert!(
            merged
                .iter()
                .all(|h| matches!(h.source, SearchHitSource::Chunk)),
            "no vector source hits should leak past the limit"
        );
    }

    #[test]
    fn is_vec_table_absent_matches_known_error_messages() {
        use rusqlite::ffi;
        fn make_err(msg: &str) -> rusqlite::Error {
            rusqlite::Error::SqliteFailure(
                ffi::Error {
                    code: ffi::ErrorCode::Unknown,
                    extended_code: 1,
                },
                Some(msg.to_owned()),
            )
        }
        assert!(is_vec_table_absent(&make_err(
            "no such table: vec_nodes_active"
        )));
        assert!(is_vec_table_absent(&make_err("no such module: vec0")));
        assert!(!is_vec_table_absent(&make_err("vec0 constraint violated")));
        assert!(!is_vec_table_absent(&make_err("no such table: nodes")));
        assert!(!is_vec_table_absent(&rusqlite::Error::QueryReturnedNoRows));
    }

    #[test]
    fn bind_value_text_maps_to_sql_text() {
        let val = bind_value_to_sql(&BindValue::Text("hello".to_owned()));
        assert_eq!(val, Value::Text("hello".to_owned()));
    }

    #[test]
    fn bind_value_integer_maps_to_sql_integer() {
        let val = bind_value_to_sql(&BindValue::Integer(42));
        assert_eq!(val, Value::Integer(42));
    }

    #[test]
    fn bind_value_bool_true_maps_to_integer_one() {
        let val = bind_value_to_sql(&BindValue::Bool(true));
        assert_eq!(val, Value::Integer(1));
    }

    #[test]
    fn bind_value_bool_false_maps_to_integer_zero() {
        let val = bind_value_to_sql(&BindValue::Bool(false));
        assert_eq!(val, Value::Integer(0));
    }

    #[test]
    fn same_shape_queries_share_one_cache_entry() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled_a = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .limit(10)
            .compile()
            .expect("compiled a");
        let compiled_b = QueryBuilder::nodes("Meeting")
            .text_search("standup", 5)
            .limit(10)
            .compile()
            .expect("compiled b");

        coordinator
            .execute_compiled_read(&compiled_a)
            .expect("read a");
        coordinator
            .execute_compiled_read(&compiled_b)
            .expect("read b");

        assert_eq!(
            compiled_a.shape_hash, compiled_b.shape_hash,
            "different bind values, same structural shape → same hash"
        );
        assert_eq!(coordinator.shape_sql_count(), 1);
    }

    #[test]
    fn vector_read_degrades_gracefully_when_vec_table_absent() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .vector_search("budget embeddings", 5)
            .compile()
            .expect("vector query compiles");

        let result = coordinator.execute_compiled_read(&compiled);
        let rows = result.expect("degraded read must succeed, not error");
        assert!(
            rows.was_degraded,
            "result must be flagged as degraded when vec_nodes_active is absent"
        );
        assert!(
            rows.nodes.is_empty(),
            "degraded result must return empty nodes"
        );
    }

    #[test]
    fn coordinator_caches_by_shape_hash() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        coordinator
            .execute_compiled_read(&compiled)
            .expect("execute compiled read");
        assert_eq!(coordinator.shape_sql_count(), 1);
    }

    // --- Item 6: explain_compiled_read tests ---

    #[test]
    fn explain_returns_correct_sql() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        let plan = coordinator.explain_compiled_read(&compiled);

        assert_eq!(plan.sql, wrap_node_row_projection_sql(&compiled.sql));
    }

    #[test]
    fn explain_returns_correct_driving_table() {
        use fathomdb_query::DrivingTable;

        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        let plan = coordinator.explain_compiled_read(&compiled);

        assert_eq!(plan.driving_table, DrivingTable::FtsNodes);
    }

    #[test]
    fn explain_reports_cache_miss_then_hit() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        // Before execution: cache miss
        let plan_before = coordinator.explain_compiled_read(&compiled);
        assert!(
            !plan_before.cache_hit,
            "cache miss expected before first execute"
        );

        // Execute to populate cache
        coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        // After execution: cache hit
        let plan_after = coordinator.explain_compiled_read(&compiled);
        assert!(
            plan_after.cache_hit,
            "cache hit expected after first execute"
        );
    }

    #[test]
    fn explain_does_not_execute_query() {
        // Call explain_compiled_read on an empty database. If explain were
        // actually executing SQL, it would return Ok with 0 rows. But the
        // key assertion is that it returns a QueryPlan (not an error) even
        // without touching the database.
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("anything", 5)
            .compile()
            .expect("compiled query");

        // This must not error, even though the database is empty
        let plan = coordinator.explain_compiled_read(&compiled);

        assert!(!plan.sql.is_empty(), "plan must carry the SQL text");
        assert_eq!(plan.bind_count, compiled.binds.len());
    }

    #[test]
    fn coordinator_executes_compiled_read() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let conn = rusqlite::Connection::open(db.path()).expect("open db");

        conn.execute_batch(
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at)
            VALUES ('row-1', 'meeting-1', 'Meeting', '{"status":"active"}', unixepoch());
            INSERT INTO chunks (id, node_logical_id, text_content, created_at)
            VALUES ('chunk-1', 'meeting-1', 'budget discussion', unixepoch());
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            VALUES ('chunk-1', 'meeting-1', 'Meeting', 'budget discussion');
            "#,
        )
        .expect("seed data");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .limit(5)
            .compile()
            .expect("compiled query");

        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        assert_eq!(rows.nodes.len(), 1);
        assert_eq!(rows.nodes[0].logical_id, "meeting-1");
    }

    #[test]
    fn text_search_finds_structured_only_node_via_property_fts() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let conn = rusqlite::Connection::open(db.path()).expect("open db");

        // Insert a structured-only node (no chunks) with a property FTS row.
        conn.execute_batch(
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-1', 'goal-1', 'Goal', '{"name":"Ship v2"}', 100, 'seed');
            INSERT INTO fts_node_properties (node_logical_id, kind, text_content)
            VALUES ('goal-1', 'Goal', 'Ship v2');
            "#,
        )
        .expect("seed data");

        let compiled = QueryBuilder::nodes("Goal")
            .text_search("Ship", 5)
            .limit(5)
            .compile()
            .expect("compiled query");

        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        assert_eq!(rows.nodes.len(), 1);
        assert_eq!(rows.nodes[0].logical_id, "goal-1");
    }

    #[test]
    fn text_search_returns_both_chunk_and_property_backed_hits() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let conn = rusqlite::Connection::open(db.path()).expect("open db");

        // Chunk-backed hit: a Meeting with a chunk containing "quarterly".
        conn.execute_batch(
            r"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 'seed');
            INSERT INTO chunks (id, node_logical_id, text_content, created_at)
            VALUES ('chunk-1', 'meeting-1', 'quarterly budget review', 100);
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            VALUES ('chunk-1', 'meeting-1', 'Meeting', 'quarterly budget review');
            ",
        )
        .expect("seed chunk-backed node");

        // Property-backed hit: a Meeting with property FTS containing "quarterly".
        conn.execute_batch(
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-2', 'meeting-2', 'Meeting', '{"title":"quarterly sync"}', 100, 'seed');
            INSERT INTO fts_node_properties (node_logical_id, kind, text_content)
            VALUES ('meeting-2', 'Meeting', 'quarterly sync');
            "#,
        )
        .expect("seed property-backed node");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("quarterly", 10)
            .limit(10)
            .compile()
            .expect("compiled query");

        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        let mut ids: Vec<&str> = rows.nodes.iter().map(|r| r.logical_id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["meeting-1", "meeting-2"]);
    }

    #[test]
    fn text_search_finds_literal_lowercase_not_text_in_chunk_content() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let conn = rusqlite::Connection::open(db.path()).expect("open db");

        conn.execute_batch(
            r"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 'seed');
            INSERT INTO chunks (id, node_logical_id, text_content, created_at)
            VALUES ('chunk-1', 'meeting-1', 'the boat is not a ship', 100);
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            VALUES ('chunk-1', 'meeting-1', 'Meeting', 'the boat is not a ship');
            ",
        )
        .expect("seed chunk-backed node");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("not a ship", 10)
            .limit(10)
            .compile()
            .expect("compiled query");

        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        assert_eq!(rows.nodes.len(), 1);
        assert_eq!(rows.nodes[0].logical_id, "meeting-1");
    }

    // --- Item 1: capability gate tests ---

    #[test]
    fn capability_gate_reports_false_without_feature() {
        let db = NamedTempFile::new().expect("temporary db");
        // Open without vector_dimension: regardless of feature flag, vector_enabled must be false
        // when no dimension is requested (the vector profile is never bootstrapped).
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        assert!(
            !coordinator.vector_enabled(),
            "vector_enabled must be false when no dimension is requested"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn capability_gate_reports_true_when_feature_enabled() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            Some(128),
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        assert!(
            coordinator.vector_enabled(),
            "vector_enabled must be true when sqlite-vec feature is active and dimension is set"
        );
    }

    // --- Item 4: runtime table read tests ---

    #[test]
    fn read_run_returns_inserted_run() {
        use crate::{ProvenanceMode, RunInsert, WriteRequest, WriterActor};

        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
            Arc::new(TelemetryCounters::default()),
        )
        .expect("writer");
        writer
            .submit(WriteRequest {
                label: "runtime".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-r1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write run");

        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let row = coordinator
            .read_run("run-r1")
            .expect("read_run")
            .expect("row exists");
        assert_eq!(row.id, "run-r1");
        assert_eq!(row.kind, "session");
        assert_eq!(row.status, "active");
    }

    #[test]
    fn read_step_returns_inserted_step() {
        use crate::{ProvenanceMode, RunInsert, WriteRequest, WriterActor, writer::StepInsert};

        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
            Arc::new(TelemetryCounters::default()),
        )
        .expect("writer");
        writer
            .submit(WriteRequest {
                label: "runtime".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-s1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-s1".to_owned(),
                    run_id: "run-s1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write step");

        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let row = coordinator
            .read_step("step-s1")
            .expect("read_step")
            .expect("row exists");
        assert_eq!(row.id, "step-s1");
        assert_eq!(row.run_id, "run-s1");
        assert_eq!(row.kind, "llm");
    }

    #[test]
    fn read_action_returns_inserted_action() {
        use crate::{
            ProvenanceMode, RunInsert, WriteRequest, WriterActor,
            writer::{ActionInsert, StepInsert},
        };

        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
            Arc::new(TelemetryCounters::default()),
        )
        .expect("writer");
        writer
            .submit(WriteRequest {
                label: "runtime".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-a1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![StepInsert {
                    id: "step-a1".to_owned(),
                    run_id: "run-a1".to_owned(),
                    kind: "llm".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                actions: vec![ActionInsert {
                    id: "action-a1".to_owned(),
                    step_id: "step-a1".to_owned(),
                    kind: "emit".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write action");

        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let row = coordinator
            .read_action("action-a1")
            .expect("read_action")
            .expect("row exists");
        assert_eq!(row.id, "action-a1");
        assert_eq!(row.step_id, "step-a1");
        assert_eq!(row.kind, "emit");
    }

    #[test]
    fn read_active_runs_excludes_superseded() {
        use crate::{ProvenanceMode, RunInsert, WriteRequest, WriterActor};

        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
            Arc::new(TelemetryCounters::default()),
        )
        .expect("writer");

        // Insert original run
        writer
            .submit(WriteRequest {
                label: "v1".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-v1".to_owned(),
                    kind: "session".to_owned(),
                    status: "active".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-1".to_owned()),
                    upsert: false,
                    supersedes_id: None,
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v1 write");

        // Supersede original run with v2
        writer
            .submit(WriteRequest {
                label: "v2".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![RunInsert {
                    id: "run-v2".to_owned(),
                    kind: "session".to_owned(),
                    status: "completed".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("src-2".to_owned()),
                    upsert: true,
                    supersedes_id: Some("run-v1".to_owned()),
                }],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("v2 write");

        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let active = coordinator.read_active_runs().expect("read_active_runs");

        assert_eq!(active.len(), 1, "only the non-superseded run should appear");
        assert_eq!(active[0].id, "run-v2");
    }

    #[allow(clippy::panic)]
    fn poison_connection(coordinator: &ExecutionCoordinator) {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = coordinator.pool.connections[0]
                .lock()
                .expect("poison test lock");
            panic!("poison coordinator connection mutex");
        }));
        assert!(
            result.is_err(),
            "poison test must unwind while holding the connection mutex"
        );
    }

    #[allow(clippy::panic)]
    fn assert_poisoned_connection_error<T, F>(coordinator: &ExecutionCoordinator, op: F)
    where
        F: FnOnce(&ExecutionCoordinator) -> Result<T, EngineError>,
    {
        match op(coordinator) {
            Err(EngineError::Bridge(message)) => {
                assert_eq!(message, "connection mutex poisoned");
            }
            Ok(_) => panic!("expected poisoned connection error, got Ok(_)"),
            Err(error) => panic!("expected poisoned connection error, got {error:?}"),
        }
    }

    #[test]
    fn poisoned_connection_returns_bridge_error_for_read_helpers() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        poison_connection(&coordinator);

        assert_poisoned_connection_error(&coordinator, |c| c.read_run("run-r1"));
        assert_poisoned_connection_error(&coordinator, |c| c.read_step("step-s1"));
        assert_poisoned_connection_error(&coordinator, |c| c.read_action("action-a1"));
        assert_poisoned_connection_error(
            &coordinator,
            super::ExecutionCoordinator::read_active_runs,
        );
        assert_poisoned_connection_error(&coordinator, |c| c.raw_pragma("journal_mode"));
        assert_poisoned_connection_error(&coordinator, |c| c.query_provenance_events("source-1"));
    }

    // --- M-2: Bounded shape cache ---

    #[test]
    fn shape_cache_stays_bounded() {
        use fathomdb_query::ShapeHash;

        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        // Directly populate the cache with MAX_SHAPE_CACHE_SIZE + 1 entries.
        {
            let mut cache = coordinator.shape_sql_map.lock().expect("lock shape cache");
            for i in 0..=super::MAX_SHAPE_CACHE_SIZE {
                cache.insert(ShapeHash(i as u64), format!("SELECT {i}"));
            }
        }
        // The cache is now over the limit but hasn't been pruned yet (pruning
        // happens on the insert path in execute_compiled_read).

        // Execute a compiled read to trigger the bounded-cache check.
        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .limit(10)
            .compile()
            .expect("compiled query");

        coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        assert!(
            coordinator.shape_sql_count() <= super::MAX_SHAPE_CACHE_SIZE,
            "shape cache must stay bounded: got {} entries, max {}",
            coordinator.shape_sql_count(),
            super::MAX_SHAPE_CACHE_SIZE
        );
    }

    // --- M-1: Read pool size ---

    #[test]
    fn read_pool_size_configurable() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            2,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator with pool_size=2");

        assert_eq!(coordinator.pool.size(), 2);

        // Basic read should succeed through the pool.
        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .limit(10)
            .compile()
            .expect("compiled query");

        let result = coordinator.execute_compiled_read(&compiled);
        assert!(result.is_ok(), "read through pool must succeed");
    }

    // --- M-4: Grouped read batching ---

    #[test]
    fn grouped_read_results_match_baseline() {
        use fathomdb_query::TraverseDirection;

        let db = NamedTempFile::new().expect("temporary db");

        // Bootstrap the database via coordinator (creates schema).
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        // Seed data: 10 root nodes (Meeting-0..9) with 2 outbound edges each
        // to expansion nodes (Task-0-a, Task-0-b, etc.).
        {
            let conn = rusqlite::Connection::open(db.path()).expect("open db for seeding");
            for i in 0..10 {
                conn.execute_batch(&format!(
                    r#"
                    INSERT INTO nodes (row_id, logical_id, kind, properties, created_at)
                    VALUES ('row-meeting-{i}', 'meeting-{i}', 'Meeting', '{{"n":{i}}}', unixepoch());
                    INSERT INTO chunks (id, node_logical_id, text_content, created_at)
                    VALUES ('chunk-m-{i}', 'meeting-{i}', 'meeting search text {i}', unixepoch());
                    INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
                    VALUES ('chunk-m-{i}', 'meeting-{i}', 'Meeting', 'meeting search text {i}');

                    INSERT INTO nodes (row_id, logical_id, kind, properties, created_at)
                    VALUES ('row-task-{i}-a', 'task-{i}-a', 'Task', '{{"parent":{i},"sub":"a"}}', unixepoch());
                    INSERT INTO nodes (row_id, logical_id, kind, properties, created_at)
                    VALUES ('row-task-{i}-b', 'task-{i}-b', 'Task', '{{"parent":{i},"sub":"b"}}', unixepoch());

                    INSERT INTO edges (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at)
                    VALUES ('edge-{i}-a', 'edge-lid-{i}-a', 'meeting-{i}', 'task-{i}-a', 'HAS_TASK', '{{}}', unixepoch());
                    INSERT INTO edges (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at)
                    VALUES ('edge-{i}-b', 'edge-lid-{i}-b', 'meeting-{i}', 'task-{i}-b', 'HAS_TASK', '{{}}', unixepoch());
                    "#,
                )).expect("seed data");
            }
        }

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("meeting", 10)
            .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1)
            .limit(10)
            .compile_grouped()
            .expect("compiled grouped query");

        let result = coordinator
            .execute_compiled_grouped_read(&compiled)
            .expect("grouped read");

        assert!(!result.was_degraded, "grouped read should not be degraded");
        assert_eq!(result.roots.len(), 10, "expected 10 root nodes");
        assert_eq!(result.expansions.len(), 1, "expected 1 expansion slot");
        assert_eq!(result.expansions[0].slot, "tasks");
        assert_eq!(
            result.expansions[0].roots.len(),
            10,
            "each expansion slot should have entries for all 10 roots"
        );

        // Each root should have exactly 2 expansion nodes (task-X-a, task-X-b).
        for root_expansion in &result.expansions[0].roots {
            assert_eq!(
                root_expansion.nodes.len(),
                2,
                "root {} should have 2 expansion nodes, got {}",
                root_expansion.root_logical_id,
                root_expansion.nodes.len()
            );
        }
    }
}
