import type { QueryEmbedder } from "./index.js";

const MODEL = "stella_en_400M_v5";
const DEFAULT_DIMENSIONS = 1024;

export interface StellaEmbedderOptions {
  apiKey: string;
  dimensions?: number;
  baseUrl: string;
}

export class StellaEmbedder implements QueryEmbedder {
  private readonly _apiKey: string;
  private readonly _dimensions: number;
  private readonly _baseUrl: string;

  constructor(options: StellaEmbedderOptions) {
    if (!options.baseUrl) {
      throw new Error(
        "StellaEmbedder: no hosted API exists for stella_en_400M_v5; " +
          "provide baseUrl pointing to your local inference server",
      );
    }
    this._apiKey = options.apiKey;
    this._dimensions = options.dimensions ?? DEFAULT_DIMENSIONS;
    this._baseUrl = options.baseUrl;
  }

  identity(): string {
    return `stella/${MODEL}/${this._dimensions}`;
  }

  maxTokens(): number {
    return 512;
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
        dimensions: this._dimensions,
      }),
    });

    if (!response.ok) {
      throw new Error(`Stella API error: ${response.status} ${response.statusText}`);
    }

    const json = (await response.json()) as { data: Array<{ embedding: number[] }> };
    if (!Array.isArray(json.data) || json.data.length !== texts.length) {
      throw new Error(
        `Stella API returned ${json.data?.length ?? 0} embeddings for ${texts.length} inputs`,
      );
    }
    return json.data.map((item) => item.embedding);
  }
}
