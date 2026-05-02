---
title: Wire Format
date: 2026-04-24
target_release: 0.6.0
desc: On-disk + IPC formats (if any) for 0.6.0; short OK
blast_radius: architecture.md § 5; design/engine.md; design/migrations.md
status: draft
---

# Wire Format

0.6.0 has no standalone IPC wire protocol. The public wire surface is limited
to the on-disk file layout and the schema-version sentinel used on the open
path.

## File layout

The 0.6.0 file set is:

- `<db-name>.sqlite`
- `<db-name>.sqlite-wal`
- `<db-name>.sqlite.lock`
- optional `<db-name>.sqlite-journal`

The authoritative layout owner remains `architecture.md` § 5.

## Schema-version sentinel

The canonical schema-version sentinel is SQLite `PRAGMA user_version`.

Ownership split:

- this file owns the fact that `PRAGMA user_version` is the public on-disk
  sentinel for schema-version compatibility
- `design/migrations.md` owns how successful migrations advance it
- `design/engine.md` owns when it is read on the open path

## Compatibility contract

- opening a supported pre-current database may auto-migrate and advance
  `PRAGMA user_version`
- opening a 0.5.x-shaped database hard-errors before partial read/write
- 0.6.0 does not expose a compatibility reader for 0.5.x layouts

## Non-surface

- no separate IPC frame format
- no secondary version manifest file
- no public promise around internal SQLite page layout beyond what the on-disk
  files and `PRAGMA user_version` already expose
