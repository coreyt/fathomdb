"""0.8.20 Slice 10a (TC-31) — ``source_id`` is readable on EVERY search hit,
so the value ``erase_source`` consumes is reachable from any hit a caller gets.

The defect this closes: 0.8.20 made provenance structurally mandatory on write
and shipped ``erase_source`` as the SDK erasure verb, but ``SearchHit.source_id``
was populated by the GRAPH ARM only. Every text/BM25F, vector and edge-FTS hit
carried ``None``, so a consumer holding a text hit and a deletion obligation
could not name the source to erase. 0.8.19 had also stopped surfacing
``write_cursor`` to the SDKs, so no fallback route from hit → document existed.

Cross-binding equivalence anchor: ``src/ts/tests/tc31-source-id-every-hit.test.ts``
asserts the SAME behaviour for the same inputs (Py ≡ TS, R-X-1).

Test-design contract (design §3, Rule 1): an erasure witness must NOT be a
``search()`` call — both read paths gate on ``canonical_nodes``, so a search
assertion passes on the broken code. The witnesses here are the returned report
counts. The RAW-TABLE erasure witnesses for the same round trip live in
``src/rust/crates/fathomdb-engine/tests/tc31_source_id_on_every_hit.rs``, which
has SQL access. What THIS file proves is the binding-level contract: the value
actually arrives in Python and is accepted by ``erase_source``.
"""

from __future__ import annotations

from fathomdb import Engine


def _node(body: str, source_id: str, logical_id: str | None = None) -> dict:
    node: dict = {"kind": "doc", "body": body, "source_id": source_id}
    if logical_id is not None:
        node["logical_id"] = logical_id
    return node


def _edge(from_id: str, to_id: str, logical_id: str, source_id: str, body: str) -> dict:
    return {
        "edge": {
            "kind": "link",
            "from": from_id,
            "to": to_id,
            "logical_id": logical_id,
            "source_id": source_id,
            "body": body,
        }
    }


def test_text_hit_exposes_source_id_that_erase_source_accepts(db_path: str) -> None:
    """The text/BM25F arm — the dominant hit class, and the one that carried
    ``None`` before TC-31."""
    engine = Engine.open(db_path)
    try:
        engine.write(
            [
                _node("tc31pytext confidential dossier", "tenant-a"),
                _node("tc31pytext unrelated retained", "tenant-b"),
            ]
        )

        result = engine.search("tc31pytext")
        hits = [h for h in result.results if "confidential" in h.body]
        assert hits, "the text arm must surface the document"
        hit = hits[0]

        assert hit.source_id == "tenant-a", (
            "TC-31: a text hit must carry its own source_id, not None "
            f"(got {hit.source_id!r})"
        )

        # The whole point: the value read off the hit is the erasure key.
        report = engine.erase_source(hit.source_id)
        assert report.source_ref == "tenant-a"
        assert report.nodes_excised == 1

        # Non-perturbation, asserted as a SECOND erase (Rule 1: not a search).
        assert engine.erase_source("tenant-b").nodes_excised == 1
    finally:
        engine.close()


def test_every_hit_carries_its_own_source_id(db_path: str) -> None:
    """Guards the cheapest fake fix (one constant stamped on every hit): two
    documents with different provenance must each report their own."""
    engine = Engine.open(db_path)
    try:
        engine.write(
            [
                _node("tc31pyshared marker alpha", "tenant-a"),
                _node("tc31pyshared marker beta", "tenant-b"),
            ]
        )

        result = engine.search("tc31pyshared")
        by_source = {h.body: h.source_id for h in result.results}
        assert any("alpha" in b for b in by_source), "alpha must be retrievable"
        assert any("beta" in b for b in by_source), "beta must be retrievable"

        for body, source_id in by_source.items():
            expected = "tenant-a" if "alpha" in body else "tenant-b"
            assert source_id == expected, f"{body!r} must report its OWN provenance"

        # Erasing one leaves the other addressable — proven by the second count.
        assert engine.erase_source("tenant-a").nodes_excised == 1
        assert engine.erase_source("tenant-b").nodes_excised == 1
    finally:
        engine.close()


def test_edge_hit_exposes_the_edges_own_source_id(db_path: str) -> None:
    """The edge-FTS arm carries the EDGE's own provenance, consistent with the
    graph arm's existing edge-source semantics."""
    engine = Engine.open(db_path)
    try:
        engine.write(
            [
                _node("anna the first entity", "tenant-e", logical_id="anna"),
                _node("bob the second entity", "tenant-e", logical_id="bob"),
                _edge("anna", "bob", "edge-ab", "tenant-e", "tc31pyedge anna trusts bob"),
            ]
        )

        result = engine.search("tc31pyedge")
        hits = [h for h in result.results if "tc31pyedge" in h.body]
        assert hits, "the edge-FTS arm must surface the edge body"
        assert hits[0].source_id == "tenant-e", (
            f"TC-31: an edge hit must carry the edge's own source_id (got {hits[0].source_id!r})"
        )

        report = engine.erase_source(hits[0].source_id)
        assert report.source_ref == "tenant-e"
    finally:
        engine.close()


def test_graph_arm_hit_source_id_semantics_unchanged(db_path: str) -> None:
    """Regression pin: TC-31 must NOT disturb the graph arm, which already
    carried the TRAVERSED EDGE's provenance."""
    engine = Engine.open(db_path)
    try:
        engine.write(
            [
                _node("carol tc31pyanchor entity", "tenant-g", logical_id="carol"),
                _node("tc31pygraph dave neighbor", "tenant-g", logical_id="dave"),
                _edge("carol", "dave", "edge-cd", "tenant-g", "carol knows dave"),
            ]
        )

        result = engine.search("tc31pyanchor", use_graph_arm=True)
        reached = [h for h in result.results if "tc31pygraph" in h.body]
        assert reached, "dave must be graph-reached from the carol seed"
        assert reached[0].source_id == "tenant-g"

        assert engine.erase_source(reached[0].source_id).source_ref == "tenant-g"
    finally:
        engine.close()
