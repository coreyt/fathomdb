//! Closure-evidence contract for the Slice 19 join-index measurement harness.

use std::path::PathBuf;

use serde_json::Value;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("engine crate is nested four levels below the repository root")
        .to_path_buf()
}

fn required<'a>(value: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(value, |current, segment| {
        current.get(segment).unwrap_or_else(|| panic!("measurement record is missing {path}"))
    })
}

#[test]
fn slice19_closure_record_has_reproducible_fixture_scoped_evidence() {
    let path = repository_root()
        .join("dev/plans/runs/0.8.22-slice-19-join-index-measurement-20260808.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("Slice 19 closure record must be committed at {}: {error}", path.display())
    });
    let record: Value = serde_json::from_str(&raw).expect("closure record must be JSON");

    assert_eq!(required(&record, "record_version"), 1);
    assert_eq!(required(&record, "scope"), "fixture_scoped");
    assert_eq!(required(&record, "schema.from"), 25);
    assert_eq!(required(&record, "schema.to"), 26);
    assert!(required(&record, "harness.command").as_str().is_some_and(|command| {
        command.contains("slice19_measurement") && command.contains("--ignored")
    }));
    assert!(required(&record, "fixture.generator.version").is_string());
    assert!(required(&record, "fixture.generator.seed").is_u64());
    assert!(required(&record, "fixture.nodes").as_u64().is_some_and(|count| count > 0));
    assert!(required(&record, "fixture.edges").as_u64().is_some_and(|count| count > 0));
    assert_eq!(required(&record, "cache_treatment.os_page_cache_evicted"), false);
    assert!(required(&record, "environment.sqlite.version").is_string());
    assert!(required(&record, "environment.sqlite.compile_options").is_array());
    assert!(required(&record, "environment.hardware.summary").is_string());
    assert_eq!(required(&record, "semantic_equivalence.verified"), true);

    for path in [
        "raw_samples.migration_25_to_26_us",
        "raw_samples.ingest_us.with_indexes",
        "raw_samples.ingest_us.without_indexes",
        "raw_samples.query_us.with_indexes",
        "raw_samples.query_us.without_indexes",
    ] {
        let samples =
            required(&record, path).as_array().unwrap_or_else(|| panic!("{path} must be an array"));
        assert!(samples.len() >= 3, "{path} needs at least three raw samples");
        assert!(samples.iter().all(Value::is_u64), "{path} samples must be integer microseconds");
    }
}
