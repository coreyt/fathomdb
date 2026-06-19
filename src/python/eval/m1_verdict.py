"""M1 adjudication VERDICT harness (Slice 20, stage 1) — the pre-registered endpoint.

Binding spec: ``dev/plans/plan-0.8.2.md`` §4 (Slice 20) + the SIGNED
``dev/design/0.8.2-m1-multihop-harness.md`` §4 (the frozen primary endpoint and
``decide()``). This module runs the **five** arms (the four baseline arms plus the
``ppr_fusion`` graph arm) over the graph-covered MuSiQue-Ans questions with the
identical answerer, computes the **frozen** primary endpoint — the pooled ≥3-hop
ΔF1 of ``ppr_fusion`` vs the fixed ``fused-RRF (k=60)`` comparator with a
question-level paired bootstrap — and derives the GO/NO-GO call **mechanically**
from the imported :func:`m1_decision_rule.decide` (never redefined).

**Comparator = the ``fused`` arm (fused-RRF k=60), NOT ``fused_rerank``.** The
design was AMENDED 2026-06-19 (HITL) to make the fixed comparator ``fused-RRF``;
``m1_baseline.COMPARATOR_ARM`` still names the *pre-amendment* ``fused_rerank``
(it drives the Slice-5 power-sim baseline only). The Slice-20 primary endpoint
uses :data:`COMPARATOR_ARM` below = ``"fused"``.

**Stage-1 scope (HITL-authorized ~$10 run on the current 299-graph).** The graph
covers only **answerable** questions — there is **no unanswerable contrast set**,
so the confident-wrong guard is **UNEVALUATED**: ``decide()`` is fed
``confident_wrong={"increase_significant": False}`` with a loud note, and
``power_ok=False`` (N≈144 ≥3-hop ≪ the 1165 required). ``decide()`` therefore
returns **NO_GO** via the power gate — expected and correct for stage 1. The
load-bearing scientific signal is the **effect size** (the ΔF1 point estimate +
its paired-bootstrap CI) and the **stage-2 recommendation** derived from it.

Pure-compute given the run's ``paired_records`` (the paired bootstrap reuses
``m1_power_sim`` primitives; deterministic given ``seed``). The one priced seam is
the shared answerer, owned by :mod:`eval.m1_verdict_run`.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Callable, Optional

import numpy as np

from eval.m1_baseline import (
    MUSIQUE_HASH,
    Question,
    load_musique,
    run_baseline,
)
from eval.m1_decision_rule import MATERIAL_F1_LIFT, decide
from eval.m1_power_sim import (
    _percentile_ci_high,
    _percentile_ci_low,
    _slope_neg_significant,
)
from eval.m1_ppr import DEFAULT_PPR_CONFIG, PPRConfig, add_ppr_fusion_arm
from eval.r2_parity_eval import BaseAnswerer

# --------------------------------------------------------------------------- #
# Frozen arm identity (design §4, AMENDED 2026-06-19)
# --------------------------------------------------------------------------- #

#: The fixed primary-endpoint comparator — the ``fused`` (fused-RRF k=60) arm.
#: AMENDED 2026-06-19; ``m1_baseline.COMPARATOR_ARM`` (= ``fused_rerank``) is the
#: stale pre-amendment value used only by the Slice-5 power sim.
COMPARATOR_ARM = "fused"

#: The graph arm under test.
TREATMENT_ARM = "ppr_fusion"

#: The five arms Slice 20 reports; ``ppr_fusion`` is appended via the augment hook.
VERDICT_ARMS: tuple[str, ...] = (
    "bm25",
    "passage_dense",
    "fused",
    "fused_rerank",
    "ppr_fusion",
)

#: Default paired-bootstrap resample count for the CI (deterministic given seed).
DEFAULT_N_BOOT = 2000


# --------------------------------------------------------------------------- #
# Corpus / graph selection
# --------------------------------------------------------------------------- #


def graph_qids(extractions: Mapping[str, Any]) -> set[str]:
    """The set of question ids the preserved extraction graph covers.

    Extraction keys are ``"{qid}#{para_idx}"``; the qid is everything before the
    first ``#``."""
    return {str(k).split("#", 1)[0] for k in extractions}


