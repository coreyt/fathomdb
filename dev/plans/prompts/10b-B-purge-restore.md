# Phase 10b-B — Logical-id purge + restore engine seams + CLI wire-up

Phase 10b second slice. Builds the two remaining 0.6.0 recovery seams:
`Engine::purge_logical_id` and `Engine::restore_logical_id`, plus their
report types, facade re-exports, and CLI wire-up. After 10b-B lands,
Phase 10b's exit gate is satisfiable (all seven `doctor` verbs + six
`recover` sub-flags reachable).

This is the higher-risk slice in Phase 10b. Restore requires writer
replay of `append_only_log` history with restore-provenance marking;
purge requires multi-table cascade in one transaction. Read the
referenced specs end-to-end before writing code.

## Model + effort

Opus 4.7, intent: xhigh. Spawn from main thread:

```bash
PHASE=10b-B-purge-restore
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
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/10b-B-purge-restore.md ) \
  | claude -p --model claude-opus-4-7 --effort xhigh \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

Anti-chaining: PREAMBLE prepended via stdin per
`dev/plans/prompts/01-orchestrator-resume.md` §4. Reviewer
(`codex --model gpt-5.4`) MANDATORY after this phase — touches writer
path + new public Engine methods + new typed error variants.

## Log destination

- stdout/stderr: `dev/plans/runs/10b-B-purge-restore-<ts>.log`
- structured output: `dev/plans/runs/10b-B-purge-restore-output.json`
- reviewer verdict: `dev/plans/runs/10b-B-review-<ts>.md`

## Required reading

Specs first:

- `dev/design/recovery.md` § Logical-id purge and restore (lines
  95-179). Cascade scope, replay source, report shapes, typed failure
  modes — all locked here. The diff MUST conform.
- `dev/design/engine.md` § Canonical identity and supersession (lines
  107-194). `row_id` / `logical_id` / `superseded_at` model. Defines
  what "active" means and what restore must preserve. § Restoration
  as a canonical write (line 171) explicitly says restore appends a
  new active row with restore provenance, not a mutation of
  `superseded_at`.

Plan + acceptance:

- `dev/plans/0.6.0-implementation.md` § Phase 10b (lines 377-425). The
  seam table + slice order; this prompt covers slice 2 (purge) and
  slice 3 (restore) plus the CLI wire-up for both.
- `dev/acceptance.md` AC-058 (line 886): `recover --help` must
  enumerate `--purge-logical-id` and `--restore-logical-id`. AC-058
  for help is satisfied by 10b-A; this phase makes the runtime
  invocation work end-to-end.

Interface contracts:

- `dev/interfaces/cli.md:71-72` — recover sub-flag synopsis already
  locks `--purge-logical-id <id>` and `--restore-logical-id <id>`. No
  cli.md amendment needed (compared to 10b-A's `verify-embedder`).
- `dev/interfaces/rust.md:106-110` — `PurgeLogicalIdReport` /
  `RestoreLogicalIdReport` named in the forward-reference paragraph;
  move them into the locked-symbol list as part of this slice.

Code precedent (mirror these patterns):

- `src/rust/crates/fathomdb-engine/src/lib.rs:1996-2122` —
  `Engine::excise_source` + `excise_source_inner`. Same cascade-in-
  one-transaction pattern; same drain-before-mutate posture
  (`projection_runtime.set_frozen(true)`; drain with
  `REBUILD_DRAIN_TIMEOUT_MS`; restore frozen state on exit). Both
  purge and restore mutate canonical state + projections and MUST
  drain the projection runtime first to prevent in-flight projection
  workers from racing the transaction.
- `src/rust/crates/fathomdb-engine/src/lib.rs:2018-2122` —
  `verify_embedder` / `dump_schema` / `dump_row_counts` /
  `dump_profile` / `truncate_wal` (10b-A landed). Same module layout.
- `src/rust/crates/fathomdb-cli/src/lib.rs:255-768` (post-10b-A) —
  CLI dispatcher + JSON serializer pattern. Two existing recover
  sub-flag arms already use `wire_recover` (`rebuild-projections`,
  `rebuild-vec0`, `excise-source`, `truncate-wal`); add two more.

Operational rules (don't violate):

- `AGENTS.md` §1 (TDD mandatory, ADRs authoritative, Public surface =
  contract). §5 (failing test first; capture the red run in the
  stream log explicitly — finding #2 in the 10b-A reviewer verdict
  flagged absent red-step evidence; do not repeat that gap).
- `MEMORY.md` + linked `feedback_*.md`. Esp. `feedback_tdd.md`,
  `feedback_reliability_principles.md` (net-negative LoC bias; no
  scope creep; delete-before-add when refactoring shared helpers).

## Scope

Two engine seams + report types + facade re-exports + CLI wire +
tests. Plus extension of typed `EngineError` variants for the three
restore-specific failure modes specified in recovery.md.

### 1. Engine seam: `Engine::purge_logical_id`

```rust
pub fn purge_logical_id(
    &self,
    logical_id: &str,
) -> Result<PurgeLogicalIdReport, EngineError>;
```

Cascade scope (per `dev/design/recovery.md:103-121`):

- **Canonical:** DELETE every row matching `logical_id` from
  `canonical_nodes` and `canonical_edges`, across active + superseded
  rows. (Other canonical kinds — chunks / runs / steps / actions —
  use `id`, not `logical_id`, per engine.md:121; they are NOT in
  scope for logical-id purge.)
- **`latest_state` op-store collections:** DELETE entries keyed by
  `logical_id`. `latest_state` writes land in `operational_mutations`
  with `collection_name = 'latest_state'`; the per-collection record
  key is `record_key`. Delete rows where `record_key = ?1` AND
  `collection_name = 'latest_state'`. (Confirm exact key shape by
  reading the writer; if `latest_state` is keyed differently for some
  kinds, surface as a blocker — do not invent semantics.)
- **Derived projections:** DELETE FTS rows (`search_index`) and
  vector rows (`vector_default`, `_fathomdb_vector_rows`,
  `_fathomdb_projection_terminal`) attributable to the purged
  `logical_id`. Mirror excise's per-cursor delete pattern
  (`src/rust/crates/fathomdb-engine/src/lib.rs:2150-2173`): collect
  the `write_cursor` set for the logical_id's canonical rows, then
  targeted-delete shadow rows by cursor. This avoids over-deleting
  projection rows attached to sibling logical ids.
- **`append_only_log` op-store collections:** PRESERVE. These are
  durable audit history and the replay source for
  `restore_logical_id`. Purge does not rewrite history.
- **Other sources:** Untouched. The non-perturbation rule from
  excise applies: do not invalidate projections for sibling logical
  ids.

Transaction discipline:

- Drain projection runtime first (`projection_runtime.set_frozen(true)`
  - `drain(REBUILD_DRAIN_TIMEOUT_MS)`) — mirrors excise.
- All cascade steps execute in one SQLite transaction; rollback on
  any failure.
- Restore frozen state on exit (success OR failure).

Report shape (`PurgeLogicalIdReport`, per `recovery.md:123-131`):

```rust
pub struct PurgeLogicalIdReport {
    pub logical_id: String,
    pub canonical_rows_deleted: u64,
    pub latest_state_keys_deleted: u64,
    pub projection_rows_invalidated: u64,
    pub append_only_log_rows_preserved: u64,
    pub status: PurgeLogicalIdStatus,
}

