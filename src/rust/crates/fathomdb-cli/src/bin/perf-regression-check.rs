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
    /// UTC epoch seconds parsed from `timestamp`. This — not the raw string —
    /// is the ordering key, so a malformed/offset timestamp cannot silently
    /// reorder latest-run selection (a lexical sort would).
    ts_epoch: i64,
}

/// Parse a strict RFC3339 timestamp to UTC epoch seconds for ordering.
///
/// Accepts `YYYY-MM-DDTHH:MM:SS(.fraction)?(Z|±HH:MM)`. Any deviation returns
/// `Err`, so a malformed timestamp fails loudly (exit 2) instead of corrupting
/// "latest by timestamp" via a lexical string sort. Dependency-free (the
/// workspace pulls in no date crate); uses Howard Hinnant's days-from-civil
/// algorithm for the proleptic-Gregorian day count.
fn parse_rfc3339_epoch(s: &str) -> Result<i64, String> {
    let (date, rest) = s.split_once('T').ok_or_else(|| format!("missing 'T': {s}"))?;
    let d: Vec<&str> = date.split('-').collect();
    if d.len() != 3 {
        return Err(format!("bad date: {s}"));
    }
    let year: i64 = d[0].parse().map_err(|_| format!("bad year: {s}"))?;
    let month: i64 = d[1].parse().map_err(|_| format!("bad month: {s}"))?;
    let day: i64 = d[2].parse().map_err(|_| format!("bad day: {s}"))?;
    if !(1..=12).contains(&month) {
        return Err(format!("month out of range: {s}"));
    }
    // Month-specific day count with proleptic-Gregorian leap-year handling, so
    // impossible dates (2026-02-31, 2026-02-29 in a non-leap year) are rejected
    // rather than silently normalized by days_from_civil.
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if !(1..=days_in_month).contains(&day) {
        return Err(format!("day out of range for month: {s}"));
    }

    // Split the time-of-day from the zone designator (Z or ±HH:MM).
    let (time_part, offset_secs) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0i64)
    } else {
        let sign_idx =
            rest.rfind(['+', '-']).ok_or_else(|| format!("missing zone designator: {s}"))?;
        let (t, off) = rest.split_at(sign_idx);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let (oh, om) = off[1..].split_once(':').ok_or_else(|| format!("bad zone offset: {s}"))?;
        let oh: i64 = oh.parse().map_err(|_| format!("bad offset hour: {s}"))?;
        let om: i64 = om.parse().map_err(|_| format!("bad offset minute: {s}"))?;
        if oh > 23 || om > 59 {
            return Err(format!("zone offset out of range: {s}"));
        }
        (t, sign * (oh * 3600 + om * 60))
    };

    // HH:MM:SS with an optional fractional-second suffix. The fraction is not
    // needed for second-resolution ordering, but it must be well-formed
    // (non-empty, digits only) — a malformed suffix (e.g. "27.badZ") is a
    // strict-RFC3339 violation, not something to silently discard.
    let (time_core, frac) = match time_part.split_once('.') {
        Some((core, f)) => (core, Some(f)),
        None => (time_part, None),
    };
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("bad fractional seconds: {s}"));
        }
    }
    let tp: Vec<&str> = time_core.split(':').collect();
    if tp.len() != 3 {
        return Err(format!("bad time: {s}"));
    }
    let hh: i64 = tp[0].parse().map_err(|_| format!("bad hour: {s}"))?;
    let mi: i64 = tp[1].parse().map_err(|_| format!("bad minute: {s}"))?;
    let ss: i64 = tp[2].parse().map_err(|_| format!("bad second: {s}"))?;
    if hh > 23 || mi > 59 || ss > 60 {
        return Err(format!("time out of range: {s}"));
    }

    // days_from_civil (Howard Hinnant): days since 1970-01-01, proleptic Gregorian.
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Ok(days * 86400 + hh * 3600 + mi * 60 + ss - offset_secs)
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
    // Timestamp must be a parseable RFC3339 instant — a bad one is a data-
    // integrity failure (exit 2), not something to sort lexically and hope.
    let ts_epoch = parse_rfc3339_epoch(&raw.timestamp)
        .map_err(|e| format!("{}: malformed timestamp: {e}", path.display()))?;
    Ok(Record {
        commit_sha: raw.commit_sha,
        ac_id: raw.ac_id,
        n: raw.n,
        p50_ms: raw.p50_ms,
        p99_ms: raw.p99_ms,
        recall: raw.recall,
        timestamp: raw.timestamp,
        ts_epoch,
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
        // Sort oldest -> newest by parsed UTC epoch (not the raw string, which
        // would mis-order offset/Z-mixed timestamps). Tie-break on commit_sha
        // for a deterministic "latest" when two runs share an instant.
        runs.sort_by(|a, b| {
            a.ts_epoch.cmp(&b.ts_epoch).then_with(|| a.commit_sha.cmp(&b.commit_sha))
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_utc_epoch() {
        // 1970-01-01T00:00:00Z is epoch 0; one day later is 86400.
        assert_eq!(parse_rfc3339_epoch("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(parse_rfc3339_epoch("1970-01-02T00:00:00Z").unwrap(), 86_400);
    }

    #[test]
    fn rfc3339_offset_normalizes_to_utc() {
        // 16:54:27-05:00 == 21:54:27Z — the two must parse to the same instant.
        let z = parse_rfc3339_epoch("2026-05-27T21:54:27Z").unwrap();
        let off = parse_rfc3339_epoch("2026-05-27T16:54:27-05:00").unwrap();
        assert_eq!(z, off);
    }

    #[test]
    fn rfc3339_orders_the_batch_collapse_arc() {
        // The committed AC-013b@10000 chronology must sort bug -> fix -> ship.
        let bug = parse_rfc3339_epoch("2026-05-27T21:47:35Z").unwrap(); // 035cfa3
        let fix = parse_rfc3339_epoch("2026-05-27T21:54:27Z").unwrap(); // 4a95cfd
        let v070 = parse_rfc3339_epoch("2026-05-28T01:18:00Z").unwrap(); // 38d5f4f
        assert!(bug < fix && fix < v070);
    }

    #[test]
    fn rfc3339_accepts_valid_edge_cases() {
        // Leap day in a leap year, fractional seconds, and a leap second.
        assert!(parse_rfc3339_epoch("2024-02-29T00:00:00Z").is_ok());
        assert!(parse_rfc3339_epoch("2026-06-01T07:23:39.500Z").is_ok());
        assert!(parse_rfc3339_epoch("2026-06-30T23:59:60Z").is_ok());
        // Fractional seconds must not change the second-resolution instant.
        assert_eq!(
            parse_rfc3339_epoch("2026-06-01T07:23:39.999Z").unwrap(),
            parse_rfc3339_epoch("2026-06-01T07:23:39Z").unwrap()
        );
    }

    #[test]
    fn rfc3339_rejects_malformed() {
        for bad in [
            "not-a-date",
            "2026-05-27",                // missing time
            "2026-05-27 21:54:27Z",      // space instead of 'T'
            "2026-13-01T00:00:00Z",      // month out of range
            "2026-05-27T25:00:00Z",      // hour out of range
            "2026-05-27T21:54:27",       // no zone designator
            "2026-02-31T00:00:00Z",      // impossible day (Feb has <= 29)
            "2026-02-29T00:00:00Z",      // 2026 is not a leap year
            "2026-05-27T21:54:27+24:00", // offset hour out of range
            "2026-05-27T21:54:27-05:99", // offset minute out of range
            "2026-06-01T07:23:39.badZ",  // non-digit fractional seconds
            "2026-06-01T07:23:39.Z",     // empty fractional seconds
        ] {
            assert!(parse_rfc3339_epoch(bad).is_err(), "should reject: {bad}");
        }
    }
}
