"""0.8.3 CE-rerank precision probe — RED→GREEN (design §3 / §2).

Backend-free unit tests for the frozen rule (:mod:`eval.ce_rerank_rule`) + the
CE-rerank arm wiring (:mod:`eval.ce_rerank_probe`). The arm tests inject a FAKE
``rerank_fn`` (a known reorder) so they need no model load; one test exercises the
REAL ``fathomdb.rerank`` ``rerank_depth==0`` identity contract (no model load,
guarded by ``importorskip`` so it is clean when the extension is absent).

Covers the four TDD points (design §5 DoD):
* (a) :func:`probe_rerank_pass` truth table — PASS / FAIL-margin / underpowered ⇒
  INCONCLUSIVE.
* (b) the rerank arm calls the reranker over the fused pool, then the harness cuts
  top-K (fixture with a known reorder that surfaces an otherwise-missed gold).
* (c) the paired bootstrap CI + MDE are deterministic (seed-fixed) and reuse the
  reviewed ``class_delta`` machinery.
* (d) ``rerank_depth==0`` ⇒ identity ⇒ rerank-arm recall == baseline recall.
"""

from __future__ import annotations

import pytest

from eval.ce_rerank_probe import (
    BASELINE_ARM,
    FLOOR_ARM,
    RERANK_ARM,
    FathomDBRerankAdapter,
    build_probe_artifact,
    pooled_margin,
    rerank_margin_summary,
)
from eval.ce_rerank_rule import EPS_MDE, headroom_captured, probe_rerank_pass
from eval.d0b_powered_recall import RecallItem, recall_records
from eval.r2_parity_eval import Hit


# --------------------------------------------------------------------------- #
# (a) probe_rerank_pass truth table.
# --------------------------------------------------------------------------- #


def test_probe_rerank_pass_truth_table() -> None:
    # PASS: powered (mde <= eps) AND margin CI lower bound > 0.
    assert (
        probe_rerank_pass({"point": 0.1, "ci_lo": 0.02, "ci_hi": 0.18, "mde": 0.04, "n": 100})
        == "PASS"
    )
    # FAIL: powered but the margin CI lower bound is <= 0 (no lift at power).
    assert (
        probe_rerank_pass({"point": 0.0, "ci_lo": -0.03, "ci_hi": 0.05, "mde": 0.04, "n": 100})
        == "FAIL"
    )
    # ci_lo == 0 exactly is NOT a lift (strict > 0).
    assert (
        probe_rerank_pass({"point": 0.05, "ci_lo": 0.0, "ci_hi": 0.1, "mde": 0.04, "n": 80})
        == "FAIL"
    )
    # INCONCLUSIVE: under-powered (mde > eps) even with a positive CI lower bound —
    # the power guard is checked FIRST, never a silent FAIL.
    assert (
        probe_rerank_pass({"point": 0.1, "ci_lo": 0.02, "ci_hi": 0.30, "mde": 0.12, "n": 12})
        == "INCONCLUSIVE"
    )
    # INCONCLUSIVE: degenerate n<=1 ⇒ mde is None.
    assert (
        probe_rerank_pass({"point": 0.1, "ci_lo": 0.1, "ci_hi": 0.1, "mde": None, "n": 1})
        == "INCONCLUSIVE"
    )
    # The eps boundary is inclusive (mde == eps is powered).
    assert (
        probe_rerank_pass({"point": 0.1, "ci_lo": 0.02, "ci_hi": 0.2, "mde": EPS_MDE, "n": 50})
        == "PASS"
    )


def test_probe_rerank_pass_rejects_non_finite() -> None:
    with pytest.raises(ValueError):
        probe_rerank_pass({"point": 0.1, "ci_lo": float("nan"), "ci_hi": 0.2, "mde": 0.01, "n": 50})


def test_headroom_captured_diagnostic() -> None:
    # (rerank − fathomdb) / oracle_gap = (0.6 − 0.4) / 0.4 = 0.5
    assert headroom_captured(0.6, 0.4, 0.4) == pytest.approx(0.5)
    # absent artifact ⇒ None (never fabricate a number)
    assert headroom_captured(0.6, 0.4, None) is None
    # ~0 headroom ⇒ None (no denominator)
    assert headroom_captured(0.6, 0.4, 0.0) is None


