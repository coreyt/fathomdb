# CLEANUP-MAP — Phase-1 ledger-prune classification (dry-run)

> **Status: PHASE 1 — awaiting HITL sign-off. No files moved/deleted/edited.**
> Produced by `dev/prune-docs.md` §3. State anchor: **0.8.5 in-flight, 0.8.4 closed**;
> 0.6.x–0.8.4 are CLOSED releases. Built by 6 read-only directory agents; the main
> thread merged + resolved cross-directory citations. Verdicts: CURRENT (live, stays) ·
> REFERENCE (historical but load-bearing; stays, distil results) · ARCHIVE (`git mv` →
> `dev/archive/<release>/` + banner) · DELETE (`git rm`; result distilled first).

## 0. Headline footprint

| Verdict | Files | Notes |
|---------|------:|-------|
| **CURRENT** | 130 | live specs, contracts, ADRs-in-force, 0.8.4/0.8.5 docs |
| **REFERENCE** | 283 | experiment findings, preregs, cited decisions, ADRs-superseded-but-recorded |
| **ARCHIVE** | 333 | closed-release plans/prompts/slice-memos/STATUS boards |
| **DELETE** | 521 | per-run `*-output.json`, codex review `.log/.md`, transcripts, checkpoints, `.npy`/flamegraph dumps |
| **Total in scope** | **1,267** | + `A3-evidence/` (6) + `__pycache__/` (delete; not counted) |

**De-clog impact:** DELETE is ~41% of files and **~160 MB** (dominated by ~78 MB of
`runs/*.log`, ~60 MB of `research/*.npy` vectors, ~2.4 MB of profiling flamegraphs). The
authored ledger that *stays* (CURRENT+REFERENCE+ARCHIVE) is ~1,000 files but only a few MB
of prose. ARCHIVE relocates 333 files out of the live cold-start paths into
`dev/archive/<release>/`.

> ⚠️ **Three findings below (§4 R-1, R-2, R-3) are blocking and need a ruling before any
> Phase-2 move/delete.** They are not mere classification doubts — they change the
> mechanism.

---

## 1. Per-directory classification

Bulk transient classes are grouped by pattern with **exact counts** (exhaustive accounting,
not sampling — counts reconcile to each directory total). Authored docs are enumerated or
tightly grouped by release.

### `dev/plans/runs/` — 660 files (+A3-evidence 6, +__pycache__)
| Group | Verdict | Reason / fidelity |
|-------|---------|-------------------|
| `STATUS-0.8.4.md`, `0.8.4-COMPREHENSIVE-REPORT.md` | **CURRENT** (2) | newest board + closing report of just-closed release; cited |
| 62 narrative result `.md` (FINDINGS/results/VERDICT/decision/report/capability-status/diagnosis), all releases | **REFERENCE** (62) | experiment records → distil to ledger (§3) |
| 56 named data JSON (`*-measurements`, `*-n606`, `*-RESULT.json`, `m1-verdict-n300`, `corpus-manifest`, `memory-gold`, `IR-C-recall-*`) + 6 cited `*-output.json` | **REFERENCE** (56) | sole structured copy of numbers / cited → distil then DELETE-eligible |
| 9 closed-release STATUS boards (`STATUS.md`, `-phase12`, `-embedder-undefer`, `-release-hardening`, `-perf-vector-quant`, `-0.8.0/0.8.1/0.8.2/0.8.3`) | **ARCHIVE** (9) | closed boards |
| 12 closed design/planning/setup/verdict `.md` (slice-0/5/5b verdicts, c1-seeding, g0-phase2, R6, graphrag-setup, FIX-33, drift-list, preflight, orchestration-prompt) | **ARCHIVE** (12) | closed-release narratives |
| 16 experiment harness `.py`/`.wf.js` (13× `0.8.4-*.py` + 3 `*.wf.js`) | **ARCHIVE** (16) | closed-release scripts (R-9: some reusable for 0.8.5) |
| 108 codex/slice `*-review-<ts>.md` / `.raw.md` | **DELETE** (108) | review dumps; verdicts captured in STATUS + git |
| ~200 `*-output.json` (incl. 22 `-rN-` rerun spam) | **DELETE** (~200) | per-run machine output; distil sole-copy first |
| 179 `*.log` + 3 `*.log.gz` | **DELETE** (182) | raw capture (~78 MB) |
| 10 `*.checkpoint/ckpt/d2-index/ce-pass.json` | **DELETE** (10) | resume checkpoints / regenerable indices |
| 3 `.txt` (transcript, folded-diff, evidence) + `A3-evidence/` (6) + `__pycache__/` | **DELETE** | dumps / bytecode |
| **subtotal** | | CURRENT 2 · REFERENCE 118 · ARCHIVE 37 · DELETE 503 (+A3 6 +pycache) |

