"""0.8.16 Slice 5 / F9 (codex §9 fix-2) — public-wrapper propagation of the
per-hit explain ``importance``/``confidence`` fields.

The native binding structs (`fathomdb-py` `PyPerHitExplain`, `fathomdb-napi`
`PerHitExplain`) already carry `importance`/`confidence` (fix-1). This test
guards the *public Python SDK* seam: the `engine.py` mapping that rebuilds
`fathomdb.types.PerHitExplain` from the native per-hit object must copy the two
F9 fields through, symmetric with the TypeScript `perHit` mapping in
`src/ts/src/index.ts`.

Runs as a pure-Python unit test — it does NOT import the compiled `_fathomdb`
extension. `fathomdb/__init__.py` eagerly imports the native module, so a
permissive fake is injected into `sys.modules` *before* `fathomdb.engine` is
imported; the mapping under test (`_map_per_hit_explain`) touches only pure
Python. Run with `FATHOMDB_TESTS_NO_REBUILD=1` so conftest does not attempt a
`maturin develop` rebuild.
"""

from __future__ import annotations

import sys
import types
from types import SimpleNamespace

# --- Isolate from the compiled extension -----------------------------------
# Inject a permissive fake `fathomdb._fathomdb` before `fathomdb` is imported so
# `fathomdb/__init__.py` (which does `from fathomdb._fathomdb import ...`) and
# `fathomdb.engine` load without the built native module.
if "fathomdb" not in sys.modules and "fathomdb._fathomdb" not in sys.modules:
    _fake = types.ModuleType("fathomdb._fathomdb")

    class _Dummy:  # stands in for any native symbol imported by name
        pass

    def _fake_getattr(name: str) -> object:  # PEP 562 module __getattr__
        return _Dummy

    _fake.__getattr__ = _fake_getattr  # type: ignore[attr-defined]
    sys.modules["fathomdb._fathomdb"] = _fake

from fathomdb.engine import _map_per_hit_explain  # noqa: E402
from fathomdb.types import PerHitExplain  # noqa: E402


def _native_per_hit(*, importance: float | None, confidence: float | None) -> SimpleNamespace:
    """A fake native per-hit object carrying the full attribute surface the
    public mapping reads (mirrors `PyPerHitExplain` / N-API `PerHitExplain`)."""
    return SimpleNamespace(
        id=7,
        arm="graph_arm",
        vector_rank=None,
        text_rank=1,
        graph_rank=0,
        fused_score=0.42,
        ce_score=None,
        blended=0.42,
        importance=importance,
        confidence=confidence,
    )


def test_importance_and_confidence_propagate() -> None:
    native = _native_per_hit(importance=0.75, confidence=0.9)
    mapped = _map_per_hit_explain(native)
    assert isinstance(mapped, PerHitExplain)
    # The F9 fields must reach the public dataclass (the fix-2 regression).
    assert mapped.importance == 0.75
    assert mapped.confidence == 0.9
    # The pre-existing fields still map through unchanged (no regression).
    assert mapped.id == 7
    assert mapped.arm == "graph_arm"
    assert mapped.vector_rank is None
    assert mapped.text_rank == 1
    assert mapped.graph_rank == 0
    assert mapped.fused_score == 0.42
    assert mapped.ce_score is None
    assert mapped.blended == 0.42


def test_none_importance_and_confidence_propagate_as_none() -> None:
    # Graceful-absent / neutral: `None` on the native object stays `None`.
    native = _native_per_hit(importance=None, confidence=None)
    mapped = _map_per_hit_explain(native)
    assert mapped.importance is None
    assert mapped.confidence is None


def test_perhitexplain_dataclass_declares_f9_fields() -> None:
    # The public dataclass must expose both fields with a `None` default so the
    # non-F9 construction path stays backward-compatible (Python evolution rule).
    fields = PerHitExplain.__dataclass_fields__
    assert "importance" in fields
    assert "confidence" in fields
    hit = PerHitExplain(
        id=1,
        arm="text",
        vector_rank=None,
        text_rank=0,
        graph_rank=None,
        fused_score=0.1,
        ce_score=None,
        blended=0.1,
    )
    assert hit.importance is None
    assert hit.confidence is None
