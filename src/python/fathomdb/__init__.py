"""FathomDB Python SDK public surface.

Top-level five-verb surface and exception hierarchy owned by
`dev/interfaces/python.md` + `dev/design/errors.md`. The package delegates
to the native PyO3 extension `fathomdb._fathomdb` which binds to
`fathomdb-engine`.
"""

from __future__ import annotations

from fathomdb import _fathomdb as _native  # noqa: F401 — load native extension
from fathomdb import admin, errors
from fathomdb.config import EngineConfig
from fathomdb.engine import Engine
from fathomdb.types import (
    CounterSnapshot,
    SearchFilter,
    SearchHit,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
)

__all__ = [
    "CounterSnapshot",
    "Engine",
    "EngineConfig",
    "SearchFilter",
    "SearchHit",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "WriteReceipt",
    "__version__",
    "admin",
    "errors",
]
__version__ = "0.6.0"
