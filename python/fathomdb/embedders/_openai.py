import time
from ._base import EmbedderIdentity, QueryEmbedder

_CACHE_TTL = 300  # seconds
_CACHE_MAX = 512


class OpenAIEmbedder(QueryEmbedder):
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
            normalization_policy="none",
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
