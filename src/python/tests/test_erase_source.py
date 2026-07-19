"""0.8.20 Slice 5d (R-20-E4, design §4 item 9b) — ``erase_source`` is a
first-class SDK lifecycle verb, reachable with NO CLI on ``PATH``.

The gap this closes: ``purge`` addresses a GOVERNED node by ``logical_id``, so
anonymous content — rows written with no ``logical_id`` — was reachable by no
SDK verb at all. The only erasure path was the operator CLI (``fathomdb recover
--excise-source``), which an embedded SDK consumer may not have. A consumer
holding a deletion obligation over anonymous content could therefore not
discharge it.

Cross-binding equivalence anchor: ``src/ts/tests/erase-source.test.ts`` asserts
the SAME behaviour for the same inputs (Py ≡ TS, R-X-1).

Test-design contract (design §3, Rule 1): an erasure witness must NOT be a
``search()`` call — both read paths gate on ``canonical_nodes``, so a search
assertion passes on the broken code. The witnesses here are the returned report
counts (a second erase proves the first did not touch the other source). The
raw-table assertions for the same erasure live in the engine suites, which have
SQL access.
"""

from __future__ import annotations

import pytest

from fathomdb import Engine
from fathomdb.errors import WriteValidationError


def _anonymous_node(body: str, source_id: str) -> dict:
    """No ``logical_id`` — exactly the content ``purge`` cannot reach."""
    return {"kind": "doc", "body": body, "source_id": source_id}


def test_erase_source_erases_anonymous_content_without_cli(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write(
            [
                _anonymous_node("erasable alpha payload", "tenant-a"),
                _anonymous_node("erasable beta payload", "tenant-a"),
                _anonymous_node("retained gamma payload", "tenant-b"),
            ]
        )

        report = engine.erase_source("tenant-a")
        assert report.source_ref == "tenant-a"
        assert report.nodes_excised == 2, "both tenant-a rows must be erased, and ONLY those"

        # Non-perturbation, asserted as a SECOND erase: its count proves
        # tenant-b's row still existed after the first call.
        second = engine.erase_source("tenant-b")
        assert second.nodes_excised == 1, "the first erasure must not have touched tenant-b"
    finally:
        engine.close()


def test_erase_source_is_idempotent(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write([_anonymous_node("idempotence payload", "tenant-a")])
        assert engine.erase_source("tenant-a").nodes_excised == 1
        # Retrying an interrupted erasure obligation must not raise.
        assert engine.erase_source("tenant-a").nodes_excised == 0
        assert engine.erase_source("never-written").nodes_excised == 0
    finally:
        engine.close()


@pytest.mark.parametrize(
    "source_id",
    [
        "",
        "   ",
        # The engine's reserved namespace is reachable ONLY through the CLI
        # recovery seam. A caller able to erase `_legacy:pre-0.8.20` through the
        # governed verb could wipe every pre-0.8.20 anonymous row in one call.
        "_engine:coverage",
        "_legacy:pre-0.8.20",
    ],
)
def test_erase_source_rejects_empty_and_reserved_ids(db_path: str, source_id: str) -> None:
    engine = Engine.open(db_path)
    try:
        with pytest.raises(WriteValidationError):
            engine.erase_source(source_id)
    finally:
        engine.close()
