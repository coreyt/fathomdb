"""X1 SDK parity — 0.8.20 Slice 20c (R-20-DR remainder): ``drain`` is the
flush-to-readiness barrier (``api-surface.md`` **C4**).

Drives the barrier through the PyO3 binding by EXECUTION, not symbol presence.
Mirrors ``src/rust/crates/fathomdb-engine/tests/slice20c_flush_barrier.rs`` and
the TS suite ``src/ts/tests/slice20c-flush-barrier.test.ts`` (Py ≡ TS, R-X-1).

The pinned invariant:

    ``engine.drain()`` returning ⟹ ``vector_dense_readiness == "ready"``
    AND every vector-eligible row has its vector row AT REST.

**Why the raw-table assertion is load-bearing.** A harness that only reads
readiness back PASSES against the defect: before this slice,
``configure_projections`` never enrolled the kind, so ``drain()`` returned
immediately and readiness read ``"ready"`` with ZERO vectors and nothing that
would ever create them. ``sqlite3`` is used purely as a READ oracle
(``mode=ro``, never ``immutable=1``, so committed WAL frames are visible).

These tests need a LIVE embedder (``use_default_embedder=True``) because the
dense arm is what is being flushed; they honour the standing
``FATHOMDB_SKIP_NETWORK_TESTS`` guard, exactly as ``test_use_default_embedder``
does.

ZERO net-new governed commands: this rides the already-governed
``Engine.configure_projections`` / ``read.projections`` verbs plus the shipped
``Engine.drain`` INSTRUMENTATION method (TC-55, steward seq-110).
"""

from __future__ import annotations

import os
import sqlite3

import pytest

from fathomdb import Engine, ProjectionRole, ProjectionSpec, read

_SOURCE_ID = "py-test:slice20c"
_DRAIN_TIMEOUT_S = 120.0


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test")


def _node(logical_id: str, body_json: str) -> dict:
    return {"kind": "doc", "body": body_json, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _vector_spec(name: str = "summary") -> ProjectionSpec:
    return ProjectionSpec(name=name, roles=frozenset({ProjectionRole.SEARCHABLE}), vector=True)


def _readiness(engine: Engine, name: str = "summary") -> str | None:
    for spec in read.projections(engine):
        if spec.name == name:
            return spec.vector_dense_readiness
    return None


def _query(path: str, sql: str) -> int:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute(sql).fetchone()[0]
    finally:
        conn.close()


def _vector_rows(path: str) -> int:
    return _query(path, "SELECT COUNT(*) FROM _fathomdb_vector_rows")


# NOTE: the `vector_default` (vec0) partition is NOT probed here. It is a
# VIRTUAL table provided by the engine-linked `vec0` extension, which stdlib
# `sqlite3` cannot load — `SELECT ... FROM vector_default` raises
# `no such module: vec0`. The shipped Slice-20 harness has the same boundary.
# The Rust suite `slice20c_flush_barrier.rs` carries that second at-rest oracle;
# these bindings assert on `_fathomdb_vector_rows`, which is an ordinary table
# written in the SAME transaction as the vec0 INSERT
# (`commit_projection_outcomes`), plus the un-joined
# `_leaf_rows_without_vectors` probe below.


def _leaf_rows_without_vectors(path: str) -> int:
    """Vector-eligible node rows carrying NO vector row.

    Deliberately does NOT join ``_fathomdb_vector_kinds``: the defect IS that the
    declaration never enrolled the kind, so a joined probe returns a hollow zero
    on the broken code. ``row_kind IN ('leaf','coverage')`` is the engine's own
    vector-eligibility predicate.
    """

    return _query(
        path,
        "SELECT COUNT(*) FROM canonical_nodes n"
        " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor"
        " WHERE n.row_kind IN ('leaf', 'coverage') AND v.write_cursor IS NULL",
    )


def _vector_kind_registered(path: str, kind: str = "doc") -> bool:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        row = conn.execute(
            "SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?", (kind,)
        ).fetchone()
        return row[0] > 0
    finally:
        conn.close()


def test_declaring_a_vector_projection_backfills_and_drain_flushes_to_ready(tmp_path) -> None:
    """The C4 rider, end to end through the binding: write rows FIRST, declare
    ``searchable→vector`` SECOND — the ordinary "turn the dense arm on over an
    existing corpus" flow — then ``drain()`` must FLUSH the backfill, not report a
    hollow ``"ready"``."""

    _skip_if_no_network()
    path = str(tmp_path / "flush_backfill.sqlite")
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write([_node(f"N{i}", f'{{"summary":"dense meaning {i}"}}') for i in range(4)])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)

        # Fixture precondition, asserted rather than assumed: this is the state
        # `configure_projections` inherits — nothing enrolled, nothing embedded.
        assert not _vector_kind_registered(path), "fixture: `doc` is not yet a vector kind"
        assert _vector_rows(path) == 0, "fixture: no vectors exist yet"

        delta = engine.configure_projections([_vector_spec()])
        assert "summary" in delta.deferred, "the vector sub-target is reported as deferred work"

        # `drain` is the flush-to-readiness barrier.
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)

        assert _readiness(engine) == "ready", "after `drain()` returns, the dense arm is caught up"
    finally:
        engine.close()

    # …and readiness `ready` must be BACKED BY VECTORS AT REST. This is the
    # assertion the defect fails: 0 rows, forever.
    assert _vector_kind_registered(path), "the declaration enrolled the vector kind"
    assert _vector_rows(path) == 4, "every pre-existing row was backfilled"
    assert _leaf_rows_without_vectors(path) == 0