def load_graph_questions(
    corpus: str | Path, extractions: Mapping[str, Any], *, assert_hash: bool = True
) -> list[Question]:
    """Load the **answerable** MuSiQue questions the graph covers (stable order).

    The graph (Slice 10) was built ``answerable_only=True``, so there is no
    unanswerable set in it — the confident-wrong guard is UNEVALUATED in stage 1.
    Returns the answerable, graph-covered questions sorted by id."""
    qids = graph_qids(extractions)
    qs = load_musique(corpus, assert_hash=assert_hash)
    return sorted((q for q in qs if q.answerable and q.id in qids), key=lambda q: q.id)


def ppr_augment(
    extractions: Mapping[str, Mapping[str, Any]], cfg: PPRConfig = DEFAULT_PPR_CONFIG
) -> Callable[[dict[str, Any], Question], dict[str, Any]]:
    """Return the ``run_baseline`` augment hook that appends the ``ppr_fusion`` arm
    (feeding each question its extractions from the preserved graph)."""

    def _aug(arm_rankings: dict[str, Any], question: Question) -> dict[str, Any]:
        return add_ppr_fusion_arm(arm_rankings, question, extractions, cfg)

    return _aug


# --------------------------------------------------------------------------- #
# $0 sanity guard — ppr_fusion must NOT be silently identical to bm25
# --------------------------------------------------------------------------- #


def ppr_divergence(
    questions: Sequence[Question],
    extractions: Mapping[str, Mapping[str, Any]],
    *,
    k: int = 10,
    cfg: PPRConfig = DEFAULT_PPR_CONFIG,
) -> dict[str, Any]:
    """$0 check that the ``ppr_fusion`` ranking actually diverges from BM25.

    Returns the fraction of questions whose ``ppr_fusion`` top-``k`` differs (as a
    set) from the BM25 top-``k``. The Slice-20 prompt's hard STOP: if the graph arm
    is *silently identical* to BM25 (fraction ≈ 0) the comparison is vacuous —
    abort rather than spend the priced pass."""
    from eval.m1_baseline import bm25_rank
    from eval.m1_ppr import ppr_fusion_ranking

    n_diff = 0
    n_total = 0
    for q in questions:
        bm = bm25_rank(q.question, q.paragraphs)[:k]
        ppr = ppr_fusion_ranking(q, extractions, cfg)[:k]
        n_total += 1
        if set(bm) != set(ppr):
            n_diff += 1
    return {
        "n_questions": n_total,
        "n_ppr_differs_from_bm25_topk": n_diff,
        "fraction_differs": round(n_diff / max(n_total, 1), 4),
        "top_k": k,
        "silently_identical_to_bm25": n_diff == 0,
    }


# --------------------------------------------------------------------------- #
# Paired-bootstrap endpoint
# --------------------------------------------------------------------------- #


def _paired(
    records: Sequence[Mapping[str, Any]],
    metric: str,
    treatment: str,
    comparator: str,
) -> tuple[np.ndarray, np.ndarray]:
    """Per-question paired (treatment − comparator) deltas + hop counts for the
    answerable records carrying both arms' ``metric`` (``"f1"`` / ``"em"``)."""
    deltas: list[float] = []
    hops: list[int] = []
    for r in records:
        if not r.get("answerable"):
            continue
        m = r.get(metric) or {}
        if treatment in m and comparator in m:
            deltas.append(float(m[treatment]) - float(m[comparator]))
            hops.append(int(r["hop_count"]))
    return np.asarray(deltas, dtype=float), np.asarray(hops, dtype=int)


