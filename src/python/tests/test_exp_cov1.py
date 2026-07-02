"""Unit tests for the EXP-COV-1 pure core (no corpora / no fathomdb / no network)."""

from __future__ import annotations

import json

import pytest

from eval.exp_cov1_common import (
    PROMPT_VERSION,
    SUFFICIENCY_CI_LO_THRESHOLD,
    DollarLedger,
    ExtractionCache,
    c0_floor_extract,
    cache_key,
    decision_for_class,
    hit_at_k,
    overall_verdict,
    paired_bootstrap_delta,
    reciprocal_rank,
    usage_cost,
)


# ------------------------- heuristic extractor ------------------------------ #
def test_c0_floor_shape_and_edges():
    out = c0_floor_extract("d1", "Alice met Bob in Paris. Bob likes Tea.", "2023-01-01")
    assert set(out) == {"entities", "edges", "warnings"}
    names = {e["name"] for e in out["entities"]}
    assert "Alice" in names and "Bob" in names and "Paris" in names
    # co-occurrence edges within a sentence, extract.v1 shape
    assert out["edges"], "expected at least one co_occurs edge"
    e0 = out["edges"][0]
    assert set(e0) >= {"from_entity", "to_entity", "relation", "source_doc_id"}
    assert e0["relation"] == "co_occurs"
    assert e0["source_doc_id"] == "d1"


def test_c0_floor_deterministic():
    a = c0_floor_extract("d", "Carol works at Acme. Carol lives in Rome.", "t")
    b = c0_floor_extract("d", "Carol works at Acme. Carol lives in Rome.", "t")
    assert a == b


# ------------------------------- scorers ------------------------------------ #
def test_hit_at_k_multi_session_needs_full_set():
    # multi_session requires ALL gold in top-k
    assert hit_at_k(["a", "b"], ["a", "x", "y"], 3, "multi_session") == 0.0
    assert hit_at_k(["a", "b"], ["a", "b", "y"], 3, "multi_session") == 1.0
    # other classes: any-hit
    assert hit_at_k(["a", "b"], ["a", "x"], 2, "temporal") == 1.0
    # abstention -> None
    assert hit_at_k([], ["a"], 2, "temporal") is None


def test_reciprocal_rank():
    assert reciprocal_rank(["b"], ["a", "b", "c"]) == pytest.approx(0.5)
    assert reciprocal_rank(["z"], ["a", "b"]) == 0.0
    assert reciprocal_rank([], ["a"]) is None


# --------------------------- paired bootstrap ------------------------------- #
def test_paired_bootstrap_alignment_guard():
    with pytest.raises(ValueError):
        paired_bootstrap_delta([1.0, 0.0], [1.0])


def test_paired_bootstrap_positive_delta():
    # treatment strictly better on every paired unit -> point == +1, lo > 0
    t = [1.0] * 40
    b = [0.0] * 40
    d = paired_bootstrap_delta(t, b)
    assert d["point"] == pytest.approx(1.0)
    assert d["lo"] > SUFFICIENCY_CI_LO_THRESHOLD
    assert d["n"] == 40


def test_paired_bootstrap_zero_delta_ci_spans_zero():
    t = [1.0, 0.0] * 20
    b = [1.0, 0.0] * 20
    d = paired_bootstrap_delta(t, b)
    assert d["point"] == pytest.approx(0.0)
    assert d["lo"] == 0.0 and d["hi"] == 0.0


# ---------------------------- decision rule --------------------------------- #
def test_decision_rule():
    assert decision_for_class({"lo": 0.10}, n_scored=100) == "SUFFICIENT"
    assert decision_for_class({"lo": 0.00}, n_scored=100) == "CEILING_ABSORBED"
    assert decision_for_class({"lo": 0.10}, n_scored=5) == "UNDERPOWERED"
    assert decision_for_class({"lo": None}, n_scored=100) == "NO_DATA"


def test_overall_verdict():
    assert overall_verdict({"a": "CEILING_ABSORBED", "b": "SUFFICIENT"}) == "SUFFICIENT"
    assert overall_verdict({"a": "CEILING_ABSORBED"}) == "CEILING_ABSORBED"
    assert overall_verdict({"a": "UNDERPOWERED"}) == "INCONCLUSIVE"


# --------------------------- ledger auto-stop ------------------------------- #
def test_usage_cost_and_fallback_never_undercounts():
    # known cheap model
    c = usage_cost("gemini-3.1-flash-lite", 1_000_000, 1_000_000)
    assert c == pytest.approx(0.10 + 0.40)
    # unknown model falls back to the most-expensive row (never under-count)
    c2 = usage_cost("some-unknown-model", 1_000_000, 0)
    assert c2 == pytest.approx(15.00)


def test_ledger_ceiling_and_persist(tmp_path):
    p = tmp_path / "ledger.ndjson"
    led = DollarLedger.load(p, ceiling=1.0)
    led.add(doc_id="d1", model="gpt-5", prompt_tokens=100_000, completion_tokens=10_000)
    # reload from disk == same running total (persistence)
    led2 = DollarLedger.load(p, ceiling=1.0)
    assert led2.total == pytest.approx(led.total)
    assert led2.total > 0
    # a projected next call that would breach the ceiling is caught
    assert led2.would_exceed(5.0) is True


# --------------------------- cache resume/atomic ---------------------------- #
def test_cache_resume_and_completeness(tmp_path):
    p = tmp_path / "cache.ndjson"
    cache = ExtractionCache.load(p)
    k1 = cache_key("d1", "m", PROMPT_VERSION)
    k2 = cache_key("d2", "m", PROMPT_VERSION)
    cache.put({"key": k1, "doc_id": "d1", "model": "m", "prompt_version": PROMPT_VERSION,
               "status": "ok", "entities": [], "edges": [], "warnings": []})
    # a failed unit is RECORDED, not silently dropped
    cache.put({"key": k2, "doc_id": "d2", "model": "m", "prompt_version": PROMPT_VERSION,
               "status": "failed", "entities": [], "edges": [], "warnings": []})
    # reload (resume): ok survives, failed present-but-not-ok
    c2 = ExtractionCache.load(p)
    assert c2.has_ok(k1) is True
    assert c2.has_ok(k2) is False
    comp = c2.completeness([k1, k2])
    assert comp["ok"] is True  # nothing MISSING (present-or-failed)
    assert comp["n_ok"] == 1 and comp["n_failed"] == 1
    # a truly missing key makes completeness refuse
    comp2 = c2.completeness([k1, k2, cache_key("d3", "m", PROMPT_VERSION)])
    assert comp2["ok"] is False and comp2["n_missing"] == 1


def test_cache_atomic_rewrite_is_valid_ndjson(tmp_path):
    p = tmp_path / "cache.ndjson"
    cache = ExtractionCache.load(p)
    for i in range(5):
        cache.put({"key": f"k{i}", "doc_id": f"d{i}", "model": "m",
                   "prompt_version": PROMPT_VERSION, "status": "ok",
                   "entities": [], "edges": [], "warnings": []})
    lines = [ln for ln in p.read_text().splitlines() if ln.strip()]
    assert len(lines) == 5
    for ln in lines:
        json.loads(ln)  # each line is valid JSON
