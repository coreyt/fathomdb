"""X1 SDK parity — 0.8.20 Slice 15d (R-20-PR / R-20-EAV projection registry).

Drives the two net-new governed verbs through the PyO3 binding by EXECUTION
(not symbol presence): ``Engine.configure_projections`` and
``read.projections``. Mirrors the Rust suite
``src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs`` and
the TS suite ``src/ts/tests/slice15d-projection-registry.test.ts`` (Py ≡ TS).

``sqlite3`` is used only as a READ oracle on a CLOSED database — the "value at
rest" assertion for the EAV store / property-FTS.
"""

from __future__ import annotations

import sqlite3

import pytest

from fathomdb import Engine, ProjectionRole, ProjectionSpec, read
from fathomdb.errors import (
    EngineError,
    ProjectionDestructiveError,
    WriteValidationError,
)

_SOURCE_ID = "py-test:slice15d"

# Slice 15d fix-5 (AC-068a) — an embedded NUL smuggled into a ProjectionSpec /
# drop string. Representable in a Python `str`, valid UTF-8 (so PyO3 String
# extraction accepts it), but must be rejected at the BINDING before the writer
# transaction opens — never persisted in `_fathomdb_projection_registry`.
_NUL = "a\x00b"


def _open(path: str) -> Engine:
    return Engine.open(path, use_default_embedder=False)


def _node(logical_id: str, source: str, body_json: str) -> dict:
    return {"kind": "doc", "body": body_json, "logical_id": logical_id, "source_id": source}


def _spec(name: str, roles: set[str], *, fts: bool = False, vector: bool = False) -> ProjectionSpec:
    return ProjectionSpec(name=name, roles=frozenset(roles), fts=fts, vector=vector)


def _eav_values(path: str, attr_name: str) -> list[str]:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return [
            r[0]
            for r in conn.execute(
                "SELECT attr_value FROM canonical_attributes"
                " WHERE attr_name = ? ORDER BY attr_value",
                (attr_name,),
            ).fetchall()
        ]
    finally:
        conn.close()


def _registry_names(path: str) -> list[str]:
    """READ oracle over the durable registry on a CLOSED database."""
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return [
            r[0]
            for r in conn.execute(
                "SELECT name FROM _fathomdb_projection_registry ORDER BY name"
            ).fetchall()
        ]
    finally:
        conn.close()


def _pfts_match(path: str, attr_name: str, query: str) -> list[int]:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return [
            r[0]
            for r in conn.execute(
                "SELECT write_cursor FROM property_search_index"
                " WHERE attr_name = ? AND property_search_index MATCH ? ORDER BY write_cursor",
                (attr_name, query),
            ).fetchall()
        ]
    finally:
        conn.close()


def test_configure_and_read_projections_round_trip(tmp_path) -> None:
    engine = _open(str(tmp_path / "round_trip.sqlite"))
    try:
        s = _spec("status", {ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}, fts=True)
        engine.configure_projections([s])
        back = read.projections(engine)
        assert len(back) == 1
        got = back[0]
        assert got.name == "status"
        assert got.roles == frozenset({"filterable", "searchable"})
        assert got.fts is True
        assert got.vector is False
    finally:
        engine.close()


def test_idempotent_reregistration_is_a_noop(tmp_path) -> None:
    engine = _open(str(tmp_path / "idempotent.sqlite"))
    try:
        engine.write([_node("N1", "src:1", '{"status":"open"}')])
        s = _spec("status", {ProjectionRole.FILTERABLE})
        first = engine.configure_projections([s])
        assert first.unchanged is False
        assert first.built == ["status"]

        second = engine.configure_projections([s])
        assert second.unchanged is True
        assert second.built == [] and second.dropped == [] and second.deferred == []
    finally:
        engine.close()


def test_property_filter_and_fts_return_correct_rows(tmp_path) -> None:
    path = str(tmp_path / "filter.sqlite")
    engine = _open(path)
    try:
        engine.write([_node("A", "src:a", '{"title":"the quick brown fox"}')])
        engine.write([_node("B", "src:b", '{"title":"lazy dogs sleeping"}')])
        engine.configure_projections(
            [_spec("title", {ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}, fts=True)]
        )
        engine.write([_node("C", "src:c", '{"title":"a brown bear"}')])
        engine.drain(timeout_s=30)
    finally:
        engine.close()

    # EAV values at rest (filterable), backfill + same-transaction write.
    assert _eav_values(path, "title") == [
        "a brown bear",
        "lazy dogs sleeping",
        "the quick brown fox",
    ]
    # property-FTS: "brown" matches A (1) and C (3).
    assert _pfts_match(path, "title", "brown") == [1, 3]
    assert _pfts_match(path, "title", "fox") == [1]


