import math
from ._base import EmbedderIdentity, QueryEmbedder


class StellaEmbedder(QueryEmbedder):
    def __init__(self, model_name: str, dimensions: int = 1024) -> None:
        self._model_name = model_name
        self._dimensions = dimensions
        self._model = None

    def identity(self) -> EmbedderIdentity:
        return EmbedderIdentity(
            model_identity=self._model_name,
            model_version=None,
            dimensions=self._dimensions,
            normalization_policy="l2",
        )

    def embed(self, text: str) -> list[float]:
        from sentence_transformers import SentenceTransformer

        if self._model is None:
            self._model = SentenceTransformer(self._model_name)

        raw = self._model.encode(text)
        # Truncate if needed
        vec = list(raw[: self._dimensions])
        # L2-normalize
        norm = math.sqrt(sum(v * v for v in vec))
        if norm > 0.0:
            vec = [v / norm for v in vec]
        return vec
