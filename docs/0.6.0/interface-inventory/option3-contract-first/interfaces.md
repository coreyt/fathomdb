# Interface Catalog

## IF-001 — Five-Verb SDK Surface Set

- `ID`: `IF-001`
- `Name`: Five-verb application runtime SDK surface
- `Class`: Public API
- `Producer`: `bindings` expressed through Python SDK and TypeScript SDK
- `Consumer`: application callers
- `Direction`: caller -> SDK/runtime
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/bindings.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`, `docs/0.6.0/interfaces/python.md`, `docs/0.6.0/interfaces/typescript.md`
- `Contract summary`: The SDK public top-level surface is exactly `Engine.open`, `admin.configure`, `write`, `search`, and `close`, in binding-idiomatic casing, with no extra top-level runtime verbs.
- `Key types/fields/enums/errors`: verb set `Engine.open`, `admin.configure`, `write`, `search`, `close`
- `Requirement/AC refs`: `REQ-053`, `AC-057a`
- `Evidence`: `docs/0.6.0/design/bindings.md` § `1. Surface-set parity invariant`; `docs/0.6.0/requirements.md` § `Public surface (REQ-053..REQ-059)`; `docs/0.6.0/acceptance.md` § `AC-057a: Five-verb application runtime SDK surface`
- `Open questions`: `interfaces/python.md` and `interfaces/typescript.md` are still `TBD`, so exact exported names and symbol grouping are not yet canonicalized there.

## IF-002 — Recovery Non-Presence on the SDK Surface

- `ID`: `IF-002`
- `Name`: SDK-side recovery unreachability
- `Class`: Binding adapter
- `Producer`: `bindings`
- `Consumer`: application callers, release/test tooling validating top-level surfaces
- `Direction`: negative surface contract from SDK to caller
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/bindings.md`
- `Other referencing docs`: `docs/0.6.0/design/recovery.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Python and TypeScript SDKs must not expose recovery or repair verbs; those workflows are CLI-only.
- `Key types/fields/enums/errors`: forbidden names include `recover`, `restore`, `repair`, `fix`, `rebuild`, `doctor`
- `Requirement/AC refs`: `REQ-037`, `REQ-054`, `REQ-031d`, `AC-035d`, `AC-041`, `AC-057a`, `AC-058`
- `Evidence`: `docs/0.6.0/design/bindings.md` § `10. Recovery surface unreachability`; `docs/0.6.0/design/recovery.md` § `Relationship to runtime SDK`; `docs/0.6.0/acceptance.md` § `AC-041: Recovery tooling unreachable from runtime SDK`
- `Open questions`: Per-language surface inventories are still deferred to the per-language interface docs.

## IF-003 — `Engine.open` Runtime Open Path

- `ID`: `IF-003`
- `Name`: `Engine.open` ordered open path and exclusivity behavior
- `Class`: Public API
- `Producer`: `engine`
- `Consumer`: Rust/Python/TypeScript callers and CLI entrypoints
- `Direction`: caller -> engine
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/engine.md`
- `Other referencing docs`: `docs/0.6.0/design/bindings.md`, `docs/0.6.0/design/errors.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`
- `Contract summary`: `Engine.open` canonicalizes the path, acquires the sidecar lock, opens SQLite, runs always-on corruption detection, executes migrations, checks embedder identity, warms the embedder, and starts writer/scheduler resources; failed opens do not return an `Engine` handle.
- `Key types/fields/enums/errors`: `EngineOpenError`, `DatabaseLocked`, `Corruption`, `EmbedderIdentityMismatchError`, `MigrationError`
- `Requirement/AC refs`: `REQ-020a`, `REQ-022a`, `REQ-031c`, `REQ-031d`, `REQ-042`, `REQ-051`, `AC-024a`, `AC-035`, `AC-035a`, `AC-035c`, `AC-046a`, `AC-055`
- `Evidence`: `docs/0.6.0/design/engine.md` § `Open path`; `docs/0.6.0/design/bindings.md` § `7. Lock + process-exclusivity contract`; `docs/0.6.0/acceptance.md` § `AC-035a: Engine.open refuses on detected corruption`
- `Open questions`: Symbol signatures and return-shape details for Rust/Python/TypeScript remain unfilled in the per-language interface docs.

