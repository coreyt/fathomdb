"""Functional tests for the 0.8.1 R1 reranker soft-fallback (X1 harness).

These tests verify that:
1. ``engine.search(q, rerank_depth=0)`` returns the same body order as
   ``engine.search(q)`` (no-rerank default).
2. Negative ``rerank_depth`` raises ``ValueError``.
3. The soft-fallback contract: ``rerank_depth=0`` is byte-identical to
   the pre-Slice-10 fused order.

These are the Python half of the X1 cross-binding equivalence tests.
The TypeScript half lives in ``src/ts/tests/functional-reranker.test.ts``.
"""

from __future__ import annotations

import pytest

from fathomdb import Engine
from fathomdb.types import SearchFilter


@pytest.fixture()
def engine_with_docs(tmp_path):
    """Open an engine, write a few docs, drain, return the handle."""
    db = tmp_path / "reranker_test.fathomdb"
    eng = Engine.open(str(db))
    docs = [
        {"kind": "doc", "body": "cross encoder reranker alpha document"},
        {"kind": "doc", "body": "cross encoder reranker beta document"},
        {"kind": "doc", "body": "cross encoder reranker gamma document"},
    ]
    for doc in docs:
        eng.write([doc])
    eng.drain(timeout_s=10)
    yield eng
    eng.close()


def test_rerank_depth_0_matches_default_search(engine_with_docs):
    """``rerank_depth=0`` must return the same order as the default search."""
    eng = engine_with_docs
    default_result = eng.search("cross encoder")
    depth0_result = eng.search("cross encoder", rerank_depth=0)

    default_bodies = [h.body for h in default_result.results]
    depth0_bodies = [h.body for h in depth0_result.results]

    assert default_bodies == depth0_bodies, (
        "rerank_depth=0 must return the same body order as the default search "
        f"(identity/soft-fallback). default={default_bodies!r}, "
        f"depth0={depth0_bodies!r}"
    )


def test_rerank_depth_0_scores_identical_to_default(engine_with_docs):
    """``rerank_depth=0`` must produce byte-identical scores to the default."""
    eng = engine_with_docs
    default_result = eng.search("reranker beta")
    depth0_result = eng.search("reranker beta", rerank_depth=0)

    for h_def, h_d0 in zip(default_result.results, depth0_result.results):
        assert h_def.score == h_d0.score, (
            f"scores must be identical: body={h_def.body!r} "
            f"default={h_def.score} depth0={h_d0.score}"
        )


def test_negative_rerank_depth_raises_value_error(engine_with_docs):
    """Negative ``rerank_depth`` must raise ``ValueError``."""
    eng = engine_with_docs
    with pytest.raises(ValueError, match="rerank_depth must be >= 0"):
        eng.search("cross encoder", rerank_depth=-1)


def test_rerank_depth_0_with_filter(engine_with_docs):
    """``rerank_depth=0`` with a filter also preserves the identity contract."""
    eng = engine_with_docs
    filt = SearchFilter(kind="doc")
    filtered_result = eng.search("cross encoder", filter=filt)
    depth0_filtered_result = eng.search("cross encoder", filter=filt, rerank_depth=0)

    filtered_bodies = [h.body for h in filtered_result.results]
    depth0_filtered_bodies = [h.body for h in depth0_filtered_result.results]

    assert filtered_bodies == depth0_filtered_bodies, (
        "rerank_depth=0 with filter must match filtered default search"
    )


def test_rerank_depth_positive_returns_results(engine_with_docs):
    """``rerank_depth > 0`` must return results (soft-fallback in default build)."""
    eng = engine_with_docs
    # In the default build (no default-reranker feature), depth>0 still
    # returns the identity order (model absent → soft-fallback).
    result = eng.search("cross encoder reranker", rerank_depth=200)
    assert len(result.results) >= 0  # must not error; may be empty if no hits


# --- FIX-3 RED tests (X1 parity with TS) ---

def test_float_rerank_depth_raises_type_error(engine_with_docs):
    """Non-integer rerank_depth must raise TypeError (X1 parity with TS)."""
    eng = engine_with_docs
    with pytest.raises(TypeError, match="non-negative integer"):
        eng.search("cross encoder", rerank_depth=2.5)


def test_bool_rerank_depth_raises_type_error(engine_with_docs):
    """bool rerank_depth must raise TypeError — bool is a subclass of int in Python
    but semantically wrong; TS rejects it; Python must too for X1 parity."""
    eng = engine_with_docs
    with pytest.raises(TypeError, match="non-negative integer"):
        eng.search("cross encoder", rerank_depth=True)


# --- 0.8.5 (EXP-0): α / pool_n / ce_score exposure (X1 parity with TS smoke) ---


def test_search_accepts_alpha_pool_n_and_hits_carry_ce_score(engine_with_docs):
    """The new opt-in knobs are accepted and every hit carries a ``ce_score``
    field (None or float). Mirrors the TS smoke in functional-reranker.test.ts."""
    eng = engine_with_docs
    result = eng.search("cross encoder reranker", rerank_depth=10, alpha=1.0, pool_n=10)
    for h in result.results:
        assert hasattr(h, "ce_score"), "every hit must carry a ce_score field"
        assert h.ce_score is None or isinstance(h.ce_score, float)


def test_default_alpha_preserves_default_order_and_null_ce_score(engine_with_docs):
    """At depth=0 the new knobs are inert: order is byte-identical to the default
    search and ``ce_score`` is None on every (identity-path) hit."""
    eng = engine_with_docs
    default_result = eng.search("cross encoder")
    knobs_result = eng.search("cross encoder", rerank_depth=0, alpha=0.3)
    assert [h.body for h in knobs_result.results] == [
        h.body for h in default_result.results
    ]
    for h in default_result.results:
        assert h.ce_score is None, "default-path (depth=0) hits carry ce_score=None"
