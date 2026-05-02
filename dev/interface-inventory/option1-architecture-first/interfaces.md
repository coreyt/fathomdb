## IF-001

- ID: `IF-001`
- Name: Rust runtime SDK surface
- Class: `Public API`
- Producer: `Rust SDK`
- Consumer: `Rust application caller`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/interfaces/rust.md#Rust Interface`
- Other referencing docs: `dev/architecture.md#1. Crate topology`; `dev/design/bindings.md#1. Surface-set parity invariant`; `dev/requirements.md#Public surface (REQ-053..REQ-059)`
- Contract summary: The Rust application surface is the same five-verb runtime surface as the other SDKs: `Engine.open`, `admin.configure`, `write`, `search`, and `close`, with no recovery or repair verbs and no semver promise for internal engine module boundaries.
- Key types/fields/enums/errors: `Engine`; `EngineConfig`; `PreparedWrite`; `WriteReceipt.cursor`; `Search.projection_cursor`; `EngineError`; `EngineOpenError`
- Requirement/AC refs: `REQ-053`; `REQ-054`; `REQ-055`; `REQ-056`
- Evidence: `dev/interfaces/rust.md#Rust Interface`; `dev/design/bindings.md#1. Surface-set parity invariant`; `dev/architecture.md#1. Crate topology`
- Open questions: `interfaces/rust.md` is still `TBD`; per-verb signatures, Rust-specific symbol names, examples, stability posture, and any Rust-specific acceptance coverage are missing.

## IF-002

- ID: `IF-002`
- Name: Python runtime SDK surface
- Class: `Public API`
- Producer: `Python SDK`
- Consumer: `Python application caller`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/interfaces/python.md#Python Interface`
- Other referencing docs: `dev/design/bindings.md#1. Surface-set parity invariant`; `dev/design/bindings.md#2. Lifecycle dispatch model`; `dev/requirements.md#Public surface (REQ-053..REQ-059)`; `dev/acceptance.md#AC-057a: Five-verb application runtime SDK surface`
- Contract summary: The Python binding exposes the canonical five-verb SDK surface only, uses the sync Path 1 dispatch model, and relies on the host to choose Python runtime patterns such as `run_in_executor` rather than defining a first-party async API.
- Key types/fields/enums/errors: `Engine.open`; `admin.configure`; `write`; `search`; `close`; Python-specific typed exceptions rooted at `EngineError`
- Requirement/AC refs: `REQ-053`; `REQ-054`; `REQ-055`; `REQ-056`; `AC-041`; `AC-057a`; `AC-060a`
- Evidence: `dev/interfaces/python.md#Python Interface`; `dev/design/bindings.md#1. Surface-set parity invariant`; `dev/design/bindings.md#2. Lifecycle dispatch model`; `dev/acceptance.md#AC-057a: Five-verb application runtime SDK surface`
- Open questions: `interfaces/python.md` is still `TBD`; symbol spellings beyond the five verbs, subscriber helper signatures, telemetry accessors, and exception attribute names are not documented.

## IF-003

- ID: `IF-003`
- Name: TypeScript runtime SDK surface
- Class: `Public API`
- Producer: `TypeScript SDK`
- Consumer: `TypeScript application caller`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/interfaces/typescript.md#TypeScript Interface`
- Other referencing docs: `dev/design/bindings.md#1. Surface-set parity invariant`; `dev/design/bindings.md#2. Lifecycle dispatch model`; `dev/requirements.md#Public surface (REQ-053..REQ-059)`; `dev/acceptance.md#AC-057a: Five-verb application runtime SDK surface`
- Contract summary: The TypeScript binding exposes the canonical five-verb SDK surface only, but as a Promise-based Path 2 surface backed by a Rust-owned thread-pool handoff rather than direct libuv worker execution.
- Key types/fields/enums/errors: `Engine.open`; `admin.configure`; `write`; `search`; `close`; TS-specific typed `EngineError` subclasses; default plus named exports
- Requirement/AC refs: `REQ-053`; `REQ-054`; `REQ-055`; `REQ-056`; `AC-041`; `AC-057a`; `AC-060a`
- Evidence: `dev/interfaces/typescript.md#TypeScript Interface`; `dev/design/bindings.md#1. Surface-set parity invariant`; `dev/design/bindings.md#2. Lifecycle dispatch model`; `dev/acceptance.md#AC-057a: Five-verb application runtime SDK surface`
- Open questions: `interfaces/typescript.md` is still `TBD`; export names, Promise signatures, callback registration calls, and exception attribute names are not documented.

