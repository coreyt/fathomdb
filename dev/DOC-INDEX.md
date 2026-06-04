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
| `dev/acceptance.md` | Acceptance criteria (AC-*); AC-057a five-verb cap superseded by AC-074 (governed surface) | 25 (AC-057a→AC-074); 40 scoreboard | 2026-06-04 |
| `dev/architecture.md` | System architecture (engine, projections, reader pool, surface) | 5/10/15/30 update read-path + receipt surface | 2026-06-03 |
| `dev/test-plan.md` | Test strategy + tiers (incl. functional-harness tier X1 + the Slice 10 G9/G10/G12-recency tier) | 5 adds functional tier; 10 adds RRF/filter/recency tier | 2026-06-03 |
| `dev/traceability.md` | REQ ↔ AC ↔ test trace matrix | 25 re-points REQ-053↔new AC; 30 adds read ACs | 2026-05-28 |
| `dev/security-review.md` | Security review (SR-*) | — (SR-005/SR-011 candidate reserved-gap) | 2026-05-02 |
| `dev/learnings.md` | Cross-phase engineering learnings | per-slice as discovered | 2026-05-31 |
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
| `dev/design/slice-15-g0-design.md` | Slice 15 design memo — G0 canonical-identity substrate: step-12 additive `ALTER` (exemption-marker rationale), tombstone-then-insert supersession + same-txn atomicity, NULL-on-legacy-rows rule, `row_cursors` semantics, op-store cascade, reserved Slice-16 shadow reconciliation, test plan | 15 (G0 keystone) | 2026-06-03 |
| `dev/design/slice-20-g8-design.md` | Slice 20 design memo — G8 dangling-edge flag-and-count: cross-row post-row-insert EXISTS pass inside `commit_batch`'s open tx (why not `validate_write`), logical_id-alone probe + step-12 partial-index hit argument, legacy-NULL endpoint consequence, flag-and-count default (strict-mode deferred to band 22), test plan | 20 (G8/F10) | 2026-06-03 |
| `dev/design/slice-25-conformance-design.md` | Slice 25 design memo — governed-surface conformance rewrite: the allowlist (core 5 + `read.*` 4), the four falsifiable properties (P1 allowlist-membership · P2 cross-binding parity · P3 recovery-denylist empty-intersection · P4 no-raw-SQL), the honest-green plan for not-yet-live `read.*`, touch-points | 25 (AC-057a→AC-074) | 2026-06-04 |
| `dev/design/agent-memory-impl-strategy.md` | Slice shapes / impl strategy for the gap ladder | 5/10/15/20/30 shapes | 2026-06-02 |
| `dev/design/retrieval.md` | Retrieval pipeline design (vector + FTS5, fusion) | 5/10 | (tree) |
| `dev/design/projections.md` | Projection model | 5/15 | (tree) |
| `dev/design/migrations.md` | Migration model (forward-only, accretion guard) | 5/15 (schema 10→11) | (tree) |
| `dev/design/vector.md`, `ann-index-vec0.md` | Vector store / vec0 ANN index | 10/15 | (tree) |
| `dev/design/op-store.md` | Operational mutation store | 30 (`read.collection`/`read.mutations`) | (tree) |
| `dev/design/engine.md`, `lifecycle.md`, `scheduler.md`, `recovery.md`, `errors.md`, `embedder.md`, `embedder-decision.md`, `release.md`, `perf-gates.md`, `perf-regression-detection.md`, `0.7.0-vector-quant-pack1.md`, `0.7.1-EU-6-FIX-*.md` | Engine/lifecycle/scheduler/recovery/error/embedder/release/perf design notes | per-slice as touched | (tree) |

