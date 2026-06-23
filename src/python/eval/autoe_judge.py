"""AutoE pairwise-LLM-judge harness (0.8.4 Slice 5b) — BenchmarkQED-style.

This module is the **measurement front-end** for the frozen 0.8.4 decision rule
(:mod:`eval.decision_rule_084`). It turns per-arm *answers* into the
``primary_per_metric`` win-rate mapping that :func:`eval.decision_rule_084.decide_084`
consumes, with the four LLM-judge bias controls (design §5) wired as first-class
structure — **not** as an afterthought a downstream slice could quietly drop.

The 0.8.4 axis is **global sensemaking**, scored by a reference-free BenchmarkQED /
GraphRAG ("From Local to Global", arXiv:2404.16130) **pairwise** judge over three
headline metrics (comprehensiveness / diversity / empowerment), with **directness**
kept SEPARATE as the length-bias corroboration (a verbose answer can win the headline
metrics for the wrong reason; directness is the non-judge cross-check).

What this module owns:

1. :func:`build_pairwise_prompt` — the deterministic pairwise-judge prompt (A/B/tie
   per metric, headline + directness).
2. :class:`JudgmentKey` / :func:`run_autoe` — the **position-bias** control: every
   pair is judged in **both** orders (treatment-as-A *and* treatment-as-B) and
   averaged; an idempotent key makes resume safe.
3. :func:`parse_verdict` — the **ABSENT-safe** parser: an empty / ``None`` /
   unparseable completion → ``ABSENT`` (excluded from the denominator), never a silent
   loss or tie ([[priced-runs-need-resilience-before-spend]]).
4. :func:`compute_winrates` — aggregation into the ``decide_084`` shape, with a
   **bootstrap CI clustered by question** (the ≥5 runs × 2 orders are within-question
   replicates) and the seed as a **parameter** (no argless RNG).
5. :func:`assemble_bias_controls` / :func:`assemble_length_corroboration` (+
   :func:`length_contradicts`) — the structs handed straight to ``decide_084``.
6. :func:`build_autoe_batch_jsonl` / :func:`parse_autoe_batch_output` — the airlock
   **Batch** integration point (the judge calls are independent → batch-suitable),
   built in the :mod:`eval.p0a_batch_e2e` shape. **No live submit here.**
7. :func:`project_autoe_cost` — the $0 cost projection (reuses
   :func:`eval.d0b_parity_run.project_full_cost`) that feeds the HITL number.

$0: this module makes **no** network call. The judge is an injected duck-typed object
(``judge_pair(question, answer_a, answer_b, metrics) -> completion_str``); tests use a
deterministic ``FakeJudge``. Pure-Python — no ``fathomdb`` / ``numpy`` import — so it
runs anywhere, independent of the native-extension build.

Binding spec: ``dev/plans/prompts/0.8.4-slice-5b-autoe-prompt.md``;
``dev/design/0.8.4-graphrag-sensemaking.md`` §5 (frozen pre-registration).
"""

from __future__ import annotations

import json
import math
import random
import re
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from statistics import fmean
from typing import Any, Literal, Optional, Protocol

from eval.d0b_parity_run import project_full_cost

# Reuse the frozen rule's types + the headline metric set — DO NOT redefine them.
from eval.decision_rule_084 import (
    BiasControls,
    HEADLINE_METRICS,
    LengthCorroboration,
)

# Reuse the answerer + Hit seam (the per-arm answers the judge compares are produced
# by this answerer over each arm's retrieved context — do not reinvent it).
from eval.r2_parity_eval import BaseAnswerer, Hit, RetrievalAdapter

__all__ = [
    "DIRECTNESS_METRIC",
    "JUDGE_METRICS",
    "ORDER_CT",
    "ORDER_TC",
    "BiasControls",
    "Judge",
    "Judgment",
    "JudgmentKey",
    "LengthCorroboration",
    "Verdict",
    "assemble_bias_controls",
    "assemble_length_corroboration",
    "build_arm_answers",
    "build_autoe_batch_jsonl",
    "build_pairwise_prompt",
    "compute_winrates",
    "length_contradicts",
    "parse_autoe_batch_output",
    "parse_verdict",
    "project_autoe_cost",
    "run_autoe",
]

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

#: The length-bias corroboration metric — judged alongside the headline metrics in one
#: call (cheaper, same context) but NEVER folded into the headline win (design §5).
DIRECTNESS_METRIC: str = "directness"

