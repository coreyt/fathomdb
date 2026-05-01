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
   * Maximum token budget per text chunk.
   */
  maxTokens(): number;
}

export { OpenAIEmbedder } from "./openai.js";
export { JinaEmbedder } from "./jina.js";
export { StellaEmbedder } from "./stella.js";
export { SubprocessEmbedder } from "./subprocess.js";
