//! Power-cut + OS-crash durability harnesses (AC-034a / AC-034b / AC-034c).
//!
//! Both bodies are gated on `AGENT_LONG=1` per `tests/perf_gates.rs`
//! precedent: `agent-verify` skips the long run, the dedicated soak
//! pipeline opts in. The harness is NOT a soak (open-ended) — it runs
//! `P-PWR-TRIALS = 100` (AC-034a/b) or `P-OS-TRIALS = 50` (AC-034c)
//! deterministic trials and asserts the bounds in `dev/acceptance.md`.
//!
//! Substrate notes:
//!   - Power-cut victim is the test binary itself, re-entered through
//!     `Command::new(current_exe())` with an env-var sentinel that
//!     routes execution to `_power_cut_victim_entry` (test-binary-as-
//!     victim trick). No dedicated `cargo-bin` victim is required.
//!   - OS-crash (AC-034c) requires a VM image + sysrq trigger per
//!     `dev/acceptance.md` § AC-034c fixture; that substrate does not
//!     exist in this repo, so the AC-034c body surfaces as a runtime
//!     blocker and the trial loop returns early. See
//!     `dev/plans/runs/12-D-durability-harnesses-output.json`.

// 0.8.9 Slice 20 (F-9): this harness is inherently POSIX — it simulates a power
// cut by re-exec'ing the test binary as a victim and `libc::kill(pid, SIGKILL)`-ing
// it; there is no Windows equivalent (and the bodies are `AGENT_LONG`-gated, so they
// never run in per-push CI regardless). The unguarded `libc::kill`/`SIGKILL` used to
// compile only because the pyo3 link error aborted `cargo test --workspace` before
// this target built; once that gate was fixed the missing Unix symbols surfaced as a
// hard Windows compile error. Gate the whole target on `unix` so it compiles to an
// empty test binary on Windows (0 tests) while staying byte-identical on Unix.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fathomdb_engine::{Engine, PreparedWrite};
use tempfile::TempDir;

fn long_run_enabled() -> bool {
    std::env::var_os("AGENT_LONG").is_some()
}

const P_PWR_TRIALS: usize = 100;
const AC_034B_P99_BUDGET_MS: u128 = 100;
// Per-trial wait between victim startup and SIGKILL. Drawn uniformly
// from [PWR_MIN_KILL_MS, PWR_MAX_KILL_MS) so kills land at varied
// commit-cycle phases per AC-034a/b fixture spec.
const PWR_MIN_KILL_MS: u64 = 30;
const PWR_MAX_KILL_MS: u64 = 180;
// Time budget for the victim child to die after SIGKILL before we
// declare the trial degenerate. Generous; SIGKILL is immediate.
const VICTIM_REAP_BUDGET: Duration = Duration::from_secs(5);
// Per-trial wait for the victim to land its first commit (signaled
// via sentinel file). Without this fence the parent can SIGKILL
// before any row reaches disk, producing degenerate "no surviving
// commit" trials that AC-034b's full-N p99 contract forbids.
const SENTINEL_WAIT_BUDGET: Duration = Duration::from_secs(5);

// ── Victim entry-point ──────────────────────────────────────────────────────

/// Test-binary-as-victim entry. Re-invoked from
/// `Command::new(current_exe()) --exact --ignored _power_cut_victim_entry`
/// with `FATHOMDB_POWER_CUT_VICTIM_DB` set. Without the env var the test
/// is a no-op so plain `cargo test --test durability_soak -- --ignored`
/// (no env var) is still safe.
#[test]
#[ignore = "test-binary-as-victim entry-point for the AC-034a/b power-cut harness"]
fn _power_cut_victim_entry() {
    let Some(db_path_os) = std::env::var_os("FATHOMDB_POWER_CUT_VICTIM_DB") else {
        return;
    };
    let path = PathBuf::from(db_path_os);
    let sentinel = std::env::var_os("FATHOMDB_POWER_CUT_VICTIM_SENTINEL").map(PathBuf::from);
    let opened = Engine::open(&path).expect("victim engine open");
    // Commit single-row writes carrying the wall-clock timestamp of the
    // commit in the body field. The parent recovers the maximum body
    // value after kill — that is the last-surviving-commit timestamp
    // per AC-034b measurement protocol.
    let mut first = true;
    loop {
        let body = micros_since_epoch().to_string();
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body,
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            }])
            .expect("victim write");
        if first {
            // Fence: the parent waits for this sentinel before sampling
            // the kill delay, so every trial yields at least one
            // surviving commit and AC-034b's p99 is computed across the
            // full P-PWR-TRIALS set (not a filtered subset).
            if let Some(path) = sentinel.as_ref() {
                std::fs::File::create(path).expect("victim sentinel create");
            }
            first = false;
        }
    }
}

