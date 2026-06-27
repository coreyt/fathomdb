"""0.8.4 — `Engine.embed()`: the read-path embed primitive.

Exposes the engine's pinned default embedder (`fathomdb-bge-small-en-v1.5`) as a
direct `text -> vector` call, so callers (e.g. the Tier-2 coverage-index clustering)
embed under the engine's OWN identity rather than a parallel, possibly-divergent
embedder. Network-hitting (weights fetched on first use) — honours
`FATHOMDB_SKIP_NETWORK_TESTS` per EU-5c.
"""

from __future__ import annotations

import json
import math
import os
from pathlib import Path

import pytest

from fathomdb import Engine
from fathomdb.errors import EmbedderNotConfiguredError

_DIM = 384  # fathomdb-bge-small-en-v1.5

# 0.8.6 Slice 10 — cross-binding golden shared with the TypeScript
# `functional-embed.test.ts`. Both bindings embed `anchor_text` and assert the
# result matches this committed vector within `tolerance`, proving Py ≡ TS.
_GOLDEN_PATH = Path(__file__).resolve().parents[2] / "conformance" / "embed-anchor-golden.json"


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping network-hitting test")


def _cosine(a: list[float], b: list[float]) -> float:
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    return dot / (na * nb) if na and nb else 0.0


def test_embed_returns_fixed_dim_float_vector(db_path: str) -> None:
    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        vec = engine.embed("influenza vaccine clinical trial")
        assert isinstance(vec, list)
        assert len(vec) == _DIM
        assert all(isinstance(x, float) for x in vec)
        assert any(x != 0.0 for x in vec)  # not a zero vector
    finally:
        engine.close()


def test_embed_is_deterministic(db_path: str) -> None:
    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        a = engine.embed("the central bank raised interest rates")
        b = engine.embed("the central bank raised interest rates")
        assert a == b
    finally:
        engine.close()


def test_embed_is_semantic(db_path: str) -> None:
    # A real semantic embedder: two paraphrases of the same topic are closer than
    # an unrelated sentence (would NOT hold for a pure hashing bag-of-words).
    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        flu_a = engine.embed("a new influenza vaccine showed progress in trials")
        flu_b = engine.embed("researchers report advances in a flu immunization candidate")
        bank = engine.embed("the treasury yield curve inverted amid inflation fears")
        assert _cosine(flu_a, flu_b) > _cosine(flu_a, bank)
    finally:
        engine.close()


def test_embed_without_embedder_raises(db_path: str) -> None:
    # No embedder configured -> the read-path primitive fails closed, same contract
    # as the vector-write path.
    engine = Engine.open(db_path, use_default_embedder=False)
    try:
        with pytest.raises(EmbedderNotConfiguredError):
            engine.embed("anything")
    finally:
        engine.close()


def test_embed_cross_binding_golden_anchor(db_path: str) -> None:
    """Cross-binding equivalence anchor for `Engine.embed` (Py ≡ TS).

    The TypeScript harness (``functional-embed.test.ts``) embeds the SAME
    ``anchor_text`` and asserts the SAME committed golden within the SAME
    tolerance — proving both bindings produce the same vector under the engine's
    pinned embedder identity.
    """
    _skip_if_no_network()
    golden = json.loads(_GOLDEN_PATH.read_text())
    assert golden["dim"] == _DIM
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        vec = engine.embed(golden["anchor_text"])
        assert len(vec) == len(golden["vector"])
        max_abs_diff = max(abs(a - b) for a, b in zip(vec, golden["vector"]))
        assert max_abs_diff <= golden["tolerance"], (
            f"embed(anchor) must match the committed golden within "
            f"{golden['tolerance']}; max abs diff was {max_abs_diff}"
        )
    finally:
        engine.close()
