use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fathomdb_query::{CompiledQuery, ShapeHash};
use fathomdb_schema::SchemaManager;
use rusqlite::{params_from_iter, types::Value};

use crate::{EngineError, sqlite};

#[derive(Clone, Debug)]
pub struct DispatchedRead {
    pub sql: String,
    pub shape_hash: ShapeHash,
}

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

#[derive(Debug)]
pub struct ExecutionCoordinator {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    statement_cache: Mutex<HashMap<ShapeHash, String>>,
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
            statement_cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn dispatch_compiled_read(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<DispatchedRead, EngineError> {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .insert(compiled.shape_hash, compiled.sql.clone());
        Ok(DispatchedRead {
            sql: compiled.sql.clone(),
            shape_hash: compiled.shape_hash,
        })
    }

    pub fn execute_compiled_read(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<QueryRows, EngineError> {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .insert(compiled.shape_hash, compiled.sql.clone());

        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

        let bind_values = compiled
            .binds
            .iter()
            .map(bind_value_to_sql)
            .collect::<Vec<_>>();
        let mut statement = conn.prepare_cached(&compiled.sql)?;
        let rows = statement.query_map(params_from_iter(bind_values.iter()), |row| {
            Ok(NodeRow {
                row_id: row.get(0)?,
                logical_id: row.get(1)?,
                kind: row.get(2)?,
                properties: row.get(3)?,
            })
        })?;

        Ok(QueryRows {
            nodes: rows.collect::<Result<Vec<_>, _>>()?,
        })
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

    use fathomdb_query::QueryBuilder;
    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use crate::ExecutionCoordinator;

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
            .dispatch_compiled_read(&compiled)
            .expect("dispatch read");
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