## IF-004

- ID: `IF-004`
- Name: Typed write and op-store submission boundary
- Class: `Binding adapter`
- Producer: `Bindings facade`
- Consumer: `Engine`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/bindings.md#4. Marshalling strategy`
- Other referencing docs: `dev/design/engine.md#Batch submission semantics`; `dev/design/op-store.md#Write contract`; `dev/requirements.md#Public surface (REQ-053..REQ-059)`; `dev/acceptance.md#AC-060b: JSON-Schema validation fires save-time, pre-commit; no open-time re-validation`
- Contract summary: Public writes cross the binding boundary as typed `PreparedWrite` batches only. The whole slice is validated before commit-sensitive work, committed in caller order inside one SQLite transaction, and yields one write cursor for the accepted batch. `PreparedWrite::OpStore` participates in the same transaction and uses save-time, pre-commit JSON-Schema validation.
- Key types/fields/enums/errors: `PreparedWrite::Node`; `PreparedWrite::Edge`; `PreparedWrite::OpStore(OpStoreInsert)`; `PreparedWrite::AdminSchema(AdminSchemaWrite)`; `WriteReceipt.cursor`; `WriteValidationError`; `SchemaValidationError`; `OpStoreError`
- Requirement/AC refs: `REQ-053`; `REQ-056`; `REQ-057`; `REQ-058`; `AC-060b`; `AC-061a`; `AC-061b`; `AC-061c`; `AC-062`
- Evidence: `dev/design/bindings.md#4. Marshalling strategy`; `dev/design/engine.md#Batch submission semantics`; `dev/design/op-store.md#Write contract`
- Open questions: The per-language builders and exact input object shapes are deferred to the still-empty language interface docs.

## IF-005

- ID: `IF-005`
- Name: Freshness cursor contract
- Class: `Public API`
- Producer: `Engine`
- Consumer: `Rust SDK`; `Python SDK`; `TypeScript SDK`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/engine.md#Cursor contract`
- Other referencing docs: `dev/architecture.md#3. Write path`; `dev/architecture.md#4. Read path`; `dev/design/projections.md#Terminal states`; `dev/requirements.md#Public surface (REQ-053..REQ-059)`; `dev/acceptance.md#AC-059a: projection_cursor exposed on read tx; monotonic non-decreasing`
- Contract summary: Write commit returns a monotonic write cursor `c_w`; read transactions expose a monotonic non-decreasing `projection_cursor`. Canonical and FTS visibility happen at write commit, vector visibility is satisfied only once a read observes `projection_cursor >= c_w`.
- Key types/fields/enums/errors: `c_w`; `projection_cursor`; `ProjectionStatus::{Pending, Failed, UpToDate}`
- Requirement/AC refs: `REQ-055`; `AC-059a`; `AC-059b`; `AC-063b`
- Evidence: `dev/design/engine.md#Cursor contract`; `dev/design/projections.md#Terminal states`; `dev/architecture.md#3. Write path`
- Open questions: Exact field spelling and result-container names per language are not documented because the language interface files are still placeholders.

## IF-006

