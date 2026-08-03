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

import json
import os
import sqlite3
import subprocess
import sys
import time

import pytest

from fathomdb import Engine, ProjectionRole, ProjectionSpec, read
from fathomdb.errors import SchedulerError

_SOURCE_ID = "py-test:slice20c"
_DRAIN_TIMEOUT_S = 120.0
# fix-2: the arms whose FAILURE MODE is a wedged projection worker use a shorter
# barrier — an assertion that "`drain` did not hang" should not cost two minutes
# per arm when it is red.
_WEDGE_TIMEOUT_S = 30.0
# fix-4: the engine's projection retry ladder is 0 + 1 + 4 + 16 s. The
# no-embedder arm waits it out so its terminal/audit probes are FALSIFYING at
# baseline rather than merely early.
_LADDER_SETTLE_S = 24.0
_READ_ONLY_SQL_CHILD = """
import json
import sqlite3
import sys

path, sql, parameters = sys.argv[1:]
conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
try:
    row = conn.execute(sql, json.loads(parameters)).fetchone()
    print(json.dumps(None if row is None else row[0]))
finally:
    conn.close()
"""


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test")


def _read_only_scalar(
    path: str, sql: str, parameters: tuple[str | int, ...] = ()
) -> int | str | None:
    """Run one raw SQLite scalar query outside the engine's process."""

    result = subprocess.run(
        [sys.executable, "-c", _READ_ONLY_SQL_CHILD, path, sql, json.dumps(parameters)],
        capture_output=True,
        check=True,
        text=True,
    )
    scalar = json.loads(result.stdout)
    if scalar is None or type(scalar) in (int, str):
        return scalar
    raise TypeError(f"unexpected raw SQLite scalar type: {type(scalar).__name__}")


def _read_only_int(path: str, sql: str, parameters: tuple[str | int, ...] = ()) -> int:
    scalar = _read_only_scalar(path, sql, parameters)
    assert type(scalar) is int, f"expected integer raw SQLite result, got {scalar!r}"
    return scalar


def test_read_only_sql_oracle_preserves_scalar_types_and_parameters(tmp_path) -> None:
    """The isolated raw-SQL oracle retains its query and result semantics."""

    path = str(tmp_path / "oracle.sqlite")
    conn = sqlite3.connect(path)
    try:
        conn.execute("CREATE TABLE oracle (state TEXT, is_ready INTEGER)")
        conn.execute("INSERT INTO oracle VALUES ('ready', 1)")
        conn.commit()
    finally:
        conn.close()

    assert _read_only_scalar(path, "SELECT state FROM oracle") == "ready"
    assert _read_only_scalar(path, "SELECT is_ready FROM oracle") == 1
    assert _read_only_scalar(path, "SELECT state FROM oracle WHERE state = ?", ("missing",)) is None


def _node(logical_id: str, body_json: str) -> dict:
    return _node_of_kind("doc", logical_id, body_json)


