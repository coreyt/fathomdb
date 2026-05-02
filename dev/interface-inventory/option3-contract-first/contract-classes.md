# Contract Classes

## Inventory

| Class               | Surfaces in class                                                                                                                                                   |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Public API          | SDK five-verb surface, `Engine.open`, `write`, `search`, `admin.configure`, `close`, read/write freshness cursors                                                   |
| Binding adapter     | Cross-language parity, typed error mapping, subscriber attachment, marshalling to `PreparedWrite`, lock/exclusivity behavior, SDK-side recovery non-presence        |
| Event/observability | Lifecycle `phase` events, structured diagnostics, counter snapshots, per-statement profile records, stress-failure payloads, migration events routed to subscribers |
| CLI/operator        | `fathomdb doctor`, `fathomdb recover --accept-data-loss`, help/exit posture, doctor-only flags                                                                      |
| Data/schema/payload | `CorruptionDetail`, `RecoveryHint`, `CorruptionLocator`, `check-integrity` report schema, cursor values, migration step payload, profile-record schema              |
| Internal subsystem  | Engine open-path ordering, writer/read split, migration loop, lifecycle-to-producer ownership split, recovery/projection regeneration routing                       |

## Public API

- Representative interfaces: `IF-001`, `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-021`
- Primary producers: `engine`, `bindings`, Python SDK, TypeScript SDK
- Primary consumers: application callers, tests, host runtimes
- Canonical owning docs: `dev/design/engine.md`, `dev/design/bindings.md`, `dev/requirements.md`
- Notable overlap risks: per-language public symbol ownership is deferred because `interfaces/rust.md`, `interfaces/python.md`, and `interfaces/typescript.md` are still `TBD`

## Binding Adapter

- Representative interfaces: `IF-002`, `IF-014`, `IF-015`
- Primary producers: `bindings`
- Primary consumers: Python SDK, TypeScript SDK, CLI adapter layer, host logging/runtime adapters
- Canonical owning docs: `dev/design/bindings.md`, `dev/design/errors.md`
- Notable overlap risks: `design/bindings.md` owns protocol, while per-language spelling is supposed to live in interface docs that do not yet exist

## Event/Observability

- Representative interfaces: `IF-007`, `IF-008`, `IF-009`, `IF-010`, `IF-011`, `IF-012`
- Primary producers: `lifecycle`, `migrations`, engine-originated diagnostics, SQLite-internal diagnostics
- Primary consumers: application code, operators, subscriber adapters, CLI human output
- Canonical owning docs: `dev/design/lifecycle.md`, `dev/design/migrations.md`
- Notable overlap risks: lifecycle owns event envelopes, but migration payload ownership is delegated away and not yet fully written down

## CLI/Operator

- Representative interfaces: `IF-016`, `IF-017`, `IF-018`, `IF-019`, `IF-020`, `IF-022`
- Primary producers: `recovery`, CLI parser/output layer
- Primary consumers: operators, automation invoking `fathomdb doctor` or `fathomdb recover`
- Canonical owning docs: `dev/design/recovery.md`, `dev/interfaces/cli.md`
- Notable overlap risks: `design/recovery.md` owns verb semantics and report schemas, while `interfaces/cli.md` owns flags and exit classes; JSON shapes for several verbs are still draft-only

## Data/Schema/Payload

- Representative interfaces: `IF-010`, `IF-011`, `IF-012`, `IF-013`, `IF-016`, `IF-021`
- Primary producers: `lifecycle`, `migrations`, `errors`, `engine`, `recovery`
- Primary consumers: bindings, SDK callers, CLI automation, operator tooling
- Canonical owning docs: `dev/design/errors.md`, `dev/design/lifecycle.md`, `dev/design/engine.md`, `dev/design/recovery.md`
- Notable overlap risks: the same stable `code` surface is shared by open-path corruption dispatch and doctor findings, but only some codes map to `Engine.open` enums

## Internal Subsystem

- Representative interfaces: `IF-003`, `IF-004`, `IF-006`, `IF-012`
- Primary producers: `engine`, `migrations`, `recovery`
- Primary consumers: `bindings`, `lifecycle`, CLI adapter layer
- Canonical owning docs: `dev/design/engine.md`, `dev/design/migrations.md`, `dev/architecture.md`
- Notable overlap risks: `architecture.md` groups distinct public contract classes under single subsystem rows, especially for `lifecycle`, `recovery`, and `bindings facade`
