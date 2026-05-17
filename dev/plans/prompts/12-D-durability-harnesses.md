# Phase 12-D — Durability harnesses (AC-034a..AC-035c)

Phase 12 first slice. Implements the durability acceptance gates:
power-cut (AC-034a/b), OS-crash (AC-034c), and open-path corruption
matrix (AC-035a/b/c). First of 10 slices in the Path-to-Client-Ready
(0.6.0 GA) sequence (per
`dev/plans/0.6.0-implementation.md` § Path to Client-Ready).

Out of scope:

- Security fixtures (12-S separate slice).
- benchmark-and-robustness.yml (12-B separate slice).
- AC-035 (recovery time ≤ 2s for 1 GB DB worst-of-10) — separate AC
  not in this slice's range; covered by Phase 12 perf-evidence pass.
- AC-035d+ (recovery CLI verbs) — already landed in Phase 10.
- Pack 7 perf work.

## Model + effort

Opus 4.7, intent: high. Spawn from main thread per canonical
orchestration doc (`dev/design/orchestration.md` § 2):

```bash
PHASE=12-D-durability-harnesses
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT
re-run that block. Use --disallowedTools Task Agent as a hard
guard. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-D-durability-harnesses.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Log destination

- stdout/stderr: `dev/plans/runs/12-D-durability-harnesses-<ts>.log`
- structured output: `dev/plans/runs/12-D-durability-harnesses-output.json`
- reviewer verdict: `dev/plans/runs/12-D-review-<ts>.md`

## Required reading

- `AGENTS.md` § 1, § 3, § 4, § 5, § 7.
- `MEMORY.md`, especially:
  - `feedback_tdd.md` — red-green-refactor mandatory.
  - `feedback_reliability_principles.md` — no soak; net-negative LoC bias.
  - `feedback_no_data_migration.md` — schema-only changes only.
  - `feedback_file_deletion.md` — never `find -delete`.
- `dev/design/orchestration.md` § 2, § 3, § 8 (output schema).
- `dev/plans/0.6.0-implementation.md` § Path to Client-Ready (0.6.0 GA)
  — your slice's row + exit criterion.
- `dev/acceptance.md`:
  - AC-034a (power-cut zero corruption, P-PWR-TRIALS=100)
  - AC-034b (power-cut final-commit-loss p99 ≤ 100 ms)
  - AC-034c (OS-crash zero commit loss, P-OS-TRIALS=50)
  - AC-035a (Engine.open refuses on corruption — 4-fixture matrix)
  - AC-035b (CorruptionDetail shape — 4 fields per fixture)
  - AC-035c (lock released + no SQLite conn retained on Corruption)
- `dev/test-plan.md` — locate AC-034a..AC-035c row (the
  `durability_soak.rs` target file).
- `dev/design/recovery.md` — operator-facing corruption workflow.
- `dev/design/errors.md` — `OpenStage` / `CorruptionKind` /
  `RecoveryHint` matrix authoritative source.
- Existing substrate (READ BEFORE WRITING — much may be reusable):
  - `src/rust/crates/fathomdb-engine/src/lib.rs` lines ~640-760:
    `CorruptionDetail`, `CorruptionKind`, `OpenStage`,
    `CorruptionLocator`, `RecoveryHint`, `EngineOpenError` enums
    already exist.
  - `src/rust/crates/fathomdb-engine/tests/support/corruption.rs`:
    page-corruption helpers (`corrupt_database_header`,
    `corrupt_interior_page_byte`) — AC-035a likely uses these +
    needs two more flavors (WAL replay + embedder identity drift).
  - `src/rust/crates/fathomdb-engine/tests/error_taxonomy.rs`:
    existing open-path error tests. AC-035a/b/c may be partially
    covered already — STOP and INVENTORY before adding new tests.

## Scope — three sub-scopes, ordered

**Read the existing substrate FIRST.** Phase 11 had a recurring
pattern where the implementer rebuilt things that already existed.
For each sub-scope, START with an inventory pass: grep + read the
existing tests, enums, and helpers; identify what's already in
place; only write what's net-new.

Each sub-scope lands as its own commit (or commit pair: failing
test + green test). TDD discipline: red test first, green
implementation second.

### Sub-1: Open-path corruption matrix (AC-035a / AC-035b / AC-035c)

**Substrate exists** — `CorruptionDetail` + `CorruptionKind` +
`OpenStage` + `RecoveryHint` enums already in
`src/rust/crates/fathomdb-engine/src/lib.rs`. Page-corruption helpers
in `tests/support/corruption.rs`. The work is:

1. **Inventory pass** — read `error_taxonomy.rs` (and any other
   open-path test file) and list what's already covered for each
   of the four `CorruptionKind` variants:
   - `WalReplayFailure`
   - `HeaderMalformed`
   - `SchemaInconsistent`
   - `EmbedderIdentityDrift`
2. **AC-035a fixtures** — for each of the four `CorruptionKind`
   variants, ensure there's a test that:
   - Builds a clean DB (or seeds an existing one).
   - Closes it.
   - Applies the corruption (via existing helpers if available;
     otherwise add a new helper to `tests/support/corruption.rs`).
   - Re-opens via `Engine::open`.
   - Asserts result is `Err(EngineOpenError::Corruption(_))`.
   - Asserts no `Engine` handle observable in caller scope.
   - Asserts DB file mtime unchanged across the failed open.
   - Asserts no writer thread + scheduler present in the process
     after the failed open (use a process-introspection helper —
     see Sub-1 blocker note below).
3. **AC-035b shape** — extend the same tests (or a sibling file)
   to assert per-fixture:
   - `kind: CorruptionKind` matches expected.
   - `stage: OpenStage` matches expected and is NEVER
     `LockAcquisition`.
   - `locator: CorruptionLocator` has no free-form `Unspecified`;
     opaque-SQLite paths surface as
     `OpaqueSqliteError { sqlite_extended_code: i32 }`.
   - `recovery_hint: RecoveryHint { code: &'static str, doc_anchor:
&'static str }` carries the documented `(kind, stage, code)`
     tuple per AC-035b assertion text: - `(WalReplayFailure, WalReplay, "E_CORRUPT_WAL_REPLAY")` - `(HeaderMalformed, HeaderProbe, "E_CORRUPT_HEADER")` - `(SchemaInconsistent, SchemaProbe, "E_CORRUPT_SCHEMA")` - `(EmbedderIdentityDrift, EmbedderIdentity, "E_CORRUPT_EMBEDDER_IDENTITY")`
   - `code` stability: re-run the fixture and assert bit-equal
     `code` string.
4. **AC-035c sibling-lock + fd-introspection** — for the
   `HeaderMalformed` fixture (and any one other), assert:
   - From a sibling process (or fresh `OsString` lock-acquire
     attempt in the SAME process via `flock` on `<db>.lock`), the
     lock can be acquired (the failed-open process released it).
   - In the failed-open process, no open file descriptor points at
     the database file (introspect `/proc/self/fd/*` on linux).
   - In the failed-open process, no thread named per fathomdb
     writer / scheduler conventions is running (introspect
     `/proc/self/task/*/comm`).

Test file: extend `tests/error_taxonomy.rs` if it has space, OR
add `tests/durability_open_path.rs` (the more honest scope name —
"durability_soak" implies long-running; this slice is per-fixture
deterministic). HITL the file name choice if unclear.

**Sub-1 blocker note:** process-introspection (AC-035c) is
linux-specific. If macOS or Windows must be supported by the test
suite, surface as a blocker. Likely answer: linux-only is fine for
the CI gate (matches existing perf_gates.rs `AGENT_LONG` pattern
which is also linux-runner only).

### Sub-2: Power-cut harness (AC-034a + AC-034b)

**Substrate likely does NOT exist** for the power-cut harness.
This is new tooling. Approach:

1. **Fork-and-kill pattern.** Parent process forks a child;
   child opens the engine and starts a write workload of small
   commits (~100 byte payloads, single-row writes). Parent waits a
   randomized interval, then `kill -9 <child-pid>`. After kill,
   parent reopens the DB, runs `PRAGMA integrity_check`, asserts
   `ok`. Records the timestamp of the last surviving commit + the
   kill timestamp.
2. **Loop P-PWR-TRIALS=100 times** under `AGENT_LONG=1` gate
   (per existing `perf_gates.rs` pattern; `agent-verify` skips the
   long body). Record per-trial (last_commit_ts, kill_ts, integrity
   result).
3. **AC-034a assertion**: `integrity_check == "ok"` on every one
   of the 100 trials.
4. **AC-034b assertion**: `(kill_ts - last_commit_ts).quantile(0.99)
<= 100ms` across the 100 trials.

Test file: `src/rust/crates/fathomdb-engine/tests/durability_soak.rs`
per `test-plan.md` § AC-034a..AC-035c row.

**Sub-2 blocker note:** if the `fork` + `kill -9` pattern requires
a binary that isn't currently buildable inside the test (e.g. a
dedicated `power-cut-victim` binary), surface as a blocker before
writing the test. Likely answer: use `std::process::Command::new(
std::env::current_exe())` with an env-var-gated entry point inside
the test binary itself (test-binary-as-victim trick).