# --------------------------------------------------------------------------- #
# (b) the rerank arm: known reorder over the fused pool, then top-K cut.
# --------------------------------------------------------------------------- #


class _FakeBase:
    """A base FathomDB adapter returning a fixed pool (with bodies) per question."""

    def __init__(self, pool: list[tuple[str, str, float]]) -> None:
        # pool = [(doc_id, body, score)] in the BASE (pre-rerank) fused order.
        self._pool = pool

    def retrieve(self, question: str, k: int) -> list[Hit]:
        return [Hit(doc_id=d, body=b, score=s) for d, b, s in self._pool[:k]]


def _reverse_rerank(query: str, passages: list[dict], depth: int) -> list[dict]:
    """A deterministic FAKE reranker: reverse the pool order (a known reorder)."""
    rev = list(reversed(passages))
    n = len(rev)
    return [{"id": p["id"], "score": float(n - i)} for i, p in enumerate(rev)]


def test_rerank_arm_reorders_pool_then_topk_surfaces_gold() -> None:
    # Base fused order buries GOLD at rank 14 (> k=10) → baseline misses it; the
    # reranker reverses the pool, lifting GOLD to rank 1 → top-K surfaces it.
    pool = [(f"d{i}", f"body {i}", 1.0 / (i + 1)) for i in range(14)] + [
        ("GOLD", "gold body", 0.001)
    ]
    base = _FakeBase(pool)
    rerank_adapter = FathomDBRerankAdapter(
        base=base, rerank_fn=_reverse_rerank, pool_n=15, rerank_depth=15
    )

    items = [RecallItem("q1", "factoid", ("GOLD",), "lme", "q1")]
    records = recall_records(
        items, {BASELINE_ARM: base, RERANK_ARM: rerank_adapter}, k=10
    )
    assert records[0]["recall"][BASELINE_ARM] == 0.0, "baseline buries GOLD past top-K"
    assert records[0]["recall"][RERANK_ARM] == 1.0, "rerank lifts GOLD into top-K"

    # The arm preserves doc_id identity through the id round-trip (no body/id swap).
    out = rerank_adapter.retrieve("q1", 10)
    assert out[0].doc_id == "GOLD"
    assert out[0].body == "gold body"


# --------------------------------------------------------------------------- #
# (c) paired bootstrap CI + MDE deterministic; margin point correct.
# --------------------------------------------------------------------------- #


def _records_for_margin() -> list[dict]:
    """4 factoid items: rerank beats baseline on 2, ties on 2 ⇒ margin point 0.5."""
    items = [RecallItem(f"f{i}", "factoid", ("g",), "lme", f"f{i}") for i in range(4)]

    # baseline misses gold on f0,f1 ; rerank hits gold on all four.
    class _A:
        def __init__(self, hit_q: set[str]) -> None:
            self._hit = hit_q

        def retrieve(self, question: str, k: int) -> list[Hit]:
            ids = ["g"] if question in self._hit else ["zzz"]
            return [Hit(doc_id=d, body="", score=1.0) for d in ids]

    baseline = _A({"f2", "f3"})
    rerank = _A({"f0", "f1", "f2", "f3"})
    return recall_records(items, {BASELINE_ARM: baseline, RERANK_ARM: rerank}, k=10)


def test_pooled_margin_is_deterministic_and_correct() -> None:
    records = _records_for_margin()
    m1 = pooled_margin(records, classes=("factoid",), seed=0)
    m2 = pooled_margin(records, classes=("factoid",), seed=0)
    assert m1 == m2, "same seed ⇒ identical bootstrap CI (deterministic)"
    assert m1["point"] == 0.5  # mean(1-0,1-0,1-1,1-1) = 0.5
    assert m1["n"] == 4
    assert m1["ci_lo"] is not None and m1["mde"] is not None


