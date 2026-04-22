# FathomDB readiness for OpenClaw integration

| Field | Value |
|---|---|
| Date | 2026-04-18 |
| FathomDB version | 0.5.1 |
| FathomDB TS SDK | 0.5.1 (napi-rs, Phase 12 vector branch incomplete) |
| OpenClaw version | 2026.4.18 |
| Requirements doc | [requirements-fathomdb-0.5.1-2026-04-18.md](./requirements-fathomdb-0.5.1-2026-04-18.md) |
| Comparison doc | [comparison-fathomdb-0.5.1-2026-04-18.md](./comparison-fathomdb-0.5.1-2026-04-18.md) |

## Verdict

**Close, not ready.** Core engine meets most requirements. TS SDK feature-complete for writes/queries/admin but has **two blocking gaps**: (1) Phase 12 unified search has no working vector branch in TS, (2) search results lack `path`/`startLine`/`endLine` fields. Other gaps are integration-layer work (adapter code) or non-blocking polish. No FathomDB core redesign required.

## Gap classification

- **BLOCK** — cannot ship integration without fix in FathomDB core/SDK.
- **ADAPT** — gap bridged by integration layer; FathomDB itself fine.
- **POLISH** — nice-to-have, ship-able without.

## Gaps

### G1. BLOCK — TS vector search not wired
**Requirement:** R1.1, R1.5, R7 (hybrid search + vector probe)
**Finding:** `Engine.open({embedder})` accepts embedder for read-time use, but Phase 12 unified vector branch is text-only; vector results NOT returned through `SearchBuilder.execute()`. `src/engine.ts:34-39` doc comment flags this.
**Impact:** `probeVectorAvailability()` would return `true` but searches produce no vector hits. Integration degrades to FTS-only silently. Violates R1.1 (hybrid) and R1.5 (vector probe must reflect reality).
**Fix location:** FathomDB core Phase 12 completion; wire vector coordinator into TS napi bindings.
**Fix size:** Medium. Already scheduled per project roadmap.

### G2. BLOCK — SearchHit lacks path/line fields
**Requirement:** R2 (MemorySearchResult needs `path`, `startLine`, `endLine`)
**Finding:** `SearchHit` (`src/types.ts:218-269`) exposes `node`, `snippet`, `projectionRowId`, `vectorDistance`. No `path`. Chunks carry `byte_start`/`byte_end`, not line numbers. Chunks have no `path` field — they reference nodes, not files.
**Impact:** Integration layer cannot construct `MemorySearchResult` directly from `SearchHit`. Must (a) look up chunk → node → convention-encoded path, (b) reconstruct line numbers from byte offsets.
**Fix location:** Integration layer **OR** FathomDB — add file-chunk convention (node kind `memory_file` with `path` property, chunks with `start_line`/`end_line` metadata).
**Fix size:** Small if handled in adapter (node property convention). Medium if upstreamed.

