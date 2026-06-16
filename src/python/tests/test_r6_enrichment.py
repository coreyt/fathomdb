"""R6 index-key enrichment — acceptance tests (mechanism-level; design §D).

Pins the enrichment MECHANISM + fairness, NOT a recall threshold (the lift is the
experiment's output). Design: dev/plans/runs/0.8.1-R6-index-key-enrichment-design.md.
"""

from __future__ import annotations

from eval.r6_index_key_enrichment import build_fts_engine, enrich_doc, placebo_doc


# AC-R6-1 — enrich applies + deterministic + dedup -------------------------------
def test_ac_r6_1_enrich_applies_deterministic_dedup() -> None:
    g = {
        "entities": [{"name": "Cooper"}, {"name": "Cooper"}, {"name": "Dr. Lin"}],
        "relations": [{"subject": "Cooper", "predicate": "vet_is", "object": "Dr. Lin"}],
    }
    body = "I adopted a beagle last March."
    e1 = enrich_doc(body, g)
    e2 = enrich_doc(body, g)
    assert e1 == e2, "enrichment must be deterministic (byte-identical)"
    assert body in e1, "original body preserved"
    assert "Cooper" in e1 and "Dr. Lin" in e1, "entity names present"
    assert "vet_is" in e1, "relation predicate token present"
    ents_line = next(ln for ln in e1.splitlines() if ln.startswith("[entities]"))
    assert ents_line.count("Cooper") == 1, "duplicate entity names collapse (dedup, order-preserving)"


# AC-R6-2 — findability + hostile-name safe --------------------------------------
def test_ac_r6_2a_fact_only_token_becomes_present() -> None:
    g = {"entities": [{"name": "Zephyrine"}], "relations": []}
    body = "a session with no special token in it"
    e = enrich_doc(body, g)
    assert "Zephyrine" not in body and "Zephyrine" in e, "fact-only token becomes lexically present"


def test_ac_r6_2b_findability_through_engine(db_path: str) -> None:
    # The enriched doc is retrievable by a fact-only token the plain doc lacks.
    docs_plain = {"s1": "a conversation about weekend plans"}
    g = {"s1": {"entities": [{"name": "Patagonia"}], "relations": []}}
    eng, _ = build_fts_engine({k: enrich_doc(v, g.get(k, {})) for k, v in docs_plain.items()}, db_path)
    try:
        eng.drain(timeout_s=30)
        hits = eng.search("Patagonia").results
        assert any("weekend plans" in h.body for h in hits), "enriched doc retrievable by fact token"
    finally:
        eng.close()


def test_ac_r6_2c_hostile_entity_name_ingests(db_path: str) -> None:
    g = {"entities": [{"name": 'weird":*;[name] AND OR'}], "relations": []}
    enriched = enrich_doc("plain body text", g)
    eng, _ = build_fts_engine({"s1": enriched}, db_path)
    try:
        eng.drain(timeout_s=30)  # must not crash on FTS-hostile chars in indexed content
        _ = eng.search("plain body").results
    finally:
        eng.close()


# AC-R6-3 — no-op on empty graph -------------------------------------------------
def test_ac_r6_3_noop_on_empty_graph() -> None:
    assert enrich_doc("body text", {}) == "body text"
    assert enrich_doc("body text", {"entities": [], "relations": []}) == "body text"


# AC-R6-4 — placebo length-matched + one-row-per-doc ----------------------------
def test_ac_r6_4a_placebo_length_matched_foreign() -> None:
    g = {
        "entities": [{"name": "Cooper"}, {"name": "Lin"}],
        "relations": [{"subject": "Cooper", "predicate": "vet", "object": "Lin"}],
    }
    body = "short body here"
    real = enrich_doc(body, g)
    # Include a MULTI-WORD foreign item — placebo must tokenize it so length stays matched
    # (codex §9 [P2] #3); a naive per-item sample would over-add tokens.
    pool = ["alpha", "beta gamma", "delta epsilon zeta"]
    plac = placebo_doc(body, g, foreign=pool, seed=1)
    assert plac != body, "placebo adds content when real enrichment does"
    assert abs(len(plac.split()) - len(real.split())) <= 2, "placebo is length-matched (multi-word foreign tokenized)"
    assert "Cooper" not in plac and "Lin" not in plac and "vet" not in plac, "placebo content is foreign"
    assert placebo_doc(body, g, foreign=pool, seed=1) == plac, "deterministic (same inputs → same output)"


def test_ac_r6_4b_one_row_per_doc_no_entity_rows(db_path: str) -> None:
    import sqlite3

    docs = {"s1": "alpha doc", "s2": "beta doc", "s3": "gamma doc"}
    g = {s: {"entities": [{"name": f"E{s}"}], "relations": []} for s in docs}
    enriched = {k: enrich_doc(v, g[k]) for k, v in docs.items()}
    eng, _ = build_fts_engine(enriched, db_path)
    try:
        eng.drain(timeout_s=30)
    finally:
        eng.close()
    c = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    n_nodes = c.execute("SELECT COUNT(*) FROM canonical_nodes").fetchone()[0]
    n_entity_rows = c.execute("SELECT COUNT(*) FROM canonical_nodes WHERE logical_id IS NOT NULL").fetchone()[0]
    n_edges = c.execute("SELECT COUNT(*) FROM canonical_edges").fetchone()[0]
    c.close()
    assert n_nodes == len(docs), f"one row per doc (no separate entity rows); got {n_nodes}"
    assert n_entity_rows == 0, "enrichment writes NO separate entity rows (avoids length-norm-pollution path)"
    assert n_edges == 0, "enrichment writes NO edges"
