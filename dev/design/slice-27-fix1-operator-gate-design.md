---
title: Slice 27 fix-1 design memo — feature-gate the operator/recovery seam off the default Rust facade (Option B)
date: 2026-06-06
target_release: 0.8.0
owning_slice: 27 fix-1 (resolves the Slice 27 codex [P1]; tightens AC-074 Rust clause to method-level)
status: accepted
desc: >
  HITL Option B: gate the operator/recovery seam behind a cargo `operator` feature
  the CLI enables, so the DEFAULT fathomdb::Engine runtime surface is recovery-clean
  at the method level; enforce method-absence with cfg-gated compile_fail doctests;
  make AC-050c removal-detect green (the pre-existing Slice-25 residue, via excluding
  tests/ from the public-API scanner). Engine behavior unchanged (gate, not delete).
---

# Slice 27 fix-1 — operator-seam feature gate (Option B)

## 0. The [P1] being resolved

Slice 27's `governed_surface.rs` audited only the 17 re-exported **types**, not the
inherent **methods** on the wholesale-re-exported `fathomdb::Engine`. So
`fathomdb::Engine::rebuild_projections()` / `rebuild_vec0()` (ungated, names contain
**"rebuild"** ∈ the recovery denylist) and `execute_for_test(sql)` (raw SQL) were
reachable on the governed Rust facade → AC-074's recovery-denylist + no-raw-SQL
guarantees were **falsely green** for Rust. Verdict:
`dev/plans/runs/0.8.0-slice-27-review-20260605T215557Z.md`. **HITL chose Option B**:
feature-gate the operator/recovery methods off the **default** facade so the default
runtime surface is recovery-clean, and the CLI opts in.

## 1. The feature: `operator`

- **`fathomdb-engine`**: new `[features] operator = []`. The operator/recovery methods
  + their operator-exclusive private helpers are `#[cfg(feature = "operator")]`. Default
  build: absent. With the feature: present and **byte-for-byte behavior-identical**
  (gate, never delete).
- **`fathomdb` (facade)**: new `[features] operator = ["fathomdb-engine/operator"]`. The
  20 operator-seam **re-exports** are `#[cfg(feature = "operator")]`; the default
  `pub use` set = the **17 governed** types (`dev/interfaces/rust.md` § Governed-surface
  contract, Slice 27 §2a).
- **`fathomdb-cli`**: depends on `fathomdb` with `features = ["operator"]` so `recover`
  / `doctor` (which call the seam) still compile and pass. The CLI is the **only**
  in-repo consumer of the seam (verified — §4).

> **Cargo feature-unification note (resolver = "2").** In a `cargo … --workspace`
> invocation the CLI's `fathomdb/operator` unifies `fathomdb` to operator-ON for that
> whole build. So a method-**absence** assertion can never be a plain `#[test]` (it would
> see the unified-on surface). The absence proofs are therefore
> `#[cfg(not(feature = "operator"))]`-gated compile_fail doctests: real under
> `cargo test -p fathomdb` (operator OFF, CLI not in the graph), inert under `--workspace`
> (operator unified ON → the gated module + its doctests vanish). The positive
> "operator seam resolves WITH the feature" check is `#[cfg(feature = "operator")]`.

## 2. Exact gated set (operator-exclusive — verified callers)

**12 pub methods on `Engine`** (their report types = the Slice-27 §2b operator-seam 20):
`recompute_mean`, `check_integrity`, `safe_export`, `rebuild_projections`,
`rebuild_vec0`, `trace_source_ref`, `excise_source`, `verify_embedder`, `dump_schema`,
`dump_row_counts`, `dump_profile`, `truncate_wal`.

**3 operator-exclusive private methods**: `run_rebuild` (only `rebuild_*`),
`rebuild_shadow_state` (only `run_rebuild`), `excise_source_inner` (only `excise_source`).

**3 operator-exclusive free fns**: `physical_section`, `logical_section`,
`semantic_section` (only `check_integrity`).

**Deliberately NOT gated** (shared with the normal embedder mean-centering path —
`recompute_mean_in_tx` is also called at the projection/commit path, lib.rs:4948):
`recompute_mean_in_tx`, `recompute_mean_in_tx_inner`. Only the *pub* `recompute_mean`
operator entrypoint is gated; its in-tx helpers stay (still live on the default build).

The exact set is **compiler-verified**: after gating, `cargo build -p fathomdb-engine`
(default) + `cargo clippy -- -D warnings` flag any straggler dead-code/unused-import,
which is then gated (or, if it stays live, proves it is shared and is left alone).

