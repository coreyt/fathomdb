---
title: toml
date: 2026-04-25
target_release: 0.6.0
desc: Audit verdict for toml — drop (HITL JSON-only config)
blast_radius: crates/fathomdb-engine/src/admin/vector.rs (load_vector_regeneration_config); crates/fathomdb-engine/src/admin/mod.rs:3739 (test write); crates/fathomdb-engine/Cargo.toml
status: draft
---

# toml

**Verdict:** drop

## HITL decision (2026-04-25)

JSON-only operator config in 0.6.0+. Decided in HITL Phase 1a. Removes:

- `Some("toml") =>` branch of `load_vector_regeneration_config` at `crates/fathomdb-engine/src/admin/vector.rs:1325-1328`.
- Test write site at `crates/fathomdb-engine/src/admin/mod.rs:3739`.
- `toml.workspace = true` line in `crates/fathomdb-engine/Cargo.toml`.

No known consumer relies on TOML form. Hermes uses YAML, OpenClaw uses JSON;
clients wanting YAML can convert client-side. Removes the dual-format branch
that lets misconfigured extensions silently take the wrong parser path.

## Current usage (pre-drop)
- Crates using it: fathomdb-engine
- Surface used: optional configure files via extension branch
- Version pin: `0.8`; latest 0.8.x

## Maintenance signals (pre-drop)
- Last release: active (toml-rs)
- License: MIT OR Apache-2.0 — compatible: yes

## Migration plan
- Estimated diff: ~10 LoC in `admin/vector.rs`, ~3 LoC in `admin/mod.rs`, ~1 LoC in `Cargo.toml`. No behavior delta — JSON path was already the fallback.
- Risk: low; integration test must cover both `.json` extension and no-extension (now the only accepted forms).
