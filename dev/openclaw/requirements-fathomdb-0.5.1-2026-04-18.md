# OpenClaw requirements for FathomDB

| Field | Value |
|---|---|
| Date | 2026-04-18 |
| FathomDB version | 0.5.1 |
| OpenClaw version | 2026.4.18 |
| `@openclaw/memory-host-sdk` | 2026.4.18 |
| `@openclaw/memory-core` | 2026.4.18 |

## Purpose

List requirements FathomDB must meet to be usable as an OpenClaw memory backend (i.e. as a third option alongside `builtin` and `qmd`). This is a requirements doc, not a design. Open assumptions are called out in [Assumptions to validate](#assumptions-to-validate) — verify before writing a design doc.

Two integration shapes are possible and should be distinguished throughout:

- **A. Backend** — FathomDB replaces `MemoryIndexManager` / `QmdMemoryManager` under the existing `memory-core` extension (new `backend: "fathomdb"` value).
- **B. Plugin** — Ship a separate `@openclaw/memory-fathomdb` extension that registers its own `MemoryPluginRuntime`.

Requirements below apply to both unless tagged `[A]` or `[B]`.

---

## R1. Interface conformance

FathomDB integration layer MUST provide an object satisfying `MemorySearchManager` (file `packages/memory-host-sdk/src/host/types.ts:61`):

```typescript
interface MemorySearchManager {
  search(query, opts?: { maxResults?, minScore?, sessionKey? }): Promise<MemorySearchResult[]>;
  readFile(params: { relPath, from?, lines? }): Promise<{ text, path, ... }>;
  status(): MemoryProviderStatus;
  sync?(params?: { reason?, force?, sessionFiles?, progress? }): Promise<void>;
  probeEmbeddingAvailability(): Promise<{ ok, error? }>;
  probeVectorAvailability(): Promise<boolean>;
  close?(): Promise<void>;
}
```

### R1.1 `search`
- MUST return results sorted descending by `score`, where `score ∈ [0, 1]` (higher = better). OpenClaw consumers assume this range for fusion/display.
- MUST respect `maxResults` and `minScore` filters server-side (don't rely on caller).
- MUST return `[]` when index empty. No throw.
- MUST honor optional `sessionKey` for session-scoped retrieval (see R5).
- MUST tolerate metachars / unicode / very long queries without raising beyond `BuilderValidationError`-style errors — no process crashes.

### R1.2 `readFile`
- `relPath` is workspace-relative (relative to memory directory). Integration layer MUST resolve it against the workspace, not arbitrary disk.
- Line numbering is **1-based**. `from` default = 1, `lines` default = 120. Must match `read-file-shared.ts:99-102`.
- Out-of-range MUST return `text: ""`, not throw.
- Result fields: `{ text, path, truncated?, from?, lines?, nextFrom? }`.

### R1.3 `status`
Synchronous (no await). MUST populate at minimum: `backend`, `provider`, `model?`, `files?`, `chunks?`, `dirty?`, `dbPath?`, `vector: { enabled, available?, dims? }`, `fts: { enabled, available }`. Consumers use this for health dashboards; missing fields degrade UX.

### R1.4 `sync`
- OpenClaw calls `sync` before search when workspace files change, on session start, on interval, and on explicit user action.
- Integration layer MUST persist state across invocations (no full rebuild every call).
- MUST accept `force: true` for full rebuild.
- MUST call `progress({ completed, total, label? })` during long runs if callback provided.
- `sessionFiles?` narrows sync to a session delta — MUST NOT trigger full scan when provided.

### R1.5 Probes
- `probeEmbeddingAvailability` returns `{ ok: true }` or `{ ok: false, error }`. Network/API reachability of the embedder.
- `probeVectorAvailability` returns `boolean`. Index has vector table + sqlite-vec equivalent loaded.

### R1.6 `close`
- Release DB file lock, flush pending writes, stop watchers / timers.
- MUST be idempotent.
- MUST be safe to call with pending in-flight `sync` or `search` (await or cancel).

---

## R2. Result data shape

`MemorySearchResult` (file `packages/memory-host-sdk/src/host/types.ts:3-11`):

```typescript
{
  path: string;          // workspace-relative
  startLine: number;     // 0-based
  endLine: number;       // 0-based, inclusive
  score: number;         // 0..1, higher better
  snippet: string;       // truncated context excerpt
  source: "memory" | "sessions";
  citation?: string;
}
```

Requirements:
- R2.1 `startLine`/`endLine` are **0-based** in `MemorySearchResult` (note: **1-based** in `readFile`; this asymmetry is inherited from OpenClaw, FathomDB must mirror it).
- R2.2 `score` normalized. FathomDB's unified ranking outputs must be mapped to `[0,1]`.
- R2.3 `source` MUST be one of `"memory" | "sessions"`. FathomDB node kinds need to classify into one of these buckets.
- R2.4 `snippet` truncated to OpenClaw config `limits.maxSnippetChars`. Integration layer owns truncation.

---

## R3. Provider status

Integration layer MUST return a `MemoryProviderStatus` with these fields populated (from `types.ts:40-75`):

- `backend`: MUST extend the type union to include `"fathomdb"` (requires SDK patch in `[A]`, or `custom` field in `[B]`).
- `provider`: e.g. `"fathomdb-sqlite"`.
- `model`: active embedder identity string (see R7 — embedder owns identity).
- `files`, `chunks`: ingested counts.
- `dirty`: true when watcher has pending unsynced changes.
- `dbPath`: absolute path to SQLite file.
- `vector.dims`: embedding dimension.
- `fts.available`: true once FTS5 schema present.
- `fallback`: populated only when FathomDB is running in a degraded mode (e.g., embedder offline, vector disabled).

---

## R4. Plugin registration

Registration path (file `src/plugins/memory-state.ts:127-175`):

```typescript
api.registerMemoryCapability({
  runtime: {
    getMemorySearchManager({ cfg, agentId, purpose }): Promise<{ manager, error? }>,
    resolveMemoryBackendConfig({ cfg, agentId }): MemoryRuntimeBackendConfig,
    closeAllMemorySearchManagers?(): Promise<void>,
  },
});
```

Requirements:

- R4.1 `[A]` — Extend `memory-core`'s `resolveMemoryBackendConfig` to recognize `"fathomdb"` as a backend value and add a new branch in `getMemorySearchManager` (`extensions/memory-core/src/memory/search-manager.ts:54-133`).
- R4.2 `[B]` — New extension package `@openclaw/memory-fathomdb`. Package manifest MUST include:
  ```json
  "openclaw": { "extensions": ["./index.ts"] }
  ```
  Entry file MUST export `register(api)` that calls `api.registerMemoryCapability(...)`.
- R4.3 Only one memory capability wins per runtime (last registration). Users select via `cfg.memory.backend`.
- R4.4 `getMemorySearchManager` MUST return `{ manager: null, error }` on failure rather than throw — caller relies on this for fallback chain.
- R4.5 Integration layer MUST participate in `FallbackMemoryManager` wrapping (`search-manager.ts:210-359`) — on primary failure, caller falls through to `builtin`. Implementations MUST throw cleanly on fatal errors to trigger the wrapper's `primaryFailed` state.

---

## R5. Configuration

User config lives at `cfg.memory` (`src/config/types.memory.ts:7-24`). Requirements:

- R5.1 New config branch `cfg.memory.fathomdb` with at minimum:
  - `dbPath?: string` — override default DB location.
  - `embedder?: { provider: "openai" | "jina" | "stella" | "candle" | "subprocess"; model?: string; ... }`.
  - `searchMode?: "hybrid" | "text" | "vector"`.
  - `limits?: { maxResults?, maxSnippetChars?, maxInjectedChars?, timeoutMs? }` (mirror `qmd.limits`).
  - `paths?: Array<{ path, name?, pattern? }>` — ingest roots (mirror `qmd.paths`).
  - `sessions?: { enabled?, retentionDays? }`.
- R5.2 Default DB path follows OpenClaw XDG convention: `$XDG_DATA_HOME/openclaw/agents/$agentId/fathomdb/index.sqlite`. MUST NOT dump into `~/`.
- R5.3 `cfg.memory.citations` ∈ `"auto" | "on" | "off"` MUST be honored — affects whether `MemorySearchResult.citation` is populated.

---

## R6. Session scoping

- R6.1 `sessionKey` (opaque string, normalized by `.trim()`) MUST filter `search` results to chunks tagged with that session OR to workspace-wide memory, per `cfg.memory.*.scope` rules (see `qmd-manager.ts:994, 1438-1449` for analog).
- R6.2 `sync({ sessionFiles: [...] })` MUST incrementally ingest session deltas. FathomDB's existing write API (`writer().submit`) is the natural fit; integration layer chunks files and submits.
- R6.3 Session transcripts MUST be classifiable under `source: "sessions"`. Node-kind mapping required (see [Assumptions A2](#a2-source-mapping)).

---

## R7. Embedder model

- R7.1 FathomDB invariant (from MEMORY.md): vector identity belongs to the embedder, not the vector config. Integration layer MUST respect this when reporting `status().model`.
- R7.2 `probeEmbeddingAvailability()` MUST exercise the configured embedder end-to-end (not just check config presence).
- R7.3 When user switches embedder model, integration layer MUST either (a) rebuild all vectors, or (b) refuse and surface error. MUST NOT silently serve stale embeddings.

---

## R8. Concurrency & lifecycle

- R8.1 FathomDB engine already enforces exclusive file lock per DB file (from MEMORY.md hard constraint). Integration layer MUST NOT open multiple `Engine` instances against same path.
- R8.2 `QMD_MANAGER_CACHE` equivalent: one manager per agent. Reuse across calls.
- R8.3 `closeAllMemorySearchManagers` MUST tear down cleanly within 5 seconds for UX (qmd impl awaits pending ops).
- R8.4 Status-only borrowed managers (see `qmd-manager.ts:150-192`) — `purpose: "status"` MUST return a read-only view that doesn't trigger ingest or hold long locks.

---

## R9. Ingestion model

- R9.1 File watcher is OpenClaw's responsibility in `builtin`, but integration layer MUST provide equivalent reactive ingest for FathomDB backend. Chokidar reuse acceptable.
- R9.2 Debounce (≥200ms) before triggering `writer().submit`.
- R9.3 Session delta tracking: only re-ingest appended bytes for session files (match `builtin` behavior in `manager-sync-ops.ts:130-133`).
- R9.4 Chunking strategy: line-bounded chunks matching existing OpenClaw semantics for result `startLine`/`endLine` to be meaningful. FathomDB `chunk` nodes already support this.

---

## R10. Error surface & logging

- R10.1 Use `createSubsystemLogger("memory")` via host API for all logging.
- R10.2 Errors from FathomDB SDK (`BuilderValidationError`, write conflicts, lock timeouts) MUST be caught at the manager boundary and mapped to the wrapper's expected error shape.
- R10.3 Never let FTS5 metachar / bad query inputs crash the process (ties to FathomDB 0.5.2 scope item `fts5-metachar-escape`).

---

## R11. Language binding

- R11.1 OpenClaw is TypeScript. Integration layer MUST use FathomDB's TypeScript binding (`typescript/packages/fathomdb`, napi-rs).
- R11.2 Integration layer is TypeScript-only; no Python/Rust code shipped with the plugin.
- R11.3 `@fathomdb/fathomdb` must be a runtime dependency; prebuilds must cover OpenClaw's target platforms (macOS arm64/x64, Linux arm64/x64, Windows x64 if supported).

---

## Assumptions to validate

All of these MUST be resolved before writing a design doc. Each is a premise the requirements above silently depend on.

### A1. Integration shape is `[B]` (separate plugin)
Assumed throughout. If the team wants `[A]` (patch `memory-core`), that changes R4 substantially — need to coordinate schema changes inside an existing package. **Validate before design.**

### A2. Source mapping
"source" in `MemorySearchResult` is `"memory" | "sessions"`. FathomDB doesn't have these kinds natively. Assumption: we introduce two conventional node kinds (or a property) to distinguish them. **Validate with FathomDB team that adding convention is acceptable vs. requiring SDK changes.**

### A3. Line numbering asymmetry
`readFile` is 1-based, `MemorySearchResult.startLine/endLine` are 0-based. **Re-read `types.ts` and `read-file-shared.ts` to confirm** — the two explorer reports did not cross-check this, and requirement R1.2 / R2.1 hinge on it.

### A4. Score normalization
Assumption: FathomDB's existing `search` score output is reliably `[0,1]`. **Check `crates/fathomdb/src/search.rs`**. If unbounded or negative, integration layer must normalize, and this belongs in design.

### A5. Fallback wrapping applies
Assumption: `FallbackMemoryManager` will wrap a FathomDB manager too. **Confirm** it's generic over `MemorySearchManager`, not QMD-specific (`search-manager.ts:210-359`).

### A6. XDG path convention
Assumed default DB path under `$XDG_DATA_HOME/openclaw/agents/$agentId/fathomdb/...`. OpenClaw team may prefer a different root for third-party backends. **Validate.**

### A7. File watcher ownership
Assumption: integration layer brings its own watcher (chokidar). Alternative: OpenClaw host emits file-change events on a bus the plugin subscribes to. **Explorer did not find a host-provided event bus** — confirm absence before committing to per-plugin watchers.

### A8. Multi-agent isolation
Assumption: one FathomDB DB file per `agentId`. Matches OpenClaw model. **Confirm** FathomDB's single-writer lock won't block when multiple agents run concurrently with distinct DB paths (it shouldn't, per MEMORY.md, but verify).

### A9. Prebuilds coverage
Assumption: FathomDB's TS napi prebuilds cover all OpenClaw target platforms. **Check `typescript/packages/fathomdb/prebuilds/`** against OpenClaw's supported matrix.

### A10. Embedder parity
Assumption: FathomDB's embedder roster (OpenAI, Jina, Stella, Candle, subprocess) covers everything OpenClaw `builtin` supports (OpenAI, Mistral, Ollama, local-embed). **Mistral and Ollama are gaps** — validate whether required or deferrable.

### A11. Memory plugin slot is exclusive
Per explorer finding, only one `registerMemoryCapability` wins. If FathomDB plugin ships, users pick it or builtin or qmd — not combine. **Confirm** this is acceptable; if OpenClaw plans multi-backend, requirements expand.

### A12. `MemorySearchManager` interface stable
OpenClaw version is `2026.4.18` (CalVer). Interface may evolve. **Pin a target `@openclaw/memory-host-sdk` version range** in the design doc and revisit on SDK bumps.

### A13. `sessionKey` semantics
Explorer report said session key "scopes search to a session's temporary files" but also that it's used for `warmSession` dedupe. **Unclear** whether it's a filter or just a cache key. **Validate by reading a consumer in `src/agents/memory-search.ts`.**

### A14. Ingestion ownership
FathomDB's `writer().submit(WriteRequest)` expects a structured request. The integration layer is responsible for file → chunk → node conversion. **Confirm no existing OpenClaw utility does this conversion** before we build one.

---

## Out of scope for this document

- Performance targets (latency, ingest throughput).
- Security / sandbox requirements — OpenClaw runs plugins in-process; any sandbox requirements would come from OpenClaw security docs, not here.
- Migration from builtin/qmd — a user-flow concern, surfaces in design.

---

## References

- OpenClaw `MemorySearchManager`: `packages/memory-host-sdk/src/host/types.ts:61`
- OpenClaw plugin capability: `src/plugins/memory-state.ts:127-175`
- OpenClaw backend config: `src/memory-host-sdk/host/backend-config.ts:349-437`
- OpenClaw backend dispatch: `extensions/memory-core/src/memory/search-manager.ts:54-133`
- OpenClaw fallback wrapper: `extensions/memory-core/src/memory/search-manager.ts:210-359`
- OpenClaw read-file contract: `extensions/memory-core/src/memory/read-file-shared.ts:99-102`
- FathomDB Engine: `crates/fathomdb/src/lib.rs`
- FathomDB search: `crates/fathomdb/src/search.rs`
- FathomDB writes: `crates/fathomdb/src/write_request_builder.rs`
- FathomDB schema: `crates/fathomdb-schema/src/lib.rs`
