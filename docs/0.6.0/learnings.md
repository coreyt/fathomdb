---
title: 0.6.0 Learnings
date: 2026-04-24
target_release: 0.6.0
desc: Keep-doing + stop-doing from 0.5.x; prior-work disposition table
blast_radius: TBD
status: living
---

# Learnings

Living during 0.6.0 doc phase. No formal lock.

## Keep doing

- Red→green→refactor TDD on every behavior change; pair the failing test with the feature in the same pack. Cite: memory `feedback_tdd`; commit pattern visible in `5f34479` `test(admin): red tests…` immediately preceding `a96d008` `feat(admin): introspection APIs…`. Why it worked: red tests proved the gap before code existed; mechanical bumps reuse the existing suite as gate.
- Orchestrator on main thread; delegate coding to `implementer` (worktree) and diffs to `code-reviewer`. Cite: memories `feedback_orchestrate_releases`, `feedback_orchestrator_thread`. Why it worked: avoids nested-orchestrator confusion; main thread keeps plan + verification in one context.
- Net-negative LoC on reliability releases; "delete before add" as the default move. Cite: `dev/notes/0.5.7-corrected-scope.md` (Principle 2; size table −5,000 LoC); memory `feedback_reliability_principles`. Why it worked: surface-area reduction is the only durable fix for the Pack A–H scar-tissue pattern.
- Post-publish smoke install from registry before declaring a release done. Cite: memory `feedback_release_verification`; `b4fe850` `fix(0.5.6): atexit engine cleanup + legacy configure_vec shim`. Why it worked: green CI + published wheel missed the atexit and shim regressions; install-from-registry caught them.
- Cross-platform CI matrix (Linux x86_64, Linux aarch64, macOS, Windows) gating publish; clippy + fmt per commit. Cite: `dev/notes/0.5.7-corrected-scope.md` T1 §6; memory `feedback_cross_platform_rust` (c_char i8/u8 split caught only by matrix). Why it worked: aarch64 Linux c_char regression would never have surfaced on a single-platform gate.
- Workflow validation via actionlint, not `yaml.safe_load`. Cite: memory `feedback_workflow_validation`. Why it worked: yaml.safe_load passes schema-invalid syntax GitHub silently rejects.
- Native build via `pip install -e python/`, never manual `cargo build` + `cp`. Cite: memory `feedback_python_native_build`. Why it worked: manual copy desynchronizes wheel/source and hides ABI breaks.
- ADRs documenting choice rationale (A/B/C tradeoffs preserved across releases). Cite: `dev/arch-decision-vector-embedding-recovery.md` (folded into 0.6.0 ADR per disposition table). Why it worked: "Choice C" framing remained load-bearing across two releases without re-arguing.
- Deprecation shims as first-class code paths with their own tests. Cite: memory `feedback_release_verification`; `b4fe850` legacy `configure_vec` shim regression. Why it worked: untested shims silently break on rename passes.
- Vector identity owned by the embedder, never carried on vector configs. Cite: memory `project_vector_identity_invariant`; `dev/notes/project-vector-identity-invariant.md`. Why it worked: removed an entire class of identity-collision and cross-model comparison bugs.
- Fused JSON filters raise `BuilderValidationError` on missing schema, never silently degrade. Cite: memory `project_fused_json_filters_contract`. Why it worked: silent degrade would hide indexing-mismatch bugs in client code.
- C interop uses `std::os::raw::c_char`, never hardcoded `i8`/`u8`. Cite: memory `feedback_cross_platform_rust`. Why it worked: c_char's signedness varies by platform; hardcoding breaks one target.
- No data migration in feature/reliability releases (schema additions only; no `INSERT…SELECT` across legacy tables). Cite: memory `feedback_no_data_migration`. Why it worked: managed-vector-projection v24 shipped without the historical migration footgun.
- Three-layer observability separation: response-cycle feedback (app contract), structured tracing (diagnostic), telemetry (resource cost). Cite: `dev/design-logging-and-tracing.md` Principle 1; `dev/design-note-telemetry-and-profiling.md` §"Relationship to Existing Systems". Why it worked: each consumer (app code / operator / capacity planner) gets the right channel; merging them historically forced one channel to compromise. **Status: carry-forward — verify implementation in 0.6.0 stays separated and meets each consumer's goal (per critic-A F10).**
- Library emits, application configures (no global subscriber set inside fathomdb crates). Cite: `dev/design-logging-and-tracing.md` Principle 2. Why it worked: library-side subscriber config breaks multi-consumer environments and was a known anti-pattern from the `tracing` ecosystem.

## Stop doing

- Cypher / alternative query-language surface. Cite: `dev/pathway-to-basic-cypher-2026-04-17.md` (dropped per disposition); `dev/notes/0.6.0-rewrite-proposal.md` § Anti-requirements; `dev/notes/0.6.0-roadmap.md` (superseded). Why it broke: surface bloat on an unstable engine; client-side traversal already covered the use case.
- Per-item variable embedding / identity strings on vector configs. Cite: memory `project_vector_identity_invariant`; `dev/notes/0.5.7-corrected-scope.md` D1 (delete `set_vec_profile`, `EmbedderChoice::Builtin`). Why it broke: identity leaked into config rows that outlived the embedder, producing cross-model comparisons silently.
- Layers-on-layers profile→kind→vec configure verbs (`configure_vec(kind_or_embedder)` dispatching on `isinstance(str)`). Cite: `dev/notes/0.5.7-corrected-scope.md` Diagnosis (`_admin.py:613-661`); `dev/notes/2026-04-22-typescript-configure-embedding-napi.md` (dropped per disposition). Why it broke: one verb, two incompatible shapes across two schema generations; produced 12+ coordinator sites with `SQLITE_SCHEMA` flooding.
- Three parallel sources of truth for one boolean (`vector_profiles`, `projection_profiles('*','vec')`, `vector_embedding_profiles`). Cite: `dev/notes/0.5.7-corrected-scope.md` Diagnosis (`bootstrap.rs:1348-1359`). Why it broke: triple-write to keep `BootstrapReport::vector_profile_enabled` from lying; classic over-design symptom.
- Runtime DDL on admin path (per-kind `vec_<Kind>` virtual tables). Cite: `dev/notes/0.5.7-corrected-scope.md` Diagnosis + F1 (`bootstrap.rs:1318-1347`). Why it broke: invalidates `prepare_cached` on shared pool → `SQLITE_SCHEMA` (code 17) flood across read path.
- **Defect deferral patterns** (merged: speculative knobs, soak-as-substitute, punt-reliability — distinct anti-patterns; common mode = defect hidden or postponed instead of caught by a deterministic test).
  - Speculative knobs / silent feature-gated fallbacks (`EmbedderChoice::Builtin` → `None` without `default-embedder` feature). Cite: `dev/notes/0.5.7-corrected-scope.md` Diagnosis + D1/F2 (`python/pyproject.toml:47`). Why it broke: silent-None surfaced downstream with the wrong error message; users could not diagnose.
  - Soak as a substitute for tests. Cite: `dev/notes/0.5.7-corrected-scope.md` Principle 1 + Release Gate. Why it broke: outsources defect discovery to users; defects only caught by soak must be caught by a test instead.
  - Punting reliability bugs to 0.6/0.7 when they have already burned a client. Cite: `dev/notes/0.5.7-corrected-scope.md` Principle 3; memory `feedback_reliability_principles`. Why it broke: retry-wrapper papered over a connection-role bug for two releases; root cause kept resurfacing.