pub enum PurgeLogicalIdStatus {
    Done,
    NoOp,    // LogicalIdNotFound per recovery.md:135 — not a hard error
    Partial, // only if recovery.md adds a partial-success path; today's
             // spec uses transactional rollback so this variant may
             // exist for forward-compat without a current emit site.
}
```

Failure mapping (per `recovery.md:133-138`):

- `LogicalIdNotFound`: id matches no canonical row AND no
  `latest_state` entry. Return `Ok` with `status = NoOp` and zero
  counters; do NOT raise.
- Any SQLite / drain failure: standard transactional rollback;
  surface as `EngineError::Storage` (or the more specific existing
  variant if applicable). No new typed error variant needed for
  purge.

### 2. Engine seam: `Engine::restore_logical_id`

```rust
pub fn restore_logical_id(
    &self,
    logical_id: &str,
) -> Result<RestoreLogicalIdReport, EngineError>;
```

Replay semantics (per `dev/design/recovery.md:140-153`):

1. Read ordered `append_only_log` events for the logical_id from
   `operational_mutations` WHERE `collection_name = 'append_only_log'`
   ORDER BY `write_cursor`. If empty → return
   `EngineError::NoRestorationSource` (new variant).
2. Verify no active canonical row exists for the logical_id
   (`SELECT 1 FROM canonical_nodes WHERE logical_id = ?1 AND
