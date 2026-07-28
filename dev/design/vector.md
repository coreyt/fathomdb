---
title: Vector Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: vec0 storage, BLOB encoding boundary, and vector recovery semantics
blast_radius: sqlite-vec integration; REQ-011, REQ-025c, REQ-040, REQ-044, REQ-051
status: locked
---

# Vector Design

> **Requirement traceability (added 2026-07-28, Steward — bookkeeping, no design change):** `vec0`
> storage design of record for the **`R-20-VC` / sqlite-vec `#99`** probe (0.8.20 Slice 22) — whether
> 0.1.7 reports a spurious `DELETE` error for a `kind`/`attr_*` value over 12 chars. Back-linked per TC-92.

This file owns vec0 table layout, LE-f32 encoding invariants, stored-profile
metadata, and the rebuild-from-canonical semantics used during recovery.
