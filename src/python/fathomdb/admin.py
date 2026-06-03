"""Admin namespace exposing the `configure` verb beside `Engine`.

Per `dev/interfaces/python.md` § Runtime surface, `admin.configure` is the
fifth canonical SDK verb. The native binding (`fathomdb._fathomdb`)
performs the writer-thread wiring; this module exposes the typed
Python signature and converts the native receipt to the public
`WriteReceipt` dataclass.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from fathomdb._fathomdb import admin_configure as _native_configure
from fathomdb.types import WriteReceipt

if TYPE_CHECKING:
    from fathomdb.engine import Engine


def configure(engine: "Engine", *, name: str, body: str) -> WriteReceipt:
    """Submit an admin schema configuration."""

    if not name:
        raise ValueError("admin.configure requires a non-empty name")
    if body is None:
        raise ValueError("admin.configure requires a body")
    receipt = _native_configure(engine._native, name, body)
    return WriteReceipt(
        cursor=receipt.cursor,
        row_cursors=tuple(receipt.row_cursors),
        dangling_edge_endpoints=receipt.dangling_edge_endpoints,
    )


__all__ = ["configure"]
