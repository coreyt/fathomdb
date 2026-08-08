"""Slice 22 projection-runtime status parity through the real PyO3 database."""

from __future__ import annotations

from dataclasses import FrozenInstanceError

import pytest

from fathomdb import (
    Engine,
    ProjectionRole,
    ProjectionRuntimeStatus,
    ProjectionRuntimeStatusEntry,
    ProjectionSpec,
    read,
)


def _open(path: str) -> Engine:
    return Engine.open(path, use_default_embedder=False)


def _node(kind: str, logical_id: str) -> dict[str, str]:
    return {
        "kind": kind,
        "logical_id": logical_id,
        "body": '{"alpha":"dense meaning","zeta":"plain value"}',
        "source_id": "py-test:slice22-status",
    }


def test_projection_status_is_a_frozen_typed_current_read(tmp_path) -> None:
    """Exact Python wires mirror the signed Rust facade, not `ProjectionDelta`."""

    engine = _open(str(tmp_path / "status.sqlite"))
    try:
        engine.configure_projections(
            [
                ProjectionSpec(
                    name="zeta", roles=frozenset({ProjectionRole.FILTERABLE})
                ),
                ProjectionSpec(
                    name="alpha", roles=frozenset({ProjectionRole.SEARCHABLE}), vector=True),
            ]
        )
        engine.write([_node("invoice", "I1")])
        engine.write([_node("doc", "D1")])
        engine.write([_node("entity", "E1")])
        engine.write([_node("invoice", "I2")])

        status = read.projection_status(engine)
        assert isinstance(status, ProjectionRuntimeStatus)
        assert all(isinstance(item, ProjectionRuntimeStatusEntry) for item in status.projections)
        assert status.runtime_embedder_available is False
        assert status.runtime_unavailability_reason == "no_runtime"
        assert [(item.name, item.dense_readiness) for item in status.projections] == [
            ("alpha", "unavailable"),
            ("zeta", "not_declared"),
        ]
        assert status.vector_unsupported_kinds == ("entity", "invoice")
        assert read.projection_status(engine) == status, "repeated reads have no side effects"

        with pytest.raises(FrozenInstanceError):
            status.runtime_embedder_available = True  # type: ignore[misc]
    finally:
        engine.close()
