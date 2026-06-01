//! 0.7.2 PR-7 — perf regression detector.
//!
//! Reads the append-only `dev/perf-history/` store (one JSON file per
//! canonical / release-time run), groups records by `(ac_id, n)`, and flags
//! the single most-recent run in each group against the rolling median of
//! the prior runs in that group. Tier-aware: `n` is part of the grouping key
//! so a 10k-tier run is never compared against a 100k/1M-tier run, and the
//! real-corpus (N=7667) group is distinct from the synthetic N=10000 group.
//!
//! Design + thresholds + provenance:
//! `dev/design/perf-regression-detection.md`.
//!
//! ## Append-only invariant
//!
//! This binary **only reads** the history directory — it never writes,
//! mutates, or deletes any file. CI (or a human, for the locally-measured
//! AC-013/AC-013b/AC-019 numbers) writes new files. A regression is a new
//! file whose numbers this bin flags, not an edit to an old file.
//!
//! ## Record schema (one JSON object per file)
//!
//! ```json
//! {
//!   "commit_sha": "c893d8b0",
//!   "ac_id": "AC-013",
//!   "n": 10000,
//!   "p50_ms": 36.0,
//!   "p99_ms": 49.0,
//!   "recall": 0.937,
//!   "timestamp": "2026-06-01T07:23:39Z"
//! }
//! ```
//!
//! - `commit_sha`, `ac_id`, `n`, `timestamp` are required.
//! - `p50_ms` / `p99_ms` are omitted for recall-only ACs (AC-013b).
//! - `recall` is omitted/null for latency-only ACs (AC-012, AC-019).
//! - A record MUST carry at least one comparable metric (a latency pair or a
//!   recall value), else it is malformed (exit 2).
//! - **Unknown fields are ignored, not rejected** (forward-extensible).
//!
//! ## Exit codes
//!
//! - `0` = no regression flagged (includes all-insufficient-history).
//! - `1` = at least one group flagged a regression.
//! - `2` = data missing / malformed (directory absent, unreadable/invalid
//!   JSON, record with no comparable metric).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

// ===========================================================================
// Detection thresholds — HITL Gate 1, LOCKED 2026-06-01.
//
// These are the ONLY tuning knobs for the detector. Keep them here as named,
// commented constants — do NOT scatter numeric literals through the logic.
// Rationale (full text in dev/design/perf-regression-detection.md):
//   * Recall sigma = 0.0116 (PR-3 measured, N=7667 anchor). A 0.03 absolute
//     recall threshold is ~2.4 sigma (~1% jitter false-flag rate) — the
//     deliberate low-cry-wolf choice; codex's review focus is false-positive
//     resistance. Tradeoff: a 0.03 drop from 0.937 lands at 0.907, near the
//     0.90 floor, so this favours false-positive resistance over early-
//     warning sensitivity (the hard floor is enforced by the AC-013b gate,
//     not by this drift detector).
//   * 15% latency absorbs normal run-to-run variance on the shared 4-core
//     canonical CI runner (cold-cache + neighbour noise swing latency more
//     than recall).
//   * Window = the prior up-to-10 runs per (ac_id, n) group; "latest" is the
//     single most-recent run by timestamp, compared against the median of the
//     prior runs in the window.
// ===========================================================================

/// Flag a latency metric if the latest run exceeds the rolling median by more
/// than this fraction (15%).
const LATENCY_DEGRADATION_FRACTION: f64 = 0.15;

/// Flag recall if the latest run is below the rolling median by more than this
/// absolute amount (0.03).
const RECALL_DEGRADATION_ABSOLUTE: f64 = 0.03;

/// Rolling-median window: the most-recent run is compared against the median
/// of up to this many prior runs in its group.
const ROLLING_WINDOW: usize = 10;

/// A group needs at least this many runs to evaluate (1 latest + >=1 prior).
const MIN_RUNS_FOR_MEDIAN: usize = 2;

// ===========================================================================

