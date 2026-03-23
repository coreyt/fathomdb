use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fathomdb_query::{CompiledQuery, ShapeHash};
use fathomdb_schema::SchemaManager;
use rusqlite::{Connection, params_from_iter, types::Value};

use crate::{EngineError, sqlite};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRow {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryRows {
    pub nodes: Vec<NodeRow>,
}

pub struct ExecutionCoordinator {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    conn: Mutex<Connection>,
    statement_cache: Mutex<HashMap<ShapeHash, String>>,
}

impl fmt::Debug for ExecutionCoordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionCoordinator")
            .field("database_path", &self.database_path)
            .finish_non_exhaustive()
    }
}

impl ExecutionCoordinator {
    pub fn open(
        path: impl AsRef<Path>,
        schema_manager: Arc<SchemaManager>,
    ) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        let conn = sqlite::open_connection(&path)?;
        schema_manager.bootstrap(&conn)?;
        Ok(Self {
            database_path: path,
            schema_manager,
            conn: Mutex::new(conn),
            statement_cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn execute_compiled_read(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<QueryRows, EngineError> {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .insert(compiled.shape_hash, compiled.sql.clone());

        let bind_values = compiled
            .binds
            .iter()
            .map(bind_value_to_sql)
            .collect::<Vec<_>>();

        let conn_guard = self.conn.lock().expect("connection mutex poisoned");
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

        Ok(QueryRows { nodes })
    }

    pub fn cached_statement_count(&self) -> usize {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .len()
    }

    pub fn schema_manager(&self) -> Arc<SchemaManager> {
        Arc::clone(&self.schema_manager)
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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()))
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
        assert_eq!(coordinator.cached_statement_count(), 1);
    }

    #[test]
    fn vector_read_returns_error_when_table_absent() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()))
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
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()))
            .expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        coordinator
            .execute_compiled_read(&compiled)
            .expect("execute compiled read");
        assert_eq!(coordinator.cached_statement_count(), 1);
    }

    #[test]
    fn coordinator_executes_compiled_read() {
        let db = NamedTempFile::new().expect("temporary db");
        let coordinator = ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()))
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
}
