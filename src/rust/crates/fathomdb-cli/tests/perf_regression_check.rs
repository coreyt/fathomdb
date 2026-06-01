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
//! - `backfill-dirty/`  — the 2026-05-27 batch-collapse arc: a degenerate
//!   recall=1.0 (`035cfa3`, the bug masquerading as perfection) followed by the
//!   honest post-fix 0.1572 (`4a95cfd`). The detector flags the regression-
//!   shaped *correction* (the 0.84 recall drop) at the fix commit — see the
//!   honesty note in `dev/design/perf-regression-detection.md`. MUST flag (exit 1).
//! - `backfill-clean/`  — clean history (excluding the bug); MUST NOT flag.
//! - `insufficient/`    — one row in a group (no median possible); graceful, no flag.
//! - `malformed/`       — a record with a non-numeric metric; MUST exit 2.
//! - `malformed-timestamp/` — a record with an unparseable timestamp; MUST exit 2.

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

// 3. Batch-collapse arc: the bug (035cfa3) read a degenerate recall=1.0; the
//    fix (4a95cfd) revealed the honest 0.1572. A degradation detector cannot
//    flag the bug (it looked like an improvement) — it flags the regression-
//    shaped *correction* (the 0.84 recall drop) at the fix commit. exit 1.
#[test]
fn correction_after_batch_collapse_flags_as_recall_drop() {
    let out = run_on(fixture("backfill-dirty"), &[]);
    assert_eq!(code(&out), 1, "the recall correction after the batch-collapse must flag (exit 1)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("AC-013b") && stdout.contains("4a95cfd") && stdout.contains("recall"),
        "flag output should name the AC, the correction commit, and the recall drop; got: {stdout}"
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

// 6. Malformed data (non-numeric metric) -> exit 2.
#[test]
fn malformed_data_exits_2() {
    let out = run_on(fixture("malformed"), &[]);
    assert_eq!(code(&out), 2, "malformed record must exit 2 (data integrity)");
}

// 6b. Malformed timestamp -> exit 2 (must fail loudly, not sort lexically and
//     silently mis-select the latest run).
#[test]
fn malformed_timestamp_exits_2() {
    let out = run_on(fixture("malformed-timestamp"), &[]);
    assert_eq!(code(&out), 2, "unparseable timestamp must exit 2 (data integrity)");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("timestamp"),
        "exit-2 reason should name the timestamp; got: {stderr}"
    );
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

// The committed store carries the full batch-collapse arc in the AC-013b@10000
// group (degenerate 1.0 -> honest 0.1572 -> dense 0.5124 -> v0.7.0 0.5124). The
// latest run (v0.7.0) is healthy, so the group is clean — but the arc must
// actually be present (4 runs), not silently dropped. This exercises the arc
// rather than just asserting a blanket exit 0.
#[test]
fn committed_batch_collapse_arc_present_and_clean() {
    let out = run_on(committed_perf_history(), &["--json"]);
    assert_eq!(code(&out), 0, "committed store must be clean");
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).expect("valid --json");
    let group = v["groups"]
        .as_array()
        .expect("groups array")
        .iter()
        .find(|g| g["ac_id"] == "AC-013b" && g["n"] == 10000)
        .expect("AC-013b@10000 group must exist in the committed store");
    assert_eq!(group["runs"], 4, "the full bug->fix->dense->ship arc must be present");
    assert_eq!(group["flagged"], serde_json::Value::Bool(false), "latest (v0.7.0) is healthy");
    assert_eq!(group["status"], "ok");
}