- Silent-degrade on missing schema. Cite: memory `project_fused_json_filters_contract`. Why it broke: hides client indexing mismatches that should fail loud at builder time.
- Mocked DB in integration tests / JSON-assertion tests of admin behavior. Cite: `dev/notes/0.5.7-corrected-scope.md` Diagnosis (`test_profile_management.py:112,419`); `dev/test-plan.md` (no-mocks rule). Why it broke: per-verb tests passed against fresh DBs while end-to-end Memex flow regressed; nothing exercised cached-reader + admin-DDL interaction that produced the SQLITE_SCHEMA bug.
- `yaml.safe_load` as workflow validator. Cite: memory `feedback_workflow_validation`. Why it broke: passes schema-invalid syntax (e.g., `${{ runner.* }}` at job-env level) that GitHub silently rejects.
- Hardcoded `i8` / `u8` at the C interop boundary. Cite: memory `feedback_cross_platform_rust`. Why it broke: c_char is `i8` on x86_64 + Darwin and `u8` on aarch64 Linux; either choice breaks one target.
- Atomic merges without an explicit deletion phase (Rust+Python+TS pack form: F13 critic rephrase). Cite: `dev/notes/0.5.7-corrected-scope.md` P1. Why it broke: Pack A–H shipped 41 admin verbs of scar tissue in 4 days with no deletion phase; cross-language synchronization is fine, but a merge that only adds (never removes) accumulates surface that no later release reverses.
- Client-coupled docs / per-client narratives in engine repo (Memex/OpenClaw integration designs). Cite: `dev/architecture-note-memex-support.md` (dropped); `dev/archive/openclaw-2026-04-18/*` (mostly dropped); `dev/notes/0.6.0-rewrite-proposal.md` Anti-requirements. Why it broke: thick-about-clients pattern produced design churn every time a client pivoted.
- Data migration in feature releases. Cite: memory `feedback_no_data_migration`. Why it broke: legacy `INSERT…SELECT` from `vector_profiles` / `projection_profiles('*','vec')` would have re-introduced the three-source-of-truth class.
- pyo3-log emitting from Rust paths while another engine instance contends the GIL. Cite: `dev/archive/pyo3-log-gil-deadlock-evidence.md`; commits `cf0b190`, `d09deb4`. Why it broke: two `Engine` instances + Python root logger at DEBUG = indefinite hang on `Engine.open`. Per-call-site fixes do not address the class.
- TS admin surface ahead of stable Python admin. Cite: `dev/notes/0.5.7-corrected-scope.md` D4 (TS admin 764 LoC, no production consumer); memory `project_typescript_sdk` (TS not yet Python-parity). Why it broke: 3× release cost on an unstable API.



## Prior work disposition

Triage performed 2026-04-24 by prose-harvester agent against
`agents/prose-harvester.md` contract. Bias toward `drop` per Phase 1a method.
Stop-doings enforced as drop signals (per learnings.md § Stop doing): cypher
surface, per-item variable embedding, nested profile→kind→vec configure,
silent-degrade-on-missing-schema, mocked DB in integration tests, data
migration in feature releases.

Verdicts: `keep | fold | archive | drop`.

### `dev/` (top-level)