#: The full per-call metric set: the three headline metrics + directness. ``decide_084``
#: gates on :data:`HEADLINE_METRICS`; directness only feeds the length corroboration.
JUDGE_METRICS: tuple[str, ...] = (*HEADLINE_METRICS, DIRECTNESS_METRIC)

#: Position-bias order tags. ``ORDER_TC`` = treatment shown as answer **A**, comparator
#: as **B**; ``ORDER_CT`` = the swap. Both are evaluated and averaged (the control).
ORDER_TC: str = "tc"
ORDER_CT: str = "ct"
_ORDERS: tuple[str, str] = (ORDER_TC, ORDER_CT)

#: Default paired-bootstrap resample count (deterministic given the seed parameter).
DEFAULT_N_BOOT: int = 2000

#: The directness contradiction margin: the headline winner must lose directness by at
#: least this much (on the win-rate scale) before we flag the win as a verbosity
#: artifact. Frozen default; overridable per-call for sensitivity checks.
DIRECTNESS_CONTRADICTS_MARGIN: float = 0.10

#: A judgment's per-metric outcome BEFORE order-normalization: A wins / B wins / tie /
#: ABSENT (unparseable — excluded, never a silent loss).
Verdict = Literal["A", "B", "tie", "ABSENT"]

_CUSTOM_ID_SEP = "||"
#: A flat ``{...}`` JSON object embedded in fenced / prose-wrapped judge output.
_JSON_OBJ_RE = re.compile(r"\{[^{}]*\}", re.DOTALL)


# --------------------------------------------------------------------------- #
# Judge seam (duck-typed; tests inject a FakeJudge — NO network here)
# --------------------------------------------------------------------------- #
class Judge(Protocol):
    """The injected judge. ``family`` feeds the self-preference bias control; it must
    differ from EVERY system-under-test family or ``decide_084`` BLOCKs."""

    family: str

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> str: ...


# --------------------------------------------------------------------------- #
# 2 — idempotent JudgmentKey
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class JudgmentKey:
    """Identifies one judgment so resume is idempotent: ``(question_id, pair, run_idx,
    order)``. ``pair`` is ``(treatment_arm, comparator_arm)``; ``order`` is
    :data:`ORDER_TC` / :data:`ORDER_CT`."""

    question_id: str
    pair: tuple[str, str]
    run_idx: int
    order: str

    def to_custom_id(self) -> str:
        """Encode as a batch ``custom_id`` (resume key). Field values must not contain
        :data:`_CUSTOM_ID_SEP`."""
        t, c = self.pair
        return _CUSTOM_ID_SEP.join([self.question_id, t, c, str(self.run_idx), self.order])

    @classmethod
    def from_custom_id(cls, cid: str) -> "JudgmentKey":
        qid, t, c, run_idx, order = cid.split(_CUSTOM_ID_SEP)
        return cls(question_id=qid, pair=(t, c), run_idx=int(run_idx), order=order)


@dataclass(frozen=True)
class Judgment:
    """One parsed pairwise judgment: its :class:`JudgmentKey` + the per-metric raw
    verdict (BEFORE order-normalization — :func:`compute_winrates` normalizes)."""

    key: JudgmentKey
    verdicts: Mapping[str, str]


# --------------------------------------------------------------------------- #
# 1 — pairwise-judge prompt builder
# --------------------------------------------------------------------------- #
_PROMPT_HEADER = (
    "You are an impartial judge comparing two answers (A and B) to the same question, "
    "for a global-sensemaking benchmark. For EACH metric below, decide whether answer "
    "A is better, answer B is better, or they are equally good (a tie). Judge each "
    "metric independently.\n\n"
    "Metrics:\n"
    "- comprehensiveness: how much of the question's scope the answer covers.\n"
    "- diversity: how varied and rich the perspectives/details are.\n"
    "- empowerment: how well it helps the reader reason and make an informed judgement.\n"
    "- directness: how concisely and to-the-point it answers (do NOT reward verbosity)."
)
_PROMPT_FOOTER = (
    'Respond with ONLY a JSON object mapping each metric to "A", "B", or "tie", e.g.\n'
    '{{"comprehensiveness": "A", "diversity": "tie", "empowerment": "B", '
    '"directness": "A"}}'
)


