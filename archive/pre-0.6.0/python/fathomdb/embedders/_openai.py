import time
from ._base import EmbedderIdentity, QueryEmbedder

_CACHE_TTL = 300  # seconds
_CACHE_MAX = 512


class OpenAIEmbedder(QueryEmbedder):
    """Query-time embedder backed by the OpenAI Embeddings API.

    Requires ``httpx`` (install via ``pip install fathomdb[openai]``).

    Embeddings are cached in a process-local dict for up to ``300`` seconds
    (TTL) with a maximum of ``512`` entries (LRU-by-insertion eviction).
    The cache is **not thread-safe**; use one instance per thread in
    concurrent contexts.

    Note: OpenAI ``text-embedding-3-*`` models return L2-normalized vectors.
    ``normalization_policy`` is reported as ``"l2"`` in the identity.

    Parameters
    ----------
    model : str
        OpenAI model name, e.g. ``"text-embedding-3-small"``.
    api_key : str
        OpenAI API key.
    dimensions : int
        Desired output dimensionality (Matryoshka truncation).
    """

    def __init__(self, model: str, api_key: str, dimensions: int) -> None:
        self._model = model
        self._api_key = api_key
        self._dimensions = dimensions
        # Simple dict-based cache: key -> (timestamp, vector)
        self._cache: dict[str, tuple[float, list[float]]] = {}

    def identity(self) -> EmbedderIdentity:
        return EmbedderIdentity(
            model_identity=self._model,
            model_version=None,
            dimensions=self._dimensions,
            normalization_policy="l2",
        )

    def embed(self, text: str) -> list[float]:
        now = time.monotonic()
        if text in self._cache:
            ts, vec = self._cache[text]
            if now - ts < _CACHE_TTL:
                return vec
            else:
                del self._cache[text]

        import httpx

        with httpx.Client() as client:
            resp = client.post(
                "https://api.openai.com/v1/embeddings",
                headers={
                    "Authorization": f"Bearer {self._api_key}",
                    "Content-Type": "application/json",
                },
                json={
                    "model": self._model,
                    "input": text,
                    "dimensions": self._dimensions,
                },
            )
        resp.raise_for_status()
        vec = resp.json()["data"][0]["embedding"]

        # Evict oldest entries if at capacity
        if len(self._cache) >= _CACHE_MAX:
            oldest_key = min(self._cache, key=lambda k: self._cache[k][0])
            del self._cache[oldest_key]

        self._cache[text] = (now, vec)
        return vec