def test_explicit_drop_and_omission_semantics(tmp_path) -> None:
    path = str(tmp_path / "drop.sqlite")
    engine = _open(path)
    try:
        engine.write([_node("A", "src:a", '{"status":"open","title":"hello"}')])
        engine.configure_projections([_spec("status", {ProjectionRole.FILTERABLE})])
        engine.configure_projections(
            [_spec("title", {ProjectionRole.SEARCHABLE}, fts=True)]
        )
        # Omission of `status` does NOT drop it.
        omit = engine.configure_projections([_spec("title", {ProjectionRole.SEARCHABLE}, fts=True)])
        assert omit.dropped == []
        assert {s.name for s in read.projections(engine)} == {"status", "title"}

        # Explicit drop of `status` removes exactly it.
        d = engine.configure_projections([], drop=["status"])
        assert d.dropped == ["status"]
        assert {s.name for s in read.projections(engine)} == {"title"}
        engine.drain(timeout_s=30)
    finally:
        engine.close()

    assert _eav_values(path, "status") == []


def test_destructive_change_requires_explicit_drop(tmp_path) -> None:
    engine = _open(str(tmp_path / "destructive.sqlite"))
    try:
        engine.write([_node("A", "src:a", '{"status":"open"}')])
        engine.configure_projections(
            [_spec("status", {ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}, fts=True)]
        )
        # Removing `searchable` is destructive → refused without a drop.
        with pytest.raises(ProjectionDestructiveError) as exc:
            engine.configure_projections([_spec("status", {ProjectionRole.FILTERABLE})])
        assert exc.value.name == "status"

        # Naming it in `drop` lets it rebuild.
        ok = engine.configure_projections(
            [_spec("status", {ProjectionRole.FILTERABLE})], drop=["status"]
        )
        assert ok.dropped == ["status"]
        assert read.projections(engine)[0].roles == frozenset({"filterable"})
    finally:
        engine.close()


def test_ffi_nul_in_projection_name_rejected_at_binding(tmp_path) -> None:
    """fix-5 (AC-068a) — a NUL in the projection `name` is rejected at the
    binding (WriteValidationError) and no registry row is persisted."""
    path = str(tmp_path / "ffi_name.sqlite")
    engine = _open(path)
    try:
        with pytest.raises(WriteValidationError) as exc:
            engine.configure_projections([_spec(_NUL, {ProjectionRole.FILTERABLE})])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "no projection may be persisted when a NUL is rejected"


def test_ffi_nul_in_fts_tokenizer_rejected_at_binding(tmp_path) -> None:
    """fix-5 (AC-068a) — a NUL in `fts_tokenizer` is rejected at the binding
    before it can be persisted in `_fathomdb_projection_registry.fts_tokenizer`."""
    path = str(tmp_path / "ffi_tok.sqlite")
    engine = _open(path)
    try:
        spec = ProjectionSpec(
            name="status",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            fts=True,
            fts_tokenizer=_NUL,
        )
        with pytest.raises(WriteValidationError) as exc:
            engine.configure_projections([spec])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "no projection may be persisted when a NUL is rejected"


def test_ffi_nul_in_vector_embedder_rejected_at_binding(tmp_path) -> None:
    """fix-5 (AC-068a) — a NUL in `vector_embedder` is rejected at the binding
    before it can be persisted in `_fathomdb_projection_registry.vector_embedder`."""
    path = str(tmp_path / "ffi_emb.sqlite")
    engine = _open(path)
    try:
        spec = ProjectionSpec(
            name="summary",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            vector=True,
            vector_embedder=_NUL,
        )
        with pytest.raises(WriteValidationError) as exc:
            engine.configure_projections([spec])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "no projection may be persisted when a NUL is rejected"


def test_ffi_nul_in_drop_entry_rejected_at_binding(tmp_path) -> None:
    """fix-5 (AC-068a) — a NUL in a `drop` list entry is rejected at the binding
    (the drop list is a caller FFI-string vector too)."""
    path = str(tmp_path / "ffi_drop.sqlite")
    engine = _open(path)
    try:
        # A live projection exists so the drop path is non-vacuous.
        engine.write([_node("A", "src:a", '{"status":"open"}')])
        engine.configure_projections([_spec("status", {ProjectionRole.FILTERABLE})])
        with pytest.raises(WriteValidationError) as exc:
            engine.configure_projections([], drop=[_NUL])
        assert isinstance(exc.value, EngineError)
        # The real projection is untouched by the refused call.
        assert {s.name for s in read.projections(engine)} == {"status"}
    finally:
        engine.close()


def test_rankable_and_vector_are_deferred_not_built(tmp_path) -> None:
    engine = _open(str(tmp_path / "deferred.sqlite"))
    try:
        engine.write([_node("A", "src:a", '{"importance":"high","summary":"a meaning"}')])
        d1 = engine.configure_projections([_spec("importance", {ProjectionRole.RANKABLE})])
        assert d1.built == [] and d1.deferred == ["importance"]

        d2 = engine.configure_projections(
            [_spec("summary", {ProjectionRole.SEARCHABLE}, vector=True)]
        )
        assert d2.deferred == ["summary"]
        # The vector sub-object round-trips (Slice 20 attaches dense_readiness).
        summary = next(s for s in read.projections(engine) if s.name == "summary")
        assert summary.vector is True
    finally:
        engine.close()