### Sub-3: OS-crash harness (AC-034c)

**Heavyweight; surface as blocker if substrate not available.**
Requires a VM (KVM or similar) with documented crash trigger
(`echo c > /proc/sysrq-trigger`) and a sync-barrier-preserving
disk config. Test-plan.md says "test-plan.md owns VM image + trigger
mechanism" — that VM image likely does not exist yet.

**Required behavior:**

1. **Inventory check first.** Read test-plan.md § AC-034a..AC-035c
   row and adjacent rows; check `dev/test-plan.md` for any VM image
   spec. Check `dev/release/fixtures/` and similar for any existing
   VM substrate.
2. **If VM substrate exists** — wire AC-034c against it; same
   structure as Sub-2 but using the documented VM trigger.
3. **If VM substrate does NOT exist** — STOP and surface as
   blocker. Document:
   - What VM substrate would be required (KVM image with synced
     disk, crash-trigger access, fathomdb binary installed inside,
     workload script).
   - Recommendation: defer AC-034c to a follow-up slice (call it
     12-D-OS-CRASH) and HITL the VM image build OR document an
     alternative crash-simulation approach (e.g. `eatmydata --no-sync`
     reverse + `kill -9` is NOT equivalent to OS crash; do NOT
     silently substitute).

Per `feedback_reliability_principles.md` (no-punt rule): if AC-034c
truly requires substrate not in this slice, surface clean and let
HITL decide. **Do not silently weaken the AC.**

