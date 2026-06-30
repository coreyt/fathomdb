"""Tests for the P0-4 OPP-3 eval-support knobs (:mod:`eval.opp3_eval_support`).

Pure-stdlib module (no native import), so these run without the engine build /
``.venv`` binding — like ``test_decision_rule_083.py``.
"""

from __future__ import annotations

import pytest

from eval.opp3_eval_support import (
    Margins,
    RESERVED_POOL_KEYS,
    decide_per_corpus,
    demote_gold,
    inject_distractors,
    margins_from_search_result,
    margins_from_triples,
    nearest_rival_margin,
    top_gap,
)


# --------------------------------------------------------------------------- #
# 1. Distractor injection
# --------------------------------------------------------------------------- #


def test_inject_default_prepends_all_pushing_gold_down() -> None:
    ranked = ["g", "a", "b"]
    out = inject_distractors(ranked, ["d1", "d2"])
    assert out == ["d1", "d2", "g", "a", "b"]
    # gold "g" demoted from rank 0 to rank 2.
    assert out.index("g") == 2


def test_inject_positions_against_original_indices() -> None:
    ranked = ["a", "b", "c"]
    # both distractors at the very top, in order.
    assert inject_distractors(ranked, ["d1", "d2"], positions=[0, 0]) == [
        "d1",
        "d2",
        "a",
        "b",
        "c",
    ]
    # spread: index 0 and index 2 of the ORIGINAL list.
    assert inject_distractors(ranked, ["d1", "d2"], positions=[0, 2]) == [
        "d1",
        "a",
        "b",
        "d2",
        "c",
    ]


def test_inject_positions_clamped() -> None:
    assert inject_distractors(["a"], ["d"], positions=[99]) == ["a", "d"]


def test_inject_spacing() -> None:
    ranked = ["a", "b", "c", "d"]
    out = inject_distractors(ranked, ["x", "y"], spacing=2)
    # before idx 0, then before idx 2 (original).
    assert out == ["x", "a", "b", "y", "c", "d"]


def test_inject_spacing_leftover_appended() -> None:
    out = inject_distractors(["a"], ["x", "y", "z"], spacing=5)
    assert out == ["x", "a", "y", "z"]


def test_inject_skip_present_and_dedupe() -> None:
    ranked = ["a", "b"]
    # "a" already present -> dropped; "d" duplicated -> injected once.
    out = inject_distractors(ranked, ["a", "d", "d"])
    assert out == ["d", "a", "b"]


def test_inject_no_skip_keeps_duplicates() -> None:
    out = inject_distractors(["a"], ["a", "a"], skip_present=False)
    assert out == ["a", "a", "a"]


def test_inject_rejects_both_modes() -> None:
    with pytest.raises(ValueError):
        inject_distractors(["a"], ["d"], positions=[0], spacing=1)


def test_inject_rejects_bad_spacing() -> None:
    with pytest.raises(ValueError):
        inject_distractors(["a"], ["d"], spacing=0)


def test_inject_does_not_mutate_input() -> None:
    ranked = ["a", "b"]
    inject_distractors(ranked, ["d"])
    assert ranked == ["a", "b"]


# --------------------------------------------------------------------------- #
# 2. Gold-rank demotion
# --------------------------------------------------------------------------- #


def test_demote_by_shifts_top_gold_down() -> None:
    ranked = ["g", "a", "b", "c"]
    assert demote_gold(ranked, ["g"], by=2) == ["a", "b", "g", "c"]


def test_demote_by_clamped_to_end() -> None:
    assert demote_gold(["g", "a"], ["g"], by=10) == ["a", "g"]


def test_demote_by_zero_is_noop_copy() -> None:
    ranked = ["g", "a"]
    out = demote_gold(ranked, ["g"], by=0)
    assert out == ["g", "a"]
    assert out is not ranked


def test_demote_to_rank_absolute() -> None:
    assert demote_gold(["g", "a", "b"], ["g"], to_rank=2) == ["a", "b", "g"]


def test_demote_only_top_gold_when_multiple() -> None:
    # only the highest-ranked gold ("g1" at rank 0) moves; "g2" stays put.
    assert demote_gold(["g1", "a", "g2", "b"], ["g1", "g2"], by=1) == [
        "a",
        "g1",
        "g2",
        "b",
    ]


def test_demote_no_gold_present_returns_copy() -> None:
    ranked = ["a", "b"]
    out = demote_gold(ranked, ["g"], by=1)
    assert out == ["a", "b"]
    assert out is not ranked


def test_demote_to_rank_promotion_rejected() -> None:
    with pytest.raises(ValueError):
        demote_gold(["a", "g"], ["g"], to_rank=0)  # g at rank 1 -> 0 promotes


def test_demote_requires_exactly_one_mode() -> None:
    with pytest.raises(ValueError):
        demote_gold(["g"], ["g"])
    with pytest.raises(ValueError):
        demote_gold(["g"], ["g"], by=1, to_rank=1)


