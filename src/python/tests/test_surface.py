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
import json
from pathlib import Path

from fathomdb import (
    CounterSnapshot,
    Engine,
    EngineConfig,
    SearchResult,
    SoftFallback,
    WriteReceipt,
    admin,
    read,
)


def _load_governed_surface_contract() -> dict[str, list[str]]:
    """Load the *single shared* governed-surface contract (AC-074 / REQ-053).

    The allowlist is declared exactly once, in
    `src/conformance/governed-surface-allowlist.json`, and read by BOTH the
    Python suite and the TypeScript suite (`src/ts/tests/surface.test.ts`).
    There is no per-binding duplicate literal, so Python and TypeScript cannot
    drift apart (cross-binding parity, P2). As of Slice 30 the four `read.*`
    members are LIVE: they are introspected from the `read` namespace and enter
    the live set, so the membership check (P1) is still subset (never equality)
    but `read.*` is now actually asserted-live, not documented-only.
    """
    # tests/ -> python/ -> src/ -> src/conformance/...
    here = Path(__file__).resolve()
    contract_path = here.parents[2] / "conformance" / "governed-surface-allowlist.json"
    with contract_path.open(encoding="utf-8") as fh:
        return json.load(fh)


_CONTRACT = _load_governed_surface_contract()
GOVERNED_SURFACE_ALLOWLIST = frozenset(_CONTRACT["allowlist"])
_CORE_LIVE_SURFACE = frozenset(_CONTRACT["core"])

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
        # 0.8.8 Slice 15 (OPP-9) — opt-in telemetry capture is observability,
        # NOT an application command (mirrors set_profiling/attach_subscriber).
        "enable_telemetry",
        "last_telemetry_query_id",
        "record_feedback",
        # 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-6) — degraded-open
        # observability accessors, NOT application commands (mirror counters /
        # open_report / last_telemetry_query_id).
        "dense_disabled",
        "dense_disabled_reason",
        "vector_equivalence_refusal_count",
    }
)

# Other public `Engine` members that are NOT application commands: the
# open-time report accessor (Shape D, an observability accessor) and the
# `path` / `config` data accessors. Subtracted from the introspected surface
# alongside `_INSTRUMENTATION` so the live-command set is exactly the command
# verbs. A NEW public command (e.g. `Engine.delete`) is NOT in this exclusion
# set, so it would enter `live` and fail the subset check (P1).
_ENGINE_NON_COMMAND = _INSTRUMENTATION | frozenset({"open_report", "path", "config"})

# The permanent recovery-name denylist (the FIVE names). `doctor` is SDK-absent
# by non-membership in the allowlist (it is a CLI verb), NOT by this denylist.
_RECOVERY_DENYLIST = frozenset(_CONTRACT["recovery_denylist"])

# Slice 30 — the four governed read verbs that go LIVE under the `read.*`
# namespace. Asserted present-in-the-introspected-surface so a future REMOVAL of
# any `read.*` verb fails this suite (not merely a documented-allowlist member).
_NOW_LIVE_READ_VERBS = frozenset(
    {"read.get", "read.get_many", "read.collection", "read.mutations"}
)


def _live_python_command_surface() -> set[str]:
    """Introspect the *live* public application-command surface.

    Built from the REAL public symbols of `Engine` (`dir(Engine)`, minus
    dunder/private names), the introspected `admin` namespace, and — as of
    Slice 30 — the introspected `read` namespace (`read.__all__`, mirroring the
    `admin.__all__` loop), subtracting the instrumentation/control methods and
    the other non-command members (`open_report`, `path`, `config`). The result
    is honest: any public name the live binding actually exposes that is not a
    known non-command verb enters the live set — so a hypothetical
    `Engine.delete` or a stray `read.delete` would surface here and fail the
    subset check (P1), rather than being silently ignored.
    """
    live: set[str] = set()
    for name in dir(Engine):
        if name.startswith("_"):
            continue
        if name in _ENGINE_NON_COMMAND:
            continue
        # `open` is the canonical `Engine.open`; the rest are bare verbs.
        live.add("Engine.open" if name == "open" else name)
    for verb in getattr(admin, "__all__", ()):
        if callable(getattr(admin, verb, None)):
            live.add(f"admin.{verb}")
    # Slice 30 — the governed `read.*` namespace. `read.__all__` is the snake_case
    # verb list (`get`, `get_many`, `collection`, `mutations`); each emits the
    # dotted allowlist name verbatim. A stray non-allowlisted `read` verb (e.g.
    # `read.delete`) enters `live` and fails the P1 subset check.
    for verb in getattr(read, "__all__", ()):
        if callable(getattr(read, verb, None)):
            live.add(f"read.{verb}")
    return live


def test_public_surface_is_allowlist() -> None:
    """P1 — every live public application command is a governed-allowlist member.

    Membership (subset), NOT equality: the allowlist is a superset; the live
    surface as of Slice 30 is the core five PLUS the four `read.*` verbs, all
    members of the 9-name allowlist.
    """
    live = _live_python_command_surface()
    extra = live - GOVERNED_SURFACE_ALLOWLIST
    assert not extra, f"live command(s) outside the governed allowlist: {sorted(extra)}"
    # The core write/lifecycle commands are live today.
    assert _CORE_LIVE_SURFACE <= live


