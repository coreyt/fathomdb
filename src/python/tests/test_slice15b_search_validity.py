"""X1 SDK parity — 0.8.20 Slice 15b fix-2 (codex §9 [P2]: the validity window
must govern ``search``, not just the five read verbs).

Slice 10b scoped ``ReadView`` to ``read.get`` / ``read.get_many`` /
``read.list`` / ``read.list_filter`` / ``graph.neighbors`` and left ``search``
out. That was defensible only while NO SDK caller could author a window — the
only way to set one was raw SQL, so the gap was unreachable. Slice 15b (TC-34)
made authoring reachable from Python and TypeScript, turning a latent gap into a
LIVE DEFECT: a caller could write a node whose window had already closed, watch
``read.get`` correctly hide it, and still get it back from ``search``.

``dev/design/record-lifecycle-protocol/api-surface.md:50`` always specified
``ReadView`` as an optional argument on **``search``** alongside the read verbs.

``sqlite3`` is used ONLY as a data-at-rest oracle on a CLOSED database: a
search-based assertion alone can pass on broken code, because a node that was
never indexed is also "not returned". The raw table proves the window landed AND
that the body reached the FTS projection, so an absence below is a filtering
decision rather than a missing row.

Windows are chosen far from the real clock (epoch 1000..2000 = 1970;
4_000_000_000 = year 2096) so the DEFAULT-view assertions are unambiguous
without pinning ``valid_as_of``. Every pinned assertion rides the BOUND ``:now``
seam — no wall clock, no sleep.

Cross-binding equivalence anchor:
``src/ts/tests/slice15b-search-validity.test.ts`` asserts the SAME behaviour for
the same inputs (Py ≡ TS, R-X-1), and
``src/rust/crates/fathomdb-engine/tests/slice15b_search_validity.rs`` is the
engine-level mirror.
"""

from __future__ import annotations

import sqlite3

import pytest

from fathomdb import Engine
from fathomdb.errors import InvalidArgumentError
from fathomdb.types import ReadView, SearchResult

_SOURCE_ID = "py-test:slice15b-search-validity"

#: Epoch second comfortably in the FUTURE relative to any real test clock.
_FAR_FUTURE = 4_000_000_000
#: Upper bound of a window that closed in 1970 — comfortably in the PAST.
_FAR_PAST_UNTIL = 2_000


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _windowed(
    logical_id: str,
    body: str,
    valid_from: int | None,
    valid_until: int | None,
) -> dict:
    item: dict = {"kind": "doc", "body": body, "logical_id": logical_id, "source_id": _SOURCE_ID}
    if valid_from is not None:
        item["valid_from"] = valid_from
    if valid_until is not None:
        item["valid_until"] = valid_until
    return item


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


def _raw_indexed_bodies(path: str) -> list[str]:
    """The bodies present in the FTS projection at rest.

    Without this the leak tests could pass because nothing was ever searchable.
    """

    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        return sorted(row[0] for row in conn.execute("SELECT body FROM search_index"))
    finally:
        conn.close()


def _bodies(result: SearchResult) -> list[str]:
    return sorted(hit.body for hit in result.results)


# ---------------------------------------------------------------------------
# (1) The leak, both directions, with its control
# ---------------------------------------------------------------------------


def test_expired_window_node_does_not_leak_through_default_search(db_path: str) -> None:
    """A node whose ``valid_until`` is in the PAST must not surface from a
    default ``search``, while an unbounded sibling matching the same query must
    (so this test cannot pass by matching nothing)."""

    _seed(
        db_path,
        [
            _windowed("EXPIRED", "quarterly telemetry report", None, _FAR_PAST_UNTIL),
            _windowed("ALWAYS", "quarterly telemetry summary", None, None),
        ],
    )

    # Data-at-rest oracle: the window landed, and BOTH bodies reached FTS — so a
    # later absence is a filtering decision, not a missing index row.
    assert _raw_window(db_path, "EXPIRED") == (None, _FAR_PAST_UNTIL)
    assert _raw_window(db_path, "ALWAYS") == (None, None)
    indexed = _raw_indexed_bodies(db_path)
    assert any("telemetry report" in b for b in indexed), (
        f"expired node must be in search_index (else this test is vacuous): {indexed!r}"
    )
    assert any("telemetry summary" in b for b in indexed)

    engine = _open(db_path)
    try:
        assert _bodies(engine.search("telemetry")) == ["quarterly telemetry summary"]
    finally:
        engine.close()


def test_future_window_node_does_not_leak_through_default_search(db_path: str) -> None:
    """The mirror case: a window that has not OPENED yet is equally hidden."""

    _seed(
        db_path,
        [
            _windowed("PENDING", "embargoed launch memo", _FAR_FUTURE, None),
            _windowed("ALWAYS", "published launch note", None, None),
        ],
    )

    assert _raw_window(db_path, "PENDING") == (_FAR_FUTURE, None)
    assert any("embargoed launch memo" in b for b in _raw_indexed_bodies(db_path))

    engine = _open(db_path)
    try:
        assert _bodies(engine.search("launch")) == ["published launch note"]
    finally:
        engine.close()


def test_covering_window_node_is_still_returned(db_path: str) -> None:
    """The control: a window that COVERS the current instant is returned, so the
    fix cannot pass by hiding every windowed node unconditionally."""

    _seed(db_path, [_windowed("COVERING", "in force policy text", 1_000, _FAR_FUTURE)])

    engine = _open(db_path)
    try:
        assert _bodies(engine.search("policy")) == ["in force policy text"]
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (2) The no-regression guard
# ---------------------------------------------------------------------------


