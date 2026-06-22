"""Slice-15b — D2 content-at-scale proxy: pure-logic acceptance tests.

Pins the MECHANISM + the gate wiring (NOT a recall threshold — the lift is the
experiment's output): the truth-table over the REAL frozen ``probe_15b_pass``, the
key-set↔placebo invariant for every key type, the deterministic paired bootstrap CI
+ MDE, the TOST ``removes_length_norm_penalty`` rule (incl. underpowered ⇒
INCONCLUSIVE), and the 3-way verdict mapping.

All tests are pure-Python (stdlib + pure BM25); no native extension, no network, no
GPU. Run: ``FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=src/python .venv/bin/python -m
pytest src/python/tests/test_s15b_d2_proxy.py -q -p no:cacheprovider``.
"""

from __future__ import annotations

import math

import pytest

from eval.decision_rule_083 import probe_15b_pass
from eval.s15b_d2_proxy import (
    DELTA_D2,
    build_enriched_placebo,
    classify_verdict,
    compute_proxy,
    neutralization_classify,
    paired_bootstrap_ci,
    paired_deltas,
    paired_mde,
    per_query_recall,
    removes_length_norm_status,
)
from eval.s15b_d2_proxy import _key_tokens, _smoke_corpus  # noqa: PLC2701 - internal helpers under test
from eval.r2_parity_eval import NaiveRAGAdapter
from eval.r6_index_key_enrichment import enrich_doc


# --------------------------------------------------------------------------- #
# (a) probe wiring truth-table via the REAL frozen probe_15b_pass.
# --------------------------------------------------------------------------- #
def test_a_probe_pass_when_both_criteria_hold() -> None:
    enriched = {"recall": 0.55, "margin_ci_lo": 0.04, "removes_length_norm_penalty": True}
    placebo = {"recall": 0.50}
    assert probe_15b_pass(enriched, placebo) is True


def test_a_probe_fails_on_margin_ci_not_positive() -> None:
    # CI lower bound exactly 0 is NOT > 0 (no raw-point gate; CI-lower-bound only).
    placebo = {"recall": 0.50}
    assert probe_15b_pass(
        {"recall": 0.55, "margin_ci_lo": 0.0, "removes_length_norm_penalty": True}, placebo
    ) is False
    assert probe_15b_pass(
        {"recall": 0.55, "margin_ci_lo": -0.01, "removes_length_norm_penalty": True}, placebo
    ) is False


def test_a_probe_fails_on_removes_flag_false() -> None:
    enriched = {"recall": 0.55, "margin_ci_lo": 0.04, "removes_length_norm_penalty": False}
    assert probe_15b_pass(enriched, {"recall": 0.50}) is False


def test_a_probe_raises_on_non_finite_endpoint() -> None:
    with pytest.raises(ValueError):
        probe_15b_pass(
            {"recall": 0.5, "margin_ci_lo": math.nan, "removes_length_norm_penalty": True},
            {"recall": 0.5},
        )


# --------------------------------------------------------------------------- #
# (b) key-set ↔ placebo invariant — length-matched + foreign-only + own-EVERY-
#     key-type-excluded (entities AND facts), not just entities.
# --------------------------------------------------------------------------- #
def test_b_placebo_matches_full_key_set_every_type() -> None:
    documents = {
        "s1": "short body one",
        "s2": "short body two",
        "s3": "short body three",
    }
    graphs = {
        "s1": {
            "entities": [{"name": "Cooper"}, {"name": "Lin"}],
            "relations": [{"subject": "Cooper", "predicate": "treats", "object": "Lin"}],
        },
        "s2": {
            "entities": [{"name": "Patagonia"}],
            "relations": [{"subject": "Patagonia", "predicate": "near", "object": "Andes"}],
        },
        "s3": {
            "entities": [{"name": "Zephyr"}],
            "relations": [{"subject": "Zephyr", "predicate": "owns", "object": "Boat"}],
        },
    }
    enriched, placebo = build_enriched_placebo(documents, graphs, seed=7)

    e1 = enriched["s1"]
    p1 = placebo["s1"]
    body1 = documents["s1"]

    # Length-matched to the FULL enriched addition (entities AND fact triples), not
    # just to the entity block.
    assert abs(len(p1.split()) - len(e1.split())) <= 2, "placebo length-matched to full key set"

    # The full key set includes a fact token ("treats") not present as an entity —
    # the enriched arm must contain it (proves facts are in the key set).
    assert "treats" in e1, "fact predicate is part of the enriched key set"

    # Foreign-only w.r.t. EVERY own key token (entities + fact subject/pred/object).
    own_tokens = set(_key_tokens(graphs["s1"]))
    assert own_tokens, "fixture has own key tokens"
    placebo_added = set(p1.split()) - set(body1.split())
    assert own_tokens.isdisjoint(placebo_added), (
        "placebo excludes the doc's OWN key tokens of every type (entities + facts)"
    )
    # Deterministic.
    e2, p2 = build_enriched_placebo(documents, graphs, seed=7)
    assert p2["s1"] == p1 and e2["s1"] == e1, "build is deterministic given seed"


