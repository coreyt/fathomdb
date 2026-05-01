---
title: 0.6.0 Followups
date: 2026-04-24
target_release: 0.6.0
desc: Items deferred beyond 0.6.0; write-only during 0.6.0 doc phase
blast_radius: TBD
status: living
---

# Followups

**Read-discipline:** this file is **write-mostly** during 0.6.0. Working agents
append items but MUST NOT read this file unless explicitly told. Keeps working
context clean.

Item format:

```
## FU-NNN: <title>

**Origin:** <who/when/why>
**Target release:** 0.6.1 | 0.7.0 | TBD
**Notes:**
```

Seeded:
- **Upgrade path for 0.5.x users** — deferred from 0.6.0. Design in later release.

---

## FU-TWB1: CI lint gate for raw-SQL leaks

**Origin:** critic-3 TWB-1 (2026-04-27); enforces ADR-0.6.0-typed-write-boundary.
**Target release:** 0.6.0 (pre-implementation gate).
**Notes:** Add CI step that fails on any new public API across bindings exposing a string-typed SQL parameter. Initial pattern: `rg 'pub.*sql.*&str|pub.*query.*&str' crates/*/src` returns zero. Lint must also catch the same in PyO3 / napi-rs binding source. Lives in pre-merge check; fails build on first regression.

## FU-TWB2: Recovery verb set enumeration

**Origin:** critic-3 TWB-2 (2026-04-27); ADR-0.6.0-typed-write-boundary cites recovery as "typed CLI flags, not SQL."
**Target release:** 0.6.0 (Phase 3e interfaces/cli.md).
**Notes:** Enumerate every recovery / inspection verb the CLI must expose so users have no reason to ask for an SQL escape hatch. Examples: dump-schema, dump-row-counts, dump-profile, vacuum, integrity-check, export-op-store, repair-vector-index. Land in `interfaces/cli.md`.

## FU-JSON1: Operator-config site enumeration

**Origin:** critic-3 JSON-1 (2026-04-27); ADR-0.6.0-operator-config-json-only.
**Target release:** 0.6.0 (pre Phase 3e lock).
**Notes:** Enumerate every config-accepting surface and confirm JSON-only. Known sites: `load_vector_regeneration_config`; engine-open options; embedder config; op-store payload-schema-validation config; FTS opts. Add a row per site; flag any that accept a non-JSON format.

## FU-JSON2: Strict RFC-8259 documentation

**Origin:** critic-3 JSON-2 (2026-04-27).
**Target release:** 0.6.0 (Phase 3e interfaces).
**Notes:** Document in every operator-facing config doc that JSON is strict RFC-8259: no comments, no trailing commas, no JSON5/JSONC. Recommend a sidecar `<config>.md` for human-readable notes. Add a parser-level rejection test for JSONC-style comments.

## FU-OPS1: Op-store schema namespacing rule

**Origin:** critic-3 OPS-1 (2026-04-27); ADR-0.6.0-op-store-same-file.
**Target release:** 0.6.0 (Phase 3 design/engine.md).
**Notes:** `operational_*` prefix (folded-design convention: `operational_collections`, `operational_mutations`, `operational_state`). Document migration ordering: op-store tables created in the same schema-migration step as the primary tables they reference. Reject any op-store table without the prefix in CI.

## FU-OPS2: safe_export op-store coverage + redaction policy

**Origin:** critic-3 OPS-2 (2026-04-27); ADR-0.6.0-op-store-same-file.
**Target release:** **0.8.0 (HITL deferral 2026-04-27).**
**Notes:** `safe_export` of op-store rows + redaction policy deferred to 0.8.0. Rationale: 0.6.0 op-store payloads are operationally-bounded (connector health, cursors, counters, heartbeats) — high-sensitivity secrets are unlikely to land there in practice. Premature redaction policy adds surface without forcing function. Revisit when (a) op-store gains a use case with operator-supplied secrets, or (b) safe_export becomes a release-blocking feature for an external client. Until then, `safe_export` may emit op-store rows verbatim or omit them entirely; specific behavior decided at implementation time, not pinned by ADR.

## FU-OPS4: Op-store transaction boundary detail