### `dev/plans/*.md` + `prompts/` + `scaffolds/` — 240 files
| Group | Verdict | Reason |
|-------|---------|--------|
| `plan-0.8.4.md`, `0.8.5-ce-rerank-alpha-expose-slice.md`, `plans/README.md`, `prompts/0.8.x-PROGRAM-STEWARD-HANDOFF.md` | **CURRENT** (4) | current-release plan + live slice + steward entry |
| 4 plans (`0.8.0-GA-and-IR-eval-roadmap`, 2× IR-C spec, `ci-deferred`) + 12 DOC-INDEX-cited prompts + 10 `scaffolds/*` | **REFERENCE** (26) | cited by CURRENT docs / live tracks |
| 18 closed-release plans/impls (`0.6.0`–`0.8.1` impl/plan, `plan-0.8.2/0.8.3`, `0.7.0-perf-experiments`, agentic-context) | **ARCHIVE** (18) | closed-release plans |
| 192 per-slice/orchestrator prompts for CLOSED slices (grouped by release prefix) | **ARCHIVE** (192) | closed-slice prompts |
| **subtotal** | | CURRENT 4 · REFERENCE 26 · ARCHIVE 210 · DELETE 0 |

### `dev/design/` — 66 files
| Group | Verdict | Reason |
|-------|---------|--------|
| 18 cross-cutting specs (`engine/lifecycle/scheduler/recovery/errors/embedder/retrieval/vector/ann-index-vec0/op-store/migrations/projections/bindings/orchestration/perf-gates/perf-regression-detection/release/README`) | **CURRENT** (18) | live system specs |
| `0.8.x-portfolio-features-and-experiment-tree`, `0.8.x-parity-portfolio-strategy`, `0.8.5-ce-rerank-slice-design` | **CURRENT** (3) | live experiment decision-tree + 0.8.5 |
| 17 load-bearing memos (preregs `0.8.2-m1-multihop-harness`, `0.8.3-mem0-parity`; `ir-recall-measure`; scope sources `0.8.0-agent-memory-fit`, `-v05-feature-triage`; `embedder-decision`; diagnosis; `0.8.1-slice-10-reranker-design`; 0.8.4 GraphRAG ×2; 3 slice memos cited from live specs — see R-7) | **REFERENCE** (17) | cited by CURRENT/ADRs; results → ledger |
| 28 closed per-slice design memos (`slice-5/10/15*/20*/25*/27-fix2/30/31/33/35/40/G0`, `0.7.0-vector-quant-pack1`, `0.7.1-EU-6-FIX-*`, `0.8.0-slice-5-G1`, `0.8.3-*probe/arm/slice-*`) | **ARCHIVE** (28) | closed memos; some carry findings → ledger (e.g. `0.8.3-gap-decomposition-probe`) |
| **subtotal** | | CURRENT 21 · REFERENCE 17 · ARCHIVE 28 · DELETE 0 |

### `dev/adr/` — 53 files (NEVER archived/deleted — guardrail §0)
| Group | Verdict | Reason |
|-------|---------|--------|
| 51 in-force ADRs + README + decision-index | **CURRENT** (51) | live decisions in force |
| `ADR-0.6.0-database-lock-mechanism` (→ reader-pool-revision), `ADR-0.6.0-text-query-latency-gates` (→ 0.7.0-revised) | **REFERENCE** (2) | superseded-but-recorded (stay in place) |
| **subtotal** | | CURRENT 51 · REFERENCE 2 · ARCHIVE 0 · DELETE 0 |

### `dev/deps/` — 36 files
| Group | Verdict | Reason |
|-------|---------|--------|
| `index.md` (status:living), `README.md` | **CURRENT** (2) | live dep-audit index (dep-auditor agent update target) |
| 34 per-crate keep/drop/replace evaluations | **REFERENCE** (34) | decisions in force; cited by index + 0.6.0 ADRs |