## IF-004 — Ordered Write Batch and Admin-on-Writer Path

- `ID`: `IF-004`
- `Name`: Ordered `write` batch submission and `admin.configure` writer routing
- `Class`: Public API
- `Producer`: `engine`
- `Consumer`: SDK callers
- `Direction`: caller -> engine
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/engine.md`
- `Other referencing docs`: `docs/0.6.0/design/bindings.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`
- `Contract summary`: `write(&[PreparedWrite])` is one ordered writer submission with all-or-nothing commit semantics, and `admin.configure` travels through the same writer machinery rather than a side channel.
- `Key types/fields/enums/errors`: `PreparedWrite::Node`, `Edge`, `OpStore`, `AdminSchema`; `WriteValidationError`; `SchemaValidationError`
- `Requirement/AC refs`: `REQ-053`, `REQ-056`, `AC-021`, `AC-060b`
- `Evidence`: `docs/0.6.0/design/engine.md` § `PreparedWrite::AdminSchema provenance`; `docs/0.6.0/design/engine.md` § `Batch submission semantics`; `docs/0.6.0/design/bindings.md` § `4. Marshalling strategy`
- `Open questions`: Per-language builder shapes for `PreparedWrite` are not yet written in the interface docs.

## IF-005 — Search Result Surface with Read-Side Freshness

- `ID`: `IF-005`
- `Name`: `search` result contract with `projection_cursor`
- `Class`: Public API
- `Producer`: `engine` through retrieval/projection and bindings
- `Consumer`: SDK callers
- `Direction`: engine -> caller
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/engine.md`
- `Other referencing docs`: `docs/0.6.0/architecture.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/design/bindings.md`
- `Contract summary`: Search returns results plus a read-side `projection_cursor`; hybrid retrieval may also return a typed soft-fallback record when a non-essential branch did not contribute.
- `Key types/fields/enums/errors`: `projection_cursor`; soft-fallback `branch`
- `Requirement/AC refs`: `REQ-029`, `REQ-055`, `AC-031`, `AC-059a`
- `Evidence`: `docs/0.6.0/design/engine.md` § `Cursor contract`; `docs/0.6.0/architecture.md` § `Read path`; `docs/0.6.0/acceptance.md` § `AC-031: Hybrid retrieval surfaces soft-fallback signal`
- `Open questions`: The exact field name for the soft-fallback payload is explicitly delegated to binding-interface ADRs and is not written in the listed docs.

## IF-006 — `Engine.close` Resource-Release Contract

