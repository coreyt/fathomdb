"""X1 SDK parity — 0.8.20 Slice 22 (R-20-VC / **TC-67**): declaring a
``searchable→vector`` projection over a kind the vector writer can never commit
must REPORT, not fall silent.

Drives ``ProjectionDelta.vector_unsupported_kinds`` through the PyO3 binding by
EXECUTION, not symbol presence. Mirrors
``src/rust/crates/fathomdb-engine/tests/tc67_unsupported_vector_kind_report.rs``
and the TS suite ``src/ts/tests/tc67-unsupported-vector-kind-report.test.ts``
(Py ≡ TS, R-X-1).

**The silence.** The engine maps a node ``kind`` onto a locked ``source_type``
partition-key vocabulary before it can commit a vector, and ``write`` accepts any
non-empty ``kind``. Slice 20c restricted enrolment to that vocabulary (enrolling
anything else wedges the projection worker forever) — but the exclusion was
silent: the declaration persists, its name lands in ``deferred``, and the caller
could not tell "waiting on the embedder" (transient) from "this kind will NEVER
be embedded" (permanent). ``vector_unsupported_kinds`` is that missing fact.

**Consumer meaning.** Rows of a reported kind still get FTS and lexical search;
they will simply never get vectors — in this or any future session.

**No embedder is needed for most of this suite, and that is the point.** The
report is a static property of the locked vocabulary, so it is IDENTICAL with and
without a live embedder. Only the readiness arm needs a real embedder and it
honours the standing ``FATHOMDB_SKIP_NETWORK_TESTS`` guard.

``sqlite3`` is used only as a READ oracle on a CLOSED database — the "still not
enrolled" assertion behind the report.

ZERO net-new governed commands: this rides the already-governed
``Engine.configure_projections`` / ``read.projections`` verbs plus the shipped
``Engine.drain`` instrumentation method.
"""

from __future__ import annotations

import os
import sqlite3

import pytest

from fathomdb import Engine, ProjectionRole, ProjectionSpec, read

_SOURCE_ID = "py-test:tc67"
_WEDGE_TIMEOUT_S = 30.0

# `doc` is coerced to the `article` partition key, so it IS commit-able.
# `invoice` is the established non-commit-able fixture kind; `entity` is the
# concrete consumer case (Memex entity kinds sit outside the locked vocabulary).
_SUPPORTED_KIND = "doc"
_UNSUPPORTED_KINDS = ["entity", "invoice"]


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping default-embedder test")


def _node(kind: str, logical_id: str, body_json: str) -> dict:
    return {"kind": kind, "body": body_json, "logical_id": logical_id, "source_id": _SOURCE_ID}


def _vector_spec(name: str = "summary") -> ProjectionSpec:
    """The real dense arm: ``searchable`` + a ``vector`` sub-object. Only this
    shape puts anything on the dense arm, so only this shape can report."""

    return ProjectionSpec(name=name, roles=frozenset({ProjectionRole.SEARCHABLE}), vector=True)


def _filterable_spec(name: str = "summary") -> ProjectionSpec:
    return ProjectionSpec(name=name, roles=frozenset({ProjectionRole.FILTERABLE}), vector=False)


def _write_mixed_corpus(engine: Engine) -> None:
    """One commit-able kind and two that are permanently outside the vocabulary,
    written out of alphabetical order, each unsupported kind written TWICE — so
    "sorted and de-duplicated" is falsifiable rather than accidental."""

    engine.write(
        [
            _node("invoice", "I1", '{"summary":"payable in 30 days"}'),
            _node("doc", "N1", '{"summary":"a dense meaning"}'),
            _node("entity", "E1", '{"summary":"Alice, a person"}'),
            _node("entity", "E2", '{"summary":"Bob, a person"}'),
            _node("invoice", "I2", '{"summary":"paid"}'),
        ]
    )


def _vector_kind_registered(path: str, kind: str) -> bool:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        row = conn.execute(
            "SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?", (kind,)
        ).fetchone()
        return row[0] > 0
    finally:
        conn.close()


