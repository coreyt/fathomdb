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
import inspect
import json
from pathlib import Path

import numpy as np
import pytest

from eval.m1_baseline import Paragraph, Question, run_baseline
from eval.m1_decision_rule import MATERIAL_F1_LIFT, decide
from eval.m1_baseline import run_baseline as _run_baseline
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
    prior_answers_from_artifact,
    run_verdict,
    stage2_recommendation,
    verdict_from_inputs,
)
from eval.r2_parity_eval import BaseAnswerer, StubAnswerer


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


# --------------------------------------------------------------------------- #
# (f) resume — reuse prior successful answers, (re)call ONLY the failed cells
# --------------------------------------------------------------------------- #


class CountingAnswerer(StubAnswerer):
    """A StubAnswerer that counts how many (question, context) cells it is asked."""

    model_id = "counting-stub-v1"

    def __init__(self) -> None:
        self.asked: list[str] = []

    def answer(self, question: str, context: list[str]):  # noqa: ANN201
        self.asked.append(question)
        return super().answer(question, context)


def test_prior_answers_from_artifact_skips_none() -> None:
    prior = {
        "baseline_run": {
            "paired_records": [
                {"qid": "2hop__a", "answers": {"bm25": "x", "ppr_fusion": None}},
                {"qid": "3hop__b", "answers": {"bm25": "y"}},
            ]
        }
    }
    m = prior_answers_from_artifact(prior)
    assert m[("2hop__a", "bm25")] == "x"
    assert m[("2hop__a", "ppr_fusion")] is None  # key-present None ⇒ REUSED (abstention, NOT re-called)
    assert m[("3hop__b", "bm25")] == "y"
    # an artifact with no persisted answers ⇒ empty map ⇒ a full (safe) re-run.
    assert prior_answers_from_artifact({"verdict": "NO_GO"}) == {}


def test_resume_recalls_only_failed_cells() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    # Pass 1 — full run; every (qid, arm) answered.
    art1 = run_verdict(
        qs, StubAnswerer(), ext, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        n_boot=300, seed=0,
    )
    prior = prior_answers_from_artifact(art1)
    assert all(v is not None for v in prior.values())

    # Simulate that the ppr_fusion arm FAILED for every question.
    # Failure = ABSENT key (not key-present-None, which is an abstention).
    failed = dict(prior)
    n_failed = 0
    for q in qs:
        del failed[(q.id, TREATMENT_ARM)]  # ABSENT = prior failure → must be re-called
        n_failed += 1

    # Pass 2 — resume: only the ABSENT ppr_fusion cells should be (re)called.
    spy = CountingAnswerer()
    art2 = _run_baseline(
        qs, spy, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext), prior_answers=failed,
    )
    assert len(spy.asked) == n_failed  # exactly the failed cells, nothing else
    # the reused (non-failed) arms keep pass-1 answers; ppr_fusion is freshly answered.
    recs1 = {r["qid"]: r for r in art1["baseline_run"]["paired_records"]}
    for r in art2["paired_records"]:
        for arm in VERDICT_ARMS:
            if arm != TREATMENT_ARM:
                assert r["answers"][arm] == recs1[r["qid"]]["answers"][arm]
        assert r["answers"][TREATMENT_ARM] is not None  # re-called, not left None


def test_resume_all_reused_pays_zero_calls() -> None:
    # if every cell is reusable, the resume run makes ZERO answerer calls and
    # reproduces the same endpoint as the original full run.
    qs = _mini_set()
    ext = _mini_extractions(qs)
    art1 = run_verdict(
        qs, StubAnswerer(), ext, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        n_boot=300, seed=0,
    )
    prior = prior_answers_from_artifact(art1)
    spy = CountingAnswerer()
    art2 = build_verdict_artifact(
        _run_baseline(
            qs, spy, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
            arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext), prior_answers=prior,
        ),
        n_boot=300, seed=0,
    )
    assert spy.asked == []  # nothing re-called
    assert art2["primary_endpoint"]["pooled_ge3hop"] == art1["primary_endpoint"]["pooled_ge3hop"]


