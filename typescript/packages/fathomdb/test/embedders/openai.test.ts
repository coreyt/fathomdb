import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { OpenAIEmbedder } from "../../src/embedders/openai.js";

function makeFetchMock(embedding: number[]) {
  return vi.fn().mockResolvedValue({
    ok: true,
    json: async () => ({
      data: [{ embedding }],
    }),
  });
}

describe("OpenAIEmbedder", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = makeFetchMock([0.1, 0.2, 0.3]);
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns embedded vectors from the API", async () => {
    const embedder = new OpenAIEmbedder({
      model: "text-embedding-3-small",
      apiKey: "test",
      dimensions: 3,
    });

    const result = await embedder.embed(["hello"]);
    expect(result).toEqual([[0.1, 0.2, 0.3]]);
  });

  it("identity() returns provider/model/dimensions", () => {
    const embedder = new OpenAIEmbedder({
      model: "text-embedding-3-small",
      apiKey: "test",
      dimensions: 3,
    });
    expect(embedder.identity()).toBe("openai/text-embedding-3-small/3");
  });

  it("maxTokens() returns 8192", () => {
    const embedder = new OpenAIEmbedder({
      model: "text-embedding-3-small",
      apiKey: "test",
      dimensions: 3,
    });
    expect(embedder.maxTokens()).toBe(8192);
  });

  it("cache hit: second embed() does not call fetch again within TTL", async () => {
    const embedder = new OpenAIEmbedder({
      model: "text-embedding-3-small",
      apiKey: "test",
      dimensions: 3,
    });

    const first = await embedder.embed(["hello"]);
    const second = await embedder.embed(["hello"]);

    expect(first).toEqual([[0.1, 0.2, 0.3]]);
    expect(second).toEqual([[0.1, 0.2, 0.3]]);
    // fetch should have been called only once
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("POSTs to the correct URL with expected body", async () => {
    const embedder = new OpenAIEmbedder({
      model: "text-embedding-3-small",
      apiKey: "sk-test",
      dimensions: 3,
    });

    await embedder.embed(["hello"]);

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("https://api.openai.com/v1/embeddings");
    const body = JSON.parse(opts.body as string);
    expect(body.model).toBe("text-embedding-3-small");
    expect(body.input).toEqual(["hello"]);
    expect(body.dimensions).toBe(3);
  });
});