- `ID`: `IF-006`
- `Name`: Blocking close and lock release
- `Class`: Public API
- `Producer`: `engine`
- `Consumer`: SDK callers
- `Direction`: caller -> engine
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/bindings.md`
- `Other referencing docs`: `docs/0.6.0/design/engine.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`
- `Contract summary`: Closing or dropping an engine releases the sidecar lock and SQLite resources; explicit `close` is part of the public runtime surface and cross-binding exclusivity contract.
- `Key types/fields/enums/errors`: `close`; `ClosingError`; `DatabaseLocked`
- `Requirement/AC refs`: `REQ-020a`, `REQ-020b`, `REQ-021`, `AC-022a`, `AC-022b`, `AC-022c`, `AC-023a`, `AC-023b`
- `Evidence`: `docs/0.6.0/design/bindings.md` § `7. Lock + process-exclusivity contract`; `docs/0.6.0/acceptance.md` § `AC-022a: Engine close releases lock`
- `Open questions`: The per-language method spellings and shutdown-return types remain undocumented in the per-language interface files.

## IF-007 — Lifecycle Phase Event

- `ID`: `IF-007`
- `Name`: Response-cycle phase event
- `Class`: Event/observability
- `Producer`: `lifecycle`
- `Consumer`: application code, operator UX, subscriber adapters
- `Direction`: lifecycle -> subscriber/caller
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/lifecycle.md`
- `Other referencing docs`: `docs/0.6.0/design/bindings.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Every response-cycle event carries a typed `phase` in the five-value lifecycle enum, with `Slow` as an advisory in-flight transition and `Finished`/`Failed` as terminal and mutually exclusive.
- `Key types/fields/enums/errors`: `phase` enum `{Started, Slow, Heartbeat, Finished, Failed}`
- `Requirement/AC refs`: `REQ-001`, `REQ-006b`, `AC-001`, `AC-008`
- `Evidence`: `docs/0.6.0/design/lifecycle.md` § `Response-cycle feedback`; `docs/0.6.0/acceptance.md` § `AC-001: Lifecycle phase tag is a typed enum`
- `Open questions`: Non-`phase` envelope fields are intentionally delegated to the producing surface or binding contract.

## IF-008 — Structured Diagnostics Event

- `ID`: `IF-008`
- `Name`: Host-routed structured diagnostics
- `Class`: Event/observability
- `Producer`: `lifecycle`, engine diagnostics, SQLite-internal diagnostics
- `Consumer`: host subscribers, operators
- `Direction`: engine/lifecycle -> host subscriber
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/lifecycle.md`
- `Other referencing docs`: `docs/0.6.0/design/bindings.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Structured diagnostics are routed only through the host subscriber path, carry typed `source` and `category` tags, and include both engine-originated and SQLite-originated events.
- `Key types/fields/enums/errors`: `source` = `Engine | SqliteInternal`; engine categories `writer | search | admin | error`; SQLite categories `corruption | recovery | io`
- `Requirement/AC refs`: `REQ-002`, `REQ-005`, `REQ-006a`, `AC-002`, `AC-003a`, `AC-003b`, `AC-003c`, `AC-003d`, `AC-006`, `AC-007a`, `AC-007b`
- `Evidence`: `docs/0.6.0/design/lifecycle.md` § `Host-routed diagnostics`; `docs/0.6.0/design/bindings.md` § `8. Logging / tracing subscriber attachment`; `docs/0.6.0/acceptance.md` § `AC-006: SQLite-internal events surfaced with typed source tag`
- `Open questions`: Python helper names and TypeScript callback signatures are not yet filled in by per-language interface docs.

## IF-009 — Counter Snapshot

- `ID`: `IF-009`
- `Name`: Cumulative counter snapshot
- `Class`: Event/observability
- `Producer`: `lifecycle`
- `Consumer`: operators, diagnostics tooling
- `Direction`: caller pulls snapshot from lifecycle-owned state
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/lifecycle.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: The counter snapshot is a cumulative, read-on-demand, non-perturbing machine-readable snapshot since engine open.
- `Key types/fields/enums/errors`: `queries`, `writes`, `write_rows`, `errors_by_code`, `admin_ops`, `cache_hit`, `cache_miss`
- `Requirement/AC refs`: `REQ-003`, `AC-004a`, `AC-004b`, `AC-004c`
- `Evidence`: `docs/0.6.0/design/lifecycle.md` § `Counter snapshot`; `docs/0.6.0/acceptance.md` § `AC-004a: Counter snapshot exposes documented key set`
- `Open questions`: Accessor/method names are not materialized in the per-language interface docs.

## IF-010 — Per-Statement Profile Record

- `ID`: `IF-010`
- `Name`: Per-statement profiling record
- `Class`: Event/observability
- `Producer`: `lifecycle`
- `Consumer`: operators, profiling/report tooling
- `Direction`: lifecycle -> subscriber/tooling
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/lifecycle.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Profiling is a separate opt-in surface from lifecycle feedback and emits typed numeric cost records.
- `Key types/fields/enums/errors`: `wall_clock_ms`, `step_count`, `cache_delta`
- `Requirement/AC refs`: `REQ-004`, `AC-005a`, `AC-005b`
- `Evidence`: `docs/0.6.0/design/lifecycle.md` § `Per-statement profiling`; `docs/0.6.0/acceptance.md` § `AC-005b: Profile record schema`
- `Open questions`: The transport for profile records is delegated outside `design/lifecycle.md`.

## IF-011 — Stress-Failure Context Payload

- `ID`: `IF-011`
- `Name`: Structured stress-failure context
- `Class`: Data/schema/payload
- `Producer`: `lifecycle`
- `Consumer`: robustness tooling, operators
- `Direction`: lifecycle -> subscriber/tooling
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/lifecycle.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Stress and robustness failures must surface a dedicated structured payload rather than ad hoc error metadata.
- `Key types/fields/enums/errors`: `thread_group_id`, `op_kind`, `last_error_chain`, `projection_state`
- `Requirement/AC refs`: `REQ-007`, `AC-009`
- `Evidence`: `docs/0.6.0/design/lifecycle.md` § `Stress-failure context`; `docs/0.6.0/acceptance.md` § `AC-009: Stress-failure event field schema`
- `Open questions`: The exact delivery route is implied as observability data but not narrowed to one transport.

## IF-012 — Migration Step Progress Event

- `ID`: `IF-012`
- `Name`: Per-step migration progress/failure event
- `Class`: Data/schema/payload
- `Producer`: `migrations`, routed through lifecycle/subscriber paths and surfaced by bindings
- `Consumer`: operators and callers observing `Engine.open`
- `Direction`: migrations -> lifecycle/bindings -> caller/subscriber
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/migrations.md`
- `Other referencing docs`: `docs/0.6.0/design/lifecycle.md`, `docs/0.6.0/design/bindings.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`
- `Contract summary`: Applied migration steps emit structured per-step duration events; failures emit a structured failed-step event and a typed migration error.
- `Key types/fields/enums/errors`: `step_id`, `duration_ms`, `failed: true`, typed `MigrationError` / `MigrationFailed`
- `Requirement/AC refs`: `REQ-042`, `REQ-045`, `AC-046a`, `AC-046b`, `AC-046c`
- `Evidence`: `docs/0.6.0/design/migrations.md` § `Migrations Design`; `docs/0.6.0/design/lifecycle.md` § `Ownership boundaries`; `docs/0.6.0/acceptance.md` § `AC-046b: Migration emits per-step duration event on success`; `docs/0.6.0/acceptance.md` § `AC-046c: Migration emits per-step duration event on failure`
- `Open questions`: `design/migrations.md` declares ownership but does not yet write the payload schema beyond what acceptance specifies.

