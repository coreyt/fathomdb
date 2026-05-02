# Findings

## Finding 1

- Severity: `high`
- Affected files: `dev/interfaces/rust.md`; `dev/interfaces/python.md`; `dev/interfaces/typescript.md`; `dev/interfaces/wire.md`
- Short explanation: The declared canonical public owners for Rust, Python, TypeScript, and wire/on-disk surfaces are still `TBD`. Cross-binding rules exist in `design/bindings.md`, but the per-surface contract documents that should own symbol spelling, examples, stability posture, export names, and wire-format specifics are not written.
- Recommended canonical owner: Keep the current ownership split: `interfaces/{rust,python,typescript,wire}.md` own per-surface contracts; `design/bindings.md` stays cross-cutting only.
- Minimal doc fix: Populate each interface file with the actual public symbols, config object/kwargs shapes, error class names, subscriber-registration calls, and any wire/on-disk sentinel/version details that are meant to be public.

## Finding 2

- Severity: `high`
- Affected files: `dev/design/lifecycle.md`; `dev/interfaces/python.md`; `dev/interfaces/typescript.md`; `dev/interfaces/rust.md`; `dev/acceptance.md`
- Short explanation: Lifecycle locks counter snapshots, profiling, and slow-threshold reconfiguration, and acceptance requires a documented API call for profiling toggle, but no public interface doc names the access path for counters, profiles, or runtime threshold changes in any binding.
- Recommended canonical owner: Keep payload semantics in `design/lifecycle.md`; put binding-specific entry points and names in `interfaces/{rust,python,typescript}.md`.
- Minimal doc fix: Add one short section per language interface that names the snapshot/profile/subscriber APIs and the runtime slow-threshold/profiling toggles referenced by `AC-005a`, `AC-007b`, and the lifecycle requirements.

## Finding 3

- Severity: `high`
- Affected files: `dev/design/migrations.md`; `dev/design/engine.md`; `dev/acceptance.md`; `dev/requirements.md`
- Short explanation: `design/migrations.md` claims ownership of the migration loop and per-step event contract, but it contains only a stub. Acceptance already locks `step_id`, `duration_ms`, `failed`, auto-apply behavior, and the accretion-guard linter, so the intended canonical owner is not actually carrying the contract.
- Recommended canonical owner: `dev/design/migrations.md`
- Minimal doc fix: Expand `design/migrations.md` to define the migration event payload schema, success/failure open-result fields, how migration events route through lifecycle/subscribers, and the exact accretion-guard linter rule cited by `AC-049`.

## Finding 4

- Severity: `medium`
- Affected files: `dev/design/errors.md`; `dev/design/bindings.md`; `dev/acceptance.md`; `dev/interfaces/python.md`; `dev/interfaces/typescript.md`; `dev/interfaces/cli.md`
- Short explanation: Error ownership is split coherently in principle, but not fully materialized in practice. `design/errors.md` owns variant taxonomy and corruption detail, `design/bindings.md` owns mapping protocol, and the per-language interface docs are supposed to own concrete class names and attribute casing. Because those interface docs are missing, the binding-facing typed error contract is only partially specified.
- Recommended canonical owner: Keep `design/errors.md` as the canonical source for roots, variants, and stable payload fields; keep `interfaces/{python,typescript,cli}.md` as the canonical place for class names and field casing.
- Minimal doc fix: Add per-binding exception tables that reference `design/errors.md` variants directly instead of restating semantics, and add the CLI exit-code mapping matrix that `design/bindings.md` says should live outside the protocol doc.

## Finding 5

- Severity: `medium`
- Affected files: `dev/design/recovery.md`; `dev/interfaces/cli.md`; `dev/acceptance.md`
- Short explanation: The CLI claims `--json` as the normative machine-readable contract on every verb, but only `doctor check-integrity` has a structurally locked schema in the corpus. `safe-export`, `verify-embedder`, `trace`, `dump-schema`, `dump-row-counts`, `dump-profile`, and `recover` progress/summary are all named public surfaces without equivalent payload schemas.
- Recommended canonical owner: `dev/design/recovery.md` for payload semantics, with `dev/interfaces/cli.md` retaining concrete command/flag/exit-code spelling.
- Minimal doc fix: Add one subsection per CLI verb in `design/recovery.md` describing the JSON payload shape, then cross-reference those subsections from `interfaces/cli.md`.

## Finding 6

- Severity: `medium`
- Affected files: `dev/interfaces/rust.md`; `dev/acceptance.md`; `dev/requirements.md`
- Short explanation: The scope explicitly includes the Rust API, but acceptance only exercises Python and TypeScript for the five-verb SDK surface and typed exception behavior. The Rust surface therefore appears broader than its acceptance coverage.
- Recommended canonical owner: `dev/interfaces/rust.md` for the Rust surface definition, plus `dev/acceptance.md` if Rust parity is meant to be locked.
- Minimal doc fix: Either add Rust-facing acceptance for the five-verb/runtime cursor/error contract, or explicitly mark the Rust facade as an undocumented shim rather than an independently supported public interface for 0.6.0.

## Finding 7

- Severity: `low`
- Affected files: `dev/design/recovery.md`; `dev/design/projections.md`; `dev/design/op-store.md`; `dev/requirements.md`
- Short explanation: The `recover --rebuild-projections` regenerate workflow is described in three places with different ownership angles: command path in recovery, terminal-state semantics in projections, and durable row storage in op-store. The split is intentional, but the failure-row payload itself is never documented by any owner.
- Recommended canonical owner: `dev/design/op-store.md` should own the durable failure-row schema; `dev/design/projections.md` should continue to own when it is emitted and why regenerate is required.
- Minimal doc fix: Add a short `projection_failures` row-schema subsection to `design/op-store.md` and cross-link it from projections and recovery.
