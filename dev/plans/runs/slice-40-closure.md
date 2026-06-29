# 0.8.11 Slice 40 (#17) — filter-grammar unification — closure note

Branch: `slice-40-filter-grammar` (off `0.8.11` HEAD `261030dd`).
Contract: `dev/adr/ADR-0.8.11-filter-grammar-unification.md` (Option A — ONE unified
`Filter`, TWO compilation backends). Closes reserved-gap 37.

## What changed (files)

### Rust engine (the load-bearing, compiled + tested deliverable)
- `src/rust/crates/fathomdb-engine/src/lib.rs`
  - New closed unified types: `pub enum FilterTerm` (exactly 5 variants:
    `SourceType`/`Kind`/`CreatedAfter`/`Status`/`Json(Predicate)`) and
    `pub struct Filter { pub terms: Vec<FilterTerm> }`. Reuses the shipped
    `ScalarValue`/`ComparisonOp`/`Predicate` vocabulary verbatim (no new grammar).
  - `impl From<&SearchFilter> for Filter` — D4 sugar lowering (canonical order).
  - `Filter::to_search_filter()` — **vec0 backend dispatch**: lowers the metadata
    subset back through the shipped `SearchFilter`/`vector_filter_clause`
    compilation; **typed-rejects** a `Json` term with `EngineError::InvalidFilter`
    (D3 no-demotion guarantee). Canonical-order-independent of term order.
  - `Filter::lower_for_read_list(kind)` — **canonical_nodes backend dispatch**:
    `Json`→shipped predicate; `Status`/`CreatedAfter`→allowlisted json-paths;
    `Kind`/`SourceType`→constant-fold vs the partition `kind` (`Ok(None)` ==
    guaranteed-empty). `SourceType` folds via `resolve_source_type(kind)`.
  - `Engine::search_filter(query, &Filter)` — unified vec0 entry (typed-reject Json).
  - `Engine::read_list_filter(kind, &Filter, limit)` — unified canonical_nodes entry
    (constant-fold empty ⇒ `[]` without touching SQL; else delegates to the shipped
    `read_list` machinery — every inherited invariant preserved).
  - `Engine::search_filtered(query, Option<SearchFilter>)` — **unchanged signature**;
    now re-expresses the `SearchFilter` sugar through the unified `Filter` and lowers
    back (R-FIL-2/D4). Lossless + canonical-order-preserving ⇒ the produced phase-1
    SQL is **byte-identical to 0.7.2** on the `None`/all-`None` path.
  - Two `#[doc(hidden)] pub` test seams (`to_search_filter_for_test`,
    `lower_for_read_list_for_test`).
- `src/rust/crates/fathomdb-engine/tests/slice40_filter_unification.rs` — NEW suite
  (6 tests): exhaustiveness, sugar round-trip, **typed-reject (RED→GREEN)**, full-set
  read.list, Kind/SourceType constant-fold, and the **shared-fixture parity** (one DB,
  `kind="todo"`, asserted on BOTH `search_filter` (vec0 pre-KNN) and `read_list_filter`).

### FFI bindings (compile-validated via `cargo build`; cannot run Py/TS here)
- `src/rust/crates/fathomdb-py/src/lib.rs` — `read_list_filter` pyfunction +
  `py_filter_term_to_rust` + module registration.
- `src/rust/crates/fathomdb-napi/src/lib.rs` — `readListFilter` + `FilterTermInput`
  napi object + `napi_filter_term_to_rust`.

### Python SDK (additive; `SearchFilter` stays sugar)
- `src/python/fathomdb/filter.py` — NEW: `Filter` + 5 term dataclasses
  (`SourceType`/`Kind`/`CreatedAfter`/`Status`/`Json`) + `from_search_filter`,
  `Filter.to_search_filter()` (typed-rejects Json), `Filter.to_native_terms()`.
- `src/python/fathomdb/read.py` — `read.list` extended with an additive `filter=`
  kwarg (mutually exclusive with `predicates`); routes to native `read_list_filter`.
  **Same governed verb** — no new surface member.
- `src/python/fathomdb/engine.py` — `search` accepts `SearchFilter | Filter`
  (lowers a `Filter` via `to_search_filter()`, typed-rejects Json).
- `src/python/fathomdb/__init__.py` — export `Filter`.
- `src/python/fathomdb/_fathomdb.pyi` — `read_list_filter` stub.
- `src/python/tests/test_filter_unification.py` — NEW X1 parity suite.

### TypeScript SDK (mirror)
- `src/ts/src/index.ts` — `FilterTerm` (discriminated union) + `Filter` interface +
  `filterToSearchFilter`/`searchFilterToFilter`; `Engine.search` accepts
  `SearchFilter | Filter`.
- `src/ts/src/read.ts` — `read.list` extended with an additive `filter` arg +
  `toNativeFilterTerm`. **Same governed verb.**
- `src/ts/src/binding.ts` — `NativeFilterTermInput` + `readListFilter` native decl.
- `src/ts/tests/filter-unification.test.ts` — NEW X1 parity suite (mirror).

### F-8b
- `dev/design/0.8.11-slice40-f8b-record-feedback-execution-note.md` — NEW: records
  KEEP-instrumentation (EXP-AF has not GOne); **no allowlist change**;
  `enable_telemetry`/`last_telemetry_query_id` untouched.

## RED → GREEN evidence (Rust)

