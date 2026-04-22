"""Pack F2: Python semantic_search + raw_vector_search bindings.

Scope:
- `Query.semantic_search(text, limit)` emits `FfiQueryStep::SemanticSearch`.
- `Query.raw_vector_search(vec, limit)` emits `FfiQueryStep::RawVectorSearch`;
  accepts list / tuple / numpy array and coerces to list[float].
- `Query.vector_search(text_or_vec, limit)` is a DeprecationWarning shim that
  routes to the new methods based on argument type.
- `AdminClient.drain_vector_projection(timeout_ms)` calls the EngineCore
  `drain_vector_projection` pyo3 method and returns a parsed dict.
- Error-class mapping: the engine's `embedder_not_configured`,
  `kind_not_vector_indexed`, and `dimension_mismatch` variants surface as
  `FathomError` (the generic engine-error Python class) with descriptive
  messages.

Capability notes:
- The Python build does NOT include `default-embedder`, and
  `EmbedderChoice::InProcess(Arc<dyn QueryEmbedder>)` is not reachable
  from Python. An end-to-end semantic_search tripwire that returns real
  hits therefore cannot run here — those scenarios are covered in the
  Rust integration suite (`crates/fathomdb/tests/semantic_search_ffi.rs`).
- For error-path and deprecation-shim tests we seed state via sqlite3
  (active profile, enabled vec_<kind>) and assert on the wire-level error
  returned from the FFI.
"""

from __future__ import annotations

import json
import sqlite3
import warnings
from pathlib import Path
from unittest.mock import MagicMock


def _seed_active_profile(db_path: Path, dimensions: int = 4) -> int:
    """Insert an active embedding profile row directly via sqlite3.

    Mirrors the seeding helper used by test_configure_vec_per_kind.py.
    """
    conn = sqlite3.connect(str(db_path))
    try:
        conn.execute(
            "INSERT INTO vector_embedding_profiles "
            "(profile_name, model_identity, model_version, dimensions, "
            "normalization_policy, max_tokens, active, activated_at, created_at) "
            "VALUES ('test-profile', 'test/model', 'v1', ?, 'l2', 512, 1, "
            "strftime('%s','now'), strftime('%s','now'))",
            (dimensions,),
        )
        conn.commit()
        return conn.execute(
            "SELECT profile_id FROM vector_embedding_profiles WHERE active = 1"
        ).fetchone()[0]
    finally:
        conn.close()


# ── semantic_search wire emission ────────────────────────────────────


def test_semantic_search_emits_expected_wire_step(tmp_path: Path) -> None:
    """`.semantic_search(text, limit)` should push a `semantic_search` step.

    We verify the wire by inspecting the compiled AST JSON passed to the
    underlying FFI. Using a MagicMock core lets us intercept without
    touching the database.
    """
    from fathomdb._query import Query

    core = MagicMock()
    core.execute_ast.return_value = json.dumps(
        {"nodes": [], "runs": [], "steps": [], "actions": [], "was_degraded": False}
    )
    q = Query(core, "KnowledgeItem")
    q.semantic_search("cats", 5).execute()
    payload = json.loads(core.execute_ast.call_args[0][0])
    assert payload["steps"] == [
        {"type": "semantic_search", "text": "cats", "limit": 5}
    ]


def test_raw_vector_search_emits_expected_wire_step_from_list() -> None:
    """`.raw_vector_search([floats], limit)` pushes `raw_vector_search`."""
    from fathomdb._query import Query

    core = MagicMock()
    core.execute_ast.return_value = json.dumps(
        {"nodes": [], "runs": [], "steps": [], "actions": [], "was_degraded": False}
    )
    q = Query(core, "KnowledgeItem")
    q.raw_vector_search([0.1, 0.2, 0.3, 0.4], 7).execute()
    payload = json.loads(core.execute_ast.call_args[0][0])
    assert payload["steps"] == [
        {
            "type": "raw_vector_search",
            "vector": [0.1, 0.2, 0.3, 0.4],
            "limit": 7,
        }
    ]


def test_raw_vector_search_accepts_tuple_and_numpy_like() -> None:
    """raw_vector_search must accept tuple and objects with `.tolist()`."""
    from fathomdb._query import Query

    core = MagicMock()
    core.execute_ast.return_value = json.dumps(
        {"nodes": [], "runs": [], "steps": [], "actions": [], "was_degraded": False}
    )

    # Tuple path.
    Query(core, "K").raw_vector_search((0.5, 1.5), 3).execute()
    payload = json.loads(core.execute_ast.call_args[0][0])
    assert payload["steps"][0]["vector"] == [0.5, 1.5]

    # Numpy-array-like (object with .tolist()).
    class FakeNdarray:
        def tolist(self) -> list[float]:
            return [1.0, 2.0, 3.0]

    Query(core, "K").raw_vector_search(FakeNdarray(), 3).execute()
    payload = json.loads(core.execute_ast.call_args[0][0])
    assert payload["steps"][0]["vector"] == [1.0, 2.0, 3.0]


def test_raw_vector_search_rejects_empty_vector() -> None:
    """Empty vectors are rejected client-side with ValueError."""
    import pytest

    from fathomdb._query import Query

    core = MagicMock()
    with pytest.raises(ValueError):
        Query(core, "K").raw_vector_search([], 3)


# ── vector_search deprecation shim ───────────────────────────────────