def test_read_namespace_verbs_are_live() -> None:
    """Slice 30 — the four `read.*` verbs are now LIVE (introspected), not just
    documented-allowlist members. A future removal of any one fails here."""
    live = _live_python_command_surface()
    missing = _NOW_LIVE_READ_VERBS - live
    assert not missing, f"read.* verbs that must be live are missing: {sorted(missing)}"
    # And they remain governed-allowlist members.
    assert _NOW_LIVE_READ_VERBS <= GOVERNED_SURFACE_ALLOWLIST


def test_search_text_only_verb_is_live() -> None:
    """0.8.18 Slice 5 (#5, fix-1 CONCERN #7) — `search_text_only` is a LIVE governed
    verb, not merely an allowlist member. The P1 subset check passes even if the
    verb VANISHES (fewer-live is allowed) and `search_text_only` is not in
    `_CORE_LIVE_SURFACE`, so guard its PRESENCE + callability directly — otherwise
    the FTS-only degraded-mode surface (R-VEQ-4) could disappear vacuously-green."""
    assert "search_text_only" in GOVERNED_SURFACE_ALLOWLIST
    assert callable(getattr(Engine, "search_text_only", None)), (
        "Engine.search_text_only must be a live callable (governed FTS-only path)"
    )
    assert "search_text_only" in _live_python_command_surface(), (
        "search_text_only must be introspected-live, not documented-only"
    )


def test_read_is_module_level_namespace() -> None:
    assert inspect.ismodule(read)
    for verb in ("get", "get_many", "collection", "mutations"):
        assert callable(getattr(read, verb, None)), f"read.{verb} must be callable"


def test_surface_parity_py_matches_ts() -> None:
    """P2 — Python and TypeScript read ONE shared governed allowlist.

    The allowlist is declared exactly once, in
    `src/conformance/governed-surface-allowlist.json`. This suite loads it via
    `_load_governed_surface_contract()`; `src/ts/tests/surface.test.ts` loads
    the same file. Because there is a single declaration, Python and TypeScript
    can no longer carry divergent copies — parity is structural, not a
    byte-compared duplicate. This test pins that the suite genuinely consumes
    the shared contract (the introspected live surface is a subset of it).
    """
    contract = _load_governed_surface_contract()
    # The constant the suite enforces against IS the shared contract's allowlist.
    assert GOVERNED_SURFACE_ALLOWLIST == frozenset(contract["allowlist"])
    # The TypeScript suite reads this identical file; drift is impossible
    # because there is no second copy to diverge from.
    assert _live_python_command_surface() <= GOVERNED_SURFACE_ALLOWLIST


def test_allowlist_excludes_recovery_denylist() -> None:
    """P3 — allowlist ∩ {recover,restore,repair,fix,rebuild} = ∅.

    Allowlist-level assertion only; the byte-frozen `test_no_recovery_surface.py`
    remains the live enforcement of recovery unreachability.
    """
    assert GOVERNED_SURFACE_ALLOWLIST & _RECOVERY_DENYLIST == set()


# X1 (0.8.14 Slice 40) — module-level embedder-helper parity ledger.
#
# The cross-binding surface harness above (`_live_python_command_surface` /
# `src/ts/tests/surface.test.ts`) introspects only `Engine` / `admin` / `read`,
# so it is BLIND to module-level functions. `embed_batch_cls` is a module-level
# embedder helper exposed PYTHON-ONLY — it lives in `fathomdb-py/src/lib.rs`,
# `fathomdb.__all__`, and the `_fathomdb.pyi` stub, and is intentionally ABSENT
# from the TS binding (`fathomdb-napi/src/lib.rs`, `src/ts`). TS parity for it is
# a TRACKED DEFERRAL (py-first). By contrast the `Engine.embed` *verb* IS at
# Py↔TS parity (governed-allowlist member `embed`). This ledger makes the
# asymmetry ASSERTED rather than a silent blind spot: a new/removed Python
# module-level embedder helper must be reflected here in lockstep with a
# conscious TS parity decision.
_PY_ONLY_MODULE_EMBEDDER_HELPERS = frozenset({"embed_batch_cls"})


def test_module_level_embedder_helper_asymmetry_is_tracked() -> None:
    import fathomdb

    # The tracked py-only helpers are present as module-level callables and
    # exported in `__all__`.
    for name in _PY_ONLY_MODULE_EMBEDDER_HELPERS:
        assert callable(getattr(fathomdb, name, None)), (
            f"expected Python-only module-level embedder helper {name!r}"
        )
        assert name in getattr(fathomdb, "__all__", ()), (
            f"{name!r} must be exported in fathomdb.__all__"
        )

    # The module-level embedder-helper surface is EXACTLY the tracked py-only set.
    # A new module-level `embed*` callable (e.g. a future `embed_batch`) would
    # break this and force a conscious update here + a TS parity decision, rather
    # than drifting in silently. (`Engine.embed` is a class method, not a module
    # attribute, so it is correctly out of scope for this module-level check.)
    live_module_embed_helpers = {
        n
        for n in getattr(fathomdb, "__all__", ())
        if "embed" in n.lower() and callable(getattr(fathomdb, n, None))
    }
    assert live_module_embed_helpers == _PY_ONLY_MODULE_EMBEDDER_HELPERS, (
        "module-level embedder-helper surface drifted from the tracked py-only "
        "set; update _PY_ONLY_MODULE_EMBEDDER_HELPERS and record the TS parity "
        f"decision. live={sorted(live_module_embed_helpers)}"
    )


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
        "enable_telemetry",
        "last_telemetry_query_id",
        "record_feedback",
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