def test_b_placebo_noop_when_no_keys() -> None:
    documents = {"s1": "a plain body", "s2": "another plain body"}
    graphs: dict[str, dict] = {"s1": {}, "s2": {}}
    enriched, placebo = build_enriched_placebo(documents, graphs, seed=1)
    assert enriched["s1"] == documents["s1"], "no enrichment on empty graph"
    assert placebo["s1"] == documents["s1"], "no placebo content on empty graph"


# --------------------------------------------------------------------------- #
# (c) paired bootstrap CI + MDE — deterministic, fixed-seed.
# --------------------------------------------------------------------------- #
def test_c_paired_deltas_pairs_by_key_and_rejects_empty() -> None:
    assert paired_deltas({"a": 0.5, "b": 0.2}, {"a": 0.3, "b": 0.2}) == [0.2, 0.0]
    with pytest.raises(ValueError):
        paired_deltas({"a": 1.0}, {"b": 1.0})


def test_c_ci_positive_for_positive_constant_delta() -> None:
    deltas = [0.1] * 12
    lo, hi = paired_bootstrap_ci(deltas, seed=123, n_boot=2000)
    assert lo > 0.0 and abs(lo - 0.1) < 1e-9 and abs(hi - 0.1) < 1e-9


def test_c_ci_not_positive_for_zero_and_negative_delta() -> None:
    lo_zero, _ = paired_bootstrap_ci([0.0] * 8, seed=1, n_boot=2000)
    assert lo_zero <= 0.0
    lo_neg, _ = paired_bootstrap_ci([-0.1] * 8, seed=1, n_boot=2000)
    assert lo_neg <= 0.0


def test_c_ci_deterministic_same_seed() -> None:
    deltas = [0.2, 0.0, 0.1, 0.3, -0.1, 0.2, 0.0, 0.4, 0.1, 0.2]
    a = paired_bootstrap_ci(deltas, seed=42, n_boot=3000)
    b = paired_bootstrap_ci(deltas, seed=42, n_boot=3000)
    assert a == b, "same seed → identical bounds"


def test_c_mde_zero_variance_and_positive() -> None:
    assert paired_mde([0.1] * 10) == 0.0, "zero variance ⇒ MDE 0"
    mde = paired_mde([0.5, -0.5] * 10)
    # MDE = Z·sqrt(var/n); var=0.25, n=20 → 1.96·sqrt(0.0125) ≈ 0.2191.
    assert abs(mde - 1.96 * math.sqrt(0.25 / 20)) < 1e-9
    assert mde > DELTA_D2, "high-variance contrast is underpowered for δ_d2"


# --------------------------------------------------------------------------- #
# (d) removes_length_norm_penalty TOST rule incl. underpowered ⇒ INCONCLUSIVE.
# --------------------------------------------------------------------------- #
def test_d_neutralization_classify_three_branches() -> None:
    # Fully contained in [−δ, +δ] ⇒ neutralized (equivalence demonstrated).
    assert neutralization_classify(-0.01, 0.01, 0.02, delta_d2=DELTA_D2) == "neutralized"
    # Outside the band at adequate power ⇒ a real residual ⇒ not_neutralized.
    assert neutralization_classify(0.08, 0.12, 0.02, delta_d2=DELTA_D2) == "not_neutralized"
    # Not contained AND underpowered (mde > δ) ⇒ inconclusive (never silent True).
    assert neutralization_classify(-0.20, 0.22, 0.21, delta_d2=DELTA_D2) == "inconclusive"


def test_d_removes_status_combines_legs() -> None:
    assert removes_length_norm_status(False, "not_neutralized") == "removed", "no penalty ⇒ removed"
    assert removes_length_norm_status(True, "neutralized") == "removed"
    assert removes_length_norm_status(True, "not_neutralized") == "not_removed"
    assert removes_length_norm_status(True, "inconclusive") == "inconclusive", (
        "underpowered neutralization never silently becomes removed"
    )


