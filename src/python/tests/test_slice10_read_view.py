"""X1 SDK parity — 0.8.20 Slice 10b (R-20-RV read view + R-20-NV node validity).

Opens a REAL engine (tmpdir SQLite, no mocking) and drives the Slice-10b read
surface THROUGH the PyO3 binding. Symbol-presence assertions are deliberately
absent: R-20-X1 requires a live functional harness, so every test below calls
across the FFI and asserts on real returned rows.

Covered, mirroring the engine matrices in
``src/rust/crates/fathomdb-engine/tests/slice10_read_view.rs`` and
``slice10_node_validity.rs``:

  * R-20-RV — the DEFAULT view is unchanged on all five read verbs
    (``read.get``, ``read.get_many``, ``read.list``, ``read.list(filter=...)``,
    ``graph.neighbors``), asserted against a raw-table oracle rather than
    against a second engine call.
  * R-20-RV — ``include_superseded`` returns history (the requirement's named
    acceptance signal) and the point lookup stays deterministic under it.
  * R-20-RV — ``include_inactive`` relaxes ``state = 'active'``.
  * R-20-RV — the flags COMPOSE: the four existence views yield the four
    distinct row sets a truth table predicts.
  * R-20-RV — ``graph.neighbors`` honours the view in all THREE directions.
  * R-20-NV — ``valid_as_of`` selects a world-time instant: a bounded node is
    visible inside its window and invisible outside it, on every read verb;
    ``include_out_of_window`` relaxes the conjunct entirely.
  * R-20-NV — ``read.crossed_boundary_since`` returns real ``BoundaryCrossing``
    rows naming WHICH boundary was crossed.

Validity windows have NO write-side authoring verb in 0.8.20 (a deliberate,
escalated gap), so the fixtures below set ``valid_from``/``valid_until`` with
direct SQL on the CLOSED database — exactly as the engine suite does. The
READ path is what is under test here, and it is exercised only through the SDK.

Cross-binding equivalence anchor: ``src/ts/tests/slice10-read-view.test.ts``
asserts the SAME behaviour for the same inputs (Py ≡ TS, R-X-1).
"""

from __future__ import annotations

import sqlite3
from collections.abc import Sequence

