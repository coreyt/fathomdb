use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static COUNTER: AtomicU64 = AtomicU64::new(0);

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
mod tests {
    use super::*;

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