def compute_endpoint(
    paired_records: Sequence[Mapping[str, Any]],
    *,
    treatment: str = TREATMENT_ARM,
    comparator: str = COMPARATOR_ARM,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
) -> dict[str, Any]:
    """The frozen primary endpoint + secondary trend/per-hop reads.

    Primary: pooled ≥3-hop (hops 3+4) ΔF1 = (treatment) − (comparator), with a
    question-level paired-bootstrap CI. Secondary: pooled ≥3-hop ΔEM (CI), per-hop
    (2/3/4) ΔF1/ΔEM, and the ΔF1-vs-hop OLS slope sign + significance (the trend
    gate). Deterministic given ``seed`` (numpy ``default_rng``)."""
    rng = np.random.default_rng(seed)

    # --- pooled ≥3-hop ΔF1 (the primary endpoint) ---
    f1_d_all, f1_hops_all = _paired(paired_records, "f1", treatment, comparator)
    ge3_mask = f1_hops_all >= 3
    f1_ge3 = f1_d_all[ge3_mask]
    if f1_ge3.size == 0:
        raise ValueError("no ≥3-hop answerable paired records — cannot compute the endpoint")
    f1_delta = float(f1_ge3.mean())
    f1_ci_low = _percentile_ci_low(f1_ge3, rng, n_boot=n_boot)
    f1_ci_high = _percentile_ci_high(f1_ge3, rng, n_boot=n_boot)

    # --- pooled ≥3-hop ΔEM (secondary, CI-banded) ---
    em_d_all, em_hops_all = _paired(paired_records, "em", treatment, comparator)
    em_ge3 = em_d_all[em_hops_all >= 3]
    em_delta = float(em_ge3.mean()) if em_ge3.size else 0.0
    em_ci_low = _percentile_ci_low(em_ge3, rng, n_boot=n_boot) if em_ge3.size else 0.0
    em_ci_high = _percentile_ci_high(em_ge3, rng, n_boot=n_boot) if em_ge3.size else 0.0

    # --- trend: ΔF1-vs-hop OLS slope over hops 2/3/4 (all answerable) ---
    trend_neg_significant = bool(
        _slope_neg_significant(f1_hops_all.astype(float), f1_d_all, rng, n_boot=n_boot)
    )
    slope = _ols_slope(f1_hops_all.astype(float), f1_d_all)

    # --- per-hop (2/3/4) ΔF1/ΔEM (secondary splits) ---
    per_hop: dict[str, Any] = {}
    for hop in (2, 3, 4):
        f1_h = f1_d_all[f1_hops_all == hop]
        em_h = em_d_all[em_hops_all == hop]
        per_hop[str(hop)] = {
            "n": int(f1_h.size),
            "f1_delta": (round(float(f1_h.mean()), 6) if f1_h.size else None),
            "f1_ci_low": (round(_percentile_ci_low(f1_h, rng, n_boot=n_boot), 6) if f1_h.size > 1 else None),
            "f1_ci_high": (round(_percentile_ci_high(f1_h, rng, n_boot=n_boot), 6) if f1_h.size > 1 else None),
            "em_delta": (round(float(em_h.mean()), 6) if em_h.size else None),
        }

    return {
        "treatment_arm": treatment,
        "comparator_arm": comparator,
        "n_boot": n_boot,
        "seed": seed,
        "pooled_ge3hop": {
            "n": int(f1_ge3.size),
            "f1_delta": round(f1_delta, 6),
            "f1_ci_low": round(f1_ci_low, 6),
            "f1_ci_high": round(f1_ci_high, 6),
            "em_delta": round(em_delta, 6),
            "em_ci_low": round(em_ci_low, 6),
            "em_ci_high": round(em_ci_high, 6),
        },
        "trend": {
            "slope": (round(slope, 6) if slope is not None else None),
            "neg_significant": trend_neg_significant,
            "hops_present": sorted(int(h) for h in set(f1_hops_all.tolist())),
        },
        "per_hop": per_hop,
    }


