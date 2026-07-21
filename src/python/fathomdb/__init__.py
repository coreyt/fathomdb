"""FathomDB Python SDK public surface.

Top-level five-verb surface and exception hierarchy owned by
`dev/interfaces/python.md` + `dev/design/errors.md`. The package delegates
to the native PyO3 extension `fathomdb._fathomdb` which binds to
`fathomdb-engine`.
"""

from __future__ import annotations

from fathomdb import _fathomdb as _native  # noqa: F401 — load native extension
from fathomdb import admin, errors, graph, read
from fathomdb._fathomdb import ConsolidateReceipt
from fathomdb._fathomdb import IngestWithExtractorReceipt
from fathomdb._fathomdb import embed_batch_cls
from fathomdb._fathomdb import rerank
from fathomdb.config import EngineConfig
from fathomdb.engine import Engine
from fathomdb.filter import Filter
from fathomdb.types import (
    CounterSnapshot,
    ExpandedNode,
    Explanation,
    IdSpace,
    NodeRecord,
    OpStoreRow,
    PerHitExplain,
    ProjectionDelta,
    ProjectionRole,
    ProjectionSpec,
    QueryTrace,
    SearchExpandResult,
    SearchFilter,
    SearchHit,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
)

__all__ = [
    "ConsolidateReceipt",
    "CounterSnapshot",
    "Engine",
    "EngineConfig",
    "Filter",
    "IngestWithExtractorReceipt",
    "ExpandedNode",
    "Explanation",
    "IdSpace",
    "NodeRecord",
    "OpStoreRow",
    "PerHitExplain",
    "ProjectionDelta",
    "ProjectionRole",
    "ProjectionSpec",
    "QueryTrace",
    "SearchExpandResult",
    "SearchFilter",
    "SearchHit",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "WriteReceipt",
    "__version__",
    "admin",
    "embed_batch_cls",
    "errors",
    "graph",
    "read",
    "rerank",
]
__version__ = "0.6.0"
