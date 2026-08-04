# Horizontal review — cross-file reconciliation

The vertical review looked at each violator alone. This phase asked whether those
proposals still hold **together**, and against the rest of the repo.

## Axis verdicts

| axis | verdict |
|:--|:--|
| empirically verify the load-bearing claim under the engine decomposition (inherent-impl-in-any-module + privat | **PROPOSALS-HOLD** |
| cross-language surface parity | **PROPOSALS-NEED-ADJUSTMENT** |
| test-file and CI-gate coupling | **PROPOSALS-HOLD** |
| does the engine decomposition contradict a shipped design document? | **PROPOSALS-HOLD** |

**0 blocking, 2 important, 2 minor** adjustments.

## Adjustments to the vertical proposals

### Important

**engine-decomposition.md §2 module tree table**

- *Change:* Confirm the inline nested modules `reader_search_hook` (line 1098) and `mean_centering_internals_for_test` (line 9327) are in scope for the `reader_pool.rs` and `vector/mean.rs` moves respectively (the table already lists `reader_search_hook` under reader_pool.rs; `mean_centering_internals_for_test` is not explicitly named anywhere in the table and should be added to `vector/mean.rs`'s owned-items list since it wraps `MeanAccumulator`).
- *Why:* Found by direct inspection that these two inline `mod { }` blocks exist inside lib.rs today (not just free functions) and would need to physically relocate with their subject types; `mean_centering_internals_for_test` in particular is currently unlisted in the module-tree table's `vector/mean.rs` row and could be silently missed during the mechanical move.

**dev/plans/refactor-background-check/refactor-proposals.md — fathomdb-py/src/lib.rs SPLIT (lines 52-67)**

- *Change:* Add a follow-up edit, landed in the same change as the split, to src/python/fathomdb/`_fathomdb.pyi` line 3: replace the literal citation 'Mirrors the surface emitted by `src/rust/crates/fathomdb-py/src/lib.rs`' with a citation to the crate/module set (e.g. 'the fathomdb-py crate (src/rust/crates/fathomdb-py/src/{errors,types,engine,verbs,translate,graph,embed}.rs)') so the hand-maintained stub's 'keep in sync with' pointer does not send a future maintainer to a near-empty lib.rs.
- *Why:* No automated gate checks this citation (confirmed: no test ties `_fathomdb.pyi` to lib.rs), so nothing will fail red, but the vertical review's LEAVE verdict for the .pyi only evaluated its own content in isolation and could not see the sibling SPLIT proposal that empties out the exact file the stub's header names as its source of truth.

### Minor

**engine-decomposition.md §0 / §7.1**

- *Change:* The core claim is verbatim TRUE and empirically confirmed both directions (works when Engine stays at crate root; breaks with the exact E0616 the plan predicts when Engine moves into a child module while capability modules stay siblings). No adjustment needed to the claim itself — flag this finding as CONFIRMED, not merely plausible, in whatever downstream record cites this axis.
- *Why:* Compiled minimal reproductions of both the positive and negative case; the negative case reproduces the plan's predicted failure with the exact error code (E0616 'field is private') the mechanism would produce, and the pub(crate) mitigation is confirmed to be exactly sufficient and exactly what the plan says the fallback degenerates into.

**engine-decomposition.md §5 mechanical rules / mod tests handling**

- *Change:* Add one explicit line to the Step-1/Step-2 mechanical rules: when `guard_row_owned_registry` (or any other named-import test) moves out of the crate-root `mod tests` alongside its subject item, the `use super::{...}` import list must be rewritten (crate-root re-exports still make every name resolvable via `use crate::{...}`, so this is find-and-replace, not a design question) — call this out so an implementer doesn't treat the compile error as evidence the mechanism failed.
- *Why:* Verified the test block currently relies on `use super::{ explicit names }` rather than `use super::*`; this is exactly the §5 'move tests with the item' rule already stated, but the plan doesn't spell out that `super` stops meaning crate-root once the test moves — worth one sentence so nobody misreads the resulting compiler error as a refutation of §0.

## Evidence per axis

### empirically verify the load-bearing claim under the engine decomposition (inherent-impl-in-any-module + privat

1) Built /tmp/.../scratchpad/visibility-probe (rustc/cargo 1.95.0, edition 2021, matching the real workspace's `edition = "2021"` in Cargo.toml). lib.rs defines `pub struct TheStruct { field_a: i32, field_b: String }` (both fields fully private, no pub/pub(crate)) at crate root plus `mod capability_a; mod capability_b;`. capability_a.rs: `impl TheStruct { pub fn a(&self) -> i32 { self.field_a * 2 } }`. capability_b.rs: `impl TheStruct { pub fn b(&self) -> String { let doubled = self.a(); format!("{}-{}", self.field_b, doubled) } }` (touches the OTHER private field AND calls a method defined in the sibling module). `cargo build` → exit 0, 'Finished dev profile' (only unused-import warnings for the `pub use` re-exports, unrelated to the claim). CONFIRMS: private fields ARE visible to sibling child modules of the defining crate, inherent impl blocks for a type may live in any module of the defining crate, and cross-module Engine-method calls work with zero facade/zero visibility change.

2) Built /tmp/.../scratchpad/visibility-probe-fail to test the plan's stated failure mode: struct moved INTO a child module `mod engine { pub struct TheStruct { field_a: i32, field_b: String } ... }` (private fields), re-exported via `pub use engine::TheStruct;`, while capability_a.rs/capability_b.rs stay SIBLINGS of `engine` (not descendants of it, both live directly under crate root next to the `mod engine` declaration). `cargo build` → exit 101: `error[E0616]: field \`field_a\` of struct \`TheStruct\` is private` (capability_a.rs:5) and the same for `field_b` (capability_b.rs:6). CONFIRMS the plan's §7.1 warning verbatim: moving the struct off the crate root into a module that the capability files are NOT descendants of breaks private-field access exactly as claimed.

3) Verified the plan's named mitigation is exactly what it claims and nothing milder works: changing only the two field declarations to `pub(crate) field_a: i32` / `pub(crate) field_b: String` (leaving struct location and everything else unchanged) made the identical code compile clean (exit 0). This is precisely the 'pub(crate)-field rewrite' the plan says the whole effort degenerates into if `struct Engine` ever leaves lib.rs — confirmed as the actual, minimal, sufficient fix, not a strawman.

