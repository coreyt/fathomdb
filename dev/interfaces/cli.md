---
title: CLI Public Interface
date: 2026-05-12
target_release: 0.6.0
desc: Public CLI surface for 0.6.0
blast_radius: src/rust/crates/fathomdb-cli/src/lib.rs; design/recovery.md; design/errors.md
status: locked
---

# CLI Interface

Public CLI surface for the 0.6.0 operator binary. The canonical verb table and
recovery semantics are owned by `design/recovery.md`; this file owns concrete
flag spelling, root command paths, and exit-code classes.

## Roots

- `fathomdb recover --accept-data-loss <sub-flag>...`
- `fathomdb doctor <verb> ...`

The CLI is **operator-only**. It does not mirror the SDK five-verb application
surface and does not ship `search` / `get` / `list` query verbs.

The 0.8.0 `doctor dump-mutations` verb is **not** an exception to this: it is a
read-only operator **diagnostic over the mutation log** (`operational_mutations`),
in the same `dump-*` family as `dump-row-counts` / `dump-schema` / `trace`, reading
op-store rows back over the existing engine read seam. It is distinct from — and
does not introduce — the still-absent `search` / `get` / `list` application query
surface over `canonical_nodes` (Option B in `ADR-0.6.0-cli-scope`, still rejected;
see that ADR's 2026-06-06 amendment).

## Output posture

- `--json` is the normative machine-readable contract on every verb.
- `doctor check-integrity` emits a single JSON object.
- `doctor check-integrity --full` may emit doctor-only finding codes such as
  `E_CORRUPT_INTEGRITY_CHECK`.
- `recover` JSON output is a progress stream plus summary, owned by
  `design/recovery.md`.
- `--pretty` is a human-only formatter on verbs that explicitly document it;
  it is not a separate machine schema.

## Exit-code classes

| Code | Stable meaning                                                       | Primary owner                    |
| ---- | -------------------------------------------------------------------- | -------------------------------- |
| `0`  | successful completion with no findings that require a non-zero exit  | this file                        |
| `64` | recovery completed only because lossy action was explicitly accepted | this file + `design/recovery.md` |
| `65` | doctor/verification surface found actionable non-clean state         | this file + `design/recovery.md` |
| `66` | export/materialization failure on an artifact-producing doctor verb  | this file + `design/recovery.md` |
| `70` | unrecoverable command failure                                        | this file                        |
| `71` | lock-held or equivalent precondition-blocked outcome                 | this file + `design/bindings.md` |

## Doctor verbs

| Verb              | Synopsis                                                                       | Exit class                          |
| ----------------- | ------------------------------------------------------------------------------ | ----------------------------------- |
| `check-integrity` | `fathomdb doctor check-integrity [--quick] [--full] [--round-trip] [--pretty]` | `doctor-check-*` = 0 / 65 / 70 / 71 |
| `safe-export`     | `fathomdb doctor safe-export <out> [--manifest <path>]`                        | `doctor-export-*` = 0 / 66 / 71     |
| `verify-embedder` | `fathomdb doctor verify-embedder --identity <s> --dimension <n>`               | `doctor-check-*` = 0 / 65           |
| `trace`           | `fathomdb doctor trace --source-ref <id>`                                      | `doctor-check-*`                    |
| `dump-schema`     | `fathomdb doctor dump-schema`                                                  | `doctor-check-*`                    |
| `dump-row-counts` | `fathomdb doctor dump-row-counts`                                              | `doctor-check-*`                    |
| `dump-profile`    | `fathomdb doctor dump-profile`                                                 | `doctor-check-*`                    |
| `recompute-mean`  | `fathomdb doctor recompute-mean <db_path> [--json]`                            | `doctor-check-*` = 0 / 70 / 71      |
| `dump-mutations`  | `fathomdb doctor dump-mutations <collection> [--after-id <n>] [--limit <n>] [--json] <db_path>` | `0 / 70 / 71`      |

`doctor-check-*` means the verb may use the exit-code class set `{0, 65, 70,
71}` depending on clean/findings/unrecoverable/lock-held outcome.

`dump-mutations` (0.8.0; gap F4-READ / reserved-gap-34) is a read-only operator
diagnostic that pages op-store (`operational_mutations`) rows for one
`append_only_log` collection back over the existing `read.collection` /
`read.mutations` engine seam (Slice 30, index-driven by Slice 33). It is
**CLI-only** (no SDK-parity obligation; the SDK `read.*` verbs are the separate
application surface). An **empty page** — empty / unknown / unregistered
collection, or `--after-id` past the end — is a normal absence and exits `0`
(never `65`/Findings). Exit class set `{0, 70, 71}`.

## Recover root

`recover` is the only lossy / non-bit-preserving root in 0.6.0.

```text
fathomdb recover --accept-data-loss
  [--truncate-wal]
  [--rebuild-vec0]
  [--rebuild-projections]
  [--excise-source <id>]
```

Exit class: `recover-*` = 0 / 64 / 70 / 71.

`--accept-data-loss` is declared on the `recover` parser only. `doctor` verbs
reject it as unknown.

`--rebuild-projections` is the canonical 0.6.0 regenerate workflow for failed
or stale projections. The docs may refer to "regenerate" as the workflow name,
but there is no separate `fathomdb regenerate` command in 0.6.0.

## Error to exit-code mapping

The CLI dispatcher translates engine error variants (and CLI-detected
preconditions) to the exit-code classes above. This table binds each variant
to its class.

| Source error variant                             | Exit code | Class         |
| ------------------------------------------------ | --------- | ------------- |
| (clean completion)                               | 0         | success       |
| recover sub-action gated by `--accept-data-loss` | 64        | data-loss-ack |
| `doctor check-integrity` findings non-empty      | 65        | findings      |
| `doctor safe-export` failed manifest/export step | 66        | artifact-fail |
| `EngineError::Storage`                           | 70        | unrecoverable |
| `EngineError::Projection`                        | 70        | unrecoverable |
| `EngineError::Vector`                            | 70        | unrecoverable |
| `EngineError::Embedder`                          | 70        | unrecoverable |
| `EngineError::Scheduler`                         | 70        | unrecoverable |
| `EngineError::OpStore`                           | 70        | unrecoverable |
| `EngineError::Overloaded`                        | 70        | unrecoverable |
| `EngineError::SchemaValidation`                  | 70        | unrecoverable |
| `EngineError::WriteValidation`                   | 70        | unrecoverable |
| `EngineError::EmbedderNotConfigured`             | 70        | unrecoverable |
| `EngineError::KindNotVectorIndexed`              | 70        | unrecoverable |
| `EngineError::EmbedderDimensionMismatch{..}`     | 70        | unrecoverable |
| `EngineOpenError::DatabaseLocked{..}`            | 71        | lock-held     |
| `EngineError::Closing`                           | 71        | lock-held     |
| `EngineOpenError::Corruption(..)`                | 70        | unrecoverable |
| `EngineOpenError::IncompatibleSchemaVersion{..}` | 70        | unrecoverable |
| `EngineOpenError::MigrationError{..}`            | 70        | unrecoverable |
| `EngineOpenError::EmbedderIdentityMismatch{..}`  | 70        | unrecoverable |
| `EngineOpenError::EmbedderDimensionMismatch{..}` | 70        | unrecoverable |
| `EngineOpenError::Io{..}`                        | 70        | unrecoverable |

## JSON output wrapping

`fathomdb-cli` owns top-level discriminator wrapping. The engine returns
typed report structs; the CLI serializes them under a `verb` discriminator.

- All `--json` output is one JSON object (or an NDJSON stream for `recover`).
- Doctor verb wrapping pattern: `{ "verb": "<verb-name>", ...flattened_engine_report_fields... }`.
- Non-flat reports nest naturally. For example, `IntegrityReport` serializes
  as `{ "verb": "check-integrity", "physical": {...}, "logical": {...}, "semantic": {...} }`.
- `doctor dump-mutations` (0.8.0) serializes as `{ "verb": "dump-mutations",
  "collection", "after_id" (or null), "limit", "count", "rows": [ { "id",
  "collection", "record_key", "op_kind", "payload", "schema_id", "write_cursor" }
  … ordered by`id`], "next_after_id" }`. The CLI serializes the engine
  `OpStoreRow` rows inline; `next_after_id` is the last row's `id` iff a full page
  was returned (resume with `--after-id <next_after_id>`, exclusive cursor → no
  overlap), else `null`.
- Field name policy: serde default `snake_case`. Any divergence from engine
  field spellings lives in the CLI serialization layer; the engine report
  structs are not renamed to satisfy CLI spelling requirements.
