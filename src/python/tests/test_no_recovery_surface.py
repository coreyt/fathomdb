"""AC-041 — recovery tooling unreachable from the Python runtime SDK.

Mirrors the introspection style of `test_surface.py`. Asserts the public
top-level `fathomdb` module, the `Engine` class, and the `admin` namespace
expose none of the canonical recovery-verb names. Recovery is CLI-only per
`dev/interfaces/cli.md`; the runtime SDK five-verb surface
(`open`, `write`, `search`, `close`, `admin.configure`) does not mirror it.
"""

from __future__ import annotations

import fathomdb
from fathomdb import Engine, admin

FORBIDDEN = {"recover", "restore", "repair", "fix", "rebuild"}


def _public(obj: object) -> set[str]:
    return {n for n in dir(obj) if not n.startswith("_")}


def test_top_level_module_has_no_recovery_symbols() -> None:
    assert FORBIDDEN.isdisjoint(_public(fathomdb))


def test_engine_class_has_no_recovery_methods() -> None:
    assert FORBIDDEN.isdisjoint(_public(Engine))


def test_engine_instance_has_no_recovery_methods(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        assert FORBIDDEN.isdisjoint(_public(engine))
    finally:
        engine.close()


def test_admin_namespace_has_no_recovery_symbols() -> None:
    assert FORBIDDEN.isdisjoint(_public(admin))
