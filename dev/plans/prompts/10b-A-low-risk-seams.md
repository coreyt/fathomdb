# Phase 10b-A — Five low-risk recovery seams + CLI wire-up

Phase 10b first slice. Builds the five low-risk engine seams locked in
`dev/plans/0.6.0-implementation.md` § Phase 10b, re-exports the corresponding
report types through the `fathomdb` facade, and replaces the
`stub_doctor(...)` / `recover --truncate-wal` not-implemented branches with
real wire-ups + tests.

Out of scope: `purge_logical_id`, `restore_logical_id`. Those land in 10b-B.

## Model + effort

Opus 4.7, intent: high. Spawn from main thread:

```bash
PHASE=10b-A-low-risk-seams
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT re-run
that block. Use --disallowedTools Task Agent as a hard guard if you
forget. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/10b-A-low-risk-seams.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Anti-chaining: PREAMBLE prepended via stdin per
`dev/plans/prompts/01-orchestrator-resume.md` §4. Reviewer
(`codex --model gpt-5.4`) MANDATORY after this phase: it touches the
Rust facade re-export surface + adds new public engine methods.

## Log destination

- stdout/stderr: `dev/plans/runs/10b-A-low-risk-seams-<ts>.log`
- structured output: `dev/plans/runs/10b-A-low-risk-seams-output.json`
- reviewer verdict: `dev/plans/runs/10b-A-review-<ts>.md`

## Required reading

- `AGENTS.md` (§1, §3, §4, §5, §7).
- `MEMORY.md` and the linked `feedback_*.md` — esp. `feedback_tdd.md`,
  `feedback_reliability_principles.md`, `feedback_python_native_build.md`.
- `dev/plans/0.6.0-implementation.md` § Phase 10b (lines 377-425) and
  § Immediate Next Slice (lines 607-624).
- `dev/design/recovery.md` § JSON shapes for other doctor verbs
  (lines 81-93). These keys are the locked CLI JSON contract.
- `dev/design/recovery.md` § Operator path lines 264, plus the
  `truncate-wal` operator behavior (`PRAGMA wal_checkpoint(TRUNCATE)`).
- `dev/interfaces/rust.md` § Recovery / operator seam re-exports
  (lines 85-117). Phase 10b adds named symbols here.
- `dev/acceptance.md` AC-040a (line 664), AC-040b (line 672),
  AC-058 (line 886). These are the closure-acceptance asserts.
- Phase 10a landed example: `src/rust/crates/fathomdb-engine/src/lib.rs`
  `Engine::check_integrity`, `Engine::safe_export`, `Engine::excise_source`
  (study these for transaction scope, error mapping, report shape).
- Phase 10a CLI wiring: `src/rust/crates/fathomdb-cli/src/lib.rs` lines
  255-592 (`run_doctor_verb_inner`, `wire_recover`, JSON serializers).
  Mirror these patterns; do not introduce new dispatch helpers.

## Scope

Five engine seams + their report types + facade re-exports + CLI wire +
tests. Plus `truncate_wal` is a `recover` sub-flag, not a doctor verb.

### 1. Engine seams

In `src/rust/crates/fathomdb-engine/src/lib.rs` add the following
public methods on `impl Engine`. Output shapes per
`dev/design/recovery.md` § JSON shapes (lines 86-93) plus the locked
report-type names from `dev/interfaces/rust.md` line 106-108.

| Method                         | Return type             | Behavior                                                                                                                                                                                                                                                  |
| ------------------------------ | ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `verify_embedder(&self, supplied_identity: &str, supplied_dimension: u32) -> Result<VerifyEmbedderReport, EngineError>` | `VerifyEmbedderReport` | Read stored embedder identity + dimension (already persisted; same source as `EngineOpenError::EmbedderIdentityMismatch` open-time check). Compare to supplied. Return `status` = typed `Match` / `IdentityMismatch` / `DimensionMismatch` / `BothMismatch`. |
| `dump_schema(&self) -> Result<DumpSchemaReport, EngineError>` | `DumpSchemaReport` | Read `PRAGMA user_version`. Walk `sqlite_schema` table; collect `{name, sql}` for `type='table'` and `type='index'` (excluding internal `sqlite_*` rows). Return `user_version`, `tables`, `indexes`. |
| `dump_row_counts(&self) -> Result<DumpRowCountsReport, EngineError>` | `DumpRowCountsReport` | For each canonical table, `SELECT COUNT(*)`. Return `counts: Vec<{name, rows}>`. There is no existing canonical-table constant (verified 2026-05-14). Define one in `fathomdb-schema` (alongside the migration step list at `src/rust/crates/fathomdb-schema/src/lib.rs:33`) and re-use it from both `dump_schema` ordering and `dump_row_counts`. Exclude projection / FTS / vec0 shadow tables — `dump_row_counts` is canonical-only. |
| `dump_profile(&self) -> Result<DumpProfileReport, EngineError>` | `DumpProfileReport` | Read stored embedder identity + dimension. Read vectorized-kind list (same source `Engine::open` uses for the AC-048/048b probe). Return `embedder_identity`, `embedder_dimension`, `vectorized_kinds`. |
| `truncate_wal(&self) -> Result<TruncateWalReport, EngineError>` | `TruncateWalReport` | Run `PRAGMA wal_checkpoint(TRUNCATE)`. Capture the three returned counters: `busy`, `log_frames`, `checkpointed_frames`. Return those plus `status` (`Done` / `Busy`). |

Report-type definitions live in `fathomdb-engine` (same module as
`IntegrityReport`, `ExciseReport`). Match the existing report derive
pattern: `#[derive(Clone, Debug, Eq, PartialEq)]`. No serde derives —
the CLI layer owns JSON serialization. Status enums (where applicable)
use the same derive. Do NOT add `verb` string fields to engine types —
the CLI layer owns the `verb` JSON discriminator.

