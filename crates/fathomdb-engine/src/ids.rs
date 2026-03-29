use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use ulid::Ulid;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a new identifier suitable for use as a `row_id`, `logical_id`, or
/// chunk/run/step/action `id`.
///
/// Returns a 26-character ULID (Universally Unique Lexicographically Sortable Identifier).
/// ULIDs are timestamp-prefixed so IDs generated close in time sort together.
/// They are case-insensitive and URL-safe.
///
/// This function is not part of the write path. Callers that already have stable
/// identifiers are not required to use it.
#[must_use]
pub fn new_id() -> String {
    Ulid::new().to_string()
}

pub fn new_row_id() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:016x}-{:08x}-{:016x}",
        now.as_secs(),
        now.subsec_nanos(),
        seq
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn new_id_returns_nonempty_string() {
        let id = new_id();
        assert!(!id.is_empty(), "new_id must return a non-empty string");
    }

    #[test]
    fn new_id_returns_unique_values() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b, "consecutive new_id calls must return distinct values");
    }

    #[test]
    fn new_id_is_26_characters() {
        let id = new_id();
        assert_eq!(
            id.len(),
            26,
            "ULID must be exactly 26 characters, got: {id}"
        );
    }

    #[test]
    fn new_id_is_valid_for_node_insert() {
        use std::sync::Arc;

        use fathomdb_schema::SchemaManager;
        use tempfile::NamedTempFile;

        use crate::{ChunkPolicy, NodeInsert, ProvenanceMode, WriteRequest, WriterActor};

        let db = NamedTempFile::new().expect("temporary db");
        let writer = WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            ProvenanceMode::Warn,
        )
        .expect("writer");

        let row_id = new_id();
        let logical_id = new_id();

        writer
            .submit(WriteRequest {
                label: "new_id_test".to_owned(),
                nodes: vec![NodeInsert {
                    row_id,
                    logical_id,
                    kind: "Note".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("test".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
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
            .expect("write with new_id must succeed");
    }

    #[test]
    fn new_row_id_returns_unique_ids() {
        let a = new_row_id();
        let b = new_row_id();
        let c = new_row_id();
        assert_ne!(a, b, "consecutive IDs must be distinct");
        assert_ne!(b, c, "consecutive IDs must be distinct");
        assert_ne!(a, c, "consecutive IDs must be distinct");
    }

    #[test]
    fn new_row_id_has_expected_format() {
        let id = new_row_id();
        assert!(!id.is_empty(), "ID must not be empty");
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
            "ID must contain only hex digits and dashes, got: {id}"
        );
    }
}
