use std::path::Path;
use std::sync::Arc;

use fathomdb_schema::SchemaManager;

use crate::{
    AdminHandle, AdminService, EngineError, ExecutionCoordinator, ProvenanceMode, WriterActor,
};

#[derive(Debug)]
pub struct EngineRuntime {
    coordinator: ExecutionCoordinator,
    writer: WriterActor,
    admin: AdminHandle,
}

impl EngineRuntime {
    /// # Errors
    /// Returns [`EngineError`] if the database connection cannot be opened, schema bootstrap fails,
    /// or the writer actor cannot be started.
    pub fn open(
        path: impl AsRef<Path>,
        provenance_mode: ProvenanceMode,
    ) -> Result<Self, EngineError> {
        let schema_manager = Arc::new(SchemaManager::new());
        let coordinator = ExecutionCoordinator::open(path.as_ref(), Arc::clone(&schema_manager))?;
        let writer =
            WriterActor::start(path.as_ref(), Arc::clone(&schema_manager), provenance_mode)?;
        let admin = AdminHandle::new(AdminService::new(path.as_ref(), schema_manager));

        Ok(Self {
            coordinator,
            writer,
            admin,
        })
    }

    pub fn coordinator(&self) -> &ExecutionCoordinator {
        &self.coordinator
    }

    pub fn writer(&self) -> &WriterActor {
        &self.writer
    }

    pub fn admin(&self) -> &AdminHandle {
        &self.admin
    }
}