## IF-013 — `CorruptionDetail` Open-Path Payload

- `ID`: `IF-013`
- `Name`: `EngineOpenError::Corruption(detail)`
- `Class`: Data/schema/payload
- `Producer`: `errors` for engine open-path failures
- `Consumer`: bindings, SDK callers, operator dispatch logic
- `Direction`: engine/errors -> caller
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/errors.md`
- `Other referencing docs`: `docs/0.6.0/design/engine.md`, `docs/0.6.0/design/recovery.md`, `docs/0.6.0/design/bindings.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Open-path corruption is surfaced as a typed payload joining `CorruptionKind`, `OpenStage`, `CorruptionLocator`, and `RecoveryHint { code, doc_anchor }`; only four open-path corruption rows exist in 0.6.0.
- `Key types/fields/enums/errors`: `CorruptionKind`, `OpenStage`, `CorruptionLocator`, `RecoveryHint.code`, `RecoveryHint.doc_anchor`
- `Requirement/AC refs`: `REQ-031d`, `AC-035a`, `AC-035b`, `AC-035c`
- `Evidence`: `docs/0.6.0/design/errors.md` § `Corruption detail owner`; `docs/0.6.0/design/errors.md` § ``Engine.open` corruption table`; `docs/0.6.0/acceptance.md` § `AC-035b: CorruptionDetail shape`
- `Open questions`: Per-language field casing is delegated to interface docs that are still `TBD`.