def _node_of_kind(kind: str, logical_id: str, body_json: str) -> dict:
    return {"kind": kind, "body": body_json, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _vector_spec(name: str = "summary") -> ProjectionSpec:
    return ProjectionSpec(name=name, roles=frozenset({ProjectionRole.SEARCHABLE}), vector=True)


def _readiness(engine: Engine, name: str = "summary") -> str | None:
    for spec in read.projections(engine):
        if spec.name == name:
            return spec.vector_dense_readiness
    return None


def _query(path: str, sql: str) -> int:
    return _read_only_int(path, sql)


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


def _leaf_rows_of_kind_without_vectors(path: str, kind: str) -> int:
    """The same un-joined at-rest probe, narrowed to one ``canonical_nodes.kind``.

    fix-2 needs the narrowed form because its fixture deliberately holds a kind
    that gets NO dense arm, so the corpus-wide count is legitimately non-zero. It
    still does not join ``_fathomdb_vector_kinds``.
    """

    return _read_only_int(
        path,
        "SELECT COUNT(*) FROM canonical_nodes n"
        " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor"
        " WHERE n.row_kind IN ('leaf', 'coverage') AND n.kind = ?"
        " AND v.write_cursor IS NULL",
        (kind,),
    )


def _vector_kind_registered(path: str, kind: str = "doc") -> bool:
    return (
        _read_only_int(path, "SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?", (kind,))
        > 0
    )


def _active_cursor(path: str, logical_id: str) -> int:
    cursor = _read_only_scalar(
        path,
        "SELECT write_cursor FROM canonical_nodes WHERE logical_id = ? AND superseded_at IS NULL",
        (logical_id,),
    )
    assert cursor is not None, f"no active row for {logical_id}"
    assert type(cursor) is int, f"expected integer active cursor, got {cursor!r}"
    return cursor


def _terminal_state(path: str, cursor: int) -> str | None:
    """The raw ``_fathomdb_projection_terminal`` state for one cursor.

    ``None`` is PENDING, and that is the fix-4 property: an ABSENT embedder is an
    ENVIRONMENT fact, not an embed failure, so it must record NO terminal. A
    ``"failed"`` terminal is permanent by design (nothing reopens one, and
    nothing should — that would loop a genuinely-failing row forever), so
    recording one here LOSES the write.
    """

    state = _read_only_scalar(
        path,
        "SELECT state FROM _fathomdb_projection_terminal WHERE write_cursor = ?",
        (cursor,),
    )
    assert state is None or type(state) is str, f"expected terminal state, got {state!r}"
    return state


def _fts_row_exists(path: str, cursor: int) -> bool:
    return (
        _read_only_int(path, "SELECT COUNT(*) FROM search_index WHERE write_cursor = ?", (cursor,))
        > 0
    )


def _projection_failure_rows(path: str) -> int:
    return _query(
        path,
        "SELECT COUNT(*) FROM operational_mutations WHERE collection_name = 'projection_failures'",
    )


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
        assert _readiness(engine) == "ready", (
            "no live embedder ⇒ no dense arm ⇒ nothing outstanding"
        )
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


def test_dropping_the_last_vector_projection_stops_embedding(tmp_path) -> None:
    """fix-1 (codex §9 [P2]) — the SYMMETRIC INVERSE, through the binding.

    Slice 20c gave ``_fathomdb_vector_kinds`` its first governed-call-reachable
    enrolment path for a node kind. Without an inverse, ``drop``ping the last
    ``searchable→vector`` declaration leaves the kind enrolled, so subsequent
    writes keep embedding for a projection ``read.projections`` no longer
    reports.

    The un-enrolment is NON-DESTRUCTIVE and this test pins both halves: the
    kind stops being enrolled, and the vectors already at rest are untouched
    (the shipped ``drop`` arm has never deleted an embedding). Re-declaring
    re-enrols and backfills, so nothing is stranded.
    """

    _skip_if_no_network()
    path = str(tmp_path / "flush_drop_inverse.sqlite")
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write([_node("N1", '{"summary":"a dense meaning"}')])
        engine.configure_projections([_vector_spec()])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
        assert _vector_kind_registered(path), "fixture: the declaration enrolled `doc`"
        assert _vector_rows(path) == 1, "fixture: N1 is embedded"

        # ---- drop the LAST `searchable→vector` declaration ----
        delta = engine.configure_projections([], drop=["summary"])
        assert "summary" in delta.dropped, "the drop is reported"
        assert _readiness(engine) is None, "the projection is gone from the registry"

        assert not _vector_kind_registered(path), (
            "ONE-WAY ENROLMENT: dropping the last `searchable→vector` declaration must un-enrol "
            "the node kind it enrolled"
        )
        assert _vector_rows(path) == 1, (
            "un-enrolment must NOT delete embeddings — the shipped `drop` arm leaves vectors at rest"
        )

        # ---- a write of the SAME kind after the drop embeds nothing ----
        engine.write([_node("N2", '{"summary":"written after the drop"}')])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _vector_rows(path) == 1, "a write after the drop must not be embedded"
        assert _leaf_rows_without_vectors(path) == 1, "N2 is the one un-embedded row"

        # ---- re-declaring re-enrols and backfills: the inverse is reversible ----
        engine.configure_projections([_vector_spec()])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
        assert _vector_kind_registered(path), "re-declaring re-enrols the kind"
        assert _vector_rows(path) == 2, "the row written while the arm was off is backfilled"
        assert _leaf_rows_without_vectors(path) == 0
    finally:
        engine.close()


def test_a_kind_the_vector_writer_cannot_commit_gets_no_dense_arm(tmp_path) -> None:
    """fix-2 (codex §9 [P1]) — enrolment is restricted to COMMIT-ABLE kinds.

    The engine maps a node ``kind`` onto a locked ``source_type`` partition-key
    vocabulary before it can commit a vector, and ``write`` accepts any non-empty
    ``kind``. Enrolling a kind outside that vocabulary made the scheduler pick the
    row up while the commit could never record a terminal: the row stayed pending
    forever, ``drain`` burned its whole timeout, and ``vector_dense_readiness``
    stuck on ``"embedding"`` — starving the rows whose kinds ARE commit-able.

    Both enrolment doors are exercised: ``invoice`` present in the corpus at
    DECLARATION time, and a second ``invoice`` written AFTER the declaration
    (the write-path late-enrolment door).

    The un-enrolled kind is not an error: nothing rejects it, no exception is
    raised, and there is no verb to ask about it. It just gets no vector.
    """

    _skip_if_no_network()
    path = str(tmp_path / "flush_uncommittable.sqlite")
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write(
            [
                _node("N1", '{"summary":"a dense meaning"}'),
                _node_of_kind("invoice", "I1", '{"summary":"payable in 30 days"}'),
            ]
        )
        engine.drain(timeout_s=_WEDGE_TIMEOUT_S)

        # ---- declare-time enrolment ----
        engine.configure_projections([_vector_spec()])
        engine.drain(timeout_s=_WEDGE_TIMEOUT_S)  # WEDGES at fix-2 baseline
        assert _readiness(engine) == "ready", (
            "a kind the engine cannot commit a vector for must not hold the whole corpus in "
            '"embedding" forever'
        )

        # ---- late (write-path) enrolment ----
        engine.write([_node_of_kind("invoice", "I2", '{"summary":"also payable"}')])
        engine.drain(timeout_s=_WEDGE_TIMEOUT_S)  # WEDGES at fix-2 baseline
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert _vector_kind_registered(path, "doc"), "the commit-able kind still gets its dense arm"
    assert _leaf_rows_of_kind_without_vectors(path, "doc") == 0, "…and its row is embedded"
    assert not _vector_kind_registered(path, "invoice"), (
        "ENROLMENT MUST BE RESTRICTED TO COMMIT-ABLE KINDS"
    )
    assert _vector_rows(path) == 1, "exactly the one commit-able row was embedded"
    assert _leaf_rows_of_kind_without_vectors(path, "invoice") == 2, "no dense arm for that kind"
    assert (
        _query(
            path,
            "SELECT COUNT(*) FROM operational_mutations"
            " WHERE collection_name = 'projection_failures'",
        )
        == 0
    ), "a kind with no dense arm is not a FAILURE — it must not pollute the failure audit"


def test_late_enrolment_backfills_rows_a_no_embedder_session_stranded(tmp_path) -> None:
    """fix-2 (codex §9 [P2]) — a LATE enrolment owes the same backfill.

    A database persists a ``searchable→vector`` declaration while opened WITHOUT
    an embedder (it defers, enrolling nothing), then reopens WITH one and writes
    the same kind BEFORE re-applying the projection. The write enrols the kind —
    and used to enqueue only its OWN row, leaving every row from the no-embedder
    session holding a permanent terminal with no vector. After the flush,
    readiness reported ``"ready"`` with pre-existing vector-eligible rows
    unembedded: a FALSE READY, the exact defect class R-20-DR exists to
    eliminate.
    """

    _skip_if_no_network()
    path = str(tmp_path / "flush_late_enrol_backfill.sqlite")

    # ---- session 1: no embedder. The declaration persists and DEFERS. ----
    engine = Engine.open(path, use_default_embedder=False)
    try:
        engine.configure_projections([_vector_spec()])
        engine.write(
            [
                _node("N1", '{"summary":"stranded one"}'),
                _node("N2", '{"summary":"stranded two"}'),
            ]
        )
        engine.drain(timeout_s=5.0)
        assert not _vector_kind_registered(path), "fixture: a dead dense arm enrols nothing"
        assert _leaf_rows_without_vectors(path) == 2, "fixture: two rows are stranded"
    finally:
        engine.close()

    # ---- session 2: SAME database, now WITH an embedder. The projection is NOT
    # re-applied — the WRITE is what turns the dense arm on. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write([_node("N3", '{"summary":"written before re-applying"}')])
        assert _vector_kind_registered(path), "the write LATE-ENROLLED the kind"
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert _leaf_rows_without_vectors(path) == 0, (
        'FALSE-READY: `drain()` returned and readiness reads "ready", but rows written in the '
        "no-embedder session still have no vector at rest"
    )
    assert _vector_rows(path) == 3, "all three rows were embedded"


def test_a_no_embedder_session_leaves_an_enrolled_kinds_write_recoverable(tmp_path) -> None:
    """fix-4 (codex §9 round 3 [P1]) — a write made with NO live embedder, over an
    ALREADY-ENROLLED kind, must stay RECOVERABLE.

    fix-2 gated ENROLMENT on a live embedder, but ``_fathomdb_vector_kinds`` is
    durable: once an embedder-backed session has enrolled ``doc``, every later
    session sees the enrolment. Reopening with no embedder and writing that kind
    therefore still enqueues. At baseline the worker exhausted its retry ladder
    against the absent embedder and recorded an ``EmbedderNotConfiguredError``
    ``'failed'`` terminal plus a ``projection_failures`` audit row — and since no
    path reopens a ``'failed'`` terminal, reopening WITH an embedder left that
    write PERMANENTLY unembedded while readiness reported ``"ready"``.

    Three sessions, no re-apply and no second write: the ORDINARY scheduler is
    the whole recovery path.

    The CONSUMER-VISIBLE consequence is asserted here on purpose, so it cannot
    change back silently: while the row is outstanding and this session cannot
    satisfy it, readiness reads ``"embedding"`` and ``drain`` raises
    ``SchedulerError``. That is what design-of-record §4.1 invariant 1 demands
    (the ONLY tolerable torn state is ``embedding`` with the vector absent) and
    it is LOUD rather than silent.
    """

    _skip_if_no_network()
    path = str(tmp_path / "flush_no_embedder_recoverable.sqlite")

    # ---- session 1: WITH an embedder. This durably ENROLS `doc`. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.configure_projections([_vector_spec()])
        engine.write([_node("N1", '{"summary":"embedded in session one"}')])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _vector_kind_registered(path), "fixture: session 1 enrolled `doc`"
        assert _leaf_rows_without_vectors(path) == 0, "fixture: session 1's row is embedded"
    finally:
        engine.close()

    c1 = _active_cursor(path, "N1")

    # ---- session 2: the SAME database, reopened with NO embedder. ----
    engine = Engine.open(path, use_default_embedder=False)
    try:
        # Fixture precondition, asserted rather than assumed: the enrolment
        # PERSISTED, so this session's write reaches the vector pipeline.
        assert _vector_kind_registered(path), (
            "fixture: the kind stays enrolled across the reopen — that is the whole finding"
        )
        engine.write([_node("N2", '{"summary":"written with no dense arm"}')])

        # THE CONSUMER-VISIBLE CONSEQUENCE. An enrolled row with no vector is
        # outstanding and this session cannot satisfy it, so the barrier must NOT
        # clear. At baseline it cleared by recording a `'failed'` terminal —
        # which is exactly how the write got lost.
        with pytest.raises(SchedulerError):
            engine.drain(timeout_s=3.0)

        # Give the BASELINE its full retry ladder (0 + 1 + 4 + 16 s) before the
        # probes below. Without this wait they all read a merely-unterminated row
        # and pass VACUOUSLY on the broken code — the SDKs expose no equivalent
        # of the Rust suite's `set_projection_retry_delays_for_test` seam, so
        # waiting is the only way to make them falsifying. Under the fix the row
        # is never dispatched, so this is dead time and nothing changes across it.
        time.sleep(_LADDER_SETTLE_S)

        assert _readiness(engine) == "embedding", (
            "§4.1 invariant 1: readiness must never read `ready` for an ENROLLED row that has "
            "no vector"
        )

        c2 = _active_cursor(path, "N2")
        assert _fts_row_exists(path, c2), "the write is accepted and still lexically searchable"
        assert _projection_failure_rows(path) == 0, (
            "an ABSENT embedder is an ENVIRONMENT fact, not an embed failure — it must not "
            "pollute the `projection_failures` audit"
        )
        assert _terminal_state(path, c2) is None, (
            "PERMANENTLY LOST WRITE: an absent embedder must record NO terminal. Leaving the row "
            "PENDING is what lets the next live-embedder session's ORDINARY scheduler pick it up"
        )
        assert _terminal_state(path, c1) == "up_to_date", "session 1's row is untouched"
        assert _vector_rows(path) == 1, "fixture: no dense arm ⇒ no new vector yet"
    finally:
        engine.close()

    # ---- session 3: WITH an embedder again. NO re-apply, NO further write. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert _leaf_rows_without_vectors(path) == 0, (
        'PERMANENTLY LOST WRITE: `drain()` returned and readiness reads "ready", but the row '
        "written in the no-embedder session still has no vector at rest"
    )
    assert _vector_rows(path) == 2, "the recovered row was embedded, and only it"
    assert _projection_failure_rows(path) == 0, "the recovery leaves no failure audit behind"