def test_checkpoint_emits_resume_loadable_records() -> None:
    # the incremental checkpoint hook receives resume-shaped records mid-pass; the
    # wrapped {baseline_run:{paired_records}} must be loadable by the resume reader.
    qs = _mini_set()
    ext = _mini_extractions(qs)
    seen: list[list[dict]] = []
    run_baseline(
        qs, StubAnswerer(), k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext),
        checkpoint=lambda recs: seen.append(recs), checkpoint_every=3,
    )
    assert seen, "checkpoint hook was never called"
    last = seen[-1]
    assert {r["qid"] for r in last} == {q.id for q in qs}
    resume = prior_answers_from_artifact({"baseline_run": {"paired_records": last}})
    for q in qs:
        for arm in VERDICT_ARMS:
            assert resume[(q.id, arm)] is not None


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


# =========================================================================== #
# Slice 20 resilience redesign (HITL: resilient BY CONSTRUCTION)
#   (1) auto-resume by default   (2) failure ≠ abstention   (3) atomic checkpoint
# =========================================================================== #


class _AbstainOrFailAnswerer(BaseAnswerer):
    """A deterministic stub that, per the failure/abstention split, can either
    **succeed-with-no-answer** (a legit abstention → returns ``None``) or **fail**
    (raises, as a retry-exhausted 429/5xx/timeout would after backoff). Targeted by
    a substring of the question text (each ``_mini_question`` embeds its answer)."""

    model_id = "abstain-or-fail-stub-v1"

    def __init__(self, *, abstain: tuple[str, ...] = (), fail: tuple[str, ...] = ()) -> None:
        self.n_errors = 0
        self.asked: list[str] = []
        self._abstain = abstain
        self._fail = fail

    def answer(self, question: str, context: list[str]):  # noqa: ANN201
        self.asked.append(question)
        if any(t in question for t in self._fail):
            raise RuntimeError("simulated retry-exhausted 429 failure (call never succeeded)")
        if any(t in question for t in self._abstain):
            return None  # a SUCCESSFUL call that returned no usable answer
        return context[0] if context else None


# --------------------------------------------------------------------------- #
# (2) failure ≠ abstention
# --------------------------------------------------------------------------- #


def test_abstention_is_scored_but_failure_is_missing() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    # 3hop__a abstains (success→None); 4hop__a fails (raises after retries).
    ans = _AbstainOrFailAnswerer(abstain=("ans3a",), fail=("ans4a",))
    art = run_baseline(
        qs, ans, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext),
    )
    recs = {r["qid"]: r for r in art["paired_records"]}

    # Abstention: a SUCCESSFUL None — every arm PRESENT with value None, scored 0.
    a = recs["3hop__a"]
    assert set(a["answers"].keys()) == set(VERDICT_ARMS)
    assert all(v is None for v in a["answers"].values())
    assert all(a["f1"][arm] == 0.0 for arm in VERDICT_ARMS)

    # Failure: the call never succeeded — the cells are MISSING, NEVER persisted as
    # a scored abstention (no None=F1=0 deflation), and excluded from scoring.
    f = recs["4hop__a"]
    assert f["answers"] == {}
    assert f["f1"] == {}
    assert f["em"] == {}

    # The five failed arms count as errors (→ answer_completeness), abstentions do not.
    assert ans.n_errors == len(VERDICT_ARMS)


def test_failure_recalled_on_resume_excluded_from_answered_set() -> None:
    qs = _mini_set()
    ext = _mini_extractions(qs)
    ans1 = _AbstainOrFailAnswerer(fail=("ans4a",))
    art1 = run_baseline(
        qs, ans1, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext),
    )
    prior = prior_answers_from_artifact({"baseline_run": {"paired_records": art1["paired_records"]}})
    # The failed cells are absent from the answered set (not persisted as None).
    for arm in VERDICT_ARMS:
        assert ("4hop__a", arm) not in prior

    spy = CountingAnswerer()  # everything succeeds on the resume pass
    art2 = run_baseline(
        qs, spy, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext), prior_answers=prior,
    )
    # ONLY the previously-missing cells are re-called — zero re-spend elsewhere.
    assert len(spy.asked) == len(VERDICT_ARMS)
    recs2 = {r["qid"]: r for r in art2["paired_records"]}
    assert set(recs2["4hop__a"]["answers"].keys()) == set(VERDICT_ARMS)
    assert all(v is not None for v in recs2["4hop__a"]["answers"].values())


# --------------------------------------------------------------------------- #
# [absten] codex §9 [P2] — resume must REUSE key-present-None abstentions
# --------------------------------------------------------------------------- #


