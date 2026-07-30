# fathomdb-schema

The on-disk schema and versioned migration registry for **FathomDB**, a local-first embedded
retrieval engine built on SQLite.

## Is this the crate you want?

Probably not, unless you are working *on* FathomDB rather than *with* it. Applications should depend
on [`fathomdb`](https://crates.io/crates/fathomdb); the engine pulls this crate in and runs
migrations for you at `Engine::open`.

This crate is published separately because it is a **leaf** — it depends only on `rusqlite` — and
because it is the useful thing to reach for when you need to reason about a FathomDB database file
without opening an engine: inspecting a database's schema version, checking whether a file is at a
version your build understands, or writing tooling over the canonical tables.

## Status: pre-1.0, beta

The 0.8.x line is under active development. The public surface can change between minor releases.
The **on-disk** format is versioned and migrated forward, but there is no downgrade path: a database
migrated by a newer build cannot be opened by an older one.

## What it contains

- `SCHEMA_VERSION` — the schema version this build migrates to, and the value it writes into
  SQLite's `user_version` pragma.
- `MIGRATIONS` — the ordered, append-only migration registry. Each entry is a named step; the
  registry is accretion-checked, so an already-released step cannot be edited in place.
- `migrate`, `migrate_with_steps`, `migrate_with_event_sink` — run pending migrations against a
  `rusqlite::Connection`, returning a `MigrationReport` (per-step names and timings) or a typed
  `MigrationError` / `MigrationFailureReport`.
- `bootstrap_steps` and `CANONICAL_TABLES` — the initial schema and the canonical table set.
- File-suffix constants (`SQLITE_SUFFIX`, `WAL_SUFFIX`, `LOCK_SUFFIX`, `JOURNAL_SUFFIX`) and
  `PRAGMA_USER_VERSION`, which together describe the on-disk footprint of one database.

## Install

```bash
cargo add fathomdb-schema
```

## License

MIT. See the `LICENSE` file shipped in this crate.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