# ---------------------------------------------------------------------------
# fix-5 (codex §9 round 4) — the scheduler's SCAN WINDOW, and the ATOMICITY of a
# late enrolment
# ---------------------------------------------------------------------------

# `PROJECTION_SCAN_FETCH`, restated: the engine's dispatcher fetches at most
# `PROJECTION_WORKERS (2) * PROJECTION_COMMIT_BATCH (16)` jobs per scan, ordered
# by `write_cursor`. It is the width of the window a post-fetch filter can
# starve. The fixture asserts the window is genuinely exceeded rather than
# trusting this number.
_PROJECTION_SCAN_FETCH = 32


def _edge(logical_id: str, from_id: str, to_id: str, body: str) -> dict:
    """An edge carrying a BODY — the only edge shape that enrols ``'edge_fact'``
    in ``_fathomdb_vector_kinds`` (engine ``project_canonical_edge_row``, G11)
    and therefore the only one that is schedulable projection work."""

    return {
        "edge": {
            "kind": "link",
            "from": from_id,
            "to": to_id,
            "source_id": _SOURCE_ID,
            "logical_id": logical_id,
            "body": body,
        }
    }


def _active_edge_cursor(path: str, logical_id: str) -> int:
    cursor = _read_only_scalar(
        path,
        "SELECT write_cursor FROM canonical_edges WHERE logical_id = ? AND superseded_at IS NULL",
        (logical_id,),
    )
    assert cursor is not None, f"no active edge for {logical_id}"
    assert type(cursor) is int, f"expected integer active edge cursor, got {cursor!r}"
    return cursor


