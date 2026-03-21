mod bootstrap;
mod migration;

pub use bootstrap::{BootstrapReport, SchemaManager};
pub use migration::{Migration, SchemaVersion};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("missing sqlite capability: {0}")]
    MissingCapability(&'static str),
}