4) Checked the real crate at src/rust/crates/fathomdb-engine/src/lib.rs (17,910 lines, confirmed via wc -l) for the five named obstruction classes: (a) macro_rules! — `grep -rn 'macro_rules!' src/` across the WHOLE crate returns zero hits, so no textual-order-dependent declarative macros exist anywhere in scope; (b) #[pymethods]/#[napi]/#[pyclass]/#[pymodule] proc-macro attrs — zero hits anywhere in the crate (those live only in the separate fathomdb-py/fathomdb-napi crates, not touched by this plan); (c) `struct Engine` — confirmed at line 274, ALL fields are plain-private (no `pub`/`pub(crate)` prefix on any of the ~20 fields enumerated, e.g. `path: PathBuf`, `connection: Mutex<Option<Connection>>`, `reader_pool: ReaderWorkerPool`, etc.), i.e. it is in exactly the state the plan's core claim requires and exactly the state probe 1 modeled; (d) `impl Engine` — confirmed there is exactly ONE top-level `impl Engine { ... }` block (line 4110), matching the plan's '5008-line block' description (the plan proposes splitting this ONE block into 16 files each with its own `impl Engine {}`, it is not claiming 16 already exist); (e) existing child modules — `pub mod lifecycle;` and `mod pcache2;` (both file-based) plus two INLINE modules nested directly in lib.rs (`mod reader_search_hook { ... }` at line 1098, self-contained static/atomic state with no Engine-field access, and `pub mod mean_centering_internals_for_test { ... }` at line 9327, which wraps `MeanAccumulator` via its public methods, not private-field poking) plus `mod tests` at line 17621 (290 lines). None of these names collide with any of the ~20 planned module names (error/id/temporal/existence/filter/reader_pool/search/read/write/projection/vector/extract/erasure/open/admin/bm25f/telemetry/sqlite_ffi/test_hooks). Inspected `mod tests` (17621-17910): the one substantive test (`guard_row_owned_registry`) uses `use super::{ Engine, IdSpace, IdSpaceKind, PreparedWrite, derive_stable_id, resolve_source_type, KIND_TO_SOURCE_TYPE_CASE_SQL, ROW_OWNED_PROJECTIONS };` — explicit named imports, NOT `use super::*` — and a targeted grep for direct private-field dotted access (`.connection`, `.reader_pool`, `.projection_runtime`, `.counters`, `.subscribers`, `.next_cursor`, `.telemetry`) inside the tests block returned zero hits: the test only goes through the public `Engine::open`/`.close()` API and `rusqlite::Connection::open` directly on the sqlite file, never through Engine's private fields. This matches the plan's own §5 rule ('move each item's tests out of mod tests with the item') — moving these named items to their new homes requires only updating the `use super::{...}` path prefix (since `super` will no longer be crate-root once `guard_row_owned_registry` physically relocates with `projection/registry.rs`), which is a mechanical path-qualification fix already anticipated by the plan's Step-1 sequencing, not a compile obstruction.

### cross-language surface parity

Ran both gate scripts on the current tree and read them in full: scripts/check-governed-surface-pin.sh (exit 0) and scripts/release/verify-embedder-api-no-drift.sh (read in full, not executed — it makes a network call to crates.io and is scoped to a different crate, see below). Read src/conformance/governed-surface-allowlist.json and scripts/governed-surface-pin.json in full. Read src/python/fathomdb/`_fathomdb.pyi` header (lines 1-6). Read the first ~60 lines of both fathomdb-py/src/lib.rs and fathomdb-napi/src/lib.rs (the FFI-contract header comments) plus grepped both files for rerank/embed_batch/cls_embedder symbols and crate Cargo.toml feature flags. Read the SPLIT proposal tables for both crates in dev/plans/refactor-background-check/refactor-proposals.md (lines 52-81). Grepped for any pyi-vs-lib.rs conformance test (none found) and read the vertical-review's LEAVE rationale for `_fathomdb.pyi` (line 168 of refactor-proposals.md / vertical-review.json:674).