fn micros_since_epoch() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_micros()
}

// ── AC-034a + AC-034b: power-cut harness ────────────────────────────────────

#[test]
fn ac_034a_and_b_power_cut_zero_corruption_and_p99_lost_commit() {
    if !long_run_enabled() {
        eprintln!("AC-034a/b skipped (AGENT_LONG not set)");
        return;
    }
    let outcomes = run_power_cut_trials(P_PWR_TRIALS);

    // Full-N contract: every trial must have yielded a measurable
    // lost-commit sample, otherwise the p99 below is not actually
    // "across P-PWR-TRIALS" and AC-034b is being measured on a
    // filtered subset (the original review finding).
    assert_eq!(
        outcomes.len(),
        P_PWR_TRIALS,
        "AC-034b: harness collected {} trials, expected {}",
        outcomes.len(),
        P_PWR_TRIALS,
    );

    // AC-034a: integrity_check == "ok" on every trial.
    let bad: Vec<_> = outcomes.iter().filter(|o| o.integrity != "ok").collect();
    assert!(
        bad.is_empty(),
        "AC-034a: {} of {} trials returned non-ok integrity_check: {:?}",
        bad.len(),
        outcomes.len(),
        bad,
    );

    // AC-034b: p99 lost-commit duration ≤ 100 ms, computed across the
    // full set (sentinel-wait guarantees every trial committed at
    // least once before SIGKILL).
    let mut lost: Vec<u128> = outcomes.iter().map(|o| o.lost_commit_ms).collect();
    lost.sort_unstable();
    let p99_index = ((lost.len() as f64 * 0.99).ceil() as usize).saturating_sub(1);
    let p99 = lost[p99_index];
    eprintln!("AC-034a: integrity_check ok on {}/{} trials", outcomes.len(), outcomes.len());
    eprintln!(
        "AC-034b: lost-commit ms — n={}, min={}, median={}, p99={}",
        lost.len(),
        lost[0],
        lost[lost.len() / 2],
        p99,
    );
    assert!(
        p99 <= AC_034B_P99_BUDGET_MS,
        "AC-034b: lost-commit p99 = {} ms > {} ms budget",
        p99,
        AC_034B_P99_BUDGET_MS,
    );
}

#[derive(Debug)]
struct TrialOutcome {
    integrity: String,
    lost_commit_ms: u128,
}

fn run_power_cut_trials(trials: usize) -> Vec<TrialOutcome> {
    let exe = std::env::current_exe().expect("current_exe");
    let mut outcomes = Vec::with_capacity(trials);
    for trial in 0..trials {
        let dir = TempDir::new().expect("tempdir");
        let db_path = dir.path().join("power-cut.sqlite");
        let sentinel_path = dir.path().join("first-commit.sentinel");

        let mut child = Command::new(&exe)
            .args(["--exact", "--ignored", "_power_cut_victim_entry"])
            .env("FATHOMDB_POWER_CUT_VICTIM_DB", &db_path)
            .env("FATHOMDB_POWER_CUT_VICTIM_SENTINEL", &sentinel_path)
            // Per-trial stdio drop; the parent does not parse victim
            // output, and leaving stdout connected to the parent test
            // would interleave with `cargo test` framing.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn victim");
        let pid = child.id() as i32;

        // Fence on the victim's first-commit sentinel. The full-N p99
        // contract requires every trial to record a real lost-commit
        // sample; sampling the kill delay before this fence would let
        // SIGKILL race the open path on slow hosts and bias the
        // distribution toward "no surviving commit" outliers.
        wait_for_sentinel(&sentinel_path, SENTINEL_WAIT_BUDGET, pid, trial);

        // Allow the child to keep writing past the first commit so the
        // kill point sweeps the commit cycle deterministically.
        let sleep_ms = trial_sleep_ms(trial);
        std::thread::sleep(Duration::from_millis(sleep_ms));

        let kill_micros = micros_since_epoch();
        // SAFETY: `pid` is owned by `child` and we have not yet
        // wait()-ed for it. SIGKILL on a still-running PID is defined.
        let kill_rc = unsafe { libc::kill(pid, libc::SIGKILL) };
        if kill_rc != 0 {
            panic!("trial {trial}: SIGKILL failed: {}", std::io::Error::last_os_error());
        }

        // Reap the child. SIGKILL is immediate, but `wait` is the only
        // way to release the zombie + confirm the process is gone
        // before we touch the WAL file from the parent.
        wait_with_budget(&mut child, VICTIM_REAP_BUDGET, trial);

        let last_commit_micros = read_last_commit_micros(&db_path).unwrap_or_else(|| {
            panic!(
                "trial {trial}: sentinel landed but no committed row \
                 recovered after SIGKILL — open path lost a durably \
                 committed write"
            )
        });
        let integrity = run_integrity_check(&db_path);
        // `kill_micros >= last_commit_micros` holds by construction
        // (sentinel proves at least one commit landed before kill,
        // and kill_micros was sampled after the post-sentinel sleep).
        // Saturating sub guards against clock jumps under load.
        let lost_commit_ms = kill_micros.saturating_sub(last_commit_micros) / 1_000;
        outcomes.push(TrialOutcome { integrity, lost_commit_ms });
    }
    outcomes
}

