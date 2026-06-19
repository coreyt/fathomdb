"""M1 adjudication VERDICT harness — TDD (RED first), Slice 20 (stage 1).

Binding spec: ``dev/plans/plan-0.8.2.md`` §4 (Slice 20) + the SIGNED
``dev/design/0.8.2-m1-multihop-harness.md`` §4. These tests pin the load-bearing
contracts of the verdict harness — none of which needs a priced LLM call (a
``StubAnswerer`` + deterministic fake encoder/reranker stand in):

(a) the 5-arm pipeline runs (4 baseline arms + ``ppr_fusion``) with the identical
    answerer, over the graph-covered answerable questions;
(b) the **pre-registered** primary endpoint is the pooled ≥3-hop ΔF1 of
    ``ppr_fusion`` vs the **fixed ``fused`` (fused-RRF k=60) comparator** — NOT
    ``fused_rerank`` (design AMENDED 2026-06-19) — with a paired-bootstrap CI;
(c) the GO/NO-GO is derived **mechanically** from the imported frozen
    :func:`m1_decision_rule.decide` — the artifact's ``verdict`` equals
    ``decide(**artifact["decide_inputs"])`` (no post-hoc rule);
(d) stage 1 feeds ``power_ok=False`` (N≪1165) and
    ``confident_wrong.increase_significant=False`` (UNEVALUATED — no unanswerable
    graph coverage) ⇒ ``decide()`` returns NO_GO via the power gate;
(e) the stage-2 recommendation is derived from the effect size, and ``ppr_fusion``
    is sanity-checked to not be silently identical to BM25.
"""

from __future__ import annotations

import hashlib

import numpy as np

from eval.m1_baseline import Paragraph, Question, run_baseline
from eval.m1_decision_rule import MATERIAL_F1_LIFT, decide
from eval.m1_verdict import (
    COMPARATOR_ARM,
    TREATMENT_ARM,
    VERDICT_ARMS,
    build_verdict_artifact,
    compute_endpoint,
    decide_inputs,
    graph_qids,
    ppr_augment,
    ppr_divergence,
    run_verdict,
    stage2_recommendation,
    verdict_from_inputs,
)
from eval.r2_parity_eval import StubAnswerer


# --------------------------------------------------------------------------- #
# Deterministic fakes (no model / no engine) — mirror test_m1_baseline
# --------------------------------------------------------------------------- #


class FakeEncoder:
    def encode(self, text: str) -> np.ndarray:
        h = hashlib.sha256(text.encode()).digest()
        v = np.frombuffer(h, dtype=np.uint8).astype(np.float32)[:16]
        return v / (np.linalg.norm(v) + 1e-9)


class FakeReranker:
    def rank_fused(self, query, passages, fused_scored, *, depth=None):  # noqa: ANN001
        return [i for i, _ in fused_scored], len(fused_scored)


def _mini_question(qid: str, hop: int, answer: str) -> Question:
    paras = tuple(
        Paragraph(
            idx=i,
            title=f"T{i}",
            text=f"body text number {i} about {answer if i == 0 else 'distractor'}",
            is_supporting=(i == 0),
        )
        for i in range(20)
    )
    return Question(
        id=qid, question=f"what is {answer}?", hop_count=hop, answer=answer,
        answer_aliases=(), answerable=True, paragraphs=paras,
    )


def _mini_set() -> list[Question]:
    qs: list[Question] = []
    for hop in (2, 3, 4):
        for tag in ("a", "b", "c"):
            qs.append(_mini_question(f"{hop}hop__{tag}", hop, f"ans{hop}{tag}"))
    return qs


def _mini_extractions(questions: list[Question]) -> dict[str, dict]:
    """One extraction per (qid, para_idx): a SHARED hub + a per-passage entity, so
    PPR has a non-trivial graph to propagate over (and diverges from BM25)."""
    out: dict[str, dict] = {}
    for q in questions:
        for p in q.paragraphs:
            out[f"{q.id}#{p.idx}"] = {
                "entities": [{"name": f"E{p.idx}"}, {"name": "SHARED"}],
                "relations": [{"subject": f"E{p.idx}", "object": "SHARED"}],
            }
    return out


# --------------------------------------------------------------------------- #
# (a) 5-arm pipeline + augment hook
# --------------------------------------------------------------------------- #


def test_augment_hook_adds_fifth_arm() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    art = run_baseline(
        qs, StubAnswerer(), k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext),
    )
    assert list(art["arms"]) == list(VERDICT_ARMS)
    assert TREATMENT_ARM in art["arms"]
    # every per-hop + pooled cell carries all five arms (incl. ppr_fusion)
    for hop in ("2", "3", "4"):
        assert set(art["per_hop"][hop].keys()) == set(VERDICT_ARMS)
    assert set(art["primary_cell_pooled_ge3hop"].keys()) == set(VERDICT_ARMS)


def test_comparator_is_fused_not_rerank() -> None:
    # the AMENDED 2026-06-19 fixed comparator is fused-RRF (k=60) = the "fused" arm.
    assert COMPARATOR_ARM == "fused"
    assert TREATMENT_ARM == "ppr_fusion"


# --------------------------------------------------------------------------- #
# (b) primary endpoint: pooled ≥3-hop ΔF1 vs fused + paired bootstrap
# --------------------------------------------------------------------------- #


