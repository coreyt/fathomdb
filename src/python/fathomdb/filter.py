"""0.8.11 Slice 40 (#17) — the unified ``Filter`` grammar (G4 + G10).

Mirrors ``fathomdb_engine::Filter`` / ``FilterTerm``
(ADR-0.8.11-filter-grammar-unification, Option A): ONE typed contract dispatched
to TWO compilation backends. The shipped ``SearchFilter`` (G10) and ``Predicate``
dicts (G4) re-express as **sugar** that lowers into this type (D4); the unified
``Filter`` is the surface the 0.8.15 router ``constraints`` block will reason over.

Two backends, two lowerings:

* ``to_search_filter()`` — the vec0 (``engine.search``) backend. Lowers the
  metadata subset to the closed :class:`~fathomdb.types.SearchFilter` and
  **typed-rejects** a :class:`Json` term with ``InvalidFilterError`` (D3: an
  arbitrary json-path predicate is never silently demoted to a post-KNN
  ``json_extract``). The four shorthand variants map to ``SearchFilter`` fields
  with no engine help, so this lowering is pure-SDK (no drift risk).
* ``to_native_terms()`` — the ``read.list`` backend. Produces the term dicts for
  the native ``read_list_filter`` entry; the **engine** performs the authoritative
  total dispatch there (``Json`` → ``json_extract``; ``SourceType``/``Kind``
  constant-fold vs the partition ``kind`` via ``resolve_source_type``), so the
  constant-fold is never re-implemented (and cannot drift) in Python.

Closed grammar (inherits ADR-0.8.0): exactly five term variants; no DSL, no
fused/unchecked builders, implicit AND, values bound as parameters. Cross-binding
parity with the TypeScript ``Filter`` (``surface.test`` / filter suites).
"""

from __future__ import annotations

import builtins
from dataclasses import dataclass, field
from typing import Any, Union

from fathomdb.errors import InvalidFilterError
from fathomdb.types import SearchFilter

# ----- the five closed FilterTerm variants (mirror the Rust enum) -----------


@dataclass(frozen=True)
class SourceType:
    """vec0 partition-key metadata column ``source_type``; on ``read.list`` it
    constant-folds against ``resolve_source_type(kind)`` (engine-side)."""

    value: str


@dataclass(frozen=True)
class Kind:
    """``kind`` — vec0 metadata column; on ``read.list`` it constant-folds
    against the partition ``kind`` argument."""

    value: str


@dataclass(frozen=True)
class CreatedAfter:
    """``created_at >= bound`` (unix seconds)."""

    value: int


@dataclass(frozen=True)
class Status:
    """vec0 metadata column ``status`` / allowlisted ``$.status`` json-path."""

    value: str


@dataclass(frozen=True)
class Json:
    """The G4 general json-path predicate (the shipped closed grammar). Carries
    the shipped ``read.list`` predicate dict
    (``{"type","path","value"}``). Accepted on ``read.list``; **typed-rejected**
    on ``search`` (D3)."""

    predicate: dict[str, Any]


FilterTerm = Union[SourceType, Kind, CreatedAfter, Status, Json]

_VEC0_JSON_REJECT = (
    "arbitrary json-path predicate not supported on search_filtered; it would "
    "require a post-KNN json_extract that defeats the indexed pre-KNN filter "
    "(ADR-0.8.11 D3 no-demotion guarantee)"
)


@dataclass(frozen=True)
class Filter:
    """The unified closed filter contract — implicit-AND :data:`FilterTerm`\\s
    dispatched to one of two backends."""

    terms: tuple[FilterTerm, ...] = field(default_factory=tuple)

    def to_search_filter(self) -> SearchFilter:
        """vec0 (``search``) backend dispatch. Typed-rejects a :class:`Json`
        term (D3 no-demotion guarantee); otherwise returns the closed
        ``SearchFilter`` sugar (canonical-order-independent of term order)."""
        source_type: str | None = None
        kind: str | None = None
        created_after: int | None = None
        status: str | None = None
        for term in self.terms:
            if isinstance(term, SourceType):
                source_type = term.value
            elif isinstance(term, Kind):
                kind = term.value
            elif isinstance(term, CreatedAfter):
                created_after = term.value
            elif isinstance(term, Status):
                status = term.value
            elif isinstance(term, Json):
                raise InvalidFilterError(_VEC0_JSON_REJECT)
            else:  # pragma: no cover - defense in depth
                raise InvalidFilterError(f"unknown filter term: {term!r}")
        return SearchFilter(
            source_type=source_type,
            kind=kind,
            created_after=created_after,
            status=status,
        )

    def to_native_terms(self) -> builtins.list[dict[str, Any]]:
        """``read.list`` backend: term dicts for the native ``read_list_filter``.
        The engine performs the authoritative total dispatch (constant-folds +
        ``json_extract``)."""
        out: builtins.list[dict[str, Any]] = []
        for term in self.terms:
            if isinstance(term, SourceType):
                out.append({"term": "source_type", "value": term.value})
            elif isinstance(term, Kind):
                out.append({"term": "kind", "value": term.value})
            elif isinstance(term, CreatedAfter):
                out.append({"term": "created_after", "value": term.value})
            elif isinstance(term, Status):
                out.append({"term": "status", "value": term.value})
            elif isinstance(term, Json):
                out.append({"term": "json", "predicate": term.predicate})
            else:  # pragma: no cover - defense in depth
                raise InvalidFilterError(f"unknown filter term: {term!r}")
        return out


def from_search_filter(sf: SearchFilter) -> Filter:
    """D4 sugar: re-express a shipped :class:`~fathomdb.types.SearchFilter` as
    the unified :class:`Filter` (canonical field order)."""
    terms: list[FilterTerm] = []
    if sf.source_type is not None:
        terms.append(SourceType(sf.source_type))
    if sf.kind is not None:
        terms.append(Kind(sf.kind))
    if sf.created_after is not None:
        terms.append(CreatedAfter(sf.created_after))
    if sf.status is not None:
        terms.append(Status(sf.status))
    return Filter(tuple(terms))


__all__ = [
    "Filter",
    "FilterTerm",
    "SourceType",
    "Kind",
    "CreatedAfter",
    "Status",
    "Json",
    "from_search_filter",
]