def _ols_slope(x: np.ndarray, y: np.ndarray) -> Optional[float]:
    """Point OLS slope of ``y`` on ``x`` (None if x has <2 distinct values)."""
    if len(np.unique(x)) < 2:
        return None
    xbar = x.mean()
    ybar = y.mean()
    denom = float(((x - xbar) ** 2).sum())
    if denom == 0.0:
        return None
    return float(((x - xbar) * (y - ybar)).sum() / denom)


# --------------------------------------------------------------------------- #
# decide() inputs + stage-2 recommendation
# --------------------------------------------------------------------------- #


def decide_inputs(endpoint: Mapping[str, Any], *, power_ok: bool = False) -> dict[str, Any]:
    """Assemble the exact argument bundle handed to :func:`decide`.

    Stage 1: ``confident_wrong.increase_significant=False`` (UNEVALUATED — no
    unanswerable graph coverage) and ``power_ok=False`` (N≈144 ≪ 1165 required).
    Stored verbatim in the artifact so the verdict is re-derivable by re-calling
    ``decide(**decide_inputs)`` (the mechanical-derivation test)."""
    pooled = endpoint["pooled_ge3hop"]
    return {
        "material": {"f1_delta": float(pooled["f1_delta"]), "f1_ci_low": float(pooled["f1_ci_low"])},
        "em": {"ci_high": float(pooled["em_ci_high"])},
        "trend": {"neg_significant": bool(endpoint["trend"]["neg_significant"])},
        "confident_wrong": {"increase_significant": False},
        "power_ok": bool(power_ok),
    }


def verdict_from_inputs(inputs: Mapping[str, Any]) -> str:
    """Call the imported frozen :func:`decide` on a stored ``decide_inputs`` bundle.

    The single source of the GO/NO-GO call — never a post-hoc rule."""
    return decide(
        material=inputs["material"],
        em=inputs["em"],
        trend=inputs["trend"],
        confident_wrong=inputs["confident_wrong"],
        power_ok=inputs["power_ok"],
    )


def stage2_recommendation(
    f1_delta: float, f1_ci_low: float, f1_ci_high: float
) -> dict[str, Any]:
    """Derive the stage-2 call from the *effect size* (not from ``decide()``).

    * Δ clearly NEGATIVE (CI upper < 0, or a large negative point estimate
      ≤ −MATERIAL_F1_LIFT) ⇒ a robust NO-GO even underpowered: record the negative
      + the index-key-enrichment pivot; **no stage 2**.
    * Δ positive / CI straddles 0 / borderline ⇒ underpowered to call it: recommend
      **stage 2** (extend the graph to N=1165 ≥3-hop + an unanswerable set, then the
      full ~$38 powered run)."""
    clear_loss = (f1_ci_high < 0.0) or (f1_delta <= -MATERIAL_F1_LIFT)
    if clear_loss:
        return {
            "recommendation": "no_stage2_robust_no_go",
            "rationale": (
                "ppr-fusion ΔF1 is clearly negative (CI upper < 0 or point estimate "
                f"≤ −{MATERIAL_F1_LIFT}); a clear loss needs no power. Record the "
                "negative + redirect to index-key enrichment ([[graph-arm-doesnt-beat-"
                "bm25-pivot]]). No stage-2 budget."
            ),
            "run_stage2": False,
        }
    return {
        "recommendation": "recommend_stage2",
        "rationale": (
            "ppr-fusion ΔF1 is positive / its CI straddles 0 / borderline — "
            "underpowered (N≈144 ≪ 1165) to call it. Recommend stage 2: extend the "
            "graph to N=1165 ≥3-hop + an unanswerable set, then the full ~$38 powered "
            "run. Do NOT run stage 2 here."
        ),
        "run_stage2": True,
    }


# --------------------------------------------------------------------------- #
# Artifact assembly + run
# --------------------------------------------------------------------------- #