## `dev/adr/` — architecture decision records

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/adr/README.md`, `ADR-0.6.0-decision-index.md` | ADR index | — | (tree) |
| `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md` | Supersede AC-057a's five-verb cap → governed surface; **status: SIGNED/accepted** (Q1–Q5 = A1/B1/amend/confirm/**BIND-RUST**; Rust pin → Slice 27) | advanced 0.b; signed 2026-06-03; executed at 25; gates 30 | 2026-06-03 |
| `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` | NEW (0.a) — canonical identity substrate (logical_id+superseded_at, Option 2A) | authored at 0.a; gates 15 | 2026-06-02 |
| `dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md` | Retrieval+identity ADR (Q1 table-stakes, Q3 RRF compat); gates Slice 10 | gates 10 | 2026-06-02 |
| `dev/adr/ADR-0.8.0-embedder-identity-change-workflow.md` | Embedder-identity change workflow | — | (tree) |
| `dev/adr/ADR-0.8.0-graph-traversal-scope.md` *(planned)* | F1/G5/G6 graph traversal scope | **35** produces | n/a (Slice 35) |
| `dev/adr/ADR-0.8.0-filter-grammar.md` *(planned)* | G4/F3 closed typed filter enum (shared with G10) | **35** produces | n/a (Slice 35) |
| `dev/adr/ADR-0.8.0-confidence-vs-importance.md` *(planned)* | F9 confidence vs G12 importance | **35** produces | n/a (Slice 35) |
| `dev/adr/ADR-0.8.0-fielded-fts-bm25f.md` *(planned)* | F5 fielded FTS / BM25F column model | **35** produces | n/a (Slice 35) |
| `dev/adr/ADR-0.6.0-*.md`, `ADR-0.7.0-*.md`, `ADR-0.7.1-*.md` | Prior-release ADRs (typed-write boundary, CLI scope, error taxonomy, etc.) | reference (e.g. typed-write boundary preserved by 25) | (tree) |

## `dev/plans/` — plans + live state

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `dev/plans/0.8.0-implementation.md` | **Authoritative slice contracts** (objective/subagents/lifecycle/success per slice 0–40) | the plan itself | 2026-06-03 |
| `dev/plans/0.8.0-plan.md` | **Mod-5 ladder + reserved-gap policy + Immediate-Next-Slice pointer + Slice-0/5/10 CLOSED blocks** | 0 authors; every slice advances the pointer | 2026-06-03 |
| `dev/plans/runs/STATUS-0.8.0.md` | **Live state board** (nine §12.5 sections + X1/X2/X3 column + witness + harness contract) | 0 authors; every slice updates at close | 2026-06-03 |
| `dev/plans/prompts/0.8.0-slice-*.md` | Self-contained per-slice subagent prompts | per slice | (per slice) |
| `dev/plans/runs/0.8.0-slice-*-output.json` / `-review-*.md` | Per-slice closure artifacts + promoted codex verdicts | per slice | (per slice) |
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
| `docs/reference/python-api.md` | Python API reference | 5 (`SearchHit`), 10 (`SearchFilter`/RRF), 30 (`read.*`) | 2026-06-03 |
| `docs/reference/typescript-api.md` | TypeScript API reference | 5 (`SearchHit`), 10 (`SearchFilter`/RRF), 30 (`read.*`) | 2026-06-03 |
| `docs/reference/cli.md` | CLI reference (recovery verbs CLI-only) | preserved | 2026-05-17 |
| `docs/reference/errors.md` | Error reference (taxonomy) | per-binding error-class adds | 2026-05-17 |
| `docs/reference/config.md` | Config reference | — | 2026-05-17 |
| `docs/concepts/index.md` | Concepts overview | — | 2026-05-17 |
| `docs/embedder.md` | Default embedder | — | 2026-06-01 |
| `docs/compatibility/index.md` | Compatibility matrix | 40 (compat events) | 2026-05-17 |
| `docs/operations/index.md` | Operations guide | — | 2026-05-01 |
| `docs/guides/index.md` | Guides hub (structured-hit / retrieve examples land here) | 5/30 add examples | 2026-06-02 |
| `docs/guides/structured-search-hits.md` | Structured `SearchHit` usage guide (id/kind/body/score/branch; Py + TS) | 5 (G1); 10 (score → RRF) | 2026-06-03 |
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
