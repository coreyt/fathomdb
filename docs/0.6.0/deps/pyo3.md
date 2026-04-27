---
title: pyo3
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for pyo3
blast_radius: Python SDK boundary (fathomdb crate, `python` feature)
status: draft
---

# pyo3

**Verdict:** keep

## Current usage
- Crates using it: fathomdb (feature `python`)
- Surface used: `#[pyclass]`, `#[pymethods]`, `Bound<'_, PyAny>`, `PyResult`, `extension-module` feature
- Version pin: `0.28` features=`extension-module`; latest 0.28.x

## Maintenance signals
- Last release: active (PyO3 org)
- Open issues / open CVEs: none on 0.28
- Maintainer count: PyO3 org multi; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.74; matches: yes

## Cross-platform
- All four target triples; abi3 not currently used.
- C-boundary footguns: PyO3 internally uses `c_char` correctly. Our wrappers do not cast fn pointers manually.

## Alternatives considered (≥1)
- `rust-cpython`: deprecated. No.
- ctypes-only Python bindings over a C ABI: pros — no PyO3 churn; cons — lose GC integration, manual refcounting, much larger SDK surface to write. Migration cost: ~3k LoC + new ABI layer. Not viable.

## Verdict rationale
Only viable Python binding generator for a Rust extension module. Keep, plan abi3 migration as a 0.7 followup.

## What would force replacement in 0.7.0?
PyO3 0.x→1.0 churn breaking our surface AND abi3 required by distribution constraints.
