"""Slice 5 (0.8.12 / OPP-6 EXP-COV) — unit tests for the coverage-census metric.

Pins the pure metric functions of ``eval.exp_cov_census`` (canonicalization, alias
resolution, the C0-floor heuristic, and scoring) on tiny constructed fixtures so a
future edit cannot silently change how coverage is computed. These tests do NOT read
the gitignored EVAL-ONLY corpora and do NOT import GLiNER — they exercise the
deterministic `$0` core only.
"""

from __future__ import annotations

from eval import exp_cov_census as C


def test_canon_accent_and_case_fold() -> None:
    assert C.canon("Renée Vasquez") == C.canon("renee vasquez")
    assert C.canon("  Acme   Corp ") == "acme corp"
    assert C.canon_relation("Works At") == "works_at"
    assert C.canon_relation("located-in") == "located_in"


def test_alias_map_resolves_alias_to_canonical_name() -> None:
    ents = [{"name": "Acme Robotics", "type": "Organization", "aliases": ["Acme"]}]
    amap = C._alias_map(ents)
    assert C._resolve("Acme", amap) == "acme robotics"
    assert C._resolve("acme robotics", amap) == "acme robotics"
    # an unknown surface resolves to its own canon form (no crash)
    assert C._resolve("Globex", amap) == "globex"


def test_gold_to_extraction_builds_triples_and_pairs() -> None:
    doc = {
        "entities": [
            {"name": "Alice", "type": "Person", "aliases": []},
            {"name": "Acme Corp", "type": "Organization", "aliases": ["Acme"]},
        ],
        "edges": [
            {"from": "Alice", "to": "Acme", "relation": "works_at"},
        ],
    }
    ex = C.gold_to_extraction(doc)
    assert len(ex.entity_keysets) == 2
    # the edge endpoint "Acme" is resolved through the alias map to "acme corp"
    assert ("alice", "works_at", "acme corp") in ex.edge_triples
    assert ("alice", "acme corp") in ex.edge_pairs


def test_score_perfect_and_partial() -> None:
    gold = C.Extraction(
        entity_keysets=[frozenset({"alice"}), frozenset({"acme corp"})],
        edge_triples={("alice", "works_at", "acme corp")},
        edge_pairs={("alice", "acme corp")},
    )
    # perfect prediction
    c = C.score_doc(gold, gold)
    assert c.entity_recall() == 1.0
    assert c.edge_recall() == 1.0
    assert c.entity_precision() == 1.0
    # relation-label divergence: right endpoints, wrong label
    pred = C.Extraction(
        entity_keysets=[frozenset({"alice"}), frozenset({"acme corp"})],
        edge_triples={("alice", "employed_by", "acme corp")},
        edge_pairs={("alice", "acme corp")},
    )
    c2 = C.score_doc(gold, pred)
    assert c2.edge_recall() == 0.0  # strict triple misses
    assert c2.pair_recall() == 1.0  # endpoint pair matches


def test_c0_floor_finds_entities_no_relations() -> None:
    body = "Alice joined Acme Corp. Bob left Globex."
    amap = {}
    ex = C.c0_floor_extraction(body, amap)
    keys = {next(iter(k)) for k in ex.entity_keysets}
    assert "alice" in keys and "bob" in keys
    assert "acme corp" in keys
    # heuristic emits co_occurs edges, never a real relation label
    assert all(rel == "co_occurs" for (_f, rel, _t) in ex.edge_triples)
    # sentence-initial pronoun / stopword caps are filtered
    ex2 = C.c0_floor_extraction("The meeting is Monday.", {})
    keys2 = {next(iter(k)) for k in ex2.entity_keysets}
    assert "the" not in keys2 and "monday" not in keys2


def test_underpowered_flag() -> None:
    c = C.Counts(gold_edges=5)
    assert c.gold_edges < C.POWER_MIN
