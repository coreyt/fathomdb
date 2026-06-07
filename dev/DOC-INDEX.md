# DOC-INDEX — FathomDB documentation map (agentic cold-start)

> **Purpose.** The single cold-start map an agent reads to find the right doc
> *without scanning the tree*. One row per doc: **path → purpose → owning
> slice/AC → last-touched**. Created at **Slice 0** of 0.8.0 (X3 cross-cutting
> requirement).
>
> **THE RULE (binds every slice — stated here, on `runs/STATUS-0.8.0.md`, and in
> `0.8.0-plan.md`):** **every slice updates `dev/DOC-INDEX.md` in its closing docs
> commit.** When a slice adds, renames, or materially changes a doc, it adds/edits
> that doc's row here (path · purpose · owning slice/AC · last-touched) in the same
> commit that closes the slice (mirrors the §12.4 plan-as-state-machine discipline,
> applied to docs). A stale or missing row is an X3 gap; Slice 40 **gate m** fails
> the release if `dev/DOC-INDEX.md` is not the accurate map of the shipped surface.

`last-touched` = date of the last git commit that modified the file (best-effort;
refresh in the closing commit when you touch a doc).

---

## `dev/` — engineering docs (the build-time source of truth)

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/README.md` | Entry map for the engineering docs tree | — | 2026-05-02 |
| `dev/needs.md` | Product/consumer needs driving requirements | — | 2026-05-28 |
| `dev/requirements.md` | Numbered requirements (REQ-*); REQ-053 = governed SDK surface (allowlist + parity + recovery-denylist + typed boundary) | 25 amended REQ-053 (Q3) | 2026-06-04 |
| `dev/acceptance.md` | Acceptance criteria (AC-*); AC-057a five-verb cap superseded by AC-074 (governed surface); AC-074 Rust-facade measurement filled (Q5=BIND-RUST), tightened to method-level + feature-gated by 27 fix-1 | 25 (AC-057a→AC-074); 27 fills AC-074 Rust clause; 27 fix-1 method-level; 40 scoreboard | 2026-06-06 |
| `dev/interfaces/rust.md` | Rust public interface (owner of the Rust-visible symbol spelling + the governed Rust-facade surface contract — AC-074 Q5=BIND-RUST positive-allowlist/parity/denylist-absence; 27 fix-1 adds the default-vs-`operator`-feature method-level boundary) | 27 (governed-surface contract); 27 fix-1 (operator feature gate); ground-truth engine type names | 2026-06-06 |
| `dev/interfaces/cli.md` | CLI public interface (concrete flag spelling, root paths, exit-code classes, `--json` wrapping for the two-root operator CLI); 34 adds the `doctor dump-mutations` op-store read-back diagnostic ({0,70,71}; empty page = exit 0) + reconciles the operator-only prose | 34 (dump-mutations); owned-by ADR-0.6.0-cli-scope | 2026-06-06 |
| `dev/architecture.md` | System architecture (engine, projections, reader pool, surface) | 5/10/15/30 update read-path + receipt surface (30 adds the governed `read.*` reader-pool dispatch); 31 re-scopes G0 active identity to `logical_id` alone | 2026-06-05 |
| `dev/test-plan.md` | Test strategy + tiers (incl. functional-harness tier X1 + the Slice 10 G9/G10/G12-recency tier) | 5 adds functional tier; 10 adds RRF/filter/recency tier | 2026-06-03 |
| `dev/traceability.md` | REQ ↔ AC ↔ test trace matrix | 25 re-points REQ-053↔new AC; 30 adds read ACs | 2026-05-28 |
| `dev/security-review.md` | Security review (SR-*) | — (SR-005/SR-011 candidate reserved-gap) | 2026-05-02 |
| `dev/learnings.md` | Cross-phase engineering learnings | per-slice as discovered | 2026-05-31 |
| `dev/notes/0.8.0-fts5-tokenizer-latency-experiment.md` | **B2 FTS5 tokenizer latency experiment report** — measured config×tier sweep + engine A/B proving the tokenizer is latency-neutral (the cost is O(N) corpus-scaling); recommends tiering AC-012 per the AC-072/073 precedent. Run artifacts under `dev/plans/runs/0.8.0-slice-6-*` | Slice 6 (B2) | 2026-06-07 |
| `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md` | **Recall-eval framework assessment** — grounds the fidelity-vs-IR/agentic-relevance axis distinction; FathomDB's GA gate (eu7/AC-075) is fidelity (system health), the product-value gate (evidence/task recall) does not exist yet (eu8 is report-only, ceiling ≈0.571). Reframes the GA recall halt + seeds the IR-eval IR-1/IR-2 initiative | IR-eval (IR-1/IR-2 input) | 2026-06-07 |
| `dev/plans/0.8.0-GA-and-IR-eval-roadmap.md` | **Sequenced roadmap** — lines up every unfinished task to GA + to the IR-eval gate: 3 tracks (GA / Corpus / IR-eval), gates, dependencies, the two critical paths. Answers B-1-vs-consensus, corpus-freeze timing, Slice-40 landing, IR-1 Ph2–4 completion | orchestrator (live) | 2026-06-07 |
| `dev/plans/prompts/0.8.x-IR-1-phase1-measure-consensus.md` | **IR-1 Phase 1 (runnable now)** — define the IR/agentic-relevance measure + Claude↔codex consensus → signed `dev/design/ir-recall-measure.md`. No AC/gold-set/experiments. No prerequisites | IR-eval (now) | 2026-06-07 |
| `dev/plans/prompts/0.8.x-IR-1-recall-measure.md` · `0.8.x-IR-2-recall-gate.md` | **IR-eval IR-1 Phases 2–4 (DEFERRED) + IR-2** — Ph2–4 mint AC-077 + build eval infra + experiments (gated on: Phase 1 merged · Slice-40 merged · B-1 ruling · corpus frozen); IR-2 analyzes → HITL gate recommendation | IR-eval (post-0.8.0-GA / 0.8.1) | 2026-06-07 |
| `dev/memex-note-on-0.6.0.md` | Memex consumer note on 0.6.0 | — | 2026-05-21 |
| `dev/DOC-INDEX.md` | **This file** — agentic doc map | 0 creates; every slice updates | 2026-06-02 |

## `dev/design/` — design notes + ADR-adjacent specs

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/design/README.md` | Design-notes index | — | (tree) |
| `dev/design/orchestration.md` | Orchestration rules (§1/§1.5/§6/§8/§9/§10/§11/§12) — the binding spine for the plan | binds every slice | 2026-05-31 |
| `dev/design/bindings.md` | SDK bindings spec; §1 governed SDK surface invariant (allowlist + parity, AC-074); §10 recovery-unreachability (BYTE-FROZEN) | 25 rewrote §1/§13/§14; §10 preserved | 2026-06-04 |
| `dev/design/0.8.0-agent-memory-fit.md` | Agent-memory gap ladder (G0–G12) + §7 read-verb HITL questions | scope source for 0.8.0 | 2026-06-02 |
| `dev/design/0.8.0-v05-feature-triage.md` | v0.5.x feature triage (ship/defer/drop) | scope source of truth | 2026-06-02 |
| `dev/design/0.8.0-slice-5-G1-design.md` | Slice 5 design memo — structured `SearchHit` shape, per-branch score, dedup/order, step-11 tokenizer migration + re-tokenization, X1/X2/X3 plan | 5 (G1) | 2026-06-02 |
| `dev/design/slice-10-design.md` | Slice 10 design memo — G9 RRF fusion (formula/tiebreak, dropped-knob note) + rerank seam, G10 `SearchFilter` + 3-way shape-sentinel, G12-recency flag, score-comparability note, test plan | 10 (G9/G10/G12-recency) | 2026-06-03 |
| `dev/design/slice-15-g0-design.md` | Slice 15 design memo — G0 canonical-identity substrate: step-12 additive `ALTER` (exemption-marker rationale), tombstone-then-insert supersession + same-txn atomicity, NULL-on-legacy-rows rule, `row_cursors` semantics, op-store cascade, reserved Slice-16 shadow reconciliation, test plan (active-identity re-scoped to `logical_id` alone by Slice 31) | 15 (G0 keystone); amended by 31 | 2026-06-05 |
| `dev/design/slice-31-identity-rescope-design.md` | Slice 31 design memo — re-scope active-row uniqueness to `logical_id` ALONE on both tables (Decision 5, HITL-SIGNED 2026-06-05): fixes the kind-change identity-fork, resolves Slice 30 read.get [P2], re-keys G8; amend step-12 in place (no SCHEMA_VERSION bump) | 31 (G0 re-scope) | 2026-06-05 |
| `dev/design/slice-20-g8-design.md` | Slice 20 design memo — G8 dangling-edge flag-and-count: cross-row post-row-insert EXISTS pass inside `commit_batch`'s open tx (why not `validate_write`), logical_id-alone probe + step-12 partial-index hit argument, legacy-NULL endpoint consequence, flag-and-count default (strict-mode deferred to band 22), test plan | 20 (G8/F10) | 2026-06-03 |
| `dev/design/slice-25-conformance-design.md` | Slice 25 design memo — governed-surface conformance rewrite: the allowlist (core 5 + `read.*` 4), the four falsifiable properties (P1 allowlist-membership · P2 cross-binding parity · P3 recovery-denylist empty-intersection · P4 no-raw-SQL), the honest-green plan for not-yet-live `read.*`, touch-points | 25 (AC-057a→AC-074) | 2026-06-04 |
| `dev/design/slice-27-rust-allowlist-design.md` | Slice 27 design memo — Rust-facade governed-surface allowlist/parity pin (Q5=BIND-RUST): the curated 17-type governed allowlist vs the 20 operator-seam re-exports (CLI-only), the three properties (P1 positive-allowlist · P2 parity-in-intent-not-identity · P3 recovery-denylist-absence exact-match), AC-074 Rust measurement, mirrors the best-effort `no_recovery_surface.rs` style (no Rust runtime introspection) | 27 (AC-074 Rust half) | 2026-06-05 |
| `dev/design/slice-27-fix1-operator-gate-design.md` | Slice 27 fix-1 design memo — feature-gate the operator/recovery seam off the default Rust facade (HITL Option B, codex [P1]): the `operator` cargo feature, the 12 gated methods + 6 private helpers + 20 re-exports, cfg-gated `compile_fail` method-absence doctests (feature-unification-safe), AC-050c green via scoping `tests/` out of the public-API scanner | 27 fix-1 (AC-074 method-level + AC-050c) | 2026-06-06 |
| `dev/design/slice-27-fix2-engine-test-gate-design.md` | Slice 27 fix-2 design memo — restore `cargo test -p fathomdb-engine` (default) under the operator gate (codex [P1]): 13 pure-operator engine test targets get `[[test]] required-features=["operator"]`; the mixed `pr2b_mean_recompute.rs` keeps its non-operator carve-out test via per-fn `#[cfg]`; `corpus_graph` is not-operator (doc-mention only); no method un-gated | 27 fix-2 (engine default test build) | 2026-06-06 |
| `dev/design/slice-30-design.md` | Slice 30 design memo — governed `read.*` (G2 `read.get`/`read.get_many` active-only by `logical_id`; G3 `read.collection`/`read.mutations` op-store read-back with mandatory limit + after-id cursor): per-request typed reader channels (no `ReaderResponse`/Search reshape), `NodeRecord`/`OpStoreRow` shapes, get_many partial-order-preserved, not-found=`None`/`null` (NotFound→gap 31), conformance-genuinely-enforced plan, test plan | 30 (G2/G3) | 2026-06-04 |
| `dev/design/slice-33-cursor-hardening-design.md` | Slice 33 design memo — op-store `read.collection`/`read.mutations` cursor + limit hardening under a genuine ~1M-row log: EXPLAIN before (id-PK walk) / after (step-13 `(collection_name,id)` index, index-driven, no SCAN / no temp B-tree), clamp/cursor edge cases (`limit==0`, over-MAX clamp, negative/past-end `after_id`, unknown collection), accretion guard (index-only ⇒ no marker), test plan; no SDK signature change | 33 (G3/F4-READ) | 2026-06-05 |
| `dev/design/slice-34-cli-op-store-readback-design.md` | Slice 34 design memo — CLI-only `doctor dump-mutations` op-store read-back: the scope call (diagnostic dump over the mutation log, NOT the rejected Option-B application query surface; ADR amendment), verb shape + default `--limit` 1000, `--json` envelope (`verb`/`collection`/`after_id`/`limit`/`count`/`rows`/`next_after_id`), exit map {0,70,71} (empty page = 0), inline-serialize rationale (no `OpStoreRow` re-export ⇒ facade type-set unchanged), RED test plan; no engine/schema/SDK change | 34 (F4-READ / reserved-gap-34) | 2026-06-06 |
| `dev/design/agent-memory-impl-strategy.md` | Slice shapes / impl strategy for the gap ladder | 5/10/15/20/30 shapes | 2026-06-02 |
| `dev/design/retrieval.md` | Retrieval pipeline design (vector + FTS5, fusion) | 5/10 | (tree) |
| `dev/design/projections.md` | Projection model | 5/15 | (tree) |
| `dev/design/migrations.md` | Migration model (forward-only, accretion guard; index-only additive steps need no marker) | 5/15/33 (schema 10→11→13) | 2026-06-05 |
| `dev/design/vector.md`, `ann-index-vec0.md` | Vector store / vec0 ANN index | 10/15 | (tree) |
| `dev/design/op-store.md` | Operational mutation store (incl. the Slice 30 `read.collection`/`read.mutations` read-back contract: reader-pool DEFERRED-tx, `ORDER BY id`, mandatory limit ≤ ~1M cap, after-id cursor; Slice 33 index-driven pagination via the step-13 `(collection_name,id)` index + clamp/cursor edge contract; Slice 34 notes the CLI `doctor dump-mutations` operator diagnostic over the same seam) | 30/33/34 (`read.collection`/`read.mutations`) | 2026-06-06 |
| `dev/design/engine.md`, `lifecycle.md`, `scheduler.md`, `recovery.md`, `errors.md`, `embedder.md`, `embedder-decision.md`, `release.md`, `perf-gates.md`, `perf-regression-detection.md`, `0.7.0-vector-quant-pack1.md`, `0.7.1-EU-6-FIX-*.md` | Engine/lifecycle/scheduler/recovery/error/embedder/release/perf design notes | per-slice as touched | (tree) |