def test_abstention_is_reused_not_recalled() -> None:
    """[absten][RED→GREEN] A key-present-None in prior_answers is a SUCCESSFUL
    abstention that must be REUSED (no re-call).  An ABSENT key is a prior
    failure that must be re-called.  This test fails against the old
    ``if prev is not None`` logic (which re-calls abstentions) and passes once
    ``_do`` uses the membership-based ``if key in prior_answers`` guard."""
    qs = _mini_set()
    ext = _mini_extractions(qs)

    # Build a prior where, for qs[0] ("2hop__a"):
    #   - "bm25" arm:  ABSENT key  (simulated prior failure → must be re-called)
    #   - all other arms: key-present-None  (prior abstention → must be REUSED)
    # All other questions have non-None cached values (never re-called).
    absent_q = qs[0]
    absent_arm = VERDICT_ARMS[0]  # "bm25"
    prior: dict[tuple[str, str], str | None] = {}
    for q in qs:
        for arm in VERDICT_ARMS:
            if q.id == absent_q.id and arm == absent_arm:
                pass  # deliberately ABSENT = prior failure
            elif q.id == absent_q.id:
                prior[(q.id, arm)] = None  # key-present-None = prior abstention
            else:
                prior[(q.id, arm)] = "cached-answer"

    spy = CountingAnswerer()
    run_baseline(
        qs, spy, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext), prior_answers=prior,
    )

    # Exactly ONE re-call: the absent failure cell.  The key-present-None
    # abstentions must NOT generate any re-calls.
    assert len(spy.asked) == 1, (
        f"Expected 1 re-call (the absent failure cell); got {len(spy.asked)}. "
        "key-present-None abstentions are being incorrectly re-called!"
    )
    # That one call must be for the question whose arm was absent.
    assert spy.asked[0] == absent_q.question


# --------------------------------------------------------------------------- #
# (1) auto-resume by default
# --------------------------------------------------------------------------- #


_PRESERVED_CHECKPOINT = Path(
    "/home/coreyt/projects/fathomdb/data/corpus-data/0.8.2-m1-verdict.checkpoint.json"
)


@pytest.mark.skipif(
    not _PRESERVED_CHECKPOINT.exists(), reason="preserved 214-cell checkpoint not present"
)
def test_auto_resume_reuses_preserved_214_cells() -> None:
    ckpt = json.loads(_PRESERVED_CHECKPOINT.read_text(encoding="utf-8"))
    recs = ckpt["baseline_run"]["paired_records"]
    prior = prior_answers_from_artifact(ckpt)
    # All keys in prior are reusable: non-None answers AND key-present-None abstentions.
    # (Before the abstention fix, n_reusable counted only non-None values = 214.
    #  Post-fix: all persisted cells are reused, including None abstentions.)
    n_reusable = len(prior)  # membership-based count: every persisted (qid, arm) cell
    assert n_reusable >= 214  # at least the 214 non-None cells must be present

    qs = [
        _mini_question(r["qid"], int(r["hop_count"]), f"a{i}")
        for i, r in enumerate(recs)
        if r.get("answerable", True)
    ]
    ext = _mini_extractions(qs)
    spy = CountingAnswerer()
    run_baseline(
        qs, spy, k=10, encoder=FakeEncoder(), reranker=FakeReranker(),
        arms=VERDICT_ARMS, augment_rankings=ppr_augment(ext), prior_answers=prior,
    )
    expected_cells = len(qs) * len(VERDICT_ARMS)
    assert len(spy.asked) == expected_cells - n_reusable  # re-call ONLY the absent cells


def test_resolve_resume_auto_detects_sidecar_without_a_flag(tmp_path) -> None:
    from eval.m1_verdict_run import _resolve_resume

    out = tmp_path / "verdict.json"
    sidecar = out.with_suffix(".checkpoint.json")
    # nothing yet → no resume source
    assert _resolve_resume(out, None, None) is None
    # the sidecar exists → auto-detected WITHOUT a manual --resume flag
    sidecar.write_text("{}", encoding="utf-8")
    assert _resolve_resume(out, None, None) == sidecar
    # an explicit --resume path overrides the auto-detected sidecar
    other = tmp_path / "preserved.checkpoint.json"
    other.write_text("{}", encoding="utf-8")
    assert _resolve_resume(out, other, None) == other
    # an explicit checkpoint path is the one auto-detected
    ckpt = tmp_path / "ck.json"
    ckpt.write_text("{}", encoding="utf-8")
    assert _resolve_resume(out, None, ckpt) == ckpt