**Origin:** critic-3 OPS-4 (2026-04-27); ADR-0.6.0-op-store-same-file.
**Target release:** 0.6.0 (Phase 3 design/engine.md).
**Notes:** Document the exact transactional API shape for the "primary entity write + step row + op-store row" tuple. Land in `design/engine.md` writer section. The invariant (atomic commit on the writer thread) is settled by the ADR; only the API shape is open.

## FU-EMB3: Per-platform wheel-size CI gate

**Origin:** critic-3 EMB-3 (2026-04-27); ADR-0.6.0-default-embedder.
**Target release:** 0.6.0 (CI matrix).
**Notes:** CI fails if the published wheel grows by more than 20 MB between releases on any tier-1 platform (linux x86_64, linux aarch64, darwin universal, windows x86_64). Threshold prevents silent dep bloat from candle / tokenizers / hf-hub upgrades. Implementation: store wheel size per platform in a small JSON manifest under `dist-meta/`; CI compares.

## FU-EMB5: hf-hub replacement design

**Origin:** critic-3 EMB-5 (2026-04-27); ADR-0.6.0-default-embedder.
**Target release:** 0.6.0 (Phase 3 design/embedder.md).
**Notes:** `hf-hub` carries Tokio + reqwest. Design a replacement model-resolver that reuses an existing HTTP client (rusqlite has none; ureq is candidate) and a flat on-disk cache layout. Cache layout per ADR-0.6.0-operator-config-json-only X-3: HF cache files are internal artifacts, not user-facing config — exempt from JSON-only.

## FU-EMB7: Structural lint for vector identity invariant

**Origin:** critic on M-4 (2026-04-27); ADR-0.6.0-vector-identity-embedder-owned.
**Target release:** 0.6.0 (CI gate).
**Notes:** Replace the grep sketch with a typed AST / typegraph check: "no struct reachable from `VectorConfig` references `EmbedderIdentity` or any of its fields by type." Concrete crate path is `fathomdb-core::config::*`. Implementation candidates: a unit test over the type graph, a `#[cfg(test)]` `static_assertions` set, or a clippy lint. Pick whichever is simplest at implementation time.

## FU-EMB8: Intentional embedder identity-change workflow

**Origin:** over-design audit OD-15 (2026-04-30); ADR-0.6.0-vector-identity-embedder-owned.
**Target release:** 0.8.0.
**Notes:** 0.6.0 fails closed on `EmbedderIdentityMismatch` and ships no `accept_identity_change` bypass. Revisit in 0.8.0 with a concrete operator workflow for intentional embedder swaps, recorded-identity update timing, and recovery / regeneration boundaries. Primary draft home: `docs/0.8.0/adr/ADR-0.8.0-embedder-identity-change-workflow.md`.

## FU-ASYNC5: TS cancellation semantics

**Origin:** critic-3 ASYNC-5 (2026-04-27); ADR-0.6.0-async-surface.
**Target release:** TBD (0.6.x or 0.7).
**Notes:** TS `Promise<...>` returns are not cancellable in 0.6.0 initial. Design: optional `AbortSignal` parameter on each TS verb; signal cancellation surfaces as a typed `EngineError::Cancelled`. Open question: whether cancellation aborts the writer-thread submission or only the napi waiter. Defer until first user request.

## FU-WIRE15: Subprocess bridge wire format

**Origin:** Phase 2 #15 deferral (HITL 2026-04-27).
**Target release:** 0.8.0.
**Notes:** Per ADR-0.6.0-subprocess-bridge-deferral, no subprocess bridge in 0.6.0; revisit in 0.8.0 (skip 0.7.0). If a forcing function emerges before 0.8.0 (non-PyO3 Python flavor, process-isolation requirement for embedders), the ADR is re-opened. Default-on-revisit format: JSON over stdio with versioned envelope `{ "v": 1, ... }` unless the consumer's needs argue otherwise.

## FU-RET17: Composable middleware retrieval pipeline

**Origin:** Phase 2 #17 deferral (HITL 2026-04-27).
**Target release:** 0.8.0.
**Notes:** Per ADR-0.6.0-retrieval-pipeline-shape, 0.6.0 ships fixed stages with per-stage config. Revisit composable middleware pipeline (trait-object stages, user-spliced stages) in 0.8.0 with concrete user needs. Forcing function: a real retrieval requirement that fixed-stage config cannot express. Until then, fixed stages remain.

