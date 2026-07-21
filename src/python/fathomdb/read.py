"""The governed ``read.*`` namespace (Slice 30 — G2 / G3; Slice 35 — G4).

Per ``dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md`` (B1 ``read.*``), this
module exposes the governed read verbs beside ``admin``:

* ``read.get`` / ``read.get_many`` — active-only point lookup by ``logical_id``
  (active = ``superseded_at IS NULL``). Not-found is a normal ``None`` (a typed
  ``NotFound`` class is reserved for a later slice), never an exception.
* ``read.collection`` / ``read.mutations`` — paginated op-store read-back over
  ``operational_mutations`` with a MANDATORY ``limit`` + ``after_id`` cursor.
* ``read.list`` (G4 / Slice 35) — list active ``canonical_nodes`` of a given
  ``kind``, optionally filtered by a list of ``Predicate`` dicts (AND-combined),
  up to ``limit`` rows. Compiles to parameterized ``json_extract`` over the
  allowlisted path set (injection-safe per ADR D-F4).

The native binding (``fathomdb._fathomdb``) performs the ReaderWorkerPool
DEFERRED-tx read; this module exposes the typed Python signatures and converts
native rows to the public dataclasses in ``fathomdb.types``. Reads NEVER take
the writer lock.
"""

from __future__ import annotations

import builtins
from typing import TYPE_CHECKING, Any

from fathomdb._fathomdb import NodeRecord as _NativeNodeRecord
from fathomdb._fathomdb import OpStoreRow as _NativeOpStoreRow
from fathomdb._fathomdb import read_collection as _native_collection
from fathomdb._fathomdb import read_get as _native_get
from fathomdb._fathomdb import read_get_many as _native_get_many
from fathomdb._fathomdb import read_list as _native_list
from fathomdb._fathomdb import read_list_filter as _native_list_filter
from fathomdb._fathomdb import read_mutations as _native_mutations
from fathomdb._fathomdb import crossed_boundary_since as _native_crossed_boundary_since
from fathomdb._fathomdb import read_projections as _native_read_projections
from fathomdb._fathomdb import ReadView as _NativeReadView
from fathomdb._fathomdb import BoundaryCrossing as _NativeBoundaryCrossing
from fathomdb.types import BoundaryCrossing, NodeRecord, OpStoreRow, ProjectionSpec, ReadView

if TYPE_CHECKING:
    from fathomdb.engine import Engine
    from fathomdb.filter import Filter


def _to_node_record(native: _NativeNodeRecord) -> NodeRecord:
    return NodeRecord(
        logical_id=native.logical_id,
        kind=native.kind,
        body=native.body,
        write_cursor=native.write_cursor,
    )


def _to_native_view(view: ReadView | None) -> _NativeReadView | None:
    """Translate the public dataclass to the native ``ReadView``.

    ``None`` stays ``None`` — the native layer then applies the strict default
    view, so an omitted ``view=`` is exactly the shipped behaviour.
    """

    if view is None:
        return None
    return _NativeReadView(
        include_superseded=view.include_superseded,
        include_inactive=view.include_inactive,
        include_out_of_window=view.include_out_of_window,
        valid_as_of=view.valid_as_of,
    )


def _to_boundary_crossing(native: _NativeBoundaryCrossing) -> BoundaryCrossing:
    return BoundaryCrossing(
        node=_to_node_record(native.node),
        became_valid_at=native.became_valid_at,
        became_invalid_at=native.became_invalid_at,
    )


def _to_op_store_row(native: _NativeOpStoreRow) -> OpStoreRow:
    return OpStoreRow(
        id=native.id,
        collection=native.collection,
        record_key=native.record_key,
        op_kind=native.op_kind,
        payload=native.payload,
        schema_id=native.schema_id,
        write_cursor=native.write_cursor,
    )


def get(engine: "Engine", logical_id: str, *, view: ReadView | None = None) -> NodeRecord | None:
    """Return the ACTIVE node carrying ``logical_id``, or ``None`` if absent.

    Active-only (``superseded_at IS NULL``): a superseded version is never
    returned. A missing/superseded id is a normal ``None``, not an exception.
    """

    if not logical_id:
        raise ValueError("read.get requires a non-empty logical_id")
    native = _native_get(engine._native, logical_id, _to_native_view(view))
    return _to_node_record(native) if native is not None else None


def get_many(
    engine: "Engine",
    logical_ids: builtins.list[str],
    *,
    view: ReadView | None = None,
) -> builtins.list[NodeRecord | None]:
    """Return one slot per requested id, in REQUEST ORDER.

    A missing/superseded id yields ``None`` in its slot (partial result, never
    all-or-nothing). Order is preserved 1:1 with ``logical_ids``.
    """

    natives = _native_get_many(engine._native, builtins.list(logical_ids), _to_native_view(view))
    return [_to_node_record(n) if n is not None else None for n in natives]


