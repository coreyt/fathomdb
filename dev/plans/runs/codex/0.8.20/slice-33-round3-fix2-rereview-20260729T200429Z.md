OpenAI Codex v0.136.0
--------
workdir: /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
model: gpt-5.5
provider: openai
approval: never
sandbox: danger-full-access
reasoning effort: medium
reasoning summaries: none
session id: 019faf7a-84e6-79f1-9bb5-4eb1399991b3
--------
user
changes against '29c2eae0'
exec
/bin/bash -lc 'cat MEMORY.md 2>/dev/null || true && ls feedback_*.md 2>/dev/null' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 exited 2 in 0ms:
exec
/bin/bash -lc 'git diff --stat 29c2eae00ba12c2929e4cd138eefaf7e74db720c && git diff --name-only 29c2eae00ba12c2929e4cd138eefaf7e74db720c' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
 dev/DOC-INDEX.md                                   |    2 +
 .../0.8.20-slice-33-library-sweep-3-FINDINGS.md    |  587 ++++
 .../0.8.20/slice-33-round1-20260729T193509Z.md     | 3347 ++++++++++++++++++++
 ...ice-33-round2-fix1-rereview-20260729T193712Z.md | 3067 ++++++++++++++++++
 scripts/sbom-survey/README.md                      |   22 +
 scripts/sbom-survey/smoke-install-run.sh           |  349 ++
 6 files changed, 7374 insertions(+)
dev/DOC-INDEX.md
dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md
scripts/sbom-survey/README.md
scripts/sbom-survey/smoke-install-run.sh

exec
/bin/bash -lc "pwd; ls -la; cat MEMORY.md; find . -maxdepth 1 -name 'feedback_*.md' -print -exec cat {} \\;" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
/home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
total 356
drwxrwxr-x 12 coreyt coreyt   4096 Jul 29 14:43 .
drwxrwxr-x  5 coreyt coreyt   4096 Jul 29 14:12 ..
drwxrwxr-x  5 coreyt coreyt   4096 Jul 29 14:12 .claude
-rw-rw-r--  1 coreyt coreyt     72 Jul 29 14:12 .git
drwxrwxr-x  3 coreyt coreyt   4096 Jul 29 14:12 .github
-rw-rw-r--  1 coreyt coreyt   6365 Jul 29 14:12 .gitignore
-rw-rw-r--  1 coreyt coreyt   2745 Jul 29 14:12 .markdownlint-cli2.jsonc
-rw-rw-r--  1 coreyt coreyt   2105 Jul 29 14:12 .markdownlint.jsonc
-rw-rw-r--  1 coreyt coreyt    657 Jul 29 14:12 .prettierignore
-rw-rw-r--  1 coreyt coreyt    296 Jul 29 14:12 .prettierrc.json
-rw-rw-r--  1 coreyt coreyt  14209 Jul 29 14:12 AGENTS.md
-rw-rw-r--  1 coreyt coreyt  64815 Jul 29 14:12 CHANGELOG.md
-rw-rw-r--  1 coreyt coreyt 115594 Jul 29 14:12 Cargo.lock
-rw-rw-r--  1 coreyt coreyt   2434 Jul 29 14:12 Cargo.toml
-rw-rw-r--  1 coreyt coreyt   1078 Jul 29 14:12 LICENSE
-rw-rw-r--  1 coreyt coreyt    939 Jul 29 14:12 README.md
-rw-rw-r--  1 coreyt coreyt    120 Jul 29 14:12 deny.toml
drwxrwxr-x 30 coreyt coreyt   4096 Jul 29 14:12 dev
drwxrwxr-x 11 coreyt coreyt   4096 Jul 29 14:12 docs
drwxrwxr-x  2 coreyt coreyt   4096 Jul 29 14:12 experiments
-rw-rw-r--  1 coreyt coreyt   1783 Jul 29 14:12 lychee.toml
-rw-rw-r--  1 coreyt coreyt   1502 Jul 29 14:12 mkdocs.yml
-rw-rw-r--  1 coreyt coreyt  48768 Jul 29 14:12 package-lock.json
-rw-rw-r--  1 coreyt coreyt   1249 Jul 29 14:12 package.json
-rw-rw-r--  1 coreyt coreyt     66 Jul 29 14:12 rust-toolchain.toml
-rw-rw-r--  1 coreyt coreyt     45 Jul 29 14:12 rustfmt.toml
drwxrwxr-x 10 coreyt coreyt   4096 Jul 29 14:12 scripts
drwxrwxr-x  6 coreyt coreyt   4096 Jul 29 14:12 src
drwxrwxr-x  6 coreyt coreyt   4096 Jul 29 14:12 test
drwxrwxr-x  4 coreyt coreyt   4096 Jul 29 14:12 tests
drwxrwxr-x  3 coreyt coreyt   4096 Jul 29 14:12 tools
cat: MEMORY.md: No such file or directory

