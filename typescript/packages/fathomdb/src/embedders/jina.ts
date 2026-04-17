import type { QueryEmbedder } from "./index.js";

const MODEL = "jina-embeddings-v2-base-en";
const DIMENSIONS = 768;
const DEFAULT_BASE_URL = "https://api.jina.ai/v1";

export interface JinaEmbedderOptions {
  apiKey: string;
  baseUrl?: string;
}

export class JinaEmbedder implements QueryEmbedder {
  private readonly _apiKey: string;
  private readonly _baseUrl: string;

  constructor(options: JinaEmbedderOptions) {
    this._apiKey = options.apiKey;
    this._baseUrl = options.baseUrl ?? DEFAULT_BASE_URL;
  }

  identity(): string {
    return `jina/${MODEL}/${DIMENSIONS}`;
  }

  maxTokens(): number {
    return 8192;
  }

  async embed(texts: string[]): Promise<number[][]> {
    const url = `${this._baseUrl}/embeddings`;
    const response = await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this._apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        model: MODEL,
        input: texts,
      }),
    });

    if (!response.ok) {
      throw new Error(`Jina API error: ${response.status} ${response.statusText}`);
    }

    const json = (await response.json()) as { data: Array<{ embedding: number[] }> };
    return json.data.map((item) => item.embedding);
  }
}