## IF-014 — Typed Error Mapping Protocol

- `ID`: `IF-014`
- `Name`: Cross-language typed error mapping
- `Class`: Binding adapter
- `Producer`: `bindings` using `errors` as input authority
- `Consumer`: Python/TypeScript/CLI callers
- `Direction`: engine error -> binding-native error surface
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/bindings.md`
- `Other referencing docs`: `docs/0.6.0/design/errors.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Each error variant maps to a distinct typed class, with one rooted hierarchy per binding and typed attributes instead of message parsing.
- `Key types/fields/enums/errors`: `EngineError`, `EngineOpenError`, variant-specific classes, structural `recovery_hint.code`
- `Requirement/AC refs`: `REQ-056`, `AC-060a`, `AC-060b`
- `Evidence`: `docs/0.6.0/design/bindings.md` § `3. Error-mapping protocol`; `docs/0.6.0/design/errors.md` § `Binding mapping ownership`; `docs/0.6.0/acceptance.md` § `AC-060a: Engine errors as typed language-idiomatic exceptions`
- `Open questions`: The corpus does not include the per-language variant-to-class matrix or concrete class names.

## IF-015 — Subscriber Attachment Contract

- `ID`: `IF-015`
- `Name`: Host subscriber attachment and payload stability
- `Class`: Binding adapter
- `Producer`: `bindings`
- `Consumer`: Python logging backends, TypeScript callbacks, CLI human/machine output adapters
- `Direction`: binding adapter <-> host runtime
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/bindings.md`
- `Other referencing docs`: `docs/0.6.0/design/lifecycle.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: Bindings attach host-native subscribers while preserving the engine event payload as a stable `fathomdb` payload with the same fields, types, and phase enum across bindings.
- `Key types/fields/enums/errors`: stable `fathomdb` payload key; lifecycle `phase`; typed diagnostic fields
- `Requirement/AC refs`: `REQ-001`, `REQ-002`, `REQ-005`, `AC-001`, `AC-002`, `AC-003a`, `AC-003b`, `AC-003c`, `AC-003d`, `AC-006`
- `Evidence`: `docs/0.6.0/design/bindings.md` § `8. Logging / tracing subscriber attachment`; `docs/0.6.0/design/lifecycle.md` § `Host-routed diagnostics`
- `Open questions`: Acceptance does not currently assert the stable `fathomdb` payload key itself.

## IF-016 — `doctor check-integrity --json` Report

- `ID`: `IF-016`
- `Name`: `check-integrity` single-object JSON report
- `Class`: CLI/operator
- `Producer`: `recovery`
- `Consumer`: operators and automation invoking `fathomdb doctor`
- `Direction`: CLI -> operator/tool
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/recovery.md`
- `Other referencing docs`: `docs/0.6.0/interfaces/cli.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/design/errors.md`
- `Contract summary`: `doctor check-integrity --json` emits a single JSON object with top-level keys `physical`, `logical`, and `semantic`; findings carry stable report fields and may include doctor-only codes.
- `Key types/fields/enums/errors`: top-level keys `physical`, `logical`, `semantic`; finding fields `code`, `stage`, `locator`, `doc_anchor`, `detail`; doctor-only code `E_CORRUPT_INTEGRITY_CHECK`
- `Requirement/AC refs`: `REQ-036`, `REQ-039`, `AC-043a`, `AC-043b`, `AC-043c`
- `Evidence`: `docs/0.6.0/design/recovery.md` § `Machine-readable output`; `docs/0.6.0/design/recovery.md` § ``check-integrity` schema owner`; `docs/0.6.0/acceptance.md` § `AC-043a: \`check-integrity\` produces structured report with three sections`
- `Open questions`: The specific check set inside each section is intentionally not fully enumerated in the listed docs.