fn wait_for_sentinel(sentinel: &std::path::Path, budget: Duration, pid: i32, trial: usize) {
    let started = Instant::now();
    while !sentinel.exists() {
        if started.elapsed() > budget {
            // Don't leave the victim around as a runaway writer if the
            // harness gives up on it.
            let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!(
                "trial {trial}: victim did not land its first commit \
                 within {budget:?}; AC-034b full-N p99 contract requires \
                 every trial to commit at least once before SIGKILL"
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn trial_sleep_ms(trial: usize) -> u64 {
    // Deterministic linear-congruential sequence keyed off the trial
    // index; trials remain reproducible across runs while still
    // sweeping the kill point across the commit cycle.
    let mixed = (trial as u64)
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    let range = PWR_MAX_KILL_MS - PWR_MIN_KILL_MS;
    PWR_MIN_KILL_MS + (mixed % range)
}

fn wait_with_budget(child: &mut std::process::Child, budget: Duration, trial: usize) {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                if started.elapsed() > budget {
                    panic!("trial {trial}: victim did not exit within {:?} after SIGKILL", budget);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(err) => panic!("trial {trial}: wait failed: {err}"),
        }
    }
}

fn read_last_commit_micros(db_path: &std::path::Path) -> Option<u128> {
    // Read via the engine to confirm the engine itself can recover the
    // killed database; raw rusqlite on a half-recovered DB might paper
    // over an open-path defect.
    let opened = Engine::open(db_path).expect("post-kill engine open");
    let path = opened.engine.path().to_path_buf();
    opened.engine.close().expect("post-kill close");
    drop(opened);

    let conn = rusqlite::Connection::open(&path).expect("post-kill rusqlite open");
    // Bodies are decimal microsecond timestamps; ordering by CAST keeps
    // semantics correct if the column is TEXT-affinity.
    let max: Option<String> = conn
        .query_row(
            "SELECT body FROM canonical_nodes
              WHERE kind = 'doc'
              ORDER BY CAST(body AS INTEGER) DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();
    max.and_then(|raw| raw.parse::<u128>().ok())
}

fn run_integrity_check(db_path: &std::path::Path) -> String {
    let conn = rusqlite::Connection::open(db_path).expect("integrity rusqlite open");
    conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .unwrap_or_else(|err| format!("integrity_check query failed: {err}"))
}

// ── AC-034c: OS-crash harness ───────────────────────────────────────────────

#[test]
#[ignore = "AC-034c blocked on missing VM/sysrq fixture; see Phase 12-D \
            output JSON (blocker-3). Do NOT clear the #[ignore] until \
            the VM image lands in dev/test-plan.md."]
fn ac_034c_os_crash_zero_committed_tx_loss() {
    // Loud-fail body: clearing `#[ignore]` without landing the VM
    // substrate now panics instead of green-passing vacuously.
    // Substituting `kill -9` for an OS crash would be silent AC
    // weakening (per `feedback_reliability_principles.md` no-punt
    // rule).
    panic!(
        "AC-034c requires a KVM image with `echo c > /proc/sysrq-trigger` \
         and a preserved disk sync barrier (per `dev/acceptance.md` § \
         AC-034c fixture). That VM substrate does not exist in this repo. \
         See `dev/plans/runs/12-D-durability-harnesses-output.json` \
         blocker-3 for the substrate-gap detail and the recommended \
         12-D-OS-CRASH follow-up slice."
    );
}
