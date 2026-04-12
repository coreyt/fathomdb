use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use fathomdb_query::{
    BindValue, ComparisonOp, CompiledGroupedQuery, CompiledQuery, CompiledSearch, DrivingTable,
    ExpansionSlot, Predicate, ScalarValue, SearchHit, SearchHitSource, SearchMatchMode, SearchRows,
    ShapeHash, render_text_query_fts5,
};
use fathomdb_schema::SchemaManager;
use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Value};

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

        // Phase 2: when migration 16 is freshly applied, the FTS virtual
        // tables were dropped and recreated with the unicode61+porter
        // tokenizer. Chunk FTS is rebuilt inline by the migration; property
        // FTS requires per-kind projection logic that lives in this crate, so
        // rebuild it here before the pool/readers observe an empty index.
        if report.applied_versions.iter().any(|v| v.0 == 16) {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM fts_node_properties", [])?;
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
    #[allow(clippy::too_many_lines)]
    pub fn execute_compiled_search(
        &self,
        compiled: &CompiledSearch,
    ) -> Result<SearchRows, EngineError> {
        use std::fmt::Write as _;
        let rendered = render_text_query_fts5(&compiled.text_query);
        let mut binds: Vec<BindValue> = vec![
            BindValue::Text(rendered.clone()),
            BindValue::Text(compiled.root_kind.clone()),
            BindValue::Text(rendered),
            BindValue::Text(compiled.root_kind.clone()),
        ];

        // Fusable predicates are injected into the CTE's outer WHERE against
        // the `hn` alias (the nodes table joined inside the CTE). Residual
        // predicates remain in the outer WHERE against `n`.
        let mut fused_clauses = String::new();
        for predicate in &compiled.fusable_filters {
            match predicate {
                Predicate::KindEq(kind) => {
                    binds.push(BindValue::Text(kind.clone()));
                    let idx = binds.len();
                    let _ = write!(fused_clauses, "\n                  AND hn.kind = ?{idx}");
                }
                Predicate::LogicalIdEq(logical_id) => {
                    binds.push(BindValue::Text(logical_id.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND hn.logical_id = ?{idx}"
                    );
                }
                Predicate::SourceRefEq(source_ref) => {
                    binds.push(BindValue::Text(source_ref.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND hn.source_ref = ?{idx}"
                    );
                }
                Predicate::ContentRefEq(uri) => {
                    binds.push(BindValue::Text(uri.clone()));
                    let idx = binds.len();
                    let _ = write!(
                        fused_clauses,
                        "\n                  AND hn.content_ref = ?{idx}"
                    );
                }
                Predicate::ContentRefNotNull => {
                    fused_clauses.push_str("\n                  AND hn.content_ref IS NOT NULL");
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
                        "\n  AND json_extract(n.properties, ?{path_idx}) = ?{value_idx}"
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
                        "\n  AND json_extract(n.properties, ?{path_idx}) {operator} ?{value_idx}"
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

        let limit = compiled.limit;
        let sql = format!(
            "WITH search_hits AS (
                SELECT
                    u.logical_id AS logical_id,
                    u.score AS score,
                    u.source AS source,
                    u.snippet AS snippet,
                    u.projection_row_id AS projection_row_id
                FROM (
                    SELECT
                        c.node_logical_id AS logical_id,
                        -bm25(fts_nodes) AS score,
                        'chunk' AS source,
                        snippet(fts_nodes, 3, '[', ']', '…', 32) AS snippet,
                        f.chunk_id AS projection_row_id
                    FROM fts_nodes f
                    JOIN chunks c ON c.id = f.chunk_id
                    JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                    WHERE fts_nodes MATCH ?1
                      AND src.kind = ?2
                    UNION ALL
                    SELECT
                        fp.node_logical_id AS logical_id,
                        -bm25(fts_node_properties) AS score,
                        'property' AS source,
                        substr(fp.text_content, 1, 200) AS snippet,
                        CAST(fp.rowid AS TEXT) AS projection_row_id
                    FROM fts_node_properties fp
                    JOIN nodes src ON src.logical_id = fp.node_logical_id AND src.superseded_at IS NULL
                    WHERE fts_node_properties MATCH ?3
                      AND fp.kind = ?4
                ) u
                JOIN nodes hn ON hn.logical_id = u.logical_id AND hn.superseded_at IS NULL
                WHERE 1 = 1{fused_clauses}
                ORDER BY u.score DESC
                LIMIT {limit}
            )
            SELECT
                n.row_id,
                n.logical_id,
                n.kind,
                n.properties,
                n.content_ref,
                am.last_accessed_at,
                n.created_at,
                h.score,
                h.source,
                h.snippet,
                h.projection_row_id
            FROM search_hits h
            JOIN nodes n ON n.logical_id = h.logical_id AND n.superseded_at IS NULL
            LEFT JOIN node_access_metadata am ON am.logical_id = n.logical_id
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
                    source,
                    match_mode: SearchMatchMode::Strict,
                    snippet: row.get(9)?,
                    projection_row_id: row.get(10)?,
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

        self.telemetry.increment_queries();
        let strict_hit_count = hits.len();
        Ok(SearchRows {
            hits,
            strict_hit_count,
            relaxed_hit_count: 0,
            fallback_used: false,
            was_degraded: false,
        })
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

    use super::{bind_value_to_sql, is_vec_table_absent, wrap_node_row_projection_sql};

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