## `dev/adr/` — architecture decision records

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/adr/README.md`, `ADR-0.6.0-decision-index.md` | ADR index | — | (tree) |
| `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md` | Supersede AC-057a's five-verb cap → governed surface; **status: SIGNED/accepted** (Q1–Q5 = A1/B1/amend/confirm/**BIND-RUST**; Rust pin → Slice 27) | advanced 0.b; signed 2026-06-03; executed at 25; gates 30 | 2026-06-03 |
| `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` | NEW (0.a) — canonical identity substrate (logical_id+superseded_at, Option 2A); Decision 5 (Slice 31) re-scopes active uniqueness to `logical_id` alone | authored at 0.a; gates 15; amended by 31 | 2026-06-05 |
| `dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md` | Retrieval+identity ADR (Q1 table-stakes, Q3 RRF compat); gates Slice 10; Q2/Q4 amended by Slice 31 (`logical_id`-alone identity; edges DO supersede) | gates 10; amended by 31 | 2026-06-05 |
| `dev/adr/ADR-0.8.0-embedder-identity-change-workflow.md` | Embedder-identity change workflow | — | (tree) |
| `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` | NEW (Slice 32) — intended graph model: one **ontology-neutral** binary property-graph substrate first-classing **corpus (GraphRAG)** + **memory (Graphiti fact-on-edge)** ontologies; opaque-id addressing for 0.8.0 (hybrid future ADR); edge-enrichment/projectability/edge-inclusive-G7 **reserved-additive**; fact-on-node escape hatch. **status: ACCEPTED — H1 (neutral-both) + H3 (reserve edge-enrichment additive) HITL-SIGNED 2026-06-05**; H2/H4/H5/H6 deferred 0.8.x; **no 0.8.0 substrate change**. Foundation for the Slice 35 graph ADRs | Slice 32 (signed) | 2026-06-05 |
| `dev/adr/ADR-0.8.0-graph-traversal-scope.md` | NEW (Slice 35) — F1/G5/G6 graph-traversal SCOPE: SDK depth ceiling ≤3 + engine hard cap 50 (v0.5.6 `MAX_TRAVERSAL_DEPTH` port); 0.8.x walk filters `superseded_at IS NULL` only (edge valid-time G11 deferred); `canonical_edges(from_id)/(to_id)` already folded into G0 → **no new migration** (gap 36 not triggered); `G6 = G1+G4+G5+G9`, build G6 before standalone G5; v0.5.6 `WITH RECURSIVE` BFS re-targeted to `from_id`/`to_id`. **status: ACCEPTED as 0.8.1 roadmap direction** (HITL-signed 2026-06-06; revisable when 0.8.1 graph work opens — recorded in `dev/roadmap/0.8.1.md`; graph work retargeted 0.8.x→0.8.1). No 0.8.0 code/schema change | **35** produces; gates 0.8.1 Slice H (G5/G6) | 2026-06-06 |
| `dev/adr/ADR-0.8.0-filter-grammar.md` | NEW (Slice 35) — G4/F3 CLOSED typed filter enum `{JsonPathEq, JsonPathCompare{Gt/Gte/Lt/Lte}, ScalarValue{Text/Integer/Bool}}` on `read.list(kind, filter?, limit)`; EXCLUDES the fused predicates + all `*_unchecked` builders; coordinates with G10 `SearchFilter` (shared value vocab; **full unification = NEEDED future work, reserved-gap 37, affects both G4+G10**); compiles to parameterized `json_extract` over allowlisted paths (no DSL/no raw SQL). **status: ACCEPTED** (HITL-signed 2026-06-06). No 0.8.0 code/schema change | **35** produces; gates 0.8.x G4 | 2026-06-06 |
| `dev/roadmap/0.8.1.md` | NEW (Slice 35 close) — 0.8.1 roadmap direction (REVISABLE): the HITL-signed graph-traversal-scope decisions recorded as roadmap direction not a frozen G-gap contract (graph work retargeted 0.8.x→0.8.1); + the G4↔G10 unification (gap 37, needed) + the parked F9/F5 Slice 46 pointer | **35** close; informs 0.8.1 | 2026-06-06 |
| `dev/adr/ADR-0.8.0-confidence-vs-importance.md` *(planned)* | F9 confidence vs G12 importance — **deferred to post-0.8.0 Slice 46** (experiment/profiling-dependent; HITL-split out of Slice 35 2026-06-06) | **46** produces | n/a (Slice 46) |
| `dev/adr/ADR-0.8.0-fielded-fts-bm25f.md` *(planned)* | F5 fielded FTS / BM25F column model — **deferred to post-0.8.0 Slice 46** (experiment/profiling-dependent; HITL-split out of Slice 35 2026-06-06) | **46** produces | n/a (Slice 46) |
| `dev/adr/ADR-0.6.0-cli-scope.md` | CLI scope = two-root operator surface (`recover` lossy / `doctor` bit-preserving); Option B (`search`/`get`/`list` application query) rejected. Amended 2026-06-06 (Slice 34): scopes the 0.8.0 `doctor dump-mutations` op-store read-back IN under `doctor` as a `dump-*` diagnostic (NOT Option B; application query verbs still require a re-open) | 34 (amendment); reference | 2026-06-06 |
| `dev/adr/ADR-0.6.0-*.md`, `ADR-0.7.0-*.md`, `ADR-0.7.1-*.md` | Prior-release ADRs (typed-write boundary, CLI scope, error taxonomy, etc.) | reference (e.g. typed-write boundary preserved by 25) | (tree) |

## `dev/plans/` — plans + live state

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/plans/0.8.0-implementation.md` | **Authoritative slice contracts** (objective/subagents/lifecycle/success per slice 0–40); Slice 15/20 active-identity language re-scoped to `logical_id` alone by Slice 31 | the plan itself | 2026-06-05 |
| `dev/plans/0.8.0-plan.md` | **Mod-5 ladder + reserved-gap policy + Immediate-Next-Slice pointer + Slice-0/5/10 CLOSED blocks** | 0 authors; every slice advances the pointer | 2026-06-03 |
| `dev/plans/runs/STATUS-0.8.0.md` | **Live state board** (nine §12.5 sections + X1/X2/X3 column + witness + harness contract) | 0 authors; every slice updates at close | 2026-06-03 |
| `dev/plans/prompts/0.8.0-slice-*.md` | Self-contained per-slice subagent prompts | per slice | (per slice) |
| `dev/plans/runs/0.8.0-slice-*-output.json` / `-review-*.md` | Per-slice closure artifacts + promoted codex verdicts | per slice | (per slice) |
| `dev/plans/runs/0.8.0-slice-6-tokenizer-experiment-*.md` | **Slice 6 (B2) FTS5 tokenizer latency experiment** — measured config×tier sweep + engine A/B proving the tokenizer is latency-neutral; recommends tiering AC-012 (10k binding) per the AC-072/073 precedent | Slice 6 (B2) | 2026-06-07 |
| `dev/plans/0.6.x/0.7.x-*.md`, `ci-deferred.md`, `README.md` | Prior-release plans + CI-deferral ledger | reference | (tree) |