- ID: `IF-006`
- Name: Lifecycle phase event stream
- Class: `Event/observability`
- Producer: `Lifecycle`
- Consumer: `Application code`; `operator UX`; `host subscriber`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/lifecycle.md#Response-cycle feedback`
- Other referencing docs: `dev/requirements.md#Observability (REQ-001..REQ-008)`; `dev/acceptance.md#AC-001: Lifecycle phase tag is a typed enum`; `dev/acceptance.md#AC-008: Slow signal participates in lifecycle attribution`
- Contract summary: Lifecycle feedback carries a typed `phase` drawn from exactly five values: `Started`, `Slow`, `Heartbeat`, `Finished`, and `Failed`. `Slow` is advisory and in-flight; `Finished` and `Failed` are terminal and mutually exclusive.
- Key types/fields/enums/errors: `phase`; `Started`; `Slow`; `Heartbeat`; `Finished`; `Failed`
- Requirement/AC refs: `REQ-001`; `REQ-006a`; `REQ-006b`; `AC-001`; `AC-007a`; `AC-007b`; `AC-008`
- Evidence: `dev/design/lifecycle.md#Response-cycle feedback`; `dev/design/lifecycle.md#Phase enum`; `dev/design/lifecycle.md#Phase semantics`
- Open questions: The non-phase envelope is intentionally not standardized here; per-operation identity/timing fields remain undocumented at the interface level.

## IF-007

- ID: `IF-007`
- Name: Structured diagnostics subscriber route
- Class: `Event/observability`
- Producer: `Lifecycle`
- Consumer: `Host subscriber`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/lifecycle.md#Host-routed diagnostics`
- Other referencing docs: `dev/design/bindings.md#8. Logging / tracing subscriber attachment`; `dev/requirements.md#Observability (REQ-001..REQ-008)`; `dev/acceptance.md#AC-003a: Writer events flow to host subscriber`
- Contract summary: All engine and SQLite-originated diagnostics route through the host subscriber path; the engine has no private sink when no subscriber is registered. Diagnostics carry typed `source` and `category` tags rather than relying on message parsing.
- Key types/fields/enums/errors: `source::{Engine, SqliteInternal}`; engine categories `writer`, `search`, `admin`, `error`; SQLite categories `corruption`, `recovery`, `io`
- Requirement/AC refs: `REQ-002`; `REQ-005`; `AC-002`; `AC-003a`; `AC-003b`; `AC-003c`; `AC-003d`; `AC-006`
- Evidence: `dev/design/lifecycle.md#Host-routed diagnostics`; `dev/design/lifecycle.md#No private sink`; `dev/design/bindings.md#8. Logging / tracing subscriber attachment`
- Open questions: The actual registration calls for Python and TypeScript are not documented because the language interface docs are missing.

## IF-008

- ID: `IF-008`
- Name: Counter snapshot surface
- Class: `Event/observability`
- Producer: `Lifecycle`
- Consumer: `Operator`; `application caller`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/lifecycle.md#Counter snapshot`
- Other referencing docs: `dev/requirements.md#Observability (REQ-001..REQ-008)`; `dev/acceptance.md#AC-004a: Counter snapshot exposes documented key set`
- Contract summary: Counter snapshots are pull-based, cumulative since engine open, and non-perturbing. The exact public key set is fixed for 0.6.0 and excludes undocumented internal counters.
- Key types/fields/enums/errors: `queries`; `writes`; `write_rows`; `errors_by_code`; `admin_ops`; `cache_hit`; `cache_miss`
- Requirement/AC refs: `REQ-003`; `AC-004a`; `AC-004b`; `AC-004c`
- Evidence: `dev/design/lifecycle.md#Counter snapshot`; `dev/design/lifecycle.md#Public key set`; `dev/design/lifecycle.md#Semantics`
- Open questions: No interface doc names the API call or return container that exposes the snapshot to Rust, Python, or TypeScript callers.

## IF-009