| Source file | Verdict | Target | Notes |
|-------------|---------|--------|-------|
| `dev/ARCHITECTURE.md` | fold | `docs/0.6.0/architecture.md` | §1–4 stack/strata/WAL/single-writer survive; sections naming `vec_<kind>` per-kind virtual tables, three vector-identity tables, and 25-migration schema must be stripped (superseded by `dev/notes/0.5.7-corrected-scope.md` 2026-04-24, "delete before add") |
| `dev/ARCHITECTURE-deferred-expansion.md` | drop | — | Defends engine-stays-thin via "nodes+edges+JSONB express any domain"; same boundary now stated more crisply in `dev/engine-vs-application-boundary.md` and in `0.6.0-rewrite-proposal.md` thin-plus thesis. Net new content = zero. |
| `dev/arch-decision-vector-embedding-recovery.md` | fold | `docs/0.6.0/adr/` (vector recovery ADR) | Choice A/B/C tradeoff still load-bearing for ADR; "Choice C" framing decided. Strip "FathomDB-managed async/incremental" implementation specifics — will be re-decided in Phase 2. |
| `dev/architecture-note-memex-support.md` | drop | — | Memex-integration narrative; per `0.6.0-rewrite-proposal.md` thin-plus thesis the engine should not document specific clients. Replace with generic boundary doc only. |
| `dev/USER_NEEDS.md` | drop | — | HITL F2: superseded by `dev/notes/0.6.0-rewrite-proposal.md` (kept) which already serves as rewrite charter; three overlapping requirement sources = three-sources-of-truth anti-pattern. |
| `dev/production-acceptance-bar.md` | fold | `docs/0.6.0/acceptance.md` | Numerical gates (write p95 100 ms, text p95 150 ms, vec p95 200 ms, safe_export 500 ms) survive as candidate AC values; the "Required CI gates" list drops — it's 0.5.x-shaped. |
| `dev/test-plan.md` | drop | — | HITL F3: 9-layer model encodes the 45-admin-verb taxonomy being deleted; redo top-down from ≤5-verb API in Phase 3f. Preserve only the no-mocks rule (already cited under stop-doing). |
| `dev/engine-vs-application-boundary.md` | fold | `docs/0.6.0/architecture.md` (boundary section) | Crisp engine-vs-application split; merges with `0.6.0-rewrite-proposal.md` thin-plus thesis. |
| `dev/dbim-playbook.md` | fold | `docs/0.6.0/design/admin.md` (or recovery doc) | HITL F8: data safety + recovery is essential to FathomDB. 3-class corruption model + 4-invariant recovery model is durable; integrity-tool surface must exist in 0.6.0. Strip 0.5.x admin verb names. |
| `dev/doc-governance.md` | drop | — | 0.5.x dev-doc taxonomy; superseded by `docs/0.6.0/plan.md` Phase 0 doc-types + done-defs. |
| `dev/fathom-integrity-recovery.md` | fold | `docs/0.6.0/design/admin.md` | Layer model for Go integrity tool survives as design input; defer cli-shape decisions to Phase 2 ADR. |
| `dev/fathomdb-v1-path-to-production-checklist.md` | drop | — | 0.5.x readiness matrix; superseded by `docs/0.6.0/acceptance.md` (planned). The `scripts/check-doc-hygiene.py` dependency is a 0.5.x artifact. |
| `dev/release-policy.md` | keep | `docs/0.6.0/` (release-policy section, possibly in followups) | Version-source-of-truth + tag policy is independent of engine rewrite; lightly applicable. |
| `dev/repair-support-contract.md` | fold | `docs/0.6.0/design/admin.md` | Specific 0.5.x verb names (restore_vector_profiles etc.) drop; the contract concept (which corruption classes are in/out of automated repair) survives as a requirement. |
| `dev/security-review.md` | drop | — | 2026-03-24 finding list, all marked remediated; redundant with planned `docs/0.6.0/security-review.md` to be re-run against the locked design. |
| `dev/setup-admin-bridge.md` | drop | — | JSON-stdio bridge protocol is 0.5.x interface decision; rewrite re-decides via `interfaces/wire.md` in Phase 3e. |
| `dev/setup-round-trip-fixtures.md` | drop | — | 0.5.x test scaffolding tracker; status note from typed-write era. |
| `dev/pathway-to-basic-cypher-2026-04-17.md` | drop | — | Cypher / alt-query-language surface — explicit stop-doing in `learnings.md`; supersedes 2026-03-28 doc but the supersession itself is dropped. |
| `dev/plan-automatic-background-retention.md` | drop | — | 0.5.x execution tracker for retention design; the design itself superseded by `dev/design-operational-retention-scheduling.md` (BY-DESIGN — operator schedules). |
| `dev/restore-reestablishes-retired-projection-state.md` | fold | `docs/0.6.0/design/projections.md` | Restore-must-restore-projections requirement is durable; folds into projections design. |
| `dev/schema-declared-full-text-projections-over-structured-node-properties.md` | fold | `docs/0.6.0/design/projections.md` | Per-kind FTS contract (`fts_props_<kind>`) is the established model and survives as input. |
| `dev/design-adaptive-text-search-surface.md` | fold | `docs/0.6.0/design/retrieval.md` | Adaptive text_search + SearchHit/SearchRows is durable retrieval-shape input. |
| `dev/design-adaptive-text-search-surface-addendum-1-vec.md` | fold | `docs/0.6.0/design/retrieval.md` | Generalizes text+vector into one adaptive retrieval surface; load-bearing for retrieval design. |
| `dev/design-add-operational-store-feature.md` | fold | `docs/0.6.0/design/engine.md` (operational store section) | HITL F8 (op-store cluster, settled): no dual-store. FathomDB operational-store needs live in the same sqlite file as primary entities; clients keep their own storage. Doc folds with that constraint. |
| `dev/design-automatic-background-retention.md` | drop | — | Decision summary "engine provides primitives, scheduling lives outside" is one-line; capture in followups, drop the doc. |
| `dev/design-bridge-input-safety.md` | drop | — | H-1/H-6 finding patches, remediated; a fresh security review will be run in Phase 3g. |
| `dev/design-detailed-supersession.md` | fold | `docs/0.6.0/design/engine.md` | row_id/logical_id/superseded_at supersession contract is durable engine semantics. |
| `dev/design-external-content-memex-integration.md` | drop | — | Memex-specific narrative; per stop-doing on client-coupling. Generic external-content design lives in next file. |
| `dev/design-external-content-objects.md` | fold | `docs/0.6.0/followups.md` | Out of 0.6.0 core scope (rewrite proposal limits to durable serialized writes + vector + FTS+graph fusion); preserve as followup candidate. |
| `dev/design-grouped-read-batching.md` | drop | — | M-4 N+1 patch design; coordinator.rs implementation specifics. Re-decide read-execution shape from scratch. |
| `dev/design-id-generation-policy.md` | fold | `docs/0.6.0/design/engine.md` | row_id/logical_id ownership policy is durable; survives. |
| `dev/design-logging-and-tracing.md` | fold | `docs/0.6.0/requirements.md` (observability) + `docs/0.6.0/design/engine.md` | Observability requirement harvest source per plan.md Phase 1a high-signal list. |
| `dev/design-note-encryption-at-rest-in-motion.md` | fold | `docs/0.6.0/followups.md` | Marked "Not implemented"; defer to followups, do not gate 0.6.0. |
| `dev/design-note-telemetry-and-profiling.md` | fold | `docs/0.6.0/requirements.md` (observability) | Telemetry/profiling requirement harvest per plan.md Phase 1a. |
| `dev/design-operational-payload-schema-validation.md` | fold | `docs/0.6.0/design/engine.md` | HITL F8 (op-store cluster, settled): op-store in same sqlite file → opt-in payload validation contract folds as engine-design input. |
| `dev/design-operational-retention-scheduling.md` | drop | — | M-3 finding "by design — operator schedules"; one-line decision. |
| `dev/design-operational-secondary-indexes.md` | fold | `docs/0.6.0/design/engine.md` | HITL F8 (op-store cluster, settled): op-store lives in same sqlite → bounded secondary-index contract folds as engine-design input. |
| `dev/design-prepared-write-representation.md` | drop | — | PreparedWrite typed-vs-SQL implementation choice; re-decide from scratch in Phase 2 ADR (writer architecture). |
| `dev/design-projection-coverage.md` | drop | — | 0.5.x typed-write Phase-2 follow-on; coverage gaps will be re-derived from Phase 3f test plan. |
| `dev/design-provenance-policy.md` | fold | `docs/0.6.0/design/engine.md` | source_ref required-vs-optional policy is durable engine semantics. |
| `dev/design-provenance-retention.md` | drop | — | C-5 unbounded-growth patch for provenance_events; implementation-specific. Capture "provenance must be bounded" in requirements; drop the patch design. |
| `dev/design-python-bindings.md` | fold | `docs/0.6.0/interfaces/python.md` | Goals/non-goals + scope of moderate Python binding survive as interface input. |
| `dev/design-python-thread-safety.md` | drop | — | Issue #30 implementation note; specific to 0.5.x EngineCore unsendable. Re-derive from binding design. |
| `dev/design-reader-connection-pool.md` | drop | — | M-1 patch design for single-reader serialization; coordinator-specific. Capture "reads must not serialize" as requirement. |
| `dev/design-read-execution.md` | fold | `docs/0.6.0/design/engine.md` (read execution) | Layer boundary + responsibilities (WAL readers, prepared cache, shape hash) is durable engine input. |
| `dev/design-read-result-diagnostics.md` | drop | — | Phase-2 followup decision on QueryRows metadata; defer to Phase 3 design. |
| `dev/design-repair-provenance-primitives.md` | fold | `docs/0.6.0/design/admin.md` | Admin/repair primitive enumeration survives as design input. |
| `dev/design-response-cycle-feedback.md` | fold | `docs/0.6.0/requirements.md` (observability) | Slow/healthy/failed/stalled signaling is durable cross-cutting requirement. |
| `dev/design-restore-edge-validation.md` | drop | — | H-3 dangling-edge fix; semantics survive in `dev/design-detailed-supersession.md`. |
| `dev/design-safe-export-manifest-atomicity.md` | drop | — | H-4/M-6 findings; standard atomic-rename pattern, will reappear in any sane design. No need to preserve. |
| `dev/design-schema-migration-safety.md` | drop | — | C-3/C-4 findings; the rewrite has no migration story (non-goal: data migration). 0.6.0 is fresh-db-only per `plan.md`. |
| `dev/design-shape-cache-bounds.md` | drop | — | M-2 finding "in practice bounded"; closed as not-an-issue. |
| `dev/design-structured-node-full-text-projections.md` | drop | — | Status: Implemented; rationale doc redundant with `dev/schema-declared-full-text-projections-over-structured-node-properties.md` (folded). |
| `dev/design-vector-regeneration-failure-audit-typing.md` | drop | — | 0.5.x audit-event typing fix; stop-doing — managed vector projection rewrite makes this obsolete. |
| `dev/design-wal-size-limit.md` | drop | — | H-2 PRAGMA fix; one-line config decision, re-decide in Phase 2. |
| `dev/design-writer-thread-safety.md` | drop | — | C-1/C-2/H-5 patches; writer architecture re-decided in Phase 2 ADR. |

### `dev/notes/`