## IF-017 — `fathomdb doctor` Root and Verb Table

- `ID`: `IF-017`
- `Name`: Read-only doctor command surface
- `Class`: CLI/operator
- `Producer`: `recovery` semantics plus CLI flag/path layer
- `Consumer`: operators
- `Direction`: operator -> CLI
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/recovery.md`
- `Other referencing docs`: `docs/0.6.0/interfaces/cli.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: `fathomdb doctor` is the read-only root with verbs `check-integrity`, `safe-export`, `verify-embedder`, `trace`, `dump-schema`, `dump-row-counts`, and `dump-profile`; `--json` is the normative machine-readable posture.
- `Key types/fields/enums/errors`: doctor verbs; doctor-only flags `--quick`, `--full`, `--round-trip`; optional `--pretty`
- `Requirement/AC refs`: `REQ-036`, `REQ-039`, `AC-040a`, `AC-040b`
- `Evidence`: `docs/0.6.0/design/recovery.md` § `Two-root CLI split`; `docs/0.6.0/interfaces/cli.md` § `Doctor verbs`; `docs/0.6.0/acceptance.md` § `AC-040a: Every \`fathomdb doctor\` verb invocable`
- `Open questions`: JSON payload shapes for doctor verbs other than `check-integrity` remain draft-only in `design/recovery.md`.

## IF-018 — `fathomdb recover --accept-data-loss`

- `ID`: `IF-018`
- `Name`: Lossy recovery root
- `Class`: CLI/operator
- `Producer`: `recovery` semantics plus CLI flag/path layer
- `Consumer`: operators
- `Direction`: operator -> CLI
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/recovery.md`
- `Other referencing docs`: `docs/0.6.0/interfaces/cli.md`, `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`
- `Contract summary`: `recover` is the only lossy root, requires root-level `--accept-data-loss`, and exposes the accepted sub-flags for truncation, vec0 rebuild, projection rebuild, excision, purge, and restore.
- `Key types/fields/enums/errors`: `--accept-data-loss`; `--truncate-wal`; `--rebuild-vec0`; `--rebuild-projections`; `--excise-source <id>`; `--purge-logical-id <id>`; `--restore-logical-id <id>`
- `Requirement/AC refs`: `REQ-036`, `REQ-054`, `REQ-059`, `AC-035d`, `AC-058`
- `Evidence`: `docs/0.6.0/design/recovery.md` § `Two-root CLI split`; `docs/0.6.0/interfaces/cli.md` § `Recover root`; `docs/0.6.0/acceptance.md` § `AC-058: Recovery verbs CLI-reachable`
- `Open questions`: The machine-readable progress-stream and terminal-summary schema are declared as owned by recovery but are not fully enumerated in the listed docs.

## IF-019 — `safe_export` Artifact and Manifest Contract

- `ID`: `IF-019`
- `Name`: Verifiable safe-export artifact
- `Class`: CLI/operator
- `Producer`: `recovery`
- `Consumer`: operators and verification tooling
- `Direction`: CLI -> operator/tool
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/recovery.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`, `docs/0.6.0/interfaces/cli.md`
- `Contract summary`: `safe_export` is a doctor verb whose artifact must be verifiable end-to-end through a SHA-256 manifest sidecar.
- `Key types/fields/enums/errors`: artifact plus `.sha256` manifest
- `Requirement/AC refs`: `REQ-024`, `REQ-035`, `AC-026`, `AC-039a`, `AC-039b`
- `Evidence`: `docs/0.6.0/design/recovery.md` § `Two-root CLI split`; `docs/0.6.0/requirements.md` § `Security (REQ-032..REQ-035)`; `docs/0.6.0/acceptance.md` § `AC-039a: \`safe_export\` artifact ships SHA-256 manifest matching contents`; `docs/0.6.0/architecture.md` § `On-disk layout`
- `Open questions`: The machine-readable JSON payload for `doctor safe-export --json` is not yet defined in the listed docs.

