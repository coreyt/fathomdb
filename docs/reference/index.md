# Reference

Reference for the published 0.8.21 surface and the local, unpublished 0.8.22
candidate. Candidate-only APIs are not available from a registry until the held
release gates complete. Field spellings and type-level details are authoritative
in the locked internal interface specs (`dev/interfaces/{python,typescript,cli}.md`);
this section is the client-facing view.

- [Python API](python-api.md) — `Engine`, `admin.configure`, data
  shapes, instrumentation methods.
- [TypeScript API](typescript-api.md) — Promise-based `Engine`,
  `admin.configure`, data shapes, instrumentation methods.
- [CLI](cli.md) — `fathomdb doctor` + `fathomdb recover` verbs, flag
  spelling, exit-code classes, JSON output shape.
- [Errors](errors.md) — the 27-class error taxonomy, base class,
  trigger, recovery hint codes.
- [Config](config.md) — `EngineConfig` knobs (Python snake_case + TS
  camelCase column).

Rust API reference is auto-published to `docs.rs/fathomdb` once the
crate publishes; until then run `cargo doc --open` or read
[`src/rust/crates/fathomdb/`](https://github.com/coreyt/fathomdb/tree/main/src/rust/crates/fathomdb).

## Known gaps in the published surface

The reference reflects the shipped surface. These are documented gaps:

- Performance gates AC-012, AC-013, AC-019, AC-020 remain open; see
  [compatibility § performance posture](../compatibility/index.md).
- The `SearchFilter.status` field is wired end-to-end but has no
  population source, so a `status=`-filtered query prunes every row.
- Custom Python / TypeScript embedder implementations are not exposed;
  the binding choice is the built-in default embedder or none.
