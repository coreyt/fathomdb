//! Closure-evidence contract and reproducible harness for Slice 19.

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use fathomdb_engine::{Engine, InitialState, PreparedWrite, SourceId};
use fathomdb_schema::{Migration, MIGRATIONS};
use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;

const FIXTURE_VERSION: &str = "slice19-deterministic-v1";
const FIXTURE_SEED: u64 = 0x5A19_2026_0808_0001;
const FIXTURE_NODES: usize = 500;
const FIXTURE_EDGES: usize = 500;
const WRITE_BATCH_SIZE: usize = 250;
const MIGRATION_SAMPLES: usize = 3;
const INGEST_SAMPLES: usize = 3;
const QUERY_SAMPLES: usize = 3;
const QUERY_WARMUPS: usize = 1;
const QUERY: &str = "slice19needle";
const OUTPUT_ENV: &str = "FATHOMDB_SLICE19_MEASUREMENT_OUTPUT";
const WITH_INDEXES: &str = "with_indexes";
const WITHOUT_INDEXES: &str = "without_indexes";

const SCHEMA_25_MIGRATIONS: &[Migration] = {
    let (head, _) = MIGRATIONS.split_at(25);
    head
};

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
    assert_eq!(required(&record, "fixture.query_api"), "Engine::search_with_limit(query, 100)");
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
    assert!(
        required(&record, "semantic_equivalence.text_edge_result_count")
            .as_u64()
            .is_some_and(|count| count > 0),
        "normal-search evidence must retain at least one edge FTS result"
    );

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

#[derive(Debug)]
struct ScenarioSamples {
    ingest_us: Vec<u64>,
    query_us: Vec<u64>,
    query_signature: Vec<String>,
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}.sqlite"))
}