## Required commands

```bash
cd /tmp/fdb-12-D-durability-harnesses-<ts>
# Net-positive LoC sanity: report new lines vs old per sub-scope.
git diff --stat 0.6.0-rewrite..HEAD
# AC-035 open-path matrix tests (fast — should run in agent-verify).
cargo test --workspace --test error_taxonomy
cargo test --workspace --test durability_open_path  # if created
# AC-034a/b power-cut tests (long-run; gated).
AGENT_LONG=1 cargo test --release --test durability_soak ac_034a
AGENT_LONG=1 cargo test --release --test durability_soak ac_034b
# AC-034c (if Sub-3 implemented, NOT skipped via blocker).
AGENT_LONG=1 cargo test --release --test durability_soak ac_034c
# Canonical local gate.
bash scripts/agent-verify.sh
```

All commands above must pass. Flake reruns (rerun once before
declaring red):

- `ac_029_canonical_writes_complete_under_projection_stall`
- `ac_017_vector_projection_freshness_p99_le_five_seconds`
- `t_safe_export_engine_error_exits_export_failure_66`

These are pre-existing host-load timing flakes unrelated to 12-D.

## Discipline

- **TDD mandatory** per `feedback_tdd.md`. Red test → green test
  → refactor. Every AC assertion lands as a failing test before
  the harness or assertion is implemented.
- **Inventory before writing.** If a CorruptionKind variant is
  already tested in `error_taxonomy.rs`, extend that test for
  AC-035b shape rather than duplicate it.
- **Net-negative LoC bias per `feedback_reliability_principles.md`.**
  Reuse `tests/support/corruption.rs` helpers; consolidate any
  duplicate corruption-injection paths discovered during inventory.
- **No-soak rule.** AC-034a/b/c use deterministic harnesses with
  bounded trial counts (P-PWR-TRIALS=100, P-OS-TRIALS=50); they are
  NOT soak tests. If a sub-scope would require open-ended soak,
  surface as blocker.
- **No silent AC weakening.** If a substrate gap blocks an AC,
  surface as blocker — do NOT replace `kill -9` with `drop()` or
  OS-crash with `kill -9`.
- **Comment policy:** WHY only, not WHAT. No
  "added for 12-D" markers.

## Blockers — surface before writing code

If any of these blocks the work, STOP and write a blocker report
at `dev/plans/runs/12-D-durability-harnesses-output.json` per the
10b-B blocker shape (see `dev/plans/runs/10b-B-purge-restore-output.json`
for the format):

1. **Process-introspection (AC-035c) requires non-linux support.**
   If Windows/macOS test runners are mandatory, current
   `/proc/self/*` approach won't work. Surface + recommend
   linux-only-CI scoping for the gate.
2. **Power-cut victim binary substrate.** If `Command::new(
current_exe())` + env-var-gated entry point cannot be wired
   (e.g. test runner forbids re-entry), surface + recommend
   alternative (dedicated `cargo-bin` victim binary or test crate).
3. **OS-crash VM substrate missing.** Surface clean per Sub-3
   blocker note above. Document the substrate gap; recommend
   12-D-OS-CRASH follow-up slice.
4. **AC-035b `EmbedderIdentityDrift` fixture not constructible.**
   This corruption flavor requires a stored embedder-profile row to
   be present and then corrupted. If the engine doesn't currently
   surface a way to seed an embedder-identity row deterministically
   from a test, surface the gap.

## Output

After all commands pass (or all blockers surfaced), write
`dev/plans/runs/12-D-durability-harnesses-output.json`:

```json
{
  "phase": "12-D-durability-harnesses",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-D-durability-harnesses-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "sub_scopes_landed": ["1 (AC-035a/b/c open-path matrix)", "2 (AC-034a/b power-cut)", "3 (AC-034c OS-crash) | blocked"],
  "acs_addressed": ["AC-034a", "AC-034b", "AC-034c", "AC-035a", "AC-035b", "AC-035c"],
  "tests_added": ["..."],
  "tests_modified": ["..."],
  "fixtures_added": ["..."],
  "substrate_reused": ["..."],
  "blockers_encountered": [{"id": "blocker-N", "description": "...", "resolution": "..."}],
  "agent_verify_result": "pass | fail (+ tail)",
  "long_run_results": {
    "ac_034a_integrity_check_ok_per_trial": "<N>/100 | not run (Sub-2 blocked)",
    "ac_034b_lost_commit_p99_ms": "<value> | not run",
    "ac_034c_committed_tx_lost_per_trial": "<N> | not run (Sub-3 blocked)"
  },
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for verdict"
}
```

Then stop. Do not advance to 12-S. Do not run the reviewer
yourself.