---

## `docs/` — user-facing documentation (mkdocs, `nav` in `mkdocs.yml`)

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `docs/index.md` | Docs home | X2 (nav) | 2026-05-17 |
| `docs/getting-started/index.md` | Getting-started overview | — | 2026-05-17 |
| `docs/getting-started/quickstart.md` | Quickstart (five-operation contract) | 5/30 (new surface examples) | 2026-05-17 |
| `docs/install/python.md` | Python install | — | 2026-05-30 |
| `docs/install/typescript.md` | TypeScript install | — | 2026-05-30 |
| `docs/install/rust.md` | Rust install | — | 2026-05-17 |
| `docs/reference/index.md` | API-reference overview | — | 2026-05-17 |
| `docs/reference/python-api.md` | Python API reference (incl. the `read.*` verbs + `NodeRecord`/`OpStoreRow` shapes) | 5 (`SearchHit`), 10 (`SearchFilter`/RRF), 30 (`read.*`), 31 (`logical_id`-alone supersession) | 2026-06-05 |
| `docs/reference/typescript-api.md` | TypeScript API reference (incl. the `read.*` verbs + `NodeRecord`/`OpStoreRow` shapes) | 5 (`SearchHit`), 10 (`SearchFilter`/RRF), 30 (`read.*`), 31 (`logicalId`-alone supersession) | 2026-06-05 |
| `docs/reference/cli.md` | CLI reference (recovery verbs CLI-only); 34 documents the `doctor dump-mutations` op-store read-back diagnostic + `--json` example | 34 (dump-mutations); preserved | 2026-06-06 |
| `docs/reference/errors.md` | Error reference (taxonomy) | per-binding error-class adds | 2026-05-17 |
| `docs/reference/config.md` | Config reference | — | 2026-05-17 |
| `docs/concepts/index.md` | Concepts overview | — | 2026-05-17 |
| `docs/embedder.md` | Default embedder | — | 2026-06-01 |
| `docs/compatibility/index.md` | Compatibility matrix | 40 (compat events) | 2026-05-17 |
| `docs/operations/index.md` | Operations guide | — | 2026-05-01 |
| `docs/guides/index.md` | Guides hub (structured-hit / retrieve examples land here) | 5/30 add examples | 2026-06-04 |
| `docs/guides/structured-search-hits.md` | Structured `SearchHit` usage guide (id/kind/body/score/branch; Py + TS) | 5 (G1); 10 (score → RRF) | 2026-06-03 |
| `docs/guides/retrieve-by-id.md` | Retrieve-by-id guide — `read.get`/`read.get_many` point lookup by `logical_id` (active-only) + `read.collection`/`read.mutations` paginated op-store read-back (mandatory limit + after-id cursor); Py + TS | 30 (G2/G3) | 2026-06-04 |
| `docs/guides/hybrid-search-filtering.md` | Hybrid search guide — G9 RRF ranking (documented behavior-compat event) + G10 `SearchFilter` metadata filtering, Py + TS examples | 10 (G9/G10) | 2026-06-03 |
| `docs/positions/index.md` | Positions hub | — | 2026-05-01 |
| `docs/positions/sdk-parity.md` | Position: SDK parity (guarantee carried forward by 25) | 25 | 2026-05-01 |
| `docs/positions/recovery-surface.md` | Position: recovery surface (denylist, CLI-only) | preserved by 25/30 | 2026-05-01 |
| `docs/positions/tokenizer-policy.md` | Position: tokenizer policy | 5 (FTS5 default upgrade) | 2026-05-01 |
| `docs/positions/embedder-identity.md` | Position: embedder identity | — | 2026-05-01 |
| `docs/release-notes/0.6.0.md` | 0.6.0 release notes | — | 2026-05-17 |
| `docs/release-notes/0.6.1.md` | 0.6.1 release notes (**added to nav at Slice 0** — was orphaned) | X2 | 2026-05-24 |
| `docs/release-notes/0.8.0.md` | 0.8.0 release notes (**stub at Slice 0**; finalized at Slice 40) | 0 stub; 40 finalizes | 2026-06-02 |

