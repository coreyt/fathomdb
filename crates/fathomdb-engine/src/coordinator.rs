use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use fathomdb_query::{CompiledQuery, DrivingTable, ShapeHash};
use fathomdb_schema::SchemaManager;
use rusqlite::{Connection, OptionalExtension, params_from_iter, types::Value};

use crate::{EngineError, sqlite};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRow {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRow {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepRow {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRow {
    pub id: String,
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryRows {
    pub nodes: Vec<NodeRow>,
    pub runs: Vec<RunRow>,
    pub steps: Vec<StepRow>,
    pub actions: Vec<ActionRow>,
}

pub struct ExecutionCoordinator {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    conn: Mutex<Connection>,
    shape_sql_map: Mutex<HashMap<ShapeHash, String>>,
    vector_enabled: bool,
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

        #[cfg(feature = "sqlite-vec")]
        let vector_enabled = report.vector_profile_enabled;
        #[cfg(not(feature = "sqlite-vec"))]
        let vector_enabled = {
            let _ = &report;
            false
        };

        if let Some(dim) = vector_dimension {
            schema_manager
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", dim)
                .map_err(EngineError::Schema)?;
        }

        Ok(Self {
            database_path: path,
            schema_manager,
            conn: Mutex::new(conn),
            shape_sql_map: Mutex::new(HashMap::new()),
            vector_enabled,
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Returns `true` when sqlite-vec was loaded and a vector profile is active.
    #[must_use]
    pub fn vector_enabled(&self) -> bool {
        self.vector_enabled
    }

    /// # Errors
    /// Returns [`EngineError`] if the SQL statement cannot be prepared or executed.
    ///
    /// # Panics
    /// Panics if the internal connection or shape-SQL-map mutex is poisoned.
    #[allow(clippy::expect_used)]
    pub fn execute_compiled_read(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<QueryRows, EngineError> {
        // FIX(review): was .expect() — panics on mutex poisoning, cascading failure.
        // Options: (A) into_inner() for all, (B) EngineError for all, (C) mixed.
        // Chose (C): shape_sql_map is a pure cache — into_inner() is safe to recover.
        // conn wraps a SQLite connection whose state may be corrupt after a thread panic,
        // so we propagate EngineError there instead.
        self.shape_sql_map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(compiled.shape_hash, compiled.sql.clone());

        let bind_values = compiled
            .binds
            .iter()
            .map(bind_value_to_sql)
            .collect::<Vec<_>>();

        // FIX(review) + Security fix M-8: was .expect() — panics on mutex poisoning.
        // shape_sql_map uses into_inner() (pure cache, safe to recover).
        // conn uses map_err → EngineError (connection state may be corrupt after panic;
        // into_inner() would risk using a connection with partial transaction state).
        let conn_guard = self
            .conn
            .lock()
            .map_err(|_| EngineError::Bridge("connection mutex poisoned".to_owned()))?;
        let mut statement = conn_guard.prepare_cached(&compiled.sql).map_err(|e| {
            if is_capability_missing_error(&e) {
                EngineError::CapabilityMissing(e.to_string())
            } else {
                EngineError::Sqlite(e)
            }
        })?;
        let nodes = statement
            .query_map(params_from_iter(bind_values.iter()), |row| {
                Ok(NodeRow {
                    row_id: row.get(0)?,
                    logical_id: row.get(1)?,
                    kind: row.get(2)?,
                    properties: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(QueryRows {
            nodes,
            runs: Vec::new(),
            steps: Vec::new(),
            actions: Vec::new(),
        })
    }

    /// Read a single run by id.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the query fails.
    ///
    /// # Panics
    /// Panics if the internal connection mutex is poisoned.
    #[allow(clippy::expect_used)]
    pub fn read_run(&self, id: &str) -> Result<Option<RunRow>, EngineError> {
        let conn = self.conn.lock().expect("coordinator connection mutex");
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
    /// Returns [`EngineError`] if the query fails.
    ///
    /// # Panics
    /// Panics if the internal connection mutex is poisoned.
    #[allow(clippy::expect_used)]
    pub fn read_step(&self, id: &str) -> Result<Option<StepRow>, EngineError> {
        let conn = self.conn.lock().expect("coordinator connection mutex");
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
    /// Returns [`EngineError`] if the query fails.
    ///
    /// # Panics
    /// Panics if the internal connection mutex is poisoned.
    #[allow(clippy::expect_used)]
    pub fn read_action(&self, id: &str) -> Result<Option<ActionRow>, EngineError> {
        let conn = self.conn.lock().expect("coordinator connection mutex");
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
    /// Returns [`EngineError`] if the query fails.
    ///
    /// # Panics
    /// Panics if the internal connection mutex is poisoned.
    #[allow(clippy::expect_used)]
    pub fn read_active_runs(&self) -> Result<Vec<RunRow>, EngineError> {
        let conn = self.conn.lock().expect("coordinator connection mutex");
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
            sql: compiled.sql.clone(),
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
    /// Returns [`EngineError`] if the PRAGMA query fails.
    ///
    /// # Panics
    /// Panics if the internal connection mutex is poisoned.
    #[allow(clippy::expect_used)]
    pub fn raw_pragma(&self, name: &str) -> Result<String, EngineError> {
        let conn = self.conn.lock().expect("coordinator connection mutex");
        let result: String = conn
            .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
            .map_err(EngineError::Sqlite)?;
        Ok(result)
    }
}

fn is_capability_missing_error(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => {
            msg.contains("no such table: vec_nodes_active")
        }
        _ => false,
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
    use std::sync::Arc;

    use fathomdb_query::{BindValue, QueryBuilder};
    use fathomdb_schema::SchemaManager;
    use rusqlite::types::Value;
    use tempfile::NamedTempFile;

    use crate::{EngineError, ExecutionCoordinator};

    use super::bind_value_to_sql;

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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
    fn vector_read_returns_error_when_table_absent() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
            .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .vector_search("budget embeddings", 5)
            .compile()
            .expect("vector query compiles");

        let result = coordinator.execute_compiled_read(&compiled);
        assert!(
            matches!(result, Err(EngineError::CapabilityMissing(_))),
            "vector read must fail with CapabilityMissing when vec_nodes_active table is absent"
        );
    }

    #[test]
    fn coordinator_caches_by_shape_hash() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
            .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        let plan = coordinator.explain_compiled_read(&compiled);

        assert_eq!(plan.sql, compiled.sql);
    }

    #[test]
    fn explain_returns_correct_driving_table() {
        use fathomdb_query::DrivingTable;

        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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

    // --- Item 1: capability gate tests ---

    #[test]
    fn capability_gate_reports_false_without_feature() {
        let db = NamedTempFile::new().expect("temporary db");
        // Open without vector_dimension: regardless of feature flag, vector_enabled must be false
        // when no dimension is requested (the vector profile is never bootstrapped).
        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), Some(128))
                .expect("coordinator");
        assert!(
            coordinator.vector_enabled(),
            "vector_enabled must be true when sqlite-vec feature is active and dimension is set"
        );
    }

    // --- Item 4: runtime table read tests ---

    #[test]
    fn read_run_returns_inserted_run() {
        use crate::{
            ProvenanceMode, RunInsert, WriteRequest, WriterActor,
            writer::{ActionInsert, StepInsert},
        };

        let db = NamedTempFile::new().expect("temporary db");
        let writer =
            WriterActor::start(db.path(), Arc::new(SchemaManager::new()), ProvenanceMode::Warn)
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
            })
            .expect("write run");

        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        use crate::{
            ProvenanceMode, RunInsert, WriteRequest, WriterActor,
            writer::{ActionInsert, StepInsert},
        };

        let db = NamedTempFile::new().expect("temporary db");
        let writer =
            WriterActor::start(db.path(), Arc::new(SchemaManager::new()), ProvenanceMode::Warn)
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
            })
            .expect("write step");

        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        let writer =
            WriterActor::start(db.path(), Arc::new(SchemaManager::new()), ProvenanceMode::Warn)
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
            })
            .expect("write action");

        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
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
        use crate::{
            ProvenanceMode, RunInsert, WriteRequest, WriterActor,
            writer::{ActionInsert, StepInsert},
        };

        let db = NamedTempFile::new().expect("temporary db");
        let writer =
            WriterActor::start(db.path(), Arc::new(SchemaManager::new()), ProvenanceMode::Warn)
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
            })
            .expect("v2 write");

        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), None)
                .expect("coordinator");
        let active = coordinator.read_active_runs().expect("read_active_runs");

        assert_eq!(active.len(), 1, "only the non-superseded run should appear");
        assert_eq!(active[0].id, "run-v2");
    }
}