## IF-020 — `trace --source-ref` Blast-Radius Report

- `ID`: `IF-020`
- `Name`: Source-ref trace report
- `Class`: CLI/operator
- `Producer`: `recovery`
- `Consumer`: operators
- `Direction`: CLI -> operator/tool
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/recovery.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/interfaces/cli.md`
- `Contract summary`: `fathomdb doctor trace --source-ref <id>` returns exactly the canonical-row id set produced by the named source reference.
- `Key types/fields/enums/errors`: `source-ref`; canonical row id set
- `Requirement/AC refs`: `REQ-038`, `AC-042`
- `Evidence`: `docs/0.6.0/design/recovery.md` § `Two-root CLI split`; `docs/0.6.0/acceptance.md` § `AC-042: Source-ref blast-radius enumeration exact`
- `Open questions`: The actual machine-readable output shape is not specified in the listed docs.

## IF-021 — Freshness Cursor Pair

- `ID`: `IF-021`
- `Name`: Write cursor and read-side `projection_cursor`
- `Class`: Data/schema/payload
- `Producer`: `engine` and `projection`
- `Consumer`: SDK callers reasoning about projection freshness
- `Direction`: engine -> caller
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/design/engine.md`
- `Other referencing docs`: `docs/0.6.0/requirements.md`, `docs/0.6.0/acceptance.md`, `docs/0.6.0/architecture.md`, `docs/0.6.0/design/bindings.md`
- `Contract summary`: Write commit returns a monotonic write cursor `c_w`, reads expose a monotonic non-decreasing `projection_cursor`, and vector read-after-write is satisfied when `projection_cursor >= c_w`.
- `Key types/fields/enums/errors`: write cursor `c_w`; `projection_cursor`
- `Requirement/AC refs`: `REQ-055`, `AC-017`, `AC-059a`, `AC-059b`
- `Evidence`: `docs/0.6.0/design/engine.md` § `Cursor contract`; `docs/0.6.0/acceptance.md` § `AC-059a: \`projection_cursor\` exposed on read tx; monotonic non-decreasing`; `docs/0.6.0/acceptance.md` § `AC-059b: Write commit returns write cursor satisfiable by \`projection_cursor\``
- `Open questions`: Per-binding field names and return wrappers are not yet published in the interface docs.

## IF-022 — CLI Exit-Code Classes

- `ID`: `IF-022`
- `Name`: Stable CLI exit-code classes
- `Class`: CLI/operator
- `Producer`: CLI interface layer
- `Consumer`: operators and automation consuming process exits
- `Direction`: CLI -> shell/tooling
- `Public/Internal`: Public
- `Canonical owning doc`: `docs/0.6.0/interfaces/cli.md`
- `Other referencing docs`: `docs/0.6.0/design/recovery.md`, `docs/0.6.0/design/errors.md`
- `Contract summary`: The CLI owns concrete exit-code classes for doctor and recover flows, while semantic error-to-exit routing is constrained by recovery and errors ownership.
- `Key types/fields/enums/errors`: `doctor-check-*`, `doctor-export-*`, `recover-*`
- `Requirement/AC refs`: `REQ-036`, `REQ-054`, `AC-040a`, `AC-040b`, `AC-058`
- `Evidence`: `docs/0.6.0/interfaces/cli.md` § `Doctor verbs`; `docs/0.6.0/interfaces/cli.md` § `Recover root`; `docs/0.6.0/design/errors.md` § `Binding mapping ownership`
- `Open questions`: The exact variant-to-exit-code matrix is delegated to `design/errors.md` / future CLI material and is not fully enumerated here.
