export interface WriteReceipt {
  cursor: number;
}

export interface SearchResult {
  projectionCursor: number;
  results: string[];
}

export class Engine {
  #cursor = 0;
  #closed = false;

  private constructor(public readonly path: string) {}

  static async open(path: string): Promise<Engine> {
    return new Engine(path);
  }

  async write(batch: unknown[] = []): Promise<WriteReceipt> {
    this.ensureOpen();
    this.#cursor += Math.max(batch.length, 1);
    return { cursor: this.#cursor };
  }

  async search(query: string): Promise<SearchResult> {
    this.ensureOpen();
    const normalized = query.trim();
    if (normalized.length === 0) {
      throw new Error("query must not be empty");
    }

    return {
      projectionCursor: this.#cursor,
      results: [`rewrite scaffold query: ${normalized}`],
    };
  }

  async close(): Promise<void> {
    this.#closed = true;
  }

  private ensureOpen(): void {
    if (this.#closed) {
      throw new Error("engine is closed");
    }
  }
}