exec
/bin/bash -lc 'git diff --unified=80 29c2eae00ba12c2929e4cd138eefaf7e74db720c -- scripts/sbom-survey/smoke-install-run.sh scripts/sbom-survey/README.md dev/DOC-INDEX.md' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
diff --git a/dev/DOC-INDEX.md b/dev/DOC-INDEX.md
index 556e8825..9d2825d9 100644
--- a/dev/DOC-INDEX.md
+++ b/dev/DOC-INDEX.md
@@ -61,160 +61,162 @@ refresh in the closing commit when you touch a doc).
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
+| `scripts/sbom-survey/smoke-install-run.sh` | TC-115 install-then-run smoke — installs the tool into a throwaway venv, invokes the INSTALLED console script, and asserts its artifacts are byte-identical to a source-tree run. Deliberately NOT CI-wired (`seq-172`) | 0.8.20 Slice 33 (Library Sweep #3 leg 3/3) | 2026-07-29 |
+| `dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` | **Findings of record** for Library Sweep #3 — the ONLINE `sbom-survey` run at `29c2eae0`: 774 components, 28 direct outdated, per-dependency surgical verdicts, and the hand-off to 0.8.22. ASCERTAIN-ONLY; applied nothing | 0.8.20 Slice 33 (Library Sweep #3 leg 3/3; no requirement id, TC-76) | 2026-07-29 |
 
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
diff --git a/scripts/sbom-survey/README.md b/scripts/sbom-survey/README.md
index 5eaef018..5fefe028 100644
--- a/scripts/sbom-survey/README.md
+++ b/scripts/sbom-survey/README.md
@@ -52,113 +52,135 @@ per the `AC-SBOM-23` precedent in design §4, so every existing id keeps its mea
 
 ## Running the tool
 
 ```bash
 sbom-survey --repo PATH [--offline | --online] [--out DIR] [--tiers FILE] [--now ISO8601]
 sbom-survey --describe
 ```
 
 **The tier rules are read from the SURVEYED repository — `<repo>/scripts/sbom-survey/tiers.toml` —
 never from the installed package.** They are data *about a repository* (every rule is a path prefix
 into the surveyed tree), so a copy baked into a wheel would describe whichever repository the wheel
 was built from. It also keeps `AC-SBOM-08` / `AC-SBOM-11` grading the very file the survey consumed.
 
 Consequences worth knowing before Slice 33 runs this:
 
 - `pip install ./scripts/sbom-survey` followed by `sbom-survey --repo <that repo> --offline --out DIR`
   works and exits `0`. (Before fix-2 it exited `1` with a bare `FileNotFoundError`, because the
   default was resolved relative to the *package* and no package data is declared.)
 - Surveying a repository that does **not** track `scripts/sbom-survey/tiers.toml` requires an
   explicit `--tiers FILE`. That is deliberate: §5.3 rules there is **no catch-all tier rule**, so
   guessing a rule set for an unknown repository would be the silent mis-tag REQ-4 exists to prevent.
   The tool exits `3` and names both the file it wanted and the `--tiers` override.
 
 **Timestamps are validated, never substituted.** `--now` and `SOURCE_DATE_EPOCH` both pass through
 one function (`util.parse_timestamp`), and so do `staleness.json`'s `generated` and the CycloneDX
 `metadata.timestamp` — so the two artifacts of a single run cannot disagree about when it happened.
 A malformed value is **rejected** rather than quietly replaced by the default epoch: this value is
 the provenance of the whole run, and §5.6 makes the findings doc's provenance header load-bearing.
 
 | Exit | Meaning |
 |---|---|
 | `0` | survey written |
 | `2` | a tracked manifest has no tier assignment (an offending path on stderr) |
 | `3` | a tracked manifest could not be parsed, or the tier rules could not be read |
 | `1` | unexpected internal error |
 | `64` | bad command line (`EX_USAGE`) — e.g. a malformed `--now` |
 
 §5.9 rules the first four. `64` is deliberately **outside** that set: a bad argument is none of those
 things, and argparse's own default for a usage error is `2`, which would collide head-on with
 "untiered manifest" and make `AC-SBOM-21`'s signal ambiguous for anyone reading exit codes.
 
 ## Running the suite
 
 The mini-project is deliberately **not installed**: `tests/conftest.py` puts `scripts/sbom-survey/`
 on `sys.path` itself (and on `PYTHONPATH` for the CLI subprocess). Only the third-party dependencies
 declared in `pyproject.toml` need to be importable by the interpreter running pytest — build a venv
 for them **outside the repository tree**, and never install anything into the repo's shared `.venv`.
 
 ```bash
 python3 -m venv /tmp/sbom-survey-venv
 /tmp/sbom-survey-venv/bin/pip install \
   'cyclonedx-python-lib[json-validation]>=8.0,<9.0' 'packageurl-python>=0.15,<1.0' \
   'packaging>=24.0,<26.0' 'semver>=3.0,<4.0' 'pytest>=8.0,<10.0'
 /tmp/sbom-survey-venv/bin/python -m pytest scripts/sbom-survey/tests -q
 ```
 
 Expected: **`24 passed, 0 failed, 0 skipped, 0 errors`** (exit code `0`).
 
 - **No test may skip.** A skip is a vacuous green — an ungraded criterion reporting success.
 - **No module-level `import sbom_survey`.** The import happens inside each test body via the
   `require()` helper in `tests/conftest.py`, so a broken package produces 24 attributable FAILEDs
   rather than one collection error that hides 23 of them.
 - The suite needs **no network**. The published-version lookup is behind an injectable seam; the
   tests inject `OfflineSource` / `StaticSource`, and one test asserts zero socket I/O.
 - **`AC-SBOM-10` grades CycloneDX validity with an INDEPENDENT validator** — the upstream
   `cyclonedx-python-lib[json-validation]` one, plus a known-invalid negative control — never with
   `sbom_survey.cyclonedx.validate()`, which would be self-certification. From Slice 32 that
   distribution must be installed: if it is missing the criterion **FAILS** naming what to install.
   It does **not** skip, because an ungraded criterion is a green that means nothing (design §5.7).
 - **TC-97.** The only pytest *settings* in this repository live in `src/python/pyproject.toml`, whose
   `pythonpath = ["."]` shadows an installed wheel. That file is **not** an ancestor of
   `scripts/sbom-survey/tests`, so this suite can never inherit it. Since Slice 32 the header does
   print `configfile: pyproject.toml`, pointing at **this directory's own** `pyproject.toml`: from
   pytest 8.1 a `pyproject.toml` found while walking up from the test arguments becomes the rootdir
   anchor even when it carries no `[tool.pytest.ini_options]` table, in which case the applied
   settings are the empty dict (verified: `config.inicfg == {}`, `config.getini("pythonpath") == []`).
   **Do not add a `[tool.pytest.ini_options]` table here, and do not add a `pyproject.toml`,
   `pytest.ini`, `tox.ini` or `setup.cfg` above this directory** — either would start applying real
   settings to this suite.
 
+## Install-then-run smoke (TC-115)
+
+`scripts/sbom-survey/smoke-install-run.sh` guards the **install path**, which the suite above does
+not reach: **installing is not verifying an install — invoking what was installed is.** It builds a
+throwaway venv **outside** the repo, `pip install`s this project, runs the **installed console
+script**, then re-runs the same survey from the source tree and asserts the two agree.
+
+```bash
+bash scripts/sbom-survey/smoke-install-run.sh     # exit 0 = PASS
+```
+
+It asserts: the console-script file exists and is executable · both runs exit `0` · **two symmetric
+provenance** checks — the installed run resolves inside the venv's `site-packages` (invoked under
+`env -u PYTHONPATH -u PYTHONHOME`, so an ambient `PYTHONPATH` cannot smuggle the source tree into
+the "installed" leg) and the source run resolves under `scripts/sbom-survey` (not the still-installed
+copy) · identical artifact **sets** · byte-identical `sbom.cdx.json` / `staleness.json` /
+`staleness.md` · and a **vacuity guard** (`summary.components > 0`, non-empty `rows`) — two empty
+files are byte-identical. Only the `pip install` needs network; both surveys run `--offline`.
+
+**It is deliberately NOT CI-wired** (steward `seq-172` ruled wiring out, not deferred) — run it by
+hand, and do not add it to `agent-test.sh` or `ci.yml`.
+
 ## Deliberately NOT wired into CI
 
 This tool is **recurring by design and NOT CI-gating** — it is **informational**
 (`plan-0.8.20.md` §3a, HITL 2026-07-29, steward `seq-153`).
 
 - It is **not** registered in `scripts/agent-test.sh`.
 - It is **not** referenced by `.github/workflows/ci.yml`.
 - It is **not** part of `scripts/agent-verify.sh`, `scripts/check.sh`, or any lint/typecheck scope
   (`ruff` and `pyright` are scoped to `src/python`).
 
 Do not wire it in. **The suite being GREEN does not authorize wiring**: the standing HITL ruling
 (`seq-166`) is that the suite may be wired only when it is green **AND** the HITL has approved —
 both required, neither sufficient. `tests/test_cli.py::test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring`
 is the standing guard: it greps both wiring files and fails if either grows a reference, so it stays
 green precisely **because the wiring is absent**.
 
 ## Isolation
 
 The mini-project's own `pyproject.toml` (a Slice 32 artifact) is standalone: not a Cargo workspace
 member, not referenced by `src/python/pyproject.toml`, not a dependency of the root `package.json`.
 It can never enlarge the published dependency graph or the advisory backlog. Its own dependencies
 are surveyed by the tool and tagged `dev-tooling` — the tool appears in its own SBOM, by design.
 
 Generated reports go to `scripts/sbom-survey/out/`, which Slice 32 added to `.gitignore`. Slice 33's
 **findings** have a separate tracked home:
 `dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` — the house convention for a dated run
 report, weighed against `dev/design/` and `dev/deps/` in design §5.6.
 
 ## Scope guard
 
 The tool **never** applies a dependency bump and **never** edits a manifest or a lockfile. Its only
 write path is its own gitignored output directory. The survey is an **input to 0.8.22**, which owns
 the actual upgrades.
