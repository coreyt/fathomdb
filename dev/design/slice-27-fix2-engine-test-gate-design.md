---
title: Slice 27 fix-2 design memo — gate the engine's operator-method test targets so the default engine test build compiles
date: 2026-06-06
target_release: 0.8.0
owning_slice: 27 fix-2 (resolves the Slice 27 fix-1 codex [P1]; bounded)
status: accepted
desc: >
  fix-1's operator gate hid 12 Engine methods behind `#[cfg(feature="operator")]`,
  which broke `cargo test -p fathomdb-engine` in its DEFAULT config (the engine's
  own integration tests call those methods). fix-2 marks the pure-operator test
  targets `required-features = ["operator"]` and per-fn `#[cfg]`s the one mixed
  file, so BOTH engine configs compile + run without un-gating any method.
---

# Slice 27 fix-2 — engine operator-method test-target gating

## 0. The [P1] being resolved

fix-1 (Option B) gated the 12 operator/recovery `Engine` methods behind
`#[cfg(feature = "operator")]` so the default `fathomdb` facade is recovery-clean
(a re-export cfg can't hide a re-exported type's *inherent* methods, so the gate
MUST live on the engine methods). Side effect: **`cargo test -p fathomdb-engine`
(default features) no longer compiles** — the engine's own integration tests call
the now-hidden methods. fix-1's self-check ran only `cargo test --workspace`,
which passed because `fathomdb-cli` enables `operator` and cargo **feature-unifies
it ON** across the workspace, masking the per-crate breakage. Verdict:
`dev/plans/runs/0.8.0-slice-27-review-fix1-20260606T010212Z.md`.

This fix is **bounded**: it touches only the engine crate's **test targets** (Cargo
`[[test]]` entries + one per-fn `#[cfg]` split). It does **not** un-gate any method,
change engine behavior, the facade gating, the conformance test, `rust.md`/AC-074,
or the AC-050c result.

## 1. RED baseline (verified)

`cargo test -p fathomdb-engine --no-run` (default) fails to compile:
`error[E0599]: no method named dump_schema / dump_row_counts / dump_profile /
rebuild_projections / rebuild_vec0 / trace_source_ref / excise_source /
truncate_wal / verify_embedder / check_integrity / safe_export / recompute_mean
found for struct fathomdb_engine::Engine`. (The exact set of *reported* failing
targets varies run-to-run because cargo aborts the parallel build early — so the
**reliable** signal is a grep of which test files actually call a gated method,
not a single aborted compile log.)

## 2. The operator-method test files (grep of direct gated calls)

14 files under `src/rust/crates/fathomdb-engine/tests/` call a gated method.
Classified **pure-operator** (every `#[test]` needs a gated method → gate the
whole target) vs **mixed** (holds a non-operator test too → must NOT skip the
whole target):

**Pure-operator (13) → `[[test]] … required-features = ["operator"]`:**
`check_integrity`, `dump_profile`, `dump_row_counts`, `dump_schema`,
`eu7_real_corpus_ac`, `excise_source`, `rebuild_projections`, `rebuild_report`,
`rebuild_vec0`, `safe_export`, `trace_source_ref`, `truncate_wal`,
`verify_embedder`. (Verified per file: every `#[test]` attribute is followed by a
gated call before the next test.)

**Mixed (1) → per-fn `#[cfg(feature = "operator")]`:** `pr2b_mean_recompute.rs`.
Its first test `topic_pivot_does_not_auto_recompute_mid_ingest` (line 202) is a
**non-operator** PR-2bc carve-out guard (write/drain/`drain_embedder_events`, no
`recompute_mean`) and MUST keep running on the default build. The other 4 tests
(`manual_recompute_matches_closed_form_and_requantizes_all`,
`recompute_fault_rolls_back_fully`, `recompute_event_fields_and_post_commit_publish`,
`recompute_rejects_non_mc_identity`) call `recompute_mean` → gated, along with
their **operator-exclusive** helpers (`decode_f32`, `subtract`, `quantize_binary`,
`closed_form_mean`, `cosine`), the `NonMcEmbedder` struct + its impls (test-7
only), and the `MeanRecomputeTrigger` import. The shared helpers (`fixture_path`,
`open_caller`, `write_docs`, `read_mean_vec`, `topic_vector`, `hash64`,
`SimulatedBgeEmbedder`, the `Connection`/`EmbedderEvent` imports) stay ungated
(the non-operator test uses them). The exact exclusive set is **compiler-verified**:
after gating, `cargo test -p fathomdb-engine --no-run` (default) + clippy
`-D warnings` flag any straggler, which is then gated.

**Not operator:** `corpus_graph.rs` — only *mentions* `trace_source_ref` in a
`//!` doc comment (0 real calls); it compiles fine on the default build. (It
appeared in the review's failing list only as noise from an aborted parallel
compile; left untouched.)

## 3. No non-test default-feature consumer is affected

Re-confirmed (fix-1 verified the same): no `fathomdb-py` / `fathomdb-napi` /
non-CLI consumer calls any of the 12 gated methods; the only non-test default
consumer that did — the `ingest_corpus` example — was already gated by fix-1
(`required-features = ["operator"]`). So fix-2's surface is exactly the engine
test targets.

## 4. Why `required-features` (not un-gating, not `#[cfg]`-ing every test fn)

- **Un-gating** the methods would re-open the original [P1] (recovery-name methods
  reachable on the default facade). Off the table.
- `required-features` is the established pattern (fix-1 used it for the example)
  and is the least-churn way to skip a whole pure-operator target on the default
  build while still running it under `--features operator`.
- Per-fn `#[cfg]` is reserved for the one mixed file, where a whole-target skip
  would silently drop the legitimate non-operator carve-out test.

## 5. Scope / non-goals

Only `fathomdb-engine/Cargo.toml` `[[test]]` entries + the `pr2b_mean_recompute.rs`
per-fn `#[cfg]` split + this memo + DOC-INDEX. No engine method un-gated; no engine
behavior change; facade gating, `governed_surface.rs`/method-absence doctests,
byte-frozen `no_recovery_surface.rs`, `rust.md`/AC-074, and AC-050c rc=0 all
unchanged from fix-1.