fn fixture_words(index: usize) -> u64 {
    let mut value = FIXTURE_SEED ^ u64::try_from(index).expect("fixture index fits u64");
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn fixture_writes() -> (Vec<PreparedWrite>, Vec<PreparedWrite>) {
    let nodes = (0..FIXTURE_NODES)
        .map(|index| PreparedWrite::Node {
            kind: "fixture_doc".to_string(),
            body: format!(
                "{QUERY} deterministic node {index} payload {:016x}",
                fixture_words(index)
            ),
            source_id: SourceId::new(format!("fixture:node:{index}")).expect("fixture source id"),
            logical_id: Some(format!("fixture-node-{index}")),
            state: InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        })
        .collect();
    let edges = (0..FIXTURE_EDGES)
        .map(|index| PreparedWrite::Edge {
            kind: "fixture_relation".to_string(),
            from: format!("fixture-node-{}", index % FIXTURE_NODES),
            to: format!("fixture-node-{}", (index + 1) % FIXTURE_NODES),
            source_id: SourceId::new(format!("fixture:edge:{index}")).expect("fixture source id"),
            logical_id: Some(format!("fixture-edge-{index}")),
            body: Some(format!(
                "{QUERY} deterministic edge {index} payload {:016x}",
                fixture_words(FIXTURE_NODES + index)
            )),
            t_valid: None,
            t_invalid: None,
            confidence: None,
            extractor_model_id: None,
            temporal_fallback: None,
        })
        .collect();
    (nodes, edges)
}

fn write_fixture(engine: &Engine) {
    let (nodes, edges) = fixture_writes();
    for batch in nodes.chunks(WRITE_BATCH_SIZE) {
        engine.write(batch).expect("write deterministic fixture node batch");
    }
    for batch in edges.chunks(WRITE_BATCH_SIZE) {
        engine.write(batch).expect("write deterministic fixture edge batch");
    }
}

fn open_ingest_scenario(path: &PathBuf, indexes_present: bool) -> Engine {
    let opened = Engine::open(path).expect("open fresh schema-26 fixture");
    if indexes_present {
        return opened.engine;
    }
    opened.engine.close().expect("close fresh schema-26 fixture");
    let connection = Connection::open(path).expect("open fixture to remove indexes for control");
    connection
        .execute_batch(
            "DROP INDEX canonical_nodes_write_cursor_idx;
             DROP INDEX canonical_edges_write_cursor_idx;",
        )
        .expect("remove Slice 19 indexes for unsupported baseline control");
    drop(connection);
    Engine::open(path).expect("reopen no-index control fixture").engine
}

fn search_signature(engine: &Engine) -> Vec<String> {
    engine
        .search_text_only_with_limit(QUERY, 100)
        .expect("run fixture text search")
        .results
        .into_iter()
        .map(|hit| format!("{}|{}|{}|{:?}", hit.write_cursor, hit.kind, hit.body, hit.branch))
        .collect()
}

fn measure_scenario(indexes_present: bool) -> ScenarioSamples {
    let mut ingest_us = Vec::with_capacity(INGEST_SAMPLES);
    let mut query_us = Vec::with_capacity(QUERY_SAMPLES);
    let mut expected_signature = None;

    for sample in 0..INGEST_SAMPLES {
        let dir = TempDir::new().expect("scenario tempdir");
        let path = db_path(&dir, if indexes_present { "with_indexes" } else { "without_indexes" });
        let engine = open_ingest_scenario(&path, indexes_present);
        let started = Instant::now();
        write_fixture(&engine);
        ingest_us.push(elapsed_us(started));

        let signature = search_signature(&engine);
        assert_eq!(
            signature.len(),
            100,
            "the public result-limit contract caps fixture results at 100"
        );
        if let Some(expected) = &expected_signature {
            assert_eq!(&signature, expected, "repeat fixture semantics remain stable");
        } else {
            expected_signature = Some(signature);
        }

        if sample == 0 {
            for _ in 0..QUERY_WARMUPS {
                assert_eq!(
                    search_signature(&engine),
                    *expected_signature.as_ref().expect("signature")
                );
            }
            for _ in 0..QUERY_SAMPLES {
                let started = Instant::now();
                let signature = search_signature(&engine);
                query_us.push(elapsed_us(started));
                assert_eq!(signature, *expected_signature.as_ref().expect("signature"));
            }
        }
        engine.close().expect("close scenario engine");
    }

    ScenarioSamples {
        ingest_us,
        query_us,
        query_signature: expected_signature.expect("at least one scenario sample"),
    }
}

fn measure_migration() -> (Vec<u64>, Vec<u64>) {
    let mut wall_clock_us = Vec::with_capacity(MIGRATION_SAMPLES);
    let mut step_report_ms = Vec::with_capacity(MIGRATION_SAMPLES);
    for sample in 0..MIGRATION_SAMPLES {
        let dir = TempDir::new().expect("migration tempdir");
        let path = db_path(&dir, &format!("schema25-{sample}"));
        let legacy = Engine::open_with_migrations_for_test(&path, SCHEMA_25_MIGRATIONS, |_| {})
            .expect("build schema-25 fixture through supported test migration seam");
        assert_eq!(legacy.report.schema_version_after, 25);
        write_fixture(&legacy.engine);
        legacy.engine.close().expect("close schema-25 fixture");

        let started = Instant::now();
        let upgraded = Engine::open(&path).expect("run schema-25 to schema-26 migration");
        wall_clock_us.push(elapsed_us(started));
        let report_ms = upgraded
            .report
            .migration_steps
            .iter()
            .find(|step| step.step_id == 26)
            .and_then(|step| step.duration_ms)
            .expect("step 26 report duration");
        step_report_ms.push(report_ms);
        upgraded.engine.close().expect("close migrated fixture");
    }
    (wall_clock_us, step_report_ms)
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn git_revision() -> String {
    Command::new("git")
        .arg("-C")
        .arg(repository_root())
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_string())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn read_trimmed(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|contents| contents.trim().to_string())
        .unwrap_or_else(|_| "unavailable".to_string())
}

fn cpu_summary() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("model name\t: ")
                    .map(str::to_string)
                    .or_else(|| line.strip_prefix("Hardware\t: ").map(str::to_string))
            })
        })
        .unwrap_or_else(|| "unavailable".to_string())
}

fn sqlite_environment() -> (String, Vec<String>) {
    let connection = Connection::open_in_memory().expect("open SQLite environment probe");
    let version = connection
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .expect("read SQLite version");
    let mut statement =
        connection.prepare("PRAGMA compile_options").expect("prepare SQLite options");
    let mut options = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("read SQLite options")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect SQLite options");
    options.sort();
    (version, options)
}

fn scenario_map(
    scenarios: &BTreeMap<&str, ScenarioSamples>,
    field: fn(&ScenarioSamples) -> &Vec<u64>,
) -> Value {
    serde_json::json!({
        WITH_INDEXES: field(scenarios.get(WITH_INDEXES).expect("with-index samples")),
        WITHOUT_INDEXES: field(scenarios.get(WITHOUT_INDEXES).expect("without-index samples")),
    })
}