1. check-governed-surface-pin.sh pins only a JSON data file (src/conformance/governed-surface-allowlist.json) by content hash (sha256 + git blob sha1) plus three flat lists of dotted/underscore STRING VERB NAMES ('Engine.open', 'write', 'read.get', 'purge', ...). It contains zero file paths and zero module paths. The actual enforcement of 'this name is really exposed' happens at runtime via src/python/tests/test_surface.py and src/ts/tests/surface.test.ts, which introspect the LIVE compiled module/class surface, not source layout. Ran it on the current tree: exit 0, unaffected by anything under fathomdb-py or fathomdb-napi src trees. Splitting fathomdb-py/src/lib.rs into errors.rs/types.rs/engine.rs/verbs.rs/translate.rs/graph.rs/embed.rs (per the proposal's own table) cannot trip this gate as long as the #[pymodule] init function — which the proposal explicitly keeps in lib.rs (L2460-2548) — still calls the same m.add_class/add_function/add exception registrations under the same names. File-internal `mod` reorganization is invisible to this gate by construction.

2. verify-embedder-api-no-drift.sh is hardcoded to CRATE="fathomdb-embedder-api" and CRATE_DIR="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api" — a completely different crate (the Axis-E embedder-trait crate from the v0.8.9 partial-publish incident). It diffs that one crate's normalized *.rs surface against the crates.io-published tarball at the locally-declared version. It has no code path that ever reads fathomdb-py or fathomdb-napi. This script is NOT a constraint on either SPLIT proposal at all; the premise that it needs reconciling against the FFI-binding splits does not hold.

3. `_fathomdb.pyi` line 3 reads literally: 'Mirrors the surface emitted by `src/rust/crates/fathomdb-py/src/lib.rs`.' No automated test ties the stub to that path (grepped all test dirs; only test_surface.py touches 'surface' and it checks the governed-allowlist subset, not stub↔lib.rs sync) so nothing will fail CI when the split lands — but the citation itself goes stale/misleading: once the #[pyclass]/create_exception!/#[pyfunction] items the docstring tells a maintainer to consult actually live in errors.rs/types.rs/engine.rs/verbs.rs/translate.rs/graph.rs/embed.rs, and lib.rs is reduced to `mod` wiring + the pymodule-init fn + the moved-out `mod tests` block, a reader following that one-line pointer lands in a near-empty file. The vertical review's LEAVE verdict for the pyi (536 LOC content, 'inherently verbose, mirrors the entire Rust binding public surface') is still correct on the CONTENT question — nothing in the stub's actual type/method declarations needs to move — but the SPLIT proposal for fathomdb-py/src/lib.rs is incomplete: it doesn't carry the one-line follow-up to repoint or generalize that citation (e.g. to the fathomdb-py crate/module set rather than the single file lib.rs). This is exactly the kind of desync the task asked me to check for, and it survives the vertical review because a single-file reviewer of the .pyi has no visibility into the sibling SPLIT proposal.

4. Verified by reading both crate headers directly, not by trusting the proposal's claim. fathomdb-py/src/lib.rs: 'FFI safety contract (mirrored by Phase 11b napi-rs): 1. GIL release via py.detach... 2. typed errors via engine_error_to_py... 3. every string checked by validate_ffi_string... 4. panics surface as PanicException via catch_unwind...'. fathomdb-napi/src/lib.rs: 'FFI safety contract (mirrors the PyO3 binding in `fathomdb-py`): 1. spawn_blocking so libuv thread isn't tied up... 2. typed errors via engine_error_to_napi... 3. validate_ffi_string_napi... 4. catch_unwind rethrown as FathomDbPanicError...'. Both numbered lists are exclusively about FFI SEMANTICS (blocking/threading model, typed-error mapping, string validation, panic translation) — neither header contains any statement about file layout or module structure. The py reviewer's claim is CONFIRMED: the documented 'mirror' contract is semantic-only, so the two independent SPLIT proposals having different module counts/boundaries breaches nothing documented.

5. Direct table comparison: both proposals name errors.rs / types.rs / engine.rs / translate.rs identically, and — checked by reading each row's description — each of those four names covers the same responsibility in both crates (exception/error-code taxonomy + FFI string validation + error-to-language conversion; DTO wrapper structs; the core Engine method impl; write-batch dict/JSON→PreparedWrite translation). Where they diverge (py additionally has verbs.rs, graph.rs, embed.rs = 7 modules; napi folds the equivalent read/graph-traversal surface into one read.rs and has no embed.rs = 5 modules) is not a naming inconsistency but a real difference in what code exists in each crate: grepped fathomdb-napi/src/lib.rs for rerank/embed_batch/cls_embedder_singleton and found none — the CLS-embedder singleton (embed_batch_cls, gated by the `default-embedder` Cargo feature declared in fathomdb-py/Cargo.toml) and the standalone module-level `rerank` pyfunction exist only in the Python binding; napi's Engine.embed (L1679) falls inside its own engine.rs range. So the four shared names are used consistently for the same concerns, and the extra py modules reflect extra Python-only surface, not a divergent naming scheme.

### test-file and CI-gate coupling

Verified CI gate infrastructure by:

1. grep across .github/workflows/ and scripts/ for test file name patterns
2. Examined perf_gates.rs for env-var gating (AGENT_LONG checks on 6 tests, 4 #[ignore] tests)
3. Confirmed tests/support/ pattern via #[path] attribute inspection across 20+ existing test files
4. Confirmed no CI references to slice15d-projection-registry or its derivatives
5. Identified that perf_gates --test flag is explicitly selected in perf-canonical.yml and 4 run-ac*.sh scripts, but proposals don't modify perf_gates.rs

File locations checked:

- /home/coreyt/projects/fathomdb-worktrees/refactor-background-check/.github/workflows/ci.yml (217 jobs/40 LOC reviewed)
- /home/coreyt/projects/fathomdb-worktrees/refactor-background-check/.github/workflows/perf-canonical.yml (perf_gates refs confirmed)
- /home/coreyt/projects/fathomdb-worktrees/refactor-background-check/scripts/agent-test.sh (TC-20 invariant about eu7_real_corpus_ac found, perf_gates not mentioned)
- /home/coreyt/projects/fathomdb-worktrees/refactor-background-check/src/rust/crates/fathomdb-engine/Cargo.toml (verified eu7_real_corpus_ac feature gating)
- /home/coreyt/projects/fathomdb-worktrees/refactor-background-check/src/rust/crates/fathomdb-engine/tests/perf_gates.rs (verified AGENT_LONG gating on lines 538, 610, 692, 999, 1107)
- /home/coreyt/projects/fathomdb-worktrees/refactor-background-check/src/ts/tests/slice15d-projection-registry.test.ts (confirmed no CI references)

### does the engine decomposition contradict a shipped design document?

Conducted systematic review of engine decomposition plan against design documents:

SCOPE CHECKED:

- engine-decomposition.md: ~20-module tree proposal with 16 impl Engine blocks across capability modules
- dev/architecture.md (locked 2026-04-29): authoritative crate topology + 14 architectural subsystems
- dev/design/*.md: subsystem design contracts (vector.md, projections.md, lifecycle.md, retrieval.md, etc.)
- dev/design/record-lifecycle-protocol/*.md: OPP-12 record lifecycle protocol (status: RATIFIED)

KEY FINDINGS:

1. DESIGN DOCS DO NOT REFERENCE INTERNAL MODULE PATHS

- Searched all design docs for module paths like search/exec.rs, projection/registry.rs, vector/partition.rs
- Result: Zero matches
- All code references use line numbers in lib.rs (e.g., L2381–2803 for filter types)
- Design docs describe subsystem responsibilities and contracts, not implementation file layout
- Examples: design/projections.md discusses "projection" subsystem semantics (FTS/vector state), not file structure; design/lifecycle.md discusses observability phases (Started/Slow/Heartbeat/Finished/Failed), not module paths

1. ARCHITECTURE.MD DESCRIBES ARCHITECTURAL SUBSYSTEMS, NOT FILE LAYOUT

- Quote: "Each module = one design/<name>.md file" — refers to 14 architectural subsystems (runtime, writer, reader, etc.), not implementation files
- Explicitly states: "No internal-types public surface. Module boundaries inside fathomdb-engine are not semver-stable"
- Architectural subsystems remain unchanged; implementation module structure is an internal detail not constrained by architecture.md

1. NAMING DISAMBIGUATION ALIGNS WITH DESIGN DOCS, NOT CONTRADICTS

- Decomposition resolves four unrelated "projection" things by creating projection/{runtime.rs, commit.rs, registry.rs, maintenance.rs}
- Decomposition resolves two unrelated "lifecycle" things: keep lifecycle.rs (observability), create existence.rs (record state machine)
- Design docs already use "existence" terminology: dev/design/record-lifecycle-protocol/structural-lifecycle-contract.md consistently uses "existence axis" (pending/active/deleted/purged)
- This naming choice IMPROVES alignment with how design docs already refer to these concepts

1. ONE PRE-EXISTING STALE REFERENCE (UNRELATED TO DECOMPOSITION)

- Location: dev/architecture.md line 52
- References: src/rust/crates/fathomdb-engine/src/embedder/mod.rs:1-7
- Status: FILE DOES NOT EXIST — no embedder/mod.rs in current code; embedder code scattered throughout lib.rs
- Cause: Pre-existing documentation issue, not introduced by decomposition
- Decomposition: Does NOT propose extracting or moving embedder module; this reference remains stale regardless

CONCLUSION:
No design document contradicts the decomposition plan. Design documents operate at architectural subsystem level (what each subsystem does, not how it's organized into files). The decomposition's ~20 implementation-detail modules do not violate any subsystem contract or naming assumption. The naming fixes for "projection" and "lifecycle" actually improve clarity and align with existing design doc terminology.

Documentation maintenance work required post-implementation (line-number citation updates), but this is routine, not blocking.

## Reconciled position

### Cross-Cutting Reconciliation — Final Position

**Headline: the vertical work held.** Four cross-cutting checks were run against the 6 actionable findings and the ~20-module engine plan. Three axes returned PROPOSALS-HOLD. One returned an adjustment, and it is a documentation-citation follow-up, not a structural defect. No proposal is withdrawn. The engine plan's single load-bearing assumption was moved from *asserted* to *empirically confirmed in both directions*.

Two additional items surfaced during this reconciliation pass (beyond the four axis reports) and are folded in below: the scope of stale `lib.rs` citations is wider than the .pyi, and the plan's per-PR verification gate has no implementing tooling in the repo.

---

#### 1. Proposals that survive unchanged

| Proposal | Verdict |
|:--|:--|
| `scripts/commission-manifest.sh` — RESTRUCTURE-IN-PLACE (extract 674-line Python heredoc to `scripts/commission-manifest.py`) | Unchanged. No CI gate, cross-language mirror, or governed-surface coupling. |
| `tests/corpus/scripts/generate_chain_corpus.py` — RESTRUCTURE-IN-PLACE (chain-builder factory) | Unchanged. `CHAIN_BUILDERS` list preserved; no CI reference to chain shapes. |
| `src/ts/tests/slice15d-projection-registry.test.ts` — RESTRUCTURE-IN-PLACE (`withEngine()` fixture) | Unchanged. Confirmed zero CI references to this test file or its derivatives; no `--test`-style selection pins it. |
| `src/rust/crates/fathomdb-cli/src/lib.rs` — RESTRUCTURE-IN-PLACE (`report_json.rs` sibling module) | Unchanged. Private fns, same-crate move, CLI surface identical. |
| `src/rust/crates/fathomdb-napi/src/lib.rs` — SPLIT into 5 modules | Unchanged. No `.d.ts` hand-maintained mirror exists in this crate; the generated TS surface is codegen-derived, not layout-derived. |
| The 59 LEAVE verdicts | Unchanged in substance. One (`_fathomdb.pyi`) acquires a follow-up edit — see §2 — but its LEAVE verdict on *content* stands. |
| Engine decomposition §0 core mechanism (Engine at crate root + 16 `impl Engine` blocks in child modules) | Unchanged, and upgraded from plausible to **CONFIRMED**. |
| Engine decomposition §2 module tree, §3 conflict resolutions, §4 impl-block clustering, §6 stay-together list, §5 sequencing | Unchanged, with two small additions to §2 and §5 (see §2 below). |
| The two cross-file observations (rust-test `support/query_synthesis.rs` extraction; CI composite action) | Unchanged. |

**Design-document check:** zero contradictions found. Design docs and `dev/architecture.md` describe architectural subsystems and contracts, never internal module paths — a targeted search for the plan's proposed paths (`search/exec.rs`, `projection/registry.rs`, `vector/partition.rs`, …) across all design docs returned no matches. `architecture.md` states explicitly that "module boundaries inside `fathomdb-engine` are not semver-stable." The plan's `existence.rs` naming choice actively *improves* alignment: `dev/design/record-lifecycle-protocol/structural-lifecycle-contract.md` already uses "existence axis" for exactly that state machine. One pre-existing stale citation was found (`dev/architecture.md:52` → `fathomdb-engine/src/embedder/mod.rs`, a file that does not exist) — unrelated to the decomposition, which neither creates nor moves an embedder module.

**Test/CI-gate check:** no coupling. `perf_gates` is explicitly selected by name in `perf-canonical.yml` and four `run-ac*.sh` scripts, and `agent-test.sh` carries a TC-20 invariant naming `eu7_real_corpus_ac` — but no proposal touches either file. `tests/support/` `#[path]` wiring is confirmed as the existing pattern for the proposed shared support module.

---

#### 2. Proposals that are ADJUSTED

##### 2.1 `fathomdb-py/src/lib.rs` SPLIT — must carry a citation-repoint edit *[important]*

- **Original:** split `lib.rs` into `errors/types/engine/verbs/translate/graph/embed.rs`; blast radius stated as "internal crate-private reorg only, no Python-facing API/ABI change."
- **Adjustment:** the split must land, **in the same change**, with a repoint of the citations that name `fathomdb-py/src/lib.rs` as the source of truth for the moved items:
  - `src/python/fathomdb/_fathomdb.pyi:3` — "Mirrors the surface emitted by `src/rust/crates/fathomdb-py/src/lib.rs`" → cite the crate/module set.
  - `src/python/fathomdb/types.py:352` — cites `fathomdb-py/src/lib.rs:417-432`.
  - `src/python/tests/test_runtime_event_shape.py:26` — cites `fathomdb-py/src/lib.rs:417-444`.

  All three point at `embedder_event_to_py`, which the proposal's own table moves to `types.rs`. Three further live references (`test_surface.py:214-216`, `test_test_hooks_gate.py:269`, `_test_hooks_gate.py:41`) name the path without line numbers and degrade to "near-empty file" rather than "wrong file"; repoint them opportunistically.
- **Why the cross-file view changed it:** the `.pyi` reviewer voted LEAVE on content in isolation and could not see the sibling SPLIT proposal that empties the exact file its header names as its source of truth. Nothing fails red — confirmed there is no test tying the stub to `lib.rs`, and `scripts/lint-plan-anchors.sh` scans only top-level `dev/plans/*.md` with `status: ACTIVE`, so none of these paths are gated. That is precisely why it needs to be written into the proposal: no gate will catch it.

Everything else about the py SPLIT was checked and **confirmed**, not merely accepted:

- `check-governed-surface-pin.sh` (run: exit 0) pins a JSON data file by content hash plus three flat lists of verb *name strings*. Zero file paths, zero module paths. Enforcement that a name is really exposed happens at runtime in `test_surface.py` / `surface.test.ts` against the live compiled module. A `mod` reorg is invisible to this gate by construction.
- `verify-embedder-api-no-drift.sh` is hardcoded to `CRATE="fathomdb-embedder-api"` — a different crate entirely. It has no code path reading `fathomdb-py` or `fathomdb-napi`. **The premise that it constrains either SPLIT does not hold.**
- The claimed py↔napi "mirror" contract was read in both crate headers: both numbered lists are exclusively FFI *semantics* (GIL-release/spawn_blocking, typed-error mapping, string validation, panic translation). Neither says anything about file layout. The two SPLITs having 7 vs 5 modules breaches nothing documented. The four shared module names (`errors/types/engine/translate`) were compared row-by-row and cover the same responsibility in both crates; the py-only extras (`verbs.rs`, `graph.rs`, `embed.rs`) reflect real Python-only surface — the CLS-embedder singleton and the standalone `rerank` pyfunction are absent from the napi crate.

##### 2.2 Engine decomposition §0/§7.1 — record the claim as CONFIRMED, not plausible *[minor]*

- **Original:** §0 asserts inherent-impl-in-any-module + private-fields-visible-to-descendants; §7.1 warns that moving `struct Engine` off the crate root collapses the plan into a `pub(crate)`-field rewrite.
- **Adjustment:** no change to the claim. Downstream records citing this axis should mark it CONFIRMED.
- **Why:** three compiled probes (rustc/cargo 1.95.0, edition 2021, matching the workspace):
  1. **Positive:** `pub struct TheStruct` with fully private fields at crate root; `capability_a.rs` and `capability_b.rs` each carry their own `impl TheStruct`, with `b()` touching the *other* module's private field and calling `a()`. `cargo build` exit 0.
  2. **Negative:** same code, struct moved into `mod engine` and re-exported, capabilities left as siblings. Exit 101 — `error[E0616]: field 'field_a' of struct 'TheStruct' is private`, and the same for `field_b`. The plan's predicted failure reproduces with the exact error code the mechanism would produce.
  3. **Mitigation:** changing only the two field declarations to `pub(crate)` makes the identical negative case compile clean. The plan's stated fallback is confirmed as exactly minimal and exactly sufficient — not a strawman.
- The real crate was checked for five obstruction classes and is clean: zero `macro_rules!` anywhere in `fathomdb-engine/src/`; zero `#[pymethods]`/`#[napi]`/`#[pyclass]`/`#[pymodule]` (those live only in the binding crates); `struct Engine` at L274 with all ~20 fields plain-private — exactly the state probe 1 modeled; exactly **one** top-level `impl Engine` block at L4110, matching the plan's 5008-line description.

##### 2.3 Engine decomposition §2 module tree — add `mean_centering_internals_for_test` *[important]*

- **Original:** the §2 table lists `reader_search_hook` under `reader_pool.rs`, but never names `mean_centering_internals_for_test`.
- **Adjustment:** add `pub mod mean_centering_internals_for_test` (an inline `mod { }` at L9327, not a free function) to the owned-items list for `vector/mean.rs`. It wraps `MeanAccumulator` via its public methods and must physically relocate with its subject type.
- **Why the cross-file view changed it:** direct inspection of `lib.rs` found four `mod` declarations beyond the two file-based ones (`lifecycle`, `pcache2`): `reader_search_hook` (L1098), `mean_centering_internals_for_test` (L9327), and `mod tests` (L17621). Only the first is in the table. An unlisted inline module is exactly the thing a mechanical move silently drops. None of the ~20 planned module names collide with any existing one.

##### 2.4 Engine decomposition §5 mechanical rules — spell out the `use super::` rewrite *[minor]*

- **Original:** §5 says "move each item's tests out of `mod tests` with the item."
- **Adjustment:** add one sentence — when `guard_row_owned_registry` (or any other test) moves out of the crate-root `mod tests`, its `use super::{…}` import list must be rewritten (crate-root re-exports keep every name resolvable via `use crate::{…}`, so this is find-and-replace).
- **Why:** the test block was read in full. It uses `use super::{ Engine, IdSpace, IdSpaceKind, PreparedWrite, derive_stable_id, resolve_source_type, KIND_TO_SOURCE_TYPE_CASE_SQL, ROW_OWNED_PROJECTIONS }` — explicit named imports, not `use super::*` — and a targeted grep for private-field dotted access (`.connection`, `.reader_pool`, `.projection_runtime`, `.counters`, `.subscribers`, `.next_cursor`, `.telemetry`) inside the block returned zero hits. The test only uses the public `Engine::open`/`.close()` API and `rusqlite::Connection::open` directly. So the move is mechanical — but `super` stops meaning crate-root the moment the test relocates, and the resulting compile error would look, to an implementer, like a refutation of §0. One sentence prevents that misread.

##### 2.5 Engine decomposition §5 — the per-PR symbol-diff gate has no implementing tooling *[new, important]*

- **Original:** §5 mandates "every PR ends with a public-symbol diff (rustdoc JSON or `cargo public-api`) proving the exported surface is byte-identical," and §7.2 demands the same verification *before* step 1.
- **Adjustment:** treat this as a prerequisite work item, not a checklist line. A grep across `scripts/`, `.github/workflows/`, and `Cargo.toml` for `public-api` / `public_api` / rustdoc-JSON returned **zero hits** — the tool does not exist in this repo and would have to be built or vendored first.
- **Partial mitigation found during this pass:** the §7.2 risk ("a consumer imports an internal path") is empirically low. Enumerating every distinct `fathomdb_engine::…` path across `fathomdb-py`, `fathomdb-napi`, `fathomdb-cli`, and the engine's own `tests/` yields 30 distinct paths, **all flat crate-root** (`fathomdb_engine::Engine`, `::EngineError`, `::PreparedWrite`, `::rerank_passages`, …) plus `fathomdb_engine::lifecycle::{Subscriber, ProjectionStatus}` — an existing published module the plan leaves untouched. Note `rerank_passages` is a *free function* consumed at the root; under the plan it moves to `search/ranking.rs` and its root `pub use` is load-bearing. This survey is not a substitute for the symbol diff (it proves what consumers import, not that the current `pub` surface is fully root-re-exported), but it removes the "wide and undocumented surface doubles the cost of every step" scenario §7.2 warned about.

---

#### 3. Proposals WITHDRAWN

**None.** Nothing in the cross-file view killed a proposal. The closest thing to a kill is a premise, not a proposal: the assumption that `verify-embedder-api-no-drift.sh` constrains the binding SPLITs is false — that script is scoped to `fathomdb-embedder-api`, a different crate, and never reads either binding.

---

#### 4. Sequencing

##### Do first: `scripts/commission-manifest.sh` heredoc extraction

Of everything actionable, this is the single change to start with. It is a genuine tooling defect (a 674-line Python program invisible to linters and untestable in isolation), it has an existing end-to-end test suite (`scripts/tests/test_commission_manifest.sh`) that verifies it, zero blast radius outside the script, and — decisively — **no prerequisite**. Every other stream either has a prerequisite (engine), a paired doc edit (py SPLIT), or is genuinely optional polish.

If the question is instead "first change *within the engine plan*," the plan's own answer stands and is now better supported: **Step 0** (collapse the duplicated `sqlite_extended_code_name*` match arms, ~20 lines deleted) — a real duplication defect, independent of the restructuring — then **Step 1** (`test_hooks.rs`). Step 1 remains correctly placed first among the moves: it is the one named cohesion defect, it is 46 `*_for_test`/`cfg`-gated methods where "if it compiles, it is correct," and it proves the mechanism under real crate constraints. §2.2 above now proves the mechanism *in principle*; Step 1 proves it *in this crate*.

##### Independent — safe to do anytime, in any order, by anyone

- `scripts/commission-manifest.sh` extraction
- `generate_chain_corpus.py` chain-builder factory
- `slice15d-projection-registry.test.ts` `withEngine()` fixture
- `fathomdb-cli/src/lib.rs` → `report_json.rs`
- `fathomdb-napi/src/lib.rs` SPLIT
- `fathomdb-py/src/lib.rs` SPLIT **+ its citation repoint as one change** (§2.1)
- rust-test `support/query_synthesis.rs` extraction (76 duplicated LOC)
- CI composite action `.github/actions/setup-fathomdb-env`
- Engine Step 0 (duplicate match arms)

The two binding SPLITs are independent **of each other and of the engine work** — different crates, no shared file, and the "mirror" contract is semantic-only, so neither waits on the other. The cli `report_json.rs` extraction should be preceded by the one-line grep the rust-src observation asks for (are there other `*_report_json` mappers elsewhere?) purely to decide module-vs-shared-crate; that grep is not a blocker.

##### Has a prerequisite

| Item | Prerequisite |
|:--|:--|
| Engine Step 1 (`test_hooks.rs`) | §7.2 public-symbol baseline. The tooling does not exist (§2.5) — build or vendor it first. The consumer-path survey above lowers the risk but does not discharge the gate. |
| Engine Steps 1–7, all of them | **A quiet window.** §7.3 is the binding scheduling constraint and it is live right now — 0.8.20 is in flight (an active slice log sits uncommitted in the working tree). A 17,910-line reshuffle landing against concurrent slices converts a mechanical refactor into a merge-conflict tax on every other stream. Steps 5–6 in particular need the file quiet. |
| Engine Step 4 (`open/`) | Steps 5–6, or move it to last. `open_locked` wires a dozen subsystems; extracting it before they move produces call sites you will rewrite twice. |
| Engine Step 6 | Do `filter.rs` first within the step — it is the leaf both `search/exec.rs` and `read/mod.rs` depend on. |
| Engine Steps 2 and 5 | §7.4's mitigation: extract the shared drain→freeze→mutate sequence into one named `pub(crate)` helper at the root *before or with* these steps. If that mitigation is not taken, the plan itself says downgrade them — because `commit_batch`, `commit_projection_outcomes`, `purge_inner`, and `excise_source_inner` currently encode that ordering reviewably-by-scrolling, and codex §9 / HITL review depends on it. |
| py SPLIT | None as a code change, but the §2.1 citation edits must ship in the same change or they will not ship. |

##### Stopping points remain valid

§8 is intact and worth restating: **after Step 1**, the one real cohesion defect is fixed for the cost of one small PR. **After Step 3**, `impl Engine` is ~2,735 and the "140 responsibilities" finding is materially discharged. §7.5 is also intact and should not be softened — there is no predicted defect benefit to this split; the justification is navigability and ownership only, and "do §1 + Step 1 and stop" is a complete, defensible outcome.

---

#### 5. What remains UNVERIFIED

Honest list of what this phase did **not** establish.

1. **Nothing was compiled against the real engine crate under any decomposition.** The visibility probes were three-file synthetic crates. They prove the language mechanism; they do not prove that 17,910 lines with real `cfg(feature = "operator")` / `default-reranker` / `test-hooks` / `eu7_real_corpus_ac` gates, `unsafe` FFI trampolines, and `#[cfg(debug_assertions)]` seams survive the move. Step 1 exists precisely to test that, and it has not been run.

2. **The §7.2 public-symbol diff was not performed.** The consumer-import survey (30 distinct paths, all crate-root) is a proxy, not the gate. It covers three in-repo crates plus engine integration tests — not doc examples, benches, or any external consumer, and it does not prove the current `pub` surface is 100% root-re-exported. And the tool the plan mandates does not exist in this repo yet.

3. **The §2 module-tree line ranges were not audited.** Roughly 20 rows of `Lxxxx–Lyyyy` claiming "~17,800 accounted for" were taken at face value. Overlaps, gaps, and off-by-N were not checked, and neither were the per-module LOC estimates. The same applies to the line ranges and LOC figures in both binding SPLIT tables.

4. **Neither binding SPLIT was compiled.** This is the sharpest asymmetry in this review: the engine's mechanism claim was probed empirically, but the two SPLITs rest on analogous unprobed claims — that napi-rs `build.rs` codegen "scans every module regardless of file layout," and that `#[pymodule]` init plus `#[pymethods]` impls work with their subjects in sibling modules. Both are near-certainly true (proc-macro expansion is per-item, and both frameworks support multi-module crates), but *near-certainly true* is exactly what §0 was before someone compiled it. A ten-minute probe of each would close this.

5. **Merge-conflict cost against in-flight 0.8.20 work was not estimated.** §7.3 is flagged as a scheduling constraint; nobody quantified how many open slices currently touch `fathomdb-engine/src/lib.rs`, which is what would turn "sequence between releases" from advice into a hard date.

6. **The 59 LEAVE verdicts were not re-litigated.** Only `_fathomdb.pyi` was revisited, and only because a sibling SPLIT emptied the file it cites. The other 58 were checked for cross-file *coupling* to the actionable set, not re-reviewed on the merits.

7. **Documentation drift beyond the three py citations was catalogued but not scoped.** ~1,067 references to `fathomdb-py/src/lib.rs` and ~1,165 to `fathomdb-napi/src/lib.rs` exist across the repo; the overwhelming majority are frozen historical records under `dev/plans/runs/**` and `dev/design/**` that are correctly immutable. Only the three live `src/python/**` citations in §2.1 were confirmed as needing action. The equivalent survey for `fathomdb-engine/src/lib.rs:NNNN` citations in design docs — which the engine decomposition will invalidate wholesale — was **not** done. The design-doc axis characterized that as "routine, not blocking" documentation maintenance; its actual volume is unmeasured.