def build_pairwise_prompt(
    question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
) -> str:
    """Build the BenchmarkQED-style pairwise prompt. Deterministic given inputs.

    Asks the judge to pick A / B / tie **per metric**. ``metrics`` is echoed into the
    requested-keys line so the parser and prompt stay in lock-step; the four metric
    definitions (headline + directness) are always shown."""
    keys = ", ".join(f'"{m}"' for m in metrics)
    return (
        f"{_PROMPT_HEADER}\n\n"
        f"Question:\n{question}\n\n"
        f"Answer A:\n{answer_a}\n\n"
        f"Answer B:\n{answer_b}\n\n"
        f"Score these metrics: {keys}.\n"
        f"{_PROMPT_FOOTER}"
    )


# --------------------------------------------------------------------------- #
# 3 — ABSENT-safe verdict parser
# --------------------------------------------------------------------------- #
def _coerce_one(value: object) -> Verdict:
    """Map a raw per-metric value to A / B / tie, else ABSENT (never a silent loss)."""
    if not isinstance(value, str):
        return "ABSENT"
    v = value.strip().lower()
    if v == "a":
        return "A"
    if v == "b":
        return "B"
    if v == "tie":
        return "tie"
    return "ABSENT"


def parse_verdict(completion: Optional[str], metrics: tuple[str, ...]) -> dict[str, Verdict]:
    """Parse a judge completion into ``{metric: A|B|tie|ABSENT}``.

    An empty / ``None`` / unparseable completion → **every** metric ``ABSENT`` (never a
    silent loss or tie); a missing metric or an unparseable value → ``ABSENT`` for that
    metric alone. Tolerant of ```` ```json ```` fences and surrounding prose."""
    obj: Optional[dict[str, object]] = None
    if completion and completion.strip():
        text = completion.strip()
        candidates = [text]
        m = _JSON_OBJ_RE.search(text)
        if m:
            candidates.append(m.group(0))
        for cand in candidates:
            try:
                parsed = json.loads(cand)
            except (json.JSONDecodeError, TypeError):
                continue
            if isinstance(parsed, dict):
                obj = parsed
                break
    if obj is None:
        return {m: "ABSENT" for m in metrics}
    return {m: _coerce_one(obj.get(m)) for m in metrics}


# --------------------------------------------------------------------------- #
# Run (position-bias control): judge each pair in BOTH orders, idempotently
# --------------------------------------------------------------------------- #
def run_autoe(
    judge: Judge,
    answers_by_arm: Mapping[str, Mapping[str, str]],
    questions: Sequence[tuple[str, str]],
    pair: tuple[str, str],
    *,
    n_runs: int,
    metrics: tuple[str, ...] = JUDGE_METRICS,
    existing: Optional[Mapping[JudgmentKey, Judgment]] = None,
) -> dict[JudgmentKey, Judgment]:
    """Judge every ``(question, run, order)`` for ``pair`` and return
    ``{JudgmentKey: Judgment}``.

    ``answers_by_arm`` is ``{arm: {qid: answer}}``; ``questions`` is ``[(qid, text)]``;
    ``pair`` is ``(treatment_arm, comparator_arm)``. Both orders are always emitted
    (the position-bias control). ``existing`` lets resume skip already-judged keys —
    re-running with the prior dict re-derives the SAME keys (idempotent), so a kill
    mid-run loses nothing."""
    treatment, comparator = pair
    t_answers = answers_by_arm[treatment]
    c_answers = answers_by_arm[comparator]
    out: dict[JudgmentKey, Judgment] = dict(existing) if existing else {}
    for qid, text in questions:
        t_ans, c_ans = t_answers[qid], c_answers[qid]
        for run_idx in range(n_runs):
            for order in _ORDERS:
                key = JudgmentKey(question_id=qid, pair=pair, run_idx=run_idx, order=order)
                if key in out:
                    continue  # idempotent resume
                if order == ORDER_TC:
                    answer_a, answer_b = t_ans, c_ans
                else:
                    answer_a, answer_b = c_ans, t_ans
                completion = judge.judge_pair(text, answer_a, answer_b, metrics)
                out[key] = Judgment(key=key, verdicts=parse_verdict(completion, metrics))
    return out


