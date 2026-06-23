"""0.8.4 standalone baselines (Slice 5, $0 eval-infra): VectorRAG + LongContext.

RED stub — implementation lands in the GREEN commit.
"""

from __future__ import annotations

from eval.r2_parity_eval import Hit


class VectorRagAdapter:
    name = "vector_rag"

    def __init__(self, documents, *, dim=512):  # noqa: ANN001, ANN204
        raise NotImplementedError

    def retrieve(self, question, k):  # noqa: ANN001, ANN201
        raise NotImplementedError


class LongContextAdapter:
    name = "long_context"

    def __init__(self, documents, *, char_budget=48000):  # noqa: ANN001, ANN204
        raise NotImplementedError

    def retrieve(self, question, k):  # noqa: ANN001, ANN201
        raise NotImplementedError


__all__ = ["Hit", "LongContextAdapter", "VectorRagAdapter"]
