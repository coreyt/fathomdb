"""Slice 5 — standalone baseline adapters (VectorRAG + LongContext).

Both must conform to the existing ``RetrievalAdapter`` Protocol from the R2 seam,
return ``list[Hit]``, respect ``k``, and be deterministic over a tiny synthetic
corpus. They are independent of the FathomDB engine (no native extension, no DB).
"""

from __future__ import annotations

from eval.baselines_084 import LongContextAdapter, VectorRagAdapter
from eval.r2_parity_eval import Hit, RetrievalAdapter

_DOCS = {
    "d1": "vaccines protect against measles and influenza outbreaks",
    "d2": "the central bank raised interest rates to fight inflation",
    "d3": "a new cancer screening guideline was issued by health officials",
    "d4": "drought conditions threaten the regional wheat harvest this year",
    "d5": "researchers report progress on an influenza vaccine candidate",
}


def _is_hit_list(x: object) -> bool:
    return isinstance(x, list) and all(isinstance(h, Hit) for h in x)


# --------------------------------------------------------------------------- #
# VectorRagAdapter
# --------------------------------------------------------------------------- #


def test_vector_conforms_to_protocol() -> None:
    adapter = VectorRagAdapter(_DOCS)
    assert isinstance(adapter, RetrievalAdapter)  # runtime_checkable structural check
    assert adapter.name == "vector_rag"


def test_vector_returns_hit_list_and_respects_k() -> None:
    adapter = VectorRagAdapter(_DOCS)
    hits = adapter.retrieve("influenza vaccine", k=2)
    assert _is_hit_list(hits)
    assert len(hits) == 2
    assert len(adapter.retrieve("influenza vaccine", k=100)) == len(_DOCS)


def test_vector_deterministic_and_relevant() -> None:
    adapter = VectorRagAdapter(_DOCS)
    a = adapter.retrieve("influenza vaccine", k=3)
    b = adapter.retrieve("influenza vaccine", k=3)
    assert [(h.doc_id, h.score) for h in a] == [(h.doc_id, h.score) for h in b]
    # the two influenza docs (d1, d5) should outrank the unrelated bank/drought docs
    top_ids = {h.doc_id for h in a}
    assert "d5" in top_ids
    assert hits_scores_nonincreasing(a)


def hits_scores_nonincreasing(hits: list[Hit]) -> bool:
    return all(hits[i].score >= hits[i + 1].score for i in range(len(hits) - 1))


# --------------------------------------------------------------------------- #
# LongContextAdapter
# --------------------------------------------------------------------------- #


def test_long_context_conforms_to_protocol() -> None:
    adapter = LongContextAdapter(_DOCS)
    assert isinstance(adapter, RetrievalAdapter)
    assert adapter.name == "long_context"


def test_long_context_returns_hit_list_and_respects_k() -> None:
    adapter = LongContextAdapter(_DOCS, char_budget=10_000)
    hits = adapter.retrieve("anything at all", k=3)
    assert _is_hit_list(hits)
    assert len(hits) == 3  # capped by k, budget is generous


def test_long_context_respects_char_budget() -> None:
    # budget below one doc still yields exactly one (never an empty context).
    adapter = LongContextAdapter(_DOCS, char_budget=5)
    hits = adapter.retrieve("q", k=100)
    assert len(hits) == 1


def test_long_context_deterministic_and_query_independent() -> None:
    adapter = LongContextAdapter(_DOCS, char_budget=10_000)
    a = adapter.retrieve("influenza", k=10)
    b = adapter.retrieve("a completely different question", k=10)
    assert [h.doc_id for h in a] == [h.doc_id for h in b] == sorted(_DOCS)
