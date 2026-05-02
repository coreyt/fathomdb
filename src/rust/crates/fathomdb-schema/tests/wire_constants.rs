use fathomdb_schema::{
    JOURNAL_SUFFIX, LOCK_SUFFIX, PRAGMA_USER_VERSION, SQLITE_SUFFIX, WAL_SUFFIX,
};

#[test]
fn pragma_user_version_spelling() {
    assert_eq!(PRAGMA_USER_VERSION, "user_version");
}

#[test]
fn file_suffix_spellings() {
    assert_eq!(SQLITE_SUFFIX, ".sqlite");
    assert_eq!(WAL_SUFFIX, "-wal");
    assert_eq!(LOCK_SUFFIX, ".lock");
    assert_eq!(JOURNAL_SUFFIX, "-journal");
}