superseded_at IS NULL` and same for canonical_edges). If any
   active row exists → return `EngineError::ConflictingActiveRow`.
3. Replay the event stream through the standard writer path. The
   final active row carries a `restore_provenance` field marking it
   as restoration output. **Implementation note:** if the writer
   path does not yet support a `restore_provenance` column on the
   canonical row, this slice MUST add the column (schema bump) AND
   plumb the field through `prepare_write` / writer. If a schema
   bump is required, surface as a blocker BEFORE writing the schema
   migration — confirm with the HITL through the orchestrator log
   first.
4. Re-derive projections (FTS + vector) from the restored active
   state. If the schema definitions referenced by the replayed
   events are no longer present (e.g. a `schema_id` referenced in
   the log was dropped) → return `EngineError::IncompatibleSchema`.
5. Commit as one transaction; restore the projection runtime on
   exit.

Transaction discipline mirrors purge (drain first, single
transaction, restore frozen state on exit).

Report shape (`RestoreLogicalIdReport`, per `recovery.md:155-163`):

```rust
pub struct RestoreLogicalIdReport {
    pub logical_id: String,
    pub events_replayed: u64,
    pub canonical_rows_restored: u64,
    pub projection_rows_rebuilt: u64,
    pub restore_cursor: i64,
    pub status: RestoreLogicalIdStatus,
}

