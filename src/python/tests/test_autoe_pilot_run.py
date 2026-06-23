"""Slice 5b-runner (0.8.4 / GraphRAG-parity) — resilient AutoE pilot runner ($0).

Every test here is **$0 and offline**: a deterministic ``FakeJudge`` / fake answerer
over a tiny synthetic document map + question list — NO real LLM, NO live airlock,
NO corpus payload (absent in the worktree). Coverage:

* :class:`LLMJudge` — env wiring + the ``R2_RUN`` gate (not available / refuses
  unset), and that the backoff seam retries a transient 429 then succeeds, and an
  exhausted / non-retryable failure → ``None`` (ABSENT, never a fabricated verdict);
* :func:`run_pilot` — the happy-path report (win-rates, bias controls, length
  corroboration, the kill-early premise, the cost projection);
* the **cross-family self-preference guard** raising BEFORE any spend;
* resilience: an atomic checkpoint per judged key + idempotent ``--resume`` that
  re-judges ONLY dead cells (a counting judge proves zero re-spend on resume);
* the ``BudgetLedger`` ``--max-usd`` pre-call guard raising before exceeding;
* empty / all-ABSENT → an ABSENT status report, never a fabricated win-rate;
* the cost projection scaling with ``--target-questions``;
* the ``--cheap-validate`` tiny-N mode;
* ``main()`` gated by ``R2_RUN`` (returns non-zero, no network, when unset).

Pure-Python; runs without the native extension build.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

import pytest

from eval.apnews_corpus import AutoQQuestion
from eval.autoe_pilot_run import (
    CHEAP_LIMIT,
    CHEAP_N_RUNS,
    DEFAULT_PAIR,
    BudgetExceeded,
    LLMJudge,
    family_of,
    main,
    run_pilot,
)
from eval.decision_rule_084 import MIN_RUNS
from eval.r2_parity_eval import BaseAnswerer

WIN_TOKEN = "WINNER"


# --------------------------------------------------------------------------- #
# Fakes (NO real LLM / no network).
# --------------------------------------------------------------------------- #
@dataclass
class FakeJudge:
    """Content-aware judge: for each metric it prefers whichever answer carries
    ``WIN_TOKEN`` (position-independent — so the order-swap control nets a true win,
    not a position artifact). Exposes usage counters for the cost projection."""

    family: str = "judgefam"
    last_prompt_tokens: int = 100
    last_completion_tokens: int = 10
    calls: int = 0

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> Optional[str]:
        self.calls += 1
        verdicts: dict[str, str] = {}
        for m in metrics:
            a, b = WIN_TOKEN in answer_a, WIN_TOKEN in answer_b
            verdicts[m] = "A" if (a and not b) else "B" if (b and not a) else "tie"
        return json.dumps(verdicts)


@dataclass
class NoneJudge:
    """A judge whose every call fails (returns ``None``) — drives the ABSENT path."""

    family: str = "judgefam"
    last_prompt_tokens: int = 0
    last_completion_tokens: int = 0
    calls: int = 0

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> Optional[str]:
        self.calls += 1
        return None


class FakeAnswerer(BaseAnswerer):
    """Deterministic, LLM-free answerer: echoes the top retrieved passage (so an
    answer carries ``WIN_TOKEN`` iff its arm's top hit does), abstains on no context."""

    model_id = "fake-answerer-v1"

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        return context[0] if context else None


# Synthetic corpus: the ``dogs`` doc carries WIN_TOKEN and matches the question, so
# the dense VectorRAG arm ranks it first (its answer wins); the long-context control
# packs by doc_id so its top passage is the non-WIN cats doc (it loses).
_DOCS = {
    "a_cats": "plain text about cats and gardens, nothing special here",
    "z_dogs": f"{WIN_TOKEN} detailed discussion of dogs and the training topic at length",
}
_QUESTIONS = [
    AutoQQuestion(
        bucket="activity_global",
        family="activity",
        scope="global",
        question_text="dogs training topic",
        question_id="q1",
    ),
    AutoQQuestion(
        bucket="data_local",
        family="data",
        scope="local",
        question_text="dogs topic details",
        question_id="q2",
    ),
    AutoQQuestion(
        bucket="activity_local",
        family="activity",
        scope="local",
        question_text="the dogs topic",
        question_id="q3",
    ),
]
_FAMILIES = {"vector_rag": "answererfam", "long_context": "answererfam"}


def _run(judge: Any, **kw: Any) -> dict[str, Any]:
    base: dict[str, Any] = dict(
        answerer=FakeAnswerer(),
        judge=judge,
        documents=_DOCS,
        questions=_QUESTIONS,
        families=_FAMILIES,
        n_runs=5,
        n_boot=200,
        judge_model="gemini-2.5-flash-lite",
    )
    base.update(kw)
    return run_pilot(**base)


# --------------------------------------------------------------------------- #
# LLMJudge — env wiring + R2_RUN gate
# --------------------------------------------------------------------------- #
def test_family_of_extracts_coarse_family() -> None:
    assert family_of("gpt-5.4") == "gpt"
    assert family_of("gemini-2.5-flash-lite") == "gemini"


def test_llmjudge_unavailable_and_gated_without_r2_run(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("R2_RUN", raising=False)
    monkeypatch.setenv("R2_JUDGE_BASE_URL", "http://localhost:9/v1")
    monkeypatch.setenv("R2_JUDGE_MODEL", "gemini-2.5-flash-lite")
    j = LLMJudge()
    assert j.available is False
    assert j.family == "gemini"
    with pytest.raises(RuntimeError):
        j.judge_pair("q?", "a", "b", ("comprehensiveness",))


def test_llmjudge_retries_transient_then_succeeds(monkeypatch: pytest.MonkeyPatch) -> None:
    import urllib.error

    monkeypatch.setenv("R2_RUN", "1")
    monkeypatch.setenv("R2_JUDGE_BASE_URL", "http://localhost:9/v1")
    monkeypatch.setenv("R2_JUDGE_MODEL", "gemini-2.5-flash-lite")
    monkeypatch.setenv("R2_JUDGE_API_KEY", "k")

    class _Resp:
        def __enter__(self):
            return self

        def __exit__(self, *a):
            return False

        def read(self):
            return json.dumps(
                {
                    "choices": [{"message": {"content": '{"comprehensiveness": "A"}'}}],
                    "usage": {"prompt_tokens": 12, "completion_tokens": 3},
                }
            ).encode("utf-8")

    calls = {"n": 0}

    def _fake_open(self, req):
        calls["n"] += 1
        if calls["n"] == 1:
            raise urllib.error.HTTPError("u", 429, "rate", {}, None)  # type: ignore[arg-type]
        return _Resp()

    sleeps: list[float] = []
    j = LLMJudge(sleep=lambda s: sleeps.append(s))
    monkeypatch.setattr(LLMJudge, "_open", _fake_open)
    out = j.judge_pair("q?", "a", "b", ("comprehensiveness",))
    assert out == '{"comprehensiveness": "A"}'
    assert j.n_retries == 1 and j.n_calls == 1 and sleeps  # it backed off once
    assert j.last_prompt_tokens == 12 and j.last_completion_tokens == 3


def test_llmjudge_nonretryable_failure_returns_none(monkeypatch: pytest.MonkeyPatch) -> None:
    import urllib.error

    monkeypatch.setenv("R2_RUN", "1")
    monkeypatch.setenv("R2_JUDGE_BASE_URL", "http://localhost:9/v1")
    monkeypatch.setenv("R2_JUDGE_MODEL", "gemini-2.5-flash-lite")

    def _boom(self, req):
        raise urllib.error.HTTPError("u", 400, "bad", {}, None)  # type: ignore[arg-type]

    j = LLMJudge()
    monkeypatch.setattr(LLMJudge, "_open", _boom)
    assert j.judge_pair("q?", "a", "b", ("comprehensiveness",)) is None
    assert j.n_errors == 1  # ABSENT, never fabricated


# --------------------------------------------------------------------------- #
# run_pilot — happy path
# --------------------------------------------------------------------------- #
def test_run_pilot_happy_path_report_shape() -> None:
    report = _run(FakeJudge(), limit=3, target_questions=50)
    assert report["status"] == "OK"
    assert report["pair"] == list(DEFAULT_PAIR)
    assert report["n_questions_sampled"] == 3
    # The VectorRAG treatment wins every headline metric (it answered with WIN_TOKEN).
    for m in ("comprehensiveness", "diversity", "empowerment"):
        assert report["per_metric"][m]["win_rate"] > 0.5
    bc = report["bias_controls"]
    assert bc["order_swapped"] is True and bc["n_runs"] == 5
    assert bc["judge_family"] == "judgefam"
    assert report["length_corroboration"]["ran"] is True
    prem = report["premise_strong_baseline_clears"]
    assert prem["all_clear"] is True
    assert report["cost_projection"]["projected_full_usd"] >= 0.0
    assert report["measured"]["n_calls"] == 3 * 5 * 2  # questions × runs × orders


# --------------------------------------------------------------------------- #
# Cross-family self-preference guard
# --------------------------------------------------------------------------- #
def test_run_pilot_raises_on_self_preferring_judge() -> None:
    biased = FakeJudge(family="answererfam")  # SAME family as the arms' answerer
    with pytest.raises(ValueError, match="self-preference"):
        _run(biased, limit=2)


# --------------------------------------------------------------------------- #
# Resilience: checkpoint + idempotent resume (zero re-spend)
# --------------------------------------------------------------------------- #
def test_checkpoint_and_resume_rejudges_nothing(tmp_path: Path) -> None:
    ckpt = tmp_path / "pilot.checkpoint.json"
    j1 = FakeJudge()
    r1 = _run(j1, limit=2, checkpoint_path=ckpt)
    assert ckpt.exists()
    n_keys = 2 * 5 * 2
    assert j1.calls == n_keys
    blob = json.loads(ckpt.read_text())
    assert len(blob["judgments"]) == n_keys  # one entry per judged key

    # Resume: the same keys are re-derived and skipped — the judge is NEVER called.
    j2 = FakeJudge()
    r2 = _run(j2, limit=2, checkpoint_path=ckpt)
    assert j2.calls == 0
    assert r2["per_metric"] == r1["per_metric"]
    # The resumed run carries the prior measured tokens forward (valid projection).
    assert r2["measured"]["n_calls"] == n_keys


# --------------------------------------------------------------------------- #
# Budget guard
# --------------------------------------------------------------------------- #
def test_max_usd_guard_raises_before_spend() -> None:
    j = FakeJudge()
    with pytest.raises(BudgetExceeded):
        _run(j, limit=2, max_usd=1e-9)
    # The guard must fire BEFORE the judge is ever invoked — not after a spent call.
    assert j.calls == 0


def test_max_usd_bounds_priced_answerer_leg_before_judge() -> None:
    # With a PRICED answerer, --max-usd must bound the answerer leg too: the guard
    # fires during answering, BEFORE any judge call (total-spend guard, [P2] #2).
    j = FakeJudge()
    with pytest.raises(BudgetExceeded):
        _run(j, limit=2, max_usd=1e-9, answerer_model="gemini-2.5-flash-lite")
    assert j.calls == 0


# --------------------------------------------------------------------------- #
# Empty / all-ABSENT → ABSENT status (never a fabricated win-rate)
# --------------------------------------------------------------------------- #
def test_all_absent_judge_yields_absent_status() -> None:
    report = _run(NoneJudge(), limit=2)
    assert report["status"] == "ABSENT"
    assert report["per_metric"] == {}
    assert report["premise_strong_baseline_clears"] is None


# --------------------------------------------------------------------------- #
# Cost projection scaling
# --------------------------------------------------------------------------- #
def test_cost_projection_scales_with_target_questions() -> None:
    small = _run(FakeJudge(), limit=2, target_questions=10)
    big = _run(FakeJudge(), limit=2, target_questions=20)
    cs, cb = small["cost_projection"], big["cost_projection"]
    assert cb["projected_full_calls"] == 2 * cs["projected_full_calls"]
    assert cb["projected_full_usd"] == pytest.approx(2 * cs["projected_full_usd"], rel=1e-6)
    assert cs["cost_per_call_usd"] > 0.0  # measured tokens → a real per-call cost


# --------------------------------------------------------------------------- #
# [P2] #1 — cheap-validate projection must size the POWERED run (>= MIN_RUNS),
# NOT the 1-run cheap execution (else it under-projects the spend ~5×).
# --------------------------------------------------------------------------- #
def test_cheap_validate_projection_sizes_powered_run() -> None:
    report = _run(FakeJudge(), cheap_validate=True, target_questions=10)
    assert report["n_runs"] == CHEAP_N_RUNS  # the EXECUTION ran a single run
    assert report["projection_n_runs"] == MIN_RUNS
    proj = report["cost_projection"]
    # full_calls = target_questions × n_pairs(1) × MIN_RUNS × n_orders(2) — the POWERED
    # fan-out, NOT the buggy cheap n_runs=1 (which would give 10 × 1 × 1 × 2 = 20).
    assert proj["projected_full_calls"] == 10 * 1 * MIN_RUNS * 2
    assert proj["projected_full_calls"] != 10 * 1 * CHEAP_N_RUNS * 2


# --------------------------------------------------------------------------- #
# [P2] #2 — the projected/total cost must include the shared-answerer leg, not
# the judge leg alone (so the HITL spend number is TOTAL spend).
# --------------------------------------------------------------------------- #
def test_projection_includes_answerer_leg() -> None:
    # A priced answerer model routes the answerer leg through the cost ledger/projection.
    # Large target so both legs round to a non-zero USD (projected_full_usd is 2-dp).
    report = _run(
        FakeJudge(), limit=2, target_questions=100_000, answerer_model="gemini-2.5-flash-lite"
    )
    am = report["answerer_measured"]
    assert am["n_calls"] == 2 * 2  # one shared-answerer call per arm per question
    ct = report["cost_total"]
    assert ct["judge_usd"] > 0.0
    assert ct["answerer_usd"] > 0.0  # the answerer leg is a NON-ZERO contribution
    assert ct["projected_full_usd"] == pytest.approx(ct["judge_usd"] + ct["answerer_usd"], rel=1e-9)
    # The TOTAL strictly exceeds the judge-only projection (answerer leg is additive).
    assert ct["projected_full_usd"] > report["cost_projection"]["projected_full_usd"]
    assert ct["max_usd_bounds"] == "answerer+judge"


def test_local_answerer_leg_is_free_and_labeled_judge_only() -> None:
    # The default FakeAnswerer (model_id 'fake-answerer-v1') is unpinned → local/$0.
    report = _run(FakeJudge(), limit=2, target_questions=10)
    ct = report["cost_total"]
    assert ct["answerer_priced"] is False
    assert ct["answerer_usd"] == 0.0
    assert ct["projected_full_usd"] == pytest.approx(ct["judge_usd"], rel=1e-9)
    assert "judge-only" in ct["max_usd_bounds"]


# --------------------------------------------------------------------------- #
# Cheap-validate tiny-N mode
# --------------------------------------------------------------------------- #
def test_cheap_validate_forces_tiny_n() -> None:
    report = _run(FakeJudge(), limit=10, n_runs=9, cheap_validate=True)
    assert report["mode"] == "cheap_validate"
    assert report["limit"] == CHEAP_LIMIT
    assert report["n_runs"] == CHEAP_N_RUNS
    assert report["n_questions_sampled"] == CHEAP_LIMIT
    assert "cost_projection" in report


# --------------------------------------------------------------------------- #
# main() gated by R2_RUN
# --------------------------------------------------------------------------- #
def test_main_gated_without_r2_run(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.delenv("R2_RUN", raising=False)
    out = tmp_path / "out.json"
    rc = main(["--out", str(out)])
    assert rc == 2  # refused, no network, no corpus touched
    assert not out.exists()
