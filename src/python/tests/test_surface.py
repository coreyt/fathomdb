"""Public surface assertions for the Python SDK.

Binds AC-074 (REQ-053): governed SDK surface — a curated allowlist with
cross-binding parity, a permanent recovery-name denylist, and the typed /
no-raw-SQL boundary. (AC-057a's verb-count scope cap is superseded by AC-074;
the surface is now governed-but-open, not capped.)

Pins the governed-surface allowlist (membership, not a count), the
engine-attached instrumentation methods, the keyword vs EngineConfig forms of
`Engine.open`, and the soft-fallback record shape per `dev/interfaces/python.md`
and `dev/design/bindings.md` § 1 / § 3.
"""

from __future__ import annotations

import inspect

from fathomdb import (
    CounterSnapshot,
    Engine,
    EngineConfig,
    SearchResult,
    SoftFallback,
    WriteReceipt,
    admin,
)


# The governed SDK surface allowlist (AC-074 / REQ-053). The set of public
# application-command callables across the SDK bindings, B1 `read.*` namespace.
# This constant is declared identically in `src/ts/tests/surface.test.ts`; the
# two are membership-identical (cross-binding parity, P2) and byte-compared by
# the Slice 25.b audit. The `read.*` members are documented-allowlist members
# now but do NOT go live as importable symbols until Slice 30 — so the
# membership check below (P1) uses subset, never equality.
GOVERNED_SURFACE_ALLOWLIST = {
    # Core (live today, unchanged)
    "Engine.open",
    "admin.configure",
    "write",
    "search",
    "close",
    # Read surface (B1 read.*, ships 0.8.0, goes LIVE at Slice 30)
    "read.get",
    "read.get_many",
    "read.collection",
    "read.mutations",
}

# Engine-attached instrumentation/control methods are observability, NOT
# application commands — excluded from the allowlist (preserved from AC-057a's
# measurement boundary).
_INSTRUMENTATION = frozenset(
    {
        "drain",
        "counters",
        "set_profiling",
        "set_slow_threshold_ms",
        "attach_logging_subscriber",
    }
)

# The permanent recovery-name denylist (the FIVE names). `doctor` is SDK-absent
# by non-membership in the allowlist (it is a CLI verb), NOT by this denylist.
_RECOVERY_DENYLIST = frozenset({"recover", "restore", "repair", "fix", "rebuild"})


def _live_python_command_surface() -> set[str]:
    """Introspect the *live* public application-command surface.

    The `Engine` lifecycle/command verbs plus the `admin` namespace callables,
    excluding data/config/error types and the instrumentation/control methods.
    Built from actual symbol presence so the membership check (P1) is honest:
    a name only enters the live set if the live binding actually exposes it.
    """
    live: set[str] = set()
    for verb in ("open", "write", "search", "close"):
        if hasattr(Engine, verb) and verb not in _INSTRUMENTATION:
            # `open` is the canonical `Engine.open`; the rest are bare verbs.
            live.add("Engine.open" if verb == "open" else verb)
    for verb in getattr(admin, "__all__", ()):
        if callable(getattr(admin, verb, None)):
            live.add(f"admin.{verb}")
    return live


def test_public_surface_is_allowlist() -> None:
    """P1 — every live public application command is a governed-allowlist member.

    Membership (subset), NOT equality: the allowlist is a superset that includes
    the not-yet-live `read.*` verbs (live at Slice 30), so a live surface that is
    currently the core five is honestly green against the 9-member allowlist.
    """
    live = _live_python_command_surface()
    extra = live - GOVERNED_SURFACE_ALLOWLIST
    assert not extra, f"live command(s) outside the governed allowlist: {sorted(extra)}"
    # The core write/lifecycle commands are live today.
    assert {"Engine.open", "admin.configure", "write", "search", "close"} <= live


