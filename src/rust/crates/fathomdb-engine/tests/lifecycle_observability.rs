//! Lifecycle observability tests bound to AC-001..AC-010.
//!
//! Pure-type tests exercise data-type contracts pinned by
//! `dev/design/lifecycle.md` and `dev/acceptance.md`. Behavior tests bind
//! the AC measurement protocols via deterministic fixtures (slow-cte,
//! page-corruption tool, one-thread-poison stress runner) — see the
//! per-test comments and `tests/support/corruption.rs`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fathomdb_engine::lifecycle::{
    Event, EventCategory, EventSource, Phase, ProfileRecord, ProjectionStatus, SlowStatement,
    StressFailureContext, Subscriber,
};
use fathomdb_engine::{CounterSnapshot, Engine, EngineOpenError, PreparedWrite};
use tempfile::TempDir;

#[path = "support/corruption.rs"]
mod corruption;

fn fixture() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(dir.path().join("observability.sqlite")).expect("engine open").engine;
    (dir, engine)
}

#[derive(Default)]
struct CapturingSubscriber {
    events: Mutex<Vec<Event>>,
    profile_records: Mutex<Vec<ProfileRecord>>,
    slow_statements: Mutex<Vec<SlowStatement>>,
    stress_failures: Mutex<Vec<StressFailureContext>>,
}

impl Subscriber for CapturingSubscriber {
    fn on_event(&self, event: &Event) {
        self.events.lock().unwrap().push(event.clone());
    }

    fn on_profile(&self, record: &ProfileRecord) {
        self.profile_records.lock().unwrap().push(*record);
    }

    fn on_slow_statement(&self, signal: &SlowStatement) {
        self.slow_statements.lock().unwrap().push(signal.clone());
    }

    fn on_stress_failure(&self, context: &StressFailureContext) {
        self.stress_failures.lock().unwrap().push(context.clone());
    }
}

// AC-001: Phase enum is a typed value, not a substring of free text.
#[test]
fn ac_001_phase_enum_has_five_typed_variants() {
    let variants = [Phase::Started, Phase::Slow, Phase::Heartbeat, Phase::Finished, Phase::Failed];

    // Pattern-match each variant exhaustively to confirm enum shape.
    for phase in variants {
        match phase {
            Phase::Started | Phase::Slow | Phase::Heartbeat | Phase::Finished | Phase::Failed => {}
        }
    }

    assert_ne!(Phase::Started, Phase::Slow);
    assert_ne!(Phase::Started, Phase::Heartbeat);
    assert_ne!(Phase::Started, Phase::Finished);
    assert_ne!(Phase::Started, Phase::Failed);
    assert_ne!(Phase::Finished, Phase::Failed);
    assert_ne!(Phase::Slow, Phase::Heartbeat);
}

// AC-001 supporting: Event struct exposes typed source + category.
#[test]
fn ac_001_event_struct_carries_typed_source_and_category() {
    let event = Event {
        phase: Phase::Started,
        source: EventSource::Engine,
        category: EventCategory::Writer,
        code: None,
    };
    assert_eq!(event.phase, Phase::Started);
    assert_eq!(event.source, EventSource::Engine);
    assert_eq!(event.category, EventCategory::Writer);
    assert_eq!(event.code, None);
}

// AC-002: No log files written without subscriber.
//
// Measurement (`dev/acceptance.md`): "Snapshot recursive directory tree
// of `$PWD`, `$HOME`, `$XDG_*`, `$TMPDIR` pre+post; assert diff = subset
// of allow-list paths."
//
// 0.8.20 Slice 21b (R-20-CR): the measurement roots are a PRIVATE, per-test
// sandbox rather than the ambient `$HOME` / `$XDG_*` / `$TMPDIR` / `$PWD`, so
// the pre/post diff over shared, concurrently-written directories is gone.
// The prior Part-2 substring scan is retired: it was flaky-positive (another
// process's file landing in a shared root mid-test was attributed to the
// engine) and simultaneously vacuous (an engine artifact not containing the
// literal "fathomdb" / "fathom_" passed cleanly — see
// `ac_002_sandbox_oracle_catches_unsigned_artifact`).
//
// Two-part assertion:
// 1. Inside the DB parent: only the documented allow-list (DB file, `.lock`,
//    `-wal`, `-shm`, optional `-journal`) per
//    ADR-0.6.0-database-lock-mechanism-reader-pool-revision. UNCHANGED.
// 2. Outside the DB parent: every sandbox root is EMPTY. Not "contains
//    nothing matching a signature" — empty.
#[test]
fn ac_002_no_log_files_without_subscriber() {
    let sandbox = Ac002Sandbox::new();
    ac_002_run_workload(&sandbox, false);

    // Anti-vacuity: both assertions below are trivially satisfiable by a
    // workload that never ran, so pin that the engine really opened inside the
    // sandbox.
    let db_path = sandbox.db_parent().join("nolog.sqlite");
    assert!(
        db_path.exists(),
        "workload child must have created the database in the sandbox: {}",
        db_path.display(),
    );

    if let Err(msg) = ac_002_db_parent_allowlist(&sandbox) {
        panic!("{msg}");
    }
    if let Err(msg) = ac_002_no_artifacts_outside_db_path(&sandbox) {
        panic!("{msg}");
    }
}

