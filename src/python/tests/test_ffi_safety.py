"""FFI safety-contract tests for the PyO3 binding.

Covers AC-067 (panic catch), AC-068a (embedded NUL rejected), and
AC-068b (unpaired UTF-16 surrogate rejected). The contract is locked
here for Python and mirrored by the napi-rs binding in Phase 11b.
"""

from __future__ import annotations

import os

import pytest

from fathomdb import Engine, SearchFilter, admin
from fathomdb._fathomdb import force_panic_for_test
from fathomdb.errors import EngineError, WriteValidationError


def test_panic_surfaces_as_python_exception(db_path: str) -> None:
    """AC-067 — engine panics must not abort the host process.

    The binding must raise PyO3's `PanicException` (not `EngineError`):
    panic is a contract bug, not a typed engine outcome, so callers that
    catch `EngineError` must not silently swallow it.
    """

    pid_before = os.getpid()
    with pytest.raises(BaseException) as excinfo:
        force_panic_for_test()
    pid_after = os.getpid()
    assert pid_before == pid_after, "host process must not be aborted by engine panic"

    exc_type = type(excinfo.value)
    assert exc_type.__name__ == "PanicException", (
        f"expected pyo3 PanicException, got {exc_type.__module__}.{exc_type.__name__}"
    )
    assert exc_type.__module__ == "pyo3_runtime", (
        f"PanicException must come from pyo3_runtime, got {exc_type.__module__}"
    )
    assert not isinstance(excinfo.value, EngineError), (
        "PanicException must NOT subclass EngineError"
    )

    # Subsequent engine work must still succeed.
    engine = Engine.open(db_path)
    try:
        snapshot = engine.counters()
        assert snapshot is not None
    finally:
        engine.close()


def test_embedded_nul_in_body_rejected_before_write(db_path: str) -> None:
    """AC-068a — embedded NUL in any FFI string raises
    WriteValidationError, and no row is written."""

    engine = Engine.open(db_path)
    try:
        # Register a latest-state collection so the body would otherwise
        # commit a row.
        admin.configure(engine, name="nul_col", body="{}")
        before = engine.counters().write_rows

        with pytest.raises(WriteValidationError) as excinfo:
            engine.write(
                [
                    {
                        "op_store": {
                            "collection": "nul_col",
                            "record_key": "k1",
                            "body": '{"x":"a\x00b"}',
                        }
                    }
                ]
            )
        assert isinstance(excinfo.value, EngineError)

        after = engine.counters().write_rows
        assert after == before, "no row may be written when a NUL is rejected"
    finally:
        engine.close()


def test_unpaired_surrogate_in_body_rejected_before_write(db_path: str) -> None:
    """AC-068b — lone UTF-16 surrogate codepoints raise
    WriteValidationError, and no row is written."""

    engine = Engine.open(db_path)
    try:
        admin.configure(engine, name="sur_col", body="{}")
        before = engine.counters().write_rows

        with pytest.raises(WriteValidationError) as excinfo:
            engine.write(
                [
                    {
                        "op_store": {
                            "collection": "sur_col",
                            "record_key": "k1",
                            "body": '{"x":"a\ud800b"}',
                        }
                    }
                ]
            )
        assert isinstance(excinfo.value, EngineError)

        after = engine.counters().write_rows
        assert after == before, "no row may be written when a surrogate is rejected"
    finally:
        engine.close()


def test_embedded_nul_in_node_kind_rejected(db_path: str) -> None:
    """AC-068a — applies to every FFI string field, not just bodies."""

    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError):
            engine.write([{"kind": "do\x00c", "body": "{}"}])
    finally:
        engine.close()


# --- Slice 10 fix-1: G10 SearchFilter string fields cross the FFI too ---
# The new filter kwargs (source_type, kind, status) are FFI strings and must
# be routed through the same validate_ffi_string_py gate as `query` and the
# write fields. created_after is i64 — no string validation. These tests pin
# the *binding wiring* (that search's filter args reject NUL / lone surrogate
# before reaching the engine), not the helper itself.

_FILTER_NUL = "a\x00b"
_FILTER_SURROGATE = "a\ud800b"


@pytest.mark.parametrize("field", ["source_type", "kind", "status"])
def test_embedded_nul_in_search_filter_rejected(db_path: str, field: str) -> None:
    """AC-068a — embedded NUL in a search filter string raises
    WriteValidationError before the query reaches the engine."""

    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError) as excinfo:
            engine.search("q", SearchFilter(**{field: _FILTER_NUL}))
        assert isinstance(excinfo.value, EngineError)
    finally:
        engine.close()


@pytest.mark.parametrize("field", ["source_type", "kind", "status"])
def test_unpaired_surrogate_in_search_filter_rejected(db_path: str, field: str) -> None:
    """AC-068b — a lone UTF-16 surrogate in a search filter string raises
    WriteValidationError before the query reaches the engine."""

    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError) as excinfo:
            engine.search("q", SearchFilter(**{field: _FILTER_SURROGATE}))
        assert isinstance(excinfo.value, EngineError)
    finally:
        engine.close()
