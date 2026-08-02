"""Compile-only contract for the Python clean-clone quality gate."""

from fathomdb._fathomdb import Engine as NativeEngine
from fathomdb._fathomdb import OpenReport as NativeOpenReport
from fathomdb.engine import Engine
from fathomdb.errors import VectorEquivalenceMismatchError
from fathomdb.graph import _to_search_hit


def native_observability_contract(engine: Engine) -> tuple[bool, str | None, int]:
    """The wrapper's three degraded-vector accessors retain their public types."""
    return (
        engine.dense_disabled(),
        engine.dense_disabled_reason(),
        engine.vector_equivalence_refusal_count(),
    )


def native_stub_contract(
    engine: NativeEngine, report: NativeOpenReport
) -> tuple[bool, str | None, int, bool, str | None]:
    """The wrapper only reaches native members that the release stub declares."""
    return (
        engine.dense_disabled(),
        engine.dense_disabled_reason(),
        engine.vector_equivalence_refusal_count(),
        report.dense_disabled,
        report.dense_disabled_reason,
    )


def graph_contract() -> object:
    """Keep the graph wrapper's native-to-public hit conversion type-checked."""
    return _to_search_hit


def vector_refusal_contract() -> str:
    """The typed Python-only constructor accepts the documented reason payload."""
    return VectorEquivalenceMismatchError("vector divergence", reason="P1 flips=3").reason