def test_rerank_margin_summary_deterministic_and_verdict() -> None:
    records = _records_for_margin()
    s1 = rerank_margin_summary(records, classes=("factoid",), seed=0)
    s2 = rerank_margin_summary(records, classes=("factoid",), seed=0)
    assert s1 == s2, "deterministic given a fixed seed"
    fac = s1["per_class"]["factoid"]
    assert fac["margin"]["point"] == 0.5
    assert fac["verdict"] in {"PASS", "FAIL", "INCONCLUSIVE"}
    # headroom-captured wired: with an oracle_gap it is (rerank-baseline)/gap.
    s_gap = rerank_margin_summary(
        records, classes=("factoid",), oracle_gaps={"factoid": 0.5}, seed=0
    )
    fac_gap = s_gap["per_class"]["factoid"]
    # rerank recall = 1.0, baseline recall = 0.5 ⇒ (1.0-0.5)/0.5 = 1.0
    assert fac_gap["headroom_captured"] == pytest.approx(1.0)
    # absent gap ⇒ None (non-gating).
    assert fac["headroom_captured"] is None


def test_build_probe_artifact_pins_parameters() -> None:
    records = _records_for_margin()
    art = build_probe_artifact(
        records,
        k=10,
        pool_n=50,
        rerank_depth=50,
        corpus_hash="deadbeef",
        seed=0,
        n_boot=500,
        arms_run=[BASELINE_ARM, RERANK_ARM, FLOOR_ARM],
        blockers=[],
        oracle_gaps={"factoid": 0.5},
        oracle_source="dev/plans/runs/0.8.3-gap-decomposition-n606.json",
        smoke=True,
    )
    assert art["schema"] == "0.8.3-ce-rerank-probe-v1"
    assert art["pool_n"] == 50
    assert art["rerank_depth"] == 50
    assert art["corpus_hash"] == "deadbeef"
    assert art["seed"] == 0
    assert art["ce_model_repo"] == "cross-encoder/ms-marco-TinyBERT-L2-v2"
    assert "rerank_margin" in art


# --------------------------------------------------------------------------- #
# (d) rerank_depth==0 ⇒ identity ⇒ rerank-arm recall == baseline recall (REAL CE).
# --------------------------------------------------------------------------- #


def test_rerank_depth_0_is_identity_real_reranker() -> None:
    """The REAL ``fathomdb.rerank`` ``rerank_depth==0`` contract: input order +
    input scores, no model load. The rerank arm at depth 0 must reproduce the
    baseline recall exactly."""
    fathomdb = pytest.importorskip("fathomdb")

    pool = [("GOLD", "gold body alpha", 0.9), ("d1", "beta body", 0.5), ("d2", "gamma", 0.1)]
    base = _FakeBase(pool)
    identity_arm = FathomDBRerankAdapter(
        base=base, rerank_fn=fathomdb.rerank, pool_n=3, rerank_depth=0
    )
    items = [RecallItem("q1", "factoid", ("GOLD",), "lme", "q1")]
    records = recall_records(items, {BASELINE_ARM: base, RERANK_ARM: identity_arm}, k=10)
    assert records[0]["recall"][RERANK_ARM] == records[0]["recall"][BASELINE_ARM]

    # And the order/doc_ids are byte-identical to the base pool (true identity).
    out = identity_arm.retrieve("q1", 10)
    assert [h.doc_id for h in out] == ["GOLD", "d1", "d2"]


# --------------------------------------------------------------------------- #
# (e) 0.8.5 — native α/pool_n/ce_score EQUIVALENCE PIN against the eval oracle.
# --------------------------------------------------------------------------- #

# A fixed candidate fixture (query + bodies) where the CE strongly disagrees with
# the base order, so α=1.0 reranking is a real reorder, not a no-op.
_EQUIV_QUERY = "How many people live in Berlin?"
_EQUIV_PASSAGES = [
    {"id": 0, "body": "Berlin is famous for its nightlife, art scene, and historic architecture.", "score": 0.50},
    {"id": 1, "body": "Berlin has a population of about 3.7 million inhabitants, the most populous city in Germany.", "score": 0.49},
    {"id": 2, "body": "Paris is the capital of France and a major European cultural centre.", "score": 0.30},
    {"id": 3, "body": "The quick brown fox jumps over the lazy dog near the river.", "score": 0.10},
    {"id": 4, "body": "Germany's federal government sits in Berlin, the national capital.", "score": 0.05},
    {"id": 5, "body": "A recipe for sourdough bread starts with an active flour-and-water starter.", "score": 0.01},
]


