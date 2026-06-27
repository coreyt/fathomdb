"""0.8.6 Slice 10 (OPP-5) — the governed READ surface is the COMPLETE consumer
boundary (R-CH-1).

A consumer (e.g. Memex) migrating onto FathomDB's governed verbs must be able to
satisfy every read-path need through the PUBLIC namespaces — `read.*`, `graph.*`,
`Engine.embed`, plus core `search`/`write` — WITHOUT reaching into the engine's
private surface (`engine._native`, internal modules). This test enumerates the
verbs a consumer needs and asserts each is:

  (a) reachable as a public callable on its namespace (no internal-engine reach), and
  (b) a member of the governed-surface allowlist (a sanctioned boundary, not an
      accidental leak).

Mirror: `src/ts/tests/consumer-boundary.test.ts` asserts the SAME set for the TS
binding, so the consumer boundary cannot drift between bindings.
"""

from __future__ import annotations

import json
from pathlib import Path

from fathomdb import Engine, graph, read

_CONTRACT = json.loads(
    (Path(__file__).resolve().parents[2] / "conformance" / "governed-surface-allowlist.json").read_text()
)
_ALLOWLIST = frozenset(_CONTRACT["allowlist"])

# The read-path verbs a consumer needs to migrate off any internal-engine reach.
# (snake_case Python names; the TS mirror uses the camelCase equivalents.)
_CONSUMER_READ_VERBS: dict[str, object] = {
    "read.get": read.get,
    "read.get_many": read.get_many,
    "read.collection": read.collection,
    "read.mutations": read.mutations,
    "read.list": read.list,
    "graph.neighbors": graph.neighbors,
    "graph.search_expand": graph.search_expand,
}


def test_consumer_read_verbs_are_public_callables() -> None:
    """(a) every consumer read verb is a public callable on its namespace —
    no `engine._native` / private reach required to use it."""
    for name, fn in _CONSUMER_READ_VERBS.items():
        assert callable(fn), f"{name} must be a public callable on the governed surface"


def test_engine_embed_is_a_public_method() -> None:
    """`Engine.embed` (the read-path embed primitive) is a public method —
    reachable without `engine._native`."""
    assert callable(getattr(Engine, "embed", None)), "Engine.embed must be a public method"


def test_consumer_boundary_verbs_are_all_governed() -> None:
    """(b) every verb a consumer needs is a member of the governed allowlist —
    so the consumer boundary is a sanctioned contract, not an accidental surface."""
    needed = set(_CONSUMER_READ_VERBS) | {"embed", "search", "write", "Engine.open", "close"}
    missing = needed - _ALLOWLIST
    assert not missing, f"consumer-boundary verbs missing from the governed allowlist: {sorted(missing)}"
