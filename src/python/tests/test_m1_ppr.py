"""Slice 15 — the four load-bearing properties of the lexically-seeded PPR-fusion
arm (AC-M1-15a..d; design ``dev/design/0.8.2-m1-multihop-harness.md`` §2/§6).

Small constructed fixtures (a tiny graph + ~4-passage pool) so the tests are fast,
offline, and **provable by construction** — no real 300-question extraction needed.

  (a) determinism      — identical (query, passages, extractions) → byte-identical
                          ranking (run twice, assert equal).
  (b) restart→1.0      — at teleport probability 1.0 (networkx ``alpha=0``) the PPR
                          mass collapses onto the seeds: every non-seed (propagated)
                          entity has EXACTLY zero mass, and the ppr-only top-K equals
                          the BM25 seed passages; at restart<1 the propagated
                          entities gain mass. Proves propagation is wired.
  (c) IDF-weighting    — toggling IDF/specificity weighting CHANGES the ranking on a
                          hub-vs-specific case (the knob is real, not a no-op).
  (d) fusion no-regress — RRF(BM25, PPR) never drops a BM25-top-K passage below the
                          floor it holds under BM25 alone (the ``pr_g9``-style pin).
"""

from __future__ import annotations

from eval.m1_baseline import Paragraph, bm25_rank
from eval.m1_ppr import DEFAULT_PPR_CONFIG, PPRConfig, retrieve_ppr


# --------------------------------------------------------------------------- #
# Fixture helpers
# --------------------------------------------------------------------------- #
def _para(i: int, text: str) -> Paragraph:
    return Paragraph(idx=i, title="", text=text, is_supporting=False)


def _ent(*names: str) -> dict:
    return {"entities": [{"name": n, "type": "X"} for n in names], "relations": []}


def _rel(s: str, o: str) -> dict:
    return {"subject": s, "predicate": "p", "object": o}


# A 4-passage pool whose BM25 top-2 is provably {0, 1} (query terms only in 0, 1).
_QUERY = "apple banana"
_PASSAGES = (
    _para(0, "apple banana apple banana"),
    _para(1, "apple banana"),
    _para(2, "cherry date cherry date"),
    _para(3, "elder fig elder fig"),
)


def _bm25_top2() -> set[int]:
    return set(bm25_rank(_QUERY, _PASSAGES)[:2])


# --------------------------------------------------------------------------- #
# (a) determinism — AC-M1-15a
# --------------------------------------------------------------------------- #
def test_ppr_determinism_byte_identical() -> None:
    assert _bm25_top2() == {0, 1}
    extractions = {
        0: {"entities": [{"name": "alice", "type": "X"}], "relations": []},
        1: {"entities": [{"name": "bob", "type": "X"}], "relations": []},
        2: {"entities": [{"name": "carol", "type": "X"}], "relations": [_rel("alice", "carol")]},
        3: {"entities": [{"name": "dave", "type": "X"}], "relations": [_rel("bob", "dave")]},
    }
    cfg = PPRConfig(seed_k=2)
    r1 = retrieve_ppr(_QUERY, _PASSAGES, extractions, cfg)
    r2 = retrieve_ppr(_QUERY, _PASSAGES, extractions, cfg)
    assert r1["ppr_only"] == r2["ppr_only"]
    assert r1["ppr_fusion"] == r2["ppr_fusion"]
    # byte-identical floats, not just ordering
    assert r1["passage_scores"] == r2["passage_scores"]
    assert r1["ppr_mass"] == r2["ppr_mass"]


