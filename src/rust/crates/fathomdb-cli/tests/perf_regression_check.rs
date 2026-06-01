//! 0.7.2 PR-7 — `perf-regression-check` bin behaviour tests.
//!
//! TDD surface for the perf-regression detector (see
//! `dev/design/perf-regression-detection.md`). Each test drives the bin
//! against a JSON-per-run fixture directory and asserts the exit code + the
//! flag verdict.
//!
//! Fixture directories (`tests/fixtures/`):
//! - `regression/`      — known degradations; MUST flag (exit 1).
//! - `jitter/`          — normal noise within thresholds; MUST NOT flag (exit 0).
//! - `backfill-dirty/`  — real history ending at the 2026-05-27 batch-collapse
//!   bug (`035cfa3`); MUST flag (exit 1).
//! - `backfill-clean/`  — clean history (excluding the bug); MUST NOT flag.
//! - `insufficient/`    — one row in a group (no median possible); graceful, no flag.
//! - `malformed/`       — a record violating the schema; MUST exit 2.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_perf-regression-check"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// Repo-root-relative `dev/perf-history/` (the committed, append-only store).
/// CARGO_MANIFEST_DIR is `src/rust/crates/fathomdb-cli`; walk up to repo root.
fn committed_perf_history() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..").join("dev/perf-history")
}

fn run_on(dir: PathBuf, extra: &[&str]) -> std::process::Output {
    let mut cmd = bin();
    cmd.arg(dir);
    cmd.args(extra);
    cmd.output().expect("spawn perf-regression-check")
}

fn code(out: &std::process::Output) -> i32 {
    out.status.code().expect("process exited via code, not signal")
}

// 1. Synthetic-regression fixture: known degradations -> flag, exit 1.
#[test]
fn regression_fixture_flags_and_exits_1() {
    let out = run_on(fixture("regression"), &[]);
    assert_eq!(
        code(&out),
        1,
        "regression fixture must exit 1 (stderr={})",
        String::from_utf8_lossy(&out.stderr)
    );
}

// 2. Synthetic-jitter fixture: normal noise -> NO flag, exit 0. (false-positive proof)
#[test]
fn jitter_fixture_does_not_flag_and_exits_0() {
    let out = run_on(fixture("jitter"), &[]);
    assert_eq!(
        code(&out),
        0,
        "jitter fixture must NOT flag (exit 0); stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// 3. Backfill flag-on-known-bug: history ending at 035cfa3 collapse -> flag, exit 1.
#[test]
fn backfill_dirty_flags_batch_collapse_bug() {
    let out = run_on(fixture("backfill-dirty"), &[]);
    assert_eq!(code(&out), 1, "history ending at the batch-collapse bug must flag (exit 1)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("AC-013b") && stdout.contains("035cfa3"),
        "flag output should name the AC and the buggy commit; got: {stdout}"
    );
}

// 4. No false positives on clean backfill: -> NO flag, exit 0.
#[test]
fn backfill_clean_does_not_flag() {
    let out = run_on(fixture("backfill-clean"), &[]);
    assert_eq!(
        code(&out),
        0,
        "clean backfill must NOT flag (exit 0); stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// 5. Insufficient-history: a group with too few points -> graceful, no flag, no crash.
#[test]
fn insufficient_history_is_graceful() {
    let out = run_on(fixture("insufficient"), &[]);
    assert_eq!(code(&out), 0, "insufficient-history must be graceful (exit 0, no flag)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.to_lowercase().contains("insufficient"),
        "insufficient-history should be noted distinctly; got: {stdout}"
    );
}

// 6. Malformed data -> exit 2.
#[test]
fn malformed_data_exits_2() {
    let out = run_on(fixture("malformed"), &[]);
    assert_eq!(code(&out), 2, "malformed record must exit 2 (data integrity)");
}

// Missing directory -> exit 2 (data missing).
#[test]
fn missing_directory_exits_2() {
    let out = run_on(fixture("does-not-exist"), &[]);
    assert_eq!(code(&out), 2, "missing directory must exit 2");
}

// --json emits machine-parseable JSON with an overall `flagged` boolean.
#[test]
fn json_output_is_parseable() {
    let out = run_on(fixture("jitter"), &["--json"]);
    assert_eq!(code(&out), 0);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("--json must emit valid JSON");
    assert_eq!(v["flagged"], serde_json::Value::Bool(false), "jitter -> flagged=false");
}

#[test]
fn json_output_marks_regression_flagged_true() {
    let out = run_on(fixture("regression"), &["--json"]);
    assert_eq!(code(&out), 1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("--json must emit valid JSON");
    assert_eq!(v["flagged"], serde_json::Value::Bool(true), "regression -> flagged=true");
}

// The committed, append-only store MUST be clean (no false positive) at HEAD.
// The batch-collapse arc lives in it, but the latest run per group is healthy
// (the recovery / forward baseline), so the detector must NOT flag it.
#[test]
fn committed_perf_history_does_not_flag() {
    let dir = committed_perf_history();
    assert!(dir.is_dir(), "committed dev/perf-history/ must exist at {dir:?}");
    let out = run_on(dir, &[]);
    assert_eq!(
        code(&out),
        0,
        "committed perf-history must be clean (exit 0); stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
}