- ID: `IF-009`
- Name: Per-statement profiling surface
- Class: `Event/observability`
- Producer: `Lifecycle`
- Consumer: `Operator`; `application caller`; `CLI dump tooling`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/lifecycle.md#Per-statement profiling`
- Other referencing docs: `dev/requirements.md#Observability (REQ-001..REQ-008)`; `dev/acceptance.md#AC-005a: Per-statement profiling toggleable at runtime`
- Contract summary: Profiling is a separate opt-in surface, toggleable at runtime, and does not disappear into free-form logs. It answers per-statement cost rather than lifecycle liveness.
- Key types/fields/enums/errors: `wall_clock_ms`; `step_count`; `cache_delta`
- Requirement/AC refs: `REQ-004`; `AC-005a`; `AC-005b`
- Evidence: `dev/design/lifecycle.md#Per-statement profiling`; `dev/design/lifecycle.md#Toggle and scope`; `dev/design/lifecycle.md#Public record shape`
- Open questions: The documented API call that enables/disables profiling is required by acceptance but is not named in any interface doc.

## IF-010

- ID: `IF-010`
- Name: Stress-failure observability payload
- Class: `Event/observability`
- Producer: `Lifecycle`
- Consumer: `Host subscriber`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/lifecycle.md#Stress-failure context`
- Other referencing docs: `dev/requirements.md#Observability (REQ-001..REQ-008)`; `dev/acceptance.md#AC-009: Stress-failure event field schema`
- Contract summary: Robustness failures must emit a dedicated structured payload, not ad hoc free-text metadata, so failures can be diagnosed without rerunning the scenario.
- Key types/fields/enums/errors: `thread_group_id`; `op_kind`; `last_error_chain`; `projection_state`
- Requirement/AC refs: `REQ-007`; `AC-009`
- Evidence: `dev/design/lifecycle.md#Stress-failure context`; `dev/acceptance.md#AC-009: Stress-failure event field schema`
- Open questions: The transport wrapper and any correlation identifiers outside the four required fields are not documented.

## IF-011

- ID: `IF-011`
- Name: Typed error hierarchy and corruption detail surface
- Class: `Binding adapter`
- Producer: `Errors`
- Consumer: `Rust SDK`; `Python SDK`; `TypeScript SDK`; `CLI`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/errors.md#Binding mapping ownership`
- Other referencing docs: `dev/design/errors.md#Top-level types`; `dev/design/errors.md#Corruption detail owner`; `dev/design/bindings.md#3. Error-mapping protocol`; `dev/acceptance.md#AC-035b: CorruptionDetail shape`
- Contract summary: 0.6.0 keeps typed top-level roots for post-open runtime failures and open-path failures, preserves distinct module/direct variants when remediation differs, and requires bindings to expose one concrete typed class per variant without message parsing. Open-path corruption uses a structured `CorruptionDetail` payload with stable machine-readable recovery hints.
- Key types/fields/enums/errors: `EngineError`; `EngineOpenError`; `StorageError`; `ProjectionError`; `VectorError`; `EmbedderError`; `SchedulerError`; `OpStoreError`; `WriteValidationError`; `SchemaValidationError`; `EmbedderIdentityMismatchError`; `MigrationError`; `OverloadedError`; `ClosingError`; `CorruptionKind`; `OpenStage`; `CorruptionLocator`; `RecoveryHint.code`; `RecoveryHint.doc_anchor`
- Requirement/AC refs: `REQ-031d`; `REQ-056`; `AC-035a`; `AC-035b`; `AC-035c`; `AC-060a`; `AC-060b`
- Evidence: `dev/design/errors.md#Top-level types`; `dev/design/errors.md#Module taxonomy`; `dev/design/errors.md#Corruption detail owner`; `dev/design/bindings.md#3. Error-mapping protocol`
- Open questions: Concrete class names and attribute casing are intentionally left to `interfaces/{python,typescript,cli}.md`, but those docs do not currently define them.

## IF-012

