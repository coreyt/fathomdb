---
title: Recovery Subsystem Design
date: 2026-05-12
target_release: 0.6.0
desc: Canonical operator CLI verb table, corruption handling workflow, and check-integrity output contract
blast_radius: fathomdb-cli; requirements REQ-035..REQ-040, REQ-054; acceptance AC-035d, AC-039*, AC-040*, AC-042..AC-044
status: locked
---

# Recovery Design

This file owns the 0.6.0 operator surface for corruption inspection, export,
and recovery.

## Two-root CLI split

0.6.0 recovery tooling splits at the root by mutation semantics:

| Root                                                | Surface                                                                                                                                     | Mutation class             |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| `fathomdb doctor <verb>`                            | `check-integrity`, `safe-export`, `verify-embedder`, `trace`, `dump-schema`, `dump-row-counts`, `dump-profile`                              | bit-preserving / read-only |
| `fathomdb recover --accept-data-loss <sub-flag>...` | `--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`, `--excise-source <id>`, `--purge-logical-id <id>`, `--restore-logical-id <id>` | lossy / non-bit-preserving |

`--accept-data-loss` is root-level and mandatory on `recover`. It is not valid
on `doctor` verbs.

`recover --rebuild-projections` is the canonical 0.6.0 regenerate workflow for
projection repair. The corpus may use "regenerate" as the workflow name, but it
does not imply a separate CLI root or verb outside the accepted `recover`
surface.

## Machine-readable output

`--json` is the normative machine-readable contract on every CLI verb.

- `doctor check-integrity` emits a **single JSON object** with top-level keys
  `physical`, `logical`, and `semantic`.
- `doctor safe-export`, `doctor verify-embedder`, `doctor trace`, and
  `doctor dump-*` emit one machine-readable JSON object per invocation.
- `recover` emits a machine-readable progress stream plus terminal summary.

`--pretty` is an optional human formatter on verbs that define it. It is not a
second machine contract.

Acceptance note:

- `doctor check-integrity` is the only CLI JSON shape independently acceptance-
  locked today.
- The remaining shapes below are design-owned 0.6.0 public contracts. They may
  gain additive fields later, but the named top-level keys here should not be
  removed or repurposed without updating both this file and `interfaces/cli.md`.

## `check-integrity` schema owner

The canonical `doctor check-integrity` JSON report owns:

- top-level keys: `physical`, `logical`, `semantic`
- per-finding fields: `code`, `stage`, `locator`, `doc_anchor`, `detail`
- single process exit code semantics for clean / findings / unrecoverable /
  lock-held outcomes

This design follows HITL `R9`; NDJSON is not the default `check-integrity`
contract in 0.6.0.

## JSON shapes for other doctor verbs

Each non-`recover` machine-readable verb returns one JSON object with a stable
`verb` discriminator plus verb-owned keys.

| Verb                     | Required top-level keys                                                                            | Notes                                                                             |
| ------------------------ | -------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `doctor safe-export`     | `verb`, `export_path`, `manifest_path`, `manifest_sha256`                                          | one object describing the completed export artifact and manifest                  |
| `doctor verify-embedder` | `verb`, `stored_identity`, `stored_dimension`, `supplied_identity`, `supplied_dimension`, `status` | `status` is a typed match/mismatch result, not free text                          |
| `doctor trace`           | `verb`, `source_ref`, `events`                                                                     | `events` is an ordered machine-readable lineage list for the requested source ref |
| `doctor dump-schema`     | `verb`, `user_version`, `tables`, `indexes`                                                        | schema inventory only; no recovery mutation                                       |
| `doctor dump-row-counts` | `verb`, `counts`                                                                                   | `counts` is an array of `{ name, rows }` records                                  |
| `doctor dump-profile`    | `verb`, `embedder_identity`, `embedder_dimension`, `vectorized_kinds`                              | stored profile / vector posture dump                                              |

## Logical-id purge and restore