#[derive(Debug, Deserialize)]
struct RawRecord {
    commit_sha: String,
    ac_id: String,
    n: u64,
    #[serde(default)]
    p50_ms: Option<f64>,
    #[serde(default)]
    p99_ms: Option<f64>,
    #[serde(default)]
    recall: Option<f64>,
    timestamp: String,
    // Forward-extensible: any unknown field is ignored, not rejected
    // (serde's deny_unknown_fields is intentionally OFF).
}

#[derive(Debug, Clone)]
struct Record {
    commit_sha: String,
    ac_id: String,
    n: u64,
    p50_ms: Option<f64>,
    p99_ms: Option<f64>,
    recall: Option<f64>,
    timestamp: String,
}

/// A loaded record paired with the file it came from (for error messages).
fn load_record(path: &Path) -> Result<Record, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: cannot read: {e}", path.display()))?;
    let raw: RawRecord = serde_json::from_str(&text)
        .map_err(|e| format!("{}: invalid JSON / schema: {e}", path.display()))?;
    // A record must carry at least one comparable metric.
    if raw.p50_ms.is_none() && raw.p99_ms.is_none() && raw.recall.is_none() {
        return Err(format!(
            "{}: record has no comparable metric (need p50_ms/p99_ms or recall)",
            path.display()
        ));
    }
    Ok(Record {
        commit_sha: raw.commit_sha,
        ac_id: raw.ac_id,
        n: raw.n,
        p50_ms: raw.p50_ms,
        p99_ms: raw.p99_ms,
        recall: raw.recall,
        timestamp: raw.timestamp,
    })
}

/// Median of a slice (sorted copy; mean of the two middle elements for an even
/// count). Returns `None` for an empty slice.
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.len() % 2 == 1 {
        Some(v[mid])
    } else {
        Some((v[mid - 1] + v[mid]) / 2.0)
    }
}

#[derive(Debug, serde::Serialize)]
struct GroupVerdict {
    ac_id: String,
    n: u64,
    latest_commit: String,
    latest_timestamp: String,
    runs: usize,
    flagged: bool,
    /// "ok" | "insufficient-history" | "regression"
    status: String,
    reasons: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct Report {
    flagged: bool,
    groups: Vec<GroupVerdict>,
}

/// Evaluate one group's runs (already sorted oldest -> newest by timestamp).
fn evaluate_group(ac_id: &str, n: u64, runs: &[Record]) -> GroupVerdict {
    let latest = runs.last().expect("group is never empty");
    let mut verdict = GroupVerdict {
        ac_id: ac_id.to_string(),
        n,
        latest_commit: latest.commit_sha.clone(),
        latest_timestamp: latest.timestamp.clone(),
        runs: runs.len(),
        flagged: false,
        status: "ok".to_string(),
        reasons: Vec::new(),
    };

    if runs.len() < MIN_RUNS_FOR_MEDIAN {
        verdict.status = "insufficient-history".to_string();
        verdict.reasons.push(format!(
            "only {} run(s) in group; need >= {} to form a median",
            runs.len(),
            MIN_RUNS_FOR_MEDIAN
        ));
        return verdict;
    }

    // Prior runs = the up-to-ROLLING_WINDOW runs immediately before the latest.
    let prior = &runs[..runs.len() - 1];
    let window_start = prior.len().saturating_sub(ROLLING_WINDOW);
    let window = &prior[window_start..];

    // Latency: flag if p50 OR p99 degraded > LATENCY_DEGRADATION_FRACTION.
    for (label, get) in [
        ("p50_ms", &Record::p50_ms as &dyn Fn(&Record) -> Option<f64>),
        ("p99_ms", &Record::p99_ms as &dyn Fn(&Record) -> Option<f64>),
    ] {
        let prior_vals: Vec<f64> = window.iter().filter_map(get).collect();
        if let (Some(latest_v), Some(med)) = (get(latest), median(&prior_vals)) {
            if med > 0.0 {
                let frac = (latest_v - med) / med;
                if frac > LATENCY_DEGRADATION_FRACTION {
                    verdict.flagged = true;
                    verdict.reasons.push(format!(
                        "{label} {latest_v:.2} ms is {:.1}% over rolling median {med:.2} ms (threshold {:.0}%) at commit {}",
                        frac * 100.0,
                        LATENCY_DEGRADATION_FRACTION * 100.0,
                        latest.commit_sha,
                    ));
                }
            }
        }
    }

    // Recall: flag if degraded > RECALL_DEGRADATION_ABSOLUTE (absolute drop).
    let prior_recall: Vec<f64> = window.iter().filter_map(|r| r.recall).collect();
    if let (Some(latest_r), Some(med)) = (latest.recall, median(&prior_recall)) {
        let drop = med - latest_r;
        if drop > RECALL_DEGRADATION_ABSOLUTE {
            verdict.flagged = true;
            verdict.reasons.push(format!(
                "recall {latest_r:.4} is {drop:.4} below rolling median {med:.4} (threshold {RECALL_DEGRADATION_ABSOLUTE}) at commit {}",
                latest.commit_sha,
            ));
        }
    }

    if verdict.flagged {
        verdict.status = "regression".to_string();
    }
    verdict
}

impl Record {
    fn p50_ms(&self) -> Option<f64> {
        self.p50_ms
    }
    fn p99_ms(&self) -> Option<f64> {
        self.p99_ms
    }
}

fn build_report(dir: &Path) -> Result<Report, String> {
    if !dir.is_dir() {
        return Err(format!("perf-history directory not found: {}", dir.display()));
    }

    // Group by (ac_id, n). BTreeMap gives deterministic, sorted output.
    let mut groups: BTreeMap<(String, u64), Vec<Record>> = BTreeMap::new();

    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let rec = load_record(&path)?;
        groups.entry((rec.ac_id.clone(), rec.n)).or_default().push(rec);
    }

