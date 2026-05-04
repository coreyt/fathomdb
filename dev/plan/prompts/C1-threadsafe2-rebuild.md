# Phase C.1 — Rebuild bundled SQLite with `SQLITE_THREADSAFE=2`

## Model + effort

Sonnet 4.6, intent: high.

```bash
PHASE=C1-threadsafe2-rebuild
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-pack5-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "pack5-${PHASE}-${TS}" <B_STACK_KEPT_COMMIT_SHA>
( cd "$WT" && \
  cat /home/coreyt/projects/fathomdb/dev/plan/prompts/C1-threadsafe2-rebuild.md \
  | claude -p --model claude-sonnet-4-6 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --permission-mode bypassPermissions \
      --output-format json \
  > "$LOG" 2>&1 )
```

Reviewer pass: mandatory (cross-cutting build flag).

## Log destination

- `dev/plan/runs/C1-threadsafe2-rebuild-<ts>.log`
- `dev/plan/runs/C1-threadsafe2-rebuild-output.json`
- `dev/plan/runs/C1-review-<ts>.md`

## Required reading + discipline

- **Read `AGENTS.md` first** — canonical agent operating manual.
  Especially §1 (Public surface is contract — build flag is part
  of the contract; document in `dev/interfaces/rust.md`),
  §3 (`agent-verify.sh`), §4 (verification ordering),
  §5 (failing test first).
- **Read `MEMORY.md` + `feedback_*.md`** — especially
  `feedback_tdd.md`, `feedback_cross_platform_rust.md`
  (cross-platform CI must pass: x86_64-linux, aarch64-linux,
  darwin-arm64; do not let one platform's behavior justify
  hardcoding for another),
  `feedback_workflow_validation.md` (any CI workflow change uses
  actionlint, not yaml.safe_load).
- **TDD**: B.1's `sqlite3_threadsafe == 2` test must still pass
  (now via build flag rather than runtime config). Failing test
  first if you add a separate compile-options assertion.
- **Run `./scripts/agent-verify.sh`** before declaring success.

## Context

- Plan §6 C.1.
- Whitepaper §7.2 (highest-payoff fix), §8 open question 3
  (deployment-mode question).
- Cargo manifest: `src/rust/crates/fathomdb-engine/Cargo.toml:13`
  (`rusqlite = { version = "0.31", features = ["bundled"] }`).
- B.1 is the runtime no-rebuild path; C.1 is the build-flag path.
  Hypothesis: runtime config silently dropped, or the rebuild removes
  residual locks that runtime config cannot.

## Mandate

Switch the bundled SQLite build flag from `THREADSAFE=1` (default) to
`THREADSAFE=2` ("multi-thread", drops the global allocator/pcache
mutexes plus the per-connection mutex; caller must not share
connections across threads).

We already guarantee single-thread use of any connection via
`ReaderPool` (lib.rs:158) and the writer mutex (`Engine.connection:
Mutex<Option<Connection>>` at lib.rs:55), so THREADSAFE=2 is safe for
this codebase.

### Mechanism

Two routes; pick the one that compiles + works on the ARMv8 Tegra
host without surprising the Cargo build:

1. **Env-driven build**: set
   `LIBSQLITE3_FLAGS="-DSQLITE_THREADSAFE=2"` in the build environment.
   `rusqlite`'s `bundled` build script honors this. Confirm by reading
   the `libsqlite3-sys` build script of the version pinned in
   `Cargo.lock`. Document the env requirement in
   `dev/interfaces/rust.md` adjacent to existing build instructions.
2. **Cargo `build.rs` shim**: add a tiny `build.rs` to
   `fathomdb-engine` (or extend an existing one) that exports the env
   var to dependent build scripts. Use route 2 only if route 1 is not
   reliably picked up.

Document the chosen route inline in
`src/rust/crates/fathomdb-engine/Cargo.toml` (comment near line 13)
AND in `dev/interfaces/rust.md` so the build flag isn't lost in a
future bump of `rusqlite`.

### Validation

After build:

- `unsafe { rusqlite::ffi::sqlite3_threadsafe() }` returns `2`.
- `PRAGMA compile_options` includes `THREADSAFE=2`.
- B.1's runtime `init_sqlite_runtime` may now be a no-op; leave it in
  place but the `sqlite3_config(MULTITHREAD)` call should now return
  `SQLITE_OK` because the compile-time setting matches.

### Cross-platform check

CI must pass on x86_64 Linux, Darwin (any arch), and ARMv8 Linux.
If route 1 (env var) does not cleanly propagate on one of these
platforms, escalate before keeping. Do not hardcode `i8`/`u8` for
any FFI (memory: `feedback_cross_platform_rust.md`).

## Acceptance criteria

