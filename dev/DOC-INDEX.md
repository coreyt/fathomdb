# DOC-INDEX — FathomDB documentation map (agentic cold-start)

> **Purpose.** The single cold-start map an agent reads to find the right doc
> *without scanning the tree*. One row per doc: **path → purpose → owning**
> **slice/AC → last-touched**. Created at **Slice 0** of 0.8.0 (X3 cross-cutting
> requirement).
>
> **THE RULE (binds every slice — stated here, on `runs/STATUS-0.8.0.md`, and in
> `0.8.0-plan.md`):** **every slice updates `dev/DOC-INDEX.md` in its closing docs
> commit.** When a slice adds, renames, or materially changes a doc, it adds/edits
> that doc's row here (path · purpose · owning slice/AC · last-touched) in the same
> commit that closes the slice (mirrors the §12.4 plan-as-state-machine discipline,
> applied to docs). A stale or missing row is an X3 gap; Slice 40 **gate m** fails
> the release if `dev/DOC-INDEX.md` is not the accurate map of the shipped surface.
>
> **THE ≤120-CHAR RULE (added DOC-HYGIENE-1 T2/5, 2026-07-24):** this file is a
> **thin map**, target ≤ 12 KB. Every row's Purpose cell is **one declarative
> clause, ≤ 120 characters** — what the doc *is*, not its slice history. Long-form
> per-doc notes (slice-by-slice narrative, HITL status, cross-links) move to the
> matching `dev/doc-index/<area>.md` file, linked at the top of each section below.
> **Keep it thin: when you're tempted to write more than one clause here, write it
> in the area's detail file instead and keep this row's Purpose cell short.**

`last-touched` = date of the last git commit that modified the file (best-effort;
refresh in the closing commit when you touch a doc).

---