// ── AC-002 oracle strength: per-test sandbox (0.8.20 Slice 21b, R-20-CR) ─────
//
// The AC-002 claim is "the engine writes nothing outside the DB path when no
// subscriber is attached". That is an ISOLATION property, so it is asserted by
// isolation: the workload runs against a private, freshly created sandbox and
// every root in that sandbox other than the DB parent must end up EMPTY.
// Anything appearing there is necessarily the workload's, whatever it is named.
//
// ISOLATION MECHANISM — option (a), child process. `std::env::set_var` and
// `std::env::set_current_dir` are process-global, and Rust's test harness runs
// the ~20 tests of this binary as THREADS IN ONE PROCESS; several of them open
// engines concurrently and `Engine`'s own open/search paths read process env
// (`src/lib.rs` FATHOMDB_PERF_* / FATHOMDB_PROJECTION_BATCH reads). Mutating
// env or CWD in-process here would therefore race every other test in this
// binary — making this test a SOURCE of flakiness rather than a fix for the
// one it replaces. A process-wide mutex (option (b)) cannot fix that, because
// the racing readers are inside the library under test and are not funnelled
// through any lock; and CWD has no per-thread analogue at all. So the workload
// is re-executed as a child process with a sandbox-pointing environment and
// its own CWD, following the established pattern in this crate
// (`durability_soak.rs`, `durability_open_path.rs`, `lifecycle_reliability.rs`:
// `Command::new(current_exe()) --exact --ignored <entry>` + env sentinel).

/// Recursive enumeration of every path under `root`, bounded at depth 6.
fn ac_002_walk(
    root: &std::path::Path,
    out: &mut std::collections::BTreeSet<std::path::PathBuf>,
    depth: u32,
) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        out.insert(path.clone());
        if meta.is_dir() && !meta.file_type().is_symlink() {
            ac_002_walk(&path, out, depth + 1);
        }
    }
}

/// Per-test filesystem sandbox for AC-002. One `TempDir` holding the DB parent
/// plus a private stand-in for each ambient path-producing variable the engine
/// could consult (`$HOME`, `$XDG_*`, `$TMPDIR`, the process CWD).
struct Ac002Sandbox {
    dir: TempDir,
}

impl Ac002Sandbox {
    /// Sandbox roots that must stay EMPTY — i.e. everything except `db/`.
    const OUTSIDE: [&'static str; 7] =
        ["home", "cwd", "tmp", "xdg/config", "xdg/data", "xdg/cache", "xdg/state"];

    fn new() -> Self {
        let dir = TempDir::new().expect("ac_002 sandbox tempdir");
        for rel in Self::OUTSIDE.iter().copied().chain(["db"]) {
            std::fs::create_dir_all(dir.path().join(rel)).expect("ac_002 sandbox subdir");
        }
        Self { dir }
    }

    fn root(&self) -> &std::path::Path {
        self.dir.path()
    }

    fn sub(&self, rel: &str) -> std::path::PathBuf {
        self.root().join(rel)
    }

    fn db_parent(&self) -> std::path::PathBuf {
        self.sub("db")
    }

    fn outside_roots(&self) -> Vec<std::path::PathBuf> {
        Self::OUTSIDE.iter().map(|rel| self.sub(rel)).collect()
    }
}

/// Point every ambient path-producing variable at the sandbox. `TMP`/`TEMP`/
/// `USERPROFILE` are set alongside their POSIX counterparts so the redirection
/// also holds on Windows, where `std::env::temp_dir` ignores `TMPDIR`.
fn ac_002_apply_sandbox_env(cmd: &mut std::process::Command, root: &std::path::Path) {
    cmd.env("FATHOMDB_AC002_SANDBOX", root)
        .env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("xdg/config"))
        .env("XDG_DATA_HOME", root.join("xdg/data"))
        .env("XDG_CACHE_HOME", root.join("xdg/cache"))
        .env("XDG_STATE_HOME", root.join("xdg/state"))
        .env("TMPDIR", root.join("tmp"))
        .env("TMP", root.join("tmp"))
        .env("TEMP", root.join("tmp"))
        .env_remove("XDG_RUNTIME_DIR")
        .env_remove("SQLITE_TMPDIR");
}