### `dev/notes/` — 55 files
| Group | Verdict | Reason |
|-------|---------|--------|
| `README.md` | **CURRENT** (1) | dir index |
| 19 cited/DOC-INDEX notes (tokenizer-latency, recall-eval-framework, elps-consult-3, longmemeval-ref, bge-cls-mean-bug, corpus-expansion ×2, ac013-019-scale-policy, perf-whitepaper-notes, perf-canonical-runner, pcache2, embedder/vector research, root-cause, IR-C ×3, EU-8-design, test-corpus) | **REFERENCE** (19) | load-bearing; results → ledger |
| 5 orphaned top-level (`0.6.0-design-docs-index`, `12-TX-parity-matrix`, `context-research-agentic-best-practices`, `0.6.0-rewrite-WIP-critique`, `phase9-handoff`) + 21 `context-research/set{1,2,3}/*` | **ARCHIVE** (26) | superseded research/notes |
| 9 `perf/ac020-*.svg/.folded` flamegraph dumps | **DELETE** (9) | result in ADR-0.7.0-ac020 + whitepaper-notes (verify first) |

### root `dev/*.md` + remaining subdirs — 157 files
| Group | Verdict | Reason |
|-------|---------|--------|
| 12 root docs (`acceptance`✦, `requirements`✦, `security-review`✦, `architecture`, `DOC-INDEX`, `learnings`, `update-docs`, `prune-docs`, `prune-docs-acceptance-tests`, `README`, `test-plan`, `traceability`) | **CURRENT** (12) | ✦ = sole home of REQ/AC/SR IDs |
| `interfaces/*` (6), `corpus-creation/*` (3), `scripts/ir_c_ce_latency.py` (1), `roadmap/0.8.5.md`+`0.8.4.md` (2), `release/README.md`+fixtures/tests (25 — test infra, out of doc scope) | **CURRENT** (37) | contracts + live infra |
| `needs.md`, `memex-note-on-0.6.0.md`, `roadmap/{0.8.0–0.8.3,plan,README}` (6), `perf-history/*` (10, machine-read — R-3), `profiling/*` (13), `agents/{dep-auditor,learnings/prose-harvester,README}` (4), `interface-inventory/README`, `research/*` non-binary (24, untracked — R-2), `turso-watch/*` (2), `templates/psd-template`, `acceptance-rowsets/AC-040a`, `releases/0.8.0.md` | **REFERENCE** (67) | cited/reusable/baselines |
| `agents/interface-inventory-option{1,2,3}.md` (3), `interface-inventory/option{1,2,3}/**` (20→ minus README = ~20), `progress/*` (5), `reports/development-state-0.6.0-to-0.7.2`, `psd/0.6.0-system-psd`, `archive/*` (2, already archived) | **ARCHIVE** (32) | closed studies/logs |
| `research/eu-0/*.npy`+sweep.log (7), `research/pr-2a/*.npy` (2) | **DELETE** (9) | regenerable vectors (~60 MB; untracked — R-2) |
| **subtotal** | | CURRENT 49 · REFERENCE 67 · ARCHIVE 32 · DELETE 9 |

---

## 2. CURRENT cold-start set (what an agent should see post-prune)

`dev/` root contracts (acceptance/requirements/architecture/interfaces/DOC-INDEX/test-plan/
traceability/learnings/security-review/needs) · all 18 `design/` cross-cutting specs ·
all 53 `dev/adr/` records · `deps/index.md` · the 0.8.4/0.8.5 plan + roadmap + STATUS +
COMPREHENSIVE-REPORT + the live experiment decision-tree (`0.8.x-portfolio-*`) · the new
`experiments-ledger.md` (Phase 2). Everything else becomes REFERENCE-in-place or moves to
`dev/archive/`.

---

## 3. Fidelity-extraction plan (results to distil into `dev/experiments-ledger.md`)

Each row → one ledger entry (hypothesis · prereg · N/power · numbers+CI · verdict · what
closed it · $ cost · original path + SHA) **before** any matching DELETE. Source files stay
REFERENCE until distilled.