# --------------------------------------------------------------------------- #
# (b) restart→1.0 collapse — AC-M1-15b
# --------------------------------------------------------------------------- #
def test_restart_one_collapses_to_seeds() -> None:
    # Seeds (BM25 top-2 = passages 0,1) = {alice, bob}. carol/dave are reachable
    # ONLY by propagation (their connecting relation lives in their own non-seed
    # passage, so membership keeps them out of the seed set).
    assert _bm25_top2() == {0, 1}
    extractions = {
        0: _ent("alice"),
        1: _ent("bob"),
        2: {"entities": [{"name": "carol", "type": "X"}], "relations": [_rel("alice", "carol")]},
        3: {"entities": [{"name": "dave", "type": "X"}], "relations": [_rel("bob", "dave")]},
    }
    collapse = retrieve_ppr(_QUERY, _PASSAGES, extractions, PPRConfig(seed_k=2, alpha=0.0))
    propagate = retrieve_ppr(_QUERY, _PASSAGES, extractions, PPRConfig(seed_k=2, alpha=0.85))

    # restart=1.0: propagated (non-seed) entities have EXACTLY zero mass.
    assert collapse["ppr_mass"].get("carol", 0.0) == 0.0
    assert collapse["ppr_mass"].get("dave", 0.0) == 0.0
    # seeds carry all the mass.
    assert collapse["ppr_mass"].get("alice", 0.0) > 0.0
    assert collapse["ppr_mass"].get("bob", 0.0) > 0.0
    # ppr-only top-2 == the BM25 seed passages.
    assert set(collapse["ppr_only"][:2]) == {0, 1}

    # restart<1: propagation gives the reachable entities strictly positive mass.
    assert propagate["ppr_mass"].get("carol", 0.0) > 0.0
    assert propagate["ppr_mass"].get("dave", 0.0) > 0.0


# --------------------------------------------------------------------------- #
# (c) IDF-weighting is live — AC-M1-15c
# --------------------------------------------------------------------------- #
def test_idf_weighting_changes_ranking() -> None:
    # hub appears in every passage (df=4 → low specificity); alice/bob/carol/dave
    # appear once (df=1 → high specificity). Two symmetric graph components:
    #   {hub - carol}  and  {alice - dave};  bob is isolated.
    # Under UNIFORM seed weights hub==alice ⇒ mass(carol)==mass(dave) (symmetry) ⇒
    # passage2 ties passage3 (broken to index order 2<3). Under IDF weights
    # alice >> hub ⇒ mass(dave) >> mass(carol) ⇒ passage3 outranks passage2. The
    # 2/3 swap makes the two ppr-only rankings differ.
    assert _bm25_top2() == {0, 1}
    extractions = {
        0: _ent("hub", "alice"),
        1: _ent("hub", "bob"),
        2: {"entities": [{"name": "hub", "type": "X"}, {"name": "carol", "type": "X"}],
            "relations": [_rel("hub", "carol")]},
        3: {"entities": [{"name": "hub", "type": "X"}, {"name": "dave", "type": "X"}],
            "relations": [_rel("alice", "dave")]},
    }
    idf_on = retrieve_ppr(_QUERY, _PASSAGES, extractions, PPRConfig(seed_k=2, idf_weighting=True))
    idf_off = retrieve_ppr(_QUERY, _PASSAGES, extractions, PPRConfig(seed_k=2, idf_weighting=False))
    assert idf_on["ppr_only"] != idf_off["ppr_only"]


# --------------------------------------------------------------------------- #
# (d) fusion no-regression — AC-M1-15d
# --------------------------------------------------------------------------- #
def test_fusion_no_regression_vs_bm25_floor() -> None:
    # Passage0 carries TWO isolated seed entities (max direct mass, no leak);
    # passage1 carries one seed that leaks to the non-seed passages 2,3. So both
    # BM25 and PPR rank the seed passages 0,1 above 2,3 AND agree on 0<1 — RRF
    # therefore cannot demote a BM25-top-K passage below its BM25-alone rank.
    bm = bm25_rank(_QUERY, _PASSAGES)
    assert bm[:2] == [0, 1]
    extractions = {
        0: _ent("a0", "a1"),
        1: _ent("b0"),
        2: {"entities": [{"name": "c0", "type": "X"}], "relations": [_rel("b0", "c0")]},
        3: {"entities": [{"name": "d0", "type": "X"}], "relations": [_rel("b0", "d0")]},
    }
    res = retrieve_ppr(_QUERY, _PASSAGES, extractions, PPRConfig(seed_k=2))
    fused = res["ppr_fusion"]
    bm_rank = {p: r for r, p in enumerate(bm)}
    fused_rank = {p: r for r, p in enumerate(fused)}
    top_k = set(bm[:2])
    for p in top_k:
        assert fused_rank[p] <= bm_rank[p], (
            f"passage {p} dropped from BM25 rank {bm_rank[p]} to fused rank {fused_rank[p]}"
        )


# --------------------------------------------------------------------------- #
# default config sanity
# --------------------------------------------------------------------------- #
def test_default_config_shape() -> None:
    assert DEFAULT_PPR_CONFIG.alpha == 0.85
    assert DEFAULT_PPR_CONFIG.idf_weighting is True
    assert DEFAULT_PPR_CONFIG.seed_k >= 1