- `cargo build --release` succeeds on the local ARMv8 Tegra box.
- Test asserting `sqlite3_threadsafe() == 2` (added in B.1) still
  passes (now via build flag, not runtime config).
- `PRAGMA compile_options` lists `THREADSAFE=2`. Capture into
  `dev/plan/runs/C1-evidence-compile-options.txt`.
- AC-018 green.
- AC-020 long-run: passes the 5.33x bound (decision rule per plan §6).
- Reviewer verdict not BLOCK.
- `dev/interfaces/rust.md` updated with the build flag note.

## Files allowed to touch

- `src/rust/crates/fathomdb-engine/Cargo.toml` (comment; build flag
  documentation only).
- `src/rust/crates/fathomdb-engine/build.rs` (new, only if route 2).
- `dev/interfaces/rust.md`.
- `dev/plan/runs/`.
- §12 + whitepaper update.

## Files NOT to touch

- `Cargo.lock` (other than what `cargo` regenerates).
- Other crates.
- Schema, migrations, src/python.

## Verification commands

```bash
cargo clean -p fathomdb-engine
LIBSQLITE3_FLAGS="-DSQLITE_THREADSAFE=2" cargo build -p fathomdb-engine --release --tests
cargo test -p fathomdb-engine --release
AGENT_LONG=1 cargo test -p fathomdb-engine --release --test perf_gates \
    ac_020_reads_do_not_serialize_on_a_single_reader_connection \
    -- --nocapture  # x5
./scripts/agent-verify.sh
```

## Required output to orchestrator

```json
{
  "phase": "C1",
  "decision": "KEEP|REVERT|INCONCLUSIVE",
  "build_route": "env_LIBSQLITE3_FLAGS|build_rs",
  "build_route_rationale": "<one line>",
  "compile_options_threadsafe": "<line>",
  "compile_options_full_path": "dev/plan/runs/C1-evidence-compile-options.txt",
  "threadsafe_after_open": <integer>,
  "build_time_clean_s": <f>,
  "binary_size_release_bytes": <n>,
  "binary_size_delta_vs_baseline_pct": <f>,
  "before": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5,
    "source": "B-stack KEPT | A.1 baseline if B-stack was REVERT"
  },
  "after": {
    "raw_runs": [{"sequential_ms": <n>, "concurrent_ms": <n>}, ...],
    "sequential_ms": <n>, "concurrent_ms": <n>, "bound_ms": <n>, "speedup": <f>, "stddev_concurrent": <n>, "n": 5
  },
  "delta_concurrent_pct": <f>,
  "delta_sequential_pct": <f>,
  "delta_speedup_pct": <f>,
  "ac017_status": "green|red:<numbers>",
  "ac018_status": "green|red:<numbers>",
  "ac018_drain_ms_after": <n>,
  "ac020_passes_5_33x": true|false,
  "ac020_passes_packet_1_25_margin": true|false,
  "ci_cross_platform": "verified|deferred:<reason>",
  "ci_platforms_passed": ["x86_64-linux", "aarch64-linux", "darwin-arm64"],
  "ci_platforms_failed": [],
  "reviewer_verdict": "PASS|CONCERN|BLOCK",
  "phase78_review_verdict": "PASS|CONCERN|BLOCK",
  "deployment_mode_question": "<text — whitepaper §8 q3 status: bundled-only commitment, or graceful degrade strategy>",
  "loc_added": <n>, "loc_removed": <n>,
  "files_changed": ["src/rust/crates/fathomdb-engine/Cargo.toml", "dev/interfaces/rust.md", "..."],
  "commit_sha": "<sha if KEEP>",
  "data_for_pivot": "<if rebuild also doesn't close the bound: mutex layer is NOT the bottleneck — promote D.1 to primary; if D.1 also fails, escalate (CPU-bound hypothesis, larger fixture needed). Also note whether reverting C.1 cleanly restores baseline or leaves a dirty Cargo.lock.>",
  "unexpected_observations": "<free text>",
  "next_phase_recommendation": "verification-gate|D1|done"
}
```

## Required output to downstream agents

- D.1 (if still needed) baseline = C.1 KEPT.
- Final synthesis must call out the deployment-mode question
  (whitepaper §8 open question 3) — fold C.1's KEEP/REVERT into
  that paragraph.

## Update log

- 2026-05-03 — B.1 REVERT (`d448263`, output JSON only; source
  unchanged). Runtime CONFIG_MULTITHREAD wiring is provably
  correct (`sqlite3_config` rc=0, OnceLock-cached, idempotent,
  test-asserted), but AC-020 ratio is unchanged within noise:
  conc 115→120.6 ms (+4.9%, +1.7σ), speedup 1.58→1.526× (-3.4%).
  Hypothesis "runtime threading-mode flag relieves bottleneck"
  falsified high-confidence (the gate is observable, not silent).
  C.1 is now the next experiment per A.4 alt-on-fail extended.
