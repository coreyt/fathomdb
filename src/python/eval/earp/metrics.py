"""S2 — IR-B evidence-recall metrics, ported from the Rust reference.

Pure: no SDK, no database, no network. Parity is pinned against values the
reference actually produced (`tests/earp/fixtures/ir_parity_vectors.json`),
not against a Python re-derivation.

Reference symbols, pinned to commit `19765415`: `evidence_recall_at_k`,
`required_doc_ids`, `supporting_doc_ids`, `negative_abstained`, `ClassAgg`,
`NegativeAgg`, `KResult`, `evaluate_gold_set`, `validate_gold_set`, `round4`.
Cited by symbol rather than line: the pre- and post-fix files differ by a few
lines throughout, and a dangling citation is a defect in a parity document.

Design of record: `dev/design/earp-slice-2-design.md`.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Callable, Iterable, Mapping, Sequence

from eval.earp.gold import GoldQuery
from eval.earp.schema.models import MetricStatus, MetricValue, QueryClass

#: The reporting ladder and the headline depth (IR-B §(c)).
K_LADDER: tuple[int, ...] = (5, 10, 20, 50)
HEADLINE_K = 10

#: Reference discriminant order. The reference keys per-class aggregates in a
#: BTreeMap, which iterates by discriminant -- not alphabetically -- so any
#: per-class output meaning to match it must iterate in this order.
_CLASS_ORDER: tuple[str, ...] = tuple(c.value for c in QueryClass)


def round4(value: float) -> float:
    """Round to 4 dp, half-AWAY-from-zero.

    Rust's `f64::round` is half-away-from-zero; Python's built-in `round` is
    banker's rounding. They differ at a `.00005` boundary, so using the builtin
    here would be a silent, tiny, permanent divergence from the reference.
    """
    scaled = value * 10_000.0
    return math.copysign(math.floor(abs(scaled) + 0.5), scaled) / 10_000.0


@dataclass(frozen=True)
class PerQueryRecall:
    strict: float
    graded: float
    #: `None` when the query has no supporting units at all; `0.0` when it has
    #: some and none were retrieved. Different facts.
    supporting_coverage: float | None
    required_n: int
    required_hits: int


def required_doc_ids(query: GoldQuery) -> set[str]:
    """The recall denominator, as a set of `doc_id`.

    Two properties that a plausible reimplementation gets wrong:

    * it collapses on `doc_id`, so two required units on the SAME document are
      one denominator entry, not two;
    * the legacy `expected_top_k_doc_ids` fallback fires only when
      `required_evidence` is ENTIRELY empty -- never added on top of an
      evidence-labelled set (§(f), the single-unit-of-relevance rule).
    """
    if not query.required_evidence:
        return set(query.expected_top_k_doc_ids)
    return {u.doc_id for u in query.required_evidence if u.necessity == "required"}


def supporting_doc_ids(query: GoldQuery) -> set[str]:
    """Computed independently of the required set, so a document that is both
    required and supporting counts in both."""
    return {u.doc_id for u in query.required_evidence if u.necessity == "supporting"}


def evidence_recall_at_k(
    query: GoldQuery, retrieved_doc_ids: Sequence[str], k: int
) -> PerQueryRecall:
    """Strict all-of and graded fraction over the `required`-only denominator.

    Takes any `k` and never refuses -- depth measurability is `earp.depth`'s
    concern, deliberately outside this function so it stays the reference's
    function.

    Note `retrieved_doc_ids[:k]` is NOT de-duplicated: a repeated document
    consumes a rank slot, exactly as the reference's `.take(k)` does.
    """
    topk = set(retrieved_doc_ids[:k])
    required = required_doc_ids(query)
    supporting = supporting_doc_ids(query)

    required_n = len(required)
    required_hits = len(required & topk)
    # An empty required set is vacuously 1.0. Safe only because the aggregator
    # routes negatives away from recall and `validate_methodology` refuses a
    # non-negative query with an empty denominator -- so this bites only
    # mislabeled data, which the validator catches.
    strict = 1.0 if required_n == 0 or required_hits == required_n else 0.0
    graded = 1.0 if required_n == 0 else required_hits / required_n

    supporting_coverage = (
        len(supporting & topk) / len(supporting) if supporting else None
    )
    return PerQueryRecall(
        strict=strict,
        graded=graded,
        supporting_coverage=supporting_coverage,
        required_n=required_n,
        required_hits=required_hits,
    )


def negative_abstained(retrieved_doc_ids: Sequence[str], k: int) -> bool:
    """Abstention-correctness for a negative query: correct iff top-K is empty.
    A non-empty top-K is a false positive. Reported separately from recall."""
    return not retrieved_doc_ids[:k]


@dataclass(frozen=True)
class ClassAgg:
    """Per-class accumulator, as a PURE FOLD.

    The reference uses `fn add(&mut self, ...)`, but every dataclass in this
    codebase is frozen, so `add` returns a new instance instead. At 4,597
    queries the allocation is irrelevant.
    """

    n: int = 0
    strict_sum: float = 0.0
    graded_sum: float = 0.0
    supporting_sum: float = 0.0
    #: Denominator for `supporting()` -- support-bearing queries only, NOT all
    #: queries. Averaging over `n` would dilute the value with queries that
    #: carry no supporting evidence at all.
    supporting_query_n: int = 0

    def add(self, m: PerQueryRecall) -> "ClassAgg":
        has_support = m.supporting_coverage is not None
        return ClassAgg(
            n=self.n + 1,
            strict_sum=self.strict_sum + m.strict,
            graded_sum=self.graded_sum + m.graded,
            supporting_sum=self.supporting_sum + (m.supporting_coverage or 0.0),
            supporting_query_n=self.supporting_query_n + (1 if has_support else 0),
        )

    def strict(self) -> float:
        return 0.0 if self.n == 0 else self.strict_sum / self.n

    def graded(self) -> float:
        return 0.0 if self.n == 0 else self.graded_sum / self.n

    def supporting(self) -> float | None:
        """`None` when no query in this aggregate carries supporting evidence.

        The asymmetry with `strict()`/`graded()` -- which return 0.0 at n==0 --
        is the reference's, and is ported as-is rather than tidied into a
        uniform `None`.
        """
        if self.supporting_query_n == 0:
            return None
        return self.supporting_sum / self.supporting_query_n


@dataclass(frozen=True)
class NegativeAgg:
    n: int = 0
    abstained: int = 0

    def add(self, abstained: bool) -> "NegativeAgg":
        return NegativeAgg(n=self.n + 1, abstained=self.abstained + (1 if abstained else 0))

    def false_positive_rate(self) -> float:
        return 0.0 if self.n == 0 else (self.n - self.abstained) / self.n


@dataclass(frozen=True)
class KResult:
    """One rung of the ladder."""

    k: int
    overall: ClassAgg = field(default_factory=ClassAgg)
    per_class: Mapping[str, ClassAgg] = field(default_factory=dict)
    negative: NegativeAgg = field(default_factory=NegativeAgg)
    #: query_id -> failure message. A failed retrieval is NEVER folded into an
    #: empty result set and scored as misses or as a correct abstention: that
    #: was a codex §9 [P2] defect in the reference's own history.
    errors: Mapping[str, str] = field(default_factory=dict)


def aggregate(
    queries: Iterable[GoldQuery],
    retrieve: Callable[[GoldQuery], Sequence[str]],
    *,
    k: int,
) -> KResult:
    """Score every query at depth `k`, routing negatives out of recall.

    A `retrieve` that raises records the failure and scores nothing for that
    query -- neither a miss nor an abstention.
    """
    overall = ClassAgg()
    per_class: dict[str, ClassAgg] = {}
    negative = NegativeAgg()
    errors: dict[str, str] = {}

    for query in queries:
        try:
            retrieved = retrieve(query)
        except Exception as exc:  # noqa: BLE001 -- surfaced, never scored
            errors[query.query_id] = str(exc)
            continue

        if query.query_class == QueryClass.NEGATIVE.value:
            negative = negative.add(negative_abstained(retrieved, k))
            continue

        recall = evidence_recall_at_k(query, retrieved, k)
        overall = overall.add(recall)
        per_class[query.query_class] = per_class.get(query.query_class, ClassAgg()).add(
            recall
        )

    ordered = {c: per_class[c] for c in _CLASS_ORDER if c in per_class}
    return KResult(k=k, overall=overall, per_class=ordered, negative=negative, errors=errors)


def validate_methodology(queries: Iterable[GoldQuery]) -> list[str]:
    """The reference's `validate_gold_set` methodology invariants.

    S2 owns these, not S1: they are metric methodology, not file integrity, and
    without them the vacuous 1.0 in `evidence_recall_at_k` has no guard at all.
    """
    issues: list[str] = []
    seen_queries: set[str] = set()
    seen_evidence: set[str] = set()

    for query in queries:
        if query.query_id in seen_queries:
            issues.append(f"duplicate query_id `{query.query_id}`")
        seen_queries.add(query.query_id)

        for unit in query.required_evidence:
            if unit.evidence_id in seen_evidence:
                issues.append(f"duplicate evidence_id `{unit.evidence_id}`")
            seen_evidence.add(unit.evidence_id)

        if query.query_class == QueryClass.NEGATIVE.value:
            continue

        if not required_doc_ids(query):
            # Two ways to land here, and they deserve different words. The
            # legacy fallback fires only when required_evidence is ENTIRELY
            # empty, so a supporting-only list yields an empty denominator with
            # no fallback -- a vacuous 1.0 the reference does not test for.
            if query.required_evidence:
                issues.append(
                    f"non-negative query `{query.query_id}` has required_evidence "
                    f"with no `required` unit, so its denominator is empty and the "
                    f"legacy doc-id fallback does not apply"
                )
            else:
                issues.append(
                    f"non-negative query `{query.query_id}` of class "
                    f"`{query.query_class}` has an empty required denominator"
                )

    return issues


def resolve_ndcg(*, has_graded_relevance: bool) -> MetricValue:
    """nDCG needs graded relevance. No gold set in this repo carries any, so
    this resolves to `not_applicable` with a reason -- never zero."""
    if has_graded_relevance:
        raise NotImplementedError(
            "nDCG computation lands with the first campaign that can emit it"
        )
    return MetricValue(
        status=MetricStatus.NOT_APPLICABLE,
        value=None,
        reason=(
            "nDCG requires graded relevance; the IR-C reuse tier carries a "
            "three-value query_class and binary necessity only"
        ),
    )


__all__ = [
    "HEADLINE_K",
    "K_LADDER",
    "ClassAgg",
    "KResult",
    "NegativeAgg",
    "PerQueryRecall",
    "aggregate",
    "evidence_recall_at_k",
    "negative_abstained",
    "required_doc_ids",
    "resolve_ndcg",
    "round4",
    "supporting_doc_ids",
    "validate_methodology",
]