diff --git a/scripts/sbom-survey/smoke-install-run.sh b/scripts/sbom-survey/smoke-install-run.sh
new file mode 100755
index 00000000..52f4560f
--- /dev/null
+++ b/scripts/sbom-survey/smoke-install-run.sh
@@ -0,0 +1,349 @@
+#!/usr/bin/env bash
+# TC-115 — install-then-run smoke for `sbom-survey` (0.8.20 Slice 33).
+#
+# WHY THIS EXISTS
+# ---------------
+# Steward `seq-172` ruled CI wiring for this tool **OUT** — not deferred, out.
+# This script is therefore the ONLY guard for the install-path defect class, and
+# it is run by hand.
+#
+# On Slice 32 both an implementer and the orchestrator made the same error:
+# **installing is not verifying an install — invoking what was installed is.**
+# A `pip install` that exits 0 proves a wheel built; it proves nothing about the
+# console script, the entry point, or the package's importability from site-
+# packages. This script closes exactly that gap: it installs into a throwaway
+# venv OUTSIDE the repository, invokes the INSTALLED console script, and then
+# proves the source tree produces byte-identical artifacts.
+#
+# WHAT IT ASSERTS
+#   A. the installed console script FILE exists and is executable;
+#   B. PROVENANCE (RUN A) — under a scrubbed import environment, the installed
+#      `sbom_survey` resolves inside the venv's site-packages and NOT under the
+#      repo. An ambient `PYTHONPATH` would otherwise make the "installed" run
+#      import the source tree, passing while the wheel is broken;
+#   C. RUN A — `$VENV/bin/sbom-survey` (the real entry point) exits 0;
+#   D. PROVENANCE (RUN B) — after uninstalling, `import sbom_survey` resolves
+#      under the repo source tree, so RUN B genuinely exercises the tree and the
+#      identity check below cannot be vacuously true against the still-installed
+#      copy (TC-105: Slice 31's dominant defect class was a criterion graded
+#      against a helper while the real boundary went ungraded);
+#   E. RUN B — `python -m sbom_survey` from the source tree exits 0;
+#   F. the artifact SETS are identical (an extra/missing file is caught too);
+#   G. all three artifacts are byte-identical between the two runs;
+#   H. VACUITY GUARD — two empty files are byte-identical, so the run is only
+#      believed when `summary.components > 0` and `rows` is non-empty.
+#
+# DELIBERATELY NOT CI-WIRED (`seq-172`). Do not add it to `scripts/agent-test.sh`,
+# `.github/workflows/ci.yml`, `scripts/agent-verify.sh` or `scripts/check.sh` —
+# `AC-SBOM-19` asserts the absence of any `sbom-survey` reference in the wiring
+# files and must stay green.
+#
+# NETWORK: the `pip install` step needs PyPI. The survey runs themselves are
+# `--offline` and consult no registry.
+#
+# USAGE:  bash scripts/sbom-survey/smoke-install-run.sh
+# EXIT:   0 = PASS, non-zero = a real defect (the diagnostic names which one).
+
+set -euo pipefail
+
+# --- 1. repo root, resolved from this script's own location (never hardcoded) --
+SCRIPT_DIR="$(dirname "$0")"
+REPO="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
+PROJECT="$REPO/scripts/sbom-survey"
+
+echo "smoke: repo    = $REPO"
+echo "smoke: project = $PROJECT"
+
+# --- 2. scrub stale build products BEFORE installing ---------------------------
+# A stale `build/` tree makes setuptools package OLD code into the wheel. That
+# cost Slice 32 an entire verification cycle chasing a phantom. One destructive
+# `rm -rf` per statement; never `find -delete`.
+scrub_build_tree() {
+    if [ -d "$PROJECT/build" ]; then
+        rm -rf "$PROJECT/build"
+    fi
+    local egg
+    shopt -s nullglob
+    for egg in "$PROJECT"/*.egg-info; do
+        rm -rf "$egg"
+    done
+    shopt -u nullglob
+}
+
+scrub_build_tree
+echo "smoke: scrubbed build/ and *.egg-info/ before install"
+
+# --- 3. work dir, asserted OUTSIDE the repo ------------------------------------
+WORK="$(mktemp -d)"
+WORK_REAL="$(cd "$WORK" && pwd -P)"
+REPO_REAL="$(cd "$REPO" && pwd -P)"
+case "$WORK_REAL/" in
+    "$REPO_REAL"/*)
+        echo "smoke: FAIL — work dir $WORK_REAL is INSIDE the repo $REPO_REAL." >&2
+        echo "smoke:        a venv inside the repo tree is the trap this guards." >&2
+        rm -rf "$WORK"
+        exit 1
+        ;;
+esac
+echo "smoke: work    = $WORK (verified outside the repo)"
+
+cleanup() {
+    local rc=$?
+    if [ -d "$WORK" ]; then
+        rm -rf "$WORK"
+    fi
+    # Leave the tree as we found it.
+    scrub_build_tree
+    exit "$rc"
+}
+trap cleanup EXIT
+
+VENV="$WORK/venv"
+
+# --- 4. venv + install ---------------------------------------------------------
+set +e
+python3 -m venv "$VENV"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — python3 -m venv exited rc=$rc" >&2
+    exit 1
+fi
+
+echo "smoke: installing $PROJECT into $VENV (needs PyPI) ..."
+set +e
+"$VENV/bin/pip" install --disable-pip-version-check "$PROJECT"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — pip install exited rc=$rc." >&2
+    echo "smoke:        The most likely cause is PyPI being unreachable: this is" >&2
+    echo "smoke:        the ONE step that needs the network (the survey runs" >&2
+    echo "smoke:        themselves are --offline). Re-run with network access" >&2
+    echo "smoke:        before treating this as a defect in the tool." >&2
+    exit 1
+fi
+
+# --- 5. the console script FILE must exist and be executable -------------------
+CONSOLE="$VENV/bin/sbom-survey"
+if [ ! -f "$CONSOLE" ]; then
+    echo "smoke: FAIL — console script $CONSOLE was not created by the install." >&2
+    echo "smoke:        [project.scripts] in pyproject.toml is not taking effect." >&2
+    exit 1
+fi
+if [ ! -x "$CONSOLE" ]; then
+    echo "smoke: FAIL — console script $CONSOLE exists but is not executable." >&2
+    exit 1
+fi
+echo "smoke: console script present and executable: $CONSOLE"
+
+OUT_INSTALLED="$WORK/out-installed"
+OUT_SOURCE="$WORK/out-source"
+
+# --- 6a. PROVENANCE ASSERTION FOR RUN A — symmetric with RUN B's (codex §9 rd 2)
+#
+# RUN A inherits the caller's environment, and `PYTHONPATH` beats site-packages
+# on `sys.path`. So an ambient `PYTHONPATH` pointing at THIS checkout (or any
+# checkout carrying `sbom_survey`) makes the installed console script import the
+# SOURCE TREE — and the smoke then passes while the installed wheel is broken,
+# incomplete, or missing files entirely. That is a vacuous pass on the exact leg
+# this script exists to prove.
+#
+# Two things are needed, and only the second is a guard:
+#   * RUN A is invoked under `env -u PYTHONPATH -u PYTHONHOME` — scrubbed
+#     PER-INVOCATION, never globally, because RUN B *needs* `PYTHONPATH`;
+#   * and that arrangement is ASSERTED here. Unsetting only ARRANGES for the
+#     right thing; the assertion PROVES it. RUN B's provenance was graded from
+#     the start and RUN A's was not — that asymmetry was the finding.
+set +e
+SITE_PACKAGES="$(env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ] || [ -z "$SITE_PACKAGES" ]; then
+    echo "smoke: FAIL — could not resolve the venv's site-packages (rc=$rc)." >&2
+    exit 1
+fi
+
+set +e
+RESOLVED_A="$(env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — the INSTALLED sbom_survey is not importable from the venv (rc=$rc)." >&2
+    echo "smoke:        pip install reported success, so the wheel does not contain" >&2
+    echo "smoke:        an importable package. This is the install-path defect." >&2
+    exit 1
+fi
+case "$RESOLVED_A" in
+    "$REPO"/*)
+        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
+        echo "smoke:        $RESOLVED_A" >&2
+        echo "smoke:        which is INSIDE the repo ($REPO), not the venv's" >&2
+        echo "smoke:        site-packages ($SITE_PACKAGES). RUN A would exercise the" >&2
+        echo "smoke:        SOURCE TREE, so a PASS would say nothing about the wheel." >&2
+        exit 1
+        ;;
+esac
+case "$RESOLVED_A" in
+    "$SITE_PACKAGES"/*)
+        echo "smoke: provenance OK (RUN A) — installed sbom_survey resolves to $RESOLVED_A"
+        ;;
+    *)
+        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
+        echo "smoke:        $RESOLVED_A" >&2
+        echo "smoke:        expected a path under the venv's site-packages:" >&2
+        echo "smoke:        $SITE_PACKAGES" >&2
+        exit 1
+        ;;
+esac
+
+# --- 6b. RUN A — the INSTALLED path, the real entry point -----------------------
+# `env -u PYTHONPATH -u PYTHONHOME` per-invocation: the same scrubbed environment
+# the assertion above was made under, so what was proved is what runs.
+echo "smoke: RUN A — installed console script"
+set +e
+env -u PYTHONPATH -u PYTHONHOME "$CONSOLE" --repo "$REPO" --offline --out "$OUT_INSTALLED"
+rc_a=$?
+set -e
+if [ "$rc_a" -ne 0 ]; then
+    echo "smoke: FAIL — RUN A (installed console script) exited rc=$rc_a, expected 0." >&2
+    exit 1
+fi
+echo "smoke: RUN A rc=$rc_a"
+
+# --- 7a. uninstall, so the code must now come from the tree --------------------
+# Dependencies stay installed; only the `sbom-survey` distribution goes.
+set +e
+"$VENV/bin/pip" uninstall -y --disable-pip-version-check sbom-survey
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — pip uninstall sbom-survey exited rc=$rc." >&2
+    exit 1
+fi
+
+# --- 8. PROVENANCE ASSERTION (RUN B) — it must really be the source tree -------
+# Without this, RUN B could silently still be the installed copy and the
+# byte-identity check below would be vacuously true. The mirror image of §6a:
+# there the repo must NOT be on the import path, here it must be.
+set +e
+RESOLVED="$(PYTHONPATH="$PROJECT" "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — could not import sbom_survey from the source tree (rc=$rc)." >&2
+    exit 1
+fi
+case "$RESOLVED" in
+    "$PROJECT"/*)
+        echo "smoke: provenance OK (RUN B) — sbom_survey resolves to $RESOLVED"
+        ;;
+    *)
+        echo "smoke: FAIL — provenance (RUN B). sbom_survey resolved to:" >&2
+        echo "smoke:        $RESOLVED" >&2
+        echo "smoke:        expected a path under $PROJECT. RUN B would have been" >&2
+        echo "smoke:        the installed copy again, making the byte-identity" >&2
+        echo "smoke:        assertion vacuously true." >&2
+        exit 1
+        ;;
+esac
+
+# --- 7b. RUN B — the SOURCE-TREE path ------------------------------------------
+echo "smoke: RUN B — source tree via python -m sbom_survey"
+set +e
+PYTHONPATH="$PROJECT" "$VENV/bin/python" -m sbom_survey --repo "$REPO" --offline --out "$OUT_SOURCE"
+rc_b=$?
+set -e
+if [ "$rc_b" -ne 0 ]; then
+    echo "smoke: FAIL — RUN B (source tree) exited rc=$rc_b, expected 0." >&2
+    exit 1
+fi
+echo "smoke: RUN B rc=$rc_b"
+
+# --- 9. the artifact SETS must be identical ------------------------------------
+# Compare sorted listings, so an EXTRA or MISSING file is caught, not just
+# differing content of the three files we go on to compare.
+set +e
+SET_A="$(cd "$OUT_INSTALLED" && ls -A | LC_ALL=C sort)"
+SET_B="$(cd "$OUT_SOURCE" && ls -A | LC_ALL=C sort)"
+set -e
+if [ "$SET_A" != "$SET_B" ]; then
+    echo "smoke: FAIL — the two runs wrote DIFFERENT artifact sets." >&2
+    echo "smoke:        installed ($OUT_INSTALLED):" >&2
+    printf '%s\n' "$SET_A" | sed 's/^/smoke:          /' >&2
+    echo "smoke:        source ($OUT_SOURCE):" >&2
+    printf '%s\n' "$SET_B" | sed 's/^/smoke:          /' >&2
+    exit 1
+fi
+echo "smoke: artifact sets identical:"
+printf '%s\n' "$SET_A" | sed 's/^/smoke:   /'
+
+# --- 10. the three artifacts must exist in BOTH and be byte-identical ----------
+ARTIFACTS="sbom.cdx.json staleness.json staleness.md"
+for name in $ARTIFACTS; do
+    if [ ! -f "$OUT_INSTALLED/$name" ]; then
+        echo "smoke: FAIL — $name missing from the INSTALLED run's output dir." >&2
+        exit 1
+    fi
+    if [ ! -f "$OUT_SOURCE/$name" ]; then
+        echo "smoke: FAIL — $name missing from the SOURCE run's output dir." >&2
+        exit 1
+    fi
+    if ! cmp -s "$OUT_INSTALLED/$name" "$OUT_SOURCE/$name"; then
+        echo "smoke: FAIL — $name DIFFERS between the installed run and the source run." >&2
+        echo "smoke:        installed: $OUT_INSTALLED/$name" >&2
+        echo "smoke:        source:    $OUT_SOURCE/$name" >&2
+        echo "smoke:        first 20 diff lines:" >&2
+        diff "$OUT_INSTALLED/$name" "$OUT_SOURCE/$name" 2>&1 | head -20 | sed 's/^/smoke:        /' >&2
+        exit 1
+    fi
+    echo "smoke: byte-identical: $name"
+done
+
+# --- 11. VACUITY GUARD — two empty files are byte-identical --------------------
+# Read the INSTALLED run's staleness.json with the venv interpreter and stdlib
+# `json` (no jq dependency). A survey that found nothing must never PASS.
+set +e
+COMPONENTS="$(
+    "$VENV/bin/python" - "$OUT_INSTALLED/staleness.json" <<'PY'
+import json
+import sys
+
+path = sys.argv[1]
+with open(path, encoding="utf-8") as handle:
+    doc = json.load(handle)
+
+summary = doc.get("summary") or {}
+components = summary.get("components")
+rows = doc.get("rows")
+
+if not isinstance(components, int) or components <= 0:
+    print(
+        f"VACUOUS: summary.components is {components!r}, expected a positive int",
+        file=sys.stderr,
+    )
+    raise SystemExit(1)
+if not isinstance(rows, list) or not rows:
+    print(
+        f"VACUOUS: rows is {type(rows).__name__} of length "
+        f"{len(rows) if isinstance(rows, list) else 'n/a'}, expected a non-empty list",
+        file=sys.stderr,
+    )
+    raise SystemExit(1)
+
+print(components)
+PY
+)"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — VACUITY GUARD. The two runs agree, but they agree on" >&2
+    echo "smoke:        nothing: $OUT_INSTALLED/staleness.json reports no" >&2
+    echo "smoke:        components and/or no rows. A byte-identity PASS over" >&2
+    echo "smoke:        empty artifacts certifies nothing." >&2
+    exit 1
+fi
+
+# --- 12. PASS -----------------------------------------------------------------
+echo "smoke: PASS — installed run rc=$rc_a, source run rc=$rc_b, artifacts byte-identical over ${COMPONENTS} components (sbom.cdx.json, staleness.json, staleness.md); both provenance guards (installed=site-packages, source=tree) and the vacuity guard held."
+exit 0

exec
/bin/bash -lc "find scripts/sbom-survey -maxdepth 3 -type f -print | sort && sed -n '1,240p' scripts/sbom-survey/pyproject.toml && find scripts/sbom-survey -maxdepth 2 -type f -not -path '*/.git/*' -print | grep -E 'sbom_survey|py"'$'"' | xargs -r sed -n '1,80p'" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
scripts/sbom-survey/.pytest_cache/.gitignore
scripts/sbom-survey/.pytest_cache/CACHEDIR.TAG
scripts/sbom-survey/.pytest_cache/README.md
scripts/sbom-survey/README.md
scripts/sbom-survey/out/sbom.cdx.json
scripts/sbom-survey/out/staleness.json
scripts/sbom-survey/out/staleness.md
scripts/sbom-survey/pyproject.toml
scripts/sbom-survey/sbom_survey/__init__.py
scripts/sbom-survey/sbom_survey/__main__.py
scripts/sbom-survey/sbom_survey/__pycache__/__init__.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/__main__.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/cli.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/constraints.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/cyclonedx.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/discovery.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/paths.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/registry.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/report.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/survey.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/tiers.cpython-312.pyc
scripts/sbom-survey/sbom_survey/__pycache__/util.cpython-312.pyc
scripts/sbom-survey/sbom_survey/cli.py
scripts/sbom-survey/sbom_survey/constraints.py
scripts/sbom-survey/sbom_survey/cyclonedx.py
scripts/sbom-survey/sbom_survey/discovery.py
scripts/sbom-survey/sbom_survey/parse/__init__.py
scripts/sbom-survey/sbom_survey/parse/cargo.py
scripts/sbom-survey/sbom_survey/parse/npm.py
scripts/sbom-survey/sbom_survey/parse/python.py
scripts/sbom-survey/sbom_survey/paths.py
scripts/sbom-survey/sbom_survey/registry.py
scripts/sbom-survey/sbom_survey/report.py
scripts/sbom-survey/sbom_survey/survey.py
scripts/sbom-survey/sbom_survey/tiers.py
scripts/sbom-survey/sbom_survey/util.py
scripts/sbom-survey/smoke-install-run.sh
scripts/sbom-survey/tests/__pycache__/conftest.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_cli.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_cyclonedx.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_discovery.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_paths.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_registry.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_report.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_tiering.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/conftest.py
scripts/sbom-survey/tests/test_cli.py
scripts/sbom-survey/tests/test_cyclonedx.py
scripts/sbom-survey/tests/test_discovery.py
scripts/sbom-survey/tests/test_paths.py
scripts/sbom-survey/tests/test_registry.py
scripts/sbom-survey/tests/test_report.py
scripts/sbom-survey/tests/test_tiering.py
scripts/sbom-survey/tiers.toml
# `sbom-survey` — the dependency-survey mini-project (0.8.20 Slice 32).
#
# ISOLATION (design §2). This project is deliberately standalone:
#   * NOT a member of the Cargo workspace,
#   * NOT referenced by `src/python/pyproject.toml`,
#   * NOT a dependency of the root `package.json`.
# It can therefore never enlarge the published dependency graph or the advisory
# backlog. Its own dependencies are surveyed BY the tool and tier `dev-tooling`
# — the tool appears in its own SBOM, which is the correct answer.
#
# TC-97 — THERE IS DELIBERATELY NO `[tool.pytest.ini_options]` TABLE HERE.
# pytest only treats a `pyproject.toml` as its *configfile* when that table is
# present. Adding one would make this file the configfile for
# `scripts/sbom-survey/tests` and start importing settings (the repo's only
# other pytest config, `src/python/pyproject.toml`, sets `pythonpath = ["."]`,
# which shadows an installed wheel). The suite must run with NO configfile: the
# pytest header prints `rootdir:` and no `configfile:` line. Do not add one.

[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[project]
name = "sbom-survey"
version = "0.1.0"
description = "CycloneDX 1.6 dependency survey over FathomDB's tracked manifests (Library Sweep #3)"
readme = "README.md"
requires-python = ">=3.11"
license = { text = "Apache-2.0" }

# Design §5.7 names every one of these and why stdlib is insufficient. The set
# is kept deliberately small: a dependency-hygiene tool with a bloated
# dependency set is self-refuting.
#
# Deliberately NOT taken (§5.7, binding):
#   * no HTTP client   — stdlib `urllib.request` covers three GET-JSON calls;
#   * no TOML library  — `tomllib` is stdlib from 3.11 (hence requires-python);
#   * no `jsonschema`  — the `json-validation` extra already binds a validator
#                        to the normative 1.6 schema; a second one would drift;
#   * no `GitPython`   — one `git ls-files -z` via `subprocess` is the whole
#                        git surface;
#   * no setup.py/AST tooling — §5.2.
dependencies = [
    "cyclonedx-python-lib[json-validation]>=8.0,<9.0",
    "packageurl-python>=0.15,<1.0",
    "packaging>=24.0,<26.0",
    "semver>=3.0,<4.0",
]

[project.optional-dependencies]
# Dev-only. `pytest` is NOT a runtime dependency of the tool.
dev = ["pytest>=8.0,<10.0"]

[project.scripts]
sbom-survey = "sbom_survey.cli:console_main"

[tool.setuptools]
packages = ["sbom_survey", "sbom_survey.parse"]
"""AC-SBOM-18 — gitignored reports, tracked findings home.

REQ-11. Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.6.
"""

from __future__ import annotations

from conftest import REPO_ROOT, is_gitignored, require


def test_report_dir_is_gitignored_and_findings_home_is_tracked() -> None:
    """AC-SBOM-18.

    Generated reports are gitignored (HITL-ruled). Slice 33's *findings*
    therefore need a separate, deliberately NOT-ignored durable home — the raw
    tool output is not one.

    The home is `dev/plans/runs/` — the dominant house convention for a dated
    run report (`0.8.2-m1-FINDINGS.md`, `0.8.3-rerank-tune-FINDINGS.md`,
    `0.8.4-cost-probe-FINDINGS.md`), and where the slice's own `-output.json`
    already lands. See design §5.6 for the comparison against `dev/design/`
    and `dev/deps/`.
    """
    paths = require(
        "sbom_survey.paths",
        "AC-SBOM-18",
        "sbom_survey.paths must expose DEFAULT_REPORT_DIR (repo-relative,"
        " GITIGNORED — Slice 32 adds the `scripts/sbom-survey/out/` rule) and"
        " SLICE_33_FINDINGS_DOC (repo-relative, NOT ignored — the tracked"
        " durable home for the survey findings).",
    )

    report_dir = paths.DEFAULT_REPORT_DIR
    findings_doc = paths.SLICE_33_FINDINGS_DOC

    assert report_dir == "scripts/sbom-survey/out"
    assert findings_doc == "dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md"

    probe = f"{report_dir}/sbom.cdx.json"
    assert is_gitignored(probe), (
        f"{probe} is NOT gitignored — generated reports must never be"
        " committable (add `scripts/sbom-survey/out/` to .gitignore)"
    )
    assert not is_gitignored(findings_doc), (
        f"{findings_doc} is gitignored — it is the TRACKED durable home for"
        " Slice 33's findings and must be committable"
    )
    assert (REPO_ROOT / "dev" / "plans" / "runs").is_dir()
"""AC-SBOM-05 .. AC-SBOM-09 and AC-SBOM-23 — tiering, fixture exclusion, the
loud gap, and the longest-prefix matching rule.

REQ-3 (tiering), REQ-4 (loud gaps), REQ-5 (fixture exclusion).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.2 and §5.3.

`AC-SBOM-23` carries a criterion id out of file order: it was added at fix-3
(codex §9 round 3) and numbered last so that AC-SBOM-10..22 keep the ids the
design, the README and the closure JSON already cite. Its subject matter is
§5.3, which is why the test lives here beside AC-SBOM-05..09.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from conftest import (
    FIXTURE_PREFIX,
    PROJECT_ROOT,
    REPO_ROOT,
    TIER_VOCABULARY,
    require,
    tracked_manifest_paths,
)


def _fixture_manifests() -> list[str]:
    return [p for p in tracked_manifest_paths() if p.startswith(FIXTURE_PREFIX)]



exec
/bin/bash -lc 'bash scripts/sbom-survey/smoke-install-run.sh' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 6846ms:
smoke: installing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey into /tmp/tmp.UtqU4bDHCR/venv (needs PyPI) ...
Processing ./scripts/sbom-survey
  Installing build dependencies: started
  Installing build dependencies: finished with status 'done'
  Getting requirements to build wheel: started
  Getting requirements to build wheel: finished with status 'done'
  Preparing metadata (pyproject.toml): started
  Preparing metadata (pyproject.toml): finished with status 'done'
Collecting cyclonedx-python-lib<9.0,>=8.0 (from cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached cyclonedx_python_lib-8.9.0-py3-none-any.whl.metadata (6.5 kB)
Collecting packageurl-python<1.0,>=0.15 (from sbom-survey==0.1.0)
  Using cached packageurl_python-0.17.6-py3-none-any.whl.metadata (5.1 kB)
Collecting packaging<26.0,>=24.0 (from sbom-survey==0.1.0)
  Using cached packaging-25.0-py3-none-any.whl.metadata (3.3 kB)
Collecting semver<4.0,>=3.0 (from sbom-survey==0.1.0)
  Using cached semver-3.0.4-py3-none-any.whl.metadata (6.8 kB)
Collecting license-expression<31,>=30 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached license_expression-30.4.4-py3-none-any.whl.metadata (11 kB)
Collecting py-serializable<2.0.0,>=1.1.1 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached py_serializable-1.1.2-py3-none-any.whl.metadata (4.2 kB)
Collecting sortedcontainers<3.0.0,>=2.4.0 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached sortedcontainers-2.4.0-py2.py3-none-any.whl.metadata (10 kB)
Collecting jsonschema<5.0,>=4.18 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonschema-4.26.0-py3-none-any.whl.metadata (7.6 kB)
Collecting attrs>=22.2.0 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached attrs-26.1.0-py3-none-any.whl.metadata (8.8 kB)
Collecting jsonschema-specifications>=2023.03.6 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonschema_specifications-2025.9.1-py3-none-any.whl.metadata (2.9 kB)
Collecting referencing>=0.28.4 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached referencing-0.37.0-py3-none-any.whl.metadata (2.8 kB)
Collecting rpds-py>=0.25.0 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rpds_py-2026.6.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl.metadata (4.1 kB)
Collecting fqdn (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached fqdn-1.5.1-py3-none-any.whl.metadata (1.4 kB)
Collecting idna (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached idna-3.18-py3-none-any.whl.metadata (6.1 kB)
Collecting isoduration (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached isoduration-20.11.0-py3-none-any.whl.metadata (5.7 kB)
Collecting jsonpointer>1.13 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonpointer-3.1.1-py3-none-any.whl.metadata (2.4 kB)
Collecting rfc3339-validator (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rfc3339_validator-0.1.4-py2.py3-none-any.whl.metadata (1.5 kB)
Collecting rfc3987 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rfc3987-1.3.8-py2.py3-none-any.whl.metadata (7.5 kB)
Collecting uri-template (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached uri_template-1.3.0-py3-none-any.whl.metadata (8.8 kB)
Collecting webcolors>=1.11 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached webcolors-25.10.0-py3-none-any.whl.metadata (2.2 kB)
Collecting boolean.py>=4.0 (from license-expression<31,>=30->cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached boolean_py-5.0-py3-none-any.whl.metadata (2.3 kB)
Collecting defusedxml<0.8.0,>=0.7.1 (from py-serializable<2.0.0,>=1.1.1->cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached defusedxml-0.7.1-py2.py3-none-any.whl.metadata (32 kB)
Collecting typing-extensions>=4.4.0 (from referencing>=0.28.4->jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached typing_extensions-4.16.0-py3-none-any.whl.metadata (3.3 kB)
Collecting arrow>=0.15.0 (from isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached arrow-1.4.0-py3-none-any.whl.metadata (7.7 kB)
Collecting six (from rfc3339-validator->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached six-1.17.0-py2.py3-none-any.whl.metadata (1.7 kB)
Collecting python-dateutil>=2.7.0 (from arrow>=0.15.0->isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached python_dateutil-2.9.0.post0-py2.py3-none-any.whl.metadata (8.4 kB)
Collecting tzdata (from arrow>=0.15.0->isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached tzdata-2026.3-py2.py3-none-any.whl.metadata (1.4 kB)
Using cached cyclonedx_python_lib-8.9.0-py3-none-any.whl (375 kB)
Using cached packageurl_python-0.17.6-py3-none-any.whl (36 kB)
Using cached packaging-25.0-py3-none-any.whl (66 kB)
Using cached semver-3.0.4-py3-none-any.whl (17 kB)
Using cached jsonschema-4.26.0-py3-none-any.whl (90 kB)
Using cached license_expression-30.4.4-py3-none-any.whl (120 kB)
Using cached py_serializable-1.1.2-py3-none-any.whl (22 kB)
Using cached sortedcontainers-2.4.0-py2.py3-none-any.whl (29 kB)
Using cached attrs-26.1.0-py3-none-any.whl (67 kB)
Using cached boolean_py-5.0-py3-none-any.whl (26 kB)
Using cached defusedxml-0.7.1-py2.py3-none-any.whl (25 kB)
Using cached jsonpointer-3.1.1-py3-none-any.whl (7.7 kB)
Using cached jsonschema_specifications-2025.9.1-py3-none-any.whl (18 kB)
Using cached referencing-0.37.0-py3-none-any.whl (26 kB)
Using cached rpds_py-2026.6.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl (366 kB)
Using cached webcolors-25.10.0-py3-none-any.whl (14 kB)
Using cached fqdn-1.5.1-py3-none-any.whl (9.1 kB)
Using cached idna-3.18-py3-none-any.whl (65 kB)
Using cached isoduration-20.11.0-py3-none-any.whl (11 kB)
Using cached rfc3339_validator-0.1.4-py2.py3-none-any.whl (3.5 kB)
Using cached rfc3987-1.3.8-py2.py3-none-any.whl (13 kB)
Using cached uri_template-1.3.0-py3-none-any.whl (11 kB)
Using cached arrow-1.4.0-py3-none-any.whl (68 kB)
Using cached typing_extensions-4.16.0-py3-none-any.whl (45 kB)
Using cached six-1.17.0-py2.py3-none-any.whl (11 kB)
Using cached python_dateutil-2.9.0.post0-py2.py3-none-any.whl (229 kB)
Using cached tzdata-2026.3-py2.py3-none-any.whl (348 kB)
Building wheels for collected packages: sbom-survey
  Building wheel for sbom-survey (pyproject.toml): started
  Building wheel for sbom-survey (pyproject.toml): finished with status 'done'
  Created wheel for sbom-survey: filename=sbom_survey-0.1.0-py3-none-any.whl size=46701 sha256=07daaacb5a0bc2e6ebd102b7387c0eb48f500de6d941664dba374d64e00881b0
  Stored in directory: /home/coreyt/.cache/pip/wheels/a2/b8/35/86f9ad363fd09045aba9c835a340d81a3ade348eb64ebf4a03
Successfully built sbom-survey
Installing collected packages: sortedcontainers, rfc3987, boolean.py, webcolors, uri-template, tzdata, typing-extensions, six, semver, rpds-py, packaging, packageurl-python, license-expression, jsonpointer, idna, fqdn, defusedxml, attrs, rfc3339-validator, referencing, python-dateutil, py-serializable, jsonschema-specifications, cyclonedx-python-lib, arrow, jsonschema, isoduration, sbom-survey
Successfully installed arrow-1.4.0 attrs-26.1.0 boolean.py-5.0 cyclonedx-python-lib-8.9.0 defusedxml-0.7.1 fqdn-1.5.1 idna-3.18 isoduration-20.11.0 jsonpointer-3.1.1 jsonschema-4.26.0 jsonschema-specifications-2025.9.1 license-expression-30.4.4 packageurl-python-0.17.6 packaging-25.0 py-serializable-1.1.2 python-dateutil-2.9.0.post0 referencing-0.37.0 rfc3339-validator-0.1.4 rfc3987-1.3.8 rpds-py-2026.6.3 sbom-survey-0.1.0 semver-3.0.4 six-1.17.0 sortedcontainers-2.4.0 typing-extensions-4.16.0 tzdata-2026.3 uri-template-1.3.0 webcolors-25.10.0
smoke: console script present and executable: /tmp/tmp.UtqU4bDHCR/venv/bin/sbom-survey
smoke: provenance OK (RUN A) — installed sbom_survey resolves to /tmp/tmp.UtqU4bDHCR/venv/lib/python3.12/site-packages/sbom_survey/__init__.py
smoke: RUN A — installed console script
sbom-survey: 774 components (52 direct, 722 transitive), 774 unknown, 8 manifests excluded
  wrote /tmp/tmp.UtqU4bDHCR/out-installed/sbom.cdx.json
  wrote /tmp/tmp.UtqU4bDHCR/out-installed/staleness.json
  wrote /tmp/tmp.UtqU4bDHCR/out-installed/staleness.md
smoke: RUN A rc=0
Found existing installation: sbom-survey 0.1.0
Uninstalling sbom-survey-0.1.0:
  Successfully uninstalled sbom-survey-0.1.0
smoke: provenance OK (RUN B) — sbom_survey resolves to /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/sbom_survey/__init__.py
smoke: RUN B — source tree via python -m sbom_survey
sbom-survey: 774 components (52 direct, 722 transitive), 774 unknown, 8 manifests excluded
  wrote /tmp/tmp.UtqU4bDHCR/out-source/sbom.cdx.json
  wrote /tmp/tmp.UtqU4bDHCR/out-source/staleness.json
  wrote /tmp/tmp.UtqU4bDHCR/out-source/staleness.md
smoke: RUN B rc=0
smoke: artifact sets identical:
smoke:   sbom.cdx.json
smoke:   staleness.json
smoke:   staleness.md
smoke: byte-identical: sbom.cdx.json
smoke: byte-identical: staleness.json
smoke: byte-identical: staleness.md
smoke: PASS — installed run rc=0, source run rc=0, artifacts byte-identical over 774 components (sbom.cdx.json, staleness.json, staleness.md); both provenance guards (installed=site-packages, source=tree) and the vacuity guard held.

exec
/bin/bash -lc '(cd scripts/sbom-survey && bash smoke-install-run.sh)' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 exited 1 in 5320ms:
smoke: installing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey into /tmp/tmp.46XzhOAMcA/venv (needs PyPI) ...
Processing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey
  Installing build dependencies: started
  Installing build dependencies: finished with status 'done'
  Getting requirements to build wheel: started
  Getting requirements to build wheel: finished with status 'done'
  Preparing metadata (pyproject.toml): started
  Preparing metadata (pyproject.toml): finished with status 'done'
Collecting cyclonedx-python-lib<9.0,>=8.0 (from cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached cyclonedx_python_lib-8.9.0-py3-none-any.whl.metadata (6.5 kB)
Collecting packageurl-python<1.0,>=0.15 (from sbom-survey==0.1.0)
  Using cached packageurl_python-0.17.6-py3-none-any.whl.metadata (5.1 kB)
Collecting packaging<26.0,>=24.0 (from sbom-survey==0.1.0)
  Using cached packaging-25.0-py3-none-any.whl.metadata (3.3 kB)
Collecting semver<4.0,>=3.0 (from sbom-survey==0.1.0)
  Using cached semver-3.0.4-py3-none-any.whl.metadata (6.8 kB)
Collecting license-expression<31,>=30 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached license_expression-30.4.4-py3-none-any.whl.metadata (11 kB)
Collecting py-serializable<2.0.0,>=1.1.1 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached py_serializable-1.1.2-py3-none-any.whl.metadata (4.2 kB)
Collecting sortedcontainers<3.0.0,>=2.4.0 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached sortedcontainers-2.4.0-py2.py3-none-any.whl.metadata (10 kB)
Collecting jsonschema<5.0,>=4.18 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonschema-4.26.0-py3-none-any.whl.metadata (7.6 kB)
Collecting attrs>=22.2.0 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached attrs-26.1.0-py3-none-any.whl.metadata (8.8 kB)
Collecting jsonschema-specifications>=2023.03.6 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonschema_specifications-2025.9.1-py3-none-any.whl.metadata (2.9 kB)
Collecting referencing>=0.28.4 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached referencing-0.37.0-py3-none-any.whl.metadata (2.8 kB)
Collecting rpds-py>=0.25.0 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rpds_py-2026.6.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl.metadata (4.1 kB)
Collecting fqdn (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached fqdn-1.5.1-py3-none-any.whl.metadata (1.4 kB)
Collecting idna (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached idna-3.18-py3-none-any.whl.metadata (6.1 kB)
Collecting isoduration (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached isoduration-20.11.0-py3-none-any.whl.metadata (5.7 kB)
Collecting jsonpointer>1.13 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonpointer-3.1.1-py3-none-any.whl.metadata (2.4 kB)
Collecting rfc3339-validator (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rfc3339_validator-0.1.4-py2.py3-none-any.whl.metadata (1.5 kB)
Collecting rfc3987 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rfc3987-1.3.8-py2.py3-none-any.whl.metadata (7.5 kB)
Collecting uri-template (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached uri_template-1.3.0-py3-none-any.whl.metadata (8.8 kB)
Collecting webcolors>=1.11 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached webcolors-25.10.0-py3-none-any.whl.metadata (2.2 kB)
Collecting boolean.py>=4.0 (from license-expression<31,>=30->cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached boolean_py-5.0-py3-none-any.whl.metadata (2.3 kB)
Collecting defusedxml<0.8.0,>=0.7.1 (from py-serializable<2.0.0,>=1.1.1->cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached defusedxml-0.7.1-py2.py3-none-any.whl.metadata (32 kB)
Collecting typing-extensions>=4.4.0 (from referencing>=0.28.4->jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached typing_extensions-4.16.0-py3-none-any.whl.metadata (3.3 kB)
Collecting arrow>=0.15.0 (from isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached arrow-1.4.0-py3-none-any.whl.metadata (7.7 kB)
Collecting six (from rfc3339-validator->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached six-1.17.0-py2.py3-none-any.whl.metadata (1.7 kB)
Collecting python-dateutil>=2.7.0 (from arrow>=0.15.0->isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached python_dateutil-2.9.0.post0-py2.py3-none-any.whl.metadata (8.4 kB)
Collecting tzdata (from arrow>=0.15.0->isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached tzdata-2026.3-py2.py3-none-any.whl.metadata (1.4 kB)
Using cached cyclonedx_python_lib-8.9.0-py3-none-any.whl (375 kB)
Using cached packageurl_python-0.17.6-py3-none-any.whl (36 kB)
Using cached packaging-25.0-py3-none-any.whl (66 kB)
Using cached semver-3.0.4-py3-none-any.whl (17 kB)
Using cached jsonschema-4.26.0-py3-none-any.whl (90 kB)
Using cached license_expression-30.4.4-py3-none-any.whl (120 kB)
Using cached py_serializable-1.1.2-py3-none-any.whl (22 kB)
Using cached sortedcontainers-2.4.0-py2.py3-none-any.whl (29 kB)
Using cached attrs-26.1.0-py3-none-any.whl (67 kB)
Using cached boolean_py-5.0-py3-none-any.whl (26 kB)
Using cached defusedxml-0.7.1-py2.py3-none-any.whl (25 kB)
Using cached jsonpointer-3.1.1-py3-none-any.whl (7.7 kB)
Using cached jsonschema_specifications-2025.9.1-py3-none-any.whl (18 kB)
Using cached referencing-0.37.0-py3-none-any.whl (26 kB)
Using cached rpds_py-2026.6.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl (366 kB)
Using cached webcolors-25.10.0-py3-none-any.whl (14 kB)
Using cached fqdn-1.5.1-py3-none-any.whl (9.1 kB)
Using cached idna-3.18-py3-none-any.whl (65 kB)
Using cached isoduration-20.11.0-py3-none-any.whl (11 kB)
Using cached rfc3339_validator-0.1.4-py2.py3-none-any.whl (3.5 kB)
Using cached rfc3987-1.3.8-py2.py3-none-any.whl (13 kB)
Using cached uri_template-1.3.0-py3-none-any.whl (11 kB)
Using cached arrow-1.4.0-py3-none-any.whl (68 kB)
Using cached typing_extensions-4.16.0-py3-none-any.whl (45 kB)
Using cached six-1.17.0-py2.py3-none-any.whl (11 kB)
Using cached python_dateutil-2.9.0.post0-py2.py3-none-any.whl (229 kB)
Using cached tzdata-2026.3-py2.py3-none-any.whl (348 kB)
Building wheels for collected packages: sbom-survey
  Building wheel for sbom-survey (pyproject.toml): started
  Building wheel for sbom-survey (pyproject.toml): finished with status 'done'
  Created wheel for sbom-survey: filename=sbom_survey-0.1.0-py3-none-any.whl size=46701 sha256=64c27ebc07759dcae474fae4d41b06cff71e94bd43ba8395f626559965a8b9e9
  Stored in directory: /home/coreyt/.cache/pip/wheels/a2/b8/35/86f9ad363fd09045aba9c835a340d81a3ade348eb64ebf4a03
Successfully built sbom-survey
Installing collected packages: sortedcontainers, rfc3987, boolean.py, webcolors, uri-template, tzdata, typing-extensions, six, semver, rpds-py, packaging, packageurl-python, license-expression, jsonpointer, idna, fqdn, defusedxml, attrs, rfc3339-validator, referencing, python-dateutil, py-serializable, jsonschema-specifications, cyclonedx-python-lib, arrow, jsonschema, isoduration, sbom-survey
Successfully installed arrow-1.4.0 attrs-26.1.0 boolean.py-5.0 cyclonedx-python-lib-8.9.0 defusedxml-0.7.1 fqdn-1.5.1 idna-3.18 isoduration-20.11.0 jsonpointer-3.1.1 jsonschema-4.26.0 jsonschema-specifications-2025.9.1 license-expression-30.4.4 packageurl-python-0.17.6 packaging-25.0 py-serializable-1.1.2 python-dateutil-2.9.0.post0 referencing-0.37.0 rfc3339-validator-0.1.4 rfc3987-1.3.8 rpds-py-2026.6.3 sbom-survey-0.1.0 semver-3.0.4 six-1.17.0 sortedcontainers-2.4.0 typing-extensions-4.16.0 tzdata-2026.3 uri-template-1.3.0 webcolors-25.10.0
smoke: console script present and executable: /tmp/tmp.46XzhOAMcA/venv/bin/sbom-survey
smoke: FAIL — provenance (RUN A). The installed entry point resolves to:
smoke:        /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/sbom_survey/__init__.py
smoke:        which is INSIDE the repo (/home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run), not the venv's
smoke:        site-packages (/tmp/tmp.46XzhOAMcA/venv/lib/python3.12/site-packages). RUN A would exercise the
smoke:        SOURCE TREE, so a PASS would say nothing about the wheel.

exec
/bin/bash -lc "nl -ba scripts/sbom-survey/smoke-install-run.sh | sed -n '120,190p'" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
   120	    echo "smoke:        The most likely cause is PyPI being unreachable: this is" >&2
   121	    echo "smoke:        the ONE step that needs the network (the survey runs" >&2
   122	    echo "smoke:        themselves are --offline). Re-run with network access" >&2
   123	    echo "smoke:        before treating this as a defect in the tool." >&2
   124	    exit 1
   125	fi
   126	
   127	# --- 5. the console script FILE must exist and be executable -------------------
   128	CONSOLE="$VENV/bin/sbom-survey"
   129	if [ ! -f "$CONSOLE" ]; then
   130	    echo "smoke: FAIL — console script $CONSOLE was not created by the install." >&2
   131	    echo "smoke:        [project.scripts] in pyproject.toml is not taking effect." >&2
   132	    exit 1
   133	fi
   134	if [ ! -x "$CONSOLE" ]; then
   135	    echo "smoke: FAIL — console script $CONSOLE exists but is not executable." >&2
   136	    exit 1
   137	fi
   138	echo "smoke: console script present and executable: $CONSOLE"
   139	
   140	OUT_INSTALLED="$WORK/out-installed"
   141	OUT_SOURCE="$WORK/out-source"
   142	
   143	# --- 6a. PROVENANCE ASSERTION FOR RUN A — symmetric with RUN B's (codex §9 rd 2)
   144	#
   145	# RUN A inherits the caller's environment, and `PYTHONPATH` beats site-packages
   146	# on `sys.path`. So an ambient `PYTHONPATH` pointing at THIS checkout (or any
   147	# checkout carrying `sbom_survey`) makes the installed console script import the
   148	# SOURCE TREE — and the smoke then passes while the installed wheel is broken,
   149	# incomplete, or missing files entirely. That is a vacuous pass on the exact leg
   150	# this script exists to prove.
   151	#
   152	# Two things are needed, and only the second is a guard:
   153	#   * RUN A is invoked under `env -u PYTHONPATH -u PYTHONHOME` — scrubbed
   154	#     PER-INVOCATION, never globally, because RUN B *needs* `PYTHONPATH`;
   155	#   * and that arrangement is ASSERTED here. Unsetting only ARRANGES for the
   156	#     right thing; the assertion PROVES it. RUN B's provenance was graded from
   157	#     the start and RUN A's was not — that asymmetry was the finding.
   158	set +e
   159	SITE_PACKAGES="$(env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')"
   160	rc=$?
   161	set -e
   162	if [ "$rc" -ne 0 ] || [ -z "$SITE_PACKAGES" ]; then
   163	    echo "smoke: FAIL — could not resolve the venv's site-packages (rc=$rc)." >&2
   164	    exit 1
   165	fi
   166	
   167	set +e
   168	RESOLVED_A="$(env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
   169	rc=$?
   170	set -e
   171	if [ "$rc" -ne 0 ]; then
   172	    echo "smoke: FAIL — the INSTALLED sbom_survey is not importable from the venv (rc=$rc)." >&2
   173	    echo "smoke:        pip install reported success, so the wheel does not contain" >&2
   174	    echo "smoke:        an importable package. This is the install-path defect." >&2
   175	    exit 1
   176	fi
   177	case "$RESOLVED_A" in
   178	    "$REPO"/*)
   179	        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
   180	        echo "smoke:        $RESOLVED_A" >&2
   181	        echo "smoke:        which is INSIDE the repo ($REPO), not the venv's" >&2
   182	        echo "smoke:        site-packages ($SITE_PACKAGES). RUN A would exercise the" >&2
   183	        echo "smoke:        SOURCE TREE, so a PASS would say nothing about the wheel." >&2
   184	        exit 1
   185	        ;;
   186	esac
   187	case "$RESOLVED_A" in
   188	    "$SITE_PACKAGES"/*)
   189	        echo "smoke: provenance OK (RUN A) — installed sbom_survey resolves to $RESOLVED_A"
   190	        ;;

exec
/bin/bash -lc "grep -R \"datetime\\|time\\|generated\" -n scripts/sbom-survey/sbom_survey | head -40" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
scripts/sbom-survey/sbom_survey/paths.py:1:"""Well-known repo-relative paths and the fixed default timestamp (design §5.6, §5.8)."""
scripts/sbom-survey/sbom_survey/paths.py:68:#: The FIXED default `metadata.timestamp` (§5.8, REQ-13).
scripts/sbom-survey/sbom_survey/paths.py:74:#: this package that calls `datetime.now()` to produce an artifact timestamp.
scripts/sbom-survey/sbom_survey/survey.py:22:from datetime import datetime, timezone
scripts/sbom-survey/sbom_survey/survey.py:39:from .util import TimestampFormatError, make_purl, normalize_timestamp
scripts/sbom-survey/sbom_survey/survey.py:49:    "resolve_timestamp",
scripts/sbom-survey/sbom_survey/survey.py:194:    timestamp: str
scripts/sbom-survey/sbom_survey/survey.py:232:# timestamps (§5.8)
scripts/sbom-survey/sbom_survey/survey.py:234:def resolve_timestamp(now: str | None) -> str:
scripts/sbom-survey/sbom_survey/survey.py:235:    """The artifact timestamp: explicit `now`, else `SOURCE_DATE_EPOCH`, else the FIXED epoch.
scripts/sbom-survey/sbom_survey/survey.py:238:    operating system what time it is. `run_survey` and the CLI share it, so the
scripts/sbom-survey/sbom_survey/survey.py:243:    EVERY branch returns a value that has been through `util.parse_timestamp`,
scripts/sbom-survey/sbom_survey/survey.py:244:    so what `Survey.timestamp` holds is always canonical ISO 8601 — which is
scripts/sbom-survey/sbom_survey/survey.py:251:        return normalize_timestamp(now, source="--now / run_survey(now=…)")
scripts/sbom-survey/sbom_survey/survey.py:254:        # SOURCE_DATE_EPOCH goes through the same door: it is a timestamp input
scripts/sbom-survey/sbom_survey/survey.py:259:            stamp = datetime.fromtimestamp(seconds, tz=timezone.utc).isoformat()
scripts/sbom-survey/sbom_survey/survey.py:267:        return normalize_timestamp(stamp, source="SOURCE_DATE_EPOCH")
scripts/sbom-survey/sbom_survey/survey.py:268:    return normalize_timestamp(DEFAULT_EPOCH_TIMESTAMP, source="the built-in default epoch")
scripts/sbom-survey/sbom_survey/survey.py:487:    rule and a timestamp that cannot be substituted cannot be *proved*
scripts/sbom-survey/sbom_survey/survey.py:499:    timestamp = resolve_timestamp(now)
scripts/sbom-survey/sbom_survey/survey.py:577:            raise RuntimeError(
scripts/sbom-survey/sbom_survey/survey.py:612:        timestamp=timestamp,
scripts/sbom-survey/sbom_survey/cyclonedx.py:33:from .util import parse_timestamp
scripts/sbom-survey/sbom_survey/cyclonedx.py:51:#: This module used to carry its own `_timestamp()` that swallowed a
scripts/sbom-survey/sbom_survey/cyclonedx.py:56:#: `util.parse_timestamp` — the same function `survey.resolve_timestamp` uses —
scripts/sbom-survey/sbom_survey/cyclonedx.py:71:    bom.metadata.timestamp = parse_timestamp(
scripts/sbom-survey/sbom_survey/cyclonedx.py:72:        survey.timestamp, source="survey.timestamp"
scripts/sbom-survey/sbom_survey/tiers.py:56:    """A `tiers.toml` that cannot be trusted — rejected at LOAD time."""
scripts/sbom-survey/sbom_survey/tiers.py:163:    time, before any path is classified: an unknown `action`, a tier outside the
scripts/sbom-survey/sbom_survey/cli.py:36:from .util import TimestampFormatError, normalize_timestamp
scripts/sbom-survey/sbom_survey/cli.py:108:    # `survey.resolve_timestamp()`, which is the SAME function `run_survey` uses
scripts/sbom-survey/sbom_survey/cli.py:111:    # argparse default of `datetime.now()` here would sail straight past an
scripts/sbom-survey/sbom_survey/cli.py:114:    parser.add_argument("--now", default=None, help="ISO-8601 timestamp (default: a FIXED epoch)")
scripts/sbom-survey/sbom_survey/cli.py:132:            normalize_timestamp(args.now, source="--now")
scripts/sbom-survey/sbom_survey/registry.py:13:from typing import Mapping, Protocol, runtime_checkable
scripts/sbom-survey/sbom_survey/registry.py:24:@runtime_checkable
scripts/sbom-survey/sbom_survey/registry.py:79:    def __init__(self, timeout: float = 10.0, user_agent: str = "fathomdb-sbom-survey") -> None:
scripts/sbom-survey/sbom_survey/registry.py:80:        self.timeogrep: scripts/sbom-survey/sbom_survey/__pycache__/survey.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/util.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/cli.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/report.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/registry.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/tiers.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/paths.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/cyclonedx.cpython-312.pyc: binary file matches
ut = timeout
scripts/sbom-survey/sbom_survey/registry.py:89:        with urllib.request.urlopen(request, timeout=self.timeout) as response:  # noqa: S310
scripts/sbom-survey/sbom_survey/report.py:5:`{generated, source, summary, rows}`, every row carries the Slice-33 field set,

codex
The new smoke script works from the repository root but can falsely fail when run from the sbom-survey project directory because its installed-provenance probe imports from the current source tree. That makes the added manual verification path unreliable.

Review comment:

- [P2] Run provenance checks outside the source tree — /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/smoke-install-run.sh:168-168
  When this smoke test is launched from `scripts/sbom-survey` (for example `cd scripts/sbom-survey && bash smoke-install-run.sh`), this `python -c` runs with the current directory on `sys.path`, so it imports the checkout's `sbom_survey` package and fails RUN A's provenance check before the installed console script is ever exercised. The script should make these installed-path probes from a directory outside the project (e.g. `$WORK`) or otherwise remove cwd from `sys.path`, so the smoke does not falsely fail for a normal invocation from the tool directory.
The new smoke script works from the repository root but can falsely fail when run from the sbom-survey project directory because its installed-provenance probe imports from the current source tree. That makes the added manual verification path unreliable.

Review comment:

- [P2] Run provenance checks outside the source tree — /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/smoke-install-run.sh:168-168
  When this smoke test is launched from `scripts/sbom-survey` (for example `cd scripts/sbom-survey && bash smoke-install-run.sh`), this `python -c` runs with the current directory on `sys.path`, so it imports the checkout's `sbom_survey` package and fails RUN A's provenance check before the installed console script is ever exercised. The script should make these installed-path probes from a directory outside the project (e.g. `$WORK`) or otherwise remove cwd from `sys.path`, so the smoke does not falsely fail for a normal invocation from the tool directory.
