# Components

## Engine

- Purpose: owns runtime `Engine.open` / `close`, writer-reader split, batch write semantics, and cursor meaning
- Contracts it produces: `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-013`, `IF-021`
- Contracts it consumes: `IF-012`, `IF-014`, lifecycle envelopes for observed operations
- Public-facing surfaces it participates in: SDK verbs, typed open/runtime errors, write cursor, read-side `projection_cursor`
- Explicit non-ownership boundaries: does not own lifecycle phase schema, migration step payload schema, or CLI machine-readable recovery output

## Lifecycle

- Purpose: owns operator-facing typed observability for live operations
- Contracts it produces: `IF-007`, `IF-008`, `IF-009`, `IF-010`, `IF-011`
- Contracts it consumes: producer-specific event envelopes from engine, migrations, and recovery-originated work
- Public-facing surfaces it participates in: response-cycle feedback, structured diagnostics, counters, profiling, stress-failure observability
- Explicit non-ownership boundaries: does not own `Engine.open` / `close` lifetime, migration step schema, cross-language subscriber registration, or CLI JSON

## Bindings

- Purpose: owns cross-language protocol invariants across Python, TypeScript, and CLI
- Contracts it produces: `IF-001`, `IF-002`, `IF-014`, `IF-015`
- Contracts it consumes: engine runtime contracts, lifecycle event payloads, `CorruptionDetail`, CLI-owned verb schemas
- Public-facing surfaces it participates in: SDK parity, typed error mapping, subscriber adapters, FFI marshalling, SDK recovery non-presence
- Explicit non-ownership boundaries: does not own per-language symbol spelling, CLI verb tables, or the variant-to-class matrix itself

## Recovery

- Purpose: owns corruption inspection, export, and recovery operator workflows
- Contracts it produces: `IF-016`, `IF-017`, `IF-018`, `IF-019`, `IF-020`, `IF-022`
- Contracts it consumes: `RecoveryHint` codes from `IF-013`, durable projection-failure state, CLI parser/output ownership
- Public-facing surfaces it participates in: `fathomdb doctor`, `fathomdb recover`, `check-integrity`, `safe_export`, trace, projection regeneration
- Explicit non-ownership boundaries: does not own SDK runtime verbs, `Engine.open` corruption enums, or public error-class mapping

## Migrations

- Purpose: owns the schema migration loop at `Engine.open`
- Contracts it produces: `IF-012`
- Contracts it consumes: engine open-path invocation, lifecycle routing, binding error surfacing
- Public-facing surfaces it participates in: auto-migration at open and per-step migration progress/failure reporting
- Explicit non-ownership boundaries: does not own lifecycle phase taxonomy, lock acquisition, or subscriber registration mechanics

## Rust API

- Purpose: intended per-language Rust public interface for runtime operations
- Contracts it produces: should spell `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-013`, `IF-021`
- Contracts it consumes: `design/bindings.md` cross-cutting protocol and engine-owned semantics
- Public-facing surfaces it participates in: runtime API and typed errors
- Explicit non-ownership boundaries: current doc is `TBD`; canonical symbol-level ownership is not yet materialized in `dev/interfaces/rust.md`

## Python SDK / Python API

- Purpose: Python binding over the runtime SDK
- Contracts it produces: Python spelling of `IF-001`, `IF-002`, `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-014`, `IF-015`, `IF-021`
- Contracts it consumes: engine runtime semantics, lifecycle payloads, bindings protocol
- Public-facing surfaces it participates in: five-verb SDK, typed exceptions, Python logging-backed subscriber adapter
- Explicit non-ownership boundaries: current doc is `TBD`; exact helper names, class names, and field casing are not yet written in `dev/interfaces/python.md`

## TypeScript SDK / TypeScript API

- Purpose: Promise-based TypeScript binding over the runtime SDK
- Contracts it produces: TypeScript spelling of `IF-001`, `IF-002`, `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-014`, `IF-015`, `IF-021`
- Contracts it consumes: engine runtime semantics, lifecycle payloads, bindings protocol
- Public-facing surfaces it participates in: five-verb SDK, typed error classes, callback subscriber adapter
- Explicit non-ownership boundaries: current doc is `TBD`; exact export list, error class names, and field casing are not yet written in `dev/interfaces/typescript.md`

## CLI

- Purpose: operator-only binary surface
- Contracts it produces: `IF-017`, `IF-018`, `IF-022`
- Contracts it consumes: recovery verb semantics, error-code routing, lifecycle diagnostics when rendered for humans
- Public-facing surfaces it participates in: root command layout, doctor verbs, recovery flags, exit-code classes, `--json` / `--pretty` posture
- Explicit non-ownership boundaries: does not own SDK parity, `Engine.open` corruption enums, or migration-step payload semantics

## Subscriber / Observability Surface

- Purpose: host-facing route for typed diagnostics and liveness
- Contracts it produces: transport of `IF-007`, `IF-008`, `IF-010`, `IF-011`, and routed `IF-012`
- Contracts it consumes: lifecycle payloads and binding-specific attachment rules
- Public-facing surfaces it participates in: host subscriber hookup, machine-readable event delivery, CLI console output in human mode
- Explicit non-ownership boundaries: does not own engine lifetime or CLI report schemas

## Machine-Readable Public Surfaces

- Purpose: groups payload-bearing contracts a tool can consume without parsing prose
- Contracts it produces: `IF-009`, `IF-010`, `IF-011`, `IF-013`, `IF-016`, `IF-019`, `IF-021`, `IF-022`
- Contracts it consumes: engine, lifecycle, errors, recovery, and CLI flag ownership
- Public-facing surfaces it participates in: JSON reports, typed error payloads, counter snapshots, cursor values, profile records
- Explicit non-ownership boundaries: it is a contract grouping, not a subsystem; concrete ownership remains with lifecycle, errors, engine, recovery, or CLI docs
