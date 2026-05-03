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

_(append B-stack KEPT numbers + decision rule + cross-platform
checklist before spawn)_
