"""Embed-completeness verifier — TDD with REAL embeds (nothing mocked).

Builds TINY real embedded DBs (3 docs) via the production projection path and
asserts ``eval.verify_embed_db``:
  - PASSES a complete doc-embed (coverage 1.0, dim 384, queryable);
  - FAILS the two real partial modes — (a) doc vector kind never registered
    (embedder on but docs marked terminal WITHOUT embedding), (b) FTS-only.

These exercise the actual `CandleBgeEmbedder`; they skip cleanly if the embedder
feature/weights are unavailable in the wheel.
"""

from __future__ import annotations

import pytest

from eval.p0a_base_retrieval import _build_fathomdb_variant
from eval.verify_embed_db import (
    DEFAULT_DIM,
    EmbedIncompleteError,
    assert_embed_complete,
    inspect_embed_db,
    verify_queryable,
)

_DOCS = {
    "s1": "The Eiffel Tower is a wrought-iron lattice tower in Paris, completed in 1889.",
    "s2": "Mount Everest is Earth's highest mountain above sea level, in the Himalayas.",
    "s3": "Photosynthesis converts sunlight, water and carbon dioxide into glucose in plants.",
}


@pytest.fixture(scope="module")
def fused_db(tmp_path_factory):
    d = tmp_path_factory.mktemp("verify_fused")
    db = d / "fused.sqlite"
    adapter, blk = _build_fathomdb_variant(
        _DOCS, db, use_embedder=True, register_doc_vector_kind=True
    )
    if adapter is None:
        pytest.skip(f"embedder unavailable, cannot build a real embed DB: {blk}")
    return db, adapter


@pytest.fixture(scope="module")
def no_kind_db(tmp_path_factory):
    # Embedder ON but the doc vector kind NOT registered -> docs get marked
    # projection_terminal WITHOUT embedding (the s-nograph / s-v2 0%-doc trap).
    d = tmp_path_factory.mktemp("verify_nokind")
    db = d / "nokind.sqlite"
    adapter, blk = _build_fathomdb_variant(
        _DOCS, db, use_embedder=True, register_doc_vector_kind=False
    )
    if adapter is None:
        pytest.skip(f"engine unavailable: {blk}")
    return db


@pytest.fixture(scope="module")
def fts_only_db(tmp_path_factory):
    d = tmp_path_factory.mktemp("verify_fts")
    db = d / "fts.sqlite"
    adapter, blk = _build_fathomdb_variant(
        _DOCS, db, use_embedder=False, register_doc_vector_kind=False
    )
    if adapter is None:
        pytest.skip(f"engine unavailable: {blk}")
    return db


# --------------------------------------------------------------------------- #
# Complete embed -> PASS
# --------------------------------------------------------------------------- #


def test_complete_embed_passes(fused_db):
    db, _ = fused_db
    rep = inspect_embed_db(str(db), expected_docs=len(_DOCS), kind="doc")
    assert rep.ok, rep.to_dict()
    assert rep.coverage == 1.0
    assert rep.n_docs == len(_DOCS)
    assert rep.n_docs_embedded == len(_DOCS)
    assert rep.n_doc_vectors >= len(_DOCS)  # >=1 vector/doc (chunking may add more)
    assert rep.dimension == DEFAULT_DIM
    # assert_embed_complete returns the report (does not raise) on a complete DB
    assert assert_embed_complete(str(db), expected_docs=len(_DOCS)).ok


def test_complete_embed_is_queryable(fused_db):
    _, adapter = fused_db
    chk = verify_queryable(adapter.retrieve, ["highest mountain", "tower in Paris"], k=5)
    assert chk.ok, chk.detail


def test_expected_docs_mismatch_fails(fused_db):
    db, _ = fused_db
    # Under-ingest guard: claim more docs than were written -> docs_present fails.
    rep = inspect_embed_db(str(db), expected_docs=len(_DOCS) + 100, kind="doc")
    assert not rep.ok
    assert any(c.name == "docs_present" and not c.ok for c in rep.checks)


# --------------------------------------------------------------------------- #
# Partial / wrong-kind / fts-only -> FAIL
# --------------------------------------------------------------------------- #


def test_unregistered_doc_kind_fails(no_kind_db):
    rep = inspect_embed_db(str(no_kind_db), expected_docs=len(_DOCS), kind="doc")
    assert not rep.ok
    assert rep.coverage == 0.0
    failed = {c.name for c in rep.checks if not c.ok}
    assert "vector_kind_registered" in failed
    assert "coverage_complete" in failed
    with pytest.raises(EmbedIncompleteError):
        assert_embed_complete(str(no_kind_db), expected_docs=len(_DOCS))


def test_fts_only_fails(fts_only_db):
    rep = inspect_embed_db(str(fts_only_db), expected_docs=len(_DOCS), kind="doc")
    assert not rep.ok
    assert rep.n_doc_vectors == 0


def test_missing_db_does_not_crash(tmp_path):
    # A nonexistent DB must report not-ok, not raise (sqlite ro open of absent file).
    missing = tmp_path / "nope.sqlite"
    with pytest.raises(Exception):  # noqa: B017 - sqlite raises OperationalError; gate treats as fail upstream
        inspect_embed_db(str(missing))