def test_write_after_declare_also_reaches_ready_with_vectors_at_rest(tmp_path) -> None:
    """Ordering must not matter. Declaring FIRST and writing SECOND has to reach
    the same place — a kind first written after the declaration is enrolled on
    the write path."""

    _skip_if_no_network()
    path = str(tmp_path / "flush_write_after.sqlite")
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.configure_projections([_vector_spec()])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready", "an empty corpus has nothing outstanding"

        engine.write([_node("N1", '{"summary":"written after declaring"}')])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert _vector_rows(path) == 1, "the post-declaration write embedded"
    assert _leaf_rows_without_vectors(path) == 0


def test_reapplying_a_satisfied_declaration_is_a_no_op(tmp_path) -> None:
    """R-20-PR: re-registration is a no-op. A satisfied declaration must not
    re-open the backfill or re-embed."""

    _skip_if_no_network()
    path = str(tmp_path / "flush_idempotent.sqlite")
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write([_node("N1", '{"summary":"a dense meaning"}')])
        engine.configure_projections([_vector_spec()])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
        before = _vector_rows(path)
        assert before == 1

        again = engine.configure_projections([_vector_spec()])
        assert again.unchanged is True, "an identical re-apply diffs to a no-op"
        # Read readiness BEFORE any drain: a spurious re-enqueue would show here.
        assert _readiness(engine) == "ready", "an idempotent re-apply must not re-open the backfill"

        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _vector_rows(path) == before, "no row was re-embedded"
    finally:
        engine.close()


def test_declaration_without_a_live_embedder_defers_and_does_not_enrol(tmp_path) -> None:
    """With ``use_default_embedder=False`` there is NO dense arm, so the
    declaration persists and DEFERS (Q6a graceful-absent, exactly like
    ``rankable``) rather than queueing embeds that could only fail. It must not
    enrol the kind, must not open an ``"embedding"`` window, and ``drain`` must
    return promptly rather than burn its timeout.

    This is the arm that runs with no network at all, so the parity harness still
    exercises the slice under ``FATHOMDB_SKIP_NETWORK_TESTS``.
    """

    path = str(tmp_path / "flush_no_embedder.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        engine.write([_node("N1", '{"summary":"a dense meaning"}')])
        engine.configure_projections([_vector_spec()])
        assert _readiness(engine) == "ready", "no live embedder ⇒ no dense arm ⇒ nothing outstanding"
        engine.drain(timeout_s=5.0)
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert not _vector_kind_registered(path), "a dead dense arm must not enrol the kind"
    assert (
        _query(
            path,
            "SELECT COUNT(*) FROM operational_mutations"
            " WHERE collection_name = 'projection_failures'",
        )
        == 0
    ), "no doomed embeds may be queued, so no projection_failures audit rows"