- ID: `IF-012`
- Name: Operator CLI root surface
- Class: `CLI/operator`
- Producer: `CLI`
- Consumer: `Operator`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/interfaces/cli.md#CLI Interface`
- Other referencing docs: `dev/design/recovery.md#Two-root CLI split`; `dev/requirements.md#Operability (REQ-036..REQ-041)`; `dev/design/bindings.md#10. Recovery surface unreachability`
- Contract summary: The CLI is an operator-only surface with two roots: read-only `fathomdb doctor <verb>` and lossy `fathomdb recover --accept-data-loss <sub-flag>...`. It is structurally distinct from the five-verb SDK surface and is the only place recovery tooling is reachable.
- Key types/fields/enums/errors: `doctor`; `check-integrity`; `safe-export`; `verify-embedder`; `trace`; `dump-schema`; `dump-row-counts`; `dump-profile`; `recover`; `--accept-data-loss`; exit classes `doctor-check-*`, `doctor-export-*`, `recover-*`
- Requirement/AC refs: `REQ-036`; `REQ-037`; `REQ-054`; `AC-035d`; `AC-040a`; `AC-040b`; `AC-041`; `AC-058`
- Evidence: `dev/interfaces/cli.md#Roots`; `dev/interfaces/cli.md#Doctor verbs`; `dev/interfaces/cli.md#Recover root`; `dev/design/recovery.md#Two-root CLI split`
- Open questions: Only help reachability and top-level verb membership are acceptance-locked; most per-verb runtime output schemas remain underspecified.

## IF-013

- ID: `IF-013`
- Name: `doctor check-integrity` machine-readable report
- Class: `Data/schema/payload`
- Producer: `Recovery`
- Consumer: `Operator`; `automation tooling`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/recovery.md#check-integrity schema owner`
- Other referencing docs: `dev/interfaces/cli.md#Output posture`; `dev/design/errors.md#Doctor-only finding codes`; `dev/requirements.md#Operability (REQ-036..REQ-041)`; `dev/acceptance.md#AC-043a: check-integrity produces structured report with three sections`
- Contract summary: `doctor check-integrity --json` emits one JSON object with top-level sections `physical`, `logical`, and `semantic`. Findings use stable fields `code`, `stage`, `locator`, `doc_anchor`, and `detail`; `--full` may emit doctor-only codes such as `E_CORRUPT_INTEGRITY_CHECK`.
- Key types/fields/enums/errors: `physical`; `logical`; `semantic`; `code`; `stage`; `locator`; `doc_anchor`; `detail`; `E_CORRUPT_INTEGRITY_CHECK`
- Requirement/AC refs: `REQ-039`; `AC-043a`; `AC-043b`; `AC-043c`
- Evidence: `dev/design/recovery.md#Machine-readable output`; `dev/design/recovery.md#check-integrity schema owner`; `dev/design/recovery.md#Integrity-check full findings`
- Open questions: The JSON shapes for the non-`check-integrity` doctor verbs and for `recover` progress are not documented to the same level.

## IF-014

- ID: `IF-014`
- Name: `safe_export` artifact and manifest contract
- Class: `Data/schema/payload`
- Producer: `Recovery`
- Consumer: `Operator`; `verifier tooling`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/recovery.md#Machine-readable output`
- Other referencing docs: `dev/architecture.md#5. On-disk layout`; `dev/interfaces/cli.md#Doctor verbs`; `dev/requirements.md#Security (REQ-032..REQ-035)`; `dev/acceptance.md#AC-039a: safe_export artifact ships SHA-256 manifest matching contents`
- Contract summary: `safe_export` produces a `.fathomdb-export` artifact plus a matching `.sha256` manifest, and the artifact must cover committed WAL-backed state rather than regressing to file-copy behavior.
- Key types/fields/enums/errors: `<export-name>.fathomdb-export`; `<export-name>.fathomdb-export.sha256`; SHA-256 manifest
- Requirement/AC refs: `REQ-024`; `REQ-035`; `AC-026`; `AC-039a`; `AC-039b`
- Evidence: `dev/architecture.md#5. On-disk layout`; `dev/interfaces/cli.md#Doctor verbs`; `dev/acceptance.md#AC-039a: safe_export artifact ships SHA-256 manifest matching contents`
- Open questions: The verifier tool name and its exact machine-readable mismatch output are not documented in the design corpus.

## IF-015

