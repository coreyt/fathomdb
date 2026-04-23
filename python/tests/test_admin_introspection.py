"""Pack H: admin introspection + batch configure_vec_kinds (Python parity)."""

from __future__ import annotations

import sqlite3
from pathlib import Path


def _seed_active_profile(db_path: Path, dimensions: int = 384) -> int:
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


def _seed_node_and_chunk(
    db_path: Path, logical_id: str, kind: str, chunk_id: str
) -> None:
    conn = sqlite3.connect(str(db_path))
    try:
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, source_ref, created_at) "
            "VALUES (?, ?, ?, '{}', 'test', strftime('%s','now'))",
            (f"row-{chunk_id}", logical_id, kind),
        )
        conn.execute(
            "INSERT INTO chunks (id, node_logical_id, text_content, created_at) "
            "VALUES (?, ?, ?, strftime('%s','now'))",
            (chunk_id, logical_id, f"body for {chunk_id}"),
        )
        conn.commit()
    finally:
        conn.close()


def test_capabilities_reports_static_surface(tmp_path: Path) -> None:
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()

    db = Engine.open(db_path)
    try:
        caps = db.admin.capabilities()
    finally:
        db.close()

    assert isinstance(caps, dict)
    assert "sqlite_vec" in caps
    assert isinstance(caps["sqlite_vec"], bool)
    assert "fts_tokenizers" in caps
    assert "recall-optimized-english" in caps["fts_tokenizers"]
    assert "embedders" in caps
    assert "builtin" in caps["embedders"]
    builtin = caps["embedders"]["builtin"]
    assert "available" in builtin
    assert caps["schema_version"] >= 24
    assert caps["fathomdb_version"]


def test_current_config_empty_on_fresh_engine(tmp_path: Path) -> None:
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()

    db = Engine.open(db_path)
    try:
        cfg = db.admin.current_config()
    finally:
        db.close()

    assert cfg["active_embedding_profile"] is None
    assert cfg["vec_kinds"] == {}
    assert cfg["fts_kinds"] == {}
    wq = cfg["work_queue"]
    assert wq["pending_incremental"] == 0
    assert wq["pending_backfill"] == 0


def test_current_config_reflects_configured_kind(tmp_path: Path) -> None:
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()
    _seed_active_profile(db_path)
    for i in range(3):
        _seed_node_and_chunk(db_path, f"note:{i}", "Note", f"chunk-{i}")

    db = Engine.open(db_path)
    try:
        db.admin.configure_vec("Note", source="chunks")
        cfg = db.admin.current_config()
    finally:
        db.close()

    assert cfg["active_embedding_profile"] is not None
    assert cfg["active_embedding_profile"]["model_identity"] == "test/model"
    assert "Note" in cfg["vec_kinds"]
    assert cfg["vec_kinds"]["Note"]["enabled"] is True
    assert cfg["work_queue"]["pending_backfill"] == 3


def test_describe_kind_unconfigured(tmp_path: Path) -> None:
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()

    db = Engine.open(db_path)
    try:
        desc = db.admin.describe_kind("Missing")
    finally:
        db.close()

    assert desc["kind"] == "Missing"
    assert desc["vec"] is None
    assert desc["fts"] is None
    assert desc["chunk_count"] == 0


def test_describe_kind_with_chunks(tmp_path: Path) -> None:
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()
    _seed_active_profile(db_path)
    for i in range(2):
        _seed_node_and_chunk(db_path, f"note:{i}", "Note", f"chunk-{i}")

    db = Engine.open(db_path)
    try:
        db.admin.configure_vec("Note", source="chunks")
        desc = db.admin.describe_kind("Note")
    finally:
        db.close()

    assert desc["kind"] == "Note"
    assert desc["vec"] is not None
    assert desc["vec"]["enabled"] is True
    assert desc["chunk_count"] == 2
    assert desc["embedding_identity"] == "test/model"


def test_configure_vec_kinds_batch(tmp_path: Path) -> None:
    from fathomdb import Engine

    db_path = tmp_path / "agent.db"
    Engine.open(db_path).close()
    _seed_active_profile(db_path)
    _seed_node_and_chunk(db_path, "a:0", "KindA", "chunk-a-0")
    _seed_node_and_chunk(db_path, "b:0", "KindB", "chunk-b-0")

    db = Engine.open(db_path)
    try:
        outcomes = db.admin.configure_vec_kinds(
            [("KindA", "chunks"), {"kind": "KindB", "source": "chunks"}]
        )
    finally:
        db.close()

    assert isinstance(outcomes, list)
    assert len(outcomes) == 2
    assert outcomes[0]["kind"] == "KindA"
    assert outcomes[0]["enqueued_backfill_rows"] == 1
    assert outcomes[0]["was_already_enabled"] is False
    assert outcomes[1]["kind"] == "KindB"
    assert outcomes[1]["enqueued_backfill_rows"] == 1