def test_default_search_unchanged_on_a_corpus_with_no_authored_windows(db_path: str) -> None:
    """The corpus that never authored a window must search IDENTICALLY.

    Every pre-existing row carries NULL/NULL (schema step 22 back-filled NULL
    with no DEFAULT) and the validity predicate treats NULL as unbounded, so the
    new conjunct is provably a no-op here. The NULL/NULL premise is ASSERTED on
    the raw table rather than assumed.
    """

    _seed(
        db_path,
        [
            _windowed("A", "alpha retrieval corpus", None, None),
            _windowed("B", "beta retrieval corpus", None, None),
            _windowed("C", "gamma retrieval corpus", None, None),
        ],
    )

    for logical_id in ("A", "B", "C"):
        assert _raw_window(db_path, logical_id) == (None, None)

    engine = _open(db_path)
    try:
        expected = [
            "alpha retrieval corpus",
            "beta retrieval corpus",
            "gamma retrieval corpus",
        ]
        assert _bodies(engine.search("retrieval")) == expected
        # Inert under a pinned instant on both sides, and under the relaxed view
        # — a NULL/NULL row is valid at EVERY instant.
        assert _bodies(engine.search("retrieval", view=ReadView(valid_as_of=1))) == expected
        assert (
            _bodies(engine.search("retrieval", view=ReadView(valid_as_of=_FAR_FUTURE))) == expected
        )
        assert (
            _bodies(engine.search("retrieval", view=ReadView(include_out_of_window=True)))
            == expected
        )
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (3) The escape hatch
# ---------------------------------------------------------------------------


def test_search_valid_as_of_selects_by_instant(db_path: str) -> None:
    """``valid_as_of`` selects by instant and ``include_out_of_window`` relaxes
    the window entirely — both through the BOUND ``:now`` seam."""

    _seed(
        db_path,
        [
            _windowed("EARLY", "epoch alpha record", 1_000, 2_000),
            _windowed("LATE", "epoch beta record", 3_000, None),
        ],
    )

    engine = _open(db_path)
    try:
        # Default view: real now is past both bounds, so only LATE is valid.
        assert _bodies(engine.search("epoch")) == ["epoch beta record"]

        def at(instant: int) -> list[str]:
            return _bodies(engine.search("epoch", view=ReadView(valid_as_of=instant)))

        assert at(1_500) == ["epoch alpha record"]
        # Half-open: `== valid_until` is OUT, `== valid_from` is IN.
        assert at(2_000) == []
        assert at(3_000) == ["epoch beta record"]
        # Between the two windows — neither is valid.
        assert at(2_500) == []

        assert _bodies(engine.search("epoch", view=ReadView(include_out_of_window=True))) == [
            "epoch alpha record",
            "epoch beta record",
        ]
    finally:
        engine.close()


def test_search_text_only_takes_the_same_predicate_and_escape_hatch(db_path: str) -> None:
    _seed(
        db_path,
        [
            _windowed("EXPIRED", "retired runbook entry", None, _FAR_PAST_UNTIL),
            _windowed("ALWAYS", "current runbook entry", None, None),
        ],
    )

    engine = _open(db_path)
    try:
        assert _bodies(engine.search_text_only("runbook")) == ["current runbook entry"]
        assert _bodies(
            engine.search_text_only("runbook", view=ReadView(include_out_of_window=True))
        ) == ["current runbook entry", "retired runbook entry"]
    finally:
        engine.close()


# ---------------------------------------------------------------------------
# (4) Scope guard — the existence axis is REFUSED, never silently ignored
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "view",
    [ReadView(include_superseded=True), ReadView(include_inactive=True)],
    ids=["include_superseded", "include_inactive"],
)
def test_search_refuses_a_view_that_relaxes_the_existence_axis(
    db_path: str, view: ReadView
) -> None:
    """fix-2 scopes the search ``ReadView`` to the VALIDITY axis. Relaxing
    ``superseded_at IS NULL`` on a retrieval path would resurrect the stale-body
    leak the Slice-15 fix-1 review closed, and search hydrates from projection
    indexes that are not version-complete — so the existence flags have no
    truthful answer here. They are REFUSED, not silently ignored (which would be
    the dead surface this fix exists to remove)."""

    _seed(db_path, [_windowed("A", "scope guard body", None, None)])

    engine = _open(db_path)
    try:
        with pytest.raises(InvalidArgumentError):
            engine.search("scope", view=view)
        # The validity axis alone is accepted.
        assert _bodies(engine.search("scope", view=ReadView(include_out_of_window=True))) == [
            "scope guard body"
        ]
    finally:
        engine.close()


def test_search_rejects_a_non_read_view_argument(db_path: str) -> None:
    """A wrong-typed ``view=`` is a ``TypeError`` at the Python boundary, matching
    the other binding-side guards (``rerank_depth``, ``explain``, α/pool_n)."""

    _seed(db_path, [_windowed("A", "type guard body", None, None)])

    engine = _open(db_path)
    try:
        with pytest.raises(TypeError):
            engine.search("type", view={"include_out_of_window": True})  # type: ignore[arg-type]
        with pytest.raises(TypeError):
            engine.search_text_only("type", view=42)  # type: ignore[arg-type]
    finally:
        engine.close()