from fathomdb import Engine, Filter, NodeRecord, graph, read
from fathomdb.types import ReadView

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:slice10-read-view"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _node(logical_id: str, body: str, kind: str = "doc") -> dict:
    return {"kind": kind, "body": body, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _edge(from_id: str, to_id: str, logical_id: str) -> dict:
    return {
        "edge": {
            "kind": "link",
            "from": from_id,
            "to": to_id,
            "logical_id": logical_id,
            "source_id": _SOURCE_ID,
        }
    }


def _open(path: str) -> Engine:
    return Engine.open(path, use_default_embedder=False)


def _ids(rows: Sequence[NodeRecord]) -> list[str]:
    return sorted(r.logical_id for r in rows)


def _bodies(rows: Sequence[NodeRecord]) -> list[str]:
    return sorted(r.body for r in rows)


def _set_window(
    path: str, logical_id: str, valid_from: int | None, valid_until: int | None
) -> None:
    """Author a validity window directly on the CLOSED database.

    The engine ships no write verb for this (deliberately out of scope for the
    slice). Writing at rest also keeps the assertion honest: what the SDK reads
    back is what is on disk, not what some engine call claimed to store.
    """

    conn = sqlite3.connect(path)
    try:
        conn.execute(
            "UPDATE canonical_nodes SET valid_from = ?, valid_until = ?"
            " WHERE logical_id = ? AND superseded_at IS NULL",
            (valid_from, valid_until, logical_id),
        )
        conn.commit()
    finally:
        conn.close()


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


def _raw_current_active_bodies(path: str) -> list[str]:
    """Bodies that are CURRENT (``superseded_at IS NULL``) AND ``state='active'``.

    The oracle the default view must reproduce, taken from data at rest.
    """

    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        rows = conn.execute(
            "SELECT body FROM canonical_nodes"
            " WHERE superseded_at IS NULL AND state = 'active'"
        ).fetchall()
    finally:
        conn.close()
    return sorted(r[0] for r in rows)


def _raw_node_count(path: str) -> int:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return conn.execute("SELECT COUNT(*) FROM canonical_nodes").fetchone()[0]
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# (1) R-20-RV — the default view is unchanged on all five read verbs
# ---------------------------------------------------------------------------


def test_default_view_is_unchanged_on_all_five_verbs(db_path: str) -> None:
    """R-20-RV keystone through the binding: omitting ``view=`` reproduces the
    shipped active-and-current-only semantics on every one of the five verbs.

    The expectation is derived from the RAW TABLE (which rows are actually
    current + active on disk), not from a second SDK call — a self-referential
    assertion would pass on broken code.
    """

    engine = _open(db_path)
    try:
        engine.write([_node("A", "alpha v1"), _node("B", "beta")])
        # Supersede A, so a historical row exists on disk.
        engine.write([_node("A", "alpha v2")])
        # Take B out of the active state.
        engine.transition("B", "deleted", "test")
        engine.write([_node("C", "gamma"), _edge("A", "C", "E-AC")])
        engine.drain(timeout_s=30)

        got_get_a = read.get(engine, "A")
        got_get_b = read.get(engine, "B")
        got_many = read.get_many(engine, ["A", "B", "C"])
        got_list = _bodies(read.list(engine, "doc"))
        got_list_filter = _bodies(read.list(engine, "doc", filter=Filter()))
        got_neighbors = _bodies(graph.neighbors(engine, "A", 1, "outgoing"))

        # Passing `view=None` EXPLICITLY must be identical to omitting it — the
        # `.pyi` declares `view: ReadView | None = None`, so both call shapes
        # are on the governed surface and both must hit the strict default.
        assert _bodies(read.list(engine, "doc", view=None)) == got_list
        assert _bodies(read.list(engine, "doc", view=ReadView())) == got_list
        # `NodeRecord` is a frozen dataclass, so this compares the WHOLE record
        # (and stays None-safe if the strict default ever regressed).
        assert read.get(engine, "A", view=None) == got_get_a
        assert read.get(engine, "A", view=ReadView()) == got_get_a
        assert read.get_many(engine, ["A", "B", "C"], view=ReadView()) == got_many
    finally:
        engine.close()

    # Data-at-rest oracle, read after the engine released its lock.
    expected = _raw_current_active_bodies(db_path)
    assert expected == ["alpha v2", "gamma"], (
        "fixture precondition: on disk exactly `alpha v2` and `gamma` are current+active "
        f"(`alpha v1` superseded, `beta` deleted); got {expected}"
    )

    assert got_get_a is not None and got_get_a.body == "alpha v2", (
        "default read.get must return the CURRENT version"
    )
    assert got_get_b is None, "default read.get must not return a deleted node"
    assert [None if r is None else r.body for r in got_many] == [
        "alpha v2",
        None,
        "gamma",
    ], "default read.get_many must be current+active only, in REQUEST order"
    assert got_list == expected, "default read.list must match the raw oracle"
    assert got_list_filter == expected, "default read.list(filter=...) must match too"
    assert got_neighbors == ["gamma"], (
        "default graph.neighbors must traverse to the current+active neighbor only"
    )


# ---------------------------------------------------------------------------
# (2) R-20-RV — include_superseded returns history
# ---------------------------------------------------------------------------


def test_include_superseded_returns_history(db_path: str) -> None:
    """The requirement's NAMED acceptance signal, live through the binding."""

    engine = _open(db_path)
    try:
        engine.write([_node("A", "v1")])
        engine.write([_node("A", "v2")])
        engine.write([_node("A", "v3")])
        engine.drain(timeout_s=30)

        assert _bodies(read.list(engine, "doc")) == ["v3"], (
            "the strict view sees only the current version"
        )
        history = _bodies(read.list(engine, "doc", view=ReadView(include_superseded=True)))
        assert history == ["v1", "v2", "v3"], (
            f"include_superseded must return the FULL history, not one extra row; got {history}"
        )
    finally:
        engine.close()


def test_point_lookup_under_include_superseded_is_deterministic(db_path: str) -> None:
    """With ``include_superseded`` a ``logical_id`` matches several rows, so the
    point-lookup slot must resolve DETERMINISTICALLY to the newest version."""

    engine = _open(db_path)
    try:
        engine.write([_node("A", "v1")])
        engine.write([_node("A", "v2")])
        engine.write([_node("A", "v3")])
        engine.drain(timeout_s=30)

        view = ReadView(include_superseded=True)
        for _ in range(5):
            row = read.get(engine, "A", view=view)
            assert row is not None and row.body == "v3", (
                "read.get under include_superseded must always resolve to the newest version"
            )
        many = read.get_many(engine, ["A"], view=view)
        assert many[0] is not None and many[0].body == "v3"
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (3) R-20-RV — the state/active relax flag
# ---------------------------------------------------------------------------


def test_include_inactive_returns_non_active_states(db_path: str) -> None:
    """``include_inactive`` relaxes ``state = 'active'`` and NOTHING else."""

    engine = _open(db_path)
    try:
        engine.write([_node("A", "kept"), _node("B", "dropped")])
        engine.transition("B", "deleted", "test")
        engine.drain(timeout_s=30)

        assert _bodies(read.list(engine, "doc")) == ["kept"]
        relaxed = _bodies(read.list(engine, "doc", view=ReadView(include_inactive=True)))
        assert relaxed == ["dropped", "kept"], (
            f"include_inactive must surface the deleted node; got {relaxed}"
        )
        row = read.get(engine, "B", view=ReadView(include_inactive=True))
        assert row is not None and row.body == "dropped", (
            "include_inactive must apply on the point-lookup verb too"
        )
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (4) R-20-RV — the flags COMPOSE
# ---------------------------------------------------------------------------


def test_existence_flags_compose_independently(db_path: str) -> None:
    """The read-mode matrix: four existence views over a corpus holding one row
    of each (current|superseded) x (active|inactive) class.

    Each flag must drop exactly one conjunct, so the four views yield four
    distinct, predicted sets — including the BOTH-flags cell, which is the
    whole point of a composable relax-flag design.
    """

    engine = _open(db_path)
    try:
        # A: current + active.
        engine.write([_node("A", "current-active")])
        # B: superseded + active (v1 superseded by v2; both rows stay active).
        engine.write([_node("B", "superseded-active")])
        engine.write([_node("B", "current-active-b")])
        # C: current + inactive.
        engine.write([_node("C", "current-inactive")])
        engine.transition("C", "deleted", "test")
        engine.drain(timeout_s=30)

        matrix = [
            (ReadView(), ["current-active", "current-active-b"]),
            (
                ReadView(include_superseded=True),
                ["current-active", "current-active-b", "superseded-active"],
            ),
            (
                ReadView(include_inactive=True),
                ["current-active", "current-active-b", "current-inactive"],
            ),
            (
                ReadView(include_superseded=True, include_inactive=True),
                [
                    "current-active",
                    "current-active-b",
                    "current-inactive",
                    "superseded-active",
                ],
            ),
        ]
        for view, expected in matrix:
            got = _bodies(read.list(engine, "doc", view=view))
            assert got == sorted(expected), (
                f"read-mode matrix cell {view} must yield exactly the predicted row set; got {got}"
            )

        # The fully-relaxed view is a FILTER, never a source: it must return
        # exactly the rows that exist in `canonical_nodes` and no more.
        widest = read.list(
            engine,
            "doc",
            limit=1000,
            view=ReadView(
                include_superseded=True, include_inactive=True, include_out_of_window=True
            ),
        )
        widest_len = len(widest)
    finally:
        engine.close()

    assert widest_len == _raw_node_count(db_path), (
        "the fully-relaxed view must return exactly the rows on disk — no more"
    )


def test_read_list_filter_form_inherits_the_view(db_path: str) -> None:
    """``read.list(filter=...)`` lowers through a different native entry point,
    so it must inherit the view rather than silently dropping it."""

    engine = _open(db_path)
    try:
        engine.write([_node("A", '{"n":1}')])
        engine.write([_node("A", '{"n":2}')])
        engine.drain(timeout_s=30)

        assert len(read.list(engine, "doc", filter=Filter())) == 1
        relaxed = read.list(
            engine, "doc", filter=Filter(), view=ReadView(include_superseded=True)
        )
        assert len(relaxed) == 2, (
            "read.list(filter=...) must pass the view down, not drop it on the "
            f"filter-lowering path; got {len(relaxed)} rows"
        )
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (5) R-20-RV — graph.neighbors honours the view in every direction
# ---------------------------------------------------------------------------


def test_graph_neighbors_honours_the_view_in_every_direction(db_path: str) -> None:
    """Full direction x view matrix on a symmetric graph.

    There are exactly THREE ``TraversalDirection`` variants. "Works on outgoing
    but silently not on incoming" is the exact failure mode the uniformity
    requirement exists to prevent, so every cell is asserted.
    """

    engine = _open(db_path)
    try:
        engine.write(
            [
                _node("R", "root"),
                _node("OA", "out-active"),
                _node("OI", "out-inactive"),
                _node("IA", "in-active"),
                _node("II", "in-inactive"),
                _edge("R", "OA", "E-R-OA"),
                _edge("R", "OI", "E-R-OI"),
                _edge("IA", "R", "E-IA-R"),
                _edge("II", "R", "E-II-R"),
            ]
        )
        engine.transition("OI", "deleted", "t")
        engine.transition("II", "deleted", "t")
        engine.drain(timeout_s=30)

        cases: list[tuple[graph.TraversalDirection, ReadView, list[str]]] = [
            ("outgoing", ReadView(), ["OA"]),
            ("outgoing", ReadView(include_inactive=True), ["OA", "OI"]),
            ("incoming", ReadView(), ["IA"]),
            ("incoming", ReadView(include_inactive=True), ["IA", "II"]),
            ("both", ReadView(), ["IA", "OA"]),
            ("both", ReadView(include_inactive=True), ["IA", "II", "OA", "OI"]),
        ]
        for direction, view, expected in cases:
            got = _ids(graph.neighbors(engine, "R", 1, direction, view=view))
            assert got == sorted(expected), (
                f"graph.neighbors matrix cell ({direction}, {view}) must yield the "
                f"predicted set; got {got}"
            )
    finally:
        engine.close()


def test_graph_neighbors_view_reaches_the_recursive_join(db_path: str) -> None:
    """The view must apply at the BFS RECURSIVE JOIN, not just at the anchor
    and the final projection.

    Proven by traversing THROUGH a non-active intermediate: reaching the far
    node is only possible if the frontier expanded past the relaxed middle.
    """

    engine = _open(db_path)
    try:
        engine.write(
            [
                _node("R", "root"),
                _node("D", "middle"),
                _node("E", "far"),
                _edge("R", "D", "E-RD"),
                _edge("D", "E", "E-DE"),
            ]
        )
        engine.transition("D", "deleted", "t")
        engine.drain(timeout_s=30)

        assert graph.neighbors(engine, "R", 3, "outgoing") == [], (
            "strict view: a non-active intermediate blocks the frontier"
        )
        relaxed = _bodies(
            graph.neighbors(engine, "R", 3, "outgoing", view=ReadView(include_inactive=True))
        )
        assert relaxed == ["far", "middle"], (
            "include_inactive must apply at the RECURSIVE JOIN: reaching `far` is only "
            f"possible if the frontier expanded THROUGH `middle`; got {relaxed}"
        )
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (6) R-20-NV — valid_as_of selects a world-time instant
# ---------------------------------------------------------------------------


def test_valid_as_of_window_visibility_on_every_read_verb(db_path: str) -> None:
    """A bounded node is visible at an instant INSIDE its window and invisible
    outside it, on all five read verbs, selected via the bound ``:now`` seam."""

    engine = _open(db_path)
    try:
        engine.write([_node("R", "root"), _node("X", "expiring"), _edge("R", "X", "E-RX")])
        engine.drain(timeout_s=30)
    finally:
        engine.close()

    _set_window(db_path, "X", 1000, 2000)
    # The fixture is real, verified at rest before anything is read back.
    assert _raw_window(db_path, "X") == (1000, 2000)
    assert _raw_window(db_path, "R") == (None, None)

    engine = _open(db_path)
    try:
        inside = ReadView(valid_as_of=1500)
        outside = ReadView(valid_as_of=5000)

        # Inside the window: X is visible on every verb.
        assert read.get(engine, "X", view=inside) is not None, "read.get inside window"
        assert read.get_many(engine, ["X"], view=inside)[0] is not None, (
            "read.get_many inside window"
        )
        assert _ids(read.list(engine, "doc", view=inside)) == ["R", "X"]
        assert _ids(read.list(engine, "doc", filter=Filter(), view=inside)) == ["R", "X"]
        assert _ids(graph.neighbors(engine, "R", 1, "outgoing", view=inside)) == ["X"], (
            "graph.neighbors inside window"
        )

        # Outside the window: X vanishes from every verb; R (unbounded) never does.
        assert read.get(engine, "X", view=outside) is None, "read.get outside window"
        assert read.get_many(engine, ["X"], view=outside)[0] is None, (
            "read.get_many outside window"
        )
        assert _ids(read.list(engine, "doc", view=outside)) == ["R"]
        assert _ids(read.list(engine, "doc", filter=Filter(), view=outside)) == ["R"]
        assert graph.neighbors(engine, "R", 1, "outgoing", view=outside) == [], (
            "graph.neighbors outside window"
        )

        # The window is HALF-OPEN [valid_from, valid_until): lower bound
        # INCLUSIVE, upper bound EXCLUSIVE.
        assert _ids(read.list(engine, "doc", view=ReadView(valid_as_of=999))) == ["R"]
        assert _ids(read.list(engine, "doc", view=ReadView(valid_as_of=1000))) == ["R", "X"]
        assert _ids(read.list(engine, "doc", view=ReadView(valid_as_of=1999))) == ["R", "X"]
        assert _ids(read.list(engine, "doc", view=ReadView(valid_as_of=2000))) == ["R"]

        # `include_out_of_window` drops the validity conjunct entirely, and
        # COMPOSES with an instant that would otherwise exclude X.
        assert _ids(
            read.list(
                engine,
                "doc",
                view=ReadView(include_out_of_window=True, valid_as_of=5000),
            )
        ) == ["R", "X"], "include_out_of_window must ignore valid_as_of entirely"
    finally:
        engine.close()


def test_validity_and_existence_flags_compose(db_path: str) -> None:
    """The validity axis composes with the existence axis: a node that is BOTH
    out-of-window and non-active needs both relaxations to surface."""

    engine = _open(db_path)
    try:
        engine.write([_node("R", "root"), _node("G", "gone-and-expired")])
        engine.transition("G", "deleted", "t")
        engine.drain(timeout_s=30)
    finally:
        engine.close()

    _set_window(db_path, "G", 1000, 2000)
    assert _raw_window(db_path, "G") == (1000, 2000)

    engine = _open(db_path)
    try:
        at_5000 = ReadView(valid_as_of=5000)
        assert _ids(read.list(engine, "doc", view=at_5000)) == ["R"]
        # Only the existence flag: still out-of-window.
        assert _ids(
            read.list(engine, "doc", view=ReadView(include_inactive=True, valid_as_of=5000))
        ) == ["R"], "include_inactive alone must not resurrect an out-of-window node"
        # Only the window flag: still non-active.
        assert _ids(
            read.list(
                engine, "doc", view=ReadView(include_out_of_window=True, valid_as_of=5000)
            )
        ) == ["R"], "include_out_of_window alone must not resurrect a deleted node"
        # BOTH flags: the node surfaces. This is the composition claim.
        assert _ids(
            read.list(
                engine,
                "doc",
                view=ReadView(
                    include_inactive=True, include_out_of_window=True, valid_as_of=5000
                ),
            )
        ) == ["G", "R"], "the two relax flags must COMPOSE (each drops exactly one conjunct)"
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (7) R-20-NV — read.crossed_boundary_since
# ---------------------------------------------------------------------------


def test_crossed_boundary_since_reports_both_boundaries(db_path: str) -> None:
    """The delta's ONE net-new command, called live and asserted on real
    ``BoundaryCrossing`` data — not on symbol presence."""

    engine = _open(db_path)
    try:
        engine.write(
            [
                _node("OPENED", "opened"),
                _node("CLOSED", "closed"),
                _node("BOTH", "both"),
                _node("OUTSIDE", "outside"),
                _node("UNBOUNDED", "unbounded"),
            ]
        )
        engine.drain(timeout_s=30)
    finally:
        engine.close()

    # The interrogated interval is (1000, 2000].
    _set_window(db_path, "OPENED", 1500, None)  # became valid inside
    _set_window(db_path, "CLOSED", 0, 1500)  # became invalid inside
    _set_window(db_path, "BOTH", 1200, 1800)  # both boundaries inside
    _set_window(db_path, "OUTSIDE", 5000, 6000)  # neither inside
    # UNBOUNDED keeps NULL/NULL — it can never cross a boundary.

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
        ], (
            "the hook must report exactly the nodes crossing a boundary in (1000, 2000], each "
            f"carrying the boundary(ies) it crossed; got {got}"
        )

        # The carried node is a real, fully-populated NodeRecord.
        by_id = {c.node.logical_id: c for c in crossings}
        assert by_id["BOTH"].node.body == "both"
        assert by_id["BOTH"].node.kind == "doc"
        assert by_id["BOTH"].node.write_cursor > 0

        # A row with no window can never cross a boundary, even over the
        # widest interval — so the hook is silent on every pre-step-22 row.
        # Bounds stay inside JS `Number.MAX_SAFE_INTEGER` so the i64 FFI
        # round-trip is exact in BOTH bindings (TS mirror uses the same
        # values); 1e12 is still astronomically outside every window here.
        widest = read.crossed_boundary_since(
            engine, -1_000_000_000_000, view=ReadView(valid_as_of=1_000_000_000_000)
        )
        assert "UNBOUNDED" not in {c.node.logical_id for c in widest}, (
            "unbounded rows cannot cross a boundary"
        )
    finally:
        engine.close()


def test_crossed_boundary_since_honours_existence_flags(db_path: str) -> None:
    """The hook honours the view's EXISTENCE flags: a deleted node is out by
    default and in under ``include_inactive``."""

    engine = _open(db_path)
    try:
        engine.write([_node("LIVE", "live"), _node("GONE", "gone")])
        engine.transition("GONE", "deleted", "t")
        engine.drain(timeout_s=30)
    finally:
        engine.close()

    _set_window(db_path, "LIVE", 1500, None)
    _set_window(db_path, "GONE", 1500, None)

    engine = _open(db_path)
    try:
        default_view = ReadView(valid_as_of=2000)
        got = [
            c.node.logical_id
            for c in read.crossed_boundary_since(engine, 1000, view=default_view)
        ]
        assert got == ["LIVE"], f"the default view excludes the deleted node; got {got}"

        relaxed = ReadView(include_inactive=True, valid_as_of=2000)
        got = sorted(
            c.node.logical_id for c in read.crossed_boundary_since(engine, 1000, view=relaxed)
        )
        assert got == ["GONE", "LIVE"], (
            f"include_inactive must widen the hook's candidate set too; got {got}"
        )
    finally:
        engine.close()