def _readiness(engine: Engine, name: str = "summary") -> str | None:
    for spec in read.projections(engine):
        if spec.name == name:
            return spec.vector_dense_readiness
    return None


def test_an_uncommittable_kind_is_reported_by_kind_not_silently_dropped(tmp_path) -> None:
    """THE defect. The excluded kinds are named, sorted and de-duplicated, and
    the commit-able kind is NOT among them.

    Also pins the two axes side by side: ``deferred`` carries the projection
    ATTRIBUTE NAME (the transient "not built yet" fact), while
    ``vector_unsupported_kinds`` carries node KINDS (the permanent one). Reading
    a kind out of ``deferred`` — or an attribute name out of the report — is a
    category error, so the suite asserts both memberships explicitly.
    """

    path = str(tmp_path / "tc67_report.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        _write_mixed_corpus(engine)
        delta = engine.configure_projections([_vector_spec()])

        assert delta.vector_unsupported_kinds == _UNSUPPORTED_KINDS, (
            "TC-67: a kind the vector writer can never commit must be REPORTED, by KIND, "
            "sorted and de-duplicated — not dropped in silence"
        )
        assert _SUPPORTED_KIND not in delta.vector_unsupported_kinds, (
            "the report must not name a kind the vector writer CAN commit"
        )
        assert delta.deferred == ["summary"], (
            "the two axes are separate: `deferred` still carries the projection ATTRIBUTE NAME"
        )
        assert "summary" not in delta.vector_unsupported_kinds
    finally:
        engine.close()

    # The report does NOT lift the exclusion — enrolling these kinds would wedge
    # the projection worker forever (Slice 20c). TC-67 changes what the engine
    # SAYS, not what it does.
    for kind in _UNSUPPORTED_KINDS:
        assert not _vector_kind_registered(path, kind), (
            f"TC-67 REPORTS the exclusion for `{kind}`; it must not lift it"
        )


def test_a_corpus_of_only_supported_kinds_reports_an_empty_list_not_absent(tmp_path) -> None:
    """Empty, never absent — the field must be readable unconditionally."""

    path = str(tmp_path / "tc67_empty.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        engine.write(
            [
                _node("doc", "N1", '{"summary":"a dense meaning"}'),
                _node("note", "N2", '{"summary":"a second supported kind"}'),
            ]
        )
        delta = engine.configure_projections([_vector_spec()])
        assert delta.vector_unsupported_kinds == [], (
            "with every kind commit-able the report is EMPTY — present and readable, never absent"
        )
        assert isinstance(delta.vector_unsupported_kinds, list)
    finally:
        engine.close()


def test_the_report_is_state_not_diff_so_an_idempotent_reapply_still_carries_it(
    tmp_path,
) -> None:
    """The other three lists describe what THIS call changed; this one describes
    the corpus as it stands, so a no-op re-apply still carries it.

    That is also the RESIDUAL's documented refresh path: the report is computed
    at DECLARE time, so a kind written LATER is absent from a delta the caller
    already holds — re-applying the same spec (a no-op) returns a current one.
    """

    path = str(tmp_path / "tc67_state.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        engine.write([_node("invoice", "I1", '{"summary":"payable in 30 days"}')])
        first = engine.configure_projections([_vector_spec()])
        assert first.vector_unsupported_kinds == ["invoice"]

        again = engine.configure_projections([_vector_spec()])
        assert again.unchanged, "re-registering the same spec is still a no-op"
        assert again.built == [] and again.dropped == [] and again.deferred == []
        assert again.vector_unsupported_kinds == ["invoice"], (
            "a STATE report, not a diff: `unchanged=True` must not suppress it"
        )

        # The residual, made concrete, and its refresh.
        engine.write([_node("entity", "E1", '{"summary":"Alice, a person"}')])
        assert first.vector_unsupported_kinds == ["invoice"], (
            "RESIDUAL: the delta already held is a snapshot; it never learns about `entity`"
        )
        refreshed = engine.configure_projections([_vector_spec()])
        assert refreshed.unchanged, "the refresh costs nothing — still a no-op"
        assert refreshed.vector_unsupported_kinds == _UNSUPPORTED_KINDS, (
            "…and the no-op re-apply reports the corpus as it stands NOW"
        )
    finally:
        engine.close()


def test_no_vector_declaration_means_no_report(tmp_path) -> None:
    """Scoped to the dense arm: with no ``searchable→vector`` declaration there is
    nothing for a kind to be unsupported FOR, so the report stays empty rather
    than becoming noise on every non-vector call."""

    path = str(tmp_path / "tc67_scoped.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        _write_mixed_corpus(engine)
        delta = engine.configure_projections([_filterable_spec()])
        assert delta.vector_unsupported_kinds == []

        declared = engine.configure_projections([_vector_spec("meaning")])
        assert declared.vector_unsupported_kinds == _UNSUPPORTED_KINDS, "fixture"

        dropped = engine.configure_projections([], ["meaning"])
        assert "meaning" in dropped.dropped, "fixture: the drop is reported"
        assert dropped.vector_unsupported_kinds == [], (
            "once the last `searchable→vector` declaration is gone the report goes quiet with it"
        )
    finally:
        engine.close()


def test_the_read_configure_round_trip_still_holds(tmp_path) -> None:
    """``read.projections`` output must still feed straight back into
    ``configure_projections`` as a no-op. It cannot break, structurally: the new
    field lives on the DELTA and ``configure_projections`` accepts specs, never a
    delta — it is OUTPUT-ONLY. Stated explicitly rather than left to inference."""

    path = str(tmp_path / "tc67_round_trip.sqlite")
    engine = Engine.open(path, use_default_embedder=False)
    try:
        _write_mixed_corpus(engine)
        engine.configure_projections([_vector_spec()])

        back = read.projections(engine)
        assert len(back) == 1
        round_tripped = engine.configure_projections(back)
        assert round_tripped.unchanged, "the shipped read→configure round-trip is still a no-op"
        assert round_tripped.vector_unsupported_kinds == _UNSUPPORTED_KINDS, (
            "…and the no-op still carries the report"
        )
    finally:
        engine.close()


def test_readiness_semantics_are_unchanged_by_the_report(tmp_path) -> None:
    """THE DoD clause most likely to regress. An un-enrolled kind is NOT
    outstanding work — nothing will ever be embedded for it, so there is nothing
    to wait for. A corpus made ENTIRELY of unsupported kinds must still reach
    ``vector_dense_readiness == "ready"`` and ``drain`` must still return.

    This is the arm that needs a LIVE embedder: without one every dense-arm path
    is short-circuited and the assertion would pass vacuously.
    """

    _skip_if_no_network()
    path = str(tmp_path / "tc67_readiness.sqlite")
    engine = Engine.open(path, use_default_embedder=True)
    try:
        engine.write(
            [
                _node("invoice", "I1", '{"summary":"payable in 30 days"}'),
                _node("entity", "E1", '{"summary":"Alice, a person"}'),
            ]
        )
        engine.drain(timeout_s=_WEDGE_TIMEOUT_S)

        delta = engine.configure_projections([_vector_spec()])
        assert delta.vector_unsupported_kinds == _UNSUPPORTED_KINDS, (
            "the report is IDENTICAL with a live embedder — it is a static property of the "
            "locked vocabulary, not a session fact"
        )

        engine.drain(timeout_s=_WEDGE_TIMEOUT_S)
        assert _readiness(engine) == "ready", (
            "READINESS SEMANTICS ARE UNCHANGED: an un-enrolled kind is not outstanding work, so "
            "a corpus made entirely of unsupported kinds is `ready`, not `embedding`"
        )

        engine.write([_node("entity", "E2", '{"summary":"Bob, a person"}')])
        engine.drain(timeout_s=_WEDGE_TIMEOUT_S)
        assert _readiness(engine) == "ready", (
            "writing MORE rows of an unsupported kind still leaves nothing outstanding"
        )
    finally:
        engine.close()
