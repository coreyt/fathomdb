import type { QueryEmbedder } from "./index.js";

const DEFAULT_BASE_URL = "https://api.openai.com/v1";
const DEFAULT_CACHE_TTL_MS = 300_000; // 5 minutes
const DEFAULT_CACHE_MAX = 512;

interface CacheEntry {
  ts: number;
  vec: number[];
}

export interface OpenAIEmbedderOptions {
  model: string;
  apiKey: string;
  dimensions: number;
  baseUrl?: string;
  cacheTtlMs?: number;
  cacheMax?: number;
}

export class OpenAIEmbedder implements QueryEmbedder {
  private readonly _model: string;
  private readonly _apiKey: string;
  private readonly _dimensions: number;
  private readonly _baseUrl: string;
  private readonly _cacheTtlMs: number;
  private readonly _cacheMax: number;
  private readonly _cache: Map<string, CacheEntry>;

  constructor(options: OpenAIEmbedderOptions) {
    this._model = options.model;
    this._apiKey = options.apiKey;
    this._dimensions = options.dimensions;
    this._baseUrl = options.baseUrl ?? DEFAULT_BASE_URL;
    this._cacheTtlMs = options.cacheTtlMs ?? DEFAULT_CACHE_TTL_MS;
    this._cacheMax = options.cacheMax ?? DEFAULT_CACHE_MAX;
    this._cache = new Map();
  }

  identity(): string {
    return `openai/${this._model}/${this._dimensions}`;
  }

  maxTokens(): number {
    return 8192;
  }

  async embed(texts: string[]): Promise<number[][]> {
    const now = Date.now();
    const results: number[][] = new Array(texts.length);
    const uncachedIndices: number[] = [];

    // Check cache for each text
    for (let i = 0; i < texts.length; i++) {
      const text = texts[i];
      const entry = this._cache.get(text);
      if (entry !== undefined && now - entry.ts < this._cacheTtlMs) {
        results[i] = entry.vec;
      } else {
        uncachedIndices.push(i);
      }
    }

    if (uncachedIndices.length === 0) {
      return results;
    }

    // Fetch uncached texts in a single batch
    const uncachedTexts = uncachedIndices.map((i) => texts[i]);
    const url = `${this._baseUrl}/embeddings`;
    const response = await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this._apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        model: this._model,
        input: uncachedTexts,
        dimensions: this._dimensions,
      }),
    });

    if (!response.ok) {
      throw new Error(`OpenAI API error: ${response.status} ${response.statusText}`);
    }

    const json = (await response.json()) as { data: Array<{ embedding: number[] }> };
    if (!Array.isArray(json.data) || json.data.length !== uncachedTexts.length) {
      throw new Error(
        `OpenAI API returned ${json.data?.length ?? 0} embeddings for ${uncachedTexts.length} inputs`,
      );
    }

    for (let i = 0; i < uncachedIndices.length; i++) {
      const originalIdx = uncachedIndices[i];
      const text = texts[originalIdx];
      const vec = json.data[i].embedding;

      // Evict oldest entry if at capacity
      if (this._cache.size >= this._cacheMax) {
        let oldestKey: string | undefined;
        let oldestTs = Infinity;
        for (const [k, v] of this._cache) {
          if (v.ts < oldestTs) {
            oldestTs = v.ts;
            oldestKey = k;
          }
        }
        if (oldestKey !== undefined) {
          this._cache.delete(oldestKey);
        }
      }

      this._cache.set(text, { ts: now, vec });
      results[originalIdx] = vec;
    }

    return results;
  }
}