#[test]
#[ignore = "measurement-only; set FATHOMDB_SLICE19_MEASUREMENT_OUTPUT to write the raw JSON"]
fn write_cursor_join_index_measurement() {
    let output = env::var_os(OUTPUT_ENV).expect("measurement output path is required");
    let (migration_us, migration_step_report_ms) = measure_migration();
    let mut scenarios = BTreeMap::new();
    scenarios.insert(WITH_INDEXES, measure_scenario(true));
    scenarios.insert(WITHOUT_INDEXES, measure_scenario(false));
    assert_eq!(
        scenarios.get(WITH_INDEXES).expect("with-index scenario").query_signature,
        scenarios.get(WITHOUT_INDEXES).expect("without-index scenario").query_signature,
        "the diagnostic no-index control must preserve the public Engine text-search result sequence"
    );
    let (sqlite_version, sqlite_compile_options) = sqlite_environment();
    let record = serde_json::json!({
        "record_version": 1,
        "scope": "fixture_scoped",
        "source_revision": git_revision(),
        "schema": {
            "from": 25,
            "to": 26,
            "construction": "Each migration sample opens an empty path with Engine::open_with_migrations_for_test and migrations 1..=25, ingests the deterministic fixture with Engine::write, closes it, then times Engine::open at head (step 26 only)."
        },
        "harness": {
            "test_target": "slice19_measurement",
            "test_name": "write_cursor_join_index_measurement",
            "command": "FATHOMDB_SLICE19_MEASUREMENT_OUTPUT=/tmp/fathomdb-slice19-measurement.json ~/.cargo/bin/cargo test -p fathomdb-engine --test slice19_measurement write_cursor_join_index_measurement -- --ignored --exact",
            "output_env": OUTPUT_ENV
        },
        "fixture": {
            "generator": {
                "version": FIXTURE_VERSION,
                "seed": FIXTURE_SEED,
                "description": "500 active nodes and 500 body-bearing edges. Every body matches slice19needle; per-row payload is a SplitMix64-style deterministic transform of the seed and row index."
            },
            "nodes": FIXTURE_NODES,
            "edges": FIXTURE_EDGES,
            "write_batch_size": WRITE_BATCH_SIZE,
            "query": QUERY,
            "query_api": "Engine::search_text_only_with_limit(query, 100)"
        },
        "environment": {
            "os": env::consts::OS,
            "arch": env::consts::ARCH,
            "kernel": read_trimmed("/proc/sys/kernel/osrelease"),
            "sqlite": {
                "version": sqlite_version,
                "compile_options": sqlite_compile_options
            },
            "hardware": {
                "summary": cpu_summary(),
                "logical_cpus": std::thread::available_parallelism().map(usize::from).unwrap_or(0)
            }
        },
        "cache_treatment": {
            "fixture_paths": "Every migration and ingest sample uses a fresh TempDir database path.",
            "migration": "A schema-25 database is closed before the timed schema-26 Engine::open. The harness does not attempt privileged OS page-cache eviction.",
            "query": format!("{QUERY_WARMUPS} unrecorded Engine::search_text_only_with_limit calls warm each scenario's reader/process path before {QUERY_SAMPLES} recorded calls."),
            "os_page_cache_evicted": false,
            "caveat": "Samples are not classified as cold-cache measurements; host filesystem-cache state is uncontrolled."
        },
        "control": {
            "with_indexes": "Fresh schema-26 database with both Slice 19 indexes present.",
            "without_indexes": "Fresh schema-26 database, closed, then both Slice 19 indexes removed with rusqlite before reopening. This deliberately altered database is a diagnostic control, not a supported production state."
        },
        "semantic_equivalence": {
            "verified": true,
            "method": "The full ordered public Engine::search_text_only_with_limit result signature (write cursor, kind, body, branch) is equal between controls and across repeats.",
            "result_count": scenarios.get(WITH_INDEXES).expect("with-index scenario").query_signature.len()
        },
        "raw_samples": {
            "units": "microseconds",
            "migration_25_to_26_us": migration_us,
            "migration_step_26_report_ms": migration_step_report_ms,
            "ingest_us": scenario_map(&scenarios, |samples| &samples.ingest_us),
            "query_us": scenario_map(&scenarios, |samples| &samples.query_us)
        },
        "limitations": [
            "This is a deterministic synthetic fixture, not a corpus-scale, workload-representative benchmark.",
            "The result does not assert a CI latency target, production throughput target, or generalized speedup.",
            "Only the public Engine write and text-search paths are timed; the direct SQLite index removal exists solely to create the diagnostic no-index control."
        ]
    });
    std::fs::write(
        &output,
        format!("{}\n", serde_json::to_string_pretty(&record).expect("serialize record")),
    )
    .expect("write measurement output");
}
