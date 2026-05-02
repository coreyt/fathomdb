---
title: Interfaces — Option 2
date: 2026-05-01
target_release: 0.6.0
desc: Normalized interface catalog (IF-###), public-surface-first canonical owners
status: living
---

# Interfaces

Catalog of documented contracts, ordered with public-facing entries first.
The "Canonical owning doc" is selected per the public-surface-first rule:
the doc that defines the caller-visible behavior, symbol set, event
schema, or machine-readable output. Internal-subsystem owners are
referenced under "Other referencing docs" only after the public surface
is established.

Where an interface is currently a TBD/empty stub, it is recorded with
"Contract summary: stub" and the open questions reflect the missing
content.

---

## IF-001: Five-verb SDK surface (canonical set)

- **ID.** IF-001
- **Name.** Five-verb SDK surface (`Engine.open`, `admin.configure`,
  `write`, `search`, `close`).
- **Class.** Public API.
- **Producer.** bindings facade (per binding); engine implements
  semantics.
- **Consumer.** Application code in Rust, Python, TypeScript.
- **Direction.** Caller → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/bindings.md` § 1 (parity claim
  across SDK bindings; canonical verb set).
- **Other referencing docs.** `architecture.md` § 2 + § 10;
  `requirements.md` REQ-053; `acceptance.md` AC-057a;
  `interfaces/rust.md` (stub); `interfaces/python.md` (stub);
  `interfaces/typescript.md` (stub).
- **Contract summary.** Every SDK binding's public top-level surface is
  exactly the five verbs in idiomatic casing. Adding a verb requires
  updating all SDK bindings together. SDK surface MUST NOT contain any
  name in `{recover, restore, repair, fix, rebuild, doctor}`.
- **Key types/fields/enums/errors.** Verb names; parity invariant;
  recovery-name exclusion list.
- **Requirement/AC refs.** REQ-053, REQ-037, REQ-054, REQ-031d;
  AC-057a, AC-035d, AC-041.
- **Evidence.** `design/bindings.md` § 1 ("Surface-set parity
  invariant"); `interfaces/cli.md` § Roots ("operator-only in 0.6.0;
  does not mirror the SDK five-verb application surface").
- **Open questions.** Per-language symbol spelling not yet committed
  (`interfaces/{rust,python,typescript}.md` are stubs).

## IF-002: `Engine.open` open-path contract

- **ID.** IF-002
- **Name.** `Engine.open` ordered open-path stages.
- **Class.** Public API.
- **Producer.** engine (runtime module).
- **Consumer.** All SDK callers; CLI.
- **Direction.** Caller → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/engine.md` § Open path.
- **Other referencing docs.** `architecture.md` § 2; `design/bindings.md`
  § 7 (lock acquisition); `design/errors.md` § Engine.open corruption
  table; `requirements.md` REQ-031d, REQ-042, REQ-043, REQ-044;
  `acceptance.md` AC-035a/b/c, AC-046a/b/c, AC-047, AC-048, AC-048b.
- **Contract summary.** Eight-step ordered sequence: path
  canonicalization; sidecar lock acquisition; SQLite open + PRAGMA
  application; always-on corruption detection; migration execution;
  embedder identity check; embedder warmup; writer/scheduler startup.
  Frozen always-on detection subset; no SDK config turns quick / full /
  round-trip integrity on at open. On corruption: refuse fail-closed
  with `EngineOpenError::Corruption`; lock released; no `Engine` handle
  returned.
- **Key types/fields/enums/errors.** `EngineOpenError`;
  `CorruptionDetail`; `OpenStage` (corruption-emitting subset:
  `WalReplay`, `HeaderProbe`, `SchemaProbe`, `EmbedderIdentity`);
  `MigrationError`; `IncompatibleSchemaVersion`;
  `EmbedderIdentityMismatch`; `EmbedderDimensionMismatch`;
  `DatabaseLocked`; `VectorExtensionUnavailable`.
- **Requirement/AC refs.** REQ-020a/b, REQ-022a/b, REQ-031c, REQ-031d,
  REQ-033, REQ-041, REQ-042, REQ-043, REQ-044, REQ-051; AC-022a,
  AC-024a/b, AC-035, AC-035a/b/c/d, AC-037, AC-046a/b/c, AC-047,
  AC-048, AC-048b, AC-055.
- **Evidence.** `design/engine.md` § Open path (steps 1–8);
  `requirements.md` REQ-031d ("On failure, no `Engine` handle is
  returned; the exclusive WAL lock is released; ...");
  `design/errors.md` § Engine.open corruption table.
- **Open questions.** `OpenStage` complete enum (including non-
  corruption-emitting stages such as `LockAcquisition`) not enumerated
  in the corpus — only the corruption-emitting subset is named.
  `EngineConfig` knob set incomplete.

## IF-003: `Engine.write(&[PreparedWrite])` batch contract

- **ID.** IF-003
- **Name.** Typed batch write submission.
- **Class.** Public API.
- **Producer.** engine writer.
- **Consumer.** All SDK callers (Rust, Python, TS).
- **Direction.** Caller → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/engine.md` § Batch submission
  semantics.
- **Other referencing docs.** `design/bindings.md` § 4 (marshalling);
  ADR-0.6.0-prepared-write-shape; ADR-0.6.0-typed-write-boundary;
  `architecture.md` § 3.
- **Contract summary.** Single ordered submission per call. Slice
  validated as one batch; on validation failure no SQLite tx commits;
  on success, batch executed in caller order in one SQLite tx; mixed
  canonical and op-store rows commit atomically; one write cursor
  `c_w` allocated for the batch; projection work enqueued only after
  the canonical commit. No regroup, split, or partial-success surface.
- **Key types/fields/enums/errors.** `PreparedWrite` enum
  (`Node | Edge | OpStore(OpStoreInsert) | AdminSchema(AdminSchemaWrite)`);
  `WriteReceipt { cursor: c_w, ... }`; errors:
  `WriteValidationError`, `SchemaValidationError`, `EmbedderError`,
  `OverloadedError`, `ClosingError`.
- **Requirement/AC refs.** REQ-009a/b, REQ-019, REQ-027, REQ-028a/b/c,
  REQ-053, REQ-055, REQ-056, REQ-057; AC-011a/b, AC-029, AC-030a/b/c,
  AC-059b, AC-060a/b, AC-061a/b/c.
- **Evidence.** `design/engine.md` § Batch submission semantics
  ("(1) The slice is validated as one batch ... (5) One write cursor
  `c_w` is allocated for the committed batch as a whole, not per
  element."); `design/bindings.md` § 4.
- **Open questions.** `WriteReceipt` field set beyond `cursor` not
  enumerated in the corpus. `AdminSchemaWrite` field set still owned
  by `design/engine.md` and "the interface docs" but not enumerated.

## IF-004: `Engine.search` retrieval result contract

- **ID.** IF-004
- **Name.** Search result with cursor + soft-fallback.
- **Class.** Public API.
- **Producer.** engine retrieval pipeline.
- **Consumer.** All SDK callers.
- **Direction.** Engine → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `architecture.md` § 4 step 5
  (return shape) plus `design/retrieval.md` (per architecture.md § 2;
  not in this option's read set; cited).
- **Other referencing docs.** `design/bindings.md` § 4
  (engine returns owned typed result rows; no lazy cursors crossing
  FFI); `requirements.md` REQ-029, REQ-055; `acceptance.md` AC-031,
  AC-059a.
- **Contract summary.** Read-tx exposes monotonic non-decreasing
  `projection_cursor`. Hybrid retrieval that loses one branch returns
  results plus a typed soft-fallback record naming the missed branch.
- **Key types/fields/enums/errors.** `Search { results,
  projection_cursor: c_r, soft_fallback: Option<...> }`;
  soft-fallback `branch` field with values including `Vector`.
- **Requirement/AC refs.** REQ-018, REQ-029, REQ-055; AC-020, AC-031,
  AC-059a/b.
- **Evidence.** `architecture.md` § 4 ("Return Search { results,
  projection_cursor: c_r, soft_fallback: Option<...> }");
  `acceptance.md` AC-031 (soft-fallback record `branch` field == `Vector`).
- **Open questions.** Soft-fallback record exact field set "owned by
  binding-interface ADRs" (REQ-029) but bindings interfaces are stubs.
  `results` row shape not enumerated.

## IF-005: Cursor contract (`c_w` and `projection_cursor`)

- **ID.** IF-005
- **Name.** Two-cursor public contract.
- **Class.** Data/schema/payload.
- **Producer.** engine writer (allocates `c_w`); engine projection
  (advances `projection_cursor`).
- **Consumer.** All SDK callers polling for vector read-after-write
  semantics.
- **Direction.** Engine → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/engine.md` § Cursor contract.
- **Other referencing docs.** `architecture.md` § 3 + § 4;
  `requirements.md` REQ-055; `acceptance.md` AC-059a/b.
- **Contract summary.** `c_w` returned on write commit identifies the
  accepted canonical write transaction; `projection_cursor` returned on
  read-tx identifies the latest projection-visible point. Canonical and
  FTS visibility immediate at `c_w`. Vector visibility: caller polls
  read-tx until `read_projection_cursor >= c_w`.
- **Key types/fields/enums/errors.** `WriteReceipt.cursor` (`c_w`);
  read-tx `projection_cursor` (`c_r`).
- **Requirement/AC refs.** REQ-055; AC-059a/b.
- **Evidence.** `design/engine.md` § Cursor contract ("This distinction
  is load-bearing for REQ-055 and AC-059b and must remain consistent
  across architecture.md, requirements.md, acceptance.md, and
  interfaces/*.md."); `architecture.md` § 3 step 2 + § 4 step 2.
- **Open questions.** Cursor type (integer? opaque?) not declared in
  the corpus.

## IF-006: PreparedWrite typed write boundary

- **ID.** IF-006
- **Name.** `PreparedWrite` enum.
- **Class.** Data/schema/payload.
- **Producer.** bindings facade (constructs from idiomatic input).
- **Consumer.** engine writer.
- **Direction.** Caller → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/bindings.md` § 4 (marshalling
  protocol; ADR-0.6.0-prepared-write-shape; ADR-0.6.0-typed-write-boundary).
- **Other referencing docs.** `design/engine.md` § PreparedWrite::AdminSchema
  provenance; `architecture.md` § 3.
- **Contract summary.** `PreparedWrite` is the only typed shape the
  engine accepts. Variants: `Node`, `Edge`, `OpStore(OpStoreInsert)`,
  `AdminSchema(AdminSchemaWrite)`. No raw SQL accepted. Vector input is
  zero-copy when LE-f32 contiguous.
- **Key types/fields/enums/errors.** `PreparedWrite` variants;
  `OpStoreInsert { schema_id, payload, ... }`;
  `AdminSchemaWrite { ... }`; `WriteValidationError`;
  `SchemaValidationError`.
- **Requirement/AC refs.** REQ-053, REQ-056, REQ-057;
  AC-060a/b, AC-061a/b/c.
- **Evidence.** `design/bindings.md` § 4 ("PreparedWrite (per
  ADR-0.6.0-prepared-write-shape) is the only typed shape the engine
  accepts."); `design/engine.md` § PreparedWrite::AdminSchema
  provenance.
- **Open questions.** Field shape of every variant not enumerated in
  any interface file. `AdminSchemaWrite` field set TBD.

## IF-007: Lifecycle phase enum

- **ID.** IF-007
- **Name.** Response-cycle phase tag.
- **Class.** Event/observability.
- **Producer.** engine emission sites; lifecycle module.
- **Consumer.** Host subscriber via every binding.
- **Direction.** Engine → host.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/lifecycle.md` § Phase enum.
- **Other referencing docs.** `design/bindings.md` § 8;
  `requirements.md` REQ-001, REQ-006a/b; `acceptance.md` AC-001,
  AC-008.
- **Contract summary.** Every lifecycle event carries a typed `phase`
  field with one of `{Started, Slow, Heartbeat, Finished, Failed}`.
  `Finished` and `Failed` mutually exclusive; no event after a
  terminal phase for the same operation.
- **Key types/fields/enums/errors.** `phase` field; enum values listed.
- **Requirement/AC refs.** REQ-001, REQ-006b; AC-001, AC-008.
- **Evidence.** `design/lifecycle.md` § Phase enum ("The lifecycle
  phase enum in 0.6.0 has exactly five values").
- **Open questions.** Non-phase event envelope (timing fields,
  operation identity) "owned by the producing surface or binding
  contract" — not enumerated.

## IF-008: Host-routed diagnostics (source / category)

- **ID.** IF-008
- **Name.** Diagnostic source + category typed tags.
- **Class.** Event/observability.
- **Producer.** engine; SQLite-internal capture.
- **Consumer.** Host subscriber.
- **Direction.** Engine → host.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/lifecycle.md` § Host-routed
  diagnostics.
- **Other referencing docs.** `design/bindings.md` § 8;
  `requirements.md` REQ-002, REQ-005; `acceptance.md` AC-002,
  AC-003a/b/c/d, AC-006.
- **Contract summary.** Structured diagnostics carry `source ∈
  {Engine, SqliteInternal}` and `category` from the source's stable
  set. Engine categories: `{writer, search, admin, error}`. SQLite
  categories: `{corruption, recovery, io}`. SQLite-internal events
  must NOT bypass the host subscriber path. No private sink: when no
  subscriber is registered, the engine writes nothing.
- **Key types/fields/enums/errors.** `source` enum; `category` enum
  partitioned by source.
- **Requirement/AC refs.** REQ-002, REQ-005; AC-002, AC-003a/b/c/d,
  AC-006.
- **Evidence.** `design/lifecycle.md` § Host-routed diagnostics
  ("Stable `source` values: `Engine`, `SqliteInternal`").
- **Open questions.** Host-native required fields "MAY translate or
  derive" from engine event under a stable `fathomdb` payload key
  (`design/bindings.md` § 8) — exact derivation map not enumerated.

## IF-009: Counter snapshot

- **ID.** IF-009
- **Name.** Pull-surface counter snapshot.
- **Class.** Data/schema/payload (also Public API for the read call).
- **Producer.** lifecycle module.
- **Consumer.** Operator code via SDK.
- **Direction.** Engine → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/lifecycle.md` § Counter snapshot.
- **Other referencing docs.** `requirements.md` REQ-003;
  `acceptance.md` AC-004a/b/c.
- **Contract summary.** Cumulative since engine open; non-perturbing
  to the counters themselves; key set is exactly: `queries`, `writes`,
  `write_rows`, `errors_by_code`, `admin_ops`, `cache_hit`,
  `cache_miss`. `errors_by_code` is keyed by stable machine-readable
  error code.
- **Key types/fields/enums/errors.** Seven-key snapshot dict;
  `errors_by_code` is a sub-mapping by error code.
- **Requirement/AC refs.** REQ-003; AC-004a/b/c.
- **Evidence.** `design/lifecycle.md` § Public key set; AC-004a
  ("exact key-set equality").
- **Open questions.** SDK call signature for "read snapshot" not
  spelled in any interface file.

## IF-010: Per-statement profile record

- **ID.** IF-010
- **Name.** Profile-record opt-in surface.
- **Class.** Event/observability.
- **Producer.** lifecycle module.
- **Consumer.** Subscriber-delivered diagnostics or CLI dump tooling.
- **Direction.** Engine → host.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/lifecycle.md` § Per-statement
  profiling.
- **Other referencing docs.** `requirements.md` REQ-004;
  `acceptance.md` AC-005a/b.
- **Contract summary.** Runtime-toggleable; when disabled, lifecycle
  feedback still exists. Public profile record exposes typed numeric
  fields `wall_clock_ms`, `step_count`, `cache_delta`.
- **Key types/fields/enums/errors.** Three-field profile record.
- **Requirement/AC refs.** REQ-004; AC-005a/b.
- **Evidence.** `design/lifecycle.md` § Public record shape (AC-005b).
- **Open questions.** Toggle API call signature not spelled
  ("documented API call enables per-statement profiling on a running
  engine"; AC-005a). Transport (subscriber? CLI dump?) "owned outside
  this file" but no specific owner is named.

## IF-011: Stress-failure context

- **ID.** IF-011
- **Name.** Structured stress-failure event payload.
- **Class.** Event/observability.
- **Producer.** lifecycle module (engine emission sites).
- **Consumer.** Host subscriber.
- **Direction.** Engine → host.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/lifecycle.md` § Stress-failure
  context.
- **Other referencing docs.** `requirements.md` REQ-007;
  `acceptance.md` AC-009.
- **Contract summary.** Required fields: `thread_group_id`, `op_kind`,
  `last_error_chain`, `projection_state`. Subscriber-routable
  observability data, not ad hoc free-text metadata.
- **Key types/fields/enums/errors.** Four-field payload.
- **Requirement/AC refs.** REQ-007; AC-009.
- **Evidence.** `design/lifecycle.md` § Stress-failure context (AC-009
  enumeration).
- **Open questions.** Field types not specified beyond
  "structurally-typed object."

## IF-012: Migration per-step event

- **ID.** IF-012
- **Name.** Migration progress event.
- **Class.** Event/observability.
- **Producer.** migrations module (during `Engine.open`).
- **Consumer.** Host subscriber.
- **Direction.** Engine → host.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/migrations.md` (currently a single-
  paragraph stub: "owns the migration loop ... per-step event contract,
  and the accretion-guard rules"). De-facto specification: AC-046b/c.
- **Other referencing docs.** `design/lifecycle.md` § Relation to other
  subsystem events; `design/bindings.md` § 11; `requirements.md`
  REQ-042; `acceptance.md` AC-046a/b/c.
- **Contract summary.** Successful migration emits one structured
  event per applied step containing `step_id` and `duration_ms`.
  Failed step emits event with `failed: true` and `duration_ms`
  populated; `Engine.open` returns typed `MigrationFailed` /
  `MigrationError`.
- **Key types/fields/enums/errors.** `step_id`, `duration_ms`,
  `failed` fields; `MigrationError` typed exception.
- **Requirement/AC refs.** REQ-042; AC-046a/b/c.
- **Evidence.** `acceptance.md` AC-046b ("contains `step_id` and
  `duration_ms` fields"), AC-046c (`failed: true`); `design/migrations.md`
  body (one paragraph: payload shape implicit by AC).
- **Open questions.** `design/migrations.md` is a stub: payload schema,
  field types, and the routing relationship to lifecycle phase are
  not yet enumerated. `MigrationFailed` vs `MigrationError` naming
  inconsistent across `acceptance.md` AC-046c (`MigrationFailed`) and
  `design/errors.md` (`MigrationError`).

## IF-013: Subscriber registration protocol

- **ID.** IF-013
- **Name.** Per-binding subscriber attachment.
- **Class.** Binding adapter.
- **Producer.** bindings facade (per binding).
- **Consumer.** Host application.
- **Direction.** Host → engine (registration); engine → host (events).
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/bindings.md` § 8 (cross-binding
  protocol). Per-binding registration call signature owned by
  `interfaces/{python,ts}.md` (stubs).
- **Other referencing docs.** `design/lifecycle.md`;
  `requirements.md` REQ-002; `acceptance.md` AC-002, AC-003a/b/c/d.
- **Contract summary.** Python: caller registers a `logging`-backed
  adapter via a binding-provided helper. TypeScript: caller registers
  a callback invoked per event. CLI: human mode = console subscriber;
  machine mode = `--json` per verb. No private sink without
  registration.
- **Key types/fields/enums/errors.** Engine event payload under stable
  `fathomdb` key.
- **Requirement/AC refs.** REQ-002, REQ-005; AC-002, AC-003a/b/c/d,
  AC-006.
- **Evidence.** `design/bindings.md` § 8 ("Python: caller registers a
  `logging`-backed adapter ... TypeScript: caller registers a
  callback ...").
- **Open questions.** Per-binding helper / callback call signature not
  spelled; helper name not committed.

## IF-014: Error taxonomy roots + variants

- **ID.** IF-014
- **Name.** `EngineError` / `EngineOpenError` taxonomy.
- **Class.** Public API (error surface).
- **Producer.** engine; per-module errors.
- **Consumer.** SDK callers via per-binding mapping.
- **Direction.** Engine → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/errors.md` § Top-level types +
  § Module taxonomy.
- **Other referencing docs.** `design/bindings.md` § 3 (mapping
  protocol); `interfaces/{python,ts,cli}.md` (stubs except cli);
  `requirements.md` REQ-056, REQ-031d; `acceptance.md` AC-035a/b/c/d,
  AC-060a.
- **Contract summary.** Two top-level Rust error types:
  `EngineError` (post-open runtime failures) and `EngineOpenError`
  (`Engine.open` failures). Per-module errors stay distinct
  (`StorageError`, `ProjectionError`, `VectorError`, `EmbedderError`,
  `SchedulerError`, `OpStoreError`, `WriteValidationError`,
  `SchemaValidationError`, `EmbedderIdentityMismatchError`,
  `MigrationError`). `OverloadedError` and `ClosingError` remain direct
  `EngineError` variants. Bindings map one class per variant; single
  rooted hierarchy per binding; typed attributes; no string parsing.
- **Key types/fields/enums/errors.** Module enums above; binding root
  `fathomdb.EngineError` (Python); `EngineError` (TS).
- **Requirement/AC refs.** REQ-031d, REQ-056; AC-035a/b/c/d, AC-060a.
- **Evidence.** `design/errors.md` § Module taxonomy table;
  `design/bindings.md` § 3 protocol commitments.
- **Open questions.** Variant→class mapping matrix is owned by
  `design/errors.md` per architecture.md § 2 but the matrix itself is
  not yet rendered (only the module taxonomy is enumerated). Per-
  language exception class names absent (`interfaces/{python,ts}.md`
  stubs). `MigrationError` vs `MigrationFailed` inconsistency
  (see IF-012).

## IF-015: CorruptionDetail open-path payload

- **ID.** IF-015
- **Name.** `EngineOpenError::Corruption(CorruptionDetail)`.
- **Class.** Data/schema/payload.
- **Producer.** engine open path.
- **Consumer.** SDK callers; CLI; doctor reports (shared `code`
  surface).
- **Direction.** Engine → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/errors.md` § Corruption detail
  owner + § Engine.open corruption table.
- **Other referencing docs.** `design/engine.md` § Open path;
  `design/recovery.md` § Recovery hint anchors; `design/bindings.md`
  § 3 + § 11; `requirements.md` REQ-031d; `acceptance.md` AC-035a/b/c.
- **Contract summary.** Carries `kind: CorruptionKind`,
  `stage: OpenStage`, `locator: CorruptionLocator`,
  `recovery_hint: RecoveryHint { code, doc_anchor }`. Corruption-emitting
  stages: `WalReplay`, `HeaderProbe`, `SchemaProbe`,
  `EmbedderIdentity`. `LockAcquisition` is never a corruption stage
  (AC-035b). `CorruptionLocator` includes `FileOffset`, `PageId`,
  `TableRow`, `Vec0ShadowRow`, `MigrationStep`, `OpaqueSqliteError`.
  `recovery_hint.code` stable: `E_CORRUPT_WAL_REPLAY`,
  `E_CORRUPT_HEADER`, `E_CORRUPT_SCHEMA`, `E_CORRUPT_EMBEDDER_IDENTITY`.
- **Key types/fields/enums/errors.** Five enums + struct (above).
- **Requirement/AC refs.** REQ-031d; AC-035a/b/c.
- **Evidence.** `design/errors.md` § Engine.open corruption table
  (the only canonical materialized join); AC-035b.
- **Open questions.** None — this is one of the most fully specified
  contracts in the corpus.

## IF-016: CLI two-root operator surface

- **ID.** IF-016
- **Name.** `fathomdb doctor <verb>` + `fathomdb recover ...`.
- **Class.** CLI/operator.
- **Producer.** fathomdb-cli binary.
- **Consumer.** Operators; CI scripts.
- **Direction.** Operator → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** `interfaces/cli.md` (concrete flag
  spelling, root command paths, exit-code classes); `design/recovery.md`
  (canonical verb table, semantic ownership, JSON shapes).
- **Other referencing docs.** `architecture.md` § 1 + § 2;
  `design/bindings.md` § 1 + § 10;
  `requirements.md` REQ-036, REQ-037, REQ-054, REQ-059;
  `acceptance.md` AC-040a/b, AC-041, AC-042, AC-043a/b/c, AC-044,
  AC-045, AC-058, AC-063c.
- **Contract summary.** Two roots split by mutation class.
  `recover --accept-data-loss` is the only lossy root; sub-flags
  `--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`,
  `--excise-source <id>`, `--purge-logical-id <id>`,
  `--restore-logical-id <id>`. `doctor` verbs:
  `check-integrity`, `safe-export`, `verify-embedder`, `trace`,
  `dump-schema`, `dump-row-counts`, `dump-profile`. `--json` normative.
  Exit-code classes:
  `doctor-check-*` = 0/65/70/71; `doctor-export-*` = 0/66/71;
  `recover-*` = 0/64/70/71. `--accept-data-loss` rejected by `doctor`.
- **Key types/fields/enums/errors.** Verb / flag set; exit-code
  classes; `--json` machine contract.
- **Requirement/AC refs.** REQ-036, REQ-037, REQ-054; AC-040a/b,
  AC-041, AC-042, AC-058, AC-063c.
- **Evidence.** `interfaces/cli.md` § Doctor verbs table + § Recover
  root; `design/recovery.md` § Two-root CLI split.
- **Open questions.** None at the CLI structural level. JSON shape
  for verbs other than `check-integrity` is the open item — see
  IF-017.

## IF-017: `doctor check-integrity` JSON report

- **ID.** IF-017
- **Name.** Single-object JSON integrity report.
- **Class.** Data/schema/payload.
- **Producer.** fathomdb-cli (`doctor check-integrity`).
- **Consumer.** Operators / CI scripts.
- **Direction.** CLI → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/recovery.md` § check-integrity
  schema owner.
- **Other referencing docs.** `interfaces/cli.md` § Output posture;
  `requirements.md` REQ-039; `acceptance.md` AC-043a/b/c.
- **Contract summary.** Single JSON object; top-level keys `physical`,
  `logical`, `semantic`. Each section holds a finding list (possibly
  empty) or `clean: true`. Per-finding fields: `code`, `stage`,
  `locator`, `doc_anchor`, `detail`. `--full` may emit
  `E_CORRUPT_INTEGRITY_CHECK`.
- **Key types/fields/enums/errors.** Three-section object; per-finding
  five-field record; doctor-only `code` set.
- **Requirement/AC refs.** REQ-039; AC-043a/b/c.
- **Evidence.** `design/recovery.md` § check-integrity schema owner;
  AC-043a/b/c.
- **Open questions.** Other doctor verb JSON shapes "remain owned here
  as the draft fills in" (`design/recovery.md`) — not yet specified.

## IF-018: `recover` machine-readable progress + summary

- **ID.** IF-018
- **Name.** `recover --json` progress stream + terminal summary.
- **Class.** Data/schema/payload.
- **Producer.** fathomdb-cli (`recover`).
- **Consumer.** Operators / CI scripts.
- **Direction.** CLI → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/recovery.md` § Machine-readable
  output ("progress stream plus terminal summary").
- **Other referencing docs.** `interfaces/cli.md` § Output posture
  (cites `design/recovery.md`); `acceptance.md` AC-058, AC-063c.
- **Contract summary.** Stream of progress events plus a final summary
  payload per invocation.
- **Key types/fields/enums/errors.** None spelled.
- **Requirement/AC refs.** REQ-036, REQ-054, REQ-059;
  AC-058, AC-063c.
- **Evidence.** `interfaces/cli.md` § Output posture ("`recover` JSON
  output is a progress stream plus summary, owned by
  `design/recovery.md`."); `design/recovery.md` § Machine-readable
  output.
- **Open questions.** Stream record schema, summary record schema, and
  field set are TBD.

## IF-019: Database lock contract (sidecar + SQLite EXCLUSIVE)

- **ID.** IF-019
- **Name.** Hybrid database file lock.
- **Class.** Public API (failure-mode + cross-process invariant).
- **Producer.** engine runtime.
- **Consumer.** Every binding; every CLI invocation.
- **Direction.** Caller → engine (open); engine → caller (rejection).
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/bindings.md` § 7
  (cross-binding lock contract); `architecture.md` § 5 (mechanism).
- **Other referencing docs.** ADR-0.6.0-database-lock-mechanism;
  `design/engine.md` § Open path;
  `requirements.md` REQ-020a, REQ-022a/b, REQ-041;
  `acceptance.md` AC-022a/b, AC-024a/b, AC-035c.
- **Contract summary.** Sidecar `{database_path}.lock` flock + SQLite
  `PRAGMA locking_mode=EXCLUSIVE` on writer connection. Reader
  connections use NORMAL locking_mode. Second open from any binding
  surfaces `DatabaseLocked { holder_pid }` before SQLite I/O begins.
  Lock lifetime bound to `Engine` instance; `Engine.close` releases
  it. Path canonicalization defeats symlink + bind-mount aliasing;
  bindings do not canonicalize. Corruption-on-open releases the lock
  before returning the error.
- **Key types/fields/enums/errors.** `DatabaseLocked { holder_pid:
  Option<u32> }`; sidecar file path convention.
- **Requirement/AC refs.** REQ-020a, REQ-022a/b; AC-022a/b, AC-024a/b,
  AC-035c.
- **Evidence.** `design/bindings.md` § 7 ("hybrid lock per
  ADR-0.6.0-database-lock-mechanism #30"); `architecture.md` § 5
  ("Hybrid lock: sidecar ... PLUS PRAGMA locking_mode=EXCLUSIVE").
- **Open questions.** None at the contract level.

## IF-020: Embedder protocol + identity invariant

- **ID.** IF-020
- **Name.** Embedder trait + cross-binding identity contract.
- **Class.** Public API (caller-supplied implementation surface).
- **Producer.** Caller (user-supplied embedder); fathomdb-embedder-api
  trait crate.
- **Consumer.** engine embedder dispatch pool.
- **Direction.** Caller → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** ADR-0.6.0-embedder-protocol +
  ADR-0.6.0-vector-identity-embedder-owned (cited; ADR docs not in
  this option's read set). Cross-binding manifestation: `design/bindings.md`
  § 5; `design/embedder.md` (cited by architecture.md § 2; not in
  read set).
- **Other referencing docs.** `architecture.md` § 1
  (`fathomdb-embedder-api` trait crate);
  `requirements.md` REQ-028a/b/c, REQ-033, REQ-044, REQ-047;
  `acceptance.md` AC-030a/b/c, AC-037, AC-048, AC-048b, AC-051a/b.
- **Contract summary.** Caller-supplied; engine never downloads model
  weights. `EmbedderIdentity` owned by the embedder, not per-DB
  config. Same DB across bindings resolves the same `EmbedderIdentity`.
  Mismatch surfaces as `EmbedderIdentityMismatchError` /
  `EmbedderDimensionMismatchError`. Engine watchdog enforces per-call
  timeout (Invariant D, default 30 s). Embedder is sync; runs on
  engine-owned pool only (Invariant B); no engine re-entrancy from
  `embed()` (Invariant C).
- **Key types/fields/enums/errors.** `Embedder` trait;
  `EmbedderIdentity`; `EmbedderError`,
  `EmbedderIdentityMismatchError`, `EmbedderDimensionMismatchError`,
  `EmbedderNotConfigured`, `KindNotVectorIndexed`.
- **Requirement/AC refs.** REQ-028a/b/c, REQ-033, REQ-044, REQ-047;
  AC-030a/b/c, AC-037, AC-048, AC-048b.
- **Evidence.** `design/bindings.md` § 5 ("vector identity is owned by
  the embedder, not by per-DB config"); `architecture.md` § 1
  (`fathomdb-embedder-api` semver-stable trait crate);
  `acceptance.md` AC-030a/b/c.
- **Open questions.** Trait shape + `EmbedderIdentity` field set live
  in `design/embedder.md` and the ADRs (out of read set); per-binding
  embedder protocol shape "owned by `interfaces/{python,ts}.md` and
  cited from `design/bindings.md`" — interface files are stubs.

## IF-021: Op-store collection registry

- **ID.** IF-021
- **Name.** Op-store collection metadata + write semantics.
- **Class.** Data/schema/payload.
- **Producer.** engine op-store module.
- **Consumer.** SDK callers via `PreparedWrite::OpStore`.
- **Direction.** Caller → engine; engine → caller (collection
  metadata).
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/op-store.md` (cited;
  not in this option's read set; ADR-0.6.0-op-store-same-file).
- **Other referencing docs.** `design/bindings.md` § 4
  (JSON-Schema cadence); `requirements.md` REQ-057, REQ-058;
  `acceptance.md` AC-061a/b/c, AC-062.
- **Contract summary.** Two collection kinds: `append_only_log`
  (preserves history; appends to `operational_mutations`),
  `latest_state` (one row per `(collection_name, record_key)` in
  `operational_state`). Collection registry exposes columns: `name`,
  `kind`, `schema_json`, `retention_json`, `format_version`,
  `created_at`. No `disabled_at`, no rename, no soft-retire. No
  `operational_current` table. JSON-Schema validation save-time,
  pre-commit; no open-time re-validation.
- **Key types/fields/enums/errors.** Two-value `kind` enum; collection
  metadata six-column shape; `SchemaValidationError`; `OpStoreError`
  (unknown collection / kind mismatch / registry misuse).
- **Requirement/AC refs.** REQ-057, REQ-058; AC-061a/b/c, AC-062,
  AC-060b.
- **Evidence.** `acceptance.md` AC-062 (column set);
  `requirements.md` REQ-057 (collection kinds enumerated).
- **Open questions.** `design/op-store.md` not in this option's read
  set; AC pins the shape but the design doc may still be a stub.

## IF-022: Bindings five-verb parity invariant

- **ID.** IF-022
- **Name.** Cross-SDK surface-set parity + recovery non-presence.
- **Class.** Public API (cross-binding invariant).
- **Producer.** bindings facade (per binding).
- **Consumer.** SDK callers.
- **Direction.** Caller-visible name set on each SDK binding.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/bindings.md` § 1 + § 10.
- **Other referencing docs.** `requirements.md` REQ-053, REQ-037,
  REQ-054, REQ-031d; `acceptance.md` AC-057a, AC-035d, AC-041.
- **Contract summary.** Symmetric across SDK bindings: a verb appears
  in every SDK binding or in none. SDK surface MUST NOT contain
  recovery names `{recover, restore, repair, fix, rebuild, doctor}`.
  CLI is outside this parity claim.
- **Key types/fields/enums/errors.** Verb set; exclusion list.
- **Requirement/AC refs.** REQ-053, REQ-037, REQ-054, REQ-031d;
  AC-057a, AC-035d, AC-041.
- **Evidence.** `design/bindings.md` § 1 + § 10.
- **Open questions.** Per-binding introspection surface (how to
  enumerate the public set per AC-057a) is delegated to
  `interfaces/{python,ts}.md` (stubs).

## IF-023: Bindings async dispatch invariants A–D

- **ID.** IF-023
- **Name.** Async-surface invariants A–D no-escape-hatch.
- **Class.** Internal subsystem (binding adapter properties; visible
  by absence of escape hatches).
- **Producer.** bindings facade per binding.
- **Consumer.** SDK callers (effectively, by lack of overrides).
- **Direction.** Property of bindings.
- **Public/Internal.** Public (non-presence claim is itself a public
  contract).
- **Canonical owning doc.** `design/bindings.md` § 2.
  Underlying ADR-0.6.0-async-surface defines invariants themselves.
- **Other referencing docs.** `architecture.md` § 6;
  `requirements.md` REQ-020a; `design/engine.md`.
- **Contract summary.** A: scheduler dispatch post-commit; bindings
  expose no escape hatch. B: embedder calls always run on engine-owned
  pool. C: no engine re-entrancy from `embed()`. D: eager warmup at
  `Engine.open` + per-call timeout default 30 s; no cold-load path,
  no per-call-timeout override that disables the watchdog.
- **Key types/fields/enums/errors.** N/A (invariants).
- **Requirement/AC refs.** REQ-023, REQ-028a/b/c.
- **Evidence.** `design/bindings.md` § 2 (table + bullets).
- **Open questions.** TS Path 2 pool-sizing knob is binding-runtime
  mechanic; whether it appears in `interfaces/typescript.md` near
  `Engine.open` is left open.

## IF-024: Build/packaging contract per binding

- **ID.** IF-024
- **Name.** Per-binding shipping path.
- **Class.** Internal subsystem (developer-facing; visible to
  consumers via package registry).
- **Producer.** release pipeline.
- **Consumer.** Operators installing the package; CI smoke gates.
- **Direction.** Pipeline → registry → consumer.
- **Public/Internal.** Public (registry-installed wheel is the
  release gate per REQ-052).
- **Canonical owning doc.** `design/bindings.md` § 9; `design/release.md`
  (cited; not in this option's read set) for CI gates.
- **Other referencing docs.** `architecture.md` § 1; memory
  `feedback_python_native_build`; `requirements.md` REQ-047,
  REQ-048, REQ-049, REQ-050, REQ-052;
  `acceptance.md` AC-051a/b, AC-052, AC-053, AC-054, AC-056.
- **Contract summary.** Python: `pip install -e src/python/` (PyO3
  cdylib package `fathomdb`). TypeScript: `npm install` against
  `src/ts/` (napi-rs cdylib package `fathomdb`). CLI: `cargo build
  --release -p fathomdb-cli`; one binary per platform.
- **Key types/fields/enums/errors.** N/A.
- **Requirement/AC refs.** REQ-047, REQ-048, REQ-049, REQ-050, REQ-052;
  AC-051a/b, AC-052, AC-053, AC-054, AC-056.
- **Evidence.** `design/bindings.md` § 9 (build path table).
- **Open questions.** None at the contract level.

## IF-025: Wire format

- **ID.** IF-025
- **Name.** On-disk + IPC format.
- **Class.** Data/schema/payload.
- **Producer.** engine on-disk layout (no IPC in 0.6.0).
- **Consumer.** Operators (file inspection); recovery tooling.
- **Direction.** Engine → disk; disk → engine.
- **Public/Internal.** Public (file layout is operator-visible).
- **Canonical owning doc.** `interfaces/wire.md` (status:
  `not-started`). De-facto specification: `architecture.md` § 5
  (on-disk layout).
- **Other referencing docs.** `architecture.md` § 5;
  ADR-0.6.0-vector-index-location;
  ADR-0.6.0-op-store-same-file; ADR-0.6.0-zerocopy-blob;
  ADR-0.6.0-durability-fsync-policy.
- **Contract summary.** Stub. From architecture.md § 5: `<db>.sqlite`
  - `<db>.sqlite-wal` + `<db>.sqlite.lock`; tables: `nodes`, `edges`,
  `chunks`, `<canonical>_fts`, `vec0` virtual tables, `operational_*`,
  provenance event table; LE-f32 BLOB encoding for vectors.
- **Key types/fields/enums/errors.** Sidecar lock file; SQLite WAL
  format; `vec0` virtual table layout.
- **Requirement/AC refs.** REQ-035, REQ-041, REQ-051;
  AC-039a/b, AC-045, AC-055.
- **Evidence.** `interfaces/wire.md` ("TBD — draft in Phase 3e ...
  Short OK if surface minimal."); `architecture.md` § 5.
- **Open questions.** `interfaces/wire.md` is a stub. The "no IPC"
  posture would justify a short doc per the file's own note ("If no
  IPC + fresh-db-only with no compat reader, may reduce to: file
  layout references + version sentinel scheme."). The version sentinel
  scheme is not yet documented.

## IF-026: Drain / bounded-completion verb

- **ID.** IF-026
- **Name.** Bounded-completion verb (drain).
- **Class.** Public API.
- **Producer.** scheduler / bindings facade.
- **Consumer.** Tests and batch-ingest callers.
- **Direction.** Caller → engine.
- **Public/Internal.** Public.
- **Canonical owning doc.** `requirements.md` REQ-030 ("API verb name
  owned by binding-interface ADRs"; ADR-0.6.0-async-surface). Per-
  language spelling owned by `interfaces/{python,ts}.md` (stubs).
  Architectural home: `design/scheduler.md` (cited; not in this
  option's read set).
- **Other referencing docs.** `acceptance.md` AC-018, AC-032a/b;
  `design/bindings.md` § 1 (does NOT add a sixth verb to the SDK
  parity set — drain is inferred to be either a method on `Engine`
  itself or a separate utility verb consistent with REQ-053).
- **Contract summary.** Caller can request bounded completion of
  background work with explicit timeout. Returns success on
  completion; returns typed timeout error on timeout (within
  P-DRAIN-TOL × T = 1.5×T).
- **Key types/fields/enums/errors.** Typed timeout error;
  parameterized timeout argument.
- **Requirement/AC refs.** REQ-016, REQ-030; AC-018, AC-032a/b.
- **Evidence.** `requirements.md` REQ-030 (verb-name delegation);
  `acceptance.md` AC-032a/b (drain-with-timeout shape).
- **Open questions.** Verb name not committed in any non-stub doc.
  Whether this is the sixth SDK-visible name (and thus violates the
  five-verb parity claim) or a method on the `Engine` instance (and
  thus part of `Engine.{open, close, write, search, configure}`'s
  surface) is not resolved in the read set. `design/bindings.md` § 1
  insists on exactly five top-level verbs — drain must be subordinate.

## IF-027: Doctor finding code stable surface

- **ID.** IF-027
- **Name.** Stable `code` dispatch surface for findings + recovery
  hints.
- **Class.** Data/schema/payload.
- **Producer.** engine open path (recovery hints) + fathomdb-cli
  doctor (findings).
- **Consumer.** Bindings (dispatch on `recovery_hint.code`); operators
  (diagnose by `code`).
- **Direction.** Engine / CLI → caller.
- **Public/Internal.** Public.
- **Canonical owning doc.** `design/errors.md` § Surface split + § Doctor-only
  finding codes. Cross-cited at `design/recovery.md` § Recovery hint
  anchors.
- **Other referencing docs.** `interfaces/cli.md` § Output posture;
  `design/bindings.md` § 3 ("stable dispatch key for `CorruptionError`").
- **Contract summary.** `code` is a stable string; bindings dispatch
  on it without parsing messages. Open-path codes:
  `E_CORRUPT_WAL_REPLAY`, `E_CORRUPT_HEADER`, `E_CORRUPT_SCHEMA`,
  `E_CORRUPT_EMBEDDER_IDENTITY`. Doctor-only codes (e.g.
  `E_CORRUPT_INTEGRITY_CHECK`) share the same dispatch namespace but
  do not map 1:1 to `Engine.open` `CorruptionKind`.
- **Key types/fields/enums/errors.** Stable string set listed above;
  `RecoveryHint { code, doc_anchor }`.
- **Requirement/AC refs.** REQ-031d, REQ-039; AC-035b, AC-043c.
- **Evidence.** `design/errors.md` § Engine.open corruption table +
  § Doctor-only finding codes; `design/recovery.md` § Doctor finding
  codes vs `Engine.open` enums.
- **Open questions.** Full doctor code set (beyond
  `E_CORRUPT_INTEGRITY_CHECK`) not enumerated.