- ID: `IF-015`
- Name: Open-time migration contract
- Class: `Internal subsystem`
- Producer: `Migrations`
- Consumer: `Engine`; `SDK callers`; `host subscriber`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/migrations.md#Migrations Design`
- Other referencing docs: `dev/design/engine.md#Open path`; `dev/design/lifecycle.md#Relation to other subsystem events`; `dev/requirements.md#Upgrade / compatibility (REQ-042..REQ-046)`; `dev/acceptance.md#AC-046a: Auto schema migration applied at open`
- Contract summary: `Engine.open` always runs schema migration when needed, applies all required steps automatically, emits one structured event per applied or failed step, and enforces an accretion-guard rule for post-v1 migrations.
- Key types/fields/enums/errors: `step_id`; `duration_ms`; `failed`; applied version; `MigrationError` / `MigrationFailed`
- Requirement/AC refs: `REQ-042`; `REQ-045`; `AC-046a`; `AC-046b`; `AC-046c`; `AC-049`
- Evidence: `dev/design/migrations.md#Migrations Design`; `dev/design/engine.md#Open path`; `dev/acceptance.md#AC-046b: Migration emits per-step duration event on success`
- Open questions: `design/migrations.md` declares ownership but does not yet spell out the event payload schema, open-result fields, or linter contract in detail.

## IF-016

- ID: `IF-016`
- Name: Local file-set and no-wire-protocol surface
- Class: `Data/schema/payload`
- Producer: `Engine`
- Consumer: `Operator`; `local tooling`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/architecture.md#5. On-disk layout`
- Other referencing docs: `dev/interfaces/wire.md#Wire Format`; `dev/requirements.md#Security (REQ-032..REQ-035)`; `dev/requirements.md#Operability (REQ-036..REQ-041)`; `dev/acceptance.md#AC-045: Single-file deploy`
- Contract summary: 0.6.0 is in-process/local only, not a server protocol. The database file set is one `.sqlite` path plus auto-managed sidecars such as `-wal` and `.lock`; there is no public network listener or separate operator-managed data file for vectors or op-store.
- Key types/fields/enums/errors: `<db-name>.sqlite`; `<db-name>.sqlite-wal`; `<db-name>.sqlite.lock`; optional `<db-name>.sqlite-journal`
- Requirement/AC refs: `REQ-032`; `REQ-041`; `AC-036`; `AC-045`
- Evidence: `dev/architecture.md#5. On-disk layout`; `dev/requirements.md#Operability (REQ-036..REQ-041)`; `dev/interfaces/wire.md#Wire Format`
- Open questions: `interfaces/wire.md` is still `TBD`, so version sentinels and any explicit machine-readable wire/on-disk schema document are absent.

## IF-017

- ID: `IF-017`
- Name: Projection failure diagnosis and explicit regenerate workflow
- Class: `CLI/operator`
- Producer: `Projection`
- Consumer: `Recovery`; `operator`
- Direction: `producer -> consumer`
- Public/Internal: `Public`
- Canonical owning doc: `dev/design/projections.md#projection_failures`
- Other referencing docs: `dev/design/projections.md#Regenerate workflow`; `dev/design/op-store.md#Projection-failure ownership`; `dev/design/recovery.md#Projection repair workflow`; `dev/requirements.md#Public surface (REQ-053..REQ-059)`
- Contract summary: Once projection retries are exhausted, exactly one durable failure row is recorded in the `projection_failures` append-only op-store collection, restart does not silently clear or retry it, and the only repair path is the explicit regenerate workflow implemented as `fathomdb recover --accept-data-loss --rebuild-projections`.
- Key types/fields/enums/errors: `projection_failures`; `Pending`; `Failed`; `UpToDate`; `recover --accept-data-loss --rebuild-projections`
- Requirement/AC refs: `REQ-059`; `AC-063a`; `AC-063b`; `AC-063c`
- Evidence: `dev/design/projections.md#projection_failures`; `dev/design/projections.md#Regenerate workflow`; `dev/design/op-store.md#Projection-failure ownership`
- Open questions: The failure-row payload schema itself is not documented, only its existence, storage class, and restart/regenerate semantics.
