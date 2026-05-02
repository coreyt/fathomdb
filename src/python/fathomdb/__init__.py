"""FathomDB Python SDK public surface.

Top-level five-verb surface and exception hierarchy owned by
`dev/interfaces/python.md` + `dev/design/errors.md`. PyO3 wiring lands in a
follow-up slice; the 0.6.0 surface-stub keeps the engine pure-Python so
parser and exception tests can run before native code exists.
"""

from __future__ import annotations

from fathomdb import admin, errors
from fathomdb.config import EngineConfig
from fathomdb.engine import Engine
from fathomdb.types import (
    CounterSnapshot,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
)

__all__ = [
    "CounterSnapshot",
    "Engine",
    "EngineConfig",
    "SearchResult",
    "SoftFallback",
    "SoftFallbackBranch",
    "WriteReceipt",
    "__version__",
    "admin",
    "errors",
]
__version__ = "0.6.0"
