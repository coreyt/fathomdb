from abc import ABC, abstractmethod
from dataclasses import dataclass


@dataclass(frozen=True)
class EmbedderIdentity:
    """Immutable identity descriptor for a vector embedding model.

    Parameters
    ----------
    model_identity : str
        Stable identifier for the model (e.g. ``"bge-small-en-v1.5"``).
        Used by the vec identity guard to detect model switches at open time.
    model_version : str or None
        Optional version string within the model family.
    dimensions : int
        Output embedding dimensionality.
    normalization_policy : str
        Normalization applied by the model or client, e.g. ``"l2"`` or ``"none"``.
    """

    model_identity: str
    model_version: str | None
    dimensions: int
    normalization_policy: str


class QueryEmbedder(ABC):
    """Abstract base class for query-time text embedders.

    Subclasses must implement :meth:`identity` and :meth:`embed`.
    Concrete implementations: :class:`OpenAIEmbedder`, :class:`JinaEmbedder`,
    :class:`StellaEmbedder`, :class:`SubprocessEmbedder`.
    """

    @abstractmethod
    def identity(self) -> EmbedderIdentity:
        """Return the stable identity descriptor for this embedder."""
        ...

    @abstractmethod
    def embed(self, text: str) -> list[float]:
        """Embed *text* and return a float vector of length ``identity().dimensions``."""
        ...
