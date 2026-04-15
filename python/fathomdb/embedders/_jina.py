from ._base import EmbedderIdentity, QueryEmbedder

_MODEL = "jina-embeddings-v2-base-en"
_DIMENSIONS = 768


class JinaEmbedder(QueryEmbedder):
    def __init__(self, api_key: str) -> None:
        self._api_key = api_key

    def identity(self) -> EmbedderIdentity:
        return EmbedderIdentity(
            model_identity=_MODEL,
            model_version=None,
            dimensions=_DIMENSIONS,
            normalization_policy="none",
        )

    def embed(self, text: str) -> list[float]:
        import httpx

        with httpx.Client() as client:
            resp = client.post(
                "https://api.jina.ai/v1/embeddings",
                headers={
                    "Authorization": f"Bearer {self._api_key}",
                    "Content-Type": "application/json",
                },
                json={"model": _MODEL, "input": [text]},
            )
        resp.raise_for_status()
        return resp.json()["data"][0]["embedding"]