/// Run the AC-002 workload (open, one node write, one search, close) inside
/// `sandbox`, as a child process so no process-global state of THIS process is
/// touched. `plant_probe` additionally plants the divergent fixture (see
/// `ac_002_sandbox_oracle_catches_unsigned_artifact`).
fn ac_002_run_workload(sandbox: &Ac002Sandbox, plant_probe: bool) {
    let exe = std::env::current_exe().expect("test binary path");
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(["--exact", "--ignored", "_ac_002_sandbox_workload_entry"])
        .current_dir(sandbox.sub("cwd"));
    ac_002_apply_sandbox_env(&mut cmd, sandbox.root());
    if plant_probe {
        cmd.env("FATHOMDB_AC002_PLANT_PROBE", "1");
    }
    let out = cmd.output().expect("spawn ac_002 sandbox workload child");
    assert!(
        out.status.success(),
        "ac_002 sandbox workload child failed ({:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Sandboxed AC-002 workload entry-point. Re-invoked by `ac_002_run_workload`
/// via `Command::new(current_exe()) --exact --ignored`. Without the sentinel
/// env var the body is a no-op, so a bare
/// `cargo test --test lifecycle_observability -- --ignored` stays safe.
#[test]
#[ignore = "sandboxed workload entry-point for AC-002 (spawned as a child process)"]
fn _ac_002_sandbox_workload_entry() {
    let Some(root) = std::env::var_os("FATHOMDB_AC002_SANDBOX") else {
        return;
    };
    let root = std::path::PathBuf::from(root);

    // Fail loudly if the parent's redirection did not take: a workload that
    // still saw the real $HOME / $TMPDIR / $PWD would make the parent's
    // emptiness assertion vacuously true.
    let same = |a: &std::path::Path, b: &std::path::Path| match (a.canonicalize(), b.canonicalize())
    {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    };
    assert!(
        same(&std::env::temp_dir(), &root.join("tmp")),
        "TMPDIR redirection did not take: temp_dir() = {}",
        std::env::temp_dir().display(),
    );
    let cwd = std::env::current_dir().expect("child cwd");
    assert!(same(&cwd, &root.join("cwd")), "CWD redirection did not take: {}", cwd.display());
    let mut expected_vars: Vec<(&str, &str)> = vec![
        ("XDG_CONFIG_HOME", "xdg/config"),
        ("XDG_DATA_HOME", "xdg/data"),
        ("XDG_CACHE_HOME", "xdg/cache"),
        ("XDG_STATE_HOME", "xdg/state"),
    ];
    expected_vars.push(if cfg!(windows) { ("USERPROFILE", "home") } else { ("HOME", "home") });
    for (var, rel) in expected_vars {
        let raw =
            std::env::var_os(var).unwrap_or_else(|| panic!("{var} must be set by the parent"));
        assert!(
            same(std::path::Path::new(&raw), &root.join(rel)),
            "{var} redirection did not take: {:?}",
            raw,
        );
    }

    if std::env::var_os("FATHOMDB_AC002_PLANT_PROBE").is_some() {
        // Divergent fixture: an artifact OUTSIDE the DB parent but INSIDE the
        // sandbox whose name contains neither "fathomdb" nor "fathom_" — the
        // exact shape the pre-21b substring scan waved through.
        std::fs::write(root.join("home").join("telemetry.spool"), b"probe")
            .expect("plant ac_002 divergent fixture");
    }

    let opened = Engine::open(root.join("db").join("nolog.sqlite")).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hello".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write");
    let _ = opened.engine.search("hello").expect("search");
    opened.engine.close().expect("close");
}

/// AC-002 oracle: nothing may appear outside the DB path.
///
/// ISOLATION, not detection. Each sandbox root is created empty and only the
/// workload child runs against it, so the assertion is plain EMPTINESS —
/// whatever the artifact is named, whatever it contains, it is caught.
fn ac_002_no_artifacts_outside_db_path(sandbox: &Ac002Sandbox) -> Result<(), String> {
    for root in sandbox.outside_roots() {
        let mut found = std::collections::BTreeSet::new();
        ac_002_walk(&root, &mut found, 0);
        if let Some(first) = found.iter().next() {
            return Err(format!(
                "engine created {} path(s) outside the DB path, under {} — first: {}",
                found.len(),
                root.display(),
                first.display(),
            ));
        }
    }
    Ok(())
}

/// The pre-21b Part-2 oracle, reproduced verbatim. Retained ONLY as the
/// negative control in `ac_002_sandbox_oracle_catches_unsigned_artifact`; it is
/// not part of the AC-002 assertion any more.
fn ac_002_legacy_substring_oracle(sandbox: &Ac002Sandbox) -> Result<(), String> {
    let roots = sandbox.outside_roots();
    let mut found = std::collections::BTreeSet::new();
    for root in &roots {
        ac_002_walk(root, &mut found, 0);
    }
    for path in &found {
        // Strip the MOST SPECIFIC (longest) matching root, as the pre-21b
        // implementation did.
        let relative = roots
            .iter()
            .filter_map(|root| path.strip_prefix(root).ok())
            .min_by_key(|rel| rel.components().count())
            .unwrap_or(path.as_path());
        let rel_lossy = relative.to_string_lossy().to_lowercase();
        if rel_lossy.contains("fathomdb") || rel_lossy.contains("fathom_") {
            return Err(format!(
                "engine created a fathomdb-named artifact outside the DB path: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Part 1 of the AC-002 assertion, unchanged since 0.6.0: inside the DB parent
/// only the documented lock/WAL/journal siblings may appear
/// (ADR-0.6.0-database-lock-mechanism-reader-pool-revision).
fn ac_002_db_parent_allowlist(sandbox: &Ac002Sandbox) -> Result<(), String> {
    const ALLOWED_NAMES: [&str; 5] = [
        "nolog.sqlite",
        "nolog.sqlite.lock",
        "nolog.sqlite-wal",
        "nolog.sqlite-shm",
        "nolog.sqlite-journal",
    ];
    let db_parent = sandbox.db_parent();
    let mut found = std::collections::BTreeSet::new();
    ac_002_walk(&db_parent, &mut found, 0);
    for path in &found {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !ALLOWED_NAMES.contains(&name) {
            return Err(format!(
                "engine created unexpected artifact inside DB parent: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

// AC-002 oracle-strength probe (permanent negative control).
//
// Pins the DIVERGENCE that motivated Slice 21b: an artifact outside the DB
// parent whose name contains neither "fathomdb" nor "fathom_" is waved through
// by the pre-21b substring scan and caught by the sandbox oracle. If this test
// ever goes green by the oracle weakening back to a name-pattern scan, AC-002
// has become vacuous — hence both directions are asserted here.
#[test]
fn ac_002_sandbox_oracle_catches_unsigned_artifact() {
    let sandbox = Ac002Sandbox::new();
    ac_002_run_workload(&sandbox, true);

    let probe = sandbox.sub("home").join("telemetry.spool");
    assert!(probe.exists(), "divergent fixture must have been planted: {}", probe.display());

    // (i) the pre-21b oracle is VACUOUS on this artifact — that is the defect.
    assert!(
        ac_002_legacy_substring_oracle(&sandbox).is_ok(),
        "negative control: the pre-21b substring scan is expected to PASS an \
         unsigned artifact; if it now fails, this control no longer witnesses \
         the divergence it was written for",
    );

    // (ii) the sandbox oracle CATCHES it.
    let err = ac_002_no_artifacts_outside_db_path(&sandbox).expect_err(
        "AC-002 oracle must reject an artifact outside the DB path even though its \
         name carries no fathomdb signature",
    );
    assert!(err.contains("telemetry.spool"), "oracle must name the offending artifact; got: {err}");
}

// AC-003a: Writer events flow to host subscriber.
#[test]
fn ac_003a_writer_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.write(&[PreparedWrite::Node {
        kind: "doc".to_string(),
        body: "hello".to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: None,
        state: fathomdb_engine::InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }]);
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Writer));
}

// AC-003b: Search events flow to host subscriber.
#[test]
fn ac_003b_search_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.search("hello");
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Search));
}

// AC-003c: Admin events flow to host subscriber.
#[test]
fn ac_003c_admin_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.write(&[PreparedWrite::AdminSchema {
        name: "things".to_string(),
        kind: "latest_state".to_string(),
        schema_json: "{}".to_string(),
        retention_json: "{}".to_string(),
    }]);
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Admin));
}

// AC-003d: Error events flow to host subscriber before failure raises.
#[test]
fn ac_003d_error_events_flow_to_subscriber() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    let _ = engine.write(&[]); // empty batch -> WriteValidation
    let captured = sink.events.lock().unwrap();
    assert!(captured
        .iter()
        .any(|e| e.source == EventSource::Engine && e.category == EventCategory::Error));
}

// AC-004a: Counter snapshot exposes documented seven-key set, fresh-engine zero.
#[test]
fn ac_004a_counter_snapshot_key_set() {
    let (_dir, engine) = fixture();
    let snapshot = engine.counters();
    // Compile-level shape lock — these field accesses must compile.
    assert_eq!(snapshot.queries, 0);
    assert_eq!(snapshot.writes, 0);
    assert_eq!(snapshot.write_rows, 0);
    assert_eq!(snapshot.admin_ops, 0);
    assert_eq!(snapshot.cache_hit, 0);
    assert_eq!(snapshot.cache_miss, 0);
    assert!(snapshot.errors_by_code.is_empty());
    let _: BTreeMap<String, u64> = snapshot.errors_by_code.clone();
}

// AC-004b: Counter delta exact for write/query/admin keys after N=1,000
// mixed ops.
//
// Measurement (`dev/acceptance.md`): "Snapshot delta over N=1,000 mixed
// ops equals issued op counts exactly for `queries`, `writes`,
// `write_rows`, `admin_ops`. `cache_hit` / `cache_miss` are monotonic
// non-decreasing." Mix is 400 writes + 400 searches + 200 admin ops =
// 1,000 ops.
#[test]
fn ac_004b_counter_delta_exact_over_mixed_ops() {
    let (_dir, engine) = fixture();
    let s0 = engine.counters();
    for _ in 0..400 {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "hello".to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            }])
            .expect("write");
    }
    for _ in 0..400 {
        let _ = engine.search("hello").expect("search");
    }
    for i in 0..200 {
        engine
            .write(&[PreparedWrite::AdminSchema {
                name: format!("things_{}", i % 4),
                kind: "latest_state".to_string(),
                schema_json: "{}".to_string(),
                retention_json: "{}".to_string(),
            }])
            .expect("admin");
    }
    let s1 = engine.counters();
    assert_eq!(s1.writes - s0.writes, 400, "writes");
    assert_eq!(s1.write_rows - s0.write_rows, 400, "write_rows");
    assert_eq!(s1.queries - s0.queries, 400, "queries");
    assert_eq!(s1.admin_ops - s0.admin_ops, 200, "admin_ops");
    assert!(s1.cache_hit >= s0.cache_hit, "cache_hit monotonic non-decreasing");
    assert!(s1.cache_miss >= s0.cache_miss, "cache_miss monotonic non-decreasing");
}