# --------------------------------------------------------------------------- #
# 4 — win-rate aggregation → decide_084 input (clustered bootstrap; seed param)
# --------------------------------------------------------------------------- #
def _treatment_score(raw: str, order: str) -> Optional[float]:
    """Normalize a raw A/B/tie verdict to the TREATMENT arm's outcome (1 win / 0.5 tie /
    0 loss), accounting for the order swap; ABSENT → ``None`` (excluded)."""
    if raw == "ABSENT":
        return None
    if raw == "tie":
        return 0.5
    treatment_is_a = order == ORDER_TC
    if raw == "A":
        return 1.0 if treatment_is_a else 0.0
    if raw == "B":
        return 0.0 if treatment_is_a else 1.0
    return None


def _percentile(sorted_xs: list[float], pct: float) -> float:
    """Linear-interpolated percentile of an already-sorted, non-empty list."""
    if len(sorted_xs) == 1:
        return sorted_xs[0]
    k = (len(sorted_xs) - 1) * pct / 100.0
    lo = math.floor(k)
    hi = math.ceil(k)
    if lo == hi:
        return sorted_xs[int(k)]
    return sorted_xs[lo] * (hi - k) + sorted_xs[hi] * (k - lo)


def _metric_winrate(
    judgments: Sequence[Judgment], metric: str, *, n_boot: int, seed: int
) -> dict[str, Any]:
    """One metric's ``{win_rate, ci_lo, ci_hi, mde, n}`` with a bootstrap CI **clustered
    by question** (resample questions, not individual judgments).

    Raises :class:`ValueError` if NO judgment is decided for the metric (all ABSENT) —
    a fabricated 0.5 would hide a dead measurement (fail loudly, not silently)."""
    by_q: dict[str, list[float]] = {}
    for j in judgments:
        score = _treatment_score(j.verdicts.get(metric, "ABSENT"), j.key.order)
        if score is None:
            continue
        by_q.setdefault(j.key.question_id, []).append(score)

    all_scores = [s for scores in by_q.values() for s in scores]
    if not all_scores:
        raise ValueError(
            f"metric {metric!r}: no decided judgments (all ABSENT) — refusing to "
            "fabricate a win-rate; record a blocker and re-judge"
        )

    win_rate = fmean(all_scores)
    qids = list(by_q)
    rng = random.Random(f"{seed}:{metric}")  # seed is a PARAMETER; deterministic
    n_q = len(qids)
    boot_means: list[float] = []
    for _ in range(n_boot):
        sample: list[float] = []
        for _ in range(n_q):
            sample.extend(by_q[qids[rng.randrange(n_q)]])
        boot_means.append(fmean(sample))
    boot_means.sort()
    ci_lo = _percentile(boot_means, 2.5)
    ci_hi = _percentile(boot_means, 97.5)
    return {
        "win_rate": round(win_rate, 6),
        "ci_lo": round(ci_lo, 6),
        "ci_hi": round(ci_hi, 6),
        "mde": round((ci_hi - ci_lo) / 2.0, 6),  # CI half-width
        "n": len(all_scores),
    }


def compute_winrates(
    judgments: Iterable[Judgment],
    pair: tuple[str, str],
    *,
    metrics: tuple[str, ...] = HEADLINE_METRICS,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
) -> dict[str, dict[str, Any]]:
    """Aggregate ``judgments`` for ``pair`` into the ``decide_084`` ``primary_per_metric``
    mapping: ``{metric: {win_rate, ci_lo, ci_hi, mde, n}}``.

    Win-rate counts ties as 0.5 and is computed over all questions×runs×orders (ABSENT
    excluded). The CI is a bootstrap **clustered by question**; ``mde`` is the CI
    half-width. ``seed`` is a parameter — same seed → identical CI."""
    pair_t = tuple(pair)
    selected = [j for j in judgments if tuple(j.key.pair) == pair_t]
    return {m: _metric_winrate(selected, m, n_boot=n_boot, seed=seed) for m in metrics}


# --------------------------------------------------------------------------- #
# 5 — bias-control + length-corroboration assembly
# --------------------------------------------------------------------------- #
def assemble_bias_controls(
    *,
    n_runs: int,
    judge_family: str,
    system_families: Sequence[str],
    order_swapped: bool = True,
) -> BiasControls:
    """Assemble the :class:`BiasControls` struct for ``decide_084``.

    ``order_swapped`` defaults True because :func:`run_autoe` ALWAYS judges both orders
    — the position-bias control is structural, not optional. The caller supplies the
    ≥5 ``n_runs`` (stochasticity), the ``judge_family`` (self-preference), and every
    system-under-test family."""
    return BiasControls(
        order_swapped=order_swapped,
        n_runs=n_runs,
        judge_family=judge_family,
        system_families=list(system_families),
    )


