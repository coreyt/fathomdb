# Reference

Reference for the public 0.6.0 surface. Field spellings and
type-level details are authoritative in the locked internal interface
specs (`dev/interfaces/{python,typescript,cli}.md`); this section is
the client-facing view.

- [Python API](python-api.md) — `Engine`, `admin.configure`, data
  shapes, instrumentation methods.
- [TypeScript API](typescript-api.md) — Promise-based `Engine`,
  `admin.configure`, data shapes, instrumentation methods.
- [CLI](cli.md) — `fathomdb doctor` + `fathomdb recover` verbs, flag
  spelling, exit-code classes, JSON output shape.
- [Errors](errors.md) — 18-leaf error taxonomy, base class, trigger,
  recovery hint codes.
- [Config](config.md) — `EngineConfig` knobs (Python snake_case + TS
  camelCase column).

Rust API reference is auto-published to `docs.rs/fathomdb` after the
crate publishes; pre-GA see
[`src/rust/crates/fathomdb/`](https://github.com/coreyt/fathomdb/tree/0.6.0-rewrite/src/rust/crates/fathomdb).

## Deferred for 0.6.0

The reference reflects the locked surface but the following are
documented gaps; see [release notes — 0.6.0](../release-notes/0.6.0.md):

- `Engine.open` structured open report dropped by both bindings;
  surfacing defers to 0.6.1.
- Logical-id verbs (`purge_logical_id`, `restore_logical_id`)
  deferred to 0.7.x.
- Performance gates AC-012, AC-013, AC-019, AC-020 deferred; see
  [compatibility § performance posture](../compatibility/index.md).
