---
title: Matrix — Option 2
date: 2026-05-01
target_release: 0.6.0
desc: Public surface to component mapping; component-to-component interfaces supporting public surfaces
status: living
---

# Matrices

## Public surface → component

Rows are public surfaces enumerated in `public-surfaces.md`; columns
are components in `components.md`. A cell entry indicates the
component contributes to the surface and what it contributes.

| Surface | engine | lifecycle | bindings | recovery | migrations | Python SDK | TypeScript SDK |
|---|---|---|---|---|---|---|---|
| S-1 Rust API | owns: open/write/search/close + cursor + EngineConfig + corruption taxonomy | feeds: phase + counter pull surface | feeds: parity + error mapping protocol; owns: SDK non-presence | — | feeds: open-time progress + MigrationError | — | — |
| S-2 Python API | owns: engine semantics behind binding | feeds: phase + diagnostics via Python subscriber | owns: parity + protocol commitments | — (recovery non-presence enforced via parity) | feeds: open events through Python subscriber | owns: idiomatic surface + sync dispatch + numpy zerocopy | — |
| S-3 TypeScript API | owns: engine semantics behind binding | feeds: phase + diagnostics via TS callback | owns: parity + Path 2 dispatch | — (recovery non-presence enforced via parity) | feeds: open events through TS callback | — | owns: Promise surface + napi-rs ThreadsafeFunction + Float32Array zerocopy |
| S-4 CLI | owns: open/close behind verb invocation | — (CLI machine mode emits verb-owned JSON; not subscriber-routed) | feeds: error mapping → exit code + JSON line | owns: verb table + check-integrity schema + recovery-hint anchors | — (migrations are not a doctor verb) | — | — |
| S-5 Subscriber/observability | feeds: emission sites for writer/search/admin/error categories + SQLite-internal events | owns: phase enum + categories + counter snapshot + profile record + stress-failure context | owns: subscriber attachment protocol + engine-event payload wire-stability | feeds: doctor + recover events emitted under same payload key (CLI mode) | feeds: per-step migration event with `step_id` + `duration_ms` + `failed` | owns: Python subscriber registration helper | owns: TS subscriber callback registration |
| S-6 Machine-readable output | feeds: WriteReceipt + projection_cursor + ProjectionStatus | feeds: counter / profile / stress / phase / category enums | feeds: typed-attribute error contract + `recovery_hint.code` dispatch | owns: doctor + recover JSON shapes; co-owns: doctor finding code set | feeds: MigrationError typed exception | feeds: idiomatic Python materialization | feeds: idiomatic TS materialization |

Empty cells (`—`) record explicit non-ownership / non-contribution
boundaries.

## Component-to-component interfaces directly supporting public

surfaces

Cells list the IF-### identifier(s) from `interfaces.md` that connect
the row component to the column component along a path that ends in a
public surface.

| From → To | engine | lifecycle | bindings | recovery | migrations |
|---|---|---|---|---|---|
| engine | (writer↔reader, internal) | IF-007, IF-008, IF-009, IF-010, IF-011 (engine emission sites → lifecycle module) | IF-002, IF-003, IF-004, IF-005, IF-006, IF-014, IF-015, IF-019, IF-020, IF-026 (engine surface → bindings facade marshalling) | IF-015, IF-019 (CorruptionDetail surface consumed by recovery for hint anchors; lock contract underpins recovery preconditions) | IF-002, IF-012 (Engine.open invokes migration loop; receives MigrationError) |
| lifecycle | — | (intra-module) | IF-007, IF-008, IF-013 (lifecycle phase + categories → bindings subscriber attachment) | IF-008 (CLI machine mode aligns with lifecycle source/category model when registering its console subscriber) | IF-012 (migration step events routed via lifecycle envelope) |
| bindings | IF-001, IF-003, IF-006, IF-022, IF-023, IF-024 (bindings facade → engine call shape; build path) | IF-013 (subscriber attachment → lifecycle delivery) | (intra-module) | IF-022 (recovery non-presence is enforced at bindings facade; CLI parity exclusion) | (no direct bindings ↔ migrations path; migrations is observed via subscriber) |
| recovery | IF-015 (recovery-hint anchors back-reference open-path corruption taxonomy) | IF-008 (doctor verbs emit through host subscriber in human mode) | IF-022 (recovery is unreachable from SDK by design) | (intra-module) | (recovery does not run migrations; AC-040a verb set excludes migrate) |
| migrations | IF-002 (migrations executes inside Engine.open) | IF-012 (per-step events delivered via lifecycle envelope) | IF-012, IF-014 (MigrationError surfaces via bindings facade error mapping) | (recovery is not a migration substitute) | (intra-module) |

Notes:

- The `bindings ↔ recovery` cell is intentionally about non-presence:
  `design/bindings.md` § 10 commits that the SDK surface contains no
  recovery verb. This is a public-surface-shaping interface (operators
  are guaranteed recovery is CLI-only).
- The `lifecycle ↔ recovery` cell is asymmetric: recovery owns its
  own `--json` machine contract per CLI verb, but in human mode the
  CLI attaches a console subscriber that routes through the same
  lifecycle host-subscriber model. `design/lifecycle.md` does not
  own the CLI verb-owned JSON.
- `migrations ↔ recovery` is empty by design: migrations are NOT a
  doctor verb (`requirements.md` REQ-036; `design/recovery.md`).
