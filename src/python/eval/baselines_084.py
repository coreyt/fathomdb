"""0.8.4 standalone baselines (Slice 5, $0 eval-infra): VectorRAG + LongContext.

Both adapters conform to the existing R2 ``RetrievalAdapter`` Protocol
(``name`` + ``retrieve(question, k) -> list[Hit]``) and are deliberately
INDEPENDENT of the FathomDB engine (pure-Python, no native extension, no DB, no
LLM) so a baseline number can never be conflated with the SUT.

* :class:`VectorRagAdapter` — a standalone dense retriever. With no eval-side
  embedding service wired in this slice, it uses a small deterministic
  hashing bag-of-words embedder + cosine similarity. This is the BASELINE; its
  job is to be reproducible and engine-independent, not state-of-the-art.
* :class:`LongContextAdapter` — the "stuff-it-all-in" control: it returns the
  corpus packed to a window-fit char budget in a deterministic, query-independent
  order, so the shared answerer can stuff the whole window. No ranking model — the
  honest upper-bar control.
"""

from __future__ import annotations

import hashlib
import math
import re
from collections import defaultdict
from collections.abc import Mapping

from eval.r2_parity_eval import Hit

_TOKEN_RE = re.compile(r"[a-z0-9]+")


def _tokenize(text: str) -> list[str]:
    return [t for t in _TOKEN_RE.findall(text.lower()) if len(t) >= 2]


def _embed(text: str, dim: int) -> dict[int, float]:
    """Deterministic L2-normalized hashing bag-of-words vector (sparse dict).

    Each token is hashed (sha1, stable across processes/runs — unlike ``hash()``)
    into one of ``dim`` buckets; the value is the term frequency. Engine-independent
    and reproducible: identical text always yields the identical vector."""
    vec: dict[int, float] = defaultdict(float)
    for tok in _tokenize(text):
        idx = int(hashlib.sha1(tok.encode("utf-8")).hexdigest(), 16) % dim
        vec[idx] += 1.0
    norm = math.sqrt(sum(v * v for v in vec.values()))
    if norm == 0.0:
        return {}
    return {i: v / norm for i, v in vec.items()}


def _cosine(a: dict[int, float], b: dict[int, float]) -> float:
    # vectors are pre-normalized → dot product is the cosine; iterate the smaller.
    if len(a) > len(b):
        a, b = b, a
    return sum(val * b.get(i, 0.0) for i, val in a.items())


class VectorRagAdapter:
    """Standalone dense retriever over the AP-News corpus (baseline, not SUT)."""

    name = "vector_rag"

    def __init__(self, documents: Mapping[str, str], *, dim: int = 512) -> None:
        self._dim = dim
        self._bodies: dict[str, str] = dict(documents)
        self._vectors: dict[str, dict[int, float]] = {
            doc_id: _embed(body, dim) for doc_id, body in self._bodies.items()
        }

    def retrieve(self, question: str, k: int) -> list[Hit]:
        qv = _embed(question, self._dim)
        scored: list[tuple[str, float]] = [
            (doc_id, _cosine(qv, dv)) for doc_id, dv in self._vectors.items()
        ]
        # Deterministic ordering: score desc, then doc_id asc to break ties stably.
        scored.sort(key=lambda kv: (-kv[1], kv[0]))
        return [
            Hit(doc_id=doc_id, body=self._bodies.get(doc_id, ""), score=float(score))
            for doc_id, score in scored[:k]
        ]


class LongContextAdapter:
    """The 'stuff-it-all-in' control: corpus packed to a window-fit char budget.

    Query-independent and deterministic — it returns documents in stable doc_id
    order up to ``min(k, budget-fit)``. ``char_budget`` is the window-fit cap; at
    least one document is always returned (never an empty context) so the answerer
    has something to stuff even when a single doc exceeds the budget."""

    name = "long_context"

    def __init__(self, documents: Mapping[str, str], *, char_budget: int = 48_000) -> None:
        self._char_budget = char_budget
        # Deterministic, query-independent order: sorted by doc_id.
        self._docs: list[tuple[str, str]] = sorted(documents.items())

    def retrieve(self, question: str, k: int) -> list[Hit]:
        hits: list[Hit] = []
        used = 0
        for rank, (doc_id, body) in enumerate(self._docs):
            if len(hits) >= k:
                break
            if hits and used + len(body) > self._char_budget:
                break
            hits.append(Hit(doc_id=doc_id, body=body, score=float(len(self._docs) - rank)))
            used += len(body)
        return hits


__all__ = ["Hit", "LongContextAdapter", "VectorRagAdapter"]
