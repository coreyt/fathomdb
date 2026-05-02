"""Public surface assertions for the Python SDK.

Pins the five-verb top-level surface, the engine-attached instrumentation
methods, the keyword vs EngineConfig forms of `Engine.open`, and the
soft-fallback record shape per `dev/interfaces/python.md` and
`dev/design/bindings.md` § 3.
"""

from __future__ import annotations

import inspect

import fathomdb
from fathomdb import Engine, EngineConfig, SearchResult, SoftFallback, WriteReceipt, admin


def test_top_level_exports_match_canonical_set() -> None:
    expected = {
        "Engine",
        "EngineConfig",
        "SearchResult",
        "SoftFallback",
        "WriteReceipt",
        "admin",
        "errors",
    }
    assert expected.issubset(set(fathomdb.__all__))


def test_admin_is_module_level_namespace() -> None:
    assert inspect.ismodule(admin)
    assert hasattr(admin, "configure")


def test_engine_exposes_five_verbs_plus_instrumentation() -> None:
    for verb in ("open", "write", "search", "close"):
        assert hasattr(Engine, verb), f"Engine must expose {verb}"

    for instr in (
        "drain",
        "counters",
        "set_profiling",
        "set_slow_threshold_ms",
        "attach_logging_subscriber",
    ):
        assert hasattr(Engine, instr), f"Engine must expose {instr}"


def test_engine_open_accepts_kwargs_and_engine_config() -> None:
    by_kwargs = Engine.open(
        "rewrite.sqlite",
        embedder_pool_size=2,
        scheduler_runtime_threads=4,
        provenance_row_cap=1024,
        embedder_call_timeout_ms=30_000,
        slow_threshold_ms=250,
    )
    cfg = EngineConfig(
        embedder_pool_size=2,
        scheduler_runtime_threads=4,
        provenance_row_cap=1024,
        embedder_call_timeout_ms=30_000,
        slow_threshold_ms=250,
    )
    by_config = Engine.open("rewrite.sqlite", config=cfg)

    assert by_kwargs.config == by_config.config


def test_engine_open_rejects_kwargs_and_config_together() -> None:
    cfg = EngineConfig()
    try:
        Engine.open("rewrite.sqlite", config=cfg, embedder_pool_size=2)
    except ValueError:
        return
    raise AssertionError("Engine.open must reject config + kwargs in the same call")


def test_write_receipt_carries_cursor() -> None:
    engine = Engine.open("rewrite.sqlite")
    receipt = engine.write([{"kind": "doc"}])
    assert isinstance(receipt, WriteReceipt)
    assert isinstance(receipt.cursor, int)


def test_search_result_carries_optional_soft_fallback() -> None:
    engine = Engine.open("rewrite.sqlite")
    result = engine.search("hello")
    assert isinstance(result, SearchResult)
    assert isinstance(result.projection_cursor, int)
    assert result.soft_fallback is None


def test_soft_fallback_branch_is_typed() -> None:
    f = SoftFallback(branch="vector")
    assert f.branch == "vector"
    f = SoftFallback(branch="text")
    assert f.branch == "text"


def test_admin_configure_returns_write_receipt_shape() -> None:
    engine = Engine.open("rewrite.sqlite")
    receipt = admin.configure(engine, name="default", body="{}")
    assert isinstance(receipt, WriteReceipt)
    assert isinstance(receipt.cursor, int)


def test_engine_attached_stub_methods_return_canonical_types() -> None:
    engine = Engine.open("rewrite.sqlite")

    engine.drain(timeout_s=0)
    snapshot = engine.counters()
    assert snapshot is not None

    engine.set_profiling(enabled=True)
    engine.set_slow_threshold_ms(value=100)

    import logging

    engine.attach_logging_subscriber(logging.getLogger("fathomdb-test"))
