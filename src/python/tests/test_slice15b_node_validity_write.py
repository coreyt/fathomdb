"""X1 SDK parity — 0.8.20 Slice 15b (TC-34: node-validity WRITE-side authoring).

Slice 10b shipped node validity read-only, so the only way to author a window
was raw SQL on a closed database — which is exactly what
``test_slice10_read_view.py`` still does. A window a caller can filter on but
can never set is dead surface. This suite drives the AUTHORING path through the
PyO3 binding: every window below is set by ``engine.write(...)``, and no test
here sets ``valid_from``/``valid_until`` with SQL.

``sqlite3`` IS used, but only as a READ oracle on a CLOSED database: the
"omitted fields land NULL/NULL" assertion has to read the raw table, because a
read-verb assertion would pass on broken code (a row wrongly written with
``valid_from = 0`` is also visible under a default view).

Covered, mirroring ``src/rust/crates/fathomdb-engine/tests/
slice15b_node_validity_write.rs``:

  * TC-34 — round trip: authored window visible INSIDE, invisible OUTSIDE, on
    the deterministic ``valid_as_of`` seam (no wall clock, no sleep).
  * TC-34 — half-open ``[valid_from, valid_until)`` survives the write path.
  * TC-34 — unbounded sides (one bound authored, the other omitted).
  * TC-34 — omitting BOTH fields lands NULL/NULL (RAW TABLE oracle) and leaves
    default-view visibility unchanged.
  * TC-34 — an unsatisfiable window (``from >= until``) is a typed refusal, and
    a non-integer value is a typed refusal rather than a silent coercion.
  * TC-34 — ``read.crossed_boundary_since`` works end-to-end on an SDK-authored
    window.

Cross-binding equivalence anchor:
``src/ts/tests/slice15b-node-validity-write.test.ts`` asserts the SAME
behaviour for the same inputs (Py ≡ TS, R-X-1).
"""

from __future__ import annotations

import sqlite3
from collections.abc import Sequence

import pytest

from fathomdb import Engine, NodeRecord, read
from fathomdb.errors import InvalidArgumentError, WriteValidationError
from fathomdb.types import ReadView

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:slice15b-node-validity-write"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _windowed(
    logical_id: str,
    body: str,
    valid_from: int | None,
    valid_until: int | None,
) -> dict:
    """A node write item carrying an explicit window.

    A ``None`` bound is OMITTED from the dict rather than sent as ``None`` —
    that is the shape a caller who simply does not have that bound produces.
    (Both forms must behave identically; the explicit-``None`` form is asserted
    separately in :func:`test_explicit_none_is_equivalent_to_omission`.)
    """

    item: dict = {"kind": "doc", "body": body, "logical_id": logical_id, "source_id": _SOURCE_ID}
    if valid_from is not None:
        item["valid_from"] = valid_from
    if valid_until is not None:
        item["valid_until"] = valid_until
    return item