## FU-VEC13-CORRUPTION: Single-file corruption-recovery posture — RESOLVED

**Origin:** Phase 2 #13 critic [vec-loc-02] (2026-04-27); ADR-0.6.0-vector-index-location.
**Target release:** 0.6.0.
**Status:** RESOLVED 2026-04-29 by ADR-0.6.0-corruption-open-behavior (#29). Engine.open behavior settled = refuse-fail-closed with structured `EngineOpenError::Corruption`; recovery ownership = `fathomdb recover` CLI exclusively (consistent with REQ-037, REQ-054). Detection mechanism (which checks run when) delegated to `design/recovery.md` per ADR §4 with anti-regression clause.

## FU-PW19-BATCH-SEMANTICS: Write batch transactional semantics

**Origin:** Phase 2 #19 critic [pw-01] (2026-04-27); ADR-0.6.0-prepared-write-shape.
**Target release:** 0.6.0 (Phase 3 design/engine.md).
**Notes:** ADR-0.6.0-prepared-write-shape commits to `&[PreparedWrite]` shape but defers transactional semantics: is the slice one transaction, N transactions, or per-variant-grouped? Decide in design/engine.md; promote to its own ADR if the answer is non-mechanical. Settling this also unlocks the per-variant-validation-error-with-batch-index question deferred in [pw-02].

## FU-PW19-BINDING-EXHAUSTIVENESS: Non-exhaustive enum across bindings

**Origin:** Phase 2 #19 critic [pw-04] (2026-04-27); ADR-0.6.0-prepared-write-shape.
**Target release:** 0.6.0 (interfaces/python.md + interfaces/typescript.md).
**Notes:** Rust `#[non_exhaustive]` does not protect Python `isinstance` chains or TS discriminated-union switches. Decide per-binding posture: default branch required by lint? runtime check on unknown variant? document the variant set as "stable within minor"? Resolve in the binding interface docs; promote to ADR if cross-binding posture diverges.

## FU-PY20-STUB-GENERATION: Python type-stub generation method

**Origin:** Phase 2 #20 critic [py-02] (2026-04-27); ADR-0.6.0-python-api-shape.
**Target release:** 0.6.0 (Phase 5 implementation).
**Notes:** ADR commits to `.pyi` shipping in the wheel. Generation method (hand-written, `pyo3-stub-gen`, `mypy stubgen`, custom) chosen at implementation time. Keep CI gate (`mypy --strict` against the stubs) regardless of generation choice.

## FU-DEP23-WITHIN-MINOR-REUSE: Within-0.6.x removed-name reuse rule

**Origin:** Phase 2 #23 critic [dep-01] (2026-04-27); ADR-0.6.0-deprecation-policy-0-5-names.
**Target release:** 0.6.0 (release-policy.md) or 0.6.x as needed.
**Notes:** ADR explicitly punts on "may a name removed in 0.6.x be reused with new meaning in a later 0.6.x release?" Resolve in release-policy.md. Default until decided: do not reuse — drop is forever within a minor series. If a real need surfaces, ADR-0.6.x-name-reuse settles it.

## FU-LOWS-2026-04-27: Lite-batch ADR low-severity findings

**Origin:** Critic on lite-batch ADRs (2026-04-27). Logged-not-applied per low-severity policy.
**Target release:** N/A (cleanup at next ADR amendment).
**Notes:**
- [tier1-04] Drop / move "this dev box is Jetson" footnote.
- [tier1-05] Name the perf-gate "reference target" (likely `x86_64-unknown-linux-gnu`) or strike the example.
- [vec-loc-04] Pin `vec0` shadow-table naming as either private-impl or documented convention; not both.
- [pw-05] Cite `AdminSchemaWrite` source or mark provisional pending design/engine.md.
- [py-04] Make snake_case-fields commitment explicit alongside snake_case-methods.
- [py-05] Name the structural enforcement of "asyncio threads never run embedders" or strike the bullet.
- [dep-03] Reword "0.5.x callers cannot run on 0.6.0 anyway" — DB-freshness ≠ API-freshness.
- [X-03] Optionally call out manylinux_2_28 baseline in 0.6.0 release notes.

## FU-REQ-LOWS-2026-04-27: Phase 3a critic low-severity findings

**Origin:** Critic on requirements.md draft (2026-04-27). Logged-not-applied per low-severity policy.
**Target release:** N/A (cleanup at next requirements amendment).
**Notes:**
- [REQ-051 inverse] Behavior on vector-free DB when `sqlite-vec` is missing — REQ leaves ambiguous; ADR allows. Tighten if a real user hits it.
- [REQ-018 citation] Re-cite to retrieval-latency-gates concurrent-read implication rather than dropped doc once acceptance.md has the AC.
- [REQ-035 citation] `dev/design-note-encryption-at-rest-in-motion.md` is a deferred-doc; primary citation now on `production-acceptance-bar.md` (kept). Acceptable.
- [REQ-031 retention specifics] Retention configuration shape (knob? cap? TTL?) deferred to design/engine.md.
- [REQ-054 vs REQ-037] Pair retained (SDK-unreachability + CLI-completeness are distinct claims). Revisit only if a binding reviewer finds the pair confusing.

## FU-REQ-010-TEXT-LATENCY: Text query latency ADR

**Origin:** Phase 3a critic [REQ-010] (2026-04-27).
**Target release:** 0.6.0 (Phase 3b acceptance.md or its own ADR).
**Status:** RESOLVED 2026-04-27 — promoted to ADR-0.6.0-text-query-latency-gates (decision-index #26).
**Notes:** Text-query latency target has no accepted ADR; harvest carried `p95 ≤ 150 ms` from `production-acceptance-bar.md` but without workload definition or ADR backing. Decide before acceptance.md lock: either promote to `ADR-0.6.0-text-query-latency-gates` (paralleling retrieval-latency-gates) or commit to text-on-FTS being implicitly bounded by the canonical-read freshness REQ-013 + a generous fallback gate in acceptance.md. Don't lock requirements with a vague REQ.

## FU-TXT-LAT-TIGHTEN: Tighten text-query p99 from 150 ms → 100 ms

**Origin:** ADR-0.6.0-text-query-latency-gates critic [high-1] (2026-04-27).
**Target release:** 0.6.0 (post-baseline) or 0.6.x.
**Notes:** ADR set p99 ≤ 150 ms with explicit headroom for shared-runner scheduler jitter. Tighten to 100 ms once a measured baseline on the pinned tier-1 runner exists and shows steady-state p99 well below 100 ms. Forcing function: real measurement, not user complaint. Update ADR via amendment.

## FU-PERF-ADR-ALIGN: Align retrieval + text-query perf ADR sample-count + tiered-gate language

**Origin:** ADR-0.6.0-text-query-latency-gates critic lows (2026-04-27).
**Target release:** 0.6.0 (Phase 3) or 0.6.x amendment.
**Status:** PARTIALLY RESOLVED 2026-04-27 — measurement-protocol backfill applied to ADR-0.6.0-retrieval-latency-gates (concurrency, warmup, sample count, in-process boundary, scope, fixture-share clarification, reference-target citation). Tiered-gate / budget-allocation framing (parse ≤ Xms, ANN ≤ Yms, fetch ≤ Zms) for regression triage NOT applied — defer to first real regression incident, since pre-emptive budget allocation is speculative without measurement.
**Notes:** Text-query ADR specifies sample-count `≥ 1000`, warmup protocol, query-frequency band, in-process boundary. ADR-0.6.0-retrieval-latency-gates has the same omissions unaddressed. Amendment pass: backfill sample-count + warmup-protocol + boundary-clarification language into retrieval-latency-gates so both perf ADRs share a measurement protocol. Also: consider tiered-gate / budget-allocation framing (parse ≤ Xms, MATCH ≤ Yms, fetch ≤ Zms) for faster regression triage on either ADR — currently both gate only the end-to-end number.

## FU-FTS5-TOKENIZER: FTS5 default tokenizer decision

**Origin:** ADR-0.6.0-text-query-latency-gates scope-creep removal (2026-04-27).
**Target release:** 0.6.0 (Phase 3d design/retrieval.md).
**Notes:** Text-query ADR does not pin the FTS5 tokenizer (was scope creep). `design/retrieval.md` owes the tokenizer decision: unicode61 (default), porter (stemming), trigram (substring/CJK), icu (locale-aware), or custom. Promote to its own ADR if non-trivial; otherwise document inline. Latency gate re-validated on tokenizer change.

## FU-FTS5-SNIPPET-HIGHLIGHT: Snippet/highlight latency posture

**Origin:** ADR-0.6.0-text-query-latency-gates boundary clarification (2026-04-27).
**Target release:** TBD (0.6.x or 0.7).
**Notes:** ADR explicitly excludes FTS5 `snippet()` / `highlight()` from the gated boundary. If snippet/highlight becomes a user-facing default in `search` results, a separate "snippet/highlight latency" ADR or amendment is required.

## FU-AC-LOWS-2026-04-27: Phase 3b critic low-severity findings

**Origin:** Critic on acceptance.md draft (2026-04-27). Logged-not-applied per low-severity policy.
**Target release:** N/A (cleanup at next acceptance amendment).
**Notes:**
- [AC-001 cross-coupling] Slow-phase moved to AC-008; AC-001 now scopes to non-slow phases — minor coupling remains acceptable.
- [AC-002 allow-list completeness] Allow-list mentions WAL/SHM/.journal — confirm SQLite-owned set is exhaustive when design/engine.md PRAGMA section lands.
- [AC-024a "1 s" rejection bound] Magic number; cite source or move to test-plan.md as a tolerance parameter.
- [AC-027c/d threshold] Kendall tau tolerance owned by test-plan.md; expect tightening once a measured baseline exists.
- [AC-030c vs AC-048] Boundary distinction (call-boundary vs reopen-boundary) made explicit in both ACs; revisit if a binding reviewer finds the pair confusing.
- [AC-037 no-embedder edge] AC scoped to "with embedder configured"; the no-embedder-open path (if exists per default-embedder ADR) needs a separate AC if it's a supported configuration.
- [AC-052 release count] "All releases or last 5, whichever is fewer" — minor.
- [AC-056 artifact format] release-checklist artifact format pinned to "script source"; if YAML is later chosen, update.
- [AC-059a interleaved-writes case] Already added; closes critic finding.
- [§Performance preamble] All performance ACs explicitly defer protocol parameters (warmup, sample count, runner pinning) to test-plan.md to avoid inventing numbers absent from ADRs. Documented inline.

## FU-AC-DDL-ENUMERATION: Enumerate DDL operations under SQLITE_SCHEMA test

**Origin:** Phase 3b critic [AC-021] (2026-04-27).
**Target release:** 0.6.0 (test-plan.md fixture).
**Notes:** AC-021 cites a documented DDL set under test (`admin.configure_kind` add + remove cycle, schema-projection rebuild). Final enumeration owed by test-plan.md fixture spec; if the DDL surface expands, AC-021 fixture must be regenerated.

## FU-AC-CORRUPTION-HARNESS: Corruption / power-cut / OS-crash harness specs

**Origin:** Phase 3b critic [AC-006, AC-027a, AC-034a, AC-034c] (2026-04-27).
**Target release:** 0.6.0 (test-plan.md harness section).
**Notes:** Multiple ACs depend on documented harnesses for corruption injection, power-cut simulation, and OS-crash simulation. test-plan.md owes:
- Corruption-injection tool path (e.g. `tools/corrupt-page.py`) and the corrupted-state shape
- Power-cut harness (kill -9 mid-commit timing strategy + reopen-and-check loop) + trial count
- OS-crash harness (VM image + trigger mechanism, e.g. `echo c > /proc/sysrq-trigger` inside KVM with sync barrier preserved) + trial count
- Sentinel pattern protocol for AC-044 (16-byte random per-test sentinel)
- Fixture corpora at scale (1M rows, 1GB DB)

## FU-AC-PROTOCOL-BACKFILL: Promote per-AC protocol parameters into ADRs vs test-plan.md

**Origin:** Phase 3b critic [§Performance, AC-011a/b sample window, AC-012/13 sample count, AC-019 stress workload, AC-029 tolerance, AC-032b tolerance, AC-033 retention bound + tolerance, AC-035 worst-of-N] (2026-04-27).
**Target release:** 0.6.0 (test-plan.md or ADR amendments).
**Status:** RESOLVED 2026-04-27 (HITL). Resolution:
- acceptance.md OWNS every numerical threshold via the new `## Parameter table` (markdown, P-NNN ids).
- test-plan.md is the *measurer*, not the *threshold owner* — owns fixture corpora + harness scripts only.
- Two parameters promoted to ADR (concise): AC-027d → ADR-0.6.0-recovery-rank-correlation (#27); AC-033 → ADR-0.6.0-provenance-retention (#28).
- All other 12+ parameters self-owned by acceptance.md; changing them follows the same critic + HITL cycle as any acceptance amendment.
- Traceability matrix appended to acceptance.md mapping AC → REQ → P-IDs → authoritative source.
**Notes:** Acceptance.md draft deferred all measurement-protocol parameters to test-plan.md to avoid inventing numbers absent from ADRs. Decide before lock per parameter: (a) lift to ADR amendment (binding numerical commitment), or (b) leave in test-plan.md (binding test-protocol commitment, ADR silent on protocol). Default: leave in test-plan.md unless a reviewer wants the number ADR-grade.

## FU-ARCH-LOWS-2026-04-27: Phase 3c critic low-severity findings

**Origin:** Critic on architecture.md draft (2026-04-27). Logged-not-applied per low-severity policy.
**Target release:** N/A (cleanup at next architecture amendment).
**Notes:**
- [§9 reader-pool ADR candidate] Reader-pool sizing decisions made in design/engine.md without an ADR. Promote to its own ADR if a forcing function (concurrent-read regression on a real workload) lands.
- [§10 deltas — 5-verb size] Could note "(down from ~25+ verbs in 0.5.x)" if accurate; currently understated.
- [crate-topology vs python build] Memory `feedback_python_native_build` says `pip install -e python/` is canonical. Architecture.md notes the directory layout (`python/`, `ts/`) is unchanged from 0.5.x; only the cdylib crate name changes. Consistent.
- [errors module home] design/errors.md split out per critic; revisit if it grows trivial — could fold back into design/bindings.md error-mapping section.
- [§7 meta-ADR framing] Subsection added per critic; future ADRs that decide policy-without-runtime-footprint should land here too.

## FU-FATHOMDB-QUERY-DISPOSITION: fathomdb-query crate fold-or-keep — RESOLVED

**Origin:** Phase 3c architecture critic (2026-04-27); ADR-0.6.0-crate-topology amendment.
**Target release:** 0.6.0 (Phase 3c lock).
**Status:** RESOLVED 2026-04-29 (HITL). Decision: **kept separate**.
**Resolution:** Investigation surfaced documented invariant at
`crates/fathomdb-engine/src/embedder/mod.rs:1-7`: the `QueryEmbedder` trait
lives in `fathomdb-engine` (not `fathomdb-query`) so that `fathomdb-query`
stays a pure AST-to-plan compiler with no `dyn` trait objects and no
runtime state. Folding would lose: (a) compile-vs-runtime split,
(b) hermetic insta snapshot tests of compiled SQL without engine fs/lock/db
deps, (c) reverse-dependency hygiene preventing query crate from pulling
in storage/threads/embedder, (d) the no-dyn / no-runtime-state invariant
explicitly engineered into the placement of `QueryEmbedder`. Architecture.md
§ 1 + § 9 updated; ADR-crate-topology unchanged (deferred-to-design clause
satisfied).

## FU-RECOVERY-CORRUPTION-DETECTION: corruption detection + Engine.open behavior — RESOLVED

**Origin:** Phase 3c architecture § 9 (cross-reference to FU-VEC13-CORRUPTION).
**Target release:** 0.6.0.
**Status:** RESOLVED 2026-04-29 by ADR-0.6.0-corruption-open-behavior (#29). Engine.open behavior committed; detection cadence delegated to `design/recovery.md` with anti-regression clause (reducing the always-on detection set is a behavior change requiring ADR amendment). Consolidated with FU-VEC13-CORRUPTION (also resolved by #29).