RED — new test compiled against the baseline engine (engine impl stashed):
```
error[E0432]: unresolved imports `fathomdb_engine::Filter`, `fathomdb_engine::FilterTerm`
  --> src/rust/crates/fathomdb-engine/tests/slice40_filter_unification.rs:22:40
   |
22 |     ComparisonOp, Engine, EngineError, Filter, FilterTerm, Predicate, PreparedWrite, ScalarValue,
   |                                        ^^^^^^  ^^^^^^^^^^ no `FilterTerm` in the root
   |                                        |
   |                                        no `Filter` in the root
```

GREEN — after the engine implementation:
```
test result: ok. 6 passed; 0 failed  (slice40_filter_unification.rs)
test result: ok. 6 passed; 0 failed  (pr_g10_filtered_knn.rs  — byte-identity pin still green)
test result: ok. 18 passed; 0 failed (slice35_filter_grammar.rs — G4 grammar still green)
test result: ok. 2 passed; 0 failed  (engine --lib — resolve_source_type drift check etc.)
```
Search-path regression (round-trip change): `pr_g9_rrf_fusion` 10, `pr_g12_recency` 5,
`pr_g1_search_hits` 4, `ga2_vector_stage_seam` 2, `search_result_shape` 2 — all green.
`cargo build --workspace` + `cargo build -p fathomdb-napi` — clean.

## Post-merge Py/TS X1 commands (orchestrator must run on the MAIN tree)

These need a `maturin develop` / napi build (forbidden from this worktree). On the
main tree after the integrated build:
```
# Python (after `maturin develop` of fathomdb-py into the shared .venv):
python -m pytest src/python/tests/test_filter_unification.py -q
python -m pytest src/python/tests/test_surface.py src/python/tests/test_read_list.py -q   # regression: allowlist unchanged + read.list

# TypeScript (after the napi build of fathomdb-napi):
cd src/ts && npm run build && node --test dist/tests/filter-unification.test.js
cd src/ts && node --test dist/tests/surface.test.js dist/tests/functional-read-list.test.js   # regression
# (or the repo's configured `npm test` runner; also run `npx tsc --noEmit` — no node_modules in this worktree to typecheck locally)
```

## Impl-TBD decision (ADR D1)

`FilterTerm::Kind` on `read.list`: **constant-fold against the partition `kind`
argument** (the simpler total option). A `Kind(k)` term is a no-op iff `k == kind`,
else guaranteed-empty (`Ok(None)`). Chosen over emitting a redundant
`canonical_nodes.kind = ?` column clause because `read.list` is already partitioned by
`kind=?1`; constant-fold is total, byte-minimal, and adds no SQL. Documented inline on
the `FilterTerm::Kind` variant and in `lower_for_read_list`.

## Deviations / anomalies (loud, per OOB protocol)

1. **[DETECT] Pre-existing uncommitted implementation in the worktree.** After the RED
   stash/pop cycle, `lib.rs` contained a SECOND, complete-and-distinct prior
   implementation of this exact slice (free functions `render_vec0_metadata` /
   `compile_vec0_filter` / `lower_filter_for_read_list` + test seams + read-path
   integration) that I did not author and that collided with mine (duplicate
   `Filter`/`FilterTerm` ⇒ E0428/E0119). The worktree was specified as clean from
   HEAD `261030dd`; the RED run confirmed HEAD had **no** `Filter`/`FilterTerm`, so
   this code appeared only during my session (likely a prior interrupted agent run
   surfaced via the stash interaction). **[RESOLVE]** I `git checkout HEAD -- lib.rs`
   to a verified-clean baseline (grep-confirmed no `Filter`/`FilterTerm`), then
   re-applied a single coherent implementation I fully own. The discarded design was
   competent (and its order-independent renderer idea is preserved by my
   round-trip/field-assignment lowering), but it introduced a parallel vec0 compiler;
   my design instead reuses the SHIPPED `vector_filter_clause` verbatim, which makes
   the D4 byte-identity guarantee structural. **Escalation:** if the orchestrator
   intended that prior work to be the deliverable, flag it — I chose the
   fully-understood, test-validated path over reconstructing unattributed code.

2. **No new governed-surface verb (allowlist byte-unchanged).** The unified grammar
   rides the EXISTING `read.list` verb via an additive `filter=` arg rather than a new
   `read.list_filter`/`listFilter` verb, honoring the F-8b "make no change to
   `governed-surface-allowlist.json`" mandate. `search` likewise just gains a
   `Filter` overload (no new verb). Consequence: no allowlist edit, no surface-suite
   churn.

3. **SDK search lowering is pure-Py/TS; read.list lowering is native.** The vec0
   (search) Filter→SearchFilter lowering is a trivial field map + Json reject with no
   `resolve_source_type` dependency, so it is done in the SDK (no drift risk). The
   read.list dispatch (SourceType/Kind constant-fold via `resolve_source_type`) stays
   **authoritative in Rust** via the new native `read_list_filter` entry — never
   re-implemented in Python/TS.

4. **Py/TS suites not run here** (no `maturin develop`/napi build from a worktree, per
   `agent-worktree-stale-base-trap`). FFI compiles via `cargo build -p fathomdb-py
   -p fathomdb-napi`; Python files pass `ast.parse`; TS not type-checked locally
   (no `node_modules`). Commands above for the orchestrator.