def collection(
    engine: "Engine",
    collection: str,
    *,
    after_id: int | None = None,
    limit: int,
) -> builtins.list[OpStoreRow]:
    """Paginated op-store read-back over ``operational_mutations``, ``ORDER BY id``.

    ``limit`` is MANDATORY (the engine clamps it to a ~1M cap, so no call yields
    an unbounded read); ``after_id`` is the exclusive cursor for the next page.
    """

    _validate_limit(limit)
    return [
        _to_op_store_row(row)
        for row in _native_collection(engine._native, collection, after_id, limit)
    ]


def mutations(
    engine: "Engine",
    collection: str,
    *,
    after_id: int | None = None,
    limit: int,
) -> builtins.list[OpStoreRow]:
    """Mutation-log-oriented alias surface over the same op-store read-back as
    :func:`collection` (identical args + semantics)."""

    _validate_limit(limit)
    return [
        _to_op_store_row(row)
        for row in _native_mutations(engine._native, collection, after_id, limit)
    ]


def list(  # noqa: A001 — shadows builtin; public API requires this name
    engine: "Engine",
    kind: str,
    predicates: builtins.list[builtins.dict[str, Any]] | None = None,
    *,
    limit: int = 100,
    filter: "Filter | None" = None,
    view: ReadView | None = None,
) -> builtins.list[NodeRecord]:
    """G4 (Slice 35) — list active ``canonical_nodes`` of the given ``kind``.

    ``predicates`` is an optional list of filter dicts (AND-combined). Each dict:
    ``{"type": "eq"|"gt"|"gte"|"lt"|"lte", "path": str, "value": str|int|bool}``.
    The ``path`` must be from the engine's allowlist (``$.status``, ``$.priority``,
    ``$.tags``, ``$.kind``, ``$.created_at``); non-allowlisted paths raise
    ``InvalidFilterError``. Values are always bound as parameterized SQL — never
    interpolated (injection-safe per ADR D-F4).

    Empty ``predicates`` (or ``None``) returns all active nodes of the kind up to
    ``limit`` (unfiltered path). ``limit`` defaults to 100.

    0.8.11 Slice 40 (#17): pass ``filter=`` (a unified :class:`fathomdb.Filter`)
    for the additive unified grammar instead of ``predicates``. The engine then
    performs the authoritative total dispatch (``Json`` → allowlisted
    ``json_extract``; ``Status``/``CreatedAfter`` → allowlisted json-paths;
    ``Kind``/``SourceType`` constant-fold vs the partition ``kind`` — a
    contradicting fold returns ``[]`` without touching SQL). ``predicates`` and
    ``filter`` are mutually exclusive. This stays the **same** governed
    ``read.list`` verb (no new surface member).
    """

    if limit < 0:
        raise ValueError("read.list limit must be non-negative")
    if filter is not None:
        if predicates:
            raise ValueError("read.list: pass either `predicates` or `filter`, not both")
        terms = filter.to_native_terms()
        rows = _native_list_filter(engine._native, kind, terms or None, limit, _to_native_view(view))
    else:
        rows = _native_list(engine._native, kind, predicates or None, limit, _to_native_view(view))
    return [_to_node_record(row) for row in rows]


def crossed_boundary_since(
    engine: "Engine",
    since: int,
    *,
    view: ReadView | None = None,
) -> builtins.list[BoundaryCrossing]:
    """R-20-NV — nodes that crossed a validity boundary in ``(since, as_of]``.

    ``since`` is an INTEGER epoch-second instant, and the upper bound is the
    view's own ``valid_as_of`` (defaulting to now). Both are bound parameters,
    so the answer is deterministic for a fixed pair.

    A node whose window opened AND closed inside the interval reports both
    boundaries. Rows with no window (every row predating schema step 22) can
    never cross one, so they never appear.

    World-time only — this asks what was true in the world, never what the
    database believed.
    """

    if not isinstance(since, int) or isinstance(since, bool):
        raise ValueError("read.crossed_boundary_since requires an integer `since`")
    rows = _native_crossed_boundary_since(engine._native, since, _to_native_view(view))
    return [_to_boundary_crossing(row) for row in rows]


def projections(engine: "Engine") -> builtins.list[ProjectionSpec]:
    """0.8.20 Slice 15d (R-20-PR) — ``read.projections`` introspection.

    Returns every declared :class:`ProjectionSpec` (sorted by name), so a caller
    can inspect current registry state — and the destructive delta a change would
    cause — BEFORE calling ``Engine.configure_projections``. Pure read.
    """

    return [
        ProjectionSpec(
            name=s.name,
            roles=frozenset(s.roles),
            fts=s.fts,
            fts_tokenizer=s.fts_tokenizer,
            vector=s.vector,
            vector_embedder=s.vector_embedder,
        )
        for s in _native_read_projections(engine._native)
    ]


def _validate_limit(limit: int) -> None:
    if not isinstance(limit, int) or isinstance(limit, bool):
        raise ValueError("read.collection/read.mutations require an integer limit")
    if limit < 0:
        raise ValueError("read.collection/read.mutations limit must be non-negative")


__all__ = [
    "get",
    "get_many",
    "collection",
    "mutations",
    "list",
    "crossed_boundary_since",
    "projections",
]
