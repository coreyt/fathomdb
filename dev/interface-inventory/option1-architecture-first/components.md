# Components

## Engine

- Purpose: Own `Engine.open` / `close`, writer-reader split, batch write semantics, cursor semantics, lock lifetime, and the semver-stable Rust-facing facade over `fathomdb-engine`.
- Incoming interfaces: `IF-001`, `IF-002`, `IF-003`, `IF-004`.
- Outgoing interfaces: `IF-005`, `IF-011`, `IF-015`, `IF-016`.
- Public surfaces it owns: open-path sequencing, ordered batch commit semantics, write cursor vs `projection_cursor`, single-file file-set behavior.
- Explicit non-ownership boundaries: lifecycle event taxonomy belongs to `lifecycle`; cross-language parity and adapter protocol belong to `bindings`; recovery verbs are CLI-only and belong to `recovery` / `cli`; migration step payloads belong to `migrations`.

## Lifecycle

- Purpose: Own response-cycle phases, slow/heartbeat semantics, host-routed diagnostics, counter snapshots, profile record shape, and stress-failure payloads.
- Incoming interfaces: operations observed through `IF-001`, `IF-002`, `IF-003`, `IF-012`.
- Outgoing interfaces: `IF-006`, `IF-007`, `IF-008`, `IF-009`, `IF-010`.
- Public surfaces it owns: typed `phase`, typed `source` / `category`, public counter-key set, public profile-record fields, stress-failure payload schema.
- Explicit non-ownership boundaries: subscriber registration protocol belongs to `bindings`; migration-step payload schema belongs to `migrations`; CLI JSON output belongs to `recovery` / `cli`; engine lifetime cleanup belongs to `engine`.

## Bindings Facade

- Purpose: Own cross-language parity, dispatch model, adapter marshalling, error-mapping protocol, SDK/CLI separation, and host-subscriber attachment rules.
- Incoming interfaces: `IF-001`, `IF-002`, `IF-003`, `IF-011`, `IF-012`.
- Outgoing interfaces: `IF-004`, `IF-007`, `IF-011`.
- Public surfaces it owns: five-verb SDK parity rule, sync Python vs Promise TS dispatch posture, typed write marshalling rules, typed error protocol, recovery non-presence on SDKs.
- Explicit non-ownership boundaries: per-language symbol spelling belongs to `interfaces/{rust,python,typescript}.md`; CLI flag spelling and exit classes belong to `interfaces/cli.md`; module-specific error semantics belong to `errors` plus the producing subsystem docs.

## Errors

- Purpose: Own top-level error roots, module/direct variant taxonomy, corruption-detail payload fields, and the stable inputs used for binding mappings.
- Incoming interfaces: failures emitted by `engine`, `migrations`, `op_store`, `projection`, and `recovery`-adjacent open paths.
- Outgoing interfaces: `IF-011`.
- Public surfaces it owns: `EngineError`, `EngineOpenError`, per-variant distinctness, `CorruptionDetail`, `RecoveryHint.code`, `RecoveryHint.doc_anchor`.
- Explicit non-ownership boundaries: when open-path stages fail belongs to `engine`; migration failure semantics belong to `migrations`; per-language class names belong to `interfaces/{python,typescript,cli}.md`.

## Migrations

- Purpose: Own auto-migration during `Engine.open`, per-step migration event semantics, and the schema-accretion guard.
- Incoming interfaces: `IF-001`, `IF-002`, `IF-003`, `IF-015`.
- Outgoing interfaces: `IF-015`.
- Public surfaces it owns: open-time migration application, per-step duration events, failed-step marker, migration-failure error identity, accretion-guard rule.
- Explicit non-ownership boundaries: lifecycle only owns subscriber routing and phase envelope; engine owns the broader open-path ordering; schema-version incompatibility at 0.5.x re-open is an engine/open compatibility outcome, not a migration payload definition.

## Recovery

