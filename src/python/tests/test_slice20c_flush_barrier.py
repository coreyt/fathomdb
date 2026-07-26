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
# fix-2: the arms whose FAILURE MODE is a wedged projection worker use a shorter
# barrier — an assertion that "`drain` did not hang" should not cost two minutes
# per arm when it is red.
_WEDGE_TIMEOUT_S = 30.0


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test")


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


def _leaf_rows_of_kind_without_vectors(path: str, kind: str) -> int:
    """The same un-joined at-rest probe, narrowed to one ``canonical_nodes.kind``.

    fix-2 needs the narrowed form because its fixture deliberately holds a kind
    that gets NO dense arm, so the corpus-wide count is legitimately non-zero. It
    still does not join ``_fathomdb_vector_kinds``.
    """

    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        row = conn.execute(
            "SELECT COUNT(*) FROM canonical_nodes n"
            " LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor"
            " WHERE n.row_kind IN ('leaf', 'coverage') AND n.kind = ?"
            " AND v.write_cursor IS NULL",
            (kind,),
        ).fetchone()
        return int(row[0])
    finally:
        conn.close()


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


def _terminal_state(path: str, logical_id: str) -> str | None:
    """The raw ``_fathomdb_projection_terminal`` state for one node's cursor.

    The two tokens are the whole fix-3 finding: ``'failed'`` is what
    retry-exhaustion against an ABSENT embedder records, and no graft path reopens
    a ``'failed'`` terminal (deliberately — re-enqueueing one would loop a
    genuinely-failing row forever). ``'up_to_date'`` is what the not-enqueued
    branch records, and that IS the stranded shape a later live-embedder session
    reopens.
    """

    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        row = conn.execute(
            "SELECT t.state FROM canonical_nodes n"
            " JOIN _fathomdb_projection_terminal t ON t.write_cursor = n.write_cursor"
            " WHERE n.logical_id = ? AND n.superseded_at IS NULL",
            (logical_id,),
        ).fetchone()
        return None if row is None else str(row[0])
    finally:
        conn.close()


def _projection_failures(path: str) -> int:
    return _query(
        path,
        "SELECT COUNT(*) FROM operational_mutations"
        " WHERE collection_name = 'projection_failures'",
    )


def test_no_embedder_session_does_not_enqueue_for_an_already_enrolled_kind(tmp_path) -> None:
    """fix-3 (codex §9 round 3 [P1]) — the live-embedder gate reaches the ENQUEUE.

    ``_fathomdb_vector_kinds`` is DURABLE. Once an embedder-backed session has
    enrolled a kind, reopening the same database with ``use_default_embedder=False``
    and writing that kind still reached the enqueue and handed the row to a worker
    that has no embedder: retry exhaustion recorded an
    ``EmbedderNotConfiguredError`` ``'failed'`` terminal plus a
    ``projection_failures`` audit row, and because the graft path only reopens
    ``'up_to_date'`` terminals, reopening WITH an embedder left that write
    permanently unembedded while readiness reported ``"ready"``. A FALSE READY.

    Pins the whole graceful-absent sentence: the no-embedder session ACCEPTS the
    write, keeps it LEXICALLY SEARCHABLE, does NOT count it as outstanding work
    for ``drain``, and DOES let a later live-embedder session GRAFT it — with no
    re-apply and no further write, because reopening is itself the graft point.

    NOT mirrored from the Rust suite: the "readiness never read ``ready`` while
    the row lacked its vector" probe. That one needs a deliberately-slowed
    embedder to be race-free, which the bindings cannot install
    (``use_default_embedder`` takes no delay). The at-rest oracles below are the
    falsifying ones in every language.
    """

    _skip_if_no_network()
    path = str(tmp_path / "flush_no_embedder_enqueue.sqlite")

    # ---- session 1: WITH an embedder. This is what durably ENROLS `doc`. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.configure_projections([_vector_spec()])
        engine.write([_node("N1", '{"summary":"embedded in session one"}')])
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _vector_kind_registered(path), "fixture: session 1 enrolled `doc`"
        assert _leaf_rows_without_vectors(path) == 0, "fixture: session 1's row is embedded"
    finally:
        engine.close()

    # ---- session 2: the SAME database, reopened with NO embedder. ----
    engine = Engine.open(path, use_default_embedder=False)
    try:
        assert _vector_kind_registered(path), (
            "fixture: the enrolment persists across the reopen — that is the whole finding"
        )
        engine.write([_node("N2", '{"summary":"written with no dense arm"}')])
        # Generous on purpose: at baseline the barrier only clears once the worker
        # has burned its whole retry ladder into the `'failed'` terminal, and a
        # short timeout would abort before that terminal lands — which would make
        # session 3 graft a merely-unterminated row and the test vacuously green.
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready", "no live embedder ⇒ nothing is outstanding"
    finally:
        engine.close()

    assert (
        _query(
            path,
            "SELECT COUNT(*) FROM search_index s JOIN canonical_nodes n"
            " ON n.write_cursor = s.write_cursor WHERE n.logical_id = 'N2'",
        )
        == 1
    ), "the write is ACCEPTED and stays lexically searchable"
    assert _projection_failures(path) == 0, (
        "NO-EMBEDDER ENQUEUE: a session with no dense arm must not enqueue vector work — an "
        "ABSENT embedder is an environment fact, not an embed failure"
    )
    assert _terminal_state(path, "N2") == "up_to_date", (
        "NO-EMBEDDER ENQUEUE: the row must take the NOT-ENQUEUED branch's `'up_to_date'` terminal "
        "— a `'failed'` terminal is permanent by design, so enqueueing here loses the write forever"
    )
    assert _vector_rows(path) == 1, "no dense arm ⇒ no vector for N2 yet"
    assert _leaf_rows_without_vectors(path) == 1

    # ---- session 3: WITH an embedder again. No re-apply and no further write. ----
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.drain(timeout_s=_DRAIN_TIMEOUT_S)
        assert _readiness(engine) == "ready"
    finally:
        engine.close()

    assert _leaf_rows_without_vectors(path) == 0, (
        'FALSE-READY: `drain()` returned and readiness reads "ready", but the row written in the '
        "no-embedder session still has no vector at rest"
    )
    assert _vector_rows(path) == 2, "the stranded row was grafted, and only it"
    assert _projection_failures(path) == 0, "the graft produced no failures either"