def length_contradicts(
    headline_winrates: Mapping[str, float],
    directness_winrate: float,
    *,
    margin: float = DIRECTNESS_CONTRADICTS_MARGIN,
) -> bool:
    """The directness length-bias rule: does directness flip AGAINST the headline winner
    by at least ``margin``?

    Let ``H`` = the mean treatment headline win-rate. The headline winner is the
    treatment if ``H >= 0.5`` else the comparator. A *contradiction* is when that same
    arm's **directness** win-rate falls below ``0.5 - margin`` (it wins comprehensiveness
    etc. but is judged materially LESS direct → the win may be a verbosity artifact, the
    GraphRAG paper's non-judge cross-check)."""
    mean_headline = fmean(headline_winrates.values())
    if mean_headline >= 0.5:
        winner_directness = directness_winrate  # treatment won the headlines
    else:
        winner_directness = 1.0 - directness_winrate  # comparator won the headlines
    return winner_directness < (0.5 - margin)


def assemble_length_corroboration(
    judgments: Iterable[Judgment],
    pair: tuple[str, str],
    *,
    ran: bool,
    margin: float = DIRECTNESS_CONTRADICTS_MARGIN,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
) -> LengthCorroboration:
    """Assemble the :class:`LengthCorroboration` struct from the judged directness +
    headline win-rates.

    ``ran`` records that the corroboration was actually computed (a missing-control
    BLOCK in ``decide_084`` if False). ``contradicts`` applies :func:`length_contradicts`
    to the directness win-rate vs the headline win-rates (both derived from the SAME
    judgment set, so directness and the headline win move on a common footing)."""
    selected = list(judgments)
    headline = {
        m: _metric_winrate(
            [j for j in selected if tuple(j.key.pair) == tuple(pair)],
            m,
            n_boot=n_boot,
            seed=seed,
        )["win_rate"]
        for m in HEADLINE_METRICS
    }
    directness = _metric_winrate(
        [j for j in selected if tuple(j.key.pair) == tuple(pair)],
        DIRECTNESS_METRIC,
        n_boot=n_boot,
        seed=seed,
    )["win_rate"]
    return LengthCorroboration(
        ran=ran,
        contradicts=length_contradicts(headline, directness, margin=margin),
    )


# --------------------------------------------------------------------------- #
# Answer production (reuse the r2_parity_eval answerer + Hit seam)
# --------------------------------------------------------------------------- #
def build_arm_answers(
    answerer: BaseAnswerer,
    adapters: Mapping[str, RetrievalAdapter],
    questions: Sequence[tuple[str, str]],
    *,
    k: int = 10,
) -> dict[str, dict[str, str]]:
    """Produce ``{arm: {qid: answer}}`` via the SHARED :class:`BaseAnswerer` over each
    arm's retrieved context — the identical-answerer invariant (ADR §3.2), reused, not
    reinvented. An abstaining / unavailable answer is stored as ``""`` (the judge then
    sees an empty candidate, never a fabricated answer). :class:`Hit` is the retrieval
    contract."""
    out: dict[str, dict[str, str]] = {name: {} for name in adapters}
    for name, adapter in adapters.items():
        for qid, text in questions:
            hits: list[Hit] = adapter.retrieve(text, k)
            answer = answerer.answer(text, [h.body for h in hits if h.body])
            out[name][qid] = answer or ""
    return out


