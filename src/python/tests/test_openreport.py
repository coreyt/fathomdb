"""AC-068c — `engine.open_report()` surfaces the native `OpenReport`.

Shape D (locked HITL 2026-05-24): the report is exposed as an
engine-attached accessor, not a return-shape change on `Engine.open`.
`engine.open_report()` is a snapshot captured at open time; repeat calls
return identical data.

Spec refs:
- `dev/design/engine.md` § "`Engine.open` success result" — locked
  field subset (`schema_version_before`, `schema_version_after`,
  `migration_steps`, `embedder_warmup_ms`).
- Native struct: `src/rust/crates/fathomdb-engine/src/lib.rs:541-548`
  carries two additional fields (`query_backend`, `default_embedder`).
- `dev/interfaces/python.md` Engine-attached instrumentation list.
"""

from __future__ import annotations

from fathomdb import Engine


def test_open_report_returns_native_fields(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        report = engine.open_report()

        assert isinstance(report.schema_version_before, int)
        assert isinstance(report.schema_version_after, int)
        assert report.schema_version_after >= report.schema_version_before
        assert isinstance(report.migration_steps, list)
        assert isinstance(report.embedder_warmup_ms, int)
        assert report.embedder_warmup_ms >= 0
        assert isinstance(report.query_backend, str)
        assert report.query_backend

        identity = report.default_embedder
        assert isinstance(identity.name, str) and identity.name
        assert isinstance(identity.revision, str) and identity.revision
        assert isinstance(identity.dimension, int) and identity.dimension > 0
    finally:
        engine.close()


def test_open_report_is_idempotent(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        first = engine.open_report()
        second = engine.open_report()

        assert first.schema_version_before == second.schema_version_before
        assert first.schema_version_after == second.schema_version_after
        assert first.embedder_warmup_ms == second.embedder_warmup_ms
        assert first.query_backend == second.query_backend
        assert first.default_embedder.name == second.default_embedder.name
        assert first.default_embedder.revision == second.default_embedder.revision
        assert first.default_embedder.dimension == second.default_embedder.dimension

        assert len(first.migration_steps) == len(second.migration_steps)
        for a, b in zip(first.migration_steps, second.migration_steps):
            assert a.step_id == b.step_id
            assert a.duration_ms == b.duration_ms
            assert a.failed == b.failed
    finally:
        engine.close()


def test_open_signature_returns_engine_handle(db_path: str) -> None:
    """Shape D guarantee — `Engine.open` still returns just the engine."""

    engine = Engine.open(db_path)
    try:
        assert isinstance(engine, Engine)
    finally:
        engine.close()