def test_endpoint_pooled_ge3hop_paired_bootstrap() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    art = run_verdict(
        qs, StubAnswerer(), ext, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        n_boot=500, seed=0,
    )
    ep = art["primary_endpoint"]
    assert ep["comparator_arm"] == "fused"
    assert ep["treatment_arm"] == "ppr_fusion"
    pooled = ep["pooled_ge3hop"]
    # the ≥3-hop cell = the 3-hop + 4-hop answerable questions (6 here)
    assert pooled["n"] == sum(1 for q in qs if q.hop_count >= 3)
    # CI brackets the point estimate, all finite
    assert pooled["f1_ci_low"] <= pooled["f1_delta"] <= pooled["f1_ci_high"]
    for key in ("f1_delta", "f1_ci_low", "f1_ci_high", "em_ci_high"):
        assert np.isfinite(pooled[key])
    # per-hop secondary splits present for 2/3/4
    assert set(ep["per_hop"].keys()) == {"2", "3", "4"}


def test_endpoint_deterministic_given_seed() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    base = run_baseline(
        qs, StubAnswerer(), k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext),
    )
    a = compute_endpoint(base["paired_records"], n_boot=500, seed=0)
    b = compute_endpoint(base["paired_records"], n_boot=500, seed=0)
    assert a == b


# --------------------------------------------------------------------------- #
# (c) the verdict is derived MECHANICALLY from the imported decide()
# --------------------------------------------------------------------------- #


def test_verdict_derived_mechanically_from_decide() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    art = run_verdict(
        qs, StubAnswerer(), ext, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        n_boot=500, seed=0,
    )
    # re-run the imported frozen rule on the artifact's OWN recorded inputs.
    inputs = art["decide_inputs"]
    recomputed = decide(
        material=inputs["material"], em=inputs["em"], trend=inputs["trend"],
        confident_wrong=inputs["confident_wrong"], power_ok=inputs["power_ok"],
    )
    assert art["verdict"] == recomputed
    # convenience wrapper agrees
    assert verdict_from_inputs(inputs) == art["verdict"]


def test_stage1_power_gate_forces_no_go() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    art = run_verdict(
        qs, StubAnswerer(), ext, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        n_boot=500, seed=0,
    )
    inputs = art["decide_inputs"]
    # stage-1 invariants: power_ok False, confident-wrong UNEVALUATED placeholder.
    assert inputs["power_ok"] is False
    assert inputs["confident_wrong"]["increase_significant"] is False
    assert art["verdict"] == "NO_GO"
    # the loud notes must be present in the artifact.
    assert "UNEVALUATED" in art["confident_wrong_status"]
    assert "power_ok=False" in art["power_status"]


def test_decide_inputs_power_ok_true_path_is_live() -> None:
    # guard against a hard-coded NO_GO: with power_ok=True + a fabricated material
    # lift the same machinery yields GO (so the power gate is what forces stage-1
    # NO_GO, not a frozen string).
    ep = {
        "pooled_ge3hop": {"f1_delta": 0.10, "f1_ci_low": 0.02, "f1_ci_high": 0.18,
                          "em_delta": 0.0, "em_ci_low": 0.0, "em_ci_high": 0.05},
        "trend": {"neg_significant": False},
    }
    inputs = decide_inputs(ep, power_ok=True)
    assert verdict_from_inputs(inputs) == "GO"
    inputs_np = decide_inputs(ep, power_ok=False)
    assert verdict_from_inputs(inputs_np) == "NO_GO"


# --------------------------------------------------------------------------- #
# (d) stage-2 recommendation from the effect size
# --------------------------------------------------------------------------- #


def test_stage2_clear_loss_recommends_no_stage2() -> None:
    # CI upper < 0 ⇒ robust NO-GO, no stage 2.
    rec = stage2_recommendation(f1_delta=-0.05, f1_ci_low=-0.09, f1_ci_high=-0.01)
    assert rec["run_stage2"] is False
    # large negative point estimate ⇒ also clear loss.
    rec2 = stage2_recommendation(f1_delta=-MATERIAL_F1_LIFT, f1_ci_low=-0.10, f1_ci_high=0.02)
    assert rec2["run_stage2"] is False


def test_stage2_borderline_recommends_stage2() -> None:
    rec = stage2_recommendation(f1_delta=0.01, f1_ci_low=-0.02, f1_ci_high=0.05)
    assert rec["run_stage2"] is True
    rec2 = stage2_recommendation(f1_delta=0.0, f1_ci_low=-0.03, f1_ci_high=0.03)
    assert rec2["run_stage2"] is True


# --------------------------------------------------------------------------- #
# (e) ppr_fusion is not silently identical to bm25 + graph-qid selection
# --------------------------------------------------------------------------- #


def test_ppr_divergence_reports_structure() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    div = ppr_divergence(qs, ext, k=5)
    assert div["n_questions"] == len(qs)
    assert 0.0 <= div["fraction_differs"] <= 1.0
    assert isinstance(div["silently_identical_to_bm25"], bool)


def test_graph_qids_extracts_question_ids() -> None:
    ext = {"2hop__10114_599630#0": {}, "2hop__10114_599630#11": {}, "3hop1__a#3": {}}
    assert graph_qids(ext) == {"2hop__10114_599630", "3hop1__a"}


def test_build_artifact_schema_and_five_arm_table() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    base = run_baseline(
        qs, StubAnswerer(), k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext),
    )
    art = build_verdict_artifact(base, n_boot=300, seed=0)
    assert art["schema"] == "0.8.2-m1-verdict-v1"
    assert set(art["five_arm_pooled_ge3hop"].keys()) == set(VERDICT_ARMS)
    assert art["comparator_arm"] == "fused"
    assert "stage2_recommendation" in art
