import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { StellaEmbedder } from "../../src/embedders/stella.js";

function makeFetchMock(embedding: number[]) {
  return vi.fn().mockResolvedValue({
    ok: true,
    json: async () => ({
      data: [{ embedding }],
    }),
  });
}

describe("StellaEmbedder construction guard", () => {
  it("throws when no baseUrl is provided", () => {
    expect(() => new StellaEmbedder({ apiKey: "test-key" })).toThrow(
      "no hosted API exists",
    );
  });

  it("does NOT throw when baseUrl is provided", () => {
    expect(
      () => new StellaEmbedder({ apiKey: "test-key", baseUrl: "http://localhost:8080" }),
    ).not.toThrow();
  });
});

describe("StellaEmbedder", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = makeFetchMock([0.5, 0.6, 0.7]);
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns embedded vectors from the Stella API", async () => {
    const embedder = new StellaEmbedder({ apiKey: "test-key", baseUrl: "http://localhost:8080" });
    const result = await embedder.embed(["hello"]);
    expect(result).toEqual([[0.5, 0.6, 0.7]]);
  });

  it("identity() returns stella/stella_en_400M_v5/1024 (default dimensions)", () => {
    const embedder = new StellaEmbedder({ apiKey: "test-key", baseUrl: "http://localhost:8080" });
    expect(embedder.identity()).toBe("stella/stella_en_400M_v5/1024");
  });

  it("identity() reflects custom dimensions", () => {
    const embedder = new StellaEmbedder({ apiKey: "test-key", baseUrl: "http://localhost:8080", dimensions: 512 });
    expect(embedder.identity()).toBe("stella/stella_en_400M_v5/512");
  });

  it("maxTokens() returns 512", () => {
    const embedder = new StellaEmbedder({ apiKey: "test-key", baseUrl: "http://localhost:8080" });
    expect(embedder.maxTokens()).toBe(512);
  });

  it("sends the correct request to the API", async () => {
    const embedder = new StellaEmbedder({ apiKey: "stella-test", baseUrl: "http://localhost:8080", dimensions: 1024 });
    await embedder.embed(["hello"]);

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toContain("/embeddings");
    const body = JSON.parse(opts.body as string);
    expect(body.input).toEqual(["hello"]);
    expect(body.dimensions).toBe(1024);
  });
});
