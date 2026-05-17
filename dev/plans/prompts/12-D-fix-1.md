# Phase 12-D-fix-1 — Reviewer remediation pass

Targeted fix for the four codex `gpt-5.4` findings on Phase 12-D
(verdict `CONCERN` with all four findings substantive — orchestrator
override NOT applicable; remediation required). See
`dev/plans/runs/12-D-review-20260517T191833Z.md`.

Operates in the **existing 12-D worktree**
`/tmp/fdb-12-D-durability-harnesses-20260517T185755Z` on branch
`phase-12-D-durability-harnesses-20260517T185755Z`. Builds new
commits on top of `df7456b`.

## Model + effort

Opus 4.7, intent: medium. Spawn from main thread per
`dev/design/orchestration.md` § 6 fix-N pattern:

```bash
PHASE=12-D-fix-1
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-12-D-durability-harnesses-20260517T185755Z
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. Use
--disallowedTools Task Agent as a hard guard. Write code, run
tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-D-fix-1.md ) \
  | claude -p --model claude-opus-4-7 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Required reading

- `dev/plans/runs/12-D-review-20260517T191833Z.md` — reviewer
  verdict.
- `src/rust/crates/fathomdb-engine/src/lib.rs` — find
  `probe_wal_sidecar` (~line 3024) and the surrounding open path.
- `src/rust/crates/fathomdb-engine/tests/durability_soak.rs` —
  current AC-034a/b harness; line numbers from finding 2 + 4 are
  the patch sites.
- `src/rust/crates/fathomdb-engine/tests/durability_open_path.rs`
  — finding 3 patch site (line ~286).
- SQLite WAL header format (for finding 1): first 32 bytes are
  fixed-layout. Magic = `0x377f0682` or `0x377f0683` (little/big
  endian) at offset 0. Page size = u32 at offset 8. Frame data
  starts at offset 32. We do NOT need anything past offset 32.

## Scope — four findings, one commit per finding is fine

TDD discipline mandatory: failing test → green fix → refactor.

### Finding 1 (`high`) — `probe_wal_sidecar` bounded-read

Current: `std::fs::read(&wal_path)` pulls the entire WAL into
memory. Risk: open-path latency + RSS regression on large unclean
WALs; threatens AC-035 budget.

**Required fix:**

1. Replace `std::fs::read` with `std::fs::File::open` +
   `read_exact(&mut [u8; 32])`. The 32 bytes cover the full WAL
   header (magic + format-version + page-size + frame-checksum +
   salt-1 + salt-2 + checksum-1 + checksum-2). Anything past
   offset 32 is frame data — not needed for the magic + page-size
   pre-check we perform.
2. Add a test under `tests/support/corruption.rs` or
   `durability_open_path.rs` (whichever fits) that:
   - Builds a fixture DB, closes it.
   - Constructs an artificial WAL sidecar of bounded large size
     (e.g. 10 MB of valid-magic header + garbage frames).
   - Asserts `Engine::open` on the DB completes within a wall-clock
     bound (say ≤ 200 ms; tune to host) AND consumes no more than
     a bounded peak RSS delta.
   - **Alternative if RSS is hard to measure portably**: assert
     the open path reads no more than 32 bytes from the WAL via a
     `read_bytes_counter` test hook OR by intercepting via a
     `std::fs::File` wrapper that counts reads. If neither is
     feasible without engine surface expansion, a wall-clock-only
     bound is acceptable — surface as note in output.json.

WAL header layout reference:

```
offset 0:  u32 BE magic (0x377f0682 or 0x377f0683)
offset 4:  u32 BE format version (3007000)
offset 8:  u32 BE page size
offset 12: u32 BE checkpoint sequence number
offset 16: u32 BE salt-1
offset 20: u32 BE salt-2
offset 24: u32 BE checksum-1
offset 28: u32 BE checksum-2
offset 32: frame data starts
```

The existing `corrupt_wal_invalid_page_size` fixture
already writes a 32-byte WAL with `page_size > SQLITE_MAX_PAGE_SIZE`
— a bounded read suffices to detect it. Keep the existing fixture.

Verify the four AC-035a/b/c tests still pass after the bounded
read.

### Finding 2 (`medium`) — AC-034b: report p99 across all P-PWR-TRIALS

Current: harness records `lost_commit_ms: Option<u128>`, filters
out `None`, computes p99 on the subset. Trials where no commit
survived (e.g. kill happened before child's first commit) are
dropped. Acceptance assertion is "report p99 across P-PWR-TRIALS
trials" — full N=100, not a subset.

**Required fix:** ensure every victim commits at least one row
before SIGKILL.

Approach:

1. Per-trial child writes a sentinel file (e.g.
   `<tempdir>/trial_<n>.ready`) immediately after its first commit
   lands.
2. Parent loop: spawn child, wait for sentinel file to appear (with
   timeout — e.g. 5s; if no sentinel by then, surface as a separate
   harness failure, not as "trial dropped"), THEN sample the
   randomized kill delay, THEN `SIGKILL`.
3. After SIGKILL, parent reopens DB, reads the latest commit
   timestamp, computes `lost_commit_ms = (kill_ts -
   last_commit_ts).as_millis()`. Always `Some(_)`.
4. Compute p99 across all 100 trials. Assert ≤ 100 ms per AC-034b.

Tighten the assertion: `assert!(trials.len() == P_PWR_TRIALS)` and
`assert!(trials.iter().all(|t| t.lost_commit_ms.is_some()))` before
computing p99 — surface any sentinel-wait failures as harness
errors not data dropouts.

### Finding 3 (`medium`) — AC-035c true sibling-process flock

Current: test reacquires `.lock` with a second fd in the SAME
process. Acceptance text requires sibling process B observes lock
as acquirable.

**Required fix:** reuse Sub-2's test-binary-as-victim trick.

1. Add a second `#[ignore]` entry-point to
   `durability_open_path.rs` (or `tests/support/`):
   `_lock_acquire_probe_entry` — env-var-gated
   (`FATHOMDB_TEST_LOCK_PROBE=<path>`); attempts `flock(LOCK_EX |
   LOCK_NB)` on the path; exits 0 on success, 1 on `EWOULDBLOCK`.