// AC-004c: Counter snapshot read does not perturb counters.
#[test]
fn ac_004c_counter_snapshot_does_not_perturb() {
    let (_dir, engine) = fixture();
    let s0 = engine.counters();
    let s1 = engine.counters();
    assert_eq!(s0, s1);
}

// AC-005a: Per-statement profiling toggleable at runtime.
//
// Measurement (`dev/acceptance.md`): "Open engine; assert profiling
// disabled (no profile records on a fixture query); call enable-profiling
// API; assert subsequent fixture query emits ≥ 1 profile record."
#[test]
fn ac_005a_profiling_toggleable_at_runtime() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    // Profiling disabled by default — fixture query emits no records.
    engine.set_profiling(false).expect("disable profiling");
    let _ = engine.search("hello").expect("search");
    assert_eq!(
        sink.profile_records.lock().unwrap().len(),
        0,
        "no profile records expected while profiling disabled"
    );

    // Enabling profiling makes the next fixture query emit ≥ 1 record.
    engine.set_profiling(true).expect("enable profiling");
    let _ = engine.search("hello").expect("search");
    let after = sink.profile_records.lock().unwrap().len();
    assert!(after >= 1, "expected ≥ 1 profile record after enabling profiling, saw {after}");

    // Disabling profiling stops further records (sanity check on the
    // runtime-toggle contract).
    engine.set_profiling(false).expect("disable profiling again");
    let frozen = sink.profile_records.lock().unwrap().len();
    let _ = engine.search("hello").expect("search");
    assert_eq!(sink.profile_records.lock().unwrap().len(), frozen);
}