**20 operator-seam re-exports** gated in the facade (Slice 27 §2b): `CheckIntegrityOpts,
IntegrityReport, SafeExportArtifact, TraceReport, TraceEvent, RebuildReport, RebuildKind,
ExciseReport, VerifyEmbedderReport, VerifyEmbedderStatus, DumpSchemaReport, SchemaObject,
DumpRowCountsReport, TableRowCount, DumpProfileReport, TruncateWalReport, TruncateWalStatus,
Finding, MeanRecomputeReport, Section`. (The engine `pub struct`s themselves stay
ungated — `pub` = not dead-code — only the facade re-export is gated.)

**Example**: `fathomdb-engine/examples/ingest_corpus.rs` calls `trace_source_ref`, so it
gets an explicit `[[example]]` entry with `required-features = ["operator"]` (skipped on
default builds; compiled when the feature is on). No engine behavior change.

## 3. Method-level enforcement (`fathomdb/src/lib.rs` doctests + `governed_surface.rs`)

- **Recovery-name method absence (default build)** — `#[cfg(not(feature = "operator"))]`
  module carrying `compile_fail` doctests asserting `Engine::rebuild_projections()` /
  `rebuild_vec0()` (the recovery-denylist-named methods) do **not** resolve. RED before
  gating (methods present → body compiles → compile_fail fails); GREEN after.
- **Raw-SQL release-surface absence** — `#[cfg(not(debug_assertions))]` module with a
  `compile_fail` doctest asserting `Engine::execute_for_test(sql)` does not resolve in a
  **release** build (it is `#[cfg(debug_assertions)] #[doc(hidden)]` → already
  release-absent; this pins the already-true shipped-surface invariant, per the review).
  Exercised by `cargo test -p fathomdb --release`.
- `governed_surface.rs` keeps the 17-type allowlist (P1) + denylist-name absence (P3) +
  non-vacuous detector, and **adds** a `#[cfg(feature = "operator")]` positive check that
  the operator seam resolves only with the feature, plus doc pointing at the doctests as
  the method-level pin. Binds **AC-074** (no new AC id).

## 4. No non-CLI / binding consumer calls a gated method (verified)

Grep of `src/rust` (excluding the engine def + tests): every one of the 12 methods is
called **only** by `fathomdb-cli/src/lib.rs` — except `trace_source_ref`, also called by
`fathomdb-engine/examples/ingest_corpus.rs` (handled via `required-features`). **No
reference in `fathomdb-py` / `fathomdb-napi`.** No internal engine self-call to any of the
12. So Option B touches no binding and no non-CLI runtime consumer. (If a re-review finds
otherwise → escalate, do not force.)

## 5. AC-050c removal-detect → green

The scanner (`scripts/security/check_removal_changelog.py`) matches removed
`pub fn|struct|enum|trait|const|type|static|mod` lines in a `v0.6.1..HEAD` diff. Empirics
of this fix:
- Gating a method **adds** a `#[cfg(feature = "operator")]` attribute line; the `pub fn …`
  line is unchanged → **not** a removal. `pub use` re-export changes are not matched by the
  regex at all. So **this fix introduces no new AC-050c removals** (confirmed by running the
  scanner on-branch).
- The only failure is the **pre-existing Slice-25 residue**: two **test functions** in
  `src/python/tests/test_surface.py` (`test_engine_exposes_five_verbs_plus_instrumentation`,
  `test_top_level_exports_match_canonical_set`) removed by Slice 25's rewrite. **Test
  functions are not public API.** The correct fix (§3.5 option b) is to **scope `tests/`
  directories out of the public-API scanner** (a one-line predicate + its `scripts/tests/
  test_removal_detect.sh` update), which permanently stops false positives on test renames.
- A CHANGELOG `### Changed` note records the default-facade operator-seam gating for users
  (honest surface-change disclosure), independent of the scanner.

Goal: `agent-verify` AC-050c removal-detect **rc=0** on-branch.

## 6. Scope / non-goals

No engine **behavior** change (methods move behind a feature, not deleted; identical with
the feature on). No Py/TS SDK or surface-suite change. The byte-frozen
`no_recovery_surface.rs` / Py/TS recovery suites stay byte-unchanged. No new AC id (tighten
AC-074's Rust clause). No release/CI/version action. Justified deviation (a binding calls a
gated method; the operator list differs from §2b; the feature breaks an unexpected consumer)
→ adapt minimally, `[DETECT]`, escalate spec-level.
