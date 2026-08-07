"""S2 metric-port tests — written RED, before `eval.earp.metrics` exists.

Parity is asserted against values the Rust reference ACTUALLY PRODUCED. The
vectors in `fixtures/ir_parity_vectors.json` were emitted by running
`ir_eval.rs` over a fixed case list; nothing here re-derives an expectation in
Python, which would pass by construction and prove nothing.

The case list deliberately includes six behaviours the reference's own test
suite does not cover -- each a plausible reimplementation choice that would
otherwise pass unnoticed. They are marked `blind.*` in the fixture.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

import pytest

from eval.earp.depth import check_depth
from eval.earp.gold import EvidenceUnit, GoldQuery
from eval.earp.metrics import (
    ClassAgg,
    aggregate,
    evidence_recall_at_k,
    negative_abstained,
    resolve_ndcg,
    round4,
    validate_methodology,
)
from eval.earp.schema.models import BlockerCode, MetricStatus, RetrievalMode

FIXTURES = Path(__file__).parent / "fixtures"
VECTORS = json.loads((FIXTURES / "ir_parity_vectors.json").read_text(encoding="utf-8"))

#: The reference file the vectors were generated against, after the
#: `supporting_coverage` fix. Prose citation is not a drift detector; a hash is.
REFERENCE_PATH = (
    Path(__file__).parents[4]
    / "src/rust/crates/fathomdb-engine/tests/support/ir_eval.rs"
)
REFERENCE_SHA256_POSTFIX = "14619d1970f53a844038904dac828a1d841e7d2f95e81834f3da141c28a76e33"
REFERENCE_SHA256_PREMERGE = "00a607d029a32a4dfaca54b9e23d0727b9705c0d69c65a3d1810e5995e1ef773"


def _query_from(spec: dict[str, Any]) -> GoldQuery:
    return GoldQuery(
        query_id=spec["query_id"],
        query=spec["query"],
        query_class=spec["query_class"],
        required_evidence=tuple(
            EvidenceUnit(
                evidence_id=u["evidence_id"],
                doc_id=u["doc_id"],
                necessity=u["necessity"],
                locator=u.get("locator"),
            )
            for u in spec["required_evidence"]
        ),
        expected_top_k_doc_ids=tuple(spec["expected_top_k_doc_ids"]),
    )


def _case_ids() -> list[str]:
    return [c["case_id"] for c in VECTORS["per_query"]]


# --- AC-1: executed parity, every case ------------------------------------


@pytest.mark.parametrize("case", VECTORS["per_query"], ids=_case_ids())
def test_per_query_matches_the_reference(case: dict[str, Any]) -> None:
    got = evidence_recall_at_k(
        _query_from(case["gold_query"]), list(case["retrieved"]), case["k"]
    )
    want = case["expected"]
    assert got.strict == want["strict"]
    assert got.graded == pytest.approx(want["graded"], abs=0.0)
    assert got.supporting_coverage == want["supporting_coverage"]
    assert got.required_n == want["required_n"]
    assert got.required_hits == want["required_hits"]


@pytest.mark.parametrize(
    "case", VECTORS["negative_abstained"], ids=lambda c: c["case_id"]
)
def test_negative_abstained_matches_the_reference(case: dict[str, Any]) -> None:
    assert negative_abstained(list(case["retrieved"]), case["k"]) is case["expected"]


def test_aggregate_matches_the_reference() -> None:
    """The aggregate comes from the reference's PUBLIC evaluate_gold_set path,
    so this pins the same loop S6 will drive."""
    want = VECTORS["class_agg"]
    queries = [
        GoldQuery(
            query_id="a1",
            query="q",
            query_class="commitment",
            required_evidence=(
                EvidenceUnit("e1", "d1", "required"),
                EvidenceUnit("e2", "d2", "required"),
            ),
        ),
        GoldQuery(
            query_id="a2",
            query="q",
            query_class="exact_fact",
            required_evidence=(
                EvidenceUnit("e1", "d1", "required"),
                EvidenceUnit("e2", "d2", "supporting"),
            ),
        ),
        GoldQuery(query_id="a3", query="q", query_class="negative", required_evidence=()),
    ]
    retrieved = {"a1": ["d1", "d2"], "a2": ["d1"], "a3": []}
    result = aggregate(queries, lambda q: retrieved[q.query_id], k=want["k"])

    assert result.overall.n == want["overall"]["n"]
    assert result.overall.strict() == want["overall"]["strict"]
    assert result.overall.graded() == want["overall"]["graded"]
    assert result.overall.supporting() == want["overall"]["supporting"]
    assert result.overall.supporting_query_n == want["overall"]["supporting_query_n"]
    assert result.negative.n == want["negative"]["n"]
    assert result.negative.abstained == want["negative"]["abstained"]
    assert result.negative.false_positive_rate() == want["negative"]["false_positive_rate"]

    for expected_class in want["per_class"]:
        agg = result.per_class[expected_class["class"]]
        assert agg.n == expected_class["n"]
        assert agg.supporting() == expected_class["supporting"]
        assert agg.supporting_query_n == expected_class["supporting_query_n"]


def test_per_class_iterates_in_reference_discriminant_order() -> None:
    """The reference keys per_class in a BTreeMap, which iterates by
    discriminant -- not alphabetically."""
    queries = [
        GoldQuery(
            query_id=f"q{i}",
            query="q",
            query_class=cls,
            required_evidence=(EvidenceUnit("e", "d", "required"),),
        )
        for i, cls in enumerate(["exploratory", "commitment", "exact_fact"])
    ]
    result = aggregate(queries, lambda q: ["d"], k=10)
    assert list(result.per_class) == ["commitment", "exact_fact", "exploratory"]


# --- AC-2: the drift detector ---------------------------------------------


def test_reference_has_not_drifted() -> None:
    digest = hashlib.sha256(REFERENCE_PATH.read_bytes()).hexdigest()
    if digest == REFERENCE_SHA256_PREMERGE:
        pytest.skip(
            "reference is the pre-merge file; the supporting_coverage fix on "
            "integrate/0.8.22-eval-supporting-coverage-20260806 has not landed. "
            "S2 cannot CLOSE until it does. Vectors were generated against the fix."
        )
    assert digest == REFERENCE_SHA256_POSTFIX, (
        "ir_eval.rs changed; regenerate fixtures/ir_parity_vectors.json against "
        "the new reference before trusting the port."
    )


# --- AC-3/4: the shapes the upstream fix established -----------------------


def test_no_supporting_units_is_none_not_zero() -> None:
    q = GoldQuery(
        query_id="q",
        query="q",
        query_class="exact_fact",
        required_evidence=(EvidenceUnit("e", "d1", "required"),),
    )
    assert evidence_recall_at_k(q, ["d1"], 10).supporting_coverage is None


def test_supporting_present_but_unretrieved_is_zero_not_none() -> None:
    """Some(0.0) and None are different facts -- the whole point of the fix."""
    q = GoldQuery(
        query_id="q",
        query="q",
        query_class="exact_fact",
        required_evidence=(
            EvidenceUnit("e1", "d1", "required"),
            EvidenceUnit("e2", "d2", "supporting"),
        ),
    )
    assert evidence_recall_at_k(q, ["d1"], 10).supporting_coverage == 0.0


def test_empty_class_agg_asymmetry_is_ported_as_is() -> None:
    """strict/graded are 0.0 at n==0 while supporting is None. Returning None
    uniformly looks more principled and would diverge from the reference."""
    empty = ClassAgg()
    assert empty.strict() == 0.0
    assert empty.graded() == 0.0
    assert empty.supporting() is None


# --- AC-5: the vacuous 1.0 is unreachable on validated gold ----------------


def test_non_negative_query_with_empty_denominator_is_refused() -> None:
    bad = GoldQuery(
        query_id="q", query="q", query_class="exact_fact", required_evidence=()
    )
    issues = validate_methodology([bad])
    assert any("empty" in i.lower() and "q" in i for i in issues)


def test_supporting_only_evidence_is_refused() -> None:
    """The legacy fallback fires only when required_evidence is ENTIRELY empty,
    so a supporting-only list yields an empty denominator with no fallback --
    a vacuous 1.0 with no guard. The reference does not test this."""
    bad = GoldQuery(
        query_id="q",
        query="q",
        query_class="exact_fact",
        required_evidence=(EvidenceUnit("e", "d", "supporting"),),
    )
    assert validate_methodology([bad])


def test_negative_query_with_empty_denominator_is_allowed() -> None:
    ok = GoldQuery(query_id="q", query="q", query_class="negative", required_evidence=())
    assert validate_methodology([ok]) == []


def test_duplicate_query_id_is_refused() -> None:
    q = GoldQuery(
        query_id="dup",
        query="q",
        query_class="exact_fact",
        required_evidence=(EvidenceUnit("e", "d", "required"),),
    )
    assert validate_methodology([q, q])


def test_duplicate_evidence_id_is_refused() -> None:
    q = GoldQuery(
        query_id="q",
        query="q",
        query_class="exact_fact",
        required_evidence=(
            EvidenceUnit("e", "d1", "required"),
            EvidenceUnit("e", "d2", "required"),
        ),
    )
    assert validate_methodology([q])


# --- AC-6: a failed retrieval is never scored ------------------------------


def test_failed_retrieval_is_surfaced_not_scored() -> None:
    """A codex §9 [P2] fix in the reference: a retrieval error must not be
    folded into an empty result set and scored as misses or as a correct
    abstention."""
    queries = [
        GoldQuery(
            query_id="boom",
            query="q",
            query_class="exact_fact",
            required_evidence=(EvidenceUnit("e", "d", "required"),),
        )
    ]

    def retrieve(_q: GoldQuery) -> list[str]:
        raise RuntimeError("retrieval exploded")

    result = aggregate(queries, retrieve, k=10)
    assert result.overall.n == 0
    assert result.errors == {"boom": "retrieval exploded"}


def test_failed_negative_retrieval_is_not_a_correct_abstention() -> None:
    queries = [
        GoldQuery(query_id="boom", query="q", query_class="negative", required_evidence=())
    ]

    def retrieve(_q: GoldQuery) -> list[str]:
        raise RuntimeError("retrieval exploded")

    result = aggregate(queries, retrieve, k=10)
    assert result.negative.n == 0
    assert result.negative.abstained == 0
    assert "boom" in result.errors


# --- AC-7: depth lives outside the metric ----------------------------------


@pytest.mark.parametrize("k", [5, 10])
def test_measurable_depth_is_allowed_for_every_mode(k: int) -> None:
    for mode in RetrievalMode:
        assert check_depth(mode, k, 10) is None


@pytest.mark.parametrize("k", [20, 50])
@pytest.mark.parametrize("mode", [RetrievalMode.VECTOR_ONLY, RetrievalMode.HYBRID])
def test_deep_k_is_refused_for_vector_and_hybrid(mode: RetrievalMode, k: int) -> None:
    blocker = check_depth(mode, k, 10)
    assert blocker is not None
    assert blocker.code is BlockerCode.METRIC_NOT_MEASURABLE
    #: The lever is 0.8.22 Slice 18's public `limit`, not the retired D-5.2
    #: commissioning or the hidden test seam.
    assert "Raise `limit`" in blocker.message
    assert "Slice 18" in blocker.message
    assert "SEARCH_RERANK_LIMIT" not in blocker.message


@pytest.mark.parametrize("k", [20, 50, 100])
def test_k_up_to_the_declared_limit_is_allowed_for_every_mode(k: int) -> None:
    """The D-5 mode table is retired (S6a): fts_only is no longer unbounded
    and hybrid/vector are no longer capped at 10 -- one rule, k <= limit."""
    for mode in RetrievalMode:
        assert check_depth(mode, k, 100) is None


@pytest.mark.parametrize("mode", list(RetrievalMode))
def test_k_beyond_the_engine_maximum_is_permanently_unmeasurable(mode: RetrievalMode) -> None:
    """k=200 was measurable for fts_only under the old doctrine; the rebuilt
    engine refuses limit > 100 on every verb, so it is refused everywhere."""
    blocker = check_depth(mode, 200, 100)
    assert blocker is not None
    assert blocker.code is BlockerCode.METRIC_NOT_MEASURABLE


def test_metric_function_itself_never_refuses_on_depth() -> None:
    """The reference's evidence_recall_at_k takes any k and never refuses. A
    version with an extra refusal arm would not be the reference's function."""
    q = GoldQuery(
        query_id="q",
        query="q",
        query_class="exact_fact",
        required_evidence=(EvidenceUnit("e", "d1", "required"),),
    )
    assert evidence_recall_at_k(q, ["d1"], 50).strict == 1.0


# --- AC-8: eligibility ------------------------------------------------------


def test_ndcg_is_not_applicable_without_graded_relevance() -> None:
    value = resolve_ndcg(has_graded_relevance=False)
    assert value.status is MetricStatus.NOT_APPLICABLE
    assert value.value is None
    assert value.reason


# --- AC-9: rounding ---------------------------------------------------------


@pytest.mark.parametrize(("value", "away", "bankers"), [(0.00035, 0.0004, 0.0003), (0.00045, 0.0005, 0.0004)])
def test_rounding_is_half_away_from_zero_not_bankers(
    value: float, away: float, bankers: float
) -> None:
    """Rust `f64::round` is half-away-from-zero; Python's `round` rounds ties to
    even. These two values scale to exact binary ties (3.5, 4.5), so the two
    rules genuinely disagree -- using the builtin would be a silent, permanent,
    tiny divergence from the reference.

    Most decimal literals are NOT exact ties (0.00005 scales to just above 0.5,
    so both rules agree); picking such a value would make this test look green
    while asserting nothing.
    """
    assert round4(value) == away
    assert round(value, 4) == bankers
    assert round4(-value) == -away
