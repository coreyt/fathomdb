"""X1 SDK parity — 0.8.20 Slice 20 (R-20-DR ``dense_readiness``).

Drives the engine-set readiness field through the PyO3 binding by EXECUTION
(not symbol presence): ``read.projections`` must surface
``vector_dense_readiness`` on a ``searchable→vector`` projection, it must never
read ``"ready"`` while an embed is outstanding, and it must be inert on the way
back in. Mirrors the Rust suite
``src/rust/crates/fathomdb-engine/tests/slice20_dense_readiness.rs`` and the TS
suite ``src/ts/tests/slice20-dense-readiness.test.ts`` (Py ≡ TS, R-X-1).

``sqlite3`` is used only as a READ oracle on a CLOSED database — the "vector at
rest" assertion behind the readiness flip.

ZERO net-new governed commands: this rides the already-governed
``Engine.configure_projections`` / ``read.projections`` verbs.
"""

from __future__ import annotations

import sqlite3
from typing import cast

import pytest

from fathomdb import DenseReadiness, Engine, ProjectionRole, ProjectionSpec, read
from fathomdb.errors import InvalidArgumentError

_SOURCE_ID = "py-test:slice20"


def _open(path: str) -> Engine:
    return Engine.open(path, use_default_embedder=False)


def _node(logical_id: str, body_json: str) -> dict:
    return {"kind": "doc", "body": body_json, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _vector_spec(name: str) -> ProjectionSpec:
    return ProjectionSpec(name=name, roles=frozenset({ProjectionRole.SEARCHABLE}), vector=True)


def _readiness(engine: Engine, name: str) -> str | None:
    for spec in read.projections(engine):
        if spec.name == name:
            return spec.vector_dense_readiness
    return None


def _vector_row_count(path: str) -> int:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute("SELECT COUNT(*) FROM _fathomdb_vector_rows").fetchone()[0]
    finally:
        conn.close()


def _torn_terminals(path: str) -> int:
    """§4.1 invariant 1 as SQL: rows at the ``up_to_date`` projection terminal
    that carry NO vector row. Must always be zero — the vector INSERT and the
    terminal are one transaction, so a torn ``ready``-without-vector is
    unreachable."""

    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute(
            "SELECT COUNT(*) FROM _fathomdb_projection_terminal t"
            " JOIN canonical_nodes n ON n.write_cursor = t.write_cursor"
            " JOIN _fathomdb_vector_kinds k ON k.kind = n.kind"
            " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = t.write_cursor"
            " WHERE t.state = 'up_to_date' AND v.write_cursor IS NULL"
        ).fetchone()[0]
    finally:
        conn.close()


def test_read_projections_surfaces_vector_dense_readiness(tmp_path) -> None:
    engine = _open(str(tmp_path / "surface.sqlite"))
    try:
        engine.configure_projections([_vector_spec("summary")])
        # No embedder is configured, so nothing is outstanding: ready.
        assert _readiness(engine, "summary") == "ready"
    finally:
        engine.close()


def test_readiness_is_scoped_to_the_vector_sub_object(tmp_path) -> None:
    """``filterable`` / ``searchable→FTS`` are same-transaction (non-stale on
    commit), so they carry NO readiness axis at all."""

    engine = _open(str(tmp_path / "scoped.sqlite"))
    try:
        engine.configure_projections(
            [ProjectionSpec(name="status", roles=frozenset({ProjectionRole.FILTERABLE}))]
        )
        got = next(s for s in read.projections(engine) if s.name == "status")
        assert got.vector is False
        assert got.vector_dense_readiness is None
    finally:
        engine.close()


def test_readiness_never_reports_ready_with_pending_embeds(tmp_path) -> None:
    """The R-20-DR acceptance signal, driven through the binding with NO network
    and NO embedder download.

    How it is made deterministic: register ``doc`` as a vector kind through the
    ``test-hooks``-gated native seam, then write a ``doc`` row on an engine
    opened WITHOUT an embedder. The row enqueues vector work that cannot
    complete, and the shipped retry ladder is 1s/4s/16s, so there is a
    ~21-second window in which the embed is genuinely outstanding. Readiness must
    read ``embedding`` throughout it — never ``ready``.
    """

    path = str(tmp_path / "pending.sqlite")
    engine = _open(path)
    try:
        # `getattr`, not attribute access: the seam is `test-hooks`-gated, so it
        # is absent from the type stub (and from release wheels).
        configure_vector_kind = getattr(engine._native, "_configure_vector_kind_for_test", None)
        if configure_vector_kind is None:
            pytest.skip("binding built without the `test-hooks` vector-kind seam")
        # Declare the projection FIRST: `configure_projections` drains, and
        # draining with work outstanding would (correctly) time out.
        engine.configure_projections([_vector_spec("summary")])
        assert _readiness(engine, "summary") == "ready", "an empty corpus is ready"

        configure_vector_kind("doc")
        engine.write([_node("N1", '{"summary":"a dense meaning"}')])

        assert (
            _readiness(engine, "summary") == "embedding"
        ), "readiness must NOT report ready while an embed is outstanding"
    finally:
        engine.close()

    # At rest: the tolerated torn state (`embedding` with the vector absent) —
    # and NOT the forbidden one.
    assert _vector_row_count(path) == 0, "no vector landed — the embed never ran"
    assert _torn_terminals(path) == 0, "§4.1: no up_to_date terminal without its vector"


def test_caller_supplied_dense_readiness_is_inert(tmp_path) -> None:
    """It is engine-set READ METADATA: accepted (it is not part of the
    declaration) but never stored and never honoured, so the engine always
    reports the derived truth — and it can never masquerade as a change."""

    engine = _open(str(tmp_path / "inert.sqlite"))
    try:
        lying = ProjectionSpec(
            name="summary",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            vector=True,
            vector_dense_readiness="embedding",
        )
        engine.configure_projections([lying])
        assert (
            _readiness(engine, "summary") == "ready"
        ), "the caller's `embedding` must NOT be honoured"
        again = engine.configure_projections([_vector_spec("summary")])
        assert again.unchanged is True, "readiness is not part of the declaration"
    finally:
        engine.close()


def test_read_projections_output_round_trips_back_into_configure_with_readiness(tmp_path) -> None:
    """The fix-4 read→configure round-trip, extended: ``read.projections`` now
    emits ``vector_dense_readiness`` for a vector projection, and feeding that
    output straight back MUST still re-apply as an idempotent no-op. Otherwise
    ``read.projections`` would produce a value its own ``configure_projections``
    cannot consume — the exact defect fix-4 closed."""

    engine = _open(str(tmp_path / "round_trip.sqlite"))
    try:
        engine.configure_projections(
            [
                ProjectionSpec(
                    name="status",
                    roles=frozenset({ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}),
                    fts=True,
                    vector=True,
                )
            ]
        )
        read_back = read.projections(engine)
        assert len(read_back) == 1
        assert read_back[0].vector_dense_readiness == "ready", "read output carries readiness"
        again = engine.configure_projections(list(read_back))
        assert again.unchanged is True, "read.projections output must re-apply as a no-op"
    finally:
        engine.close()


def test_readiness_with_vector_false_is_refused(tmp_path) -> None:
    """Mirrors the shipped ``vector_embedder``-without-``vector`` gate: a shape
    ``read.projections`` could never echo back is refused at the binding."""

    engine = _open(str(tmp_path / "refuse_shape.sqlite"))
    try:
        spec = ProjectionSpec(
            name="status",
            roles=frozenset({ProjectionRole.FILTERABLE}),
            vector=False,
            vector_dense_readiness="ready",
        )
        with pytest.raises(InvalidArgumentError, match="vector_dense_readiness"):
            engine.configure_projections([spec])
    finally:
        engine.close()


@pytest.mark.parametrize("bad", ["pending", "", "Ready", "embedded"])
def test_unknown_readiness_spelling_is_refused(tmp_path, bad: str) -> None:
    """``read.projections`` only ever emits a declared readiness literal, so any
    other spelling could not round-trip. ``"pending"`` in particular is RESERVED
    for the orthogonal admission axis (quarantine/trust, an app judgment) and is
    never a readiness value."""

    engine = _open(str(tmp_path / f"refuse_{bad or 'empty'}.sqlite"))
    try:
        spec = ProjectionSpec(
            name="summary",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            vector=True,
            # Deliberately cross the typed boundary to exercise runtime rejection.
            vector_dense_readiness=cast(DenseReadiness, bad),
        )
        with pytest.raises(InvalidArgumentError):
            engine.configure_projections([spec])
    finally:
        engine.close()