| Release | Experiment | Headline to capture | Primary sources |
|---------|-----------|---------------------|-----------------|
| 0.7.0 | Perf/vector-quant sweep | AC-012/013/020 p50/p99; binary-quant recall floor | `runs/0.7.0-perf-experiments-results.md`, `…W*-output.json`, `notes/0.7.0-vector-cost-research` |
| 0.7.1 | EU-7 recall | 0.937→0.896 anchor; cause = vector-stage/SUT | `runs/0.7.1-EU-7-findings.md` |
| 0.7.2 | EU-8 IR recall · PR-2a/2bc · PR-3 | IR ceiling ≈0.571; PR-2a GO=artifact; tiered latency 10k/100k/1M | `runs/0.7.2-EU-8-ir-recall-results`, `-PR-2a/2c/2bc-*`, `-PR-3-perf-data` |
| 0.8.0 | B2 tokenizer · graph-model · corpus A/B · recall-eval framework | tokenizer latency-neutral; logical_id-alone; fidelity-vs-relevance axis | `notes/0.8.0-fts5-tokenizer-*`, `runs/0.8.0-graph-model-resolution-*`, `notes/recall-eval-framework-*` |
| 0.8.1 | Graph arm NO-GO · diagnosis · IR-C r0/r2 | BFS adds ~0 recall; precision/coverage diagnosis; CDF/eval results | `runs/0.8.1-beat-bm25-report`, `design/…diagnosis`, `design/0.8.1-graph-experiment-plan`, `runs/IR-C-r0/r2-*` |
| 0.8.2 | M1 multi-hop | **DECISIVE NO-GO ΔF1 −0.0405 n=300**; bridge-vs-answer; bge-cls bug | `runs/0.8.2-m1-FINDINGS`, `-m1-verdict-n300.json`, `notes/0.8.2-bge-cls-mean-engine-bug` |
| 0.8.3 | CE-rerank α-lever · mem0-parity · eu7-bisect · gap-decomp · 15a embedder | α=1.0 MRR 0.347→0.589, r@1 ×3.9; parity-or-better; **gap = retrieval precision**; NO-SWAP | `runs/0.8.3-rerank-tune-FINDINGS`, `-mem0-parity-VERDICT`, `-eu7-bisect-report`, `-gap-decomposition-report`, `design/0.8.3-slice-15a-embedder-probe` |
| 0.8.4 | GraphRAG-parity gating · fair re-run · scale · vs-MS | **SPLIT** (C surpasses, D2 product loses; ≥$42.34); fair re-run flip 0.06→0.81 ($0.324) | `runs/0.8.4-gating-rerun-RESULT`, `-tier1-fair-rerun-RESULT`, `-scale-powered-run-RESULT`, `-vs-microsoft-graphrag-RESULT`, `-cost-probe-FINDINGS` |
| eu7 / pr-2a | Embedder sweep · recall-artifact (research, untracked) | eu7 sweep numbers; pr-2a recall artifact (RESOLVED) | `research/eu-0/*result*.json`, `research/pr-2a/*result*.json` |
| 0.7.0 ac020 | Architectural lever | flamegraph finding (already in ADR-0.7.0-ac020) | `notes/perf/ac020-*` (verify ADR captures before DELETE) |

---

## 4. Risk list (resolve before Phase 2)

> **HITL rulings 2026-06-26 (now codified in `prune-docs.md`):**
> **R-1 →** archive `plans/prompts/` **in place** (banner only, no move); record archived/stale
> status in the existing `dev/plans/README.md` (no second index). **R-2 →** `dev/research/`:
> distil to ledger, **HOLD on delete**, leave the rest; deferred-items note records that
> delete-vs-archive-subdir must be revisited. **R-3 →** machine-read `.json` (perf-history)
> never DELETE. (R-4..R-11 stand as written.)

