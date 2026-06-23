"""Slice 5b (0.8.4 / GraphRAG-parity) — AutoE pairwise-LLM-judge harness ($0, fake judge).

The harness produces input in the *exact* shape ``eval.decision_rule_084.decide_084``
consumes (the frozen rule), with the four LLM-judge bias controls wired as first-class
structure. NO real LLM, NO live batch submission — every test here uses a deterministic
``FakeJudge`` + tiny synthetic answers (design §5; [[priced-runs-need-resilience]]).

Coverage:

* the pairwise-judge prompt builder (headline metrics + directness, deterministic);
* the idempotent :class:`JudgmentKey` round-trip;
* the ABSENT-safe verdict parser (empty/None/unparseable → ABSENT, never a silent loss);
* the position-bias order-swap control (a position-biased judge nets exactly 0.5);
* ``compute_winrates`` → the ``decide_084`` ``primary_per_metric`` shape, with a
  bootstrap CI **clustered by question** and a seed **parameter** (no argless RNG);
* the BiasControls / LengthCorroboration assembly + the directness ``contradicts`` rule;
* the ``build_autoe_batch_jsonl`` integration point (fake, no submit);
* the ``project_full_cost``-reusing cost projection;
* the **contract**: ``compute_winrates`` output round-trips straight through
  ``decide_084`` (all-A-wins → REACHED with surpass candidates; all-B-wins → below_parity).

Pure-Python; runs without the native extension build.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field

import pytest

from eval.autoe_judge import (
    JUDGE_METRICS,
    ORDER_CT,
    ORDER_TC,
    BiasControls,
    Judgment,
    JudgmentKey,
    LengthCorroboration,
    assemble_bias_controls,
    assemble_length_corroboration,
    build_autoe_batch_jsonl,
    build_pairwise_prompt,
    compute_winrates,
    length_contradicts,
    parse_verdict,
    project_autoe_cost,
    run_autoe,
)
from eval.decision_rule_084 import HEADLINE_METRICS, decide_084

# --------------------------------------------------------------------------- #
# Fakes + tiny synthetic answers (NO real LLM).
# --------------------------------------------------------------------------- #

WIN_TOKEN = "WINNER"  # an answer carrying this token "wins" the marked metrics.


@dataclass
class FakeJudge:
    """Deterministic, content-aware judge: for each metric it prefers the answer
    carrying that metric's token (``WIN_TOKEN`` by default). Content-aware (not
    position-aware) is the whole point — the order-swap control then nets a true
    win, not a position artifact."""

    family: str = "fake-judge-family"
    tokens: dict[str, str] = field(default_factory=dict)

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> str:
        verdicts: dict[str, str] = {}
        for m in metrics:
            tok = self.tokens.get(m, WIN_TOKEN)
            a, b = tok in answer_a, tok in answer_b
            verdicts[m] = "A" if (a and not b) else "B" if (b and not a) else "tie"
        return json.dumps(verdicts)


@dataclass
class PositionBiasedJudge:
    """A pathological judge that ALWAYS prefers whichever answer is shown first (A),
    ignoring content — the failure mode the order-swap control must neutralize."""

    family: str = "position-biased-family"

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> str:
        return json.dumps({m: "A" for m in metrics})


def _answers(treatment_wins: bool) -> dict[str, dict[str, str]]:
    """Two arms × three questions. ``s1`` (treatment) carries WIN_TOKEN when
    ``treatment_wins`` else ``graphrag`` (comparator) carries it."""
    qids = ("q1", "q2", "q3")
    winner, loser = ("s1", "graphrag") if treatment_wins else ("graphrag", "s1")
    return {
        winner: {q: f"answer for {q} {WIN_TOKEN} and more detail" for q in qids},
        loser: {q: f"plain answer for {q}" for q in qids},
    }


_PAIR = ("s1", "graphrag")
_QUESTIONS = [("q1", "What happened?"), ("q2", "Why?"), ("q3", "Who?")]


# --------------------------------------------------------------------------- #
# 1 — pairwise-judge prompt builder
# --------------------------------------------------------------------------- #


def test_prompt_builder_is_deterministic_and_carries_all_metrics() -> None:
    p1 = build_pairwise_prompt("Q?", "answer alpha", "answer beta", JUDGE_METRICS)
    p2 = build_pairwise_prompt("Q?", "answer alpha", "answer beta", JUDGE_METRICS)
    assert p1 == p2  # deterministic given inputs
    assert "Q?" in p1
    assert "answer alpha" in p1 and "answer beta" in p1
    for m in HEADLINE_METRICS:
        assert m in p1
    assert "directness" in p1  # the length-bias corroboration metric, kept separate
    # asks for an A/B/tie choice
    assert "A" in p1 and "B" in p1 and "tie" in p1


def test_directness_is_separate_from_headline_metrics() -> None:
    assert "directness" in JUDGE_METRICS
    assert "directness" not in HEADLINE_METRICS
    assert set(HEADLINE_METRICS).issubset(set(JUDGE_METRICS))


# --------------------------------------------------------------------------- #
# 2 — idempotent JudgmentKey
# --------------------------------------------------------------------------- #


def test_judgment_key_round_trips_through_custom_id() -> None:
    key = JudgmentKey(question_id="q1", pair=("s1", "graphrag"), run_idx=3, order=ORDER_TC)
    cid = key.to_custom_id()
    assert JudgmentKey.from_custom_id(cid) == key


def test_judgment_key_is_hashable_and_idempotent() -> None:
    a = JudgmentKey("q1", ("s1", "graphrag"), 0, ORDER_TC)
    b = JudgmentKey("q1", ("s1", "graphrag"), 0, ORDER_TC)
    assert a == b
    assert hash(a) == hash(b)
    assert len({a, b}) == 1


# --------------------------------------------------------------------------- #
# 3 — ABSENT-safe verdict parser
# --------------------------------------------------------------------------- #


def test_parse_verdict_valid_json() -> None:
    out = parse_verdict(
        json.dumps({"comprehensiveness": "A", "diversity": "tie", "empowerment": "B"}),
        ("comprehensiveness", "diversity", "empowerment"),
    )
    assert out == {"comprehensiveness": "A", "diversity": "tie", "empowerment": "B"}


def test_parse_verdict_tolerates_code_fences_and_prose() -> None:
    raw = 'Here is my call:\n```json\n{"comprehensiveness": "a", "diversity": "B"}\n```\n'
    out = parse_verdict(raw, ("comprehensiveness", "diversity"))
    assert out == {"comprehensiveness": "A", "diversity": "B"}


@pytest.mark.parametrize("bad", ["", "   ", None, "not json at all", "{broken"])
def test_parse_verdict_empty_or_unparseable_is_absent_never_loss(bad) -> None:
    out = parse_verdict(bad, ("comprehensiveness", "diversity"))
    assert out == {"comprehensiveness": "ABSENT", "diversity": "ABSENT"}


def test_parse_verdict_missing_metric_and_bad_value_are_absent() -> None:
    out = parse_verdict(
        json.dumps({"comprehensiveness": "A", "diversity": "maybe"}),
        ("comprehensiveness", "diversity", "empowerment"),
    )
    assert out["comprehensiveness"] == "A"
    assert out["diversity"] == "ABSENT"  # unparseable value, not a silent tie/loss
    assert out["empowerment"] == "ABSENT"  # missing metric


# --------------------------------------------------------------------------- #
# 4 — win-rate aggregation → decide_084 input shape
# --------------------------------------------------------------------------- #


def test_compute_winrates_shape_matches_decide_084_contract() -> None:
    judgments = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    primary = compute_winrates(judgments.values(), _PAIR, seed=11)
    assert set(primary) == set(HEADLINE_METRICS)
    for m in HEADLINE_METRICS:
        fields = primary[m]
        assert set(fields) >= {"win_rate", "ci_lo", "ci_hi", "mde", "n"}
        assert isinstance(fields["n"], int)
        assert 0.0 <= fields["win_rate"] <= 1.0


def test_compute_winrates_seed_is_a_parameter_and_deterministic() -> None:
    judgments = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    a = compute_winrates(judgments.values(), _PAIR, seed=7)
    b = compute_winrates(judgments.values(), _PAIR, seed=7)
    assert a == b  # same seed → identical bootstrap CI


def test_all_treatment_wins_gives_unit_winrate() -> None:
    judgments = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    primary = compute_winrates(judgments.values(), _PAIR, seed=3)
    for m in HEADLINE_METRICS:
        assert primary[m]["win_rate"] == pytest.approx(1.0)
        assert primary[m]["ci_lo"] == pytest.approx(1.0)


def test_position_bias_is_neutralized_by_order_swap() -> None:
    # A judge that always picks A nets EXACTLY 0.5 once both orders are averaged —
    # the position-bias control is structural, not optional.
    judgments = run_autoe(PositionBiasedJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    primary = compute_winrates(judgments.values(), _PAIR, seed=1)
    for m in HEADLINE_METRICS:
        assert primary[m]["win_rate"] == pytest.approx(0.5)


def test_run_autoe_emits_both_orders_and_is_resume_idempotent() -> None:
    judgments = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    # 3 questions × 5 runs × 2 orders
    assert len(judgments) == 3 * 5 * 2
    orders = {k.order for k in judgments}
    assert orders == {ORDER_TC, ORDER_CT}
    # Resume: feeding the prior dict back in re-derives the same keys (idempotent).
    again = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5, existing=judgments)
    assert set(again) == set(judgments)


def test_winrate_excludes_absent_from_denominator() -> None:
    # A judgment whose metric is ABSENT must not count as a loss or a tie.
    key = JudgmentKey("q1", _PAIR, 0, ORDER_TC)
    decided = Judgment(key=key, verdicts={"comprehensiveness": "A"})
    absent = Judgment(
        key=JudgmentKey("q2", _PAIR, 0, ORDER_TC), verdicts={"comprehensiveness": "ABSENT"}
    )
    primary = compute_winrates([decided, absent], _PAIR, metrics=("comprehensiveness",), seed=0)
    assert primary["comprehensiveness"]["win_rate"] == pytest.approx(1.0)
    assert primary["comprehensiveness"]["n"] == 1  # the ABSENT one is excluded


# --------------------------------------------------------------------------- #
# 5 — bias-control + length assembly
# --------------------------------------------------------------------------- #


def test_assemble_bias_controls_shape() -> None:
    bc: BiasControls = assemble_bias_controls(
        n_runs=5, judge_family="fake-judge-family", system_families=("s1-fam", "graphrag-fam")
    )
    assert bc["order_swapped"] is True
    assert bc["n_runs"] == 5
    assert bc["judge_family"] == "fake-judge-family"
    assert list(bc["system_families"]) == ["s1-fam", "graphrag-fam"]


def test_length_contradicts_rule_both_branches() -> None:
    # treatment won the headlines but LOSES directness by a margin → contradiction.
    assert (
        length_contradicts(
            {"comprehensiveness": 0.9, "diversity": 0.8, "empowerment": 0.7},
            directness_winrate=0.2,
            margin=0.1,
        )
        is True
    )
    # treatment won the headlines AND is direct enough → no contradiction.
    assert (
        length_contradicts(
            {"comprehensiveness": 0.9, "diversity": 0.8, "empowerment": 0.7},
            directness_winrate=0.6,
            margin=0.1,
        )
        is False
    )
    # comparator won the headlines and ALSO loses directness (treatment very direct) → contradiction.
    assert (
        length_contradicts(
            {"comprehensiveness": 0.2, "diversity": 0.1, "empowerment": 0.3},
            directness_winrate=0.9,
            margin=0.1,
        )
        is True
    )


def test_assemble_length_corroboration_flags_verbosity_artifact() -> None:
    # treatment wins headlines (WIN_TOKEN) but comparator wins directness (DIRECT_TOKEN).
    direct_token = "DIRECT"
    answers = {
        "s1": {q: f"{WIN_TOKEN} long answer {q}" for q in ("q1", "q2", "q3")},
        "graphrag": {q: f"{direct_token} short {q}" for q in ("q1", "q2", "q3")},
    }
    judge = FakeJudge(tokens={"directness": direct_token})
    judgments = run_autoe(judge, answers, _QUESTIONS, _PAIR, n_runs=5)
    lc: LengthCorroboration = assemble_length_corroboration(
        judgments.values(), _PAIR, ran=True, seed=5
    )
    assert lc["ran"] is True
    assert lc["contradicts"] is True


def test_assemble_length_corroboration_clean_when_directness_agrees() -> None:
    judgments = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    lc = assemble_length_corroboration(judgments.values(), _PAIR, ran=True, seed=5)
    assert lc["ran"] is True
    assert lc["contradicts"] is False  # treatment wins directness too → no artifact


# --------------------------------------------------------------------------- #
# 6 — batch-build integration point (fake, NO live submit)
# --------------------------------------------------------------------------- #


def test_build_autoe_batch_jsonl_is_well_formed_and_round_trips_keys() -> None:
    jsonl, sidecar = build_autoe_batch_jsonl(
        _answers(True), _QUESTIONS, _PAIR, judge_model="fake-judge-model", n_runs=5
    )
    lines = [ln for ln in jsonl.splitlines() if ln.strip()]
    assert len(lines) == 3 * 5 * 2  # questions × runs × orders
    assert len(sidecar) == len(lines)
    for ln in lines:
        rec = json.loads(ln)  # every line is valid JSON
        cid = rec["custom_id"]
        assert rec["body"]["model"] == "fake-judge-model"
        assert rec["url"] == "/v1/chat/completions"
        assert rec["body"]["messages"][0]["role"] == "user"
        # the custom_id encodes a resumable JudgmentKey
        key = JudgmentKey.from_custom_id(cid)
        assert key.pair == _PAIR
        assert cid in sidecar


# --------------------------------------------------------------------------- #
# 7 — cost-projection helper (reuses project_full_cost)
# --------------------------------------------------------------------------- #


def test_project_autoe_cost_reuses_project_full_cost_shape() -> None:
    proj = project_autoe_cost(
        prompt_tokens=2000,
        completion_tokens=200,
        n_calls=10,
        price_in_per_1m=0.15,
        price_out_per_1m=0.60,
        n_questions=50,
        n_pairs=1,
        n_runs=5,
    )
    # full_calls = n_questions × (n_pairs × n_runs × 2 orders)
    assert proj["projected_full_calls"] == 50 * 1 * 5 * 2
    assert proj["projected_full_usd"] >= 0.0
    assert "cost_per_call_usd" in proj


# --------------------------------------------------------------------------- #
# CONTRACT — compute_winrates output round-trips straight through decide_084.
# --------------------------------------------------------------------------- #


def _clean_controls(judge_family: str = "fake-judge-family"):
    bc = assemble_bias_controls(
        n_runs=5, judge_family=judge_family, system_families=("s1-fam", "graphrag-fam")
    )
    lc: LengthCorroboration = {"ran": True, "contradicts": False}
    return bc, lc


def test_contract_all_a_wins_round_trips_to_reached_with_surpass() -> None:
    judgments = run_autoe(FakeJudge(), _answers(True), _QUESTIONS, _PAIR, n_runs=5)
    primary = compute_winrates(judgments.values(), _PAIR, seed=42)
    bc, lc = _clean_controls()
    res = decide_084(primary, bc, lc)
    assert res["verdict"] == "REACHED"
    assert set(res["surpass_candidates"]) == set(HEADLINE_METRICS)  # ci_lo > 0.5 everywhere


def test_contract_all_b_wins_round_trips_to_below_parity() -> None:
    judgments = run_autoe(FakeJudge(), _answers(False), _QUESTIONS, _PAIR, n_runs=5)
    primary = compute_winrates(judgments.values(), _PAIR, seed=42)
    bc, lc = _clean_controls()
    res = decide_084(primary, bc, lc)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] is not None
    assert res["binding_constraint"].startswith("below_parity:")