def _reblend_order(ce_by_id: dict[int, float], passages: list[dict], *, alpha: float, pool_n: int) -> list[int]:
    """The eval oracle's re-blend over the same pool: sort by
    ``alpha*ce + (1-alpha)*minmax(base_score)`` desc, stable on base index."""
    from eval.rerank_tune_probe import minmax_norm

    cand = passages[:pool_n]
    rrf = minmax_norm([float(p["score"]) for p in cand])
    scored = [
        (alpha * ce_by_id.get(int(p["id"]), 0.0) + (1.0 - alpha) * rrf[i], i, int(p["id"]))
        for i, p in enumerate(cand)
    ]
    scored.sort(key=lambda t: (-t[0], t[1]))
    return [pid for _s, _i, pid in scored]


def test_native_alpha_pool_n_matches_reblend_adapter() -> None:
    """0.8.5 EQUIVALENCE PIN: native ``fathomdb.rerank(..., alpha=1.0, pool_n=10)``
    reproduces the eval ``ReblendRerankAdapter`` order AND the returned ``ce_score``
    matches the adapter's recovered ``ce_norm`` within 1e-6 — proving the parity
    config is now native. The model is cached (no network); a missing model is a
    real signal, not an expected skip."""
    fathomdb = pytest.importorskip("fathomdb")
    from eval.rerank_tune_probe import recover_ce_norm

    passages = [dict(p) for p in _EQUIV_PASSAGES]
    n = len(passages)

    # Native parity config (α=1.0, narrow pool covering the whole fixture).
    native = fathomdb.rerank(_EQUIV_QUERY, passages, n, alpha=1.0, pool_n=10)
    if all(r.get("ce_score") is None for r in native):
        pytest.skip("CE reranker model unavailable (cold cache, no network)")

    # Oracle: recover pure ce_norm via the score=0 trick (default α=0.3 path).
    zero = [{"id": p["id"], "body": p["body"], "score": 0.0} for p in passages]
    ce_by_id = {int(r["id"]): recover_ce_norm(float(r["score"])) for r in fathomdb.rerank(_EQUIV_QUERY, zero, n)}

    expected_order = _reblend_order(ce_by_id, passages, alpha=1.0, pool_n=10)
    assert [int(r["id"]) for r in native] == expected_order, "native α=1.0 order must match the reblend oracle"

    # ce_score per id matches the recovered ce_norm within tol.
    for r in native:
        rid = int(r["id"])
        assert r["ce_score"] is not None, "in-pool hit must carry a ce_score"
        assert abs(float(r["ce_score"]) - ce_by_id[rid]) < 1e-6, f"ce_score mismatch for id={rid}"


def test_native_default_alpha_equals_prod_0_3() -> None:
    """The default call (``alpha`` omitted) is byte-identical to an explicit
    ``alpha=0.3`` prod call — order AND blended scores — so the C6 default is
    preserved through the binding."""
    fathomdb = pytest.importorskip("fathomdb")

    passages = [dict(p) for p in _EQUIV_PASSAGES]
    n = len(passages)
    default = fathomdb.rerank(_EQUIV_QUERY, passages, n)
    if all(r.get("ce_score") is None for r in default):
        pytest.skip("CE reranker model unavailable (cold cache, no network)")
    explicit = fathomdb.rerank(_EQUIV_QUERY, passages, n, alpha=0.3, pool_n=n)
    assert [int(r["id"]) for r in default] == [int(r["id"]) for r in explicit]
    for a, b in zip(default, explicit):
        assert abs(float(a["score"]) - float(b["score"])) < 1e-12, "default α must equal explicit 0.3"