- C.1 spawn baseline: `0.6.0-rewrite` tip after B.1 + this
  bookkeeping commit lands. Replace `<B_STACK_KEPT_COMMIT_SHA>`
  with `0.6.0-rewrite`. The B-stack is REVERT, NOT KEPT — `before`
  block in C.1 output JSON sources from A.1 baseline directly
  (sequential 182, concurrent 115, speedup 1.58, n=5). Do **not**
  use B.1 #2 numbers as the `before` block — B.1's `after` was a
  REVERT-state snapshot and is statistically indistinguishable
  from A.1.
- A.1 baseline (carry into `before` block):
  - sequential N=5 `[189,199,182,179,176]` ms; median 182, stddev 9.2
  - concurrent N=5 `[120,110,117,115,112]` ms; median 115, stddev 4.0
  - speedup_observed 1.58×; required 5.33×; gap 3.4×
- A.2 classification (carry-forward; the contended primitives C.1
  must move):
  - mutex_atomic 6.45% seq → 36.98% conc (5.73× growth, +262M cycles)
  - Top concurrent symbols to watch on `after`:
    `__aarch64_swp4_rel` 11.2%, `__aarch64_cas4_acq` 9.8%,
    `___pthread_mutex_lock` 6.8%, `__aarch64_swp4_acq` 5.9%,
    `lll_mutex_lock_optimized` 1.8%.
  - If C.1 KEEPs but mutex_atomic share doesn't drop substantially
    in a fresh A.1-recapture, C.1 didn't reach the right mutex
    either — escalate (the WAL-pager spinlock would be unaffected
    by `THREADSAFE=2`).
- A.3 confirms `sqlite3_threadsafe()` returns the **compile-time**
  value, currently `1`. After C.1 rebuild it should return `2`
  (this assertion IS valid for C.1 because C.1 changes the
  compile-time flag, unlike B.1 which only changed runtime config).
- B.1 `init_sqlite_runtime()` should be deleted by C.1 if C.1 KEEPs
  (`THREADSAFE=2` makes runtime config unnecessary). Net-negative
  LoC per `feedback_reliability_principles.md`. If C.1 REVERTs,
  B.1's revert state is already restored — nothing to undo.
- Decision rule for C.1 KEEP/REVERT (numeric, mirrors A.4 form):
  - KEEP iff `concurrent_median_ms ≤ 80` AND `speedup ≥ 5.0×` AND
    AC-018 green (≥30% drop from A.1 baseline 115 ms). Same threshold
    as B.1 because the goal is unchanged: close AC-020.
  - INCONCLUSIVE band 80-100 ms → recapture A.1 against the C.1
    branch and re-classify; if mutex_atomic share dropped but speedup
    didn't reach 5.0×, the bottleneck moved (likely vec0_fts) →
    promote D.1.
  - REVERT iff `concurrent_median_ms > 115` OR AC-018 red OR
    cross-platform build fails. Both `THREADSAFE=2` and
    `NO_MUTEX` should be set together (per §7.2); reverting means
    restoring the bundled SQLite default.
- Kill criteria: if C.1 also lands flat (within ±5% of A.1
  baseline AND mutex_atomic share unchanged in recapture), the
  mutex track is wrong — promote D.1. Per A.4: "if B.1+B.3
  stacked still <10% drop, mutex track wrong, promote D.1" —
  C.1 substitutes for "B.1+B.3 stacked" because it directly
  tests the mutex layer at the compile-time strongest
  intervention.
- Cross-platform checklist (load-bearing — the bundled build
  flag change must work on aarch64-Linux + x86_64-Linux + macOS):
  - `feedback_cross_platform_rust.md`: c_char is i8 on x86_64
    plus Darwin; u8 on aarch64 Linux. C.1 should not introduce new
    FFI hardcodes; B.1 already used `std::os::raw::c_int` /
    `c_char`.
  - `libsqlite3-sys-0.30.1/build.rs:136` is the line to change
    (`-DSQLITE_THREADSAFE=1` → `-DSQLITE_THREADSAFE=2`). Path
    is `/home/coreyt/.cargo/registry/src/index.crates.io-...`;
    the rebuild is via Cargo.toml feature flag or workspace
    patch — see prompt §"What to do".
  - Validate post-rebuild: `unsafe { rusqlite::ffi::sqlite3_threadsafe() } == 2`
    (this assertion IS valid here, unlike B.1).
- Reviewer (codex `gpt-5.4`) MANDATORY for C.1 — Cargo.toml /
  build.rs change has cross-platform implications and the
  reviewer must verify no new hardcoded `i8`/`u8` slipped in,
  schema/migration files unchanged, no Cargo.lock churn that
  would block downstream builds.
