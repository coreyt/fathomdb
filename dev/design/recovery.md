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

| Root                                                | Surface                                                                                                        | Mutation class             |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | -------------------------- |
| `fathomdb doctor <verb>`                            | `check-integrity`, `safe-export`, `verify-embedder`, `trace`, `dump-schema`, `dump-row-counts`, `dump-profile` | bit-preserving / read-only |
| `fathomdb recover --accept-data-loss <sub-flag>...` | `--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`, `--excise-source <id>`                            | lossy / non-bit-preserving |

`--accept-data-loss` is root-level and mandatory on `recover`. It is not valid
on `doctor` verbs.

`recover --rebuild-projections` is the canonical 0.6.0 regenerate workflow for
projection repair. The corpus may use "regenerate" as the workflow name, but it
does not imply a separate CLI root or verb outside the accepted `recover`
surface.

### Lock posture

All operator seams (doctor + recover) serialize on the engine's main
`Mutex<connection>`. Neither root uses the runtime reader pool; the reader
pool is runtime-only and is owned by `ReaderWorkerPool`.

- `doctor check-integrity`, `doctor safe-export`, and `doctor trace` hold
  an immutable lock and do not open a transaction.
- `recover --rebuild-projections`, `recover --rebuild-vec0`, and
  `recover --excise-source` hold a mutable lock plus a SQLite transaction.
  They freeze the projection runtime before acquiring the lock and unfreeze
  on commit or rollback.
- Concurrent `doctor` and `recover` invocations against the same engine
  block at the mutex. This is acceptable for operator workflows and matches
  the `ADR-0.6.0-single-writer-thread` invariant.

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

## Logical-id purge and restore — deferred to 0.8.0

HITL 2026-05-16: `--purge-logical-id` and `--restore-logical-id` are
removed from the 0.6.0 recover sub-flag set. Implementation was blocked
on substrate that does not exist in 0.6.0: canonical tables carry no
`logical_id` / `row_id` / `superseded_at` columns, the writer takes no
`logical_id`, and op-store collections (`latest_state`,
`append_only_log`) have no `logical_id` contract. The full canonical
identity substrate (`design/engine.md § Canonical identity and
supersession`) is design-only in 0.6.0 and must land before the two
verbs are buildable.

**Originally deferred to 0.7.x; moved to 0.8.0 per HITL 2026-05-24.**
0.7.0 is now perf-only (single thrust: AC-012/013/019/020 budget
revision + SQLite/FathomDB tuning). The canonical-identity substrate
plus the two CLI verbs land in 0.8.0 alongside the Memex
knowledge-store / retrieval features that consume the substrate
(`dev/roadmap/0.8.0.md`).

Re-introducing the verbs requires landing the identity substrate
first (schema bump on `canonical_nodes` / `canonical_edges`,
`PreparedWrite` field additions, writer supersession step, op-store
cascade contract). The cascade scope, replay source, report shapes,
and typed failure modes previously specified in this section are
preserved in version-control history and will be reinstated when
0.8.0 re-opens this scope.

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

All three flags are accepted on `doctor check-integrity`. `--full` activates
`PRAGMA integrity_check` in the engine (`Engine::check_integrity` honours
`opts.full`). `--quick` and `--round-trip` are accepted-as-default in 0.6.0:
the CLI emits no error, no warning, and no log event, and behaves identically
to the no-flag default path. Rationale: the flag surface is locked in
`interfaces/cli.md`, but the engine has no separate fast or round-trip path
in 0.6.0. Future 0.6.x / 0.7.0 may diverge behavior on these flags without
changing the locked flag surface.

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