| Source file | Verdict | Target | Notes |
|-------------|---------|--------|-------|
| `dev/notes/0.5.7-corrected-scope.md` | keep | `docs/0.6.0/learnings.md` (already cited) | Source-of-truth for stop-doing list (over-design diagnosis, three-vector-tables, runtime DDL). Already drives `learnings.md` § Stop doing. |
| `dev/notes/0.5.7-design.md` | drop | — | Superseded by `0.5.7-corrected-scope.md` (per its own header). |
| `dev/notes/0.5.7-scope.md` | drop | — | Superseded by `0.5.7-corrected-scope.md` (per its own header). |
| `dev/notes/0.6.0-rewrite-proposal.md` | keep | `docs/0.6.0/requirements.md` (input) + `docs/0.6.0/architecture.md` (input) | Thin-plus thesis + 2000-line test + agentic-client requirements is the rewrite charter. Load-bearing. |
| `dev/notes/0.6.0-roadmap.md` | drop | — | Superseded by `0.6.0-rewrite-proposal.md` (per its own header — Cypher-forward direction explicitly rejected). Also stop-doing on Cypher. |
| `dev/notes/2026-04-22-auto-drain-error-tracing.md` | drop | — | 0.5.4 follow-up patch; specific to current `auto_drain_vector_work`; rewrite re-designs vector projection. |
| `dev/notes/2026-04-22-typescript-configure-embedding-napi.md` | drop | — | TS napi wrapper for `configure_embedding`/`configure_vec_kind`; both names are nested-configure-layer stop-doings. |
| `dev/notes/adaptive-search-response-shape.md` | fold | `docs/0.6.0/design/retrieval.md` | Concrete SearchRows/SearchHit shape — useful retrieval design input. |
| `dev/notes/adaptive-search-winning-branch.md` | fold | `docs/0.6.0/design/retrieval.md` | Dedup/precedence rule for cross-branch hits; load-bearing for retrieval determinism. |
| `dev/notes/agent-harness-reference.md` | archive | `docs/archive/0.5.x/agent-harness-reference.md` | Agent-harness ops; useful historical context; not part of engine doc set. |
| `dev/notes/agent-harness-runbook.md` | archive | `docs/archive/0.5.x/agent-harness-runbook.md` | Same as above; orchestrator runbook. |
| `dev/notes/design-0.5.4-async-rebuild-lifecycle-hardening.md` | drop | — | 0.5.4 implementation patches; specific to RebuildActor lifecycle. |
| `dev/notes/design-0.5.4-projection-identity-and-tokenizer-hardening.md` | fold | `docs/0.6.0/design/projections.md` | Projection-identity-collision class is a real lesson (per stop-doing on layered abstractions); fold the class, drop the patch. |
| `dev/notes/design-0.5.4-recover-destination-atomicity.md` | drop | — | Recovery destination TOCTOU; standard mkstemp pattern — not load-bearing. |
| `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md` | drop | — | HITL F1: doc's decision body is the implementation strategy (FathomDB-managed async drain on admin path) which violates Stop-doing on runtime DDL on admin path + nested-configure layers. Preserve only the **invariant** "embedding identity is database-wide; vector indexing is per-kind" — this is already covered by `project-vector-identity-invariant.md` (kept). Folding the doc minus its decision = folding the title. |
| `dev/notes/design-projection-freshness-sli-harness-2026-04-23.md` | fold | `docs/0.6.0/design/projections.md` + `docs/0.6.0/acceptance.md` | Freshness SLI is in plan.md Phase 1a high-signal list; survives as both design input and AC source. |
| `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md` | fold | `docs/0.6.0/acceptance.md` + `docs/0.6.0/test-plan.md` | Performance gate proposals; AC-source material per plan.md Phase 1a. |
| `dev/notes/design-vector-projection-background-scheduler-2026-04-23.md` | fold-rewrite | `docs/0.6.0/design/scheduler.md` | HITL F5: do **not** lift as-is. Scheduler must be rewritten top-down from "client-observable completion barrier" raw-req. Required properties (per HITL): leverage Arc/async; surface "vec-not-yet-consistent" to client when responding so clients can decide whether to wait or proceed. Do not inherit 0.5.x VectorProjectionActor + per-kind DDL boundary problem (root issue dropped per F1). |
| `dev/notes/implementation-plan-db-wide-embedding-per-kind-vector-2026-04-22.md` | drop | — | Pack sequencing for the 0.5.4 work; superseded by managed-vector handoff and now by the rewrite. |
| `dev/notes/managed-vector-projection-followups-2026-04-23.md` | fold | `docs/0.6.0/followups.md` | In plan.md Phase 1a high-signal list; followups still valid. |
| `dev/notes/managed-vector-projection-handoff-2026-04-22.md` | drop | — | 0.5.4 pack handoff narrative; the work shipped, summary not load-bearing for rewrite. |
| `dev/notes/memex-vector-integration-example-2026-04-22.md` | drop | — | Client-specific (Memex); per stop-doing on client coupling. Also explicitly criticized in `0.5.7-corrected-scope.md` as misleading. |
| `dev/notes/project-vector-identity-invariant.md` | keep | `docs/0.6.0/design/vector.md` (invariants section) | Identity-belongs-to-embedder rule, codified in user memory `project_vector_identity_invariant`. Load-bearing. |
| `dev/notes/release-checklist.md` | fold | `docs/0.6.0/` (release section in followups or release-policy.md) | Procedural; some 0.5.x specifics drop. |
| `dev/notes/spec-supported-query-primitives-and-operators-2026-04-11.md` | fold | `docs/0.6.0/design/retrieval.md` | Constrained safe-subset operator spec; durable retrieval surface input. |
| `dev/notes/wake-enhancements.md` | drop | — | Exploratory cross-project (wake) speculation; not in 0.6.0 scope per rewrite proposal's "core 8000 LoC" focus. |

### `dev/archive/`