// AC-005b: Profile record schema is typed numeric.
//
// Measurement: emit one profile record via AC-005a's protocol; assert
// all three fields present and numeric. We deliberately do not pin
// non-zero values — `step_count` and `cache_delta` are emitted as 0
// in 0.6.0 because `sqlite3_profile` does not surface them in its
// callback. AC-005b contract is "typed numeric", not "non-zero".
#[test]
fn ac_005b_profile_record_typed_numeric_fields() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    engine.set_profiling(true).expect("enable profiling");
    let _ = engine.search("hello").expect("search");

    let records = sink.profile_records.lock().unwrap();
    let record = records.first().expect("at least one profile record");
    let _: u64 = record.wall_clock_ms;
    let _: u64 = record.step_count;
    let _: i64 = record.cache_delta;
}

// AC-006: SQLite-internal events surfaced with typed source tag.
//
// Page-1 magic-header bit-flip flavor: documented page-corruption tool
// (`corruption::corrupt_database_header`) overwrites the SQLite magic
// string. Reopen surfaces `SQLITE_NOTADB` and the engine emits a
// `(SqliteInternal, Corruption, code = "SQLITE_NOTADB")` event.
#[test]
fn ac_006_sqlite_internal_events_typed_source() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt-open.sqlite");
    let opened = Engine::open(&path).expect("seed database");
    opened.engine.close().expect("close before corruption");
    corruption::corrupt_database_header(&path);

    let sink = Arc::new(CapturingSubscriber::default());
    let err = Engine::open_with_subscriber_for_test(&path, sink.clone())
        .expect_err("corrupted database must fail open");

    assert!(matches!(err, EngineOpenError::Corruption(_)));

    let captured = sink.events.lock().unwrap();
    assert!(captured.iter().any(|e| {
        e.source == EventSource::SqliteInternal
            && e.category == EventCategory::Corruption
            && e.code == Some("SQLITE_NOTADB")
    }));
}

