from abc import ABC, abstractmethod
from dataclasses import dataclass


@dataclass(frozen=True)
class EmbedderIdentity:
    model_identity: str
    model_version: str | None
    dimensions: int
    normalization_policy: str


class QueryEmbedder(ABC):
    @abstractmethod
    def identity(self) -> EmbedderIdentity: ...

    @abstractmethod
    def embed(self, text: str) -> list[float]: ...
