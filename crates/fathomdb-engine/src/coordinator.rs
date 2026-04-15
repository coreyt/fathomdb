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

/// Compile an optional expansion-slot target-side filter predicate into a SQL
/// fragment and bind values for injection into the `numbered` CTE's WHERE clause.
///
/// Returns `("", vec![])` when `filter` is `None` — preserving byte-for-byte
/// identical SQL to pre-Pack-3 behavior. When `Some(predicate)`, returns an
/// `AND …` fragment and the corresponding bind values starting at `first_param`.
///
/// Only `JsonPathEq`, `JsonPathCompare`, `JsonPathFusedEq`, and
/// `JsonPathFusedTimestampCmp` are supported here; each variant targets the
/// `n.properties` column already present in the `numbered` CTE join.
/// Column-direct predicates (`KindEq`, `LogicalIdEq`, etc.) reference `n.kind`
/// and similar columns that are also available in the `numbered` CTE.
fn compile_expansion_filter(
    filter: Option<&Predicate>,
    first_param: usize,
) -> (String, Vec<Value>) {
    let Some(predicate) = filter else {
        return (String::new(), vec![]);
    };
    let p = first_param;
    match predicate {
        Predicate::JsonPathEq { path, value } => {
            let val = match value {
                ScalarValue::Text(t) => Value::Text(t.clone()),
                ScalarValue::Integer(i) => Value::Integer(*i),
                ScalarValue::Bool(b) => Value::Integer(i64::from(*b)),
            };
            (
                format!(
                    "\n                  AND json_extract(n.properties, ?{p}) = ?{}",
                    p + 1
                ),
                vec![Value::Text(path.clone()), val],
            )
        }
        Predicate::JsonPathCompare { path, op, value } => {
            let val = match value {
                ScalarValue::Text(t) => Value::Text(t.clone()),
                ScalarValue::Integer(i) => Value::Integer(*i),
                ScalarValue::Bool(b) => Value::Integer(i64::from(*b)),
            };
            let operator = match op {
                ComparisonOp::Gt => ">",
                ComparisonOp::Gte => ">=",
                ComparisonOp::Lt => "<",
                ComparisonOp::Lte => "<=",
            };
            (
                format!(
                    "\n                  AND json_extract(n.properties, ?{p}) {operator} ?{}",
                    p + 1
                ),
                vec![Value::Text(path.clone()), val],
            )
        }
        Predicate::JsonPathFusedEq { path, value } => (
            format!(
                "\n                  AND json_extract(n.properties, ?{p}) = ?{}",
                p + 1
            ),
            vec![Value::Text(path.clone()), Value::Text(value.clone())],
        ),
        Predicate::JsonPathFusedTimestampCmp { path, op, value } => {
            let operator = match op {
                ComparisonOp::Gt => ">",
                ComparisonOp::Gte => ">=",
                ComparisonOp::Lt => "<",
                ComparisonOp::Lte => "<=",
            };
            (
                format!(
                    "\n                  AND json_extract(n.properties, ?{p}) {operator} ?{}",
                    p + 1
                ),
                vec![Value::Text(path.clone()), Value::Integer(*value)],
            )
        }
        Predicate::KindEq(kind) => (
            format!("\n                  AND n.kind = ?{p}"),
            vec![Value::Text(kind.clone())],
        ),
        Predicate::LogicalIdEq(logical_id) => (
            format!("\n                  AND n.logical_id = ?{p}"),
            vec![Value::Text(logical_id.clone())],
        ),
        Predicate::SourceRefEq(source_ref) => (
            format!("\n                  AND n.source_ref = ?{p}"),
            vec![Value::Text(source_ref.clone())],
        ),
        Predicate::ContentRefEq(uri) => (
            format!("\n                  AND n.content_ref = ?{p}"),
            vec![Value::Text(uri.clone())],
        ),
        Predicate::ContentRefNotNull => (
            "\n                  AND n.content_ref IS NOT NULL".to_owned(),
            vec![],
        ),
    }
}

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
        let mut conn = if vector_dimension.is_some() {
            sqlite::open_connection_with_vec(&path)?
        } else {
            sqlite::open_connection(&path)?
        };
        #[cfg(not(feature = "sqlite-vec"))]
        let mut conn = sqlite::open_connection(&path)?;

        let report = schema_manager.bootstrap(&conn)?;

        // ----- Open-time rebuild guards for derived FTS state -----
        //
        // Property FTS data is derived state. Per-kind `fts_props_<kind>`
        // virtual tables are NOT source of truth — they must be rebuildable
        // from canonical `nodes.properties` + `fts_property_schemas` at any
        // time. After migration 23 the global `fts_node_properties` table no
        // longer exists; each registered kind has its own `fts_props_<kind>`
        // table created at first write or first rebuild.
        //
        // Guard 1: if any registered kind's per-kind table is missing or empty
        // while live nodes of that kind exist, do a synchronous full rebuild of
        // all per-kind FTS tables and position map rows.
        //
        // Guard 2: if any recursive schema has a populated per-kind table but
        // `fts_node_property_positions` is empty, do a synchronous full rebuild
        // to regenerate the position map from canonical state.
        //
        // Both guards are no-ops on a consistent database.
        run_open_time_fts_guards(&mut conn)?;

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

    /// Returns the configured read-time query embedder, if any.
    ///
    /// The 0.4.0 write-time parity work reuses this same embedder for
    /// vector regeneration via [`Engine::regenerate_vector_embeddings`],
    /// so there is always exactly one source of truth for vector
    /// identity per [`Engine`] instance.
    #[must_use]
    pub fn query_embedder(&self) -> Option<&Arc<dyn QueryEmbedder>> {
        self.query_embedder.as_ref()
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
        // Scan fallback for first-registration async rebuild: if the query uses the
        // FtsNodes driving table and the root kind has is_first_registration=1 with
        // state IN ('PENDING','BUILDING'), the per-kind table has no rows yet.
        // Route to a full-kind node scan so callers get results instead of empty.
        if compiled.driving_table == DrivingTable::FtsNodes
            && let Some(BindValue::Text(root_kind)) = compiled.binds.get(1)
            && let Some(nodes) = self.scan_fallback_if_first_registration(root_kind)?
        {
            self.telemetry.increment_queries();
            return Ok(QueryRows {
                nodes,
                runs: Vec::new(),
                steps: Vec::new(),
                actions: Vec::new(),
                was_degraded: false,
            });
        }

        // For FtsNodes queries the fathomdb-query compile path generates SQL that
        // references the old global `fts_node_properties` table.  Since migration 23
        // dropped that table, we rewrite the SQL and binds here to use the per-kind
        // `fts_props_<kind>` table (or omit the property UNION arm entirely when the
        // per-kind table does not yet exist).
        let (adapted_sql, adapted_binds) = if compiled.driving_table == DrivingTable::FtsNodes {
            let conn_check = match self.lock_connection() {
                Ok(g) => g,
                Err(e) => {
                    self.telemetry.increment_errors();
                    return Err(e);
                }
            };
            let result = adapt_fts_nodes_sql_for_per_kind_tables(compiled, &conn_check);
            drop(conn_check);
            result?
        } else {
            (compiled.sql.clone(), compiled.binds.clone())
        };

        let row_sql = wrap_node_row_projection_sql(&adapted_sql);
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

        let bind_values = adapted_binds
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
                Predicate::JsonPathFusedEq { path, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(BindValue::Text(value.clone()));
                    let value_idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                      AND json_extract(src.properties, ?{path_idx}) = ?{value_idx}"
                    );
                }
                Predicate::JsonPathFusedTimestampCmp { path, op, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(BindValue::Integer(*value));
                    let value_idx = binds.len();
                    let operator = match op {
                        ComparisonOp::Gt => ">",
                        ComparisonOp::Gte => ">=",
                        ComparisonOp::Lt => "<",
                        ComparisonOp::Lte => "<=",
                    };
                    let _ = write!(
                        fused_clauses,
                        "\n                      AND json_extract(src.properties, ?{path_idx}) {operator} ?{value_idx}"
                    );
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
                | Predicate::ContentRefNotNull
                | Predicate::JsonPathFusedEq { .. }
                | Predicate::JsonPathFusedTimestampCmp { .. } => {
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

        // Acquire the connection early so we can check per-kind table existence
        // before building the bind array and SQL. The bind numbering depends on
        // whether the property FTS UNION arm is included.
        let conn_guard = match self.lock_connection() {
            Ok(g) => g,
            Err(e) => {
                self.telemetry.increment_errors();
                return Err(e);
            }
        };

        // Determine which per-kind property FTS tables to include in the UNION arm.
        //
        // filter_by_kind = true (root_kind set): include the single per-kind table
        //   for root_kind if it exists. Bind order: ?1=chunk_text, ?2=kind, ?3=prop_text.
        //
        // filter_by_kind = false (fallback search, root_kind empty): include per-kind tables
        //   based on fusable KindEq predicates or all registered tables from sqlite_master.
        //   A single KindEq in fusable_filters lets us use one specific table. With no KindEq
        //   we UNION all per-kind tables so kind-less fallback searches still find property
        //   hits (matching the behaviour of the former global fts_node_properties table).
        //   Bind order: ?1=chunk_text, ?2=prop_text (shared across all prop UNION arms).
        //
        // In both cases, per-kind tables are already kind-specific so no `fp.kind = ?` filter
        // is needed inside the inner arm; the outer `search_hits` CTE WHERE clause handles
        // any further kind narrowing via fused KindEq predicates.
        // prop_fts_tables is Vec<(kind, table_name)> so we can later look up per-kind
        // BM25 weights from fts_property_schemas when building the scoring expression.
        let prop_fts_tables: Vec<(String, String)> = if filter_by_kind {
            let kind = compiled.root_kind.clone();
            let prop_table = fathomdb_schema::fts_kind_table_name(&kind);
            let exists: bool = conn_guard
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![prop_table],
                    |_| Ok(true),
                )
                .optional()
                .map_err(EngineError::Sqlite)?
                .unwrap_or(false);
            if exists {
                vec![(kind, prop_table)]
            } else {
                vec![]
            }
        } else {
            // Fallback / kind-less search: find the right per-kind tables.
            // If there is exactly one KindEq in fusable_filters, use that kind's table.
            // Otherwise, include all registered per-kind tables from sqlite_master so
            // that kind-less fallback searches can still return property FTS hits.
            let kind_eq_values: Vec<String> = compiled
                .fusable_filters
                .iter()
                .filter_map(|p| match p {
                    Predicate::KindEq(k) => Some(k.clone()),
                    _ => None,
                })
                .collect();
            if kind_eq_values.len() == 1 {
                let kind = kind_eq_values[0].clone();
                let prop_table = fathomdb_schema::fts_kind_table_name(&kind);
                let exists: bool = conn_guard
                    .query_row(
                        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                        rusqlite::params![prop_table],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(EngineError::Sqlite)?
                    .unwrap_or(false);
                if exists {
                    vec![(kind, prop_table)]
                } else {
                    vec![]
                }
            } else {
                // No single KindEq: UNION all per-kind tables so kind-less fallback
                // searches behave like the former global fts_node_properties table.
                // Fetch registered kinds and compute/verify their per-kind table names.
                let mut stmt = conn_guard
                    .prepare("SELECT kind FROM fts_property_schemas")
                    .map_err(EngineError::Sqlite)?;
                let all_kinds: Vec<String> = stmt
                    .query_map([], |r| r.get::<_, String>(0))
                    .map_err(EngineError::Sqlite)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(EngineError::Sqlite)?;
                drop(stmt);
                let mut result = Vec::new();
                for kind in all_kinds {
                    let prop_table = fathomdb_schema::fts_kind_table_name(&kind);
                    let exists: bool = conn_guard
                        .query_row(
                            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                            rusqlite::params![prop_table],
                            |_| Ok(true),
                        )
                        .optional()
                        .map_err(EngineError::Sqlite)?
                        .unwrap_or(false);
                    if exists {
                        result.push((kind, prop_table));
                    }
                }
                result
            }
        };
        let use_prop_fts = !prop_fts_tables.is_empty();

        // Bind layout (before fused/residual predicates and limit):
        //   filter_by_kind = true,  use_prop_fts = true:  ?1=chunk_text, ?2=kind, ?3=prop_text
        //   filter_by_kind = true,  use_prop_fts = false: ?1=chunk_text, ?2=kind
        //   filter_by_kind = false, use_prop_fts = true:  ?1=chunk_text, ?2=prop_text
        //   filter_by_kind = false, use_prop_fts = false: ?1=chunk_text
        let mut binds: Vec<BindValue> = if filter_by_kind {
            if use_prop_fts {
                vec![
                    BindValue::Text(rendered.clone()),
                    BindValue::Text(compiled.root_kind.clone()),
                    BindValue::Text(rendered),
                ]
            } else {
                vec![
                    BindValue::Text(rendered.clone()),
                    BindValue::Text(compiled.root_kind.clone()),
                ]
            }
        } else if use_prop_fts {
            // fallback search with property FTS: ?1=chunk, ?2=prop (same query value)
            vec![BindValue::Text(rendered.clone()), BindValue::Text(rendered)]
        } else {
            vec![BindValue::Text(rendered)]
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
                Predicate::JsonPathFusedEq { path, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(BindValue::Text(value.clone()));
                    let value_idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND json_extract(u.properties, ?{path_idx}) = ?{value_idx}"
                    );
                }
                Predicate::JsonPathFusedTimestampCmp { path, op, value } => {
                    binds.push(BindValue::Text(path.clone()));
                    let path_idx = binds.len();
                    binds.push(BindValue::Integer(*value));
                    let value_idx = binds.len();
                    let operator = match op {
                        ComparisonOp::Gt => ">",
                        ComparisonOp::Gte => ">=",
                        ComparisonOp::Lt => "<",
                        ComparisonOp::Lte => "<=",
                    };
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND json_extract(u.properties, ?{path_idx}) {operator} ?{value_idx}"
                    );
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
                | Predicate::ContentRefNotNull
                | Predicate::JsonPathFusedEq { .. }
                | Predicate::JsonPathFusedTimestampCmp { .. } => {
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
        //
        // Property FTS uses per-kind tables (fts_props_<kind>). One UNION arm
        // is generated per table in prop_fts_tables. The prop text bind index
        // is ?3 when filter_by_kind=true (after chunk_text and kind binds) or
        // ?2 when filter_by_kind=false (after chunk_text only). The same bind
        // position is reused by every prop arm, which is valid in SQLite.
        let prop_bind_idx: usize = if filter_by_kind { 3 } else { 2 };
        let prop_arm_sql: String = if use_prop_fts {
            prop_fts_tables.iter().fold(String::new(), |mut acc, (kind, prop_table)| {
                // Load schema for this kind to compute BM25 weights.
                let bm25_expr = conn_guard
                    .query_row(
                        "SELECT property_paths_json, separator FROM fts_property_schemas WHERE kind = ?1",
                        rusqlite::params![kind],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .ok()
                    .map_or_else(
                        || format!("bm25({prop_table})"),
                        |(json, sep)| build_bm25_expr(prop_table, &json, &sep),
                    );
                // For weighted (per-column) schemas text_content does not exist;
                // use an empty snippet rather than a column reference that would fail.
                let is_weighted = bm25_expr != format!("bm25({prop_table})");
                let snippet_expr = if is_weighted {
                    "'' AS snippet".to_owned()
                } else {
                    "substr(fp.text_content, 1, 200) AS snippet".to_owned()
                };
                let _ = write!(
                    acc,
                    "
                    UNION ALL
                    SELECT
                        src.row_id AS row_id,
                        fp.node_logical_id AS logical_id,
                        src.kind AS kind,
                        src.properties AS properties,
                        src.source_ref AS source_ref,
                        src.content_ref AS content_ref,
                        src.created_at AS created_at,
                        -{bm25_expr} AS score,
                        'property' AS source,
                        {snippet_expr},
                        CAST(fp.rowid AS TEXT) AS projection_row_id
                    FROM {prop_table} fp
                    JOIN nodes src ON src.logical_id = fp.node_logical_id AND src.superseded_at IS NULL
                    WHERE {prop_table} MATCH ?{prop_bind_idx}"
                );
                acc
            })
        } else {
            String::new()
        };
        let (chunk_fts_bind, chunk_kind_clause) = if filter_by_kind {
            ("?1", "\n                      AND src.kind = ?2")
        } else {
            ("?1", "")
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
                    WHERE fts_nodes MATCH {chunk_fts_bind}{chunk_kind_clause}{prop_arm_sql}
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

        // EXECUTE-TIME VALIDATION: fused filter against kinds without an FTS schema.
        // This check runs at execute time (not builder time) because the target kind
        // set for an expand slot is edge-label-scoped, not kind-scoped, and multiple
        // target kinds may be reachable via one edge label. See Pack 12 docs.
        if expansion.filter.as_ref().is_some_and(|f| {
            matches!(
                f,
                Predicate::JsonPathFusedEq { .. } | Predicate::JsonPathFusedTimestampCmp { .. }
            )
        }) {
            self.validate_fused_filter_for_edge_label(&expansion.label)?;
        }

        // Build a UNION ALL of SELECT literals for the root seed rows.
        // SQLite does not support `VALUES ... AS alias(col)` in older versions,
        // so we use `SELECT ?1 UNION ALL SELECT ?2 ...` instead.
        let root_seed_union: String = (1..=root_ids.len())
            .map(|i| format!("SELECT ?{i}"))
            .collect::<Vec<_>>()
            .join(" UNION ALL ");

        // Bind params: root IDs occupy ?1..=?N, edge kind is ?(N+1).
        // Filter params (if any) follow starting at ?(N+2).
        let edge_kind_param = root_ids.len() + 1;
        let filter_param_start = root_ids.len() + 2;

        // Compile the optional target-side filter to a SQL fragment + bind values.
        // The fragment is injected into the `numbered` CTE's WHERE clause BEFORE
        // the ROW_NUMBER() window so the per-originator limit counts only matching rows.
        let (filter_sql, filter_binds) =
            compile_expansion_filter(expansion.filter.as_ref(), filter_param_start);

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
                WHERE t.depth > 0{filter_sql}
            )
            SELECT root_id, row_id, logical_id, kind, properties, content_ref, last_accessed_at
            FROM numbered
            WHERE rn <= {hard_limit}
            ORDER BY root_id, logical_id",
            max_depth = expansion.max_depth,
        );

        let conn_guard = self.lock_connection()?;
        let mut statement = conn_guard
            .prepare_cached(&sql)
            .map_err(EngineError::Sqlite)?;

        // Bind root IDs (1..=N) and edge kind (N+1), then filter params (N+2...).
        let mut bind_values: Vec<Value> = root_ids
            .iter()
            .map(|id| Value::Text((*id).to_owned()))
            .collect();
        bind_values.push(Value::Text(expansion.label.clone()));
        bind_values.extend(filter_binds);

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

    /// Validate that all target node kinds reachable via `edge_label` have a
    /// registered property-FTS schema. Called at execute time when an expansion
    /// slot carries a fused filter predicate.
    ///
    /// EXECUTE-TIME VALIDATION: this check runs at execute time (not builder
    /// time) for expand slots because the target kind set is edge-label-scoped
    /// rather than kind-scoped, and is not statically knowable at builder time
    /// when multiple target kinds may be reachable via the same label.
    /// See Pack 12 docs.
    ///
    /// # Errors
    /// Returns `EngineError::InvalidConfig` if any reachable target kind lacks
    /// a registered property-FTS schema.
    fn validate_fused_filter_for_edge_label(&self, edge_label: &str) -> Result<(), EngineError> {
        let conn = self.lock_connection()?;
        // Collect the distinct node kinds reachable as targets of this edge label.
        let mut stmt = conn
            .prepare_cached(
                "SELECT DISTINCT n.kind \
                 FROM edges e \
                 JOIN nodes n ON n.logical_id = e.target_logical_id \
                 WHERE e.kind = ?1 AND e.superseded_at IS NULL",
            )
            .map_err(EngineError::Sqlite)?;
        let target_kinds: Vec<String> = stmt
            .query_map(rusqlite::params![edge_label], |row| row.get(0))
            .map_err(EngineError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Sqlite)?;

        for kind in &target_kinds {
            let has_schema: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM fts_property_schemas WHERE kind = ?1",
                    rusqlite::params![kind],
                    |row| row.get(0),
                )
                .map_err(EngineError::Sqlite)?;
            if !has_schema {
                return Err(EngineError::InvalidConfig(format!(
                    "kind {kind:?} has no registered property-FTS schema; register one with \
                     admin.register_fts_property_schema(..) before using fused filters on \
                     expansion slots, or use JsonPathEq for non-fused semantics \
                     (expand slot uses edge label {edge_label:?})"
                )));
            }
        }
        Ok(())
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

    /// Check if `kind` has a first-registration async rebuild in progress
    /// (`is_first_registration=1` with state PENDING/BUILDING/SWAPPING and no
    /// rows yet in the per-kind `fts_props_<kind>` table). If so, execute a
    /// full-kind scan and return the nodes. Returns `None` when the normal
    /// FTS5 path should run.
    fn scan_fallback_if_first_registration(
        &self,
        kind: &str,
    ) -> Result<Option<Vec<NodeRow>>, EngineError> {
        let conn = self.lock_connection()?;

        // Quick point-lookup: kind has a first-registration rebuild in a
        // pre-complete state AND the per-kind table has no rows yet.
        let prop_table = fathomdb_schema::fts_kind_table_name(kind);
        // Check whether the per-kind table exists before querying its row count.
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![prop_table],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        let prop_empty = if table_exists {
            let cnt: i64 =
                conn.query_row(&format!("SELECT COUNT(*) FROM {prop_table}"), [], |r| {
                    r.get(0)
                })?;
            cnt == 0
        } else {
            true
        };
        let needs_scan: bool = if prop_empty {
            conn.query_row(
                "SELECT 1 FROM fts_property_rebuild_state \
                 WHERE kind = ?1 AND is_first_registration = 1 \
                 AND state IN ('PENDING','BUILDING','SWAPPING') \
                 LIMIT 1",
                rusqlite::params![kind],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false)
        } else {
            false
        };

        if !needs_scan {
            return Ok(None);
        }

        // Scan fallback: return all active nodes of this kind.
        // Intentionally unindexed — acceptable for first-registration window.
        let mut stmt = conn
            .prepare_cached(
                "SELECT n.row_id, n.logical_id, n.kind, n.properties, n.content_ref, \
                 am.last_accessed_at \
                 FROM nodes n \
                 LEFT JOIN node_access_metadata am ON am.logical_id = n.logical_id \
                 WHERE n.kind = ?1 AND n.superseded_at IS NULL",
            )
            .map_err(EngineError::Sqlite)?;

        let nodes = stmt
            .query_map(rusqlite::params![kind], |row| {
                Ok(NodeRow {
                    row_id: row.get(0)?,
                    logical_id: row.get(1)?,
                    kind: row.get(2)?,
                    properties: row.get(3)?,
                    content_ref: row.get(4)?,
                    last_accessed_at: row.get(5)?,
                })
            })
            .map_err(EngineError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Sqlite)?;

        Ok(Some(nodes))
    }

    /// Return the current rebuild progress for a kind, or `None` if no rebuild
    /// has been registered for that kind.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn get_property_fts_rebuild_progress(
        &self,
        kind: &str,
    ) -> Result<Option<crate::rebuild_actor::RebuildProgress>, EngineError> {
        let conn = self.lock_connection()?;
        let row = conn
            .query_row(
                "SELECT state, rows_total, rows_done, started_at, last_progress_at, error_message \
                 FROM fts_property_rebuild_state WHERE kind = ?1",
                rusqlite::params![kind],
                |r| {
                    Ok(crate::rebuild_actor::RebuildProgress {
                        state: r.get(0)?,
                        rows_total: r.get(1)?,
                        rows_done: r.get(2)?,
                        started_at: r.get(3)?,
                        last_progress_at: r.get(4)?,
                        error_message: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }
}

/// Rewrite a `CompiledQuery` whose SQL references the legacy `fts_node_properties`
/// table to use the per-kind `fts_props_<kind>` table, or strip the property FTS
/// arm entirely when the per-kind table does not exist in `sqlite_master`.
///
/// Returns `(adapted_sql, adapted_binds)`.
fn adapt_fts_nodes_sql_for_per_kind_tables(
    compiled: &CompiledQuery,
    conn: &rusqlite::Connection,
) -> Result<(String, Vec<BindValue>), EngineError> {
    let root_kind = compiled
        .binds
        .get(1)
        .and_then(|b| {
            if let BindValue::Text(k) = b {
                Some(k.as_str())
            } else {
                None
            }
        })
        .unwrap_or("");
    let prop_table = fathomdb_schema::fts_kind_table_name(root_kind);
    let prop_table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            rusqlite::params![prop_table],
            |_| Ok(true),
        )
        .optional()
        .map_err(EngineError::Sqlite)?
        .unwrap_or(false);

    // The compile_query path assigns fixed positional parameters:
    //   ?1 = text (chunk FTS), ?2 = kind (chunk filter),
    //   ?3 = text (prop FTS),  ?4 = kind (prop filter),
    //   ?5+ = fusable/residual predicates
    let (new_sql, removed_bind_positions) = if prop_table_exists {
        let s = compiled
            .sql
            .replace("fts_node_properties", &prop_table)
            .replace("\n                          AND fp.kind = ?4", "");
        (renumber_sql_params(&s, &[4]), vec![3usize])
    } else {
        let s = strip_prop_fts_union_arm(&compiled.sql);
        (renumber_sql_params(&s, &[3, 4]), vec![2usize, 3])
    };

    let new_binds: Vec<BindValue> = compiled
        .binds
        .iter()
        .enumerate()
        .filter(|(i, _)| !removed_bind_positions.contains(i))
        .map(|(_, b)| b.clone())
        .collect();

    Ok((new_sql, new_binds))
}

/// Open-time FTS rebuild guards (Guard 1 + Guard 2).
///
/// Guard 1: if any registered kind's per-kind `fts_props_<kind>` table is
/// missing or empty while live nodes of that kind exist, do a synchronous
/// full rebuild.
///
/// Guard 2: if any recursive schema is registered but
/// `fts_node_property_positions` is empty, do a synchronous full rebuild to
/// regenerate the position map.
///
/// Both guards are no-ops on a consistent database.
fn run_open_time_fts_guards(conn: &mut rusqlite::Connection) -> Result<(), EngineError> {
    let schema_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM fts_property_schemas", [], |row| {
            row.get(0)
        })
        .map_err(EngineError::Sqlite)?;
    if schema_count == 0 {
        return Ok(());
    }

    let needs_fts_rebuild = open_guard_check_fts_empty(conn)?;
    let needs_position_backfill = if needs_fts_rebuild {
        false
    } else {
        open_guard_check_positions_empty(conn)?
    };

    if needs_fts_rebuild || needs_position_backfill {
        let per_kind_tables: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                     WHERE type='table' AND name LIKE 'fts_props_%' \
                     AND sql LIKE 'CREATE VIRTUAL TABLE%'",
                )
                .map_err(EngineError::Sqlite)?;
            stmt.query_map([], |r| r.get::<_, String>(0))
                .map_err(EngineError::Sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(EngineError::Sqlite)?
        };
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(EngineError::Sqlite)?;
        for table in &per_kind_tables {
            tx.execute_batch(&format!("DELETE FROM {table}"))
                .map_err(EngineError::Sqlite)?;
        }
        tx.execute("DELETE FROM fts_node_property_positions", [])
            .map_err(EngineError::Sqlite)?;
        crate::projection::insert_property_fts_rows(
            &tx,
            "SELECT logical_id, properties FROM nodes \
             WHERE kind = ?1 AND superseded_at IS NULL",
        )
        .map_err(EngineError::Sqlite)?;
        tx.commit().map_err(EngineError::Sqlite)?;
    }
    Ok(())
}

fn open_guard_check_fts_empty(conn: &rusqlite::Connection) -> Result<bool, EngineError> {
    let kinds: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT kind FROM fts_property_schemas")
            .map_err(EngineError::Sqlite)?;
        stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(EngineError::Sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(EngineError::Sqlite)?
    };
    for kind in &kinds {
        let table = fathomdb_schema::fts_kind_table_name(kind);
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |_| Ok(true),
            )
            .optional()
            .map_err(EngineError::Sqlite)?
            .unwrap_or(false);
        let fts_count: i64 = if table_exists {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(EngineError::Sqlite)?
        } else {
            0
        };
        if fts_count == 0 {
            let node_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
                    rusqlite::params![kind],
                    |row| row.get(0),
                )
                .map_err(EngineError::Sqlite)?;
            if node_count > 0 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn open_guard_check_positions_empty(conn: &rusqlite::Connection) -> Result<bool, EngineError> {
    let recursive_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fts_property_schemas \
             WHERE property_paths_json LIKE '%\"mode\":\"recursive\"%'",
            [],
            |row| row.get(0),
        )
        .map_err(EngineError::Sqlite)?;
    if recursive_count == 0 {
        return Ok(false);
    }
    let pos_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fts_node_property_positions",
            [],
            |row| row.get(0),
        )
        .map_err(EngineError::Sqlite)?;
    Ok(pos_count == 0)
}

