use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fathomdb_query::{CompiledQuery, ShapeHash};
use fathomdb_schema::SchemaManager;

use crate::{sqlite, EngineError};

#[derive(Clone, Debug)]
pub struct DispatchedRead {
    pub sql: String,
    pub shape_hash: ShapeHash,
}

#[derive(Debug)]
pub struct ExecutionCoordinator {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    statement_cache: Mutex<HashMap<ShapeHash, String>>,
}

impl ExecutionCoordinator {
    pub fn open(path: impl AsRef<Path>, schema_manager: Arc<SchemaManager>) -> Result<Self, EngineError> {
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

    pub fn dispatch_compiled_read(&self, compiled: &CompiledQuery) -> Result<DispatchedRead, EngineError> {
        self.statement_cache
            .lock()
            .expect("statement cache mutex poisoned")
            .insert(compiled.shape_hash, compiled.sql.clone());
        Ok(DispatchedRead {
            sql: compiled.sql.clone(),
            shape_hash: compiled.shape_hash,
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
        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new())).expect("coordinator");

        let compiled = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .compile()
            .expect("compiled query");

        coordinator
            .dispatch_compiled_read(&compiled)
            .expect("dispatch read");
        assert_eq!(coordinator.cached_statement_count(), 1);
    }
}
