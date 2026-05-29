"""Pyright narrowing fixture — fed to pyright as a subprocess by
``test_pyright_narrowing.py``. Not executed at test time.

The file uses ``reveal_type`` (a pyright analysis-time directive; no-op /
shim at runtime). Each ``reveal_type`` call asserts that, inside the
narrowed branch of ``if event["kind"] == "..."``, pyright resolves the
variant payload key to its concrete type (e.g. ``int``), not the wider
union or ``Unknown``.

See ``dev/design/0.7.1-EU-6-FIX-2-design.md`` §5.1.
"""

from __future__ import annotations

# `reveal_type` is stdlib on 3.11+. On 3.10 it lives in `typing_extensions`.
# At runtime this fixture is never executed; pyright reads it statically and
# resolves the import via its own type stubs, so the import-error path is
# analysis-only.
from typing import reveal_type  # type: ignore[attr-defined]

from fathomdb.types import EmbedderEvent


def consume(events: list[EmbedderEvent]) -> None:
    for event in events:
        # Negative: without narrowing, accessing a variant-specific key
        # on the raw `EmbedderEvent` union must be a type error
        # (TypedDict key not present in every union member).
        _unsafe = event["bytes"]  # pyright: error
        del _unsafe

        if event["kind"] == "DefaultEmbedderDownload":
            reveal_type(event["file"])         # expect: str
            reveal_type(event["url"])          # expect: str
            reveal_type(event["bytes"])        # expect: int
            reveal_type(event["sha256"])       # expect: str
            reveal_type(event["cache_path"])   # expect: str
            reveal_type(event["duration_ms"])  # expect: int
        elif event["kind"] == "DefaultEmbedderCacheHit":
            reveal_type(event["file"])         # expect: str
            reveal_type(event["sha256"])       # expect: str
            reveal_type(event["cache_path"])   # expect: str
        elif event["kind"] == "MeanVecPinned":
            reveal_type(event["dim"])          # expect: int
            reveal_type(event["doc_count"])    # expect: int
