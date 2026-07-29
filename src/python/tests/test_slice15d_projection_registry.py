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

import dataclasses
import sqlite3

import pytest

from fathomdb import Engine, ProjectionRole, ProjectionSpec, read
from fathomdb._fathomdb import ProjectionSpec as _NativeProjectionSpec
from fathomdb._fathomdb import configure_projections as _native_configure_projections
from fathomdb.errors import (
    EngineError,
    InvalidArgumentError,
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


# ---------------------------------------------------------------------------
# 0.8.20 keystone closeout fix-4 — projection-spec binding round-trip
# consistency. A ProjectionSpec the binding ACCEPTS must round-trip through
# `read.projections` IDENTICALLY; a shape that would be silently dropped or
# normalized is refused at the binding boundary with the typed validation error
# (InvalidArgumentError, the same variant the unknown-role rejection uses).
# Mirrors the TS suite one-for-one (Py ≡ TS: both reject the same shapes).
# ---------------------------------------------------------------------------


def test_orphaned_fts_tokenizer_rejected_at_binding(tmp_path) -> None:
    """fix-4 — `fts_tokenizer` supplied while `fts=False` is refused (else the
    tokenizer is silently dropped: configure reports success but
    read.projections cannot round-trip what the caller sent)."""
    path = str(tmp_path / "orphan_tok.sqlite")
    engine = _open(path)
    try:
        spec = ProjectionSpec(
            name="status",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            fts=False,
            fts_tokenizer="unicode61",
        )
        with pytest.raises(InvalidArgumentError) as exc:
            engine.configure_projections([spec])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "a refused spec must persist no registry row"


def test_orphaned_vector_embedder_rejected_at_binding(tmp_path) -> None:
    """fix-4 — `vector_embedder` supplied while `vector=False` is refused."""
    path = str(tmp_path / "orphan_emb.sqlite")
    engine = _open(path)
    try:
        spec = ProjectionSpec(
            name="summary",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            vector=False,
            vector_embedder="bge-small",
        )
        with pytest.raises(InvalidArgumentError) as exc:
            engine.configure_projections([spec])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "a refused spec must persist no registry row"


def test_empty_fts_tokenizer_rejected_at_binding(tmp_path) -> None:
    """fix-4 — an empty `fts_tokenizer` with `fts=True` is refused: the engine
    normalizes `""` to the default, so it reads back as None (non-round-trip)."""
    path = str(tmp_path / "empty_tok.sqlite")
    engine = _open(path)
    try:
        spec = ProjectionSpec(
            name="status",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            fts=True,
            fts_tokenizer="",
        )
        with pytest.raises(InvalidArgumentError) as exc:
            engine.configure_projections([spec])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "a refused spec must persist no registry row"


def test_empty_vector_embedder_rejected_at_binding(tmp_path) -> None:
    """fix-4 — an empty `vector_embedder` with `vector=True` is refused (same
    silent-normalize-to-default non-round-trip as the fts twin)."""
    path = str(tmp_path / "empty_emb.sqlite")
    engine = _open(path)
    try:
        spec = ProjectionSpec(
            name="summary",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            vector=True,
            vector_embedder="",
        )
        with pytest.raises(InvalidArgumentError) as exc:
            engine.configure_projections([spec])
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "a refused spec must persist no registry row"


def test_duplicate_role_rejected_at_binding(tmp_path) -> None:
    """fix-4 — a duplicate role spelling in the flat list is refused. The SDK
    dataclass uses a `frozenset` (which dedups), so this exercises the NATIVE
    binding boundary directly, where the flat `roles: list[str]` COULD carry a
    duplicate that the registry's de-duplicated BTreeSet cannot round-trip."""
    path = str(tmp_path / "dup_role.sqlite")
    engine = _open(path)
    try:
        native = _NativeProjectionSpec(
            "status",
            ["searchable", "searchable"],
            False,
            None,
            False,
            None,
        )
        with pytest.raises(InvalidArgumentError) as exc:
            _native_configure_projections(engine._native, [native], None)
        assert isinstance(exc.value, EngineError)
    finally:
        engine.close()
    assert _registry_names(path) == [], "a refused spec must persist no registry row"


def test_consistent_fts_spec_round_trips(tmp_path) -> None:
    """fix-4 non-vacuous CONTROL — a CONSISTENT spec (`fts=True` WITH a real
    custom tokenizer) is ACCEPTED and, read back via `read.projections`, equals
    what was sent. Proves the gate rejects only inconsistent shapes, not every
    tokenizer/embedder."""
    engine = _open(str(tmp_path / "consistent.sqlite"))
    try:
        sent = ProjectionSpec(
            name="status",
            roles=frozenset({ProjectionRole.SEARCHABLE}),
            fts=True,
            fts_tokenizer="unicode61",
            vector=True,
            vector_embedder="bge-small",
        )
        delta = engine.configure_projections([sent])
        assert delta.unchanged is False
        back = read.projections(engine)
        assert len(back) == 1
        got = back[0]
        # The full round-trip invariant: read-back equals what was sent, PLUS the
        # one engine-set READ-METADATA field 0.8.20 Slice 20 (R-20-DR) attached
        # to the vector sub-object. `vector_dense_readiness` is not a
        # declaration, so it is expected to differ from the sent spec (which
        # never authors it); every DECLARED field must still match exactly.
        assert got.vector_dense_readiness == "ready"
        assert dataclasses.replace(got, vector_dense_readiness=None) == sent
        assert got.fts is True and got.fts_tokenizer == "unicode61"
        assert got.vector is True and got.vector_embedder == "bge-small"
    finally:
        engine.close()


def test_fts_or_vector_without_searchable_role_is_rejected(tmp_path) -> None:
    """0.8.20 Slice 23 (``R-20-SV``) — an ``fts`` or ``vector`` sub-object
    declared WITHOUT the ``searchable`` role is an **INVALID SPEC** and is
    REJECTED with ``WriteValidationError``.

    **This OVERTURNS the shipped 15d fix-4 position** that the same shape is
    accepted and round-trips faithfully (this test asserted exactly that until
    this slice). HITL ruling 2026-07-24, ``dev/plans/plan-0.8.20.md`` §11 item 4,
    option **(b) REJECT**: the sub-objects SELECT a sub-target of ``searchable``
    and do not confer it, so without the role the declaration is a meaningless
    config that builds, embeds and enrols nothing.

    The error family is ``WriteValidationError`` per decision #18 (Slice 22): a
    malformed write SHAPE is one family. It is message-less by construction, so
    the refusal does not name which spec in the list was invalid (TC-95/TC-98,
    deferred by the HITL).

    ``read.projections`` is a pure read and is UNAFFECTED — a legacy row in this
    shape is still reported verbatim; it simply can no longer be re-applied.
    """
    engine = _open(str(tmp_path / "fts_no_searchable.sqlite"))
    try:
        for fts, vector in ((True, False), (False, True), (True, True)):
            with pytest.raises(WriteValidationError):
                engine.configure_projections(
                    [
                        ProjectionSpec(
                            name="status",
                            roles=frozenset({ProjectionRole.FILTERABLE}),
                            fts=fts,
                            vector=vector,
                        )
                    ]
                )
        # `rankable` does not supply the role either — the reject is keyed on the
        # ABSENCE of `searchable`, not on which other roles are present.
        with pytest.raises(WriteValidationError):
            engine.configure_projections(
                [
                    ProjectionSpec(
                        name="status",
                        roles=frozenset({ProjectionRole.RANKABLE}),
                        fts=False,
                        vector=True,
                    )
                ]
            )
        # Nothing was persisted by any refused call.
        assert read.projections(engine) == []

        # CONTROL — with `searchable` present the identical sub-objects are a
        # VALID spec and still round-trip.
        sent = ProjectionSpec(
            name="status",
            roles=frozenset({ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}),
            fts=True,
            vector=True,
        )
        engine.configure_projections([sent])
        got = next(s for s in read.projections(engine) if s.name == "status")
        # 0.8.20 Slice 20 (R-20-DR) — modulo the engine-set readiness read
        # metadata, which is not a declaration and is never authored by a caller.
        assert dataclasses.replace(got, vector_dense_readiness=None) == sent
        assert got.fts is True and got.vector is True
    finally:
        engine.close()


def test_read_projections_output_round_trips_back_into_configure(tmp_path) -> None:
    """fix-4 — the read→configure round-trip: `read.projections` output fed
    straight back into `configure_projections` must re-apply as an idempotent
    no-op (the sub-field `None` must be accepted). pyo3 accepts `None` natively
    (this is GREEN at RED for Python); the napi twin needed a `null → None`
    normalization to match — the Py ≡ TS parity anchor for that fix."""
    engine = _open(str(tmp_path / "read_configure_rt.sqlite"))
    try:
        engine.configure_projections(
            [_spec("status", {ProjectionRole.FILTERABLE, ProjectionRole.SEARCHABLE}, fts=True, vector=True)]
        )
        read_back = read.projections(engine)
        assert len(read_back) == 1
        assert read_back[0].fts_tokenizer is None, "read output carries a None sub-field"
        # Re-applying the read output verbatim is a no-op — proves None round-trips.
        again = engine.configure_projections(list(read_back))
        assert again.unchanged is True, "read.projections output must re-apply as a no-op"
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
