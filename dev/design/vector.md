---
title: Vector Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: vec0 storage, BLOB encoding boundary, and vector recovery semantics
blast_radius: sqlite-vec integration; REQ-011, REQ-025c, REQ-040, REQ-044, REQ-051
status: locked
---

# Vector Design

> **Requirement traceability (Steward, 2026-07-28; corrected after independent audit).** Nominally the vec0
> storage doc for the **`R-20-VC` / sqlite-vec `#99`** probe (0.8.20 Slice 22). ⚠ **In practice this file is
> a stub and carries NO material on `#99`** — no vec0 column list, no `kind`/`attr_*` columns, no DELETE
> semantics, no `excise_source`/`purge`. The real sources are `dev/design/0.8.20-slice0-erasure-design.md`
> (the `purge_inner` / `excise_source_inner` DELETE-site matrix against `vector_default`) and
> `dev/adr/ADR-0.8.11-filter-grammar-unification.md` (the canonical vec0 TEXT metadata-column table). The
> `attr_<hex>` columns were minted in 0.8.20 Slice 15e and were never designed in a doc.

This file owns vec0 table layout, LE-f32 encoding invariants, stored-profile
metadata, and the rebuild-from-canonical semantics used during recovery.
