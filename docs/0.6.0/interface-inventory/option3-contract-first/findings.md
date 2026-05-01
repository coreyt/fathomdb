# Findings

## F-001

- `severity`: high
- `affected files`: `docs/0.6.0/interfaces/rust.md`, `docs/0.6.0/interfaces/python.md`, `docs/0.6.0/interfaces/typescript.md`, `docs/0.6.0/interfaces/wire.md`, `docs/0.6.0/design/bindings.md`
- `short explanation`: The public per-language interface docs are still `TBD`, so `design/bindings.md` is currently carrying public contract ownership for symbol sets, error spelling, and subscriber attachment details that it explicitly says should live in `interfaces/*.md`.
- `recommended canonical owner`: the respective `interfaces/*.md` files for symbol spelling and per-language shapes; `design/bindings.md` should remain the protocol owner only
- `minimal doc fix`: replace each `TBD` interface file with a minimal symbol-and-payload table that cites `design/bindings.md` only for cross-language invariants

## F-002

- `severity`: high
- `affected files`: `docs/0.6.0/design/migrations.md`, `docs/0.6.0/design/lifecycle.md`, `docs/0.6.0/design/bindings.md`, `docs/0.6.0/acceptance.md`
- `short explanation`: Migration progress is a public contract routed through lifecycle subscribers and surfaced through bindings, but the claimed owner `design/migrations.md` currently stops at an ownership statement. The actual payload fields live only in `AC-046b` and `AC-046c`.
- `recommended canonical owner`: `docs/0.6.0/design/migrations.md`
- `minimal doc fix`: add a migration-event schema section naming `step_id`, `duration_ms`, failure signaling, and the ownership split with lifecycle

## F-003

- `severity`: medium
- `affected files`: `docs/0.6.0/design/errors.md`, `docs/0.6.0/design/engine.md`, `docs/0.6.0/design/recovery.md`, `docs/0.6.0/interfaces/cli.md`
- `short explanation`: The open-path corruption contract is precise but scattered. `design/errors.md` owns the payload join, `design/engine.md` owns when it is emitted, `design/recovery.md` owns the operator routes behind `RecoveryHint.doc_anchor`, and `interfaces/cli.md` owns exit classes. A reader has to reconcile all four docs to connect one machine-readable `code` to one operator action.
- `recommended canonical owner`: `docs/0.6.0/design/errors.md` for the payload; `docs/0.6.0/design/recovery.md` for operator routing
- `minimal doc fix`: add a short cross-reference table in `design/recovery.md` or `interfaces/cli.md` that points each recovery code back to the canonical `CorruptionDetail` owner

## F-004

- `severity`: medium
- `affected files`: `docs/0.6.0/design/recovery.md`, `docs/0.6.0/interfaces/cli.md`, `docs/0.6.0/interfaces/python.md`, `docs/0.6.0/interfaces/typescript.md`
- `short explanation`: Doctor-only flags and report surfaces are clearly CLI-only in `design/recovery.md`, but the absent Python/TypeScript interface docs mean there is no per-SDK document that explicitly rules out `--quick`, `--full`, `--round-trip`, or any corresponding `Engine.open` knobs. That leaves room for CLI-only surfaces to be inferred as SDK capabilities.
- `recommended canonical owner`: `docs/0.6.0/design/recovery.md` for the operator flags; per-language interface docs for explicit SDK non-presence
- `minimal doc fix`: when the Python and TypeScript interface docs land, add an explicit "not in SDK" note for doctor-only flags and recovery verbs

## F-005

- `severity`: medium
- `affected files`: `docs/0.6.0/design/lifecycle.md`, `docs/0.6.0/design/bindings.md`, `docs/0.6.0/design/recovery.md`, `docs/0.6.0/acceptance.md`
- `short explanation`: Observability/reporting contract breadth exceeds acceptance coverage in a few places. The docs claim a stable `fathomdb` payload key in host records and a recovery progress-stream schema owned by recovery, but acceptance only locks the lifecycle enum, selected diagnostic fields, `check-integrity`, and some CLI reachability.
- `recommended canonical owner`: `docs/0.6.0/design/lifecycle.md` for host-record payload stability; `docs/0.6.0/design/recovery.md` for recover progress-stream shape
- `minimal doc fix`: either narrow the prose to covered guarantees or add ACs for the host-record payload key and recover progress-stream fields

## F-006

- `severity`: medium
- `affected files`: `docs/0.6.0/architecture.md`
- `short explanation`: The architecture rows for `lifecycle`, `recovery`, and `bindings facade` flatten multiple contract classes too aggressively. `lifecycle` combines response-cycle events, diagnostics, counters, and profiling; `recovery` combines two CLI roots plus report schemas; `bindings facade` combines public verb parity, error mapping, cursor exposure, and deprecation policy.
- `recommended canonical owner`: `docs/0.6.0/architecture.md`
- `minimal doc fix`: split those rows into sub-surfaces or add a contract-class note per row so public API, observability, and CLI/reporting contracts remain distinguishable at the architecture layer
