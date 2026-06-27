"""X1 EXP-OBS explain parity harness (Python SDK) — 0.8.8 Slice 10.

Opens a REAL engine, writes a small corpus, and exercises
`search(..., explain=True/False)` end-to-end across the FFI. Asserts the
cross-binding contract the TypeScript harness
(`src/ts/tests/exp-obs-explain.test.ts`) mirrors:

  1. Carrier gating — explain=False ⇒ `explanation is None` AND results are
     byte-identical to the same query without explain; explain=True ⇒ present.
  2. QueryTrace — all 12 fields present + correctly typed; `alpha` exact (0.3
     default); `embedder_id` is a str ("" sentinel when no embedder).
  3. per_hit ↔ results alignment — same length, same order, `per_hit[i].id ==
     results[i].id`.
  4. The three self-consistency identities — `arm == branch`, `ce_score ==
     hit.ce_score`, `blended == hit.score`.
  5. None/Some rank fidelity — at least one arm rank is None and at least one is
     an int across the pool (no embedder ⇒ vector_rank None, text_rank set).
  6. `"graph_arm"` is a member of the SoftFallbackBranch Literal (the Slice-10
     prereq that brought Python on-contract with Rust/TS).
"""

from __future__ import annotations

import time
import typing

from fathomdb import Engine, Explanation, PerHitExplain, QueryTrace, SearchResult
from fathomdb.types import SoftFallbackBranch


def _search_after_projection(engine: Engine, query: str, *, explain: bool) -> SearchResult:
    deadline = time.monotonic() + 10.0
    last = engine.search(query, explain=explain)
    while time.monotonic() < deadline:
        last = engine.search(query, explain=explain)
        if last.results:
            return last
        time.sleep(0.02)
    return last


def _seed(engine: Engine) -> None:
    for body in ["hybrid retrieval alpha", "hybrid retrieval beta", "hybrid retrieval gamma"]:
        engine.write([{"kind": "doc", "body": body}])
    engine.drain(timeout_s=30)


def test_exp_obs_carrier_gating_and_byte_stability(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed(engine)
        plain = _search_after_projection(engine, "hybrid", explain=False)
        assert plain.results, "expected hits"
        # (1) default path suppresses the sidecar.
        assert plain.explanation is None

        explained = engine.search("hybrid", explain=True)
        # (1) on path populates it; results byte-identical to the plain call.
        assert explained.explanation is not None
        assert isinstance(explained.explanation, Explanation)
        assert [h.id for h in explained.results] == [h.id for h in plain.results]
        assert [h.score for h in explained.results] == [h.score for h in plain.results]
        assert explained.projection_cursor == plain.projection_cursor
    finally:
        engine.close()


def test_exp_obs_trace_and_per_hit_fidelity(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed(engine)
        result = _search_after_projection(engine, "hybrid", explain=True)
        exp = result.explanation
        assert exp is not None

        # (2) QueryTrace — 12 fields, typed; alpha exact; embedder_id is a str.
        t: QueryTrace = exp.trace
        assert isinstance(t.query_chars, int) and t.query_chars == len("hybrid")
        assert isinstance(t.k, int)
        assert isinstance(t.rerank_depth, int) and t.rerank_depth == 0
        assert isinstance(t.pool_n, int)
        assert t.alpha == 0.3
        assert isinstance(t.use_graph_arm, bool) and t.use_graph_arm is False
        assert isinstance(t.recency, bool)
        assert isinstance(t.embedder_id, str)
        assert isinstance(t.ce_active, bool) and t.ce_active is False
        assert isinstance(t.vector_hits, int)
        assert isinstance(t.text_hits, int)
        assert isinstance(t.graph_hits, int)

        # (3) per_hit ↔ results alignment + (4) the three identities.
        assert len(exp.per_hit) == len(result.results)
        for p, h in zip(exp.per_hit, result.results):
            assert isinstance(p, PerHitExplain)
            assert p.id == h.id
            assert p.arm == h.branch
            assert p.ce_score == h.ce_score
            assert p.blended == h.score
            # fused_score is the RAW RRF value (not normalized to [0,1]).
            assert 0.0 < p.fused_score < 1.0

        # (5) None/Some rank fidelity across the pool (no embedder ⇒ vector_rank
        # None, text_rank set on text-arm hits).
        ranks = [(p.vector_rank, p.text_rank, p.graph_rank) for p in exp.per_hit]
        assert any(r[0] is None for r in ranks), "expected at least one None rank"
        assert any(
            isinstance(v, int) for r in ranks for v in r if v is not None
        ), "expected at least one int rank"
    finally:
        engine.close()


def test_graph_arm_is_in_softfallbackbranch_literal() -> None:
    # (6) The Slice-10 prereq: Python's Literal is on-contract with Rust/TS.
    assert "graph_arm" in typing.get_args(SoftFallbackBranch)


def test_explain_param_type_validation(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        # Mirrors the TS guard (cross-SDK parity): non-bool explain rejected.
        import pytest

        with pytest.raises(TypeError):
            engine.search("hybrid", explain=typing.cast(bool, "yes"))
    finally:
        engine.close()