// AC-006: Interior-page bit-flip flavor.
//
// Documented page-corruption tool (`corruption::corrupt_interior_page_byte`)
// XORs a byte inside the page-1 b-tree header (offset 100, the first byte
// of the b-tree page header that follows the SQLite file header). The
// SQLite magic string is preserved, so reopen reaches the engine's
// schema probe (`PRAGMA schema_version`); decoding the corrupt b-tree
// page surfaces `SQLITE_CORRUPT`. The engine emits a
// `(SqliteInternal, Corruption, code = "SQLITE_CORRUPT")` event.
#[test]
fn ac_006_interior_page_corruption_emits_sqlite_corrupt() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt-interior.sqlite");
    let opened = Engine::open(&path).expect("seed database");
    // Force some user data so b-tree pages have non-trivial content;
    // not strictly required for the SQLITE_CORRUPT trip, but makes the
    // corruption locator deterministic across runs.
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "interior".to_string(),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("seed write");
    opened.engine.close().expect("close before corruption");
    // Page index 0, byte offset 100 (start of the page-1 b-tree header),
    // XOR mask 0xFF (flips every bit). Preserves the magic header.
    corruption::corrupt_interior_page_byte(&path, 0, 100, 0xFF);

    let sink = Arc::new(CapturingSubscriber::default());
    let err = Engine::open_with_subscriber_for_test(&path, sink.clone())
        .expect_err("corrupted interior page must fail open");

    assert!(matches!(err, EngineOpenError::Corruption(_)));

    let captured = sink.events.lock().unwrap();
    assert!(
        captured.iter().any(|e| {
            e.source == EventSource::SqliteInternal
                && e.category == EventCategory::Corruption
                && e.code == Some("SQLITE_CORRUPT")
        }),
        "expected (SqliteInternal, Corruption, SQLITE_CORRUPT) event, saw: {:?}",
        captured.iter().map(|e| (e.source, e.category, e.code)).collect::<Vec<_>>(),
    );
}

// AC-007a: Slow-statement event when wall-clock crosses default threshold.
//
// Measurement: default threshold = 100 ms (REQ-006a). The
// deterministic-slow-cte fixture (≥ 200 ms guaranteed by recursive-CTE
// counter) emits exactly one slow-statement signal identifying the SQL.
#[test]
fn ac_007a_slow_statement_event_at_default_threshold() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    engine.execute_for_test(SLOW_CTE).expect("slow cte");

    let signals = sink.slow_statements.lock().unwrap();
    assert_eq!(
        signals.len(),
        1,
        "expected exactly one slow-statement signal at default threshold, saw {}",
        signals.len(),
    );
    assert!(
        signals[0].statement.contains("RECURSIVE"),
        "slow signal must identify the statement; got: {:?}",
        signals[0].statement,
    );
    assert!(
        signals[0].wall_clock_ms >= 100,
        "slow signal wall_clock_ms must be ≥ 100 ms (default threshold); got {} ms",
        signals[0].wall_clock_ms,
    );
}