def _five_arm_pooled_f1_table(baseline_art: Mapping[str, Any]) -> dict[str, Any]:
    """The 5-arm pooled ≥3-hop F1/EM table straight from the run aggregation."""
    pooled = baseline_art["primary_cell_pooled_ge3hop"]
    return {arm: {"f1": pooled[arm]["f1"], "em": pooled[arm]["em"], "n": pooled[arm]["n"]} for arm in pooled}


def build_verdict_artifact(
    baseline_art: Mapping[str, Any],
    *,
    treatment: str = TREATMENT_ARM,
    comparator: str = COMPARATOR_ARM,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
    power_ok: bool = False,
) -> dict[str, Any]:
    """Compute the endpoint, derive the verdict from ``decide()``, assemble the
    Slice-20 verdict artifact (schema ``0.8.2-m1-verdict-v1``)."""
    endpoint = compute_endpoint(
        baseline_art["paired_records"],
        treatment=treatment,
        comparator=comparator,
        n_boot=n_boot,
        seed=seed,
    )
    # RED STUB — a post-hoc string verdict, NOT yet derived from the imported
    # decide(); no decide_inputs recorded. The mechanical-derivation test fails here.
    verdict = "NO_GO"
    pooled = endpoint["pooled_ge3hop"]
    stage2 = stage2_recommendation(pooled["f1_delta"], pooled["f1_ci_low"], pooled["f1_ci_high"])

    return {
        "schema": "0.8.2-m1-verdict-v1",
        "stage": "stage-1 (299-graph, HITL-authorized ~$10 run)",
        "musique_hash": baseline_art.get("musique_hash", MUSIQUE_HASH),
        "arms": list(VERDICT_ARMS),
        "comparator_arm": comparator,
        "treatment_arm": treatment,
        "n_questions": baseline_art.get("n_questions"),
        "n_answerable": baseline_art.get("n_answerable"),
        "primary_endpoint": endpoint,
        "five_arm_pooled_ge3hop": _five_arm_pooled_f1_table(baseline_art),
        "per_hop_arms": baseline_art.get("per_hop"),
        "verdict": verdict,
        "verdict_source": "imported m1_decision_rule.decide (frozen; not redefined)",
        "confident_wrong_status": (
            "UNEVALUATED in stage 1 — the graph covers only answerable questions, so "
            "there is NO unanswerable contrast set; decide() is fed "
            "confident_wrong.increase_significant=False as a placeholder, NOT a measured "
            "no-confident-wrong result."
        ),
        "power_status": (
            "power_ok=False by construction — the ≥3-hop cell N≈144 is far below the "
            f"1165 the whole-rule power sim requires; decide() => {verdict} via the power "
            "gate is expected and correct for stage 1."
        ),
        "stage2_recommendation": stage2,
        "decision_rule_note": (
            "decide() is formally NO_GO via the power gate. The load-bearing scientific "
            "read is the effect size (pooled ≥3-hop ΔF1 point estimate + paired-bootstrap "
            "CI) and the stage-2 recommendation derived from it."
        ),
    }


def run_verdict(
    questions: Sequence[Question],
    answerer: BaseAnswerer,
    extractions: Mapping[str, Mapping[str, Any]],
    *,
    k: int = 10,
    encoder: Any = None,
    reranker: Any = None,
    cfg: PPRConfig = DEFAULT_PPR_CONFIG,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
    progress: Any = None,
    answer_workers: int = 1,
    power_ok: bool = False,
) -> dict[str, Any]:
    """Run the 5-arm pipeline (4 baseline + ppr_fusion) with the identical answerer
    and return the full verdict artifact (the baseline run nested under
    ``baseline_run``)."""
    baseline_art = run_baseline(
        questions,
        answerer,
        k=k,
        encoder=encoder,
        reranker=reranker,
        arms=VERDICT_ARMS,
        progress=progress,
        answer_workers=answer_workers,
        augment_rankings=ppr_augment(extractions, cfg),
    )
    art = build_verdict_artifact(
        baseline_art, n_boot=n_boot, seed=seed, power_ok=power_ok
    )
    art["baseline_run"] = baseline_art
    return art
