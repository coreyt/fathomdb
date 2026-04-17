# TypeScript Embedding Adapters — Design (0.5.0)

Companion to `dev/notes/0.5.0-scope.md` item 4 and `dev/notes/roadmap-0.4.5.md`
§ "TypeScript adapters".

---

## Interface

New file `typescript/packages/fathomdb/src/embedders/index.ts` exports:

```typescript
export interface QueryEmbedder {
  /** Embed a batch of texts. Returns one vector per input text. */
  embed(texts: string[]): Promise<number[][]>;
  /**
   * Stable identity string for this embedder.
   * Format: `<provider>/<model>/<dimensions>` (e.g. `openai/text-embedding-3-small/1536`).
   * SubprocessEmbedder uses the command joined by spaces.
   */
  identity(): string;
  /**
   * Maximum token budget per text chunk. Engine chunker reads this to
   * avoid splitting documents that fit within the embedder's context window.
   * Introduced in 0.5.0 (item 5). Embedders that don't constrain chunking
   * return a large value (e.g. 8192).
   */
  maxTokens(): number;
}
```

`maxTokens()` ships now to avoid a second breaking interface change post-0.5.0.

---

## File layout

```
typescript/packages/fathomdb/src/embedders/
  index.ts          — interface + barrel export
  openai.ts         — OpenAIEmbedder
  jina.ts           — JinaEmbedder
  stella.ts         — StellaEmbedder
  subprocess.ts     — SubprocessEmbedder
```

Exported from top-level `src/index.ts`:

```typescript
export {
  type QueryEmbedder,
  OpenAIEmbedder,
  JinaEmbedder,
  StellaEmbedder,
  SubprocessEmbedder,
} from "./embedders/index.js";
```

---

## OpenAIEmbedder

```typescript
class OpenAIEmbedder implements QueryEmbedder {
  constructor(options: {
    model: string;       // e.g. "text-embedding-3-small"
    apiKey: string;
    dimensions: number;  // Matryoshka truncation
    baseUrl?: string;    // default "https://api.openai.com/v1"
    cacheTtlMs?: number; // default 300_000 (5 min)
    cacheMax?: number;   // default 512
  });
  embed(texts: string[]): Promise<number[][]>;
  identity(): string;    // `openai/${model}/${dimensions}`
  maxTokens(): number;   // 8192
}
```

**Cache**: Map<string, { ts: number; vec: number[] }> keyed on text. Eviction:
check on write — if size >= cacheMax, delete the entry with the smallest `ts`
(insertion-order oldest). TTL checked on read.

**HTTP**: Use Node built-in `fetch`. POST to `${baseUrl}/embeddings` with
`{ model, input: texts, dimensions }`. Parse `data[].embedding`.

**Test**: vitest with `fetch` mocked via `vi.stubGlobal("fetch", ...)`.

---

## JinaEmbedder

```typescript
class JinaEmbedder implements QueryEmbedder {
  constructor(options: {
    apiKey: string;
    baseUrl?: string;    // default "https://api.jina.ai/v1"
  });
  embed(texts: string[]): Promise<number[][]>;
  identity(): string;    // "jina/jina-embeddings-v2-base-en/768"
  maxTokens(): number;   // 8192
}
```

POST to `${baseUrl}/embeddings` with `{ model: "jina-embeddings-v2-base-en", input: texts }`.
Parse `data[].embedding`. 768 dimensions fixed. No client-side cache (Jina
caches server-side).

---

## StellaEmbedder

```typescript
class StellaEmbedder implements QueryEmbedder {
  constructor(options: {
    apiKey: string;
    dimensions?: number; // default 1024; Matryoshka truncation
    baseUrl?: string;
  });
  embed(texts: string[]): Promise<number[][]>;
  identity(): string;    // `stella/stella_en_400M_v5/${dimensions}`
  maxTokens(): number;   // 512 (stella_en_400M_v5 context window)
}
```

Implementation mirrors JinaEmbedder's HTTP pattern. Truncation to
`dimensions` happens server-side when `dimensions` < 1024.

---

## SubprocessEmbedder

```typescript
class SubprocessEmbedder implements QueryEmbedder {
  constructor(options: {
    command: string[];
    dimensions: int;
    identityOverride?: string; // default: command.join(" ")
  });
  embed(texts: string[]): Promise<number[][]>;
  identity(): string;
  maxTokens(): number;   // 512
}
```

**Wire protocol**: matches Python `SubprocessEmbedder` exactly —
- Write `text + "\n"` (UTF-8) to stdin per text.
- Read `dimensions * 4` bytes from stdout as little-endian float32.
- Process texts sequentially (one at a time) to match Python parity.

**Process lifecycle**: lazily spawn via `child_process.spawn` on first
`embed()` call. Restart on unexpected exit.

**Node.js async**: stdin write and stdout read wrapped in Promises over the
existing sync stream events. Each text processed serially within a call to
`embed(texts)`.

**Test**: shell script fixture (`#!/bin/sh` that echoes a fixed f32 LE
vector), tested in the SDK harness stress/baseline scenario.

---

## Cross-language parity contract

For a given text T and embedder E with the same underlying model:
- `python_embedder.embed(T)` == `ts_embedder.embed([T])[0]` (within float32 precision)

Verified via SubprocessEmbedder using a shared fixture binary: the same
binary is called from both Python and TypeScript tests, and the returned
vectors must be bitwise-equal (same float32 LE encoding).

---

## Ship criteria

1. All four classes implement `QueryEmbedder`.
2. `OpenAIEmbedder` unit-tested with mocked `fetch` (vitest).
3. `SubprocessEmbedder` round-trips a fixed embedding through a shell-script
   fixture in the SDK harness.
4. Python and TypeScript SubprocessEmbedder produce bitwise-identical vectors
   for the same fixture binary + same input text.
5. `maxTokens()` returns documented values for each adapter.
6. All adapters exported from top-level `@fathomdb/fathomdb` index.
7. SDK harness baseline + vector scenario counts unchanged.

---

## Non-goals

- Wiring TS adapters to Rust `regenerate_vector_embeddings` via FFI.
  The TS SDK's regeneration still uses the built-in Candle embedder.
  External adapters are for caller use (read-time reranking, external
  pipelines). FFI wiring is post-0.5.0.
- Batching within SubprocessEmbedder. Sequential per-text protocol matches
  Python. True batch protocol is a future optimization.
- Connection pooling or retry logic in HTTP adapters. Post-0.5.0.