**R-1 · BLOCKING — `dev/plans/` "archive-in-place" convention conflicts with §4 `git mv`.**
`dev/plans/README.md` states completed prompts are archived *in place* because ~120 prompt
paths are cross-referenced by path from ADRs/design/run logs. Relocating the 210 ARCHIVE
plans/prompts would break a large web of by-path inbound links. **Ruling needed:** (a)
honor archive-in-place for `plans/prompts/` (mark ARCHIVE but don't move; add banners only),
or (b) relocate + rewrite all inbound links in the same commits. Recommend (a) for prompts.

**R-2 · BLOCKING — `dev/research/` is untracked / git-ignored (local-only).** `git mv`/`git
rm` don't apply; DELETE there is a plain disk delete with **no git recovery**. The 9 `.npy`
(~60 MB) are regenerable, but the REFERENCE result JSONs must be distilled to the ledger
**before** any removal. **Ruling needed:** delete `.npy` outright (regenerable) vs leave
research/ entirely untouched by this prune.

**R-3 · BLOCKING — `dev/perf-history/*.json` are machine-read baselines.** Append-only,
consumed by the perf-regression-check binary. Classified REFERENCE — **must never be
DELETEd** despite the `.json` extension. Ensure no Phase-2 rule sweeps them.

**R-4 · DOC-INDEX bundled rows must be split.** DOC-INDEX line ~94 bundles
`engine/lifecycle/.../0.7.0-vector-quant-pack1/0.7.1-EU-6-FIX-*` into one row mixing CURRENT
+ ARCHIVE files. Splitting is required when those ARCHIVE files move (gate-m).

**R-5 · Stale DOC-INDEX / ADR labels (out of prune scope, flag for update-docs).**
`0.8.1-graph-track-HANDOFF-2` is labelled "(CURRENT entry point)" but the 0.8.1 graph track
is closed; `ADR-0.6.0-decision-index` lists several 0.8.x ADRs as draft though their bodies
are HITL-signed; two ADR frontmatters say `draft` while bodies say accepted. These are
sync-drift, not prune verdicts — log to `learnings.md`/STATUS, don't fix here.

**R-6 · `dev/release/fixtures/` + `tests/` (24) are test infra, not docs.** Read-only per
§0 — exclude from any sweep.

**R-7 · 3 slice memos are REFERENCE only via live citation.** `slice-27-rust-allowlist`,
`slice-27-fix1-operator-gate` (← `interfaces/rust.md`), `slice-34-cli-op-store-readback`
(← `op-store.md`). If reclassified ARCHIVE, the citing CURRENT docs need link-repair in the
same commit. Recommend keep REFERENCE.

**R-8 · 0.8.4 GraphRAG docs borderline REFERENCE/ARCHIVE.** Fork-E / GraphRAG-scale is
re-opened per memory → kept REFERENCE; downgrade only if portfolio confirms closed.

**R-9 · `plan-0.8.3.md` + 13 `0.8.4-*.py` borderline.** CE-rerank lineage + reusable
0.8.5 drivers — ARCHIVE per closed-release rule, but defensibly REFERENCE.

**R-10 · ~126 `*-output.json` have no stem-matched narrative.** Most auto-aggregate into
`0.7.0-perf-experiments-results.md`. Per §4, each needs a complete ledger row (path+SHA)
before `git rm` — exhaustive accounting, not the sampled spot-check.

**R-11 · No locked-ID or ADR-deletion violations.** All REQ/AC/SR homes
(`acceptance.md`/`requirements.md`/`security-review.md`) are CURRENT; all 53 ADRs are
CURRENT/REFERENCE only. Guardrails §0 hold across the map.

---

## 5. Recommended Phase-2 sequencing (on approval)

1. Build `dev/experiments-ledger.md` from §3 (all releases) — verify numbers vs source.
2. DELETE the result-free transients first (codex review `.log/.md`, `.npy`, flamegraphs,
   checkpoints, rerun spam) — each reconciled against §3 / a ledger row.
3. ARCHIVE closed-release `design/` memos, `runs/` STATUS boards + narratives, `notes/`
   research, `progress/`, `interface-inventory/`, `psd/`, `reports/` → `dev/archive/<release>/`
   with banners + manifest rows; repair inbound links in the same commit.
4. Handle `plans/prompts/` per the R-1 ruling (in-place banners vs relocate).
5. Refresh DOC-INDEX (split R-4 row), READMEs, traceability — every commit self-consistent.
6. Leave ADRs, perf-history baselines, release fixtures, and all CURRENT docs untouched.

> **STOP — Phase 1 complete. Review verdicts (especially R-1/R-2/R-3) and the
> fidelity-extraction plan, then approve to proceed to Phase 2.**
