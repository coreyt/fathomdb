---
title: Vector Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: vec0 storage, BLOB encoding boundary, and vector recovery semantics
blast_radius: sqlite-vec integration; REQ-011, REQ-025c, REQ-040, REQ-044, REQ-051
status: locked
---

# Vector Design

This file owns vec0 table layout, LE-f32 encoding invariants, stored-profile
metadata, and the rebuild-from-canonical semantics used during recovery.