def test_d_underpowered_contrast_classifies_inconclusive_via_bootstrap() -> None:
    # A high-variance, mean≈0 contrast: CI wide, mde > δ ⇒ inconclusive end-to-end.
    deltas = [0.5, -0.5] * 12
    lo, hi = paired_bootstrap_ci(deltas, seed=9, n_boot=3000)
    mde = paired_mde(deltas)
    assert mde > DELTA_D2
    assert not (lo >= -DELTA_D2 and hi <= DELTA_D2), "CI too wide to fit the band"
    assert neutralization_classify(lo, hi, mde, delta_d2=DELTA_D2) == "inconclusive"


def test_d_neutralized_contrast_via_bootstrap() -> None:
    deltas = [0.0] * 16
    lo, hi = paired_bootstrap_ci(deltas, seed=3, n_boot=2000)
    mde = paired_mde(deltas)
    assert neutralization_classify(lo, hi, mde, delta_d2=DELTA_D2) == "neutralized"


# --------------------------------------------------------------------------- #
# (e) 3-way verdict mapping — routed through the REAL probe_15b_pass.
# --------------------------------------------------------------------------- #
def test_e_verdict_pass() -> None:
    assert classify_verdict(0.05, 0.01, "removed") == "PASS"


def test_e_verdict_fail_at_power_on_margin() -> None:
    # Not significant (ci_lo ≤ 0) AND powered (mde ≤ δ) ⇒ FAIL (Slice-25 defers).
    assert classify_verdict(-0.01, 0.01, "removed") == "FAIL"


def test_e_verdict_fail_on_removes_false_at_power() -> None:
    assert classify_verdict(0.05, 0.01, "not_removed") == "FAIL"


def test_e_verdict_inconclusive_margin_underpowered() -> None:
    # Not significant but MDE > δ ⇒ INCONCLUSIVE, never FAIL.
    assert classify_verdict(-0.01, 0.10, "removed") == "INCONCLUSIVE"


def test_e_verdict_inconclusive_neutralization_overrides_fail() -> None:
    # removes inconclusive ⇒ INCONCLUSIVE even when the margin alone would be FAIL.
    assert classify_verdict(-0.01, 0.01, "inconclusive") == "INCONCLUSIVE"
    assert classify_verdict(0.05, 0.01, "inconclusive") == "INCONCLUSIVE"


# --------------------------------------------------------------------------- #
# (f) end-to-end pure wiring on the synthetic smoke corpus (BM25, no native).
# --------------------------------------------------------------------------- #
def test_f_compute_proxy_end_to_end_synthetic() -> None:
    smoke, graphs = _smoke_corpus()
    enriched, placebo = build_enriched_placebo(dict(smoke.documents), graphs, seed=11)

    # Sanity: enrichment makes the entity-only query token lexically present.
    s0 = sorted(smoke.documents)[0]
    ent0 = graphs[s0]["entities"][0]["name"]
    assert ent0 not in smoke.documents[s0] and ent0 in enrich_doc(smoke.documents[s0], graphs[s0])

    enriched_pq = per_query_recall(smoke, NaiveRAGAdapter(enriched))
    placebo_pq = per_query_recall(smoke, NaiveRAGAdapter(placebo))
    plain_bhi = per_query_recall(smoke, NaiveRAGAdapter(dict(smoke.documents), b=0.75))
    placebo_bhi = per_query_recall(smoke, NaiveRAGAdapter(placebo, b=0.75))
    plain_blo = per_query_recall(smoke, NaiveRAGAdapter(dict(smoke.documents), b=0.0))
    placebo_blo = per_query_recall(smoke, NaiveRAGAdapter(placebo, b=0.0))

    proxy = compute_proxy(
        enriched_pq=enriched_pq,
        placebo_pq=placebo_pq,
        plain_bhi_pq=plain_bhi,
        placebo_bhi_pq=placebo_bhi,
        plain_blo_pq=plain_blo,
        placebo_blo_pq=placebo_blo,
        seed=11,
        n_boot=2000,
    )
    assert proxy["verdict"] in {"PASS", "FAIL", "INCONCLUSIVE"}
    # The enriched arm recalls the entity-only token; placebo does not.
    assert proxy["criterion_1_content"]["enriched_recall"] > proxy["criterion_1_content"]["placebo_recall"]
    # probe_15b_pass echo is consistent with the verdict (None only when inconclusive removes).
    if proxy["verdict"] == "PASS":
        assert proxy["probe_15b_pass"] is True