`fathomdb recover --accept-data-loss --purge-logical-id <id>` and
`--restore-logical-id <id>` operate on the canonical identity model owned
by `design/engine.md § Canonical identity and supersession`. This section
specifies their cascade scope, replay source, output report shape, and
failure modes.

### Cascade scope (`--purge-logical-id`)

Purge is lossy. The cascade scope is:

- **Canonical:** delete every physical row (`row_id`) matching the
  `logical_id`, across all canonical tables and kinds. Includes both
  active and superseded rows for that logical id.
- **`latest_state` op-store collections:** delete keyed entries for the
  logical id. `latest_state` is the current-state map and must not retain
  stale references.
- **Derived projections:** drop FTS rows and per-kind vector rows
  attributable to the logical id (the rows are no longer recoverable from
  canonical state).
- **`append_only_log` op-store collections:** **preserved**. The event
  stream is durable audit history and is the replay source for
  `--restore-logical-id`. Purge does not rewrite history.
- **Other sources:** untouched. Rows associated with sibling source ids
  remain. The non-perturbation rule from Pack B (`excise_source`) applies:
  no blanket projection rebuild; invalidate only the affected logical id.

Report shape (`PurgeLogicalIdReport`):

- `verb`
- `logical_id`
- `canonical_rows_deleted` — count, sum across kinds
- `latest_state_keys_deleted` — count
- `projection_rows_invalidated` — count, sum of FTS + vector rows
- `append_only_log_rows_preserved` — count, informational
- `status` — typed success / no-op (logical id not present) / partial

Failure modes:

- `LogicalIdNotFound` — id matches no canonical row and no `latest_state`
  key. Returns `status = no_op`, not a hard error.
- Standard transactional rollback on any sub-step failure; no partial
  purge state persists.

### Replay source and semantics (`--restore-logical-id`)

Restore replays the `append_only_log` write history for the logical id
back into canonical state through the standard writer:

1. Read the ordered `append_only_log` events for the logical id.
2. Verify no active canonical row currently exists for the logical id
   (precondition for restore — restore does not overwrite live state).
3. Replay the event stream through the writer, producing canonical rows
   and supersession markers identical to the original write history,
   except the final active row carries a new `restore_provenance` field
   marking it as restoration output.
4. Re-derive projections (FTS + vector) from the restored active state.
5. Commit as one transaction.

Report shape (`RestoreLogicalIdReport`):

- `verb`
- `logical_id`
- `events_replayed` — count
- `canonical_rows_restored` — count, sum across kinds
- `projection_rows_rebuilt` — count
- `restore_cursor` — the `c_w` of the commit
- `status` — typed success / failure variant

Failure modes (typed; not free-text):

- `NoRestorationSource` — `append_only_log` has no events for the logical
  id. Restore is impossible.
- `ConflictingActiveRow` — a canonical row with `superseded_at IS NULL`
  already exists for the logical id. Operator must purge first or accept
  the existing row.
- `IncompatibleSchema` — replay would write columns or kinds no longer
  present in the current schema. Restore aborts; the operator must
  migrate manually.
- Standard transactional rollback on writer / projection failures.

Restore is reachable only via `recover --accept-data-loss
--restore-logical-id <id>`; it is not an SDK runtime verb, consistent
with the `recover` boundary in `ADR-0.6.0-cli-scope`.

## `recover` machine-readable output

`recover --json` is the only NDJSON-style machine-readable surface in 0.6.0.

Progress records carry:

- `action`
- `status`
- `detail`

Terminal summary carries:

- `status`
- `actions_applied`
- `accepted_data_loss`

`status` is machine-readable and distinguishes success, partial/lossy success,
and unrecoverable failure. `detail` is explanatory text or structured sub-data
owned by the action being reported.

### Doctor-only flags

`--quick`, `--full`, and `--round-trip` are doctor-only invocation flags in
0.6.0. They do not configure `Engine.open`, do not correspond to an SDK
`EngineConfig` knob, and do not imply an open-path opt-in integrity surface.

### Open-path always-on detection set

`Engine.open` always runs the frozen 0.6.0 corruption-detection subset below.
No env var, SDK config knob, doctor flag, or binding-specific option can turn
these checks on or off:

- WAL replay verdict
- page-1/header sanity probe
- schema / migration-table consistency probe
- stored embedder-profile identity probe

`design/errors.md` owns the exact `OpenStage`, `CorruptionKind`, locator, and
`RecoveryHint` table for those checks. This file owns the operator-facing
cadence rule: the checks above are always-on during open, while `--quick`,
`--full`, and `--round-trip` remain doctor-only diagnostics.

### Doctor finding codes vs `Engine.open` enums

`doctor check-integrity` findings use stable `code` values for machine
dispatch. Those `code` values may include checks that are not represented in
the `Engine.open` `CorruptionKind` / `OpenStage` enums.

- `CorruptionKind` / `OpenStage` = `Engine.open` structured error surface
- `code` = stable report / dispatch surface for bindings and doctor output

There is no 1:1 requirement between every doctor finding code and an open-path
corruption kind.

### Integrity-check full findings

`doctor check-integrity --full` retains the dedicated page-damage finding code
`E_CORRUPT_INTEGRITY_CHECK`.

- Surface: doctor report only
- Shape: normal finding record with `code`, `stage`, `locator`, `doc_anchor`,
  `detail`
- Non-surface: not an `Engine.open` `CorruptionKind`, not an `OpenStage`, and
  not evidence of any open-time integrity knob

### Recovery hint anchors

The open-path `RecoveryHint.doc_anchor` values cited from `design/errors.md`
resolve into the recovery workflow headings below.

#### Wal replay failures

Code: `E_CORRUPT_WAL_REPLAY`

Operator path: `fathomdb recover --accept-data-loss --truncate-wal`

#### Header malformed

Code: `E_CORRUPT_HEADER`

Operator path: `fathomdb doctor safe-export <out>` followed by external rebuild
or re-import.

#### Schema inconsistent

Code: `E_CORRUPT_SCHEMA`

Operator path: investigate with `doctor` and, where applicable, use
`fathomdb recover --accept-data-loss --rebuild-projections`.

#### Embedder identity drift

Code: `E_CORRUPT_EMBEDDER_IDENTITY`

Operator path: treat as corrupt stored profile state; do not auto-accept an
identity change through `Engine.open`.

### Code-to-operator-action cross-reference

| `RecoveryHint.code`           | Canonical owner of typed payload     | Operator path owner                         |
| ----------------------------- | ------------------------------------ | ------------------------------------------- |
| `E_CORRUPT_WAL_REPLAY`        | `design/errors.md`                   | this file, `#wal-replay-failures`           |
| `E_CORRUPT_HEADER`            | `design/errors.md`                   | this file, `#header-malformed`              |
| `E_CORRUPT_SCHEMA`            | `design/errors.md`                   | this file, `#schema-inconsistent`           |
| `E_CORRUPT_EMBEDDER_IDENTITY` | `design/errors.md`                   | this file, `#embedder-identity-drift`       |
| `E_CORRUPT_INTEGRITY_CHECK`   | doctor-only report code in this file | this file, `#integrity-check-full-findings` |

## Relationship to runtime SDK

Recovery and doctor tooling are unreachable from the runtime SDK. Python,
TypeScript, and the Rust facade do not expose SDK equivalents for any
`doctor` verb (`check-integrity`, `safe-export`, `verify-embedder`, `trace`,
`dump-schema`, `dump-row-counts`, `dump-profile`) or any `recover` sub-flag.

No SDK verb mutates a corruption-marked database. The only mutating recovery
path is `fathomdb recover --accept-data-loss`.

## Projection repair workflow

Projection failure handling crosses runtime and operator surfaces:

- runtime records exhausted projection failures durably in the op-store
  `projection_failures` collection
- operator diagnosis is based on those durable failure records and projection
  status
- repair is explicit via `fathomdb recover --accept-data-loss --rebuild-projections`

  0.6.0 does not promise an automatic background "heal failed projections"
  workflow at open, and it does not add a second repair command distinct from the
  accepted `recover` surface.