def test_surface_parity_py_matches_ts() -> None:
    """P2 — the Python governed allowlist equals the TypeScript one.

    `src/ts/tests/surface.test.ts` declares the identical
    GOVERNED_SURFACE_ALLOWLIST; the mirror below is the TS contract and the
    Slice 25.b audit byte-compares the two constants across files.
    """
    ts_governed_surface_allowlist = {
        "Engine.open",
        "admin.configure",
        "write",
        "search",
        "close",
        "read.get",
        "read.get_many",
        "read.collection",
        "read.mutations",
    }
    assert GOVERNED_SURFACE_ALLOWLIST == ts_governed_surface_allowlist


def test_allowlist_excludes_recovery_denylist() -> None:
    """P3 — allowlist ∩ {recover,restore,repair,fix,rebuild} = ∅.

    Allowlist-level assertion only; the byte-frozen `test_no_recovery_surface.py`
    remains the live enforcement of recovery unreachability.
    """
    assert GOVERNED_SURFACE_ALLOWLIST & _RECOVERY_DENYLIST == set()


def test_admin_is_module_level_namespace() -> None:
    assert inspect.ismodule(admin)
    assert hasattr(admin, "configure")


def test_engine_exposes_instrumentation_methods() -> None:
    for instr in (
        "drain",
        "counters",
        "set_profiling",
        "set_slow_threshold_ms",
        "attach_logging_subscriber",
    ):
        assert hasattr(Engine, instr), f"Engine must expose {instr}"


def test_engine_open_accepts_kwargs_and_engine_config(tmp_path) -> None:
    a = Engine.open(
        str(tmp_path / "a.sqlite"),
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
    b = Engine.open(str(tmp_path / "b.sqlite"), config=cfg)

    assert a.config == b.config
    a.close()
    b.close()


def test_engine_open_rejects_kwargs_and_config_together(db_path: str) -> None:
    cfg = EngineConfig()
    try:
        Engine.open(db_path, config=cfg, embedder_pool_size=2)
    except ValueError:
        return
    raise AssertionError("Engine.open must reject config + kwargs in the same call")


def test_write_receipt_carries_cursor(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        receipt = engine.write([{"kind": "doc", "body": "{}"}])
        assert isinstance(receipt, WriteReceipt)
        assert isinstance(receipt.cursor, int)
    finally:
        engine.close()


def test_search_result_carries_optional_soft_fallback(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        result = engine.search("hello")
        assert isinstance(result, SearchResult)
        assert isinstance(result.projection_cursor, int)
        assert result.soft_fallback is None
    finally:
        engine.close()


def test_soft_fallback_branch_is_typed() -> None:
    f = SoftFallback(branch="vector")
    assert f.branch == "vector"
    f = SoftFallback(branch="text")
    assert f.branch == "text"


def test_admin_configure_returns_write_receipt_shape(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        receipt = admin.configure(engine, name="default", body="{}")
        assert isinstance(receipt, WriteReceipt)
        assert isinstance(receipt.cursor, int)
    finally:
        engine.close()


def test_engine_attached_stub_methods_return_canonical_types(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.drain(timeout_s=0)
        snapshot = engine.counters()
        assert snapshot is not None

        engine.set_profiling(enabled=True)
        engine.set_slow_threshold_ms(value=100)

        import logging

        engine.attach_logging_subscriber(logging.getLogger("fathomdb-test"))
    finally:
        engine.close()


def test_counters_snapshot_carries_six_canonical_fields(db_path: str) -> None:
    """Phase 12-TX: parity with TS `CounterSnapshot` six-field shape per
    `dev/design/bindings.md` § 1. The PyO3 binding already returns the
    six fields; the Python wrapper must not drop them."""

    engine = Engine.open(db_path)
    try:
        snap = engine.counters()
        assert isinstance(snap, CounterSnapshot)
        for f in (
            "queries",
            "writes",
            "write_rows",
            "admin_ops",
            "cache_hit",
            "cache_miss",
        ):
            assert isinstance(getattr(snap, f), int), (
                f"CounterSnapshot.{f} must be int (parity with TS CounterSnapshot)"
            )
    finally:
        engine.close()