// AC-007b: Slow threshold reconfigurable at runtime.
//
// At threshold = 500 ms, a sub-threshold statement emits no slow-statement
// signal and a super-threshold statement emits exactly one. The CTE sizes are
// CALIBRATED to this host at runtime (`calibrate_cte_n`) rather than pinned to
// fixed N, so the test holds across hardware speeds (a fixed N=1_000_000 ran
// ~800 ms on the aarch64 probe but only ~144 ms on a fast x86_64 box, which
// silently dropped the "slow" fixture under the 500 ms threshold).
#[test]
fn ac_007b_slow_threshold_reconfigurable() {
    const THRESHOLD_MS: u64 = 500;
    let (_dir, engine) = fixture();
    // Calibrate BEFORE subscribing so the probe runs are not captured.
    let fast_n = calibrate_cte_n(&engine, THRESHOLD_MS / 5); // ~100 ms, well under
    let slow_n = calibrate_cte_n(&engine, THRESHOLD_MS * 3); // ~1500 ms, well over

    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());
    engine.set_slow_threshold_ms(THRESHOLD_MS).expect("set threshold");

    engine.execute_for_test(&cte_sql(fast_n)).expect("fast cte");
    assert_eq!(
        sink.slow_statements.lock().unwrap().len(),
        0,
        "sub-threshold statement (calibrated ~{} ms) must not emit a slow-statement \
         signal at threshold={THRESHOLD_MS} ms",
        THRESHOLD_MS / 5,
    );

    engine.execute_for_test(&cte_sql(slow_n)).expect("slow cte");
    let signals = sink.slow_statements.lock().unwrap();
    assert_eq!(
        signals.len(),
        1,
        "super-threshold statement (calibrated ~{} ms) must emit exactly one \
         slow-statement signal at threshold={THRESHOLD_MS} ms",
        THRESHOLD_MS * 3,
    );
    assert!(signals[0].wall_clock_ms >= THRESHOLD_MS);
}

/// Recursive-CTE counter SQL whose runtime scales linearly with `n`.
fn cte_sql(n: u64) -> String {
    format!(
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < {n}) \
         SELECT count(*) FROM c"
    )
}

/// Measure this host's recursive-CTE cost and return an `n` whose CTE runs for
/// approximately `target_ms`. Makes timing-threshold tests hardware-independent
/// instead of pinning fixed iteration counts to one CI runner's speed. Runs a
/// probe twice and uses the faster sample to damp transient load. The returned
/// `n` is clamped to a sane range.
fn calibrate_cte_n(engine: &Engine, target_ms: u64) -> u64 {
    const PROBE_N: u64 = 1_000_000;
    let probe = cte_sql(PROBE_N);
    let mut best = Duration::from_secs(3600);
    for _ in 0..2 {
        let start = Instant::now();
        engine.execute_for_test(&probe).expect("calibration probe cte");
        best = best.min(start.elapsed());
    }
    let per_probe_ms = (best.as_secs_f64() * 1000.0).max(0.1);
    let n = (PROBE_N as f64 * (target_ms as f64) / per_probe_ms) as u64;
    n.clamp(10_000, 500_000_000)
}

// Deterministic recursive-CTE fixture. `SLOW_CTE` (N=1_000_000) is used where a
// statement need only clear a low threshold (AC-007a default 100 ms; AC-008).
// AC-007b instead CALIBRATES its CTE sizes to the host at runtime
// (`cte_sql` + `calibrate_cte_n`) because its 500 ms threshold is sensitive to
// host speed — the fixed N=1_000_000 ran ~800 ms on the original aarch64 probe
// but only ~144 ms on a fast x86_64 box, silently falling under 500 ms.
const SLOW_CTE: &str = "WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL \
                        SELECT x + 1 FROM c WHERE x < 1000000) \
                        SELECT count(*) FROM c";