pub enum RestoreLogicalIdStatus {
    Done,
    // Failure modes use typed EngineError variants; no in-report
    // Failure variant needed.
}
```

New `EngineError` variants (per `recovery.md:165-175`):

```rust
NoRestorationSource,
ConflictingActiveRow,
IncompatibleSchema,
```

Add stable_code mappings in `EngineError::stable_code()` and the
matching CLI `engine_error_code` arm in
`src/rust/crates/fathomdb-cli/src/lib.rs:459-475`. Map all three to
`CliOutcome::Unrecoverable` (exit 70) — they are operator-visible
restore-precondition failures, not lock-held or data-loss-accepted
classes.

### 3. Facade re-exports

Extend `pub use fathomdb_engine::{...}` in
`src/rust/crates/fathomdb/src/lib.rs` with:

- `PurgeLogicalIdReport`
- `PurgeLogicalIdStatus`
- `RestoreLogicalIdReport`
- `RestoreLogicalIdStatus`

Update `dev/interfaces/rust.md`: move `PurgeLogicalIdReport` and
`RestoreLogicalIdReport` from the forward-reference paragraph
(currently at lines 106-110 post-10b-A) into the locked-symbol list.
Add the new status enums to the same list.

### 4. CLI wire-up

In `src/rust/crates/fathomdb-cli/src/lib.rs`:

- Replace the `purge_logical_id` and `restore_logical_id`
  `not_implemented` fall-through branches in `run_recover` with
  `wire_recover` calls invoking the new engine methods.
- Add JSON serializers
  `purge_logical_id_report_json(&PurgeLogicalIdReport) -> Value` and
  `restore_logical_id_report_json(...)` next to the existing
  `excise_report_json` / `truncate_wal_report_json`. Top-level
  `verb` discriminator is `purge-logical-id` and `restore-logical-id`
  respectively (matches the CLI sub-flag spelling).

JSON shape MUST mirror the engine report fields one-to-one, with a
`verb` discriminator prepended.

### 5. Tests

TDD: red-green-refactor. **Capture the failing red run in the
stream log explicitly** (reviewer finding #2 from 10b-A). Run
`cargo test ... -- --no-capture` on the failing assertion at least
once and let the failure land in the log before implementing the
green path.

Engine integration tests (new files alongside the 10b-A test files):

- `src/rust/crates/fathomdb-engine/tests/purge_logical_id.rs`:
  - happy path: seed two logical ids, purge one, assert canonical
    - projection rows for the purged id are gone, sibling id
      untouched, `append_only_log` rows preserved.
  - `LogicalIdNotFound`: purge unknown id → `status = NoOp`,
    zero counters, no error.
  - cascade: superseded rows for the purged id are also deleted
    (not only the active version).
- `src/rust/crates/fathomdb-engine/tests/restore_logical_id.rs`:
  - happy path: write → purge → restore; assert
    `events_replayed > 0`, canonical row restored, projection
    rebuilt, `restore_provenance` marker present.
  - `NoRestorationSource`: restore an id with no
    `append_only_log` history → typed error.
  - `ConflictingActiveRow`: restore an id whose active row was NOT
    purged → typed error.
  - `IncompatibleSchema`: (only if reachable without a contrived
    schema mutation; if reaching this variant requires
    test-only schema munging that does not exist today, write a
    unit test of the variant's stable_code mapping instead and
    document the gap.)

CLI integration tests in
`src/rust/crates/fathomdb-cli/tests/recovery_cli.rs`:

- `recover --accept-data-loss --purge-logical-id <id>`: happy path
  asserts exit 64, JSON `verb: "purge-logical-id"`, key set matches
  the report. Refusal-without-`--accept-data-loss` case asserts
  exit 70 with the standard refusal envelope.
- `recover --accept-data-loss --restore-logical-id <id>`: same
  shape; additionally assert a `NoRestorationSource` invocation
  emits the typed error code.

Cite acceptance ids in test names / module docs: at minimum
`AC-058`. The cascade-correctness tests do not bind a numbered AC
today; reference the spec sections directly in the test module doc
(`dev/design/recovery.md § Logical-id purge and restore`).

## Required commands

Run inside the worktree (`$WT`):

```bash
cd "$WT"
# Inner loop while developing — fast feedback per crate
cargo test -p fathomdb-engine -p fathomdb -p fathomdb-cli
# Canonical gate (lint → typecheck → test). MUST pass before commit.
./scripts/agent-verify.sh
```

If `agent-verify.sh` surfaces a failure unrelated to this slice
(e.g. the AC-029 flake that hit 10b-A on first run), retry once. If
it persists, surface and stop.

## Discipline

- TDD: red-green-refactor per `feedback_tdd.md`. **Capture the red
  run in the stream log** — finding #2 of 10b-A reviewer verdict
  flagged absent red-step evidence; do not repeat.
- No data-migration shim, no 0.5.x compat, no soak. Per
  `feedback_reliability_principles.md`.
- If the writer path needs a `restore_provenance` column and schema
  bump, surface BEFORE adding migration code. The HITL has not
  pre-approved a schema bump in this slice.
- No `_`-prefixed unused-var hacks per
  `feedback_file_deletion.md` / general AGENTS.md guidance.
- Comment policy: no WHAT comments; only non-obvious WHY. No
  "10b-B" markers.
- Cite acceptance / spec ids in test names.

## Output

After all commands pass, write
`dev/plans/runs/10b-B-purge-restore-output.json`:

```json
{
  "phase": "10b-B",
  "baseline_sha": "e18747a",
  "branch": "phase-10b-B-purge-restore-<ts>",
  "head_sha": "<HEAD after final commit>",
  "seams_added": ["Engine::purge_logical_id", "Engine::restore_logical_id"],
  "engine_error_variants_added": ["NoRestorationSource", "ConflictingActiveRow", "IncompatibleSchema"],
  "facade_reexports_added": ["PurgeLogicalIdReport", "PurgeLogicalIdStatus", "RestoreLogicalIdReport", "RestoreLogicalIdStatus"],
  "schema_bump": "none | added column restore_provenance @ schema v??",
  "tests_added": ["<test names>"],
  "spec_sections_bound": ["recovery.md § Logical-id purge and restore", "engine.md § Canonical identity and supersession"],
  "acceptance_ids_bound": ["AC-058"],
  "red_runs_captured": ["<grep markers from log>"],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "spawn 10b-B reviewer"
}
```

Then stop. Do not run the reviewer yourself. Do not advance to
Phase 10b close-note.
