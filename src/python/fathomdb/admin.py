"""Admin namespace exposing the `configure` verb beside `Engine`.

Per `dev/interfaces/python.md` § Runtime surface, `admin.configure` is the
fifth canonical SDK verb. The 0.6.0 surface-stubs slice publishes the
signature and return shape; the writer-thread wiring lands later under
`design/engine.md`.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from fathomdb.types import WriteReceipt

if TYPE_CHECKING:
    from fathomdb.engine import Engine


def configure(engine: "Engine", *, name: str, body: str) -> WriteReceipt:
    """Submit an admin schema configuration.

    `engine` is the open engine handle. `name` is the registered schema
    identifier; `body` is the JSON-Schema body. Returns the canonical
    `WriteReceipt` shape; the 0.6.0 stub does not commit anything to the
    engine and returns a synthetic cursor reflecting the call sequence.
    """

    if not name:
        raise ValueError("admin.configure requires a non-empty name")
    if body is None:
        raise ValueError("admin.configure requires a body")
    return engine._record_admin_configure(name=name, body=body)


__all__ = ["configure"]
