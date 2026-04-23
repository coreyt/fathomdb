# User-reported OpenClaw memory gaps + FathomDB response map

| Field | Value |
|---|---|
| Date | 2026-04-18 |
| FathomDB version | 0.5.1 |
| OpenClaw version | 2026.4.18 |
| Companion | [requirements](./requirements-fathomdb-0.5.1-2026-04-18.md), [readiness](./readiness-fathomdb-0.5.1-2026-04-18.md) |

## Method

Three haiku-model explorer agents ran in parallel:
- GitHub issues on `openclaw/openclaw` (state: open + closed).
- Hacker News, blog posts (Augmented Mind, MemU, Daily Dose of DS), Discord surfaces (AnswerOverflow).
- In-repo mining of `CHANGELOG.md`, docs, TODO/FIXME/HACK markers.

Findings below are user-reported or team-acknowledged only. No synthetic gaps.

## Consolidated gap catalogue

Each gap tagged with ID (G-U##) for cross-reference with [Gap analysis + FathomDB fit](#gap-analysis--fathomdb-fit).

### Correctness / reliability

| ID | Gap | Source |
|---|---|---|
| **G-U01** | SQLite sync fails with "database is not open" on Node 24 experimental SQLite; auto-sync broken, manual CLI works | GH #20557, #7464 |
| **G-U02** | Index marked `Dirty: yes`, search returns empty despite indexed files; requires manual re-index | GH #4868 (closed "not planned") |
| **G-U03** | MEMORY.md + bootstrap files silently truncated at 20K chars/file, 150K total — agent runs on partial context with no warning | GH #54623 |
| **G-U04** | QMD `memory_search` returns empty results while direct QMD call works (XDG/collection mismatch) | GH #28181 |
| **G-U05** | Session memory indexing silently skipped — 0 sessions indexed despite correct config | GH #12633 |
| **G-U06** | macOS x64 on 2026.3.24: memory stuck at 0 files / 0 chunks across builtin and QMD | GH #55754 |
| **G-U07** | `sqlite-vec` loads but registers no functions on some builds (ABI mismatch v4.11), vector search silently dead | GH #65156 |
| **G-U08** | Read-only DB state — index becomes read-only on disk, needs reopen + retry (recovery path exists in code but condition recurs) | `manager-sync-control.ts:35`, `manager.readonly-recovery.test.ts`, CHANGELOG L43-44 |
| **G-U09** | Discord channels don't share memory by default — each channel has isolated session | AnswerOverflow #1476295791000948766 |
| **G-U10** | Legacy `memory.md` treated as a second root collection, caused phantom `memory-alt-*` searches in QMD | CHANGELOG L263 (fixed, signals prior duplicate-collection bug class) |

### Performance / scalability

| ID | Gap | Source |
|---|---|---|
| **G-U11** | Active Memory 2026.4.14 regression: 10-30s query latency, Telegram delays, terminated runs | GH #66708, #66849 |
| **G-U12** | QMD on ARM (Pi 5) CPU-only: `qmd embed` timeout loop, 120s cold-start per query | GH #67113, #65553, #8786 |
| **G-U13** | QMD cold-start = GGUF model reload per subprocess invocation; subprocess-per-query design broken | GH #9581 (MCP-server-mode feature request) |
| **G-U14** | `chunks_vec` not updated when sqlite-vec unavailable; vector recall degrades to in-process cosine | `cli.runtime.ts:1153` |
| **G-U15** | QMD multi-collection search runs sequentially per collection, results merged client-side; no native batch | `qmd-manager.ts:2756` |
| **G-U16** | `memory_search` injection alone can exceed model context window — memory meant to compress, instead expands | GH #5771, #14247 |
| **G-U17** | `softThresholdTokens` doesn't scale with context window size | GH #17034 |
| **G-U18** | Compaction fails when context already over limit — deadlock: can't compact without memory write, can't write without compacting | GH #25620 |

### Semantic / quality

| ID | Gap | Source |
|---|---|---|
| **G-U19** | Vector search retrieves semantically similar chunks but cannot connect facts across conversations — "remembers everything, understands none" | Daily Dose of DS blog |
| **G-U20** | Cross-project memory contamination — memories from unrelated projects bleed into retrieval | Daily Dose of DS blog |
| **G-U21** | Default compaction summarizes without discrimination — loses file paths, decision rationale, error messages, dependency versions | Augmented Mind blog, MemU blog |
| **G-U22** | BM25 + recency boost flood candidates with plausible-but-wrong results when embedding model changes | MemU blog |
| **G-U23** | Recall accuracy drops from 92% (QMD hybrid) to ~45% (default SQLite) — but QMD performance tax makes the gain unusable | MemU community benchmark |
| **G-U24** | Flat-file semantic search can't leverage organizational hierarchy or structural relationships | MemU blog |
| **G-U25** | "If a conversation never saved to memory files, there's nothing to retrieve" — no implicit capture | Augmented Mind blog |

### Security / governance

| ID | Gap | Source |
|---|---|---|
| **G-U26** | QMD `memory_get` previously allowed read of arbitrary workspace markdown, bypassing `read` tool-policy denials | CHANGELOG L154 (fixed, #66026) |
| **G-U27** | QMD symlink handling ignores OpenClaw containment rules — user must manually avoid temp checkouts in indexed paths | `docs/concepts/memory-qmd.md:172-176` |
| **G-U28** | Active Memory blocking recall timeout ceiling raised to 120s; earlier values could hang indefinitely | CHANGELOG L87 (fix #68410) |

### UX / migration

| ID | Gap | Source |
|---|---|---|
| **G-U29** | Docs recommend `cacheTtlMs: 300000` but schema rejects >120000 | GH #65708 |
| **G-U30** | "Unreliable, and you don't know when it will break" — no predictable failure surface | HN #47721955 |
| **G-U31** | Market signal: multiple third-party memory plugins (Momo, Sekha, MemOS, LosslessClaw, Cognee, MemU) — demand for replacements | HN Show HNs |

---

## Gap analysis + FathomDB fit

Three columns per relevant gap:

- **(1) Current FathomDB** — what 0.5.1 already solves.
- **(2) Aligned feature** — what FathomDB could add that fits its thesis (graph + provenance + hybrid + deterministic).
- **(3) Related functionality** — what would help the user but is NOT FathomDB's job; belongs in adapter, orchestrator, or OpenClaw itself.

### Correctness cluster

**G-U01 / G-U06 / G-U08** — SQLite lifecycle, read-only recovery, Node 24 experimental bindings
- (1) FathomDB bypasses `node:sqlite` entirely — uses rusqlite via napi-rs. Node-version-experimental-SQLite class of bug is definitionally outside scope. Exclusive file lock (`EngineRuntime::open`) catches "another instance" cases cleanly with typed `DatabaseLocked` error. Eliminates G-U01 failure mode.
- (2) Add `Engine.reopen()` helper that detects corrupt/readonly state and attempts safe reopen — mirrors OpenClaw's `readonly-recovery.test.ts` dance. Philosophically aligned (lifecycle clarity, durable state).
- (3) File-permission monitoring / auto-chmod — not FathomDB's job. Adapter-level concern.

**G-U02** — Index marked dirty, search returns empty
- (1) FathomDB's projection model (FTS + vec) is managed deterministically by the write coordinator. `dirty` isn't a user-visible state — writes are atomic with projection updates. `admin.checkIntegrity()` + `admin.rebuildMissing()` give a principled repair path rather than "manually re-index".
- (2) Per-kind projection health telemetry in `telemetrySnapshot` (file count, FTS row count, vec row count, last-rebuild timestamp) so adapter can surface "stale" to users actively.
- (3) Scheduled integrity sweeps — cron-style, orchestrator concern.

**G-U03** — Silent bootstrap truncation at 20K chars
- (1) FathomDB has no char cap on node properties or chunk text. Chunks have `byte_start`/`byte_end` ranges — caller controls chunking but doesn't silently drop.
- (2) `admin.describeIngest(sourceRef)` reporting total bytes / chunks written from source, so callers can detect drop vs full ingest.
- (3) The 20K cap is a prompt-budget concern, not storage. FathomDB can't fix prompt-composition; OpenClaw must.

**G-U04 / G-U05 / G-U10** — QMD empty-result, session skip, phantom collections
- (1) Single DB file + exclusive lock = no collection-name ambiguity, no XDG split. `source_ref` convention plus `admin.traceSource` gives exact inventory per source. Eliminates class of bug.
- (2) Adapter-layer `ingestSessionFiles(paths[])` helper bundled with FathomDB examples (not core) — reusable across OpenClaw and other integrators.
- (3) Session recording itself — orchestrator owns the session→file pipeline.

**G-U07 / G-U14** — sqlite-vec ABI / degraded vector recall
- (1) FathomDB bundles sqlite-vec at build time with version pinned; no runtime "is the extension loadable?" gamble. `probeVectorAvailability` reflects real state after `Engine.open`. Eliminates silent-fail mode.
- (2) Built-in in-process cosine fallback kind with clear `ProviderStatus.vector.fallback = "cpu_cosine"` flag so users see degraded mode, not discover it via latency.
- (3) GPU-accelerated vector backends (cuVS / Metal) — out of scope, separate project.

**G-U09** — Discord channels don't share memory
- (1) FathomDB is agent-scoped already. Cross-channel sharing is a session-identity concern, not storage. Tag writes with channel via `source_ref` or node properties; query layer filters.
- (2) Predefined `scope` property on nodes with shared edge kind `visible_in_channel`; traversal does cross-channel discovery naturally (graph use case).
- (3) Identity mapping (which Discord user = which agent user) — OpenClaw auth layer.

### Performance cluster

**G-U11 / G-U12 / G-U13** — Active Memory latency, QMD cold-start, subprocess-per-query
- (1) FathomDB = in-process library. No subprocess, no MCP bridge, no cold start. Answer to #9581 by construction. Single-writer architecture + read-pool removes serialization tax.
- (2) Per-query cost budget in `SearchBuilder.withTimeout(ms)` that short-circuits long searches with partial results — matches Active Memory's 120s cap desire but deterministically.
- (3) Pre-warming Active Memory sub-agent / speculative prefetch — orchestrator concern.

**G-U15** — QMD multi-collection sequential + merge
- (1) FathomDB has no "collections". All data in one engine, query-time filters select scope. Batch semantics are the default — the problem doesn't exist.
- (2) `Query.unionScopes([...sourceRefs])` sugar if callers want explicit multi-scope unions.
- (3) Same as G-U09.

**G-U16 / G-U17 / G-U18** — token budgeting, memory injection overflow, compaction deadlock
- (1) FathomDB `limit(n)` + `withSnippetMaxChars` deterministic. `readFile` + chunk byte ranges give adapter the primitives for bounded prompt assembly.
- (2) Token-aware top-K: `SearchBuilder.withTokenBudget({encoder, maxTokens})` — FathomDB accepts a tokenizer callback and stops accumulating when budget reached. Philosophically aligned if we treat token count as a query-plan constraint. Alternatively: expose `tokenCount` alongside `snippet` so adapter can do its own budgeting.
- (3) Compaction itself — prompt engineering / orchestrator concern, not storage.

### Semantic / quality cluster

**G-U19 / G-U24** — No relationship reasoning, flat-file limitation
- (1) FathomDB IS a graph. `Query.traverse({edge_kind, direction})`, `expand`, `filterLogicalIdEq`. Entity edges between chunks, between people mentioned, between decisions — exactly the "connect facts across conversations" ask. This is FathomDB's killer feature vs OpenClaw builtin.
- (2) Relationship-extraction pipeline (LLM-driven edge proposal) that adapter calls on ingest. Philosophically aligned — edge creation is still via `writer().submit`, FathomDB just provides a conventional node-kind-agnostic helper.
- (3) Ontology/taxonomy curation UI — out of scope.

**G-U20** — Cross-project contamination
- (1) `source_ref` scoping + `filterSourceRefEq` solves this at query time. Multi-project: one DB per project, or a `project` node property with filter.
- (2) `Engine.withProjectScope(projectId)` wrapper that auto-applies source_ref filters — opt-in. Aligned if kept as a view, not a separate DB.
- (3) Project-identity resolution (which workspace is the user in?) — OpenClaw CLI concern.

**G-U21** — Lossy compaction
- (1) Append-only mutation log + supersession: FathomDB never overwrites, old versions retained. Compaction becomes a query-time summary, not a destructive write. Restore via `restore_logical_id`.
- (2) `admin.describeMutationLog({logicalId})` for auditing what was dropped. Aligned with provenance thesis.
- (3) LLM-driven summarization pipeline — orchestrator concern.

**G-U22** — BM25 + recency flood with wrong results when embedder model changes
- (1) FathomDB's vector identity invariant: embedders own identity; regen takes an embedder. Model change → supersede chunks, re-embed. No mixed-identity searches. Match-attribution in `SearchBuilder.withMatchAttribution()` exposes which path matched, so adapter can debug floods.
- (2) Embedder-version-aware query routing: if current embedder identity ≠ stored chunk identity, skip vector branch for those chunks. Aligned with invariant.
- (3) Re-ranker layer (cross-encoder) — separate concern, could be an adapter or FathomDB companion crate.

**G-U23** — Recall 45% vs 92% but QMD unusable
- (1) FathomDB targets QMD's quality (hybrid FTS + vec) with QMD's-not latency (in-process, no cold start). Assuming Phase 12 TS vector lands, FathomDB closes this wedge.
- (2) Benchmark harness + published numbers against OpenClaw's recall test set.
- (3) Claim verification (is 92% real for QMD?) — third-party evaluation.

**G-U25** — "Nothing saved = nothing to retrieve"
- (1) N/A — that's an orchestrator decision about what to persist.
- (2) `Engine.autoChunkConversation({run})` helper? Philosophically aligned *if* writes remain explicit. FathomDB shouldn't grow a conversation-capture feature.
- (3) Conversation-auto-capture policy — OpenClaw or adapter concern.

### Security / governance cluster

**G-U26** — QMD bypass of read tool-policy
- (1) FathomDB doesn't serve arbitrary markdown. `readFile` is path-validated per adapter. Ingestion is explicit via `writer().submit`.
- (2) Adapter templating: workspace-root-anchored path resolution helper shipped with TS SDK.
- (3) Tool-policy engine — OpenClaw concern.

**G-U27** — Symlink traversal
- (1) FathomDB doesn't traverse filesystem. Adapter owns ingest policy.
- (2) Sample adapter that uses OpenClaw's own containment rules.
- (3) Filesystem sandbox — separate library.

**G-U28** — Timeout ceiling hangs
- (1) `DatabaseLockedError` + writer timeouts cap writes. Reads timeout via caller's AbortController wiring in napi (pending).
- (2) `Engine.open({defaultTimeoutMs})` applied to all operations — aligned with predictability thesis.
- (3) Application-level watchdog — orchestrator concern.

### UX / migration cluster

**G-U29** — Doc/schema mismatch
- (1) N/A — OpenClaw docs concern.
- (2) N/A.
- (3) Doc linting in OpenClaw CI — OpenClaw concern.

**G-U30** — "Unreliable, you don't know when it breaks"
- (1) Typed error hierarchy (`FathomError` + 11 subclasses), deterministic projections, atomic writes, append-only provenance. This is FathomDB's identity pitch.
- (2) Publish an "observable failure surface" contract: every possible error code documented with triggering conditions. Aligned with predictability thesis.
- (3) SRE / observability dashboards — OpenClaw concern.

**G-U31** — Market demand signal
- (1) Existing TS SDK gives FathomDB a credible shot at being one of those third-party plugins.
- (2) Bundled adapter + published npm package `@openclaw/memory-fathomdb`.
- (3) Positioning / marketing — not a FathomDB-engineering concern.

---

## Score

| Cluster | Current FathomDB covers | Needs aligned feature | Needs out-of-scope companion |
|---|---|---|---|
| Correctness | 7 of 10 | G-U08 reopen helper, G-U07/14 status flags | Scheduled integrity sweeps |
| Performance | 4 of 8 | Token-aware top-K (G-U16) | Prompt compaction, pre-warm |
| Semantic | 4 of 7 | Relationship extraction helper (G-U19), embedder-version routing (G-U22) | Ontology, re-ranker |
| Security | 1 of 3 | Path helpers | Policy engine, sandbox |
| UX/signal | 1 of 3 | Failure-surface doc | Marketing |

### Headline findings

- **FathomDB's core thesis (graph + provenance + deterministic projections + in-process) directly neutralizes the largest user-reported cluster: subprocess cold-start, dirty-state, lossy compaction, no cross-fact reasoning** (G-U11/12/13/19/21/30).
- **Aligned feature wishlist** (philosophically coherent): `Engine.reopen`, projection telemetry, mutation-log inspection, token-aware search, match-attribution publicized, published failure-surface contract.
- **Don't build**: compaction pipeline, conversation capture, tool-policy engine, re-ranker, filesystem sandbox. Those are orchestrator/OpenClaw problems.
- **Adjacent work to sequence with FathomDB integration**: token-budget search primitive (G-U16) would land well paired with the TS vector branch (Phase 12) since token budgets and hybrid planning share the coordinator.

## Sources

### GitHub issues
- #4868, #5771, #7464, #8786, #9581, #11308, #12633, #14247, #17034, #20557, #25620, #28181, #54623, #55754, #65156, #65553, #65708, #66708, #66849, #67113, #68410

### CHANGELOG entries
- L43-44 (sqlite-vec degraded warning + readonly recovery)
- L87 (Active Memory 120s ceiling)
- L115 (memory_get excerpt caps)
- L154 (#66026 path traversal)
- L263 (memory-alt-* phantom collections)

### External posts
- HN #47721955 — unreliability as core concern
- Augmented Mind — "OpenClaw Forgot Everything Again"
- MemU Blog — memory-gap analysis + recall benchmark
- Daily Dose of DS — "OpenClaw's Memory Is Broken"
- AnswerOverflow #1476295791000948766 — Discord channel isolation

### In-repo annotations
- `extensions/memory-core/src/cli.runtime.ts:1153`
- `extensions/memory-core/src/memory/qmd-manager.ts:2756`
- `extensions/memory-core/src/memory/manager-sync-control.ts:35`
- `extensions/memory-core/src/memory/manager.readonly-recovery.test.ts`
- `docs/concepts/memory-qmd.md:172-176`
- `docs/concepts/memory-search.md:72`
- `docs/concepts/active-memory.md:200-202`