def _distinct_question(qid: str, hop: int, answer: str) -> Question:
    """A question whose 20 paragraphs have DISTINCT lexical content (so BM25 has a
    real ranking) — used by the run()-level guard, which aborts if ppr_fusion is
    silently identical to BM25 on every question."""
    paras = tuple(
        Paragraph(
            idx=i, title=f"T{i}",
            text=f"paragraph {i} keyword{i} about {answer if i == 0 else 'topic' + str(i)}",
            is_supporting=(i == 0),
        )
        for i in range(20)
    )
    return Question(
        id=qid, question=f"what is {answer} keyword3 keyword7?", hop_count=hop,
        answer=answer, answer_aliases=(), answerable=True, paragraphs=paras,
    )


def _star_extractions(questions: list[Question]) -> dict[str, dict]:
    """A hub graph that lets PPR pull high-index paragraphs into the top-k (so the
    ppr_fusion arm materially diverges from BM25 — clears the run() vacuity guard)."""
    out: dict[str, dict] = {}
    for q in questions:
        for p in q.paragraphs:
            rels = [{"subject": f"E{p.idx}", "object": "HUB"}]
            if p.idx in (15, 16, 17, 18, 19):
                rels += [{"subject": "HUB", "object": f"E{p.idx}"},
                         {"subject": f"E{p.idx}", "object": "E0"}]
            out[f"{q.id}#{p.idx}"] = {
                "entities": [{"name": f"E{p.idx}"}, {"name": "HUB"}], "relations": rels,
            }
    return out


def test_run_auto_resumes_with_no_flag_zero_respend(tmp_path) -> None:
    from eval.m1_verdict_run import run

    qs = [_distinct_question(f"{h}hop__{t}", h, f"ans{h}{t}")
          for h in (2, 3, 4) for t in ("a", "b", "c")]
    ext = _star_extractions(qs)
    out = tmp_path / "verdict.json"

    # Pass 1 — a full priced-mode run (stub answerer) that writes the sidecar checkpoint.
    spy1 = CountingAnswerer()
    run(
        mode="priced", reader="stub", corpus=Path("/nonexistent"),
        extractions_path=Path("/nonexistent"), output=out, k=3, n_boot=80,
        questions=qs, extractions=ext, answerer=spy1,
        encoder=FakeEncoder(), reranker=FakeReranker(),
    )
    sidecar = out.with_suffix(".checkpoint.json")
    assert sidecar.exists()
    assert spy1.asked, "pass 1 should have made the priced calls"

    # Pass 2 — relaunch with NO --resume flag. Auto-resume must re-use every cell →
    # ZERO re-spend (the exact loop the un-resumed run kept paying for).
    spy2 = CountingAnswerer()
    run(
        mode="priced", reader="stub", corpus=Path("/nonexistent"),
        extractions_path=Path("/nonexistent"), output=out, k=3, n_boot=80,
        questions=qs, extractions=ext, answerer=spy2,
        encoder=FakeEncoder(), reranker=FakeReranker(),
    )
    assert spy2.asked == []  # auto-resumed from the sidecar; nothing re-called


# --------------------------------------------------------------------------- #
# (3) atomic, frequent checkpoint
# --------------------------------------------------------------------------- #


def test_atomic_write_json_round_trips(tmp_path) -> None:
    from eval.m1_verdict_run import _atomic_write_json

    p = tmp_path / "out.checkpoint.json"
    _atomic_write_json(p, {"answered": 100})
    assert json.loads(p.read_text(encoding="utf-8"))["answered"] == 100
    # the temp file is never left behind as the live path
    assert not (p.with_name(p.name + ".tmp")).exists()


def test_atomic_checkpoint_survives_midwrite_death(tmp_path, monkeypatch) -> None:
    import eval.m1_verdict_run as mod
    from eval.m1_verdict_run import _atomic_write_json

    p = tmp_path / "out.checkpoint.json"
    _atomic_write_json(p, {"answered": 100})  # a good prior checkpoint

    def _boom(*_a: object, **_k: object) -> str:
        raise RuntimeError("process killed mid-serialize")

    monkeypatch.setattr(mod.json, "dumps", _boom)
    with pytest.raises(RuntimeError):
        _atomic_write_json(p, {"answered": 200})

    # os.replace never ran → the live checkpoint is the intact PRIOR one, never a
    # half-written file. No corruption on a mid-write death.
    assert json.loads(p.read_text(encoding="utf-8"))["answered"] == 100


def test_checkpoint_cadence_is_at_least_every_10_questions() -> None:
    from eval.m1_verdict_run import run

    default = inspect.signature(run).parameters["checkpoint_every"].default
    assert default <= 10  # frequent: checkpoint at least every ~10 questions