| Source file | Verdict | Target | Notes |
|-------------|---------|--------|-------|
| `dev/archive/0.1_IMPLEMENTATION_PLAN.md` | drop | — | 0.1 monorepo layout plan; long superseded. |
| `dev/archive/0.2.0current-state.md` | drop | — | 2026-04-08 state snapshot. |
| `dev/archive/0.4.1-scope.md` | drop | — | Historical scope. |
| `dev/archive/0.4.x-todo.md` | drop | — | Historical todo. |
| `dev/archive/0.5.0-scope.md` | drop | — | Historical scope. |
| `dev/archive/0.5.1-scope.md` | drop | — | Historical scope, all done. |
| `dev/archive/0.5.2-scope.md` | drop | — | Historical hotfix scope. |
| `dev/archive/0.5.3-scope.md` | drop | — | Historical scope. |
| `dev/archive/additional-stress-tests-2026-04-10.md` | drop | — | 0.4.x stress test scope; supersession by `dev/test-plan.md`. |
| `dev/archive/additiona-stress-tests-plan-2026-04-10.md` | drop | — | Same as above (note: typo'd filename). |
| `dev/archive/db-integrity-management.md` | drop | — | Already-archived expert notes; current playbook is `dev/dbim-playbook.md` (folded). |
| `dev/archive/design-0.4.1-async-rebuild.md` | drop | — | 0.4.1 implementation design, shipped. |
| `dev/archive/design-0.4.1-documentation.md` | drop | — | 0.4.1 docs gate, shipped. |
| `dev/archive/design-0.4.1-expand-target-filter.md` | drop | — | 0.4.1 implementation design, shipped. |
| `dev/archive/design-0.4.1-searchbuilder-expand-chain.md` | drop | — | 0.4.1 implementation, shipped. |
| `dev/archive/design-0.4.1-stress-tests.md` | drop | — | 0.4.1 stress test scope, shipped. |
| `dev/archive/design-0.5.1-edge-property-filter.md` | drop | — | 0.5.1 implementation, shipped. |
| `dev/archive/design-0.5.1-fused-filter-completeness.md` | drop | — | 0.5.1 implementation, shipped. Also: per `feedback_python_native_build.md` memory, fused-filter contract is preserved separately. |
| `dev/archive/design-0.5.1-matched-paths-chunk.md` | drop | — | 0.5.1 wiring fix, shipped. |
| `dev/archive/design-0.5.1-stella-baseurl.md` | drop | — | 0.5.1 baseUrl fix, shipped; embedder-specific. |
| `dev/archive/design-0.5.2-check-semantics-weighted-fts.md` | drop | — | 0.5.2 hotfix, shipped. |
| `dev/archive/design-0.5.3-configure-fts-ts-parity.md` | drop | — | 0.5.3 TS parity fix; nested configure stop-doing scope. |
| `dev/archive/design-0.5.3-edge-projecting-traversal.md` | fold | `docs/0.6.0/design/retrieval.md` | First-class EdgeRow + edge-projecting traversal is a durable retrieval/traversal lesson; survives. |
| `dev/archive/design-0.5.3-fts5-metachar-escape.md` | drop | — | 0.5.3 escape fix, shipped. |
| `dev/archive/design-0.5.3-fts-live-table-preservation.md` | drop | — | 0.5.3 hotfix, shipped. |
| `dev/archive/design-0.5.3-node20-upgrade.md` | drop | — | CI upgrade, shipped. |
| `dev/archive/design-0.5.3-scale-race.md` | drop | — | Test-only fix, shipped. |
| `dev/archive/design-0.5.3-ts-feedback-fastpath.md` | drop | — | TS perf refactor, shipped. |
| `dev/archive/design-detailed-safe_export.md` | drop | — | safe_export hardening, shipped; design will re-emerge from acceptance. |
| `dev/archive/design-engine-lifecycle.md` | fold | `docs/0.6.0/design/engine.md` | HITL F8: lifecycle re-derived from close/exit reliability raw-req (not 0.5.x atexit shape). atexit hook in 0.5.6 (commit `b4fe850`) was load-bearing — CPython does not reliably run `__del__` on module-level refs at shutdown, so worker thread kept process alive after bare `Engine.open()` with no explicit `close()`. 0.6.0 design must preserve "interpreter exit completes within bounded time even with un-`close()`d engines" as a reliability invariant; specific mechanism (atexit, weakref, finalizer) re-decided. |
| `dev/archive/design-fts-empty-leaf-position-fix.md` | drop | — | 0.3.1 patch, shipped. |
| `dev/archive/design-gap-fixes-0.4.2-0.4.5.md` | drop | — | 0.4.x binding gaps, shipped. |
| `dev/archive/design-note-typescript-sdk.md` | fold | `docs/0.6.0/interfaces/typescript.md` (input) | TS-SDK strategic decisions (napi-rs, Node-only, parity-with-Python) are durable interface-shape input. |
| `dev/archive/design-python-application-harness.md` | drop | — | Harness implementation plan; per stop-doing on mocked DB, real-harness pattern survives but this specific design doesn't. |
| `dev/archive/design-remaining-work-fathomdb-support-memex.md` | drop | — | Memex-specific remaining-work narrative; client coupling stop-doing. |
| `dev/archive/design-text-query-ast-2026-04-11.md` | fold | `docs/0.6.0/design/retrieval.md` (input) | Typed TextQuery layer is the strict-grammar foundation; survives as input alongside adaptive design. |
| `dev/archive/design-typed-write.md` | fold | `docs/0.6.0/design/engine.md` (writer section, input) | HITL F8: **typed at engine boundary is settled** — clients never push raw SQL. Specific PreparedWrite shape re-decided in Phase 2 ADR. Boundary itself is decision-recorded, not pending. |
| `dev/archive/design-typescript-embedding-adapters-0.5.0.md` | drop | — | Embedder adapter set (OpenAI/Jina/Stella) — per `0.6.0-rewrite-proposal.md` thin-plus, embedder is client concern. |
| `dev/archive/design-user-picked-projections.md` | drop | — | User-picked tokenizers/embeddings; supplanted by db-wide embedding-identity invariant + nested-configure stop-doing. |
| `dev/archive/design-vector-regeneration-hardening.md` | drop | — | 0.1 vector regen hardening; managed vector projection rewrite obsoletes. |
| `dev/archive/design-windows-executable-trust-support.md` | drop | — | Cross-platform executable trust; specific 0.5.x platform fix, will reappear if needed. |
| `dev/archive/dev-document-cleanup-plan-2026-04-22.md` | drop | — | Already-completed cleanup pass. |
| `dev/archive/dev-document-status-audit-2026-04-22.md` | drop | — | Audit baseline for prior cleanup, superseded. |
| `dev/archive/documentation-review-plan-2026-04-22.md` | drop | — | Superseded by `docs/0.6.0/plan.md`. |
| `dev/archive/experts-view.md` | archive | `docs/archive/0.5.x/experts-view.md` | Original architectural rationale (SQLite-as-VM); historical context worth retaining. |
| `dev/archive/fathomdb-production-readiness-2026-03-28.md` | drop | — | Snapshot, superseded by `dev/fathomdb-v1-path-to-production-checklist.md` (also dropped). |
| `dev/archive/fathom-memex-near-term-roadmap.md` | drop | — | Memex-specific roadmap; client coupling. |
| `dev/archive/fts-level-three-2026-04-11.md` | drop | — | FTS Level 3 research, implemented in 0.5.0. |
| `dev/archive/handoff-0.4.5.md` | drop | — | 0.4.5 handoff, shipped. |
| `dev/archive/implementation-operational-store-plan.md` | drop | — | Op-store implementation tracker, shipped. |
| `dev/archive/implementation-plan-0.5.0.md` | drop | — | 0.5.0 implementation plan, shipped. |
| `dev/archive/implementation-plan-text-query-tdd-2026-04-11.md` | drop | — | TextQuery TDD implementation plan, shipped. |
| `dev/archive/improving-with-better-tokenization-2026-04-11.md` | drop | — | Superseded by `dev/design-adaptive-text-search-surface.md` (per its own header). |
| `dev/archive/investigation-edge-properties-20260417.md` | drop | — | Investigation note that fed `0.5.1-edge-property-filter` and `0.5.3-edge-projecting-traversal`; both folded/dropped accordingly. |
| `dev/archive/layer-8-9-test-implementation.md` | drop | — | 0.5.x test implementation plan; supersession by `dev/test-plan.md`. |
| `dev/archive/memex-fathomdb-readiness-2026-03-28.md` | drop | — | Memex readiness snapshot; client coupling. |
| `dev/archive/memex-gap-map.md` | archive | `docs/archive/0.5.x/memex-gap-map.md` | Original "what should engine own vs client" framing — useful historical context for engine-vs-app boundary. |
| `dev/archive/memex-question-cte-strategy-20260420.md` | drop | — | 0.5.3 implementation question, resolved. |
| `dev/archive/memex-question-expansionkind-20260420.md` | drop | — | 0.5.3 implementation question, resolved. |
| `dev/archive/memex-remodel-notes.md` | drop | — | Memex remodel exploration; client coupling. |
| `dev/archive/memex-reply-to-memex-remodel-notes.md` | drop | — | Memex feasibility reply; client coupling. |
| `dev/archive/memex-review-operational-store-design.md` | drop | — | Memex review; client coupling. |
| `dev/archive/older_memex_views.md` | drop | — | Already noted as older, in older_ prefix. |
| `dev/archive/openclaw-2026-04-18/comparison-fathomdb-0.5.1-2026-04-18.md` | drop | — | OpenClaw client comparison; client coupling. |
| `dev/archive/openclaw-2026-04-18/gap-analysis-fathomdb-0.5.1-2026-04-18.md` | drop | — | OpenClaw gap analysis; client coupling. |
| `dev/archive/openclaw-2026-04-18/readiness-fathomdb-0.5.1-2026-04-18.md` | drop | — | OpenClaw readiness; client coupling. |
| `dev/archive/openclaw-2026-04-18/requirements-fathomdb-0.5.1-2026-04-18.md` | fold | `docs/0.6.0/requirements.md` (input only) | Generic agentic-client requirements (ignoring OpenClaw-specific shape) — useful as one of multiple requirement-source inputs. Strip OpenClaw narrative. |
| `dev/archive/openclaw-2026-04-18/user-reported-gaps-fathomdb-0.5.1-2026-04-18.md` | drop | — | OpenClaw user gaps; client coupling. |
| `dev/archive/operator-retention-guide.md` | drop | — | "Retention is not automatic" operator guide; one-line decision lives elsewhere. |
| `dev/archive/path-to-production-2026-03-29.md` | drop | — | Resolved-findings summary; remediations are in code. |
| `dev/archive/pathway-to-basic-cypher-2026-03-28.md` | drop | — | Cypher pathway; explicit stop-doing. |
| `dev/archive/phase3-tasks.md` | drop | — | 0.1 phase tracker, shipped. |
| `dev/archive/phase4-5-6.md` | drop | — | 0.1 phase tracker, shipped. |
| `dev/archive/plan-0.5.1-execution.md` | drop | — | Execution plan, shipped. |
| `dev/archive/plan-0.5.2-execution.md` | drop | — | Execution plan, shipped. |
| `dev/archive/plan-close-remaining-production-risks.md` | drop | — | 0.5.x risk plan, shipped. |
| `dev/archive/plan-operational-payload-schema-validation.md` | drop | — | Implementation tracker, shipped. |
| `dev/archive/plan-operational-secondary-indexes.md` | drop | — | Implementation tracker, shipped. |
| `dev/archive/plan-structured-node-full-text-projections.md` | drop | — | Implementation plan, shipped. |
| `dev/archive/plan-typescript-sdk.md` | drop | — | TS SDK plan, shipped (per its own status banner). |
| `dev/archive/preliminary-solution-design.md` | archive | `docs/archive/0.5.x/preliminary-solution-design.md` | Original USER_NEEDS↔ARCHITECTURE bridge; historical context. |
| `dev/archive/production-readiness-checklist.md` | drop | — | Old prod-readiness gate; superseded twice. |
| `dev/archive/publish-steps.md` | drop | — | One-time publish setup. |
| `dev/archive/pyo3-0.28-upgrade-plan.md` | drop | — | 2026-04-09 upgrade plan, shipped. |
| `dev/archive/pyo3-log-gil-deadlock-evidence.md` | fold | `docs/0.6.0/learnings.md` (stop-doing source) | Concrete GIL/log deadlock evidence — feeds stop-doing entry on multi-instance + pyo3-log; valuable hazard documentation. |
| `dev/archive/remaining-work-fathomdb-support-memex.md` | drop | — | Memex-specific remaining-work; client coupling. |
| `dev/archive/research-comparison.md` | archive | `docs/archive/0.5.x/research-comparison.md` | Independent research validating SQLite-centric design choice; historical context. |
| `dev/archive/roadmap-0.4.5.md` | drop | — | Historical roadmap. |
| `dev/archive/roadmap-0.5.0.md` | drop | — | Historical roadmap. |
| `dev/archive/scope-0.4.1.md` | drop | — | Historical scope. |
| `dev/archive/scope-0.4.2.md` | drop | — | Historical scope. |
| `dev/archive/SDK-typescript-spec.md` | drop | — | 0.5.x TS SDK spec, shipped; rewrite re-decides interface in Phase 3e. |
| `dev/archive/setup-sqlite-vec-capability.md` | drop | — | Phase 3 setup, shipped. |
| `dev/archive/sqlite-plugin-report-2026-03-28.md` | archive | `docs/archive/0.5.x/sqlite-plugin-report-2026-03-28.md` | "fathomdb is not plugin-shaped" decision rationale; potential future revisit and feeds dep audit. |
| `dev/archive/thought.md` | archive | `docs/archive/0.5.x/thought.md` | Original architectural thinking sketch. |
| `dev/archive/TODO-response-cycle-feedback.md` | drop | — | Implementation tracker, shipped. |
| `dev/archive/TODO-t-038-to-t-041-dev.md` | drop | — | t-038–041 tracker, shipped. |
| `dev/archive/TODO-t-038-to-t-041.md` | drop | — | t-038–041 tracker, shipped. |

### `docs/concepts/`

| Source file | Verdict | Target | Notes |
|-------------|---------|--------|-------|
| `docs/concepts/architecture.md` | fold | `docs/0.6.0/architecture.md` | Single-writer/multi-reader/WAL overview is durable; binding inventory and 0.5.x specifics drop. |
| `docs/concepts/data-model.md` | fold | `docs/0.6.0/architecture.md` (data model section) | Nodes/edges/chunks/runs/steps/actions baseline; survives in current shape. |
| `docs/concepts/operational-store.md` | fold | `docs/0.6.0/design/engine.md` | HITL F8 (op-store cluster, settled): no dual-store; op-store concept folds into engine design as a same-file logical surface. |
| `docs/concepts/temporal-model.md` | fold | `docs/0.6.0/design/engine.md` (supersession) | created_at/superseded_at + active-vs-historical model is durable. |

### `docs/reference/`

| Source file | Verdict | Target | Notes |
|-------------|---------|--------|-------|
| `docs/reference/admin.md` | drop | — | 0.5.x AdminClient surface; rewrite re-derives from `interfaces/python.md` after Phase 3e. |
| `docs/reference/engine.md` | drop | — | 0.5.x Engine surface; rewrite re-derives. |
| `docs/reference/query.md` | drop | — | 0.5.x Query surface; rewrite re-derives. The "constrained safe subset" prose is preserved via `dev/notes/spec-supported-query-primitives-and-operators-2026-04-11.md` (folded). |
| `docs/reference/types.md` | drop | — | 0.5.x type surface; rewrite re-derives. |
| `docs/reference/write-builder.md` | drop | — | 0.5.x WriteRequestBuilder surface; rewrite re-derives. |

### Ambiguous (HITL — resolved 2026-04-25)

Critic-A (`architecture-inspector`) attacked the disposition table and flagged 14 findings. HITL resolutions applied above:

- **F1** `dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md` → drop. Invariant lifted into vector design via kept `project-vector-identity-invariant.md`.
- **F2** `dev/USER_NEEDS.md` → drop. Rewrite proposal supersedes.
- **F3** `dev/test-plan.md` → drop. Redo top-down from ≤5-verb API.
- **F5** `dev/notes/design-vector-projection-background-scheduler-2026-04-23.md` → fold-rewrite (not lift-as-is). Top-down from completion-barrier raw-req; Arc/async; client sees vec-not-yet-consistent.
- **F8 dbim-playbook** → kept folded; integrity-tool surface required in 0.6.0.
- **F8 typed-write** → kept folded; typed-at-engine-boundary is settled (no raw SQL ever).
- **F8 engine-lifecycle** → kept folded; lift "interpreter exit bounded with un-closed engines" reliability invariant; specific mechanism re-decided.
- **F8 op-store cluster (4 docs)** → all kept folded; no dual-store, op-store lives in same sqlite file alongside primary entities.
- **F12** `dev/notes/release-checklist.md` → kept fold (durable procedural sections lift into followups runbook).
- **F4** raw-req verb-name rephrase deferred until 0.6.0 architecture written; tracked in followups.
- **F7** Stop-doings #9/#11/#3 merged under "Defect deferral patterns" with three sub-bullets (citations preserved).
- **F10** Keep #14 telemetry-separation marked carry-forward + verify in 0.6.0 implementation.
- **F11** Keep #2 orchestrator pattern → marked **proven** (no provisional flag) per HITL.
- **F13** Stop atomic-pack rephrased to "atomic merges without explicit deletion phase".
- **F14** misconfig-hard-errors enumeration deferred to Phase 3b acceptance; enumeration must use deterministic parsing scripts (cargo-expand on error enums, grep extraction), **not LLM token output**.
- **F6** ADR Keep item retained as-is (no demotion).

Two open items:

- `dev/release-policy.md` — kept; verify against 0.6.0 freeze gates during Phase 4.
- `dev/archive/design-typed-write.md` Phase-2 ADR queue: PreparedWrite shape (types only — boundary-no-raw-SQL is settled).

## Raw requirement candidates

User-visible outcomes harvested from prior design notes. These feed Phase 3a; they are not yet AC ids.

### Observability

- Operator can attribute any visible delay to a specific lifecycle phase (started / slow / heartbeat / finished / failed) without parsing stderr. Source: `dev/design-response-cycle-feedback.md` (Core Correctness Rule). Category: observability.
- Operator can route fathomdb diagnostic events through their existing application logging (Python `logging`, Rust `tracing-subscriber`, or JSON-on-stderr for the bridge) without fathomdb writing log files of its own. Source: `dev/design-logging-and-tracing.md` Principle 2 + "What fathomdb does NOT do". Category: observability.
- Operator can read cumulative engine counters (queries, writes, write rows, errors by code, admin ops, cache hit/miss) on demand at any time without per-operation overhead. Source: `dev/design-note-telemetry-and-profiling.md` Level 0. Category: observability.
- Operator can opt into per-statement profiling (wall-clock, VM steps, full-scan steps, cache delta) without rebuilding the engine. Source: `dev/design-note-telemetry-and-profiling.md` Level 1. Category: observability.
- Operator receives SQLite-internal corruption / recovery / I/O events through the same channel as fathomdb's own diagnostics. Source: `dev/design-logging-and-tracing.md` Principle 5. Category: observability.
- Slow-statement detection surfaces statements exceeding a configurable threshold (default 100ms) or with non-zero `FULLSCAN_STEP`, and that signal contributes to the "slow" feedback transition. Source: `dev/design-note-telemetry-and-profiling.md` §"Slow statement detection". Category: observability.
- Stress / robustness test failures emit thread group, operation kind, last error, projection status for affected kind, and integrity counters — never just "stress failed". Source: `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md` §"Observability Requirements". Category: observability.
- Operator can distinguish "vector work pending" from "vector work failed" via projection status; semantic search timeout is interpretable by mode. Source: `dev/notes/design-projection-freshness-sli-harness-2026-04-23.md` §"Failure Interpretation". Category: observability.

### Performance

- Single-node+chunk write submit p95 ≤ 100 ms on the reference benchmark dataset. Source: `dev/production-acceptance-bar.md`. Category: perf.
- Text query p95 ≤ 150 ms on the seeded benchmark dataset. Source: `dev/production-acceptance-bar.md`. Category: perf.
- Vector query p95 ≤ 200 ms on the seeded benchmark dataset. Source: `dev/production-acceptance-bar.md`. Category: perf.
- `safe_export` completes in ≤ 500 ms on the seeded benchmark dataset. Source: `dev/production-acceptance-bar.md`. Category: perf.
- Canonical-read freshness after write acknowledgement p99 ≤ 50 ms. Source: `dev/notes/design-projection-freshness-sli-harness-2026-04-23.md` §Thresholds; `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md` §"Suggested Thresholds". Category: perf.
- FTS-search freshness after write acknowledgement p99 ≤ 50 ms. Source: same as above. Category: perf.
- Semantic-search freshness after explicit drain p99 ≤ 500 ms; background-mode design target ≤ 2 s. Source: same. Category: perf.
- Drain of 100 deterministic-embedder vectors completes in ≤ 2 s. Source: `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md` §"Suggested Thresholds". Category: perf.
- Mixed-retrieval stress workload keeps read p99 within max(10× baseline, 150 ms). Source: same. Category: perf.
- Reads must not serialize behind a single reader connection. Source: `dev/design-reader-connection-pool.md` (dropped doc, requirement preserved). Category: perf.

### Reliability

- Concurrent reads issued during admin DDL emit zero `SQLITE_SCHEMA` warnings. Source: `dev/notes/0.5.7-corrected-scope.md` T1 §4. Category: reliability.
- Engine close releases all OS resources (writer/vector/rebuild actors joined, exclusive lock released, FDs closed) and the host process exits within 5 s of `close()`. Source: `dev/notes/0.5.7-corrected-scope.md` T1 §3; memory `feedback_release_verification`. Category: reliability.
- A second engine on the same DB file is rejected with a clear `DatabaseLocked` while the first holds it, including while pending vector work exists. Source: `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md` Test Set 3. Category: reliability.
- Dropping an engine with pending vector projection work does not deadlock. Source: same. Category: reliability.
- `safe_export` includes committed WAL-backed state and never regresses to file-copy semantics. Source: `dev/production-acceptance-bar.md`. Category: reliability.
- Recovered databases preserve canonical rows, restore FTS usability, and (for vector-enabled DBs) preserve vector profile metadata and table capability. Source: `dev/production-acceptance-bar.md`. Category: reliability.
- `excise_source` preserves auditability and leaves projections consistent. Source: `dev/production-acceptance-bar.md`. Category: reliability.
- Canonical writes are never blocked by projection (FTS or vector) unavailability. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §13. Category: reliability.
- Misconfiguration (no embedder wired, kind not vector-indexed, dimension mismatch) hard-errors at the call boundary; never silent-degrades. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §12 + Anti-requirements. Category: reliability.
- Hybrid retrieval reports a `was_degraded` soft-fallback signal when a non-essential branch could not contribute. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §5. Category: reliability.
- `drain(timeout)` hook is available for tests and batch ingest, with bounded completion or explicit timeout. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §14. Category: reliability.
- Schema migrations are auto-applied on engine open (no DBA step) and report version + per-step duration on completion or failure. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §16; `dev/design-logging-and-tracing.md` Tier 1/2. Category: reliability.
- Provenance growth is bounded; the engine does not silently grow `provenance_events` without a retention story. Source: `dev/design-provenance-retention.md` (patch dropped, requirement preserved per disposition). Category: reliability.
- Operator can verify integrity and recover from corruption via a dedicated integrity-tool surface (3-class corruption model: physical / logical / semantic; 4-invariant recovery). Source: `dev/dbim-playbook.md` (folded); HITL F8. Category: reliability.
- Interpreter / host process exits within a bounded window even when engine instances are not explicitly `close()`d. Source: commit `b4fe850` (0.5.6 atexit hook for un-closed engines, Memex regression); HITL F8 lifecycle lift. Category: reliability.

### Security

- Engine never exposes a network listener or wire protocol; all access is in-process or local subprocess (no TLS/auth surface to misconfigure). Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17 + Anti-requirements ("no server protocol"); `dev/design-note-encryption-at-rest-in-motion.md` §2 "Current State". Category: security.
- Engine never downloads or hosts embedder model weights; embedder is supplied by the caller, so no implicit network fetch occurs at `Engine.open`. Source: `dev/notes/0.6.0-rewrite-proposal.md` § Anti-requirements ("Embedder model hosting / downloading / lifecycle"). Category: security.
- Agent/LLM-generated text queries cannot inject FTS5 control syntax; the safe grammar tokenises at parse time and never passes raw input to FTS5. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §3 + Architecture §"Query execution". Category: security.
- A second process attempting to open a DB file already held by another engine is rejected with a typed `DatabaseLocked` error rather than corrupting state, including while pending vector work exists. Source: `dev/notes/0.6.0-rewrite-proposal.md` POST check 8; `dev/notes/design-retrieval-robustness-performance-gates-2026-04-23.md` Test Set 3 (already cited under reliability — falsifiable as a security-relevant isolation property). Category: security.
- Operator can verify a safe-export artifact end-to-end via the SHA-256 manifest written alongside it. Source: `dev/design-note-encryption-at-rest-in-motion.md` §1 "Current State" + §3. Category: security.

### Operability

- Operator can recover from physical, logical, or semantic corruption via a dedicated `fathomdb doctor` CLI (check-integrity, regen-vectors, rebuild-missing-projections, rebuild-fts, excise-source, purge-logical-id, restore-logical-id, safe-export, trace-source) without writing application code. Source: `dev/notes/0.6.0-rewrite-proposal.md` § "Recovery tooling (CLI, not SDK)"; `dev/dbim-playbook.md` §3, §11. Category: operability.
- Recovery tooling is never reachable from the application runtime SDK — runtime callers cannot accidentally invoke `excise_source`, `purge_logical_id`, or `safe_export`. Source: `dev/notes/0.5.7-corrected-scope.md` D2; `dev/notes/0.6.0-rewrite-proposal.md` § "Recovery tooling". Category: operability.
- Operator can `trace --source-ref <id>` to enumerate every canonical row produced by a given run/step/action for blast-radius analysis before excision. Source: `dev/dbim-playbook.md` §6, §11. Category: operability.
- Operator can run `check-integrity` to aggregate `PRAGMA integrity_check`, `PRAGMA foreign_key_check`, projection-shape checks, missing-chunk/vector detection, and active-row uniqueness per `logical_id` in one invocation. Source: `dev/dbim-playbook.md` §10. Category: operability.
- Physical-recovery flow recovers canonical tables only (`nodes`, `edges`, `chunks`, plus operational entities) and rebuilds projections rather than trusting recovered FTS5 / sqlite-vec shadow tables. Source: `dev/dbim-playbook.md` §7. Category: operability.
- Engine ships as a single-file DB with no server and no network dependency; operator deploys one binary and one `.sqlite` path. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17. Category: operability.

### Upgrade / compatibility

- Schema migrations auto-apply on `Engine.open` with no DBA step, and the open call reports applied version + per-step duration. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §16; `dev/design-logging-and-tracing.md` Tier 1/2. Category: upgrade.
- Opening a 0.5.x-shaped database with a 0.6.0 engine hard-errors at POST naming the schema version seen, rather than silently attempting partial reads. Source: `dev/notes/0.6.0-rewrite-proposal.md` POST check 1 + § "What happens to 0.5.x" ("existing 0.5.x databases cannot be opened by 0.6.0"). Category: upgrade.
- Re-opening a store with a differently-dimensioned or differently-identified embedder hard-errors at POST naming both sides, rather than serving wrong vectors. Source: `dev/notes/0.6.0-rewrite-proposal.md` POST checks 3–4. Category: upgrade.
- Every post-v1 schema migration that adds a table or column must name a table/column it removes (or document why removal is impossible), preventing the v1..v25-style chain from re-accreting. Source: `dev/notes/0.6.0-rewrite-proposal.md` § "Future schema-migration policy". Category: upgrade.
- Engine has no deprecation shims and no `#[allow(deprecated)]` in crate roots; breaking changes are announced in changelog and removed in the same release. Source: `dev/notes/0.6.0-rewrite-proposal.md` § "What we got wrong" #5 + § "Architectural invariants". Category: upgrade.

### Supply chain

- Engine and sibling embedder packages share a tiny `fathomdb-embedder-api` crate with a stable trait set so a version-skewed install of `fathomdb` + `fathomdb-embedder` is detected at resolution time, not at runtime. Source: `dev/notes/0.6.0-rewrite-proposal.md` § "Version-skew policy (sibling packages)". Category: supply-chain.
- All three packages (`fathomdb`, `fathomdb-embedder`, `fathomdb-embedder-api`) are tagged and published together on every release even when content is unchanged, so cross-package version pins remain meaningful. Source: `dev/notes/0.6.0-rewrite-proposal.md` § "Version-skew policy". Category: supply-chain.
- Release tags have a single source of truth for version across Rust and Python (`Cargo.toml` workspace + `python/pyproject.toml`), enforced by `scripts/check-version-consistency.py` before any artifact publish. Source: `dev/release-policy.md` § "Version Source Of Truth" + § "Release Gates". Category: supply-chain.
- A release is published only after all artifacts (PyPI, crates.io, GitHub Release) succeed; partial publishes are forbidden. Source: `dev/release-policy.md` § "Manual Fallback" + § "Release Workflow Shape". Category: supply-chain.
- Engine validates that the `sqlite-vec` extension is available at `Engine.open` whenever the store contains any vector rows, rather than failing per-query under partial extension installs. Source: `dev/notes/0.6.0-rewrite-proposal.md` POST check 5. Category: supply-chain.
- Built wheel is installed from the registry into a fresh venv and run end-to-end as the release gate; `pip install -e` source-tree runs are not accepted as evidence. Source: memory `feedback_release_verification`; `dev/notes/0.6.0-rewrite-proposal.md` Tests §5. Category: supply-chain.

### Other

- Public Python admin surface is ≤ 5 verbs (`Engine.open`, `admin.configure`, `write`, `search`, `close`). Source: `dev/notes/0.5.7-corrected-scope.md` §"0.6.0 RFC"; `dev/notes/0.6.0-rewrite-proposal.md` §"Public API: five verbs". Category: other (API surface).
- Recovery tools (`rebuild_projections`, `restore_*`, `check_integrity`, `safe_export`, etc.) are CLI-only, not part of the application runtime SDK. Source: `dev/notes/0.5.7-corrected-scope.md` D2. Category: other (surface boundary).
- Engine ships as a single-file DB with no server and no network dependency. Source: `dev/notes/0.6.0-rewrite-proposal.md` Essentials §17. Category: other.