- Purpose: Own operator-only corruption inspection, export, verification, tracing, and lossy recovery workflows.
- Incoming interfaces: `IF-012`, `IF-013`, `IF-014`, `IF-017`.
- Outgoing interfaces: `IF-012`, `IF-013`, `IF-014`, `IF-017`.
- Public surfaces it owns: `doctor` / `recover` split, `check-integrity` JSON report, `safe_export` artifact contract, canonical regenerate workflow name and command path.
- Explicit non-ownership boundaries: CLI concrete flag spelling and exit classes belong to `interfaces/cli.md`; `CorruptionDetail` payload shape belongs to `errors`; projection terminal-state semantics belong to `projection`.

## Op Store

- Purpose: Own operational collection registry, authoritative `append_only_log` / `latest_state` semantics, accepted `operational_*` table set, and save-time schema validation behavior.
- Incoming interfaces: `IF-004`, `IF-017`.
- Outgoing interfaces: `IF-004`, `IF-017`.
- Public surfaces it owns: `PreparedWrite::OpStore(OpStoreInsert)` storage semantics, `operational_collections`, `operational_mutations`, `operational_state`, `projection_failures` storage class.
- Explicit non-ownership boundaries: scheduler/projection retry policy belongs to `projection`; write batching and transaction envelope belong to `engine`; CLI repair commands belong to `recovery`.

## Projection

- Purpose: Own push projection semantics, terminal status model, `projection_cursor` advancement rules, and failure-to-regenerate workflow semantics.
- Incoming interfaces: `IF-005`, `IF-017`.
- Outgoing interfaces: `IF-005`, `IF-017`.
- Public surfaces it owns: `Pending` / `Failed` / `UpToDate`, terminal advancement of `projection_cursor`, durable failure recording preconditions, explicit regenerate requirement.
- Explicit non-ownership boundaries: failure-row table semantics belong to `op_store`; concrete recovery command spelling belongs to `recovery` / `cli`; write cursor allocation belongs to `engine`.

## Rust SDK

- Purpose: Expose the semver-stable Rust application runtime surface over the engine.
- Incoming interfaces: `IF-001`, `IF-004`, `IF-005`, `IF-011`, `IF-015`, `IF-016`.
- Outgoing interfaces: calls into `engine`, `lifecycle`, `migrations`, and `errors`.
- Public surfaces it owns: per-symbol Rust signatures and stability posture once `interfaces/rust.md` is populated.
- Explicit non-ownership boundaries: parity rules belong to `bindings`; internal engine module boundaries are not public.

## Python SDK

- Purpose: Expose the sync Python application runtime surface and Python logging/subscriber adapter.
- Incoming interfaces: `IF-002`, `IF-004`, `IF-005`, `IF-007`, `IF-011`, `IF-015`.
- Outgoing interfaces: calls into `bindings` and `engine`.
- Public surfaces it owns: per-symbol Python spellings, exception class names, and Python-side subscriber helper once `interfaces/python.md` is populated.
- Explicit non-ownership boundaries: dispatch model and parity rules belong to `bindings`; recovery is CLI-only; lifecycle payload schemas belong to `lifecycle`.

## TypeScript SDK

- Purpose: Expose the Promise-based TypeScript application runtime surface and callback-style subscriber adapter.
- Incoming interfaces: `IF-003`, `IF-004`, `IF-005`, `IF-007`, `IF-011`, `IF-015`.
- Outgoing interfaces: calls into `bindings` and `engine`.
- Public surfaces it owns: per-symbol TS spellings, error class names, export layout, and callback signatures once `interfaces/typescript.md` is populated.
- Explicit non-ownership boundaries: thread-pool adapter protocol belongs to `bindings`; recovery is CLI-only; event payload schemas belong to `lifecycle`.

## CLI

- Purpose: Expose the operator command surface, help text, concrete flag spelling, root command paths, and exit classes.
- Incoming interfaces: `IF-012`, `IF-013`, `IF-014`, `IF-017`.
- Outgoing interfaces: invokes `recovery`, `engine`, `migrations`, and `errors` through operator workflows.
- Public surfaces it owns: `fathomdb doctor <verb>`, `fathomdb recover --accept-data-loss ...`, `--json` posture, exit classes.
- Explicit non-ownership boundaries: recovery semantics belong to `recovery`; migration and corruption payload semantics belong to `migrations` and `errors`; SDK parity does not apply.