2. Modify AC-035c lock-release tests to:
   - Trigger the corruption fixture in process A (current test
     process).
   - Assert `Engine::open` returns `Corruption(...)`.
   - Spawn child via
     `Command::new(std::env::current_exe()).env("FATHOMDB_TEST_LOCK_PROBE",
     <lock_path>).args(&["--ignored",
     "_lock_acquire_probe_entry"])`.
   - Assert child exits 0 (lock was acquirable from sibling
     process).
3. Same change for both `linux-only` AC-035c tests (header +
   schema fixtures).

Existing in-process fd reacquire can stay as a sanity check or be
removed (orchestrator-discretion; reviewer signal was that
in-process is "useful but not the contract" — sibling is the
contract).

### Finding 4 (`low`) — AC-034c stub body must `panic!()`

Current: `ac_034c_os_crash_zero_committed_tx_loss` body is empty
under `#[ignore]`. If `#[ignore]` is cleared without doing the
substrate work, test passes vacuously.

**Required fix:** body should `panic!()` with the blocker text +
pointer to substrate gap:

```rust
panic!(
    "AC-034c requires a KVM image with `echo c > /proc/sysrq-trigger` \
     and a preserved disk sync barrier (per `dev/acceptance.md` § \
     AC-034c fixture). That VM substrate does not exist in this repo. \
     See `dev/plans/runs/12-D-durability-harnesses-output.json` \
     blocker-3 for the substrate-gap detail and the recommended \
     12-D-OS-CRASH follow-up slice."
);
```

Net: clearing `#[ignore]` without implementing the harness fails
loudly instead of green-passing.

## Required commands

```bash
cd /tmp/fdb-12-D-durability-harnesses-20260517T185755Z
# AC-035a/b/c matrix still passes after bounded WAL read.
cargo test --workspace --test durability_open_path
# Bounded-WAL-read perf test (new in finding 1).
cargo test --workspace --test durability_open_path probe_wal_sidecar_bounded_read
# AC-034a/b power-cut harness with sentinel-wait + full-N p99.
AGENT_LONG=1 cargo test --release --test durability_soak ac_034a_and_b
# AC-034c stub panics if #[ignore] cleared (manual verify; not run in CI).
# Existing 12-D regression guards.
cargo test --workspace --test error_taxonomy
bash scripts/agent-verify.sh
```

All must pass. Known flakes (rerun once before declaring red):
`ac_029_canonical_writes_complete_under_projection_stall`,
`ac_017_vector_projection_freshness_p99_le_five_seconds`,
`t_safe_export_engine_error_exits_export_failure_66`.

## Discipline

- TDD: each finding's test lands red before the fix makes it
  green. For finding 1, the bounded-read test should fail against
  the current `std::fs::read` implementation.
- No scope creep into 12-D-OS-CRASH (that's a separate slice).
- Comment policy: WHY only, not WHAT. No "fixed in 12-D-fix-1"
  markers.
- Reuse test-binary-as-victim helper if one already exists in
  Sub-2 — do not duplicate.

## Blockers — surface before writing code

If any of these blocks the work, STOP and write a blocker report
at `dev/plans/runs/12-D-fix-1-output.json`:

1. **Bounded-read assertion infrastructure**: if neither RSS
   measurement nor a read-byte-counter is feasible without engine
   surface expansion, fall back to wall-clock-only bound + note
   the gap in output.json. Not a blocker, just a fallback.
2. **Sentinel-wait timeout tuning**: if 5s sentinel timeout is too
   tight under high host load, surface + propose larger bound (do
   NOT silently extend without orchestrator visibility).
3. **`std::env::current_exe()` test-binary re-entry**: already
   used in Sub-2 power-cut; if it doesn't work for the
   `_lock_acquire_probe_entry` pattern (e.g. cargo test runner
   forbids second re-entry per binary), surface + propose dedicated
   helper binary.

## Output

After all commands pass, write
`dev/plans/runs/12-D-fix-1-output.json`:

```json
{
  "phase": "12-D-fix-1",
  "baseline_sha": "df7456b",
  "branch": "phase-12-D-durability-harnesses-20260517T185755Z",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "findings_addressed": [
    "1 [high]: probe_wal_sidecar bounded read (32-byte header only); fixture asserts no full-file read",
    "2 [medium]: AC-034b harness uses per-trial sentinel-wait + full-N p99 (no trials dropped)",
    "3 [medium]: AC-035c lock-released asserts via spawned sibling process via Command::new(current_exe())",
    "4 [low]: AC-034c #[ignore] body panic!()s with blocker text if ignore is cleared without substrate"
  ],
  "long_run_results_post_fix": {
    "ac_034a_integrity_check_ok_per_trial": "<N>/100",
    "ac_034b_lost_commit_p99_ms": "<value> (computed across full 100-trial set)"
  },
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for clean PASS"
}
```

Then stop. Do not advance to 12-S. Do not run the reviewer
yourself.
