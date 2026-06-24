"""0.8.4 — `Engine.embed()`: the read-path embed primitive.

Exposes the engine's pinned default embedder (`fathomdb-bge-small-en-v1.5`) as a
direct `text -> vector` call, so callers (e.g. the Tier-2 coverage-index clustering)
embed under the engine's OWN identity rather than a parallel, possibly-divergent
embedder. Network-hitting (weights fetched on first use) — honours
`FATHOMDB_SKIP_NETWORK_TESTS` per EU-5c.
"""

from __future__ import annotations

import math
import os

import pytest

from fathomdb import Engine
from fathomdb.errors import EmbedderNotConfiguredError

_DIM = 384  # fathomdb-bge-small-en-v1.5


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