// AC-008: Slow signal participates in lifecycle attribution.
//
// Measurement: per `dev/design/lifecycle.md` § Slow and heartbeat
// policy, crossing the threshold produces TWO correlated facts —
// (i) a statement-level slow-statement signal, (ii) ≥ 1 lifecycle
// `phase == Slow` event during the operation's wall-clock window. The
// slow CTE fixture from AC-007a satisfies both.
#[test]
fn ac_008_slow_signal_feeds_lifecycle() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    engine.execute_for_test(SLOW_CTE).expect("slow cte");

    let signals = sink.slow_statements.lock().unwrap();
    assert!(!signals.is_empty(), "expected at least one slow-statement signal");

    let events = sink.events.lock().unwrap();
    assert!(
        events.iter().any(|e| e.phase == Phase::Slow),
        "expected at least one lifecycle event with phase == Slow"
    );
}

// AC-009: Stress-failure event field schema bound to a deterministic
// one-thread-poison fixture.
//
// The fixture spawns N reader threads and one writer thread that all
// make forward progress, plus a single designated poison thread that
// runs an op chosen to fail deterministically (an empty-batch write,
// which is rejected with `EngineError::WriteValidation`). The resulting
// `StressFailureContext` must have all four fields populated end-to-end:
//
// - `thread_group_id` non-zero
// - `op_kind` is the documented op label ("write")
// - `last_error_chain` carries `EngineError::stable_code()` as its first
//   entry plus at least one causal-chain segment (the `Display` form of
//   the engine error)
// - `projection_state` is one of the `ProjectionStatus` variants
//   stringified
#[test]
fn ac_009_poison_thread_emits_full_stress_failure_context() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    engine.run_one_thread_poison_for_test().expect("poison fixture should run");

    let captured = sink.stress_failures.lock().unwrap();
    let ctx = captured.first().expect("one-thread poison must emit a stress failure context");

    assert_ne!(ctx.thread_group_id, 0, "thread_group_id must be non-zero");
    assert_eq!(ctx.op_kind, "write", "op_kind must be the documented op label");
    assert!(
        ctx.last_error_chain.len() >= 2,
        "last_error_chain must include stable_code + ≥ 1 causal-chain segment, got {:?}",
        ctx.last_error_chain,
    );
    assert_eq!(
        ctx.last_error_chain[0], "WriteValidationError",
        "first chain entry must equal EngineError::stable_code"
    );
    assert!(
        ctx.last_error_chain.iter().skip(1).any(|s| !s.is_empty()),
        "at least one causal-chain segment after stable_code must be non-empty"
    );
    assert!(
        matches!(ctx.projection_state.as_str(), "Pending" | "Failed" | "UpToDate"),
        "projection_state must be a stringified ProjectionStatus variant, got {:?}",
        ctx.projection_state,
    );
}

// AC-009 supporting: Pure-type construction of StressFailureContext.
#[test]
fn ac_009_stress_failure_context_constructs() {
    let (_dir, engine) = fixture();
    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    engine.run_one_thread_poison_for_test().expect("poison fixture should emit");

    let captured = sink.stress_failures.lock().unwrap();
    let ctx = captured.first().expect("one-thread poison must emit a stress failure context");
    let _: u64 = ctx.thread_group_id;
    let _: String = ctx.op_kind.clone();
    let _: Vec<String> = ctx.last_error_chain.clone();
    let _: String = ctx.projection_state.clone();
    assert!(!ctx.op_kind.is_empty());
    assert!(!ctx.last_error_chain.is_empty());
    assert!(!ctx.projection_state.is_empty());
}

// AC-010: Projection-status enum coverage.
#[test]
fn ac_010_projection_status_enum_three_values() {
    let variants =
        [ProjectionStatus::Pending, ProjectionStatus::Failed, ProjectionStatus::UpToDate];
    for status in variants {
        match status {
            ProjectionStatus::Pending | ProjectionStatus::Failed | ProjectionStatus::UpToDate => {}
        }
    }
    assert_ne!(ProjectionStatus::Pending, ProjectionStatus::Failed);
    assert_ne!(ProjectionStatus::Pending, ProjectionStatus::UpToDate);
    assert_ne!(ProjectionStatus::Failed, ProjectionStatus::UpToDate);
}

// Compile-level: CounterSnapshot Default produces zeroed snapshot.
#[test]
fn counter_snapshot_default_is_zero() {
    let s = CounterSnapshot::default();
    assert_eq!(s.queries, 0);
    assert_eq!(s.writes, 0);
    assert_eq!(s.write_rows, 0);
    assert_eq!(s.admin_ops, 0);
    assert_eq!(s.cache_hit, 0);
    assert_eq!(s.cache_miss, 0);
    assert!(s.errors_by_code.is_empty());
}