## `dev/doc-index/` — long-form per-doc detail (linked from each section below)

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/doc-index/dev.md` | Long-form detail for the `dev/` section below | DOC-HYGIENE-1 T2/5 | 2026-07-24 |
| `dev/doc-index/design.md` | Long-form detail for the `dev/design/` section below | DOC-HYGIENE-1 T2/5 | 2026-07-24 |
| `dev/doc-index/adr.md` | Long-form detail for the `dev/adr/` section below | DOC-HYGIENE-1 T2/5 | 2026-07-24 |
| `dev/doc-index/plans.md` | Long-form detail for the `dev/plans/` section below | DOC-HYGIENE-1 T2/5 | 2026-07-24 |
| `dev/doc-index/docs.md` | Long-form detail for the `docs/` section below | DOC-HYGIENE-1 T2/5 | 2026-07-24 |
| `dev/doc-index/corpus.md` | Long-form detail for the Corpus/eval section below | DOC-HYGIENE-1 T2/5 | 2026-07-24 |

## `dev/` — engineering docs (the build-time source of truth)

> Long-form per-doc notes: [`dev/doc-index/dev.md`](doc-index/dev.md).

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/README.md` | Entry map for the engineering docs tree | — | 2026-05-02 |
| `dev/needs.md` | Product/consumer needs driving requirements | — | 2026-05-28 |
| `dev/requirements.md` | Numbered requirements (REQ-*); REQ-053 = governed SDK surface (allowlist + parity + recovery-denylist + typed boundary) | 25 amended REQ-053 (Q3) | 2026-06-04 |
| `dev/acceptance.md` | Acceptance criteria (AC-*); AC-057a five-verb cap superseded by AC-074 (governed surface); AC-074 Rust-facade... | 25 (AC-057a→AC-074); 27 fills AC-074 Rust clause; 27 fix-1 method-level; 40/GA-2 mints AC-075/076 | 2026-06-08 |
| `dev/interfaces/rust.md` | Rust public interface (owner of the Rust-visible symbol spelling + the governed Rust-facade surface contract... | 27 (governed-surface contract); 27 fix-1 (operator feature gate); 0.8.20 15b fix-1 (node-validity write fields); 0.8.20 15b fix-2 (search validity + `ReadView` on search); 0.8.20 20b (`DenseReadiness` + `ProjectionVector::dense_readiness`); ground-truth engine type names | 2026-07-25 |
| `dev/interfaces/python.md` | Python public interface (owner of Python-visible symbol spelling + attribute casing); 0.8.20 15b fix-1 adds the node... | 0.8.20 15b fix-1 (node-validity write keys); 0.8.20 15b fix-2 (`view=` on search); 0.8.20 20b (`vector_dense_readiness`); parity owned by `design/bindings.md` | 2026-07-25 |
| `dev/interfaces/typescript.md` | TypeScript public interface (owner of TS-visible symbol spelling + export shape); 0.8.20 15b fix-1 adds the node... | 0.8.20 15b fix-1 (node-validity write keys); 0.8.20 15b fix-2 (`view` on search); 0.8.20 20b (`vectorDenseReadiness` + `DenseReadiness`); parity owned by `design/bindings.md` | 2026-07-25 |
| `dev/interfaces/cli.md` | CLI public interface (concrete flag spelling, root paths, exit-code classes, `--json` wrapping for the two-root... | 34 (dump-mutations); owned-by ADR-0.6.0-cli-scope | 2026-06-06 |
| `dev/architecture.md` | System architecture (engine, projections, reader pool, surface) | 5/10/15/30 update read-path + receipt surface (30 adds the governed `read.*` reader-pool dispatch); 31 re-scopes G0 active identity to `logical_id` alone | 2026-06-05 |
| `dev/test-plan.md` | Test strategy + tiers (incl. functional-harness tier X1 + the Slice 10 G9/G10/G12-recency tier) | 5 adds functional tier; 10 adds RRF/filter/recency tier | 2026-06-03 |
| `dev/traceability.md` | REQ ↔ AC ↔ test trace matrix | 25 re-points REQ-053↔new AC; 30 adds read ACs | 2026-05-28 |
| `dev/security-review.md` | Security review (SR-*) | — (SR-005/SR-011 candidate reserved-gap) | 2026-05-02 |
| `dev/learnings.md` | Cross-phase engineering learnings | per-slice as discovered | 2026-05-31 |
| `dev/experiments-ledger.md` | Distilled results of record for EVERY FathomDB experiment | ledger-prune (`scripts/repo-prune/prompts/prune-docs.md`) | 2026-06-26 |
| `scripts/repo-prune/README.md` | repo-prune mini-project | repo-prune (tooling) | 2026-06-26 |
| `dev/tools/onnx/README.md` · `dev/tools/onnx/export_bge_small_onnx.py` | ONNX embedder export tooling (0.8.16 Slice 10) | 0.8.16 Slice 10 (tooling) | 2026-07-08 |
| `dev/notes/0.8.0-fts5-tokenizer-latency-experiment.md` | B2 FTS5 tokenizer latency experiment report | Slice 6 (B2) | 2026-06-07 |
| `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md` | Recall-eval framework assessment | IR-eval (IR-1/IR-2 input) | 2026-06-07 |
| `dev/plans/0.8.0-GA-and-IR-eval-roadmap.md` | Sequenced roadmap (+ ASCII visual map) | orchestrator (live) | 2026-06-07 |
| `dev/plans/prompts/0.8.0-MASTER-ORCHESTRATOR-HANDOFF.md` | Master orchestrator hand-off | orchestrator (entry point) | 2026-06-07 |
| `dev/plans/prompts/0.8.0-SESSION-SCAFFOLD-GENERATOR.md` · `dev/plans/prompts/scaffolds/` | Session scaffold generator | orchestrator (bootstrap) | 2026-06-07 |
| `dev/plans/prompts/scaffolds/README.md` · `scaffolds/<order>-<id>.md` (1–9) | Session scaffolds (generated) | orchestrator (bootstrap); generated by `0.8.0-SESSION-SCAFFOLD-GENERATOR.md` | 2026-06-07 |
| `dev/plans/prompts/0.8.x-IR-1-phase1-measure-consensus.md` | IR-1 Phase 1 (runnable now) | IR-eval (now) | 2026-06-07 |
| `dev/plans/prompts/0.8.x-IR-1-recall-measure.md` · `0.8.x-IR-2-recall-gate.md` | IR-eval IR-1 Phases 2–4 (DEFERRED) + IR-2 | IR-eval (post-0.8.0-GA / 0.8.1) | 2026-06-07 |
| `dev/memex-note-on-0.6.0.md` | Memex consumer note on 0.6.0 | — | 2026-05-21 |
| `dev/DOC-INDEX.md` | This file | 0 creates; every slice updates | 2026-06-02 |

## `dev/design/` — design notes + ADR-adjacent specs

> Long-form per-doc notes: [`dev/doc-index/design.md`](doc-index/design.md).

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/design/README.md` | Design-notes index | — | (tree) |
| `dev/design/gpu-eval-activities-policy.md` | Policy — repo MUST use the 3090s for eval/embed activities when there is room | 0.8.14 Slice 20 (eu7 policy) | 2026-07-05 |
| `dev/design/free-threaded-python-value-lift-and-experiments.md` | Free-threaded Python (PEP 703) for FathomDB — value, lift, experiment plan | 0.8.15 ladder (pyo3 dep @ 0.8.8) | 2026-06-27 |
| `dev/design/0.8.18-slice-20-publish-pipeline.md` | 0.8.18 Slice 20 — #11-full full publish pipeline (implementation design) | 0.8.18 Slice 20 | 2026-07-09 |
| `dev/adr/ADR-0.8.18-full-publish-pipeline.md` | #11-full full publish pipeline | 0.8.18 Slice 0 gates; Slice 20 implements | 2026-07-09 |
| `dev/design/0.8.16-slice-0-f9-onnx-design.md` | 0.8.16 Slice-0 design package — F9 importance/confidence ranking + cross-vendor ONNX embedder | 0.8.16 Slice 0 | 2026-07-08 |
| `dev/design/0.8.18-slice-5-vector-equivalence-probe.md` | 0.8.18 Slice 5 — #5 vector-equivalence probe (SHIPPED surface) | 0.8.18 Slice 5 | 2026-07-09 |
| `dev/design/0.8.2-m1-multihop-harness.md` | 0.8.2 / M1 multi-hop answer-accuracy harness — design + FROZEN pre-registration (AMENDED 2026-06-16; re-frozen... | 0.8.2 Slice 0-rev2 | 2026-06-19 |
| `dev/design/0.8.3-mem0-parity.md` | 0.8.3 Slice-0 design + FROZEN pre-registration (Mem0-parity resolution) | 0.8.3 Slice 0 | 2026-06-21 |
| `dev/design/0.8.5-ce-rerank-slice-design.md` · `dev/plans/0.8.5-ce-rerank-alpha-expose-slice.md` | 0.8.5 (EXP-0) — expose tuned CE-rerank α / pool_n / ce_score | 0.8.5 (EXP-0) | 2026-06-25 |
| `dev/design/0.8.12-coverage-probe-and-value-test.md` | 0.8.12 Slice-0 pre-registration | 0.8.12 Slice 0 authors; Slice 5/20 execute | 2026-07-01 |
| `dev/design/0.8.8-explain-and-telemetry-adr.md` · `dev/plans/runs/0.8.8-explanation-fieldset-ratification.md` | 0.8.8 EXP-OBS — `Explanation` payload + telemetry/gold schema ADR | 0.8.8 Slice 0/5 | 2026-06-27 |
| `dev/design/0.8.8-telemetry-design.md` | 0.8.8 Slice 15 — telemetry capture mechanism | 0.8.8 Slice 15/20 | 2026-06-28 |
| `dev/design/slice-40-gate-restructure-and-ga.md` | Slice 40 / GA-2 gate-restructure + GA verification design memo | 40/GA-2 | 2026-06-08 |
| `dev/design/slice-0-adr-plan.md` | Slice 0 design memo — one-paragraph per ADR: BYO-LLM extraction protocol, IR-measure/eval design (R0+R2), G11 graph... | Slice 0 | 2026-06-12 |
| `dev/design/ir-recall-measure.md` | IR/agentic evidence-recall MEASURE (definition + methodology) | IR-eval (IR-1 Phase 1) | 2026-06-08 |
| `dev/design/orchestration.md` | Cross-release runbook | binds every slice | 2026-06-26 |
| `dev/agent-harness-bootstrap-prompt.md` | Method on-ramp (portable distillation) | method on-ramp (cross-release) | 2026-06-26 |
| `scripts/preflight.sh` | Orchestrator preflight gate | binds every spawn | 2026-06-26 |
| `scripts/check-c1-conformance.sh` · `scripts/c1-conformance-pin.json` | RUBRIC-H7 `can-i-deploy` gate (R-20-H7) — pins the ratified `OPP-12-C1-converged-contract.md` bytes and asserts its 26 CHECKABLE clauses against as-built code; the pin carries the full reviewable clause registry (26 CHECKABLE / 12 CROSS-REPO / 7 PROSE) | 0.8.20 Slice 30 (R-20-H7); publish precondition | 2026-07-27 |
| `dev/design/bindings.md` | SDK bindings spec; §1 governed SDK surface invariant (allowlist + parity, AC-074); §10 recovery-unreachability... | 25 rewrote §1/§13/§14; §10 preserved | 2026-06-04 |
| `dev/design/0.8.0-agent-memory-fit.md` | Agent-memory gap ladder (G0–G12) + §7 read-verb HITL questions | scope source for 0.8.0 | 2026-06-02 |
| `dev/design/0.8.0-v05-feature-triage.md` | v0.5.x feature triage (ship/defer/drop) | scope source of truth | 2026-06-02 |
| `dev/design/0.8.0-slice-5-G1-design.md` | Slice 5 design memo — structured `SearchHit` shape, per-branch score, dedup/order, step-11 tokenizer migration +... | 5 (G1) | 2026-06-02 |
| `dev/design/slice-10-design.md` | Slice 10 design memo — G9 RRF fusion (formula/tiebreak, dropped-knob note) + rerank seam, G10 `SearchFilter` + 3-way... | 10 (G9/G10/G12-recency) | 2026-06-03 |
| `dev/design/slice-15-g0-design.md` | Slice 15 design memo — G0 canonical-identity substrate: step-12 additive `ALTER` (exemption-marker rationale)... | 15 (G0 keystone); amended by 31 | 2026-06-05 |
| `dev/design/slice-15-design.md` | Slice 15 design memo — G11 edge enrichment + BYO-LLM ingest + edge projectability: step-14 exact migration SQL (5... | 15 (G11 BYO-LLM keystone) | 2026-06-13 |
| `dev/design/slice-31-identity-rescope-design.md` | Slice 31 design memo — re-scope active-row uniqueness to `logical_id` ALONE on both tables (Decision 5, HITL-SIGNED... | 31 (G0 re-scope) | 2026-06-05 |
| `dev/design/slice-20-g8-design.md` | Slice 20 design memo — G8 dangling-edge flag-and-count: cross-row post-row-insert EXISTS pass inside... | 20 (G8/F10) | 2026-06-03 |
| `dev/design/slice-20-design.md` | Slice 20 design memo — G5/G6 graph traversal: BFS CTE SQL (ADR conflict resolution: t_invalid filter)... | 20 (0.8.1 G5/G6) | 2026-06-13 |
| `dev/design/slice-25-conformance-design.md` | Slice 25 design memo — governed-surface conformance rewrite: the allowlist (core 5 + `read.*` 4), the four... | 25 (AC-057a→AC-074) | 2026-06-04 |
| `dev/design/slice-27-rust-allowlist-design.md` | Slice 27 design memo — Rust-facade governed-surface allowlist/parity pin (Q5=BIND-RUST): the curated 17-type... | 27 (AC-074 Rust half) | 2026-06-05 |
| `dev/design/slice-27-fix1-operator-gate-design.md` | Slice 27 fix-1 design memo — feature-gate the operator/recovery seam off the default Rust facade (HITL Option B... | 27 fix-1 (AC-074 method-level + AC-050c) | 2026-06-06 |
| `dev/design/slice-27-fix2-engine-test-gate-design.md` | Slice 27 fix-2 design memo — restore `cargo test -p fathomdb-engine` (default) under the operator gate (codex [P1])... | 27 fix-2 (engine default test build) | 2026-06-06 |
| `dev/design/slice-30-design.md` | 0.8.1 Slice 30 design memo | 30 (R3) | 2026-06-13 |
| `dev/design/slice-33-cursor-hardening-design.md` | Slice 33 design memo — op-store `read.collection`/`read.mutations` cursor + limit hardening under a genuine ~1M-row... | 33 (G3/F4-READ) | 2026-06-05 |
| `dev/design/slice-34-cli-op-store-readback-design.md` | Slice 34 design memo — CLI-only `doctor dump-mutations` op-store read-back: the scope call (diagnostic dump over the... | 34 (F4-READ / reserved-gap-34) | 2026-06-06 |
| `dev/design/slice-35-design.md` | Slice 35 design memo — G4 filter grammar: `read.list(kind, predicates?, limit)` with closed `Predicate` enum... | 35 (G4 filter grammar) | 2026-06-13 |
| `dev/design/slice-5-design.md` | 0.8.1 Slice 5 design memo | 0.8.1 Slice 5 | 2026-06-13 |
| `dev/design/slice-25-r2-design.md` | 0.8.1 Slice 25 design memo | 0.8.1 Slice 25 (R2) | 2026-06-14 |
| `dev/plans/prompts/0.8.1-graph-track-HANDOFF-2.md` | 0.8.1 graph-track hand-off #2 (CURRENT entry point) | 0.8.1 graph track (entry point) | 2026-06-14 |
| `dev/plans/prompts/0.8.1-graph-track-HANDOFF.md` | 0.8.1 graph-track orchestrator continuation hand-off (#1, deep reference) | 0.8.1 graph track (deep ref) | 2026-06-14 |
| `dev/plans/runs/0.8.1-g0-design-review.md` | G0 Phase-1 adversarial design review | 0.8.1 G0 | 2026-06-14 |
| `dev/design/slice-G0-design.md` · `dev/plans/runs/0.8.1-g0-capability-map-*.json` | G0 Phase-1 design memo + capability map | 0.8.1 G0 | 2026-06-14 |
| `dev/design/0.8.1-graph-experiment-plan.md` | 0.8.1 graph/IR experiment plan (LME) | 0.8.1 graph track | 2026-06-14 |
| `dev/notes/elps-consult-3-provenance.md` | ELPS consult #3 — `ready.provenance` (PRE-3) ANSWERED | 0.8.1 graph track / G0 PRE-3 | 2026-06-14 |
| `dev/notes/longmemeval-leaderboard-reference.md` | LongMemEval external leaderboard + reading notes | 0.8.1 graph track (reference) | 2026-06-14 |
| `dev/design/fathomdb-graph-vs-mem0-zep-and-longmemeval-diagnosis.md` | Graph implementation vs Mem0/GraphRAG/Zep + LongMemEval diagnosis | 0.8.1 graph track (input to experiments) | 2026-06-14 |
| `dev/design/0.8.1-slice-10-reranker-design.md` | 0.8.1 Slice 10 design memo — IMPLEMENTED 0.8.2 Slice E1 | 0.8.1 Slice 10 (R1) → impl 0.8.2 Slice E1; standalone API Slice E2 | 2026-06-18 |
| `dev/design/agent-memory-impl-strategy.md` | Slice shapes / impl strategy for the gap ladder | 5/10/15/20/30 shapes | 2026-06-02 |
| `dev/design/retrieval.md` | Retrieval pipeline design (vector + FTS5, fusion) | 5/10 | (tree) |
| `dev/design/projections.md` | Projection model | 5/15 | (tree) |
| `dev/design/migrations.md` | Migration model (forward-only, accretion guard; index-only additive steps need no marker) | 5/15/33 (schema 10→11→13) | 2026-06-05 |
| `dev/design/vector.md`, `ann-index-vec0.md` | Vector store / vec0 ANN index | 10/15 | (tree) |
| `dev/design/op-store.md` | Operational mutation store (incl. the Slice 30 `read.collection`/`read.mutations` read-back contract: reader-pool... | 30/33/34 (`read.collection`/`read.mutations`) | 2026-06-06 |
| `dev/design/engine.md`, `lifecycle.md`, `scheduler.md`, `recovery.md`, `errors.md`, `embedder.md`, `embedder-decision.md`, `release.md`, `perf-gates.md`, `perf-regression-detection.md`, `0.7.0-vector-quant-pack1.md`, `0.7.1-EU-6-FIX-*.md` | Engine/lifecycle/scheduler/recovery/error/embedder/release/perf design notes | per-slice as touched | (tree) |
| `dev/design/0.8.20-sqlite-vec-99-vec0-delete-probe.md` | Finding — sqlite-vec `vec0` DELETE fails for >12-byte TEXT metadata; engine workaround; remedy is a bump to 0.1.9 | 0.8.20 Slice 22 (R-20-VC leg 4); TC-76 re-open trigger | 2026-07-28 |
| `dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md` | Cache the 0.8.18 vector-equivalence verdict on an embedder-identity fingerprint so `Engine::open` cost is constant | 0.8.20 Slice 22 (R-20-VC leg 2) | 2026-07-28 |
| `dev/design/0.8.20-tc67-unsupported-vector-kind-report.md` | `ProjectionDelta.vector_unsupported_kinds` — report node kinds the vector writer can never commit, instead of silence | 0.8.20 Slice 22 (R-20-VC leg 1) | 2026-07-28 |
| `dev/design/0.8.20-tc90-tc91-characterization.md` | Characterization (no fix) — `Engine::transition`'s deferred write race (reproduces 10/10 under stress), and the cadence-sensitive duplicate embeds whose discarded worker commit is structurally invisible to terminal-state counting | 0.8.20 Slice 23 (R-20-SV leg 2); TC-90/TC-91, fix at 0.8.21 | 2026-07-29 |
| `dev/design/0.8.20-slice-31-sbom-survey-tool.md` | Spec of record for `scripts/sbom-survey` — CycloneDX SBOM over tracked manifests, tiering, used-vs-published diff; 23 criteria | 0.8.20 Slice 31 (Library Sweep #3 leg 1/3; no requirement id, TC-76) | 2026-07-29 |
| `scripts/sbom-survey/README.md` | Operating note for the dependency-survey mini-project — how to run the suite, and why it is deliberately not CI-gating | 0.8.20 Slice 31 (Library Sweep #3 leg 1/3) | 2026-07-29 |

## `dev/adr/` — architecture decision records

> Long-form per-doc notes: [`dev/doc-index/adr.md`](doc-index/adr.md).

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/adr/README.md`, `ADR-0.6.0-decision-index.md` | ADR index | — | (tree) |
| `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md` | Supersede AC-057a's five-verb cap → governed surface; **status: SIGNED/accepted** (Q1–Q5 =... | advanced 0.b; signed 2026-06-03; executed at 25; gates 30 | 2026-06-03 |
| `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` | NEW (0.a) — canonical identity substrate (logical_id+superseded_at, Option 2A); Decision 5 (Slice 31) re-scopes... | authored at 0.a; gates 15; amended by 31 | 2026-06-05 |
| `dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md` | Retrieval+identity ADR (Q1 table-stakes, Q3 RRF compat); gates Slice 10; Q2/Q4 amended by Slice 31... | gates 10; amended by 31 | 2026-06-05 |
| `dev/adr/ADR-0.8.0-embedder-identity-change-workflow.md` | Embedder-identity change workflow | — | (tree) |
| `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` | NEW (Slice 32) — intended graph model: one **ontology-neutral** binary property-graph substrate first-classing... | Slice 32 (signed) | 2026-06-05 |
| `dev/adr/ADR-0.8.0-graph-traversal-scope.md` | NEW (Slice 35) — F1/G5/G6 graph-traversal SCOPE: SDK depth ceiling ≤3 + engine hard cap 50 (v0.5.6... | **35** produces; gates 0.8.1 Slice H (G5/G6) | 2026-06-06 |
| `dev/adr/ADR-0.8.0-filter-grammar.md` | NEW (Slice 35) — G4/F3 CLOSED typed filter enum `{JsonPathEq, JsonPathCompare{Gt/Gte/Lt/Lte}... | **35** produces; gates 0.8.x G4 | 2026-06-06 |
| `dev/roadmap/0.8.1.md` | NEW (Slice 35 close) — 0.8.1 roadmap direction (REVISABLE): the HITL-signed graph-traversal-scope decisions recorded... | **35** close; informs 0.8.1 | 2026-06-06 |
| `dev/adr/ADR-0.8.1-deferred-f9-confidence-importance.md` | F9 confidence vs G12 importance — **DEFERRED 0.8.2+**; prerequisites: R2 eval (Slice 25), ≥100 confidence-bearing... | **35** produces; gates 0.8.2+ | 2026-06-13 |
| `dev/adr/ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md` | F5 fielded FTS / BM25F column model — **DEFERRED 0.8.2+**; prerequisites: R0 CDF (Slice 5), R2 eval (Slice 25), HITL... | **35** produces; gates 0.8.2+ | 2026-06-13 |
| `dev/adr/ADR-0.8.1-byo-llm-extraction-protocol.md` | BYO-LLM Extraction Provider Protocol ADR — `fathomdb.extract.v1` engine-side contract (spawn+handshake+ingest... | Slice 0; Slice 15 implements | 2026-06-12 |
| `dev/adr/ADR-0.8.6-generalized-provider-protocol.md` | OPP-8 generalized typed-task provider protocol | 0.8.6 Slice 0 gates; **Slice 5 implements** | 2026-06-26 |
| `dev/adr/ADR-0.8.6-governed-verb-coupling-hygiene.md` | OPP-5 governed-verb coupling hygiene | 0.8.6 Slice 0 gates; **Slice 10 implements** | 2026-06-26 |
| `dev/adr/ADR-0.8.1-ir-measure-eval-design.md` | IR-measure/Eval Design ADR — R0 CDF spec (found@K for K∈{50..1000}, per-class, all arms; gates Slice 10 rerank... | Slice 0; Slice 5/25 implements | 2026-06-12 |
| `dev/adr/ADR-0.8.1-graph-substrate-g11-migration.md` | G11 edge enrichment ADR — activates H3 reservation; step-14 SCHEMA_VERSION 13→14 (additive... | Slice 0; Slice 15 implements | 2026-06-12 |
| `dev/adr/ADR-0.8.12-consolidation-recency-provider.md` | OPP-2 consolidation/recency provider | 0.8.12 Slice 0 gates; **Slice 15 implements**, Slice 20 value-gates | 2026-07-01 |
| `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` | EXP-S kind-tagged coexisting-index substrate migration (+ F5 co-land) | 0.8.14 Slice 0 gates; Slices 5/10 implement | 2026-07-03 |
| `dev/adr/ADR-0.8.16-f9-importance-confidence-ranking.md` | F9 importance/confidence ranking — opens the deferred F9 signal | 0.8.16 Slice 0 gates; **Slice 5 implements** | 2026-07-08 |
| `dev/adr/ADR-0.8.16-onnx-embedder-backend.md` | Cross-vendor ONNX embedder backend (`OrtBgeEmbedder`) | 0.8.16 Slice 0 gates; **Slices 10→15 implement** | 2026-07-08 |
| `dev/adr/ADR-0.6.0-cli-scope.md` | CLI scope = two-root operator surface (`recover` lossy / `doctor` bit-preserving); Option B (`search`/`get`/`list`... | 34 (amendment); reference | 2026-06-06 |
| `dev/adr/ADR-0.7.0-vector-binary-quant.md` | Binary-quant + f32 rerank recall floor (0.90). **40/GA-2 amendment (AC-075, ◆ B-1):** floor now GATED on the... | 40/GA-2 amends § 2 pt 4 + status | 2026-06-08 |
| `dev/adr/ADR-0.6.0-*.md`, `ADR-0.7.0-*.md`, `ADR-0.7.1-*.md` | Prior-release ADRs (typed-write boundary, CLI scope, error taxonomy, etc.) | reference (e.g. typed-write boundary preserved by 25) | (tree) |

## `dev/plans/` — plans + live state

> Long-form per-doc notes: [`dev/doc-index/plans.md`](doc-index/plans.md).

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` | 0.8.x program schedule-of-record (THE master). | Program Steward (keeps true) | 2026-06-29 |
| `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` | Program Steward hand-off — role/mandate (canonical). | Program Steward (entry point) | 2026-06-27 |
| `dev/plans/prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` | Release Orchestrator hand-off — sibling role (NOT the Steward). | Release Orchestrator (per-release) | 2026-06-27 |
| `dev/plans/0.8.0-implementation.md` | Authoritative slice contracts | the plan itself | 2026-06-05 |
| `dev/plans/0.8.0-plan.md` | Mod-5 ladder + reserved-gap policy + Immediate-Next-Slice pointer + Slice-0/5/10 CLOSED blocks | 0 authors; every slice advances the pointer | 2026-06-03 |
| `dev/plans/runs/STATUS-0.8.0.md` | Live state board | 0 authors; every slice updates at close | 2026-06-03 |
| `dev/plans/prompts/PLAN-TEMPLATE.md` | Per-release plan authoring template | authors every plan-<release>.md | 2026-06-26 |
| `dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md` | Per-slice prompt template | authors every slice prompt | 2026-06-03 |
| `dev/plans/prompts/0.8.0-slice-*.md` | Self-contained per-slice subagent prompts | per slice | (per slice) |
| `dev/plans/runs/0.8.0-slice-*-output.json` / `-review-*.md` | Per-slice closure artifacts + promoted codex verdicts | per slice | (per slice) |
| `dev/plans/runs/0.8.0-slice-6-tokenizer-experiment-*.md` | Slice 6 (B2) FTS5 tokenizer latency experiment | Slice 6 (B2) | 2026-06-07 |
| `dev/plans/0.8.1-plan.md` | 0.8.1 mod-5 ladder | 0.8.1 Slice 0 authors; every slice advances the pointer | 2026-06-12 |
| `dev/plans/0.8.1-implementation.md` | 0.8.1 authoritative slice contracts | the plan itself | 2026-06-12 |
| `dev/plans/runs/STATUS-0.8.1.md` | 0.8.1 live state board | 0.8.1 Slice 0 authors; every slice updates at close | 2026-06-12 |
| `dev/plans/prompts/0.8.1-MASTER-ORCHESTRATOR-HANDOFF.md` | 0.8.1 orchestrator hand-off | orchestrator | 2026-06-12 |
| `dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md` | BYO-LLM extraction-harness brief | 0.8.1 Slice 15 | 2026-06-12 |
| `dev/plans/prompts/README.md` | Archive-in-place convention for `prompts/` + the short list of live (non-archived) prompts | DOC-HYGIENE-1 T2/7 | 2026-07-24 |
| `dev/roadmap/0.8.2.md` | 0.8.2 roadmap | 0.8.2 | 2026-06-19 |
| `dev/plans/plan-0.8.2.md` | 0.8.2 slice plan (as-built) | 0.8.2 | 2026-06-19 |
| `dev/plans/plan-0.8.7.md` + `dev/plans/runs/STATUS-0.8.7.md` | 0.8.7 GPU embedder (OOB) — plan + live status (COMPLETE). | 0.8.7 Slices 0/5/10/40 | 2026-06-26 |
| `dev/plans/plan-0.8.9.md` + `dev/plans/runs/STATUS-0.8.9.md` | 0.8.9 CI-integrity micro (OOB) — plan + live status. | 0.8.9 Slices 0/1/5/10/15/40 | 2026-06-28 |
| `dev/plans/runs/0.8.16-slice-15-candle-onnx-equivalence.md` | 0.8.16 Slice 15 — candle↔ONNX numeric-equivalence measurement (R-ONNX-3) | 0.8.16 Slice 15 | 2026-07-08 |
| `dev/plans/plan-0.8.14.md` + `dev/plans/runs/STATUS-0.8.14.md` | 0.8.14 Substrate & recall (the schema-migration release) — plan + live orchestrator board. | 0.8.14 Slice 0 authors; every slice updates | 2026-07-05 |
| `dev/plans/plan-0.8.12.md` + `dev/plans/runs/STATUS-0.8.12.md` | 0.8.12 Memory-quality plumbing — plan + live orchestrator board. | 0.8.12 Slice 0 authors; every slice updates | 2026-07-01 |
| `dev/plans/runs/EXP-COV-results.md` | OPP-6 EXP-COV Phase-A `$0` results (discharges the parked OPP-6 eval). | 0.8.12 Slice 5 | 2026-07-01 |
| `dev/plans/runs/0.8.2-m1-corpus-manifest.json` | M1 corpus manifest artifact | 0.8.2 Slice 4 | 2026-06-17 |
| `src/python/eval/m1_graph_build.py` (+ `tests/test_m1_graph_build.py`) | M1 per-question graph build | 0.8.2 Slice 10 | 2026-06-17 |
| `dev/plans/runs/0.8.2-m1-graph-coverage-n300.json` | M1 graph coverage artifact | 0.8.2 Slice 10 | 2026-06-17 |
| `src/python/eval/m1_baseline.py` (+ `tests/test_m1_baseline.py`) | M1 strong-baseline harness — THE BAR | 0.8.2 Slice 5 + fix-2 | 2026-06-18 |
| `dev/notes/0.8.12-cpu-embedder-defect-blocks-dense-eval.md` | Env finding (0.8.12 EXP-COV-1) | 0.8.12 EXP-COV-1; ties to 0.8.14 | 2026-07-02 |
| `dev/notes/0.8.2-bge-cls-mean-engine-bug.md` | Engine bug note — CandleBgeEmbedder defaults to Mean pooling for a CLS model | 0.8.2 Slice 5 fix-2 | 2026-06-18 |
| `src/python/eval/m1_power_sim.py` (+ `tests/test_m1_power_sim.py`) | M1 whole-`decide()`-rule power simulation | 0.8.2 Slice 5 | 2026-06-18 |
| `src/python/eval/m1_baseline_run.py` | M1 baseline runner | 0.8.2 Slice 5 | 2026-06-18 |
| `dev/plans/runs/0.8.2-slice-10-graph-build.md` | Slice 10 note | 0.8.2 Slice 10 | 2026-06-17 |
| `src/python/eval/m1_ppr.py` (+ `tests/test_m1_ppr.py`) | M1 PPR-fusion arm — the graph mechanism KEYSTONE | 0.8.2 Slice 15 | 2026-06-19 |
| `dev/plans/runs/0.8.2-slice-15-ppr-arm.md` | Slice 15 note | 0.8.2 Slice 15 | 2026-06-19 |
| `dev/roadmap/0.8.3.md` | 0.8.3 roadmap | 0.8.3 | 2026-06-21 |
| `dev/plans/plan-0.8.3.md` | 0.8.3 slice plan | 0.8.3 | 2026-06-21 |
| `dev/roadmap/0.8.4.md` | 0.8.4 roadmap | 0.8.4 | 2026-06-21 |
| `dev/roadmap/0.8.5.md` | 0.8.5 roadmap | 0.8.5 | 2026-06-19 |
| `dev/archive/README.md` | Archive manifest | ledger-prune (`scripts/repo-prune/prompts/prune-docs.md`) | 2026-06-26 |
| `dev/archive/0.8.1-roadmap-direction-20260612.md` | Archived | reference | 2026-06-12 |
| `dev/plans/runs/IR-C-roadmap.md` (+ `-analysis-dossier`, `-deep-research`) | IR-C retrieval roadmap | reference / 0.8.1 source | 2026-06-12 |
| `dev/plans/runs/IR-C-recall-cdf.json` | R0 candidate-recall CDF artifact | 0.8.1 Slice 5 produces; gates Slice 10 | 2026-06-13 |
| `dev/plans/runs/IR-C-r0-findings.md` | R0 findings | 0.8.1 Slice 5 | 2026-06-13 |
| `src/python/eval/r2_parity_eval.py` (+ `eval/__init__.py`) | R2 parity eval harness | 0.8.1 Slice 25 (R2) + 0.8.3 Slice 5 | 2026-06-21 |
| `src/python/eval/gold_repin.py` | 0.8.3 D0a power-sized gold re-pin builder | 0.8.3 Slice 5 | 2026-06-21 |
| `src/python/eval/corpus_validity.py` | 0.8.3 D0a corpus-validity guard | 0.8.3 Slice 5 | 2026-06-21 |
| `src/python/eval/mem0_local.py` | 0.8.3 Mem0-OSS footprint-safe LOCAL backend config | 0.8.3 Slice 5 | 2026-06-21 |
| `dev/plans/runs/0.8.3-d0a-corpus-manifest.json` + `0.8.3-d0a-memory-gold.json` | 0.8.3 D0a re-pin artifacts | 0.8.3 Slice 5 | 2026-06-21 |
| `dev/plans/runs/0.8.3-eu7-bisect-report.md` (+ `0.8.3-eu7-bisect.json`) | 0.8.3 eu7 0.937→0.896 root-cause / bisect | 0.8.3 Slice 20 (de-risk) | 2026-06-22 |
| `dev/plans/runs/0.8.3-resolution-verdict.md` (+ `.json`) | 0.8.3 RESOLUTION VERDICT (Slice 30) — CLOSED AS-IS | 0.8.3 Slice 30 | 2026-06-23 |
| `dev/plans/runs/0.8.3-parity-forward-plan.md` | 0.8.3→parity forward plan | 0.8.3 / 0.8.4 framing | 2026-06-22 |
| `dev/design/0.8.3-slice-5-design.md` | 0.8.3 Slice 5 (D0a) design + de-risk findings | 0.8.3 Slice 5 | 2026-06-21 |
| `src/python/eval/decision_rule_083.py` (+ `tests/test_decision_rule_083.py`) | 0.8.3 frozen RESOLUTION decision rule | 0.8.3 Slice 0 | 2026-06-21 |
| `dev/plans/runs/IR-C-r2-eval-results.md` | R2 eval results doc | 0.8.1 Slice 25 (R2) | 2026-06-14 |
| `dev/plans/0.6.x/0.7.x-*.md`, `ci-deferred.md`, `README.md` | Prior-release plans + CI-deferral ledger | reference | (tree) |
| `dev/scripts/ir_c_ce_latency.py` | CE latency benchmark | 0.8.1 Slice 5 | 2026-06-13 |

## `docs/` — user-facing documentation (mkdocs, `nav` in `mkdocs.yml`)

> Long-form per-doc notes: [`dev/doc-index/docs.md`](doc-index/docs.md).

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `docs/index.md` | Docs home | X2 (nav) | 2026-05-17 |
| `docs/getting-started/index.md` | Getting-started overview | — | 2026-05-17 |
| `docs/getting-started/quickstart.md` | Quickstart (five-operation contract) | 5/30 (new surface examples) | 2026-05-17 |
| `docs/install/python.md` | Python install | — | 2026-05-30 |
| `docs/install/typescript.md` | TypeScript install | — | 2026-05-30 |
| `docs/install/rust.md` | Rust install | — | 2026-05-17 |
| `docs/reference/index.md` | API-reference overview | — | 2026-05-17 |
| `docs/reference/python-api.md` | Python API reference (incl. the `read.*` verbs + `NodeRecord`/`OpStoreRow` shapes); 0.8.20 Slice 5d adds... | 5 (`SearchHit`), 10 (`SearchFilter`/RRF), 30 (`read.*`), 31 (`logical_id`-alone supersession), 0.8.1-10 (`rerank_depth` opt-in), 0.8.20 Slice 5d (`erase_source`) | 2026-07-19 |
| `docs/reference/typescript-api.md` | TypeScript API reference (incl. the `read.*` verbs + `NodeRecord`/`OpStoreRow` shapes); 0.8.20 Slice 5d adds... | 5 (`SearchHit`), 10 (`SearchFilter`/RRF), 30 (`read.*`), 31 (`logicalId`-alone supersession), 0.8.1-10 (`rerankDepth` opt-in), 0.8.20 Slice 5d (`eraseSource`) | 2026-07-19 |
| `docs/reference/cli.md` | CLI reference (recovery verbs CLI-only); 34 documents the `doctor dump-mutations` op-store read-back diagnostic +... | 34 (dump-mutations); 0.8.20 Slice 5d (R-20-E8) | 2026-07-19 |
| `docs/reference/errors.md` | Error reference (taxonomy) | per-binding error-class adds | 2026-05-17 |
| `docs/reference/config.md` | Config reference | — | 2026-05-17 |
| `docs/concepts/index.md` | Concepts overview | — | 2026-05-17 |
| `docs/embedder.md` | Default embedder | — | 2026-06-01 |
| `docs/compatibility/index.md` | Compatibility matrix | 40 (compat events) | 2026-05-17 |
| `docs/operations/index.md` | Operations guide | — | 2026-07-19 |
| `docs/operations/erasure.md` | Erasure boundary | 0.8.20 Slice 5d (R-20-E4/E8, design §4 item 12) | 2026-07-19 |
| `docs/guides/index.md` | Guides hub (structured-hit / retrieve examples land here) | 5/30 add examples | 2026-06-04 |
| `docs/guides/structured-search-hits.md` | Structured `SearchHit` usage guide (id/kind/body/score/branch; Py + TS) | 5 (G1); 10 (score → RRF) | 2026-06-03 |
| `docs/guides/retrieve-by-id.md` | Retrieve-by-id guide — `read.get`/`read.get_many` point lookup by `logical_id` (active-only) +... | 30 (G2/G3) | 2026-06-04 |
| `docs/guides/hybrid-search-filtering.md` | Hybrid search guide — G9 RRF ranking (documented behavior-compat event) + G10 `SearchFilter` metadata filtering, Py... | 10 (G9/G10) | 2026-06-03 |
| `docs/positions/index.md` | Positions hub | — | 2026-05-01 |
| `docs/positions/sdk-parity.md` | Position: SDK parity (guarantee carried forward by 25) | 25 | 2026-05-01 |
| `docs/positions/recovery-surface.md` | Position: recovery surface (denylist, CLI-only) | preserved by 25/30 | 2026-05-01 |
| `docs/positions/tokenizer-policy.md` | Position: tokenizer policy | 5 (FTS5 default upgrade) | 2026-05-01 |
| `docs/positions/embedder-identity.md` | Position: embedder identity | — | 2026-05-01 |
| `docs/release-notes/0.6.0.md` | 0.6.0 release notes | — | 2026-05-17 |
| `docs/release-notes/0.6.1.md` | 0.6.1 release notes (**added to nav at Slice 0** — was orphaned) | X2 | 2026-05-24 |
| `docs/release-notes/0.8.0.md` | 0.8.0 release notes — finalized at 40/GA-2 (3 behavior-compat events, governed read.*, structured hits, G0 identity... | 0 stub; 40/GA-2 finalizes | 2026-06-08 |
| `dev/releases/0.8.0.md` | 0.8.0 internal release record | 40/GA-2 | 2026-06-08 |

## Corpus / eval expansion (out-of-band, owner-managed — integrated at Slice-5 push 2026-06-02)

> Out-of-band corpus-work line (owner-managed, integrated at Slice-5 push 2026-06-02). Long-form per-doc notes: [`dev/doc-index/corpus.md`](doc-index/corpus.md).

| Doc | Purpose | Owning slice/AC | Last-touched |
|-----|---------|-----------------|--------------|
| `dev/corpus-creation/README.md` + `architecture.md` | Corpus-creation overview + architecture | corpus-work (out-of-band) | 2026-06-02 |
| `dev/notes/0.8.x-corpus-source-expansion-research.md` | Corpus source-expansion research notes | corpus-work (0.8.x) | 2026-06-02 |
| `dev/notes/0.8.x-pmc-oa-reconsideration.md` | PMC-OA source reconsideration note | corpus-work (0.8.x) | 2026-06-02 |
| `dev/plans/prompts/0.8.x-corpus-qa-expansion-handoff.md` | Corpus QA-expansion handoff prompt | corpus-work (0.8.x) | 2026-06-02 |
| `dev/plans/prompts/0.8.x-corpus-source-expansion-search.md` | Corpus source-expansion search prompt | corpus-work (0.8.x) | 2026-06-02 |
| `tests/corpus/corpus-card.md` + `README.md` | Eval corpus card + acquisition README (scripts under `tests/corpus/scripts/`) | corpus-work (eval) | 2026-06-02 |
| `tests/corpus/scripts/acquire_musique.py` | MuSiQue-Ans corpus acquire script | 0.8.2 Slice 4 | 2026-06-17 |
| `tests/corpus/scripts/test_acquire_musique.py` | TDD tests for MuSiQue materializer | 0.8.2 Slice 4 | 2026-06-17 |