def test_vector_search_text_deprecated_routes_to_semantic_search() -> None:
    """`vector_search(text)` is deprecated; routes to semantic_search."""
    from fathomdb._query import Query

    core = MagicMock()
    core.execute_ast.return_value = json.dumps(
        {"nodes": [], "runs": [], "steps": [], "actions": [], "was_degraded": False}
    )

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        Query(core, "K").vector_search("cats", 5).execute()

    assert any(
        issubclass(w.category, DeprecationWarning)
        and "semantic_search" in str(w.message)
        for w in caught
    ), f"expected DeprecationWarning mentioning semantic_search, got {caught!r}"
    payload = json.loads(core.execute_ast.call_args[0][0])
    assert payload["steps"][0]["type"] == "semantic_search"
    assert payload["steps"][0]["text"] == "cats"


def test_vector_search_vec_deprecated_routes_to_raw_vector_search() -> None:
    """`vector_search([floats])` is deprecated; routes to raw_vector_search."""
    from fathomdb._query import Query

    core = MagicMock()
    core.execute_ast.return_value = json.dumps(
        {"nodes": [], "runs": [], "steps": [], "actions": [], "was_degraded": False}
    )

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        Query(core, "K").vector_search([0.1, 0.2, 0.3, 0.4], 5).execute()

    assert any(
        issubclass(w.category, DeprecationWarning)
        and "raw_vector_search" in str(w.message)
        for w in caught
    ), f"expected DeprecationWarning mentioning raw_vector_search, got {caught!r}"
    payload = json.loads(core.execute_ast.call_args[0][0])
    assert payload["steps"][0]["type"] == "raw_vector_search"
    assert payload["steps"][0]["vector"] == [0.1, 0.2, 0.3, 0.4]


# ── drain_vector_projection admin binding ─────────────────────────────


def test_drain_vector_projection_forwards_timeout_and_parses_json(
    tmp_path: Path,
) -> None:
    """AdminClient.drain_vector_projection serialises the timeout and parses the result."""
    from fathomdb._admin import AdminClient

    mock_core = MagicMock()
    mock_core.drain_vector_projection.return_value = json.dumps(
        {"processed": 0, "failed": 0, "remaining": 0}
    )
    admin = AdminClient(mock_core)
    result = admin.drain_vector_projection(timeout_ms=1234)
    assert result == {"processed": 0, "failed": 0, "remaining": 0}
    args, _ = mock_core.drain_vector_projection.call_args
    assert json.loads(args[0]) == {"timeout_ms": 1234}


def test_drain_vector_projection_without_embedder_raises(tmp_path: Path) -> None:
    """Real engine without an embedder raises FathomError on drain."""
    import pytest

    from fathomdb import Engine, FathomError

    db = Engine.open(tmp_path / "agent.db")
    with pytest.raises(FathomError):
        db.admin.drain_vector_projection(timeout_ms=100)


# ── Error-path coverage for semantic_search / raw_vector_search ──────
#
# These tests exercise the engine's Pack F1.5 error contract. They depend
# on the FFI routing `semantic_search` / `raw_vector_search` steps through
# the coordinator's `execute_compiled_semantic_search` /
# `execute_compiled_raw_vector_search` dispatch. If the FFI route is not
# yet wired into `execute_ast`, these tests surface the gap.


def test_raw_vector_search_without_active_profile_raises(tmp_path: Path) -> None:
    """No active embedding profile → EmbedderNotConfigured (surfaced as FathomError)."""
    import pytest

    from fathomdb import Engine, FathomError

    db = Engine.open(tmp_path / "agent.db")
    with pytest.raises(FathomError) as exc:
        db.nodes("KnowledgeItem").raw_vector_search(
            [0.1, 0.2, 0.3, 0.4], 5
        ).execute()
    # Match on the wire-error hint — the Rust render mentions "embedder".
    assert "embedder" in str(exc.value).lower()


def test_raw_vector_search_without_configured_kind_raises(tmp_path: Path) -> None:
    """Active profile but no vector index for kind → KindNotVectorIndexed."""
    import pytest

    from fathomdb import Engine, FathomError

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()
    _seed_active_profile(db_path, dimensions=4)

    db = Engine.open(db_path)
    with pytest.raises(FathomError) as exc:
        db.nodes("KnowledgeItem").raw_vector_search(
            [0.1, 0.2, 0.3, 0.4], 5
        ).execute()
    assert "vector" in str(exc.value).lower() or "kind" in str(exc.value).lower()


def test_raw_vector_search_dimension_mismatch_raises(tmp_path: Path) -> None:
    """vec.len() ≠ profile.dimensions → DimensionMismatch."""
    import pytest

    from fathomdb import Engine, FathomError

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()
    _seed_active_profile(db_path, dimensions=4)

    db = Engine.open(db_path)
    db.admin.configure_vec("KnowledgeItem", source="chunks")
    with pytest.raises(FathomError) as exc:
        db.nodes("KnowledgeItem").raw_vector_search([0.1, 0.2], 5).execute()
    msg = str(exc.value).lower()
    assert "dimension" in msg or "expected" in msg


def test_semantic_search_without_active_profile_raises(tmp_path: Path) -> None:
    """Fresh engine + semantic_search → EmbedderNotConfigured (FathomError)."""
    import pytest

    from fathomdb import Engine, FathomError

    db = Engine.open(tmp_path / "agent.db")
    with pytest.raises(FathomError) as exc:
        db.nodes("KnowledgeItem").semantic_search("cats", 5).execute()
    assert "embedder" in str(exc.value).lower()


def test_semantic_search_without_configured_kind_raises(tmp_path: Path) -> None:
    """Profile configured but no vector index for kind → KindNotVectorIndexed."""
    import pytest

    from fathomdb import Engine, FathomError

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()
    _seed_active_profile(db_path, dimensions=4)

    db = Engine.open(db_path)
    with pytest.raises(FathomError) as exc:
        db.nodes("KnowledgeItem").semantic_search("cats", 5).execute()
    assert "vector" in str(exc.value).lower() or "kind" in str(exc.value).lower()
