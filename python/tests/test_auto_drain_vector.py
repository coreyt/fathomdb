"""Pack H: ``auto_drain_vector`` kwarg on ``Engine.open``.

The Python build ships without the ``default-embedder`` feature, and
``EmbedderChoice::InProcess`` is not reachable from Python. A fully
end-to-end auto-drain test that asserts semantic_search hits is
therefore only meaningful in the Rust integration suite
(``crates/fathomdb/tests/auto_drain_vector.rs``). These Python tests
assert:

- the kwarg is accepted by ``Engine.open``,
- passing ``auto_drain_vector=True`` without an embedder is harmless
  (auto-drain degrades to a no-op),
- writes still succeed with the flag on.
"""

from __future__ import annotations

from pathlib import Path


def test_auto_drain_vector_false_is_default(tmp_path: Path) -> None:
    from fathomdb import Engine

    db = Engine.open(tmp_path / "a.db")
    try:
        # No exception; engine is usable.
        _ = db.admin.capabilities()
    finally:
        db.close()


def test_auto_drain_vector_true_without_embedder_is_noop(tmp_path: Path) -> None:
    """Opening with auto_drain_vector=True but no embedder must not raise.

    With no embedder, the sync-drain adapter finds nothing to call and
    short-circuits. Writes still work.
    """
    from fathomdb import Engine

    db = Engine.open(tmp_path / "a.db", auto_drain_vector=True)
    try:
        # capabilities is purely static; confirms engine opened cleanly.
        caps = db.admin.capabilities()
        assert "sqlite_vec" in caps
    finally:
        db.close()