def test_demote_rejects_negative() -> None:
    with pytest.raises(ValueError):
        demote_gold(["g"], ["g"], by=-1)


# --------------------------------------------------------------------------- #
# 3. Per-corpus decision guard
# --------------------------------------------------------------------------- #


def test_decide_per_corpus_runs_each_independently() -> None:
    by_corpus = {"lme": 1, "locomo": 2, "musique": 3}
    out = decide_per_corpus(by_corpus, decide=lambda p: p * 10)
    assert out == {"lme": 10, "locomo": 20, "musique": 30}


def test_decide_per_corpus_rejects_pooled_key() -> None:
    for bad in RESERVED_POOL_KEYS:
        with pytest.raises(ValueError):
            decide_per_corpus({bad: 1, "lme": 2}, decide=lambda p: p)


def test_decide_per_corpus_pooled_key_case_insensitive() -> None:
    with pytest.raises(ValueError):
        decide_per_corpus({"POOLED": 1}, decide=lambda p: p)


def test_decide_per_corpus_empty() -> None:
    assert decide_per_corpus({}, decide=lambda p: p) == {}


# --------------------------------------------------------------------------- #
# 4. Margin measurements
# --------------------------------------------------------------------------- #


def test_top_gap() -> None:
    assert top_gap([0.9, 0.4, 0.1]) == pytest.approx(0.5)
    assert top_gap([0.9]) is None
    assert top_gap([]) is None


def test_nearest_rival_margin_positive_when_gold_on_top() -> None:
    ids = ["g", "d1", "d2"]
    scores = [0.9, 0.5, 0.3]
    assert nearest_rival_margin(ids, scores, ["g"]) == pytest.approx(0.4)


def test_nearest_rival_margin_negative_when_distractor_wins() -> None:
    ids = ["d1", "g"]
    scores = [0.8, 0.6]
    assert nearest_rival_margin(ids, scores, ["g"]) == pytest.approx(-0.2)


def test_nearest_rival_margin_none_without_gold_or_rival() -> None:
    assert nearest_rival_margin(["d"], [0.5], ["g"]) is None  # no gold present
    assert nearest_rival_margin(["g"], [0.5], ["g"]) is None  # no rival present


def test_nearest_rival_margin_parallel_length_checked() -> None:
    with pytest.raises(ValueError):
        nearest_rival_margin(["a", "b"], [0.1], ["a"])


def test_margins_from_triples_full() -> None:
    # ranked: g(0.9, ce .8), d1(0.5, ce .4), d2(0.3, ce None)
    triples = [("g", 0.9, 0.8), ("d1", 0.5, 0.4), ("d2", 0.3, None)]
    m = margins_from_triples(triples, ["g"])
    assert isinstance(m, Margins)
    assert m.top_gap == pytest.approx(0.4)  # 0.9 - 0.5
    assert m.gold_rival_margin == pytest.approx(0.4)  # 0.9 - 0.5
    # CE sub-pool = [g .8, d1 .4]; d2 (None) excluded.
    assert m.top_gap_ce == pytest.approx(0.4)  # 0.8 - 0.4
    assert m.gold_rival_margin_ce == pytest.approx(0.4)


def test_margins_ce_none_when_no_ce_scores() -> None:
    triples = [("g", 0.9, None), ("d1", 0.5, None)]
    m = margins_from_triples(triples, ["g"])
    assert m.top_gap == pytest.approx(0.4)
    assert m.top_gap_ce is None
    assert m.gold_rival_margin_ce is None


class _StubHit:
    def __init__(self, id: int, score: float, ce_score=None) -> None:
        self.id = id
        self.score = score
        self.ce_score = ce_score


class _StubResult:
    def __init__(self, results: list) -> None:
        self.results = results


def test_margins_from_search_result_reads_existing_fields() -> None:
    # mirrors the engine SearchResult: each hit carries .score and .ce_score.
    result = _StubResult(
        [
            _StubHit(id=10, score=0.9, ce_score=0.8),
            _StubHit(id=11, score=0.5, ce_score=0.4),
        ]
    )
    m = margins_from_search_result(result, gold_ids=["10"])
    assert m.top_gap == pytest.approx(0.4)
    assert m.gold_rival_margin == pytest.approx(0.4)
    assert m.top_gap_ce == pytest.approx(0.4)


def test_margins_from_search_result_custom_id_of() -> None:
    result = _StubResult([_StubHit(id=10, score=0.9), _StubHit(id=11, score=0.5)])
    m = margins_from_search_result(
        result, gold_ids=["doc-10"], id_of=lambda h: f"doc-{h.id}"
    )
    assert m.gold_rival_margin == pytest.approx(0.4)
    assert m.top_gap_ce is None  # no ce scores
