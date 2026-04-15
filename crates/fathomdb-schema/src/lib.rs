#[macro_use]
mod trace_support;

mod bootstrap;
mod migration;

pub use bootstrap::{
    BootstrapReport, DEFAULT_FTS_TOKENIZER, SchemaManager, fts_column_name, fts_kind_table_name,
    resolve_fts_tokenizer,
};
pub use migration::{Migration, SchemaVersion};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("missing sqlite capability: {0}")]
    MissingCapability(&'static str),
    #[error(
        "database schema version {database_version} is newer than engine version {engine_version}; upgrade the engine"
    )]
    VersionMismatch {
        database_version: u32,
        engine_version: u32,
    },
}