def _pending_node_rows_below(path: str, cursor: int) -> int:
    return _read_only_int(
        path,
        "SELECT COUNT(*) FROM canonical_nodes n"
        " JOIN _fathomdb_vector_kinds k ON k.kind = n.kind"
        " LEFT JOIN _fathomdb_projection_terminal t"
        "   ON t.write_cursor = n.write_cursor"
        " WHERE n.row_kind IN ('leaf', 'coverage')"
        "   AND n.superseded_at IS NULL"
        "   AND t.write_cursor IS NULL"
        "   AND n.write_cursor < ?",
        (cursor,),
    )


def _poll_until(timeout_s: float, probe) -> bool:
    deadline = time.monotonic() + timeout_s
    while True:
        if probe():
            return True
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.05)


def test_a_pending_edge_body_survives_a_full_scan_window_of_node_rows(tmp_path) -> None:
    """fix-5 (codex §9 round 4 [P1]) — a pending EDGE body must stay reachable
    behind a full scan window of no-embedder NODE rows.

    fix-4 excluded no-embedder node jobs by filtering what the scheduler's scan
    RETURNED — i.e. after its ``ORDER BY write_cursor LIMIT
    PROJECTION_SCAN_FETCH``. The ``LIMIT`` therefore applied to the UNFILTERED
    set: with more than one window of pending node rows ordered before a pending
    edge body, the scan came back entirely full of node jobs, the filter dropped
    all of them, the dispatcher went back to sleep with its wake already
    consumed — and the edge body was NEVER scheduled. Permanently, because those
    node rows stay pending for the life of a no-embedder session, so ``drain``
    could time out indefinitely on that edge workload.

    Fully OFFLINE and network-free: the kind is enrolled through the
    ``test-hooks``-gated ``_configure_vector_kind_for_test`` seam (the same one
    the shipped ``test_slice20_dense_readiness.py`` fixture uses), so no embedder
    is ever downloaded. Edges keep their SHIPPED no-embedder behaviour, so
    "scheduled" is observable as the edge body reaching a TERMINAL; this test
    asserts only that the scheduler REACHES it.
    """

    path = str(tmp_path / "flush_scan_window_starvation.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        engine.configure_projections([_vector_spec()])
        configure_vector_kind = getattr(engine._native, "_configure_vector_kind_for_test", None)
        assert configure_vector_kind is not None, "test-hooks seam required for the offline fixture"
        configure_vector_kind("doc")

        # MORE than one scan window of node rows, all ordered BEFORE the edge.
        node_rows = _PROJECTION_SCAN_FETCH + 8
        engine.write([_node(f"N{i}", '{"summary":"row %d"}' % i) for i in range(node_rows)])
        engine.write([_edge("E1", "N0", "N1", "the edge body that must not be starved")])

        edge_cursor = _active_edge_cursor(path, "E1")
        assert _vector_kind_registered(path, "edge_fact"), (
            "fixture: an edge body auto-registers `edge_fact` (G11), so it IS schedulable work"
        )
        pending_before = _pending_node_rows_below(path, edge_cursor)
        assert pending_before > _PROJECTION_SCAN_FETCH, (
            "fixture: the scan window must be over-subscribed by node rows ordered BEFORE the "
            f"edge body. Pending: {pending_before}, window: {_PROJECTION_SCAN_FETCH}"
        )

        # The shipped edge path with no embedder is the 0 + 1 + 4 + 16 s retry
        # ladder into a `'failed'` terminal; the SDKs expose no equivalent of the
        # Rust `set_projection_retry_delays_for_test` seam, so this waits it out.
        scheduled = _poll_until(
            _LADDER_SETTLE_S + 16.0, lambda: _terminal_state(path, edge_cursor) is not None
        )
        assert scheduled, (
            "SCAN-WINDOW STARVATION: with more than PROJECTION_SCAN_FETCH "
            f"({_PROJECTION_SCAN_FETCH}) pending node rows ordered before it, the pending edge "
            f"body at cursor {edge_cursor} was NEVER scheduled. The no-embedder node exclusion "
            "must happen INSIDE the scheduler's SQL so the LIMIT applies to the ALREADY-FILTERED "
            "set; filtering after the fetch lets one full window of node rows hide every later "
            "job, and `drain` then times out on that edge workload indefinitely"
        )
        assert _terminal_state(path, edge_cursor) == "failed", (
            "edges keep their shipped no-embedder behaviour: the retry ladder exhausts into a "
            "`'failed'` terminal. fix-5 changes WHICH rows the scan returns, not what happens to "
            "an edge once it is dispatched"
        )
        assert _pending_node_rows_below(path, edge_cursor) == pending_before, (
            "fix-4 stands: an absent embedder records NO terminal for a NODE row, so every one "
            "of them is still pending and still recoverable by the next live-embedder session"
        )
    finally:
        engine.close()


def test_a_late_enrolment_whose_repair_fails_registers_nothing(tmp_path) -> None:
    """fix-5 (codex §9 round 4 [P2]) — the late-enrolment registry INSERT and the
    un-stranding it owes must commit as ONE transaction.

    A late enrolment registers the kind in ``_fathomdb_vector_kinds`` AND repairs
    the rows that kind stranded (delete their ``'up_to_date'`` terminals, rewind
    the watermark). While the INSERT autocommitted ahead of the repair's own
    transaction, a failure in between left the kind REGISTERED with older rows
    still holding their terminals and no vectors. That state is SELF-SEALING: the
    kind is now registered, so every later write skips the enrolment path and
    therefore skips the repair, while readiness reads ``"ready"`` for rows nothing
    will ever embed.

    The failure is injected as a real SQLite ``BEFORE DELETE`` trigger on
    ``_fathomdb_projection_terminal`` — the repair genuinely fails, against a real
    engine and a real database, at exactly the point a crash would truncate it.
    The trigger is installed and dropped while NO engine holds the file. Nothing
    is mocked.
    """

    _skip_if_no_network()
    path = str(tmp_path / "flush_enrolment_atomicity.sqlite")

    # ---- session A: NO embedder. The declaration persists and DEFERS, so these
    # rows take permanent terminals with no vector: the stranded set. ----
    engine = Engine.open(path, use_default_embedder=False)
    try:
        engine.configure_projections([_vector_spec()])
        engine.write([_node("N1", '{"summary":"stranded one"}')])
        engine.write([_node("N2", '{"summary":"stranded two"}')])
        engine.drain(timeout_s=5.0)
        assert not _vector_kind_registered(path), "fixture: a dead dense arm enrols nothing"
        assert _leaf_rows_without_vectors(path) == 2, "fixture: two rows are stranded"
    finally:
        engine.close()

    conn = sqlite3.connect(path)
    try:
        conn.executescript(
            "CREATE TRIGGER fix5_repair_fails"
            " BEFORE DELETE ON _fathomdb_projection_terminal"
            " BEGIN SELECT RAISE(ABORT, 'fix-5: the un-stranding repair failed'); END"
        )
    finally:
        conn.close()

    # ---- session B: WITH an embedder. The write triggers the LATE enrolment,
    # whose repair now fails. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        with pytest.raises(Exception):
            engine.write([_node("N3", '{"summary":"the late write"}')])
        assert not _vector_kind_registered(path), (
            "TORN LATE ENROLMENT: the registry INSERT committed while the un-stranding it owes "
            "did not. `_fathomdb_vector_kinds` now holds `doc` with N1/N2 still carrying "
            "`'up_to_date'` terminals and no vectors — and because the kind is now registered, "
            "every future write SKIPS the enrolment path and therefore skips the repair. The "
            "registry insert and the terminal/cursor repair must commit as ONE transaction"
        )
    finally:
        engine.close()

    conn = sqlite3.connect(path)
    try:
        conn.executescript("DROP TRIGGER fix5_repair_fails")
    finally:
        conn.close()

    # ---- session C: one ordinary write must now enrol AND un-strand, because
    # nothing was left half-done behind it. No re-apply, no operator rebuild. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write([_node("N4", '{"summary":"the healing write"}')])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert _leaf_rows_without_vectors(path) == 0, (
        "SELF-SEALED FALSE READY: `drain()` returned and readiness reads \"ready\", but the rows "
        "the torn enrolment stranded still have no vector at rest. The torn state is invisible "
        "to every later write precisely BECAUSE the kind is already registered — which is why "
        "the two statements have to be atomic"
    )
    assert _vector_rows(path) == 3, "N1, N2 and N4 were all embedded"
