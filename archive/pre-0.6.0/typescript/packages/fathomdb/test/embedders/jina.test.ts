import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { JinaEmbedder } from "../../src/embedders/jina.js";

function makeFetchMock(embedding: number[]) {
  return vi.fn().mockResolvedValue({
    ok: true,
    json: async () => ({
      data: [{ embedding }],
    }),
  });
}

describe("JinaEmbedder", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = makeFetchMock([0.1, 0.2, 0.3]);
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns embedded vectors from the Jina API", async () => {
    const embedder = new JinaEmbedder({ apiKey: "test-key" });
    const result = await embedder.embed(["hello"]);
    expect(result).toEqual([[0.1, 0.2, 0.3]]);
  });

  it("identity() returns jina/model/768", () => {
    const embedder = new JinaEmbedder({ apiKey: "test-key" });
    expect(embedder.identity()).toBe("jina/jina-embeddings-v2-base-en/768");
  });

  it("maxTokens() returns 8192", () => {
    const embedder = new JinaEmbedder({ apiKey: "test-key" });
    expect(embedder.maxTokens()).toBe(8192);
  });

  it("POSTs to the correct Jina URL", async () => {
    const embedder = new JinaEmbedder({ apiKey: "jina-test" });
    await embedder.embed(["hello"]);

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("https://api.jina.ai/v1/embeddings");
  });

  it("sends correct body to Jina", async () => {
    const embedder = new JinaEmbedder({ apiKey: "jina-test" });
    await embedder.embed(["hello"]);

    const [, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(opts.body as string);
    expect(body.model).toBe("jina-embeddings-v2-base-en");
    expect(body.input).toEqual(["hello"]);
  });
});