## Corpus / eval expansion (out-of-band, owner-managed — integrated at Slice-5 push 2026-06-02)

> These come from the parallel **corpus-work** line (origin/main `83f5156`), integrated into
> `main` when the 0.8.0 campaign was pushed. They are **owner-managed**, not driven by a campaign
> slice; the owner curates/expands these rows. Listed here so DOC-INDEX maps the full shipped doc
> surface (Slice-40 gate m).

| Doc | Purpose | Owning slice/AC | Last-touched |
|-----|---------|-----------------|--------------|
| `dev/corpus-creation/README.md` + `architecture.md` | Corpus-creation overview + architecture | corpus-work (out-of-band) | 2026-06-02 |
| `dev/notes/0.8.x-corpus-source-expansion-research.md` | Corpus source-expansion research notes | corpus-work (0.8.x) | 2026-06-02 |
| `dev/notes/0.8.x-pmc-oa-reconsideration.md` | PMC-OA source reconsideration note | corpus-work (0.8.x) | 2026-06-02 |
| `dev/plans/prompts/0.8.x-corpus-qa-expansion-handoff.md` | Corpus QA-expansion handoff prompt | corpus-work (0.8.x) | 2026-06-02 |
| `dev/plans/prompts/0.8.x-corpus-source-expansion-search.md` | Corpus source-expansion search prompt | corpus-work (0.8.x) | 2026-06-02 |
| `tests/corpus/corpus-card.md` + `README.md` | Eval corpus card + acquisition README (scripts under `tests/corpus/scripts/`) | corpus-work (eval) | 2026-06-02 |