# --------------------------------------------------------------------------- #
# 6 — batch-build integration point (p0a_batch_e2e shape; NO live submit)
# --------------------------------------------------------------------------- #
def build_autoe_batch_jsonl(
    answers_by_arm: Mapping[str, Mapping[str, str]],
    questions: Sequence[tuple[str, str]],
    pair: tuple[str, str],
    *,
    judge_model: str,
    n_runs: int,
    metrics: tuple[str, ...] = JUDGE_METRICS,
    max_tokens: int = 64,
) -> tuple[str, dict[str, dict[str, Any]]]:
    """Build the AutoE judge **batch** input JSONL + a ``custom_id -> meta`` sidecar,
    in the :func:`eval.p0a_batch_e2e.build_judge_jsonl` shape (one
    ``/v1/chat/completions`` request per ``(question, run, order)``, keyed by the
    resumable :meth:`JudgmentKey.to_custom_id`).

    The AutoE judge calls are independent → batch-suitable. This only BUILDS the JSONL;
    submitting it is a later, HITL-gated priced step. No network here."""
    treatment, comparator = pair
    t_answers = answers_by_arm[treatment]
    c_answers = answers_by_arm[comparator]
    lines: list[str] = []
    sidecar: dict[str, dict[str, Any]] = {}
    for qid, text in questions:
        t_ans, c_ans = t_answers[qid], c_answers[qid]
        for run_idx in range(n_runs):
            for order in _ORDERS:
                key = JudgmentKey(question_id=qid, pair=pair, run_idx=run_idx, order=order)
                if order == ORDER_TC:
                    answer_a, answer_b = t_ans, c_ans
                else:
                    answer_a, answer_b = c_ans, t_ans
                prompt = build_pairwise_prompt(text, answer_a, answer_b, metrics)
                cid = key.to_custom_id()
                sidecar[cid] = {
                    "question_id": qid,
                    "pair": list(pair),
                    "run_idx": run_idx,
                    "order": order,
                    "metrics": list(metrics),
                }
                lines.append(
                    json.dumps(
                        {
                            "custom_id": cid,
                            "method": "POST",
                            "url": "/v1/chat/completions",
                            "body": {
                                "model": judge_model,
                                "messages": [{"role": "user", "content": prompt}],
                                "temperature": 0,
                                "seed": 0,
                                "max_completion_tokens": max_tokens,
                            },
                        }
                    )
                )
    return ("\n".join(lines) + "\n" if lines else ""), sidecar


def parse_autoe_batch_output(
    text: str, sidecar: Mapping[str, Mapping[str, Any]]
) -> dict[JudgmentKey, Judgment]:
    """Parse an OpenAI-batch output JSONL of judge completions back into
    ``{JudgmentKey: Judgment}`` via the ``custom_id`` (the counterpart of
    :func:`build_autoe_batch_jsonl`).

    A missing / malformed line yields an ABSENT-everywhere :class:`Judgment` for its cid
    (never a silent loss) so resume can re-judge exactly the dead cells. ``sidecar``
    supplies each cid's metric set."""
    out: dict[JudgmentKey, Judgment] = {}
    for ln in text.splitlines():
        if not ln.strip():
            continue
        try:
            rec = json.loads(ln)
        except json.JSONDecodeError:
            continue  # a corrupt line: the cid is unknown → leave it for resume
        cid = rec.get("custom_id")
        if cid is None or cid not in sidecar:
            continue
        metrics = tuple(str(m) for m in sidecar[cid]["metrics"])
        body = (rec.get("response") or {}).get("body") or {}
        try:
            content: Optional[str] = body["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError):
            content = None
        key = JudgmentKey.from_custom_id(str(cid))
        out[key] = Judgment(key=key, verdicts=parse_verdict(content, metrics))
    return out


# --------------------------------------------------------------------------- #
# 7 — cost projection (reuse project_full_cost)
# --------------------------------------------------------------------------- #
def project_autoe_cost(
    *,
    prompt_tokens: int,
    completion_tokens: int,
    n_calls: int,
    price_in_per_1m: float,
    price_out_per_1m: float,
    n_questions: int,
    n_pairs: int,
    n_runs: int,
    n_orders: int = 2,
    context_token_budget: Optional[int] = None,
    overhead_tokens: int = 60,
) -> dict[str, Any]:
    """Project the full priced AutoE run cost from a measured pilot, reusing
    :func:`eval.d0b_parity_run.project_full_cost`.

    One judge call scores ALL metrics for one ``(question, pair, run, order)``, so the
    full call count is ``n_questions × n_pairs × n_runs × n_orders``. We fold the
    per-question fan-out (``n_pairs × n_runs × n_orders``) into ``project_full_cost``'s
    ``n_priced_arms`` and pass ``n_questions`` as ``n_questions_full`` — the product is
    the full matrix. ``prompt_tokens`` / ``completion_tokens`` are the measured totals
    over ``n_calls`` pilot calls. Pure arithmetic; no LLM call."""
    fan_out = n_pairs * n_runs * n_orders
    return project_full_cost(
        prompt_tokens=prompt_tokens,
        completion_tokens=completion_tokens,
        n_calls=n_calls,
        price_in_per_1m=price_in_per_1m,
        price_out_per_1m=price_out_per_1m,
        n_questions_full=n_questions,
        n_priced_arms=fan_out,
        context_token_budget=context_token_budget,
        overhead_tokens=overhead_tokens,
    )
