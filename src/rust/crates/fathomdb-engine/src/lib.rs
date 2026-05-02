use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_query::compile_text_query;
use fathomdb_schema::SCHEMA_VERSION;

#[derive(Debug)]
pub struct Engine {
    path: PathBuf,
    next_cursor: AtomicU64,
    closed: AtomicBool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenReport {
    pub schema_version: u32,
    pub query_backend: &'static str,
    pub default_embedder: EmbedderIdentity,
}

#[derive(Debug)]
pub struct OpenedEngine {
    pub engine: Engine,
    pub report: OpenReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteReceipt {
    pub cursor: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub projection_cursor: u64,
    pub results: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedWrite {
    Node { kind: String, body: String },
    Edge { kind: String, from: String, to: String },
    OpStore { schema_id: String, body: String },
    AdminSchema { name: String, body: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineOpenError {
    EmptyPath,
}

impl Display for EngineOpenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "engine path must not be empty"),
        }
    }
}

impl Error for EngineOpenError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineError {
    Closed,
    EmptyQuery,
}

impl Display for EngineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "engine is closed"),
            Self::EmptyQuery => write!(f, "search query must not be empty"),
        }
    }
}

impl Error for EngineError {}

impl Engine {
    pub fn open(path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(EngineOpenError::EmptyPath);
        }

        let report = OpenReport {
            schema_version: SCHEMA_VERSION,
            query_backend: "fathomdb-query scaffold",
            default_embedder: EmbedderIdentity::new("fathomdb-noop", "0.6.0-scaffold"),
        };

        Ok(OpenedEngine {
            engine: Self { path, next_cursor: AtomicU64::new(0), closed: AtomicBool::new(false) },
            report,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, batch: &[PreparedWrite]) -> Result<WriteReceipt, EngineError> {
        self.ensure_open()?;

        let increment = u64::try_from(batch.len()).unwrap_or(u64::MAX).max(1);
        let previous = self.next_cursor.fetch_add(increment, Ordering::SeqCst);

        Ok(WriteReceipt { cursor: previous.saturating_add(increment) })
    }

    pub fn search(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.ensure_open()?;
        if query.trim().is_empty() {
            return Err(EngineError::EmptyQuery);
        }

        let compiled = compile_text_query(query);
        let cursor = self.next_cursor.load(Ordering::SeqCst);

        Ok(SearchResult { projection_cursor: cursor, results: vec![compiled.sql] })
    }

    pub fn close(&self) -> Result<(), EngineError> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), EngineError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(EngineError::Closed);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, PreparedWrite};

    #[test]
    fn write_advances_cursor() {
        let opened = Engine::open("rewrite.sqlite").expect("scaffold engine should open");
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
            .expect("write should succeed");

        assert_eq!(receipt.cursor, 1);
    }
}