def _plain(logical_id: str, body: str) -> dict:
    """A node write item that omits the window entirely — the pre-slice shape."""

    return {"kind": "doc", "body": body, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _open(path: str) -> Engine:
    return Engine.open(path, use_default_embedder=False)


def _seed(path: str, batch: list[dict]) -> None:
    """Seed a batch on a fresh engine, drain, and CLOSE — freeing the file."""

    engine = _open(path)
    try:
        engine.write(batch)
        engine.drain(timeout_s=30)
    finally:
        engine.close()


def _ids(rows: Sequence[NodeRecord]) -> list[str]:
    return sorted(r.logical_id for r in rows)


def _raw_window(path: str, logical_id: str) -> tuple[int | None, int | None]:
    """Read a window back from the raw table — the data-at-rest oracle."""

    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute(
            "SELECT valid_from, valid_until FROM canonical_nodes"
            " WHERE logical_id = ? AND superseded_at IS NULL",
            (logical_id,),
        ).fetchone()
    finally:
        conn.close()


def _raw_row_count(path: str, logical_id: str) -> int:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute(
            "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = ?",
            (logical_id,),
        ).fetchone()[0]
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# (1) TC-34 — the round trip
# ---------------------------------------------------------------------------


def test_authored_window_round_trips_through_valid_as_of(db_path: str) -> None:
    """TC-34 keystone: author a window through the SDK, then read INSIDE it
    (present) and OUTSIDE it on both sides (absent).

    The ``valid_as_of`` seam is a BOUND parameter, so this needs no wall clock
    and no sleep.
    """

    _seed(db_path, [_windowed("WINDOWED", "bounded body", 1000, 2000)])

    # On disk exactly as authored — no coercion, no clock.
    assert _raw_window(db_path, "WINDOWED") == (1000, 2000)

    engine = _open(db_path)
    try:
        # INSIDE.
        assert read.get(engine, "WINDOWED", view=ReadView(valid_as_of=1500)) is not None
        assert _ids(read.list(engine, "doc", view=ReadView(valid_as_of=1500))) == ["WINDOWED"]

        # OUTSIDE, both sides.
        assert read.get(engine, "WINDOWED", view=ReadView(valid_as_of=500)) is None
        assert read.get(engine, "WINDOWED", view=ReadView(valid_as_of=2500)) is None
        assert read.list(engine, "doc", view=ReadView(valid_as_of=2500)) == []

        # The escape hatch still relaxes the conjunct.
        relaxed = ReadView(valid_as_of=2500, include_out_of_window=True)
        assert read.get(engine, "WINDOWED", view=relaxed) is not None
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (2) TC-34 — half-open boundaries
# ---------------------------------------------------------------------------


def test_authored_window_is_half_open(db_path: str) -> None:
    """``[valid_from, valid_until)``: lower INCLUSIVE, upper EXCLUSIVE — asserted
    on a window that went through the WRITE path, so a bound silently shifted by
    one second would fail here."""

    _seed(db_path, [_windowed("HALFOPEN", "boundary body", 1000, 2000)])

    engine = _open(db_path)
    try:
        assert read.get(engine, "HALFOPEN", view=ReadView(valid_as_of=999)) is None
        assert read.get(engine, "HALFOPEN", view=ReadView(valid_as_of=1000)) is not None
        assert read.get(engine, "HALFOPEN", view=ReadView(valid_as_of=1999)) is not None
        assert read.get(engine, "HALFOPEN", view=ReadView(valid_as_of=2000)) is None
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (3) TC-34 — unbounded sides
# ---------------------------------------------------------------------------


def test_one_authored_bound_leaves_the_other_side_unbounded(db_path: str) -> None:
    _seed(
        db_path,
        [
            _windowed("FROM_ONLY", "from only", 1000, None),
            _windowed("UNTIL_ONLY", "until only", None, 2000),
        ],
    )

    # The omitted side is NULL on disk, not a sentinel.
    assert _raw_window(db_path, "FROM_ONLY") == (1000, None)
    assert _raw_window(db_path, "UNTIL_ONLY") == (None, 2000)

    engine = _open(db_path)
    try:
        assert read.get(engine, "FROM_ONLY", view=ReadView(valid_as_of=999)) is None
        assert read.get(engine, "FROM_ONLY", view=ReadView(valid_as_of=1000)) is not None
        assert read.get(engine, "FROM_ONLY", view=ReadView(valid_as_of=4_000_000_000)) is not None

        assert read.get(engine, "UNTIL_ONLY", view=ReadView(valid_as_of=0)) is not None
        assert read.get(engine, "UNTIL_ONLY", view=ReadView(valid_as_of=1999)) is not None
        assert read.get(engine, "UNTIL_ONLY", view=ReadView(valid_as_of=2000)) is None
    finally:
        engine.close()


def test_explicit_none_is_equivalent_to_omission(db_path: str) -> None:
    """``valid_from=None`` must behave exactly as omitting the key — the pyo3
    ``dict_str``/``dict_get`` convention treats a present ``None`` as absent, and
    the new integer reader must not diverge from it."""

    _seed(
        db_path,
        [
            {
                "kind": "doc",
                "body": "explicit none",
                "logical_id": "EXPLICIT",
                "source_id": _SOURCE_ID,
                "valid_from": None,
                "valid_until": None,
            }
        ],
    )

    assert _raw_window(db_path, "EXPLICIT") == (None, None)


# ---------------------------------------------------------------------------
# (4) TC-34 — MUST NOT REGRESS: omitting both fields
# ---------------------------------------------------------------------------


def test_omitted_window_lands_null_null_and_preserves_default_visibility(
    db_path: str,
) -> None:
    """A write that omits both fields lands NULL/NULL — what every row predating
    this slice carries — so default-view behaviour is byte-stable.

    The RAW TABLE is the oracle deliberately: a read-verb assertion would pass on
    broken code, because a row wrongly written with ``valid_from = 0`` is ALSO
    visible under a default view.
    """

    _seed(db_path, [_plain("PLAIN", "no window authored")])

    assert _raw_window(db_path, "PLAIN") == (None, None), (
        "a write omitting the window MUST land NULL/NULL — not 0, not now(), not a sentinel"
    )

    engine = _open(db_path)
    try:
        for instant in (0, 1, 1000, 2_000_000_000):
            assert read.get(engine, "PLAIN", view=ReadView(valid_as_of=instant)) is not None, (
                f"NULL/NULL row must be valid at instant {instant}"
            )

        # And through the shipped DEFAULT view (no valid_as_of — resolves to the
        # wall clock), which is the path every existing caller takes.
        assert read.get(engine, "PLAIN") is not None
        assert _ids(read.list(engine, "doc")) == ["PLAIN"]
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (5) TC-34 — typed refusals
# ---------------------------------------------------------------------------


def test_unsatisfiable_window_is_a_typed_refusal(db_path: str) -> None:
    """``valid_from >= valid_until`` can never match any instant under a
    half-open predicate, so it is refused rather than silently stored."""

    engine = _open(db_path)
    try:
        with pytest.raises(InvalidArgumentError):
            engine.write([_windowed("BAD", "inverted", 2000, 1000)])
        with pytest.raises(InvalidArgumentError):
            engine.write([_windowed("BAD", "empty", 1500, 1500)])

        # The refusal rejects the WHOLE batch.
        with pytest.raises(InvalidArgumentError):
            engine.write(
                [_plain("GOOD", "well formed"), _windowed("BAD", "inverted", 2000, 1000)]
            )
    finally:
        engine.close()

    assert _raw_row_count(db_path, "BAD") == 0, "a refused write must not land a row"
    assert _raw_row_count(db_path, "GOOD") == 0, (
        "batch rejection must not commit the sibling row"
    )


def test_single_bound_is_never_refused(db_path: str) -> None:
    """Only the PAIR can be unsatisfiable — a one-sided window never is,
    however extreme its value."""

    engine = _open(db_path)
    try:
        engine.write(
            [
                _windowed("A", "from only", 2**62, None),
                _windowed("B", "until only", None, -(2**62)),
            ]
        )
    finally:
        engine.close()


@pytest.mark.parametrize("bad", ["1000", 10.5, True, [], {}])
def test_non_integer_bound_is_a_typed_refusal(db_path: str, bad: object) -> None:
    """A non-integer bound is refused, never coerced.

    ``True`` is in the matrix on purpose: ``bool`` is a subclass of ``int`` in
    Python, so a naive ``extract::<i64>()`` would silently accept it as ``1``.
    """

    engine = _open(db_path)
    try:
        with pytest.raises(WriteValidationError):
            engine.write(
                [
                    {
                        "kind": "doc",
                        "body": "bad",
                        "logical_id": "BADTYPE",
                        "source_id": _SOURCE_ID,
                        "valid_from": bad,
                    }
                ]
            )
    finally:
        engine.close()

    assert _raw_row_count(db_path, "BADTYPE") == 0


# ---------------------------------------------------------------------------
# (6) TC-34 — the boundary hook, end-to-end on an authored window
# ---------------------------------------------------------------------------


def test_crossed_boundary_since_works_on_authored_windows(db_path: str) -> None:
    """``crossed_boundary_since`` shipped in Slice 10b but could only ever be
    exercised against raw-SQL fixtures. It now works end-to-end."""

    _seed(
        db_path,
        [
            _windowed("OPENED", "opened", 1500, None),
            _windowed("CLOSED", "closed", 0, 1500),
            _windowed("BOTH", "both", 1200, 1800),
            _windowed("OUTSIDE", "outside", 5000, 6000),
            _plain("UNBOUNDED", "unbounded"),
        ],
    )

    engine = _open(db_path)
    try:
        crossings = read.crossed_boundary_since(engine, 1000, view=ReadView(valid_as_of=2000))
        got = sorted(
            (c.node.logical_id, c.became_valid_at, c.became_invalid_at) for c in crossings
        )
        assert got == [
            ("BOTH", 1200, 1800),
            ("CLOSED", None, 1500),
            ("OPENED", 1500, None),
        ]
    finally:
        engine.close()