### 2. Facade re-exports

Extend the `pub use fathomdb_engine::{...};` block in
`src/rust/crates/fathomdb/src/lib.rs` to include the five new report
types + any new status enums they introduce. Match the ordering style
already in the file (alphabetical within each cluster is fine if
unclear).

Update `dev/interfaces/rust.md` § Recovery / operator seam re-exports
(lines 93-103): move the five report types from the "Phase 10b"
forward-reference paragraph into the locked-symbol list. Keep the
"locked 2026-05-12" attribution; add "extended <today's-UTC-date>".

### 3. CLI wire-up

In `src/rust/crates/fathomdb-cli/src/lib.rs`:

- Replace the four `stub_doctor(&args.db_path, "<verb>")` calls (lines
  342-345) with real `run_doctor_verb(...)` wirings that call the
  corresponding engine method and emit the JSON shape from
  `recovery.md` § JSON shapes.
- For `recover --truncate-wal`, add a sixth dispatch arm in
  `run_recover` (after the `excise_source` arm, before the
  `not_implemented` fall-through). It uses `wire_recover` and returns
  the `TruncateWalReport` JSON.
- Add JSON serializers next to the existing
  `safe_export_json` / `trace_report_json` / `excise_report_json`
  helpers. Naming convention: `<seam>_report_json`.
- For `verify-embedder` the CLI must accept supplied identity +
  dimension. `dev/interfaces/cli.md:52` was amended 2026-05-15 to lock
  the invocation as
  `fathomdb doctor verify-embedder --identity <s> --dimension <n>`.
  Introduce a `VerifyEmbedderArgs` struct (mirror `TraceArgs` shape)
  with `--identity` (String, required), `--dimension` (u32, required),
  `--json` (bool), and `db_path` (PathBuf positional). Update
  `DoctorCommand::VerifyEmbedder` to use it.

Outcome mapping: all five doctor verbs use the default
`run_doctor_verb` path (no special error-class override). `truncate_wal`
uses `wire_recover` (returns `RECOVERY_ACCEPTED_LOSS` = 64 on success).

### 4. Tests

TDD: red-green-refactor. Write the failing test first; verify it fails
for the expected reason; then implement.