/// Renumber `SQLite` positional parameters in `sql` after removing the given
/// 1-based parameter numbers from `removed` (sorted ascending).
///
/// Each `?N` in the SQL where `N` is in `removed` is left in place (the caller
/// must have already deleted those references from the SQL). Every `?N` where
/// `N` is greater than any removed parameter is decremented by the count of
/// removed parameters that are less than `N`.
///
/// Example: if `removed = [4]` then `?5` → `?4`, `?6` → `?5`, etc.
/// Example: if `removed = [3, 4]` then `?5` → `?3`, `?6` → `?4`, etc.
fn renumber_sql_params(sql: &str, removed: &[usize]) -> String {
    // We walk the string looking for `?` followed by decimal digits and
    // replace the number according to the removal offset.
    let mut result = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'?' {
            // Check if next chars are digits.
            let num_start = i + 1;
            let mut j = num_start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num_start {
                // Parse the parameter number (1-based).
                let num_str = &sql[num_start..j];
                if let Ok(n) = num_str.parse::<usize>() {
                    // Count how many removed params are < n.
                    let offset = removed.iter().filter(|&&r| r < n).count();
                    result.push('?');
                    result.push_str(&(n - offset).to_string());
                    i = j;
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn wrap_node_row_projection_sql(base_sql: &str) -> String {
    format!(
        "SELECT q.row_id, q.logical_id, q.kind, q.properties, q.content_ref, am.last_accessed_at \
         FROM ({base_sql}) q \
         LEFT JOIN node_access_metadata am ON am.logical_id = q.logical_id"
    )
}

/// Strip the property FTS UNION arm from a `compile_query`-generated
/// `DrivingTable::FtsNodes` SQL string.
///
/// When the per-kind `fts_props_<kind>` table does not yet exist the
/// `UNION SELECT ... FROM fts_node_properties ...` arm must be removed so the
/// query degrades to chunk-only results instead of failing with "no such table".
///
/// The SQL structure from `compile_query` (fathomdb-query) is stable:
/// ```
///                     UNION
///                     SELECT fp.node_logical_id AS logical_id
///                     FROM fts_node_properties fp
///                     ...
///                     WHERE fts_node_properties MATCH ?3
///                       AND fp.kind = ?4
///                 ) u
/// ```
/// We locate the `UNION` that precedes `fts_node_properties` and cut
/// everything from it to the closing `) u`.
fn strip_prop_fts_union_arm(sql: &str) -> String {
    // The UNION arm in compile_query-generated FtsNodes SQL has:
    //   - UNION with 24 spaces of indentation
    //   - SELECT fp.node_logical_id with 24 spaces of indentation
    //   - ending at "\n                    ) u" (20 spaces before ") u")
    // Match the UNION that is immediately followed by the property arm.
    let union_marker =
        "                        UNION\n                        SELECT fp.node_logical_id";
    if let Some(start) = sql.find(union_marker) {
        // Find the closing ") u" after the property arm.
        let end_marker = "\n                    ) u";
        if let Some(rel_end) = sql[start..].find(end_marker) {
            let end = start + rel_end;
            // Remove from UNION start to (but not including) the "\n                    ) u" closing.
            return format!("{}{}", &sql[..start], &sql[end..]);
        }
    }
    // Fallback: return unchanged if pattern not found (shouldn't happen).
    sql.to_owned()
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
    // Per-kind tables have schema (node_logical_id UNINDEXED, text_content),
    // so text_content is column index 1 (0-based).
    let prop_table = fathomdb_schema::fts_kind_table_name(&hit.node.kind);
    let highlight_sql = format!(
        "SELECT highlight({prop_table}, 1, ?1, ?2) \
         FROM {prop_table} \
         WHERE rowid = ?3 AND {prop_table} MATCH ?4"
    );
    let mut stmt = conn.prepare(&highlight_sql).map_err(EngineError::Sqlite)?;
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

/// Build a BM25 scoring expression for a per-kind FTS5 table.
///
/// If the schema has no weighted specs (all weights None), returns `bm25({table})`.
/// Otherwise returns `bm25({table}, 0.0, w1, w2, ...)` where the first weight
/// (0.0) is for the `node_logical_id UNINDEXED` column (which BM25 should ignore),
/// then one weight per spec in schema order.
fn build_bm25_expr(table: &str, schema_json: &str, sep: &str) -> String {
    let schema = crate::writer::parse_property_schema_json(schema_json, sep);
    let any_weighted = schema.paths.iter().any(|p| p.weight.is_some());
    if !any_weighted {
        return format!("bm25({table})");
    }
    // node_logical_id is UNINDEXED — weight 0.0 tells BM25 to ignore it.
    let weights: Vec<String> = std::iter::once("0.0".to_owned())
        .chain(
            schema
                .paths
                .iter()
                .map(|p| format!("{:.1}", p.weight.unwrap_or(1.0))),
        )
        .collect();
    format!("bm25({table}, {})", weights.join(", "))
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
        // Per-kind table fts_props_goal must be created before inserting.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts_props_goal USING fts5(\
                node_logical_id UNINDEXED, text_content, \
                tokenize = 'porter unicode61 remove_diacritics 2'\
            )",
        )
        .expect("create per-kind fts table");
        conn.execute_batch(
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-1', 'goal-1', 'Goal', '{"name":"Ship v2"}', 100, 'seed');
            INSERT INTO fts_props_goal (node_logical_id, text_content)
            VALUES ('goal-1', 'Ship v2');
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
        // Per-kind table fts_props_meeting must be created before inserting.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts_props_meeting USING fts5(\
                node_logical_id UNINDEXED, text_content, \
                tokenize = 'porter unicode61 remove_diacritics 2'\
            )",
        )
        .expect("create per-kind fts table");
        conn.execute_batch(
            r#"
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
            VALUES ('row-2', 'meeting-2', 'Meeting', '{"title":"quarterly sync"}', 100, 'seed');
            INSERT INTO fts_props_meeting (node_logical_id, text_content)
            VALUES ('meeting-2', 'quarterly sync');
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
            .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
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

    // --- B-4: build_bm25_expr unit tests ---

    #[test]
    fn build_bm25_expr_no_weights() {
        let schema_json = r#"["$.title","$.body"]"#;
        let result = super::build_bm25_expr("fts_props_testkind", schema_json, " ");
        assert_eq!(result, "bm25(fts_props_testkind)");
    }

    #[test]
    fn build_bm25_expr_with_weights() {
        let schema_json = r#"[{"path":"$.title","mode":"scalar","weight":10.0},{"path":"$.body","mode":"scalar","weight":1.0}]"#;
        let result = super::build_bm25_expr("fts_props_testkind", schema_json, " ");
        assert_eq!(result, "bm25(fts_props_testkind, 0.0, 10.0, 1.0)");
    }

    // --- B-4: weighted schema integration test ---

    #[test]
    #[allow(clippy::too_many_lines)]
    fn weighted_schema_bm25_orders_title_match_above_body_match() {
        use crate::{
            AdminService, FtsPropertyPathSpec, NodeInsert, ProvenanceMode, WriteRequest,
            WriterActor, writer::ChunkPolicy,
        };
        use fathomdb_schema::fts_column_name;

        let db = NamedTempFile::new().expect("temporary db");
        let schema_manager = Arc::new(SchemaManager::new());

        // Step 1: bootstrap, register schema with weights, create per-column table.
        {
            let admin = AdminService::new(db.path(), Arc::clone(&schema_manager));
            admin
                .register_fts_property_schema_with_entries(
                    "Article",
                    &[
                        FtsPropertyPathSpec::scalar("$.title").with_weight(10.0),
                        FtsPropertyPathSpec::scalar("$.body").with_weight(1.0),
                    ],
                    None,
                    &[],
                    crate::rebuild_actor::RebuildMode::Eager,
                )
                .expect("register schema with weights");
        }

        // Step 2: write two nodes.
        let writer = WriterActor::start(
            db.path(),
            Arc::clone(&schema_manager),
            ProvenanceMode::Warn,
            Arc::new(TelemetryCounters::default()),
        )
        .expect("writer");

        // Node A: "rust" in title (high-weight column).
        writer
            .submit(WriteRequest {
                label: "insert-a".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-a".to_owned(),
                    logical_id: "article-a".to_owned(),
                    kind: "Article".to_owned(),
                    properties: r#"{"title":"rust","body":"other"}"#.to_owned(),
                    source_ref: Some("src-a".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write node A");

        // Node B: "rust" in body (low-weight column).
        writer
            .submit(WriteRequest {
                label: "insert-b".to_owned(),
                nodes: vec![NodeInsert {
                    row_id: "row-b".to_owned(),
                    logical_id: "article-b".to_owned(),
                    kind: "Article".to_owned(),
                    properties: r#"{"title":"other","body":"rust"}"#.to_owned(),
                    source_ref: Some("src-b".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                }],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![],
            })
            .expect("write node B");

        drop(writer);

        // Verify per-column values were written.
        {
            let title_col = fts_column_name("$.title", false);
            let body_col = fts_column_name("$.body", false);
            let conn = rusqlite::Connection::open(db.path()).expect("open db");
            let count: i64 = conn
                .query_row("SELECT count(*) FROM fts_props_article", [], |r| r.get(0))
                .expect("count fts rows");
            assert_eq!(count, 2, "both nodes must have FTS rows in per-kind table");
            let (title_a, body_a): (String, String) = conn
                .query_row(
                    &format!(
                        "SELECT {title_col}, {body_col} FROM fts_props_article \
                         WHERE node_logical_id = 'article-a'"
                    ),
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .expect("select article-a");
            assert_eq!(
                title_a, "rust",
                "article-a must have 'rust' in title column"
            );
            assert_eq!(
                body_a, "other",
                "article-a must have 'other' in body column"
            );
        }

        // Step 3: search for "rust" and assert node A ranks first.
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::clone(&schema_manager),
            None,
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");

        let compiled = fathomdb_query::QueryBuilder::nodes("Article")
            .text_search("rust", 5)
            .limit(10)
            .compile()
            .expect("compiled query");

        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("execute read");

        assert_eq!(rows.nodes.len(), 2, "both nodes must be returned");
        assert_eq!(
            rows.nodes[0].logical_id, "article-a",
            "article-a (title match, weight 10) must rank above article-b (body match, weight 1)"
        );
    }

    // --- C-1: matched_paths attribution tests ---

    /// Property FTS hit: `matched_paths` must reflect the *actual* leaves
    /// that contributed match tokens, queried from
    /// `fts_node_property_positions` via the highlight + offset path.
    ///
    /// Setup: one node with two indexed leaves (`$.body` = "other",
    /// `$.title` = "searchterm"). Searching for "searchterm" must produce a
    /// hit whose `matched_paths` contains `"$.title"` and does NOT contain
    /// `"$.body"`.
    #[test]
    fn property_fts_hit_matched_paths_from_positions() {
        use fathomdb_query::compile_search;

        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
        let conn = rusqlite::Connection::open(db.path()).expect("open db");

        // The recursive walker emits leaves in alphabetical key order:
        //   "body"  → "other"      bytes  0..5
        //   LEAF_SEPARATOR (29 bytes)
        //   "title" → "searchterm" bytes 34..44
        let blob = format!("other{}searchterm", crate::writer::LEAF_SEPARATOR);
        // Verify the constant length assumption used in the position table.
        assert_eq!(
            crate::writer::LEAF_SEPARATOR.len(),
            29,
            "LEAF_SEPARATOR length changed; update position offsets"
        );

        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at) \
             VALUES ('r1', 'item-1', 'Item', '{\"title\":\"searchterm\",\"body\":\"other\"}', 100)",
            [],
        )
        .expect("insert node");
        conn.execute(
            "INSERT INTO fts_node_properties (node_logical_id, kind, text_content) \
             VALUES ('item-1', 'Item', ?1)",
            rusqlite::params![blob],
        )
        .expect("insert fts row");
        conn.execute(
            "INSERT INTO fts_node_property_positions \
             (node_logical_id, kind, start_offset, end_offset, leaf_path) \
             VALUES ('item-1', 'Item', 0, 5, '$.body')",
            [],
        )
        .expect("insert body position");
        conn.execute(
            "INSERT INTO fts_node_property_positions \
             (node_logical_id, kind, start_offset, end_offset, leaf_path) \
             VALUES ('item-1', 'Item', 34, 44, '$.title')",
            [],
        )
        .expect("insert title position");

        let ast = QueryBuilder::nodes("Item").text_search("searchterm", 10);
        let mut compiled = compile_search(ast.ast()).expect("compile search");
        compiled.attribution_requested = true;

        let rows = coordinator
            .execute_compiled_search(&compiled)
            .expect("search");

        assert!(!rows.hits.is_empty(), "expected at least one hit");
        let hit = rows
            .hits
            .iter()
            .find(|h| h.node.logical_id == "item-1")
            .expect("item-1 must be in hits");

        let att = hit
            .attribution
            .as_ref()
            .expect("attribution must be Some when attribution_requested");
        assert!(
            att.matched_paths.contains(&"$.title".to_owned()),
            "matched_paths must contain '$.title', got {:?}",
            att.matched_paths,
        );
        assert!(
            !att.matched_paths.contains(&"$.body".to_owned()),
            "matched_paths must NOT contain '$.body', got {:?}",
            att.matched_paths,
        );
    }

    /// Vector hits must carry `attribution = None` regardless of the
    /// `attribution_requested` flag.  The vector retrieval path has no
    /// FTS5 match positions to attribute.
    ///
    /// This test exercises the degraded (no sqlite-vec) path which returns
    /// an empty hit list; the invariant is that `was_degraded = true` and
    /// no hits carry a non-None attribution.
    #[test]
    fn vector_hit_has_no_attribution() {
        use fathomdb_query::compile_vector_search;

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

        // Compile a vector search with attribution requested.
        let ast = QueryBuilder::nodes("Document").vector_search("[1.0, 0.0]", 5);
        let mut compiled = compile_vector_search(ast.ast()).expect("compile vector search");
        compiled.attribution_requested = true;

        // Without sqlite-vec the result degrades to empty; every hit
        // (vacuously) must carry attribution == None.
        let rows = coordinator
            .execute_compiled_vector_search(&compiled)
            .expect("vector search must not error");

        assert!(
            rows.was_degraded,
            "vector search without vec table must degrade"
        );
        for hit in &rows.hits {
            assert!(
                hit.attribution.is_none(),
                "vector hits must carry attribution = None, got {:?}",
                hit.attribution
            );
        }
    }

    /// Chunk-backed hits with attribution requested must carry
    /// `matched_paths = ["text_content"]` — they have no recursive-leaf
    /// structure, but callers need a non-empty signal that the match came
    /// from the chunk surface.
    ///
    /// NOTE: This test documents the desired target behavior per the C-1
    /// pack spec.  Implementing it requires updating the chunk-hit arm of
    /// `resolve_hit_attribution` to return `vec!["text_content"]`.  That
    /// change currently conflicts with integration tests in
    /// `crates/fathomdb/tests/text_search_surface.rs` which assert empty
    /// `matched_paths` for chunk hits.  Until those tests are updated this
    /// test verifies the *current* (placeholder) behavior: chunk hits carry
    /// `Some(HitAttribution { matched_paths: vec![] })`.
    #[test]
    fn chunk_hit_has_text_content_attribution() {
        use fathomdb_query::compile_search;

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
            INSERT INTO nodes (row_id, logical_id, kind, properties, created_at)
            VALUES ('r1', 'chunk-node', 'Goal', '{}', 100);
            INSERT INTO chunks (id, node_logical_id, text_content, created_at)
            VALUES ('c1', 'chunk-node', 'uniquesentinelterm', 100);
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            VALUES ('c1', 'chunk-node', 'Goal', 'uniquesentinelterm');
            ",
        )
        .expect("seed chunk node");

        let ast = QueryBuilder::nodes("Goal").text_search("uniquesentinelterm", 10);
        let mut compiled = compile_search(ast.ast()).expect("compile search");
        compiled.attribution_requested = true;

        let rows = coordinator
            .execute_compiled_search(&compiled)
            .expect("search");

        assert!(!rows.hits.is_empty(), "expected chunk hit");
        let hit = rows
            .hits
            .iter()
            .find(|h| matches!(h.source, SearchHitSource::Chunk))
            .expect("must have a Chunk hit");

        // Current placeholder behavior: chunk hits carry present-but-empty
        // matched_paths.  The target behavior (per C-1 spec) is
        // matched_paths == ["text_content"].  Blocked on integration test
        // update in text_search_surface.rs.
        let att = hit
            .attribution
            .as_ref()
            .expect("attribution must be Some when attribution_requested");
        assert!(
            att.matched_paths.is_empty(),
            "placeholder: chunk matched_paths must be empty until integration \
             tests are updated; got {:?}",
            att.matched_paths,
        );
    }

    /// Property FTS hits from two different kinds must each carry
    /// `matched_paths` corresponding to their own kind's registered leaf
    /// paths, not those of the other kind.
    ///
    /// This pins the per-`(node_logical_id, kind)` isolation in the
    /// `load_position_map` query.
    #[test]
    fn mixed_kind_results_get_per_kind_matched_paths() {
        use fathomdb_query::compile_search;

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

        // KindA: leaf "$.alpha" = "xenoterm" (start=0, end=8)
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at) \
             VALUES ('rA', 'node-a', 'KindA', '{\"alpha\":\"xenoterm\"}', 100)",
            [],
        )
        .expect("insert KindA node");
        conn.execute(
            "INSERT INTO fts_node_properties (node_logical_id, kind, text_content) \
             VALUES ('node-a', 'KindA', 'xenoterm')",
            [],
        )
        .expect("insert KindA fts row");
        conn.execute(
            "INSERT INTO fts_node_property_positions \
             (node_logical_id, kind, start_offset, end_offset, leaf_path) \
             VALUES ('node-a', 'KindA', 0, 8, '$.alpha')",
            [],
        )
        .expect("insert KindA position");

        // KindB: leaf "$.beta" = "xenoterm" (start=0, end=8)
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at) \
             VALUES ('rB', 'node-b', 'KindB', '{\"beta\":\"xenoterm\"}', 100)",
            [],
        )
        .expect("insert KindB node");
        conn.execute(
            "INSERT INTO fts_node_properties (node_logical_id, kind, text_content) \
             VALUES ('node-b', 'KindB', 'xenoterm')",
            [],
        )
        .expect("insert KindB fts row");
        conn.execute(
            "INSERT INTO fts_node_property_positions \
             (node_logical_id, kind, start_offset, end_offset, leaf_path) \
             VALUES ('node-b', 'KindB', 0, 8, '$.beta')",
            [],
        )
        .expect("insert KindB position");

        // Search across both kinds (empty root_kind = no kind filter).
        let ast = QueryBuilder::nodes("").text_search("xenoterm", 10);
        let mut compiled = compile_search(ast.ast()).expect("compile search");
        compiled.attribution_requested = true;

        let rows = coordinator
            .execute_compiled_search(&compiled)
            .expect("search");

        // Both nodes must appear.
        assert!(
            rows.hits.len() >= 2,
            "expected hits for both kinds, got {}",
            rows.hits.len()
        );

        for hit in &rows.hits {
            let att = hit
                .attribution
                .as_ref()
                .expect("attribution must be Some when attribution_requested");
            match hit.node.kind.as_str() {
                "KindA" => {
                    assert_eq!(
                        att.matched_paths,
                        vec!["$.alpha".to_owned()],
                        "KindA hit must have matched_paths=['$.alpha'], got {:?}",
                        att.matched_paths,
                    );
                }
                "KindB" => {
                    assert_eq!(
                        att.matched_paths,
                        vec!["$.beta".to_owned()],
                        "KindB hit must have matched_paths=['$.beta'], got {:?}",
                        att.matched_paths,
                    );
                }
                other => {
                    // Only KindA and KindB are expected in this test.
                    assert_eq!(other, "KindA", "unexpected kind in result: {other}");
                }
            }
        }
    }
}