### G3. BLOCK — Score not normalized to [0,1]
**Requirement:** R1.1, R2.2 (score ∈ [0,1], higher=better)
**Finding:** `SearchHit.score` explicitly documented as "raw engine score, NOT normalized, NOT bounded, NOT comparable across blocks" (`src/types.ts:224-234`). Text hits = `-bm25(...)`, vector hits = `-distance`.
**Impact:** Direct return to OpenClaw breaks `minScore` filter semantics and result display.
**Fix location:** Integration layer. Apply BM25→[0,1] via `relevance / (1 + relevance)` (matching OpenClaw's `bm25RankToScore`); map vector distance → `1 - distance` clamped. MMR/temporal decay in adapter.
**Fix size:** Small.

### G4. ADAPT — No file ingest helper
**Requirement:** R9.1-R9.4 (reactive file ingest, chunking, session deltas)
**Finding:** `WriteRequestBuilder.addChunk` requires caller-supplied `textContent`, `byteStart`, `byteEnd`, `contentHash`. No file-reader, no chunker, no watcher.
**Impact:** None on FathomDB. Integration layer owns chokidar, chunking strategy, hashing.
**Fix location:** Integration layer.
**Fix size:** Medium (port chunking logic from `@openclaw/memory-core`).

### G5. POLISH — TS embedder gaps
**Requirement:** A10 (embedder parity with OpenClaw builtin roster)
**Finding:** TS exports `OpenAIEmbedder`, `JinaEmbedder`, `StellaEmbedder`, `SubprocessEmbedder`. Python adds `BuiltinEmbedder` (Candle bge-small). **Missing in TS:** builtin Candle, Mistral, Ollama.
**Impact:**
- No Candle → no fully-offline default. Users must configure remote embedder.
- No Mistral/Ollama → can't match existing OpenClaw user configs 1:1.
**Fix location:** TS SDK (Candle via napi) + `SubprocessEmbedder` wrappers for Ollama/Mistral.
**Fix size:** Candle in napi = large (binary size, cross-platform). Subprocess wrappers = trivial.
**Mitigation:** Ship without Candle in TS; document subprocess Ollama config as offline path.

### G6. POLISH — Prebuild platform gap
**Requirement:** R11.3 (macOS arm64/x64, Linux arm64/x64, Windows x64)
**Finding:** napi triples in `typescript/packages/fathomdb/package.json:39-50` cover `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. **Missing:** `aarch64-unknown-linux-gnu` (Linux ARM64 — OpenClaw targets this via Docker on ARM servers, Raspberry Pi hosts).
**Impact:** OpenClaw users on Linux ARM64 can't install without local build.
**Fix location:** FathomDB CI (add triple).
**Fix size:** Small.

### G7. ADAPT — No "list chunks by path" admin op
**Requirement:** R10, R9.3 (delete on path change, session delta retarget)
**Finding:** Admin API supports `excise_source(source_ref)` and `purge_logical_id(logical_id)`. No list-by-path. By convention, integration layer sets `source_ref = workspace-relative path`; `excise_source(path)` becomes delete-on-change primitive.
**Impact:** None once convention adopted.
**Fix location:** Integration layer convention. Document in design.
**Fix size:** Trivial.

### G8. POLISH — Engine.close synchronous
**Requirement:** R1.6 (close releases lock, R8.3 tears down ≤5s)
**Finding:** `Engine.close(): void` — synchronous, idempotent. No async variant.
**Impact:** Blocks Node.js event loop during flush. Acceptable for shutdown path; awkward inside `closeAllMemorySearchManagers` which expects `Promise<void>`.
**Fix location:** Integration layer wraps in `Promise.resolve().then(...)` or microtask.
**Fix size:** Trivial.

### G9. POLISH — No status telemetry for OpenClaw's MemoryProviderStatus
**Requirement:** R1.3 (status with `files`, `chunks`, `dirty`, `vector.dims`, `fts.available`)
**Finding:** `telemetrySnapshot()` exists (`src/engine.ts:118-121`), but shape unknown from audit. Integration layer must compose `MemoryProviderStatus` from:
- `chunks` count → admin query or operational collection size
- `dirty` → integration-layer flag (FathomDB doesn't track it)
- `vector.dims` → `EngineOptions.vectorDimension`
- `fts.available` → `admin.listFtsPropertySchemas()` non-empty
**Impact:** Status assembly is adapter work. No missing primitives.
**Fix size:** Small.

### G10. POLISH — Error mapping
**Requirement:** R10.2
**Finding:** FathomDB TS errors clean (`FathomError` hierarchy, 11 typed classes, `BuilderValidationError` for bad fused filters). FTS metachar handling: text queries parsed safely, unsupported syntax becomes literal — no panic. Matches FathomDB 0.5.2 scope item `fts5-metachar-escape`.
**Impact:** None — requirement met once 0.5.2 ships with metachar escape design completed.
**Fix location:** Already in 0.5.2 scope.

## Requirements coverage matrix

| Req | Status | Notes |
|---|---|---|
| R1.1 `search` ranked, [0,1] | BLOCK | G1 (vector), G3 (score) |
| R1.2 `readFile` | ADAPT | Node kind convention + chunk lookup |
| R1.3 `status` | POLISH | G9 — compose from admin + flags |
| R1.4 `sync` | ADAPT | Integration layer owns |
| R1.5 probes | BLOCK | G1 for vector probe |
| R1.6 `close` | POLISH | G8 — sync wrap |
| R2 result shape | BLOCK | G2 — path/line missing |
| R3 status shape | POLISH | G9 |
| R4 plugin registration | ADAPT | OpenClaw-side; TS plugin entry |
| R5 config | ADAPT | `cfg.memory.fathomdb` branch |
| R6 session scoping | ADAPT | `source_ref` + filter convention |
| R7 embedder | BLOCK | G1 — embedder must be wired to vector path |
| R8 concurrency / lifecycle | MET | Exclusive file lock matches requirement |
| R9 ingestion | ADAPT | G4 — integration layer chunks + watches |
| R10 errors/logging | MET | 0.5.2 metachar fix completes this |
| R11 TS binding | POLISH | G6 — add Linux ARM64 |

## Blocking work summary

To unblock shipping OpenClaw integration:

1. **Complete Phase 12 TS vector branch** (G1) — FathomDB core work.
2. **Decide path/line handling** (G2) — either adapter convention (fast, recommended) or upstream schema change.
3. **Score normalization in adapter** (G3) — adapter work, no core change.

(1) is only hard block on FathomDB. (2) and (3) ship in integration layer.

## Assumption recheck (relative to [requirements doc](./requirements-fathomdb-0.5.1-2026-04-18.md))

- **A3 (line numbering asymmetry)** — FathomDB chunks use **byte offsets**, not lines. Requirement doc assumed lines. Integration layer now has to derive lines from bytes during ingest AND readFile. Re-validate whether OpenClaw consumers actually need line granularity or byte ranges suffice.
- **A4 (score range)** — CONFIRMED NOT [0,1]. Adapter normalization mandatory.
- **A14 (ingestion ownership)** — CONFIRMED no OpenClaw or FathomDB utility does file → chunk conversion. Integration layer builds it.
- **A9 (prebuilds)** — Missing Linux ARM64. Add before shipping for server/edge deployments.
- **A10 (embedder parity)** — Gaps confirmed (no Mistral, no Ollama, no Candle in TS). Document subprocess wrappers as bridge.

Other assumptions still unvalidated; same flags as requirements doc.

## Recommended next step

Write design doc targeting:
- Integration shape `[B]` (separate plugin) — cleanest.
- Node-kind convention: `memory_file { path, source: "memory"|"sessions" }` with chunks carrying `start_line`, `end_line` in chunk properties (or reconstructed at read time).
- Adapter normalization: OpenClaw-style BM25→score + vector distance→score, weighted fusion with OpenClaw's default (0.7 vector, 0.3 text).
- Depend on FathomDB ≥ 0.5.3 (assumes Phase 12 TS vector lands there).
- Linux ARM64 prebuild added in FathomDB CI task concurrently.