- **Engine unit tests** in `src/rust/crates/fathomdb-engine/src/lib.rs`
  test module. One smoke test per seam, on a freshly-opened in-memory
  or tempfile-backed engine:
  - `verify_embedder`: open with identity X, verify matching X → `Match`;
    verify mismatched identity → `IdentityMismatch`; verify wrong
    dimension → `DimensionMismatch`.
  - `dump_schema`: assert `user_version` > 0, at least one table named
    by the canonical schema is present.
  - `dump_row_counts`: count empty database returns all-zero counts;
    after one write the relevant table count increases.
  - `dump_profile`: open with embedder X / dim N; assert returned
    identity/dim match X/N.
  - `truncate_wal`: assert `status = Done`, counters non-negative.
- **CLI integration tests** in
  `src/rust/crates/fathomdb-cli/tests/operator_cli.rs`. Extend the
  existing `AC-040a/040b` coverage so the four newly-wired doctor
  verbs (`verify-embedder`, `dump-schema`, `dump-row-counts`,
  `dump-profile`) execute end-to-end with `--json` against a real
  on-disk DB and emit the documented top-level keys.
- **CLI integration tests** in
  `src/rust/crates/fathomdb-cli/tests/recovery_cli.rs`. Add a
  `recover --accept-data-loss --truncate-wal` happy-path test that
  asserts exit 64, valid JSON with `verb: "truncate-wal"`, and the
  refusal-without-`--accept-data-loss` case (exit 70). Also extend
  the AC-058 `--help` sub-flag enumeration test if it does not already
  cover `--truncate-wal` (it should — the flag is parser-bound;
  verify).

If existing operator_cli.rs tests are gated with `#[ignore]` for the
deferred verbs, remove those gates as part of green.

## Required commands

Run inside the worktree (`$WT`):

```
cd "$WT"
# Inner loop while developing — fast feedback per crate
cargo test -p fathomdb-engine -p fathomdb -p fathomdb-cli
# Canonical gate (covers lint → typecheck → test). MUST pass before commit.
./scripts/agent-verify.sh
```

`scripts/agent-verify.sh` is the canonical local gate (per `AGENTS.md`
and the script itself: "lint -> typecheck -> test in latency order").
If it surfaces a failure that is not directly caused by this slice,
stop and surface it — do not fix unrelated breakage in this commit.

## Discipline

- TDD per `feedback_tdd.md`: each seam is red-green-refactor.
- No data-migration shim, no 0.5.x compat code, no soak. Per
  `feedback_reliability_principles.md`: delete-before-add; no scope
  creep into Phase 11 / 12 work.
- Do NOT silently broaden the canonical-table list. Use the existing
  schema-owned constant. If it does not exist, surface that as a
  blocker; do not invent a duplicate list.
- Comment policy per `AGENTS.md` and `feedback_*`: no comments
  explaining WHAT the code does; only non-obvious WHY. No "added in
  10b-A" or "for the new seam" markers.
- Cite acceptance ids in test names / module docs:
  `AC-040a`, `AC-040b`, `AC-058`. The reviewer cross-checks.
- One commit per logical step is fine; squash before push is not
  required. Last commit message should include a Phase-10b-A
  closure summary line.

## Output

After all commands pass, write
`dev/plans/runs/10b-A-low-risk-seams-output.json` with:

```json
{
  "phase": "10b-A",
  "baseline_sha": "bd0cba1debcea738802cc3b87315b95dcc0af355",
  "branch": "phase-10b-A-low-risk-seams-<ts>",
  "head_sha": "<HEAD after final commit>",
  "seams_added": [
    "Engine::verify_embedder",
    "Engine::dump_schema",
    "Engine::dump_row_counts",
    "Engine::dump_profile",
    "Engine::truncate_wal"
  ],
  "facade_reexports_added": [
    "VerifyEmbedderReport",
    "DumpSchemaReport",
    "DumpRowCountsReport",
    "DumpProfileReport",
    "TruncateWalReport"
  ],
  "tests_added": ["<test names>"],
  "acceptance_ids_bound": ["AC-040a", "AC-040b", "AC-058"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "spawn 10b-A reviewer"
}
```

Then stop. Do not advance to 10b-B. Do not run the reviewer yourself.
