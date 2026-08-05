---
title: Wire Format
date: 2026-07-29
target_release: 0.8.21
desc: On-disk + IPC formats (if any) for 0.8.21; short OK
blast_radius: architecture.md § 5; design/engine.md; design/migrations.md
status: locked
---

# Wire Format

FathomDB has no standalone IPC wire protocol. The public wire surface is limited
to the on-disk file layout and the schema-version sentinel used on the open
path.

## File layout

The file set is:

- `<db-name>.sqlite`
- `<db-name>.sqlite-wal`
- `<db-name>.sqlite.lock`
- optional `<db-name>.sqlite-journal`

The authoritative layout owner remains `architecture.md` § 5.

## Schema-version sentinel

The canonical schema-version sentinel is SQLite `PRAGMA user_version`. As of
0.8.21 `fathomdb-schema::SCHEMA_VERSION` is **25** (0.8.20 shipped **24**).

Ownership split:

- this file owns the fact that `PRAGMA user_version` is the public on-disk
  sentinel for schema-version compatibility
- `design/migrations.md` owns how successful migrations advance it
- `design/engine.md` owns when it is read on the open path

## Compatibility contract

- opening a supported pre-current database may auto-migrate and advance
  `PRAGMA user_version`
- ⚠ **migration step 23 (TC-33) is NOT data-preserving for edges.** It
  recreates `canonical_edges` with INTEGER `t_valid`/`t_invalid` and type
  CHECKs; per the HITL ruling of 2026-07-21 there is NO data migration —
  existing edge rows do not survive and no stored ISO-8601 value is converted.
  Nodes are unaffected. This is the one on-disk compatibility break in the
  0.8.9 → 0.8.20 span and must be disclosed wherever upgrade is described.
- opening a 0.5.x-shaped database hard-errors before partial read/write
- there is no compatibility reader for 0.5.x layouts

## Non-surface

- no separate IPC frame format
- no secondary version manifest file
- no public promise around internal SQLite page layout beyond what the on-disk
  files and `PRAGMA user_version` already expose