    if groups.is_empty() {
        return Err(format!("no *.json perf records found in {}", dir.display()));
    }

    let mut report = Report { flagged: false, groups: Vec::new() };
    for ((ac_id, n), mut runs) in groups {
        // Sort oldest -> newest by timestamp (RFC3339 sorts lexically).
        runs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        let v = evaluate_group(&ac_id, n, &runs);
        report.flagged |= v.flagged;
        report.groups.push(v);
    }
    Ok(report)
}

fn print_human(report: &Report) {
    println!("perf-regression-check — {} group(s)\n", report.groups.len());
    for g in &report.groups {
        let marker = if g.flagged {
            "REGRESSION"
        } else if g.status == "insufficient-history" {
            "insufficient"
        } else {
            "ok"
        };
        println!(
            "[{marker}] {} @ n={} — {} run(s), latest {} ({})",
            g.ac_id, g.n, g.runs, g.latest_commit, g.latest_timestamp
        );
        for reason in &g.reasons {
            println!("    - {reason}");
        }
    }
    println!();
    if report.flagged {
        println!("RESULT: regression(s) flagged.");
    } else {
        println!("RESULT: no regressions.");
    }
}

fn print_usage() {
    eprintln!(
        "usage: perf-regression-check <perf-history-dir> [--json]\n\
         \n\
         Reads the append-only perf-history directory, groups runs by\n\
         (ac_id, n), and flags the latest run vs the rolling median of the\n\
         prior up-to-{ROLLING_WINDOW} runs.\n\
         Exit: 0 = clean, 1 = regression flagged, 2 = data missing/malformed."
    );
}

fn main() -> ExitCode {
    let mut dir: Option<PathBuf> = None;
    let mut json = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::from(0);
            }
            other if other.starts_with('-') => {
                eprintln!("unknown flag: {other}");
                print_usage();
                return ExitCode::from(2);
            }
            other => {
                if dir.is_some() {
                    eprintln!("unexpected extra argument: {other}");
                    print_usage();
                    return ExitCode::from(2);
                }
                dir = Some(PathBuf::from(other));
            }
        }
    }

    let dir = match dir {
        Some(d) => d,
        None => {
            print_usage();
            return ExitCode::from(2);
        }
    };

    match build_report(&dir) {
        Ok(report) => {
            if json {
                match serde_json::to_string_pretty(&report) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("failed to serialize report: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                print_human(&report);
            }
            if report.flagged {
                ExitCode::from(1)
            } else {
                ExitCode::from(0)
            }
        }
        Err(e) => {
            eprintln!("perf-regression-check: {e}");
            ExitCode::from(2)
        }
    }
}
