"""0.8.4 Slice 5b-runner — the resilient AutoE pilot runner + a real ``LLMJudge``.

This module ties the existing Slice-5 pieces (the AutoE harness
:mod:`eval.autoe_judge`, the frozen rule :mod:`eval.decision_rule_084`, the
AP-News corpus :mod:`eval.apnews_corpus`, the standalone baselines
:mod:`eval.baselines_084`, and the shared answerer
:mod:`eval.r2_parity_eval`) into a **single command** so a cheap-validate cost
probe — and later the priced pilot — is reproducible and resilient.

It is the [[priced-runs-need-resilience-before-spend]] precondition made concrete:
*before any spend* (even cheap-validate) the runner must already have atomic
per-judgment checkpointing, idempotent ``--resume``, 429/5xx backoff that honors
``Retry-After``, and a :class:`~eval.gap_decomposition_run.BudgetLedger` pre-call
``--max-usd`` guard. None of that lives in the in-memory :func:`eval.autoe_judge.run_autoe`
sibling, so the resilient judging loop (:func:`_judge_resiliently`) is the thin
**resilience wrapper** around the harness's per-key primitives
(:class:`~eval.autoe_judge.JudgmentKey` / :func:`~eval.autoe_judge.parse_verdict`
/ :class:`~eval.autoe_judge.Judgment`) — it reuses the harness's parsing,
win-rate aggregation and decision rule unchanged; it does **not** reinvent them.

What this module owns:

1. :class:`LLMJudge` — a real :class:`~eval.autoe_judge.Judge` POSTing
   ``/chat/completions`` to a **judge** endpoint from a SEPARATE judge env
   (``R2_JUDGE_BASE_URL`` / ``R2_JUDGE_MODEL`` / ``R2_JUDGE_API_KEY``), mirroring
   :class:`eval.r2_parity_eval.LLMAnswerer`'s urllib pattern with the 429/5xx
   backoff of :func:`eval.m1_baseline_run._is_retryable`. Empty / None / HTTP-fail
   → ``None`` (the harness maps ``None`` → ABSENT, re-judged on resume).
2. :func:`run_pilot` — the orchestration (sample AutoQ across buckets → baseline
   adapters → shared-answerer answers → resilient judging → win-rates → bias
   controls → the kill-early premise read → a cost projection report dict), with
   the **cross-family self-preference guard** as a fail-loud precondition.
3. :func:`_judge_resiliently` — the resilient loop: atomic checkpoint per judged
   key, idempotent resume, the ``BudgetLedger`` pre-call guard, ``None`` → ABSENT.
4. :func:`main` — the CLI, gated by ``R2_RUN=1`` for any real call. Imports and
   unit-tests cleanly (with fakes) when ``R2_RUN`` is unset.

$0: this module makes **no** network call on import or in any unit test. The judge
is duck-typed; tests inject a deterministic fake. Pure-Python — no ``fathomdb`` /
``numpy`` import — so it runs independent of the native-extension build.

Binding spec: ``dev/plans/prompts/0.8.4-slice-5b-pilot-runner-prompt.md``.
"""

from __future__ import annotations

import json
import math
import os
import re
import time
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Optional, Protocol

from eval.apnews_corpus import AutoQQuestion
from eval.autoe_judge import (
    DEFAULT_N_BOOT,
    JUDGE_METRICS,
    ORDER_CT,
    ORDER_TC,
    Judgment,
    JudgmentKey,
    assemble_bias_controls,
    assemble_length_corroboration,
    build_arm_answers,
    build_pairwise_prompt,
    compute_winrates,
    parse_verdict,
    project_autoe_cost,
)
from eval.baselines_084 import LongContextAdapter, VectorRagAdapter
from eval.decision_rule_084 import (
    HEADLINE_METRICS,
    MIN_RUNS,
    strong_baseline_clears,
)
from eval.gap_decomposition_run import (
    BudgetExceeded,
    BudgetLedger,
    UnpinnedPricing,
    price_for,
)
from eval.m1_baseline_run import _is_retryable, _retry_after_seconds
from eval.m1_verdict_run import _atomic_write_json
from eval.r2_parity_eval import BaseAnswerer, LLMAnswerer, NullAnswerer, RetrievalAdapter

__all__ = [
    "CHEAP_LIMIT",
    "CHEAP_N_RUNS",
    "DEFAULT_JUDGE_MODEL",
    "DEFAULT_PAIR",
    "BudgetExceeded",
    "LLMJudge",
    "PilotJudge",
    "family_of",
    "main",
    "run_pilot",
]

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

#: The default treatment-vs-comparator pair: the standalone VectorRAG baseline
#: against the long-context "stuff-it-all-in" honest upper-bar control. (S1 /
#: GraphRAG is out of scope for this $0 cost probe; the projection is
#: embedder-agnostic — design §6 kill-early premise.)
DEFAULT_PAIR: tuple[str, str] = ("vector_rag", "long_context")

#: The cheap-validate tiny-N path: 2 questions × 1 run, intended for
#: ``gemini-2.5-flash-lite`` — proves the pipeline end-to-end and measures per-call
#: cost so the projection can be written. NOT a valid resolution (n_runs < MIN_RUNS
#: blocks ``decide_084`` by design; the cheap pass only sizes the spend).
CHEAP_LIMIT: int = 2
CHEAP_N_RUNS: int = 1

#: A pinned cheap judge id used when no judge model is otherwise resolvable, so the
#: cost projection's :func:`price_for` lookup never fails closed on a default.
DEFAULT_JUDGE_MODEL: str = "gemini-2.5-flash-lite"

#: Conservative per-call output-token estimate for the budget pre-call projection
#: and the cost fallback when a fake judge reports no usage (the judge emits a tiny
#: JSON verdict object).
_ESTIMATE_COMPLETION_TOKENS: int = 16

#: Absolute ceiling on a server-stated ``Retry-After`` cooldown (so a pathological
#: value cannot hang the run) — mirrors ``m1_baseline_run._RETRY_AFTER_HARD_CAP``.
_RETRY_AFTER_HARD_CAP: float = 600.0

_R2_RUN_ENV = "R2_RUN"
_FAMILY_RE = re.compile(r"[a-z0-9]+")


# --------------------------------------------------------------------------- #
# Judge seam (duck-typed; the real LLMJudge + tests' fakes both satisfy it)
# --------------------------------------------------------------------------- #
class PilotJudge(Protocol):
    """The pilot's judge seam. Unlike :class:`eval.autoe_judge.Judge` (whose
    ``judge_pair`` returns ``str``), the pilot judge may return ``None`` on an
    empty / failed call — the resilient loop maps ``None`` → ABSENT (re-judged on
    resume), never a fabricated score. ``family`` feeds the self-preference guard."""

    family: str

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> Optional[str]: ...


def family_of(model_id: str) -> str:
    """Derive a coarse model **family** token from a model id (first alnum run,
    lower-cased): ``gpt-5.4`` → ``gpt``; ``gemini-2.5-flash-lite`` → ``gemini``.

    Used to populate the self-preference families map when a caller does not supply
    one explicitly — the judge family must differ from every answerer family."""
    m = _FAMILY_RE.search(model_id.lower())
    return m.group(0) if m else model_id.strip().lower()


# --------------------------------------------------------------------------- #
# 1 — the real LLMJudge (mirrors LLMAnswerer; SEPARATE judge env; backoff)
# --------------------------------------------------------------------------- #
class LLMJudge:
    """A real pairwise :class:`~eval.autoe_judge.Judge` over an OpenAI-compatible
    ``/chat/completions`` endpoint, configured from a **SEPARATE** judge env so the
    judge can be a different model family than the answerer (the self-preference
    control, design §5): ``R2_JUDGE_BASE_URL`` / ``R2_JUDGE_MODEL`` /
    ``R2_JUDGE_API_KEY``. Gated by ``R2_RUN=1``.

    Mirrors :class:`eval.r2_parity_eval.LLMAnswerer`'s urllib pattern (temp 0, fixed
    seed; stdlib only, no SDK) plus the 429/5xx backoff of
    :class:`eval.m1_baseline_run.CostTrackingAnswerer` (honoring ``Retry-After``).

    A retry-exhausted / non-retryable HTTP failure, or an empty completion, returns
    ``None`` — the harness maps ``None`` → ABSENT (excluded from the denominator,
    re-judged on resume), never a silent loss. Per-call token usage is accumulated
    (and exposed as ``last_prompt_tokens`` / ``last_completion_tokens``) so the
    runner can build the cost projection from measured tokens."""

    def __init__(
        self,
        *,
        family: Optional[str] = None,
        max_completion_tokens: int = 64,
        timeout_s: float = 120.0,
        max_retries: int = 4,
        backoff_base: float = 1.0,
        max_backoff: float = 30.0,
        sleep: Any = time.sleep,
    ) -> None:
        self.base_url = os.environ.get("R2_JUDGE_BASE_URL", "")
        self.api_key = os.environ.get("R2_JUDGE_API_KEY", "")
        self.model_id = os.environ.get("R2_JUDGE_MODEL", "<unset>")
        self.family = family if family is not None else family_of(self.model_id)
        self._max_completion_tokens = int(max_completion_tokens)
        self._timeout = float(timeout_s)
        self._max_retries = int(max_retries)
        self._backoff_base = float(backoff_base)
        self._max_backoff = float(max_backoff)
        self._sleep = sleep
        # Usage accounting (for the cost projection).
        self.n_calls = 0
        self.n_errors = 0
        self.n_retries = 0
        self.prompt_tokens = 0
        self.completion_tokens = 0
        self.last_prompt_tokens = 0
        self.last_completion_tokens = 0

    @property
    def available(self) -> bool:
        """Whether a real judge call can be made in this environment."""
        return (
            os.environ.get(_R2_RUN_ENV) == "1"
            and bool(self.base_url)
            and self.model_id != "<unset>"
        )

    def _open(self, req: Any) -> Any:
        """Seam over the raw POST (injectable in tests). Returns the urlopen ctx mgr."""
        import urllib.request

        return urllib.request.urlopen(req, timeout=self._timeout)  # noqa: S310

    def judge_pair(
        self, question: str, answer_a: str, answer_b: str, metrics: tuple[str, ...]
    ) -> Optional[str]:
        """POST the pairwise-judge prompt and return the raw completion, or ``None``.

        ``R2_RUN`` must be ``1`` and the judge env configured (else a loud
        ``RuntimeError`` — a misconfigured judge is a setup error, not a silent
        ABSENT). A transient 429/5xx is retried with backoff (honoring
        ``Retry-After``); an exhausted / non-retryable failure or an empty body
        returns ``None`` (ABSENT)."""
        if os.environ.get(_R2_RUN_ENV) != "1":
            raise RuntimeError(f"{_R2_RUN_ENV} not set; set to 1 to run a real judge call")
        if not self.available:
            raise RuntimeError(
                "LLMJudge not configured: set R2_JUDGE_BASE_URL + R2_JUDGE_MODEL (+ R2_RUN=1)"
            )
        import urllib.request

        prompt = build_pairwise_prompt(question, answer_a, answer_b, metrics)
        payload = json.dumps(
            {
                "model": self.model_id,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0,
                "seed": 0,
                "max_completion_tokens": self._max_completion_tokens,
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            self.base_url.rstrip("/") + "/chat/completions",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        attempt = 0
        while True:
            try:
                with self._open(req) as resp:
                    body = json.loads(resp.read().decode("utf-8"))
                break
            except Exception as exc:  # noqa: BLE001 — classify; retry only the transient ones
                if attempt < self._max_retries and _is_retryable(exc):
                    cooldown = _retry_after_seconds(exc)
                    if cooldown is not None:
                        delay = min(cooldown + 5.0, _RETRY_AFTER_HARD_CAP)
                    else:
                        delay = min(self._backoff_base * (2.0**attempt), self._max_backoff)
                    self.n_retries += 1
                    self._sleep(delay)
                    attempt += 1
                    continue
                # Retry-exhausted or non-retryable: ABSENT, never a fabricated verdict.
                self.n_errors += 1
                self.last_prompt_tokens = 0
                self.last_completion_tokens = 0
                return None
        usage = body.get("usage") or {}
        self.last_prompt_tokens = int(usage.get("prompt_tokens", 0))
        self.last_completion_tokens = int(usage.get("completion_tokens", 0))
        self.prompt_tokens += self.last_prompt_tokens
        self.completion_tokens += self.last_completion_tokens
        self.n_calls += 1
        try:
            content = body["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError):
            return None
        if content is None or not str(content).strip():
            return None  # empty → ABSENT
        return str(content)


# --------------------------------------------------------------------------- #
# Resilience helpers (atomic checkpoint, resume, budget, backoff are all REUSED)
# --------------------------------------------------------------------------- #
def _estimate_tokens(text: str) -> int:
    """A coarse ``~chars/4`` prompt-token estimate for the pre-call budget guard.
    Deterministic; only the budget projection (never the recorded cost) uses it."""
    return max(1, len(text) // 4)


def _fully_absent(judgment: Judgment, metrics: tuple[str, ...]) -> bool:
    """True iff EVERY metric is ABSENT (the judge call failed / was a dead cell).
    Such a cell is RE-judged on resume; a ≥1-decided-metric cell is kept. Mirrors
    :func:`eval.autoe_judge._is_fully_absent` (kept local to avoid a private import)."""
    return all(judgment.verdicts.get(m, "ABSENT") == "ABSENT" for m in metrics)


def _dump_judgments(out: Mapping[JudgmentKey, Judgment]) -> dict[str, dict[str, str]]:
    """Serialize ``{JudgmentKey: Judgment}`` → ``{custom_id: {metric: verdict}}``
    (the resume sidecar), via the harness's idempotent ``custom_id`` encoding."""
    return {key.to_custom_id(): dict(j.verdicts) for key, j in out.items()}


def _load_judgments(blob: Mapping[str, Any]) -> dict[JudgmentKey, Judgment]:
    """Inverse of :func:`_dump_judgments`: rebuild ``{JudgmentKey: Judgment}`` from a
    checkpoint's ``judgments`` map (tolerant of a malformed entry → skipped)."""
    out: dict[JudgmentKey, Judgment] = {}
    for cid, verdicts in (blob.get("judgments") or {}).items():
        try:
            key = JudgmentKey.from_custom_id(str(cid))
        except ValueError:
            continue
        if isinstance(verdicts, Mapping):
            out[key] = Judgment(key=key, verdicts={str(m): str(v) for m, v in verdicts.items()})
    return out


def _resolve_checkpoint_source(
    checkpoint_path: Optional[Path], resume: Optional[Path]
) -> Optional[Path]:
    """The resume source — auto-detected, no manual flag required (mirrors
    :func:`eval.m1_verdict_run._resolve_resume`): an explicit ``--resume`` path
    wins; else this run's own checkpoint sidecar when it already exists; else
    ``None`` (a clean from-scratch pass)."""
    if resume is not None:
        return resume
    if checkpoint_path is not None and checkpoint_path.exists():
        return checkpoint_path
    return None


def _judge_resiliently(
    judge: PilotJudge,
    answers_by_arm: Mapping[str, Mapping[str, str]],
    questions: Sequence[tuple[str, str]],
    pair: tuple[str, str],
    *,
    n_runs: int,
    metrics: tuple[str, ...],
    judge_model: str,
    ledger: BudgetLedger,
    checkpoint_path: Optional[Path],
    existing: Mapping[JudgmentKey, Judgment],
    cost_state: dict[str, int],
) -> dict[JudgmentKey, Judgment]:
    """Judge every ``(question, run, order)`` for ``pair`` — the resilient sibling
    of :func:`eval.autoe_judge.run_autoe`.

    Adds, around the harness's per-key primitives (which it reuses unchanged): a
    :class:`BudgetLedger` **pre-call** guard (raises :class:`BudgetExceeded` BEFORE
    a call that would exceed ``--max-usd``), an **atomic checkpoint after each judged
    key** (so a kill mid-run loses nothing), and idempotent resume (a key already
    decided is skipped; a fully-ABSENT dead cell is re-judged). A ``None`` completion
    → an ABSENT-everywhere :class:`Judgment`, never a fabricated score.

    ``cost_state`` accumulates ``n_calls`` / ``prompt_tokens`` / ``completion_tokens``
    over NEW calls (carried across a resume via the checkpoint), feeding the
    measured-token cost projection."""
    t_arm, c_arm = pair
    out: dict[JudgmentKey, Judgment] = dict(existing)
    for qid, text in questions:
        t_ans, c_ans = answers_by_arm[t_arm][qid], answers_by_arm[c_arm][qid]
        for run_idx in range(n_runs):
            for order in (ORDER_TC, ORDER_CT):
                key = JudgmentKey(question_id=qid, pair=pair, run_idx=run_idx, order=order)
                if key in out and not _fully_absent(out[key], metrics):
                    continue  # idempotent resume — re-judge only dead (fully-ABSENT) cells
                if order == ORDER_TC:
                    answer_a, answer_b = t_ans, c_ans
                else:
                    answer_a, answer_b = c_ans, t_ans
                prompt = build_pairwise_prompt(text, answer_a, answer_b, metrics)
                est_pt = _estimate_tokens(prompt)
                # Pre-call budget guard: raise BEFORE the call that would exceed the cap.
                ledger.guard(judge_model, est_pt)
                completion = judge.judge_pair(text, answer_a, answer_b, metrics)
                # Record measured tokens (fall back to the estimate for a fake judge).
                pt = getattr(judge, "last_prompt_tokens", None)
                ct = getattr(judge, "last_completion_tokens", None)
                pt = est_pt if pt is None else int(pt)
                ct = _ESTIMATE_COMPLETION_TOKENS if ct is None else int(ct)
                ledger.record(judge_model, pt, ct)
                cost_state["n_calls"] += 1
                cost_state["prompt_tokens"] += pt
                cost_state["completion_tokens"] += ct
                out[key] = Judgment(key=key, verdicts=parse_verdict(completion, metrics))
                if checkpoint_path is not None:
                    _atomic_write_json(
                        checkpoint_path,
                        {
                            "judgments": _dump_judgments(out),
                            "judge_model": judge_model,
                            "cost": {**cost_state, "spent_usd": ledger.spent},
                        },
                    )
    return out


# --------------------------------------------------------------------------- #
# Question sampling (balanced spread across AutoQ buckets)
# --------------------------------------------------------------------------- #
def _sample_questions(questions: Sequence[AutoQQuestion], *, limit: int) -> list[tuple[str, str]]:
    """Sample up to ``limit`` questions as a **balanced round-robin** across buckets,
    in stable bucket-then-input order (deterministic, so resume re-derives the same
    set + the same :class:`JudgmentKey`s).

    Returns ``[(qid, question_text)]`` with a unique, separator-safe ``qid`` per
    question (the question's own id when present, else ``"<bucket>#<n>"``)."""
    by_bucket: dict[str, list[AutoQQuestion]] = {}
    for q in questions:
        by_bucket.setdefault(q.bucket, []).append(q)
    pools = [list(v) for v in by_bucket.values()]
    picked: list[AutoQQuestion] = []
    while len(picked) < limit and any(pools):
        for pool in pools:
            if not pool:
                continue
            picked.append(pool.pop(0))
            if len(picked) >= limit:
                break
    out: list[tuple[str, str]] = []
    for i, q in enumerate(picked):
        qid = q.question_id or f"{q.bucket}#{i}"
        qid = qid.replace("||", "__")  # keep the custom_id separator unambiguous
        out.append((qid, q.question_text))
    return out


def _make_ledger(max_usd: Optional[float], opening_spent: float = 0.0) -> BudgetLedger:
    """A fresh pilot ledger: opening balance 0, hard cap ``--max-usd`` (or ``inf``
    when unset — track spend, never guard). Output-token projection sized small (the
    judge emits a tiny JSON verdict). ``opening_spent`` restores a resumed total."""
    cap = float(max_usd) if max_usd is not None else math.inf
    ledger = BudgetLedger(opening_balance_usd=0.0, hard_cap_usd=cap, max_output_tokens=64)
    if opening_spent:
        ledger.restore_spent(float(opening_spent))
    return ledger


def _is_priced(model: str) -> bool:
    """Whether ``model`` has pinned pricing. A BYO/local answerer (e.g. a llama.cpp /
    ollama shim) is legitimately ``$0`` and unpinned — for it the answerer leg is a
    free local generation (not guarded, projected at ``$0``); a hosted/priced answerer
    is routed through the same :class:`BudgetLedger` as the judge so ``--max-usd``
    bounds the TOTAL spend (answerer + judge), not just the judge leg."""
    try:
        price_for(model)
    except UnpinnedPricing:
        return False
    return True


class _MeteringAnswerer(BaseAnswerer):
    """A thin metering wrapper around the shared :class:`BaseAnswerer` so the
    answerer-generation leg is **guarded and recorded through the same ledger** as the
    judge — making the ``--max-usd`` cap and the HITL spend number TOTAL spend, not
    judge-only (§9 fallback review [P2] #2).

    Per delegated :meth:`answer` it estimates the answerer prompt tokens, applies the
    :class:`BudgetLedger` **pre-call** guard (only when the answerer model is priced —
    a local/$0 answerer is never guarded), calls the inner answerer, then records the
    measured-or-estimated tokens into the ledger and ``cost_state``. It reuses the
    harness's :func:`eval.autoe_judge.build_arm_answers` unchanged (the metering lives
    in this wrapper, not in the harness)."""

    def __init__(
        self,
        inner: BaseAnswerer,
        *,
        model: str,
        priced: bool,
        ledger: BudgetLedger,
        cost_state: dict[str, int],
    ) -> None:
        self._inner = inner
        self._model = model
        self._priced = priced
        self._ledger = ledger
        self._cost_state = cost_state
        self.model_id = getattr(inner, "model_id", "<unset>")

    @property
    def available(self) -> bool:
        return bool(getattr(self._inner, "available", True))

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        # Unused: answer() is overridden to delegate to the inner answerer directly.
        raise NotImplementedError  # pragma: no cover

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        prompt = self._inner.build_prompt(question, context)
        est_pt = _estimate_tokens(prompt)
        if self._priced:
            # Pre-call budget guard for the answerer leg — raise BEFORE the call that
            # would exceed --max-usd (so the cap bounds answerer + judge spend).
            self._ledger.guard(self._model, est_pt)
        answer = self._inner.answer(question, context) or ""
        pt = getattr(self._inner, "last_prompt_tokens", None)
        ct = getattr(self._inner, "last_completion_tokens", None)
        pt = est_pt if pt is None else int(pt)
        ct = max(_estimate_tokens(answer), 1) if ct is None else int(ct)
        if self._priced:
            self._ledger.record(self._model, pt, ct)
        self._cost_state["n_calls"] += 1
        self._cost_state["prompt_tokens"] += pt
        self._cost_state["completion_tokens"] += ct
        return answer


# --------------------------------------------------------------------------- #
# 2 — run_pilot orchestration
# --------------------------------------------------------------------------- #
def run_pilot(
    *,
    answerer: BaseAnswerer,
    judge: PilotJudge,
    documents: Mapping[str, str],
    questions: Sequence[AutoQQuestion],
    families: Mapping[str, str],
    pair: tuple[str, str] = DEFAULT_PAIR,
    limit: int = 10,
    n_runs: int = MIN_RUNS,
    k: int = 10,
    seed: int = 0,
    n_boot: int = DEFAULT_N_BOOT,
    judge_model: Optional[str] = None,
    answerer_model: Optional[str] = None,
    projection_n_runs: Optional[int] = None,
    checkpoint_path: Optional[str | Path] = None,
    resume: Optional[str | Path] = None,
    max_usd: Optional[float] = None,
    target_questions: int = 100,
    cheap_validate: bool = False,
) -> dict[str, Any]:
    """Run the resilient AutoE pilot and return a report dict.

    Orchestration: sample ``limit`` AutoQ questions balanced across buckets → build
    the ``{vector_rag, long_context}`` baseline adapters over ``documents`` → produce
    each arm's answer through the **shared** ``answerer`` (the identical-answerer
    invariant) → judge the ``pair`` over ``n_runs`` × both orders **resiliently**
    (checkpoint/resume/backoff/budget) → :func:`compute_winrates` → assemble the bias
    controls + length corroboration → the Slice-5 kill-early premise read
    (:func:`strong_baseline_clears` of the treatment vs the long-context control) →
    a measured-token cost projection for a ``target_questions``-sized powered run.

    **Cross-family self-preference guard (fail loud):** the ``judge.family`` must NOT
    be among the arm answerer families in ``families`` — a self-preference-biased
    judge is a measurement error, so this raises :class:`ValueError` BEFORE any spend.

    ``cheap_validate`` forces the tiny-N **execution** path (:data:`CHEAP_LIMIT` ×
    :data:`CHEAP_N_RUNS`) whose only job is to prove the pipeline and size the spend.
    Critically, the cost projection sizes the *powered* run (``projection_n_runs``,
    default ``max(n_runs, MIN_RUNS)``) — NOT the 1-run cheap execution — because the
    powered run needs ``n_runs >= MIN_RUNS`` (``decide_084`` blocks below it); using the
    cheap ``n_runs=1`` would under-project the powered spend ~5× (§9 review [P2] #1). No
    network here unless ``judge`` makes one; tests inject a fake.
    """
    # The cost projection sizes the POWERED run, so resolve its n_runs BEFORE the
    # cheap-validate execution override drops n_runs to 1 (§9 review [P2] #1).
    projected_n_runs = (
        int(projection_n_runs) if projection_n_runs is not None else max(int(n_runs), MIN_RUNS)
    )
    if cheap_validate:
        limit = CHEAP_LIMIT
        n_runs = CHEAP_N_RUNS

    t_arm, c_arm = pair
    for arm in (t_arm, c_arm):
        if arm not in families:
            raise ValueError(f"families map missing the arm {arm!r} (need its answerer family)")

    # --- cross-family self-preference guard (BEFORE any spend) --------------- #
    arm_families = [families[t_arm], families[c_arm]]
    bias_controls = assemble_bias_controls(
        n_runs=n_runs, judge_family=judge.family, system_families=arm_families
    )
    judge_fam = judge.family.strip().lower()
    if judge_fam in {f.strip().lower() for f in arm_families}:
        raise ValueError(
            f"self-preference bias: judge family {judge.family!r} is among the arm "
            f"answerer families {arm_families!r}; the judge must be cross-family "
            "(design §5). Refusing to run a biased measurement."
        )

    resolved_model = judge_model or getattr(judge, "model_id", None) or DEFAULT_JUDGE_MODEL
    pin, pout = price_for(resolved_model)  # fail closed BEFORE spend if judge model unpinned

    # The shared-answerer leg: priced (hosted) → guarded+recorded through the SAME ledger
    # as the judge so --max-usd bounds TOTAL spend; unpinned (BYO/local) → a free local
    # generation, projected at $0 (§9 review [P2] #2).
    resolved_answerer_model = (
        answerer_model or getattr(answerer, "model_id", None) or DEFAULT_JUDGE_MODEL
    )
    answerer_priced = _is_priced(resolved_answerer_model)

    # --- sample + build the arms' answers ------------------------------------ #
    qpairs = _sample_questions(questions, limit=limit)
    adapters: dict[str, RetrievalAdapter] = {
        "vector_rag": VectorRagAdapter(documents),
        "long_context": LongContextAdapter(documents),
    }
    for arm in (t_arm, c_arm):
        if arm not in adapters:
            raise ValueError(f"unknown arm {arm!r}; known baselines: {sorted(adapters)}")

    # --- resume state (load BEFORE building the ledger / answering) ----------- #
    ckpt = Path(checkpoint_path) if checkpoint_path is not None else None
    source = _resolve_checkpoint_source(ckpt, Path(resume) if resume is not None else None)
    existing: dict[JudgmentKey, Judgment] = {}
    cost_state = {"n_calls": 0, "prompt_tokens": 0, "completion_tokens": 0}
    if source is not None and source.exists():
        blob = json.loads(source.read_text(encoding="utf-8"))
        existing = _load_judgments(blob)
        prior_cost = blob.get("cost") or {}
        for key in ("n_calls", "prompt_tokens", "completion_tokens"):
            cost_state[key] = int(prior_cost.get(key, 0))

    # The judge spend already paid (restored from the persisted JUDGE token counts, so a
    # resume never double-charges the re-run answerer leg). The ledger then accrues this
    # process's answerer + new-judge spend on top — the live --max-usd guard is TOTAL.
    judge_opening_spent = (
        cost_state["prompt_tokens"] / 1e6 * pin + cost_state["completion_tokens"] / 1e6 * pout
    )
    ledger = _make_ledger(max_usd, opening_spent=judge_opening_spent)

    # The answerer leg runs through the metering wrapper (pre-call guard + record on the
    # shared ledger); build_arm_answers itself is the harness function, reused unchanged.
    answerer_cost_state = {"n_calls": 0, "prompt_tokens": 0, "completion_tokens": 0}
    metering_answerer = _MeteringAnswerer(
        answerer,
        model=resolved_answerer_model,
        priced=answerer_priced,
        ledger=ledger,
        cost_state=answerer_cost_state,
    )
    answers_by_arm = build_arm_answers(metering_answerer, adapters, qpairs, k=k)

    # --- the resilient judging pass ------------------------------------------ #
    out = _judge_resiliently(
        judge,
        answers_by_arm,
        qpairs,
        pair,
        n_runs=n_runs,
        metrics=JUDGE_METRICS,
        judge_model=resolved_model,
        ledger=ledger,
        checkpoint_path=ckpt,
        existing=existing,
        cost_state=cost_state,
    )

    # --- aggregation → report ------------------------------------------------ #
    # Judge leg: one call scores all metrics for each (question, pair, run, order); the
    # projection sizes the POWERED run (projected_n_runs >= MIN_RUNS), not the cheap exec.
    projection = project_autoe_cost(
        prompt_tokens=cost_state["prompt_tokens"],
        completion_tokens=cost_state["completion_tokens"],
        n_calls=max(cost_state["n_calls"], 1),
        price_in_per_1m=pin,
        price_out_per_1m=pout,
        n_questions=target_questions,
        n_pairs=1,
        n_runs=projected_n_runs,
    )
    # Answerer leg: one shared-answerer call per arm per question (answers are reused
    # across runs/orders → no run/order fan-out). Priced → its cost is projected and
    # added to the TOTAL; unpinned (local/$0) → $0 (§9 review [P2] #2).
    n_arms = len({t_arm, c_arm})
    if answerer_priced:
        apin, apout = price_for(resolved_answerer_model)
        answerer_projection = project_autoe_cost(
            prompt_tokens=answerer_cost_state["prompt_tokens"],
            completion_tokens=answerer_cost_state["completion_tokens"],
            n_calls=max(answerer_cost_state["n_calls"], 1),
            price_in_per_1m=apin,
            price_out_per_1m=apout,
            n_questions=target_questions,
            n_pairs=n_arms,
            n_runs=1,
            n_orders=1,
        )
    else:
        answerer_projection = {
            "context_token_budget": None,
            "projected_prompt_tokens_per_call": 0.0,
            "cost_per_call_usd": 0.0,
            "projected_full_calls": target_questions * n_arms,
            "projected_full_usd": 0.0,
        }
    judge_full_usd = float(projection["projected_full_usd"])
    answerer_full_usd = float(answerer_projection["projected_full_usd"])
    cost_total = {
        "judge_usd": judge_full_usd,
        "answerer_usd": answerer_full_usd,
        "projected_full_usd": round(judge_full_usd + answerer_full_usd, 2),
        "answerer_model": resolved_answerer_model,
        "answerer_priced": answerer_priced,
        "max_usd_bounds": (
            "answerer+judge"
            if answerer_priced
            else "judge-only (answerer model unpinned → treated as local/$0)"
        ),
    }

    decided = [j for j in out.values() if not _fully_absent(j, JUDGE_METRICS)]
    report: dict[str, Any] = {
        "mode": "cheap_validate" if cheap_validate else "full",
        "pair": list(pair),
        "n_runs": n_runs,
        "projection_n_runs": projected_n_runs,
        "limit": limit,
        "k": k,
        "seed": seed,
        "n_questions_sampled": len(qpairs),
        "judge_model": resolved_model,
        "judge_family": judge.family,
        "bias_controls": dict(bias_controls),
        "measured": dict(cost_state),
        "answerer_measured": dict(answerer_cost_state),
        "cost_projection": projection,
        "answerer_cost_projection": answerer_projection,
        "cost_total": cost_total,
        "target_questions": target_questions,
        "ledger": {
            "spent_usd": ledger.spent,
            "hard_cap_usd": ledger.hard_cap_usd if math.isfinite(ledger.hard_cap_usd) else None,
            "remaining_usd": ledger.remaining if math.isfinite(ledger.hard_cap_usd) else None,
            "max_usd": max_usd,
        },
    }

    if not decided:
        # Empty / all-ABSENT: NEVER fabricate a win-rate (compute_winrates would raise).
        report["status"] = "ABSENT"
        report["per_metric"] = {}
        report["length_corroboration"] = None
        report["premise_strong_baseline_clears"] = None
        return report

    try:
        per_metric = compute_winrates(
            out.values(), pair, metrics=HEADLINE_METRICS, n_boot=n_boot, seed=seed
        )
        length = assemble_length_corroboration(
            out.values(), pair, ran=True, n_boot=n_boot, seed=seed
        )
    except ValueError:
        # A headline metric had zero decided judgments — refuse to fabricate.
        report["status"] = "ABSENT"
        report["per_metric"] = {}
        report["length_corroboration"] = None
        report["premise_strong_baseline_clears"] = None
        return report

    premise_per_metric = {
        m: strong_baseline_clears({"ci_lo": per_metric[m]["ci_lo"]}) for m in HEADLINE_METRICS
    }
    report["status"] = "OK"
    report["per_metric"] = per_metric
    report["length_corroboration"] = dict(length)
    report["premise_strong_baseline_clears"] = {
        "per_metric": premise_per_metric,
        "all_clear": all(premise_per_metric.values()),
    }
    return report


# --------------------------------------------------------------------------- #
# 6 — CLI (gated by R2_RUN=1 for any real call)
# --------------------------------------------------------------------------- #
def _parse_pair(value: str) -> tuple[str, str]:
    parts = [p.strip() for p in value.split(",") if p.strip()]
    if len(parts) != 2:
        raise SystemExit(f"--pair must be 'treatment,comparator'; got {value!r}")
    return (parts[0], parts[1])


def main(argv: Optional[list[str]] = None) -> int:
    """CLI entry point. Gated by ``R2_RUN=1`` — with ``R2_RUN`` unset it refuses to
    make any real call (prints a blocker and returns non-zero) so importing this
    module and running its unit tests never touches the network or the corpus."""
    import argparse

    parser = argparse.ArgumentParser(description="Resilient AutoE pilot runner (0.8.4 Slice 5b)")
    parser.add_argument("--limit", type=int, default=10)
    parser.add_argument("--n-runs", type=int, default=MIN_RUNS)
    parser.add_argument("--pair", type=_parse_pair, default=DEFAULT_PAIR)
    parser.add_argument("--cheap-validate", action="store_true")
    parser.add_argument("--checkpoint", default=None)
    parser.add_argument("--resume", default=None)
    parser.add_argument("--out", required=True)
    parser.add_argument("--max-usd", type=float, default=None)
    parser.add_argument("--target-questions", type=int, default=100)
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument(
        "--answerer-model",
        default=None,
        help="Pricing id for the shared-answerer leg (default: the answerer's model_id). "
        "A priced id routes the answerer through the --max-usd ledger (TOTAL spend); an "
        "unpinned/local id is treated as a free local generation.",
    )
    args = parser.parse_args(argv)

    if os.environ.get(_R2_RUN_ENV) != "1":
        print(
            f"[pilot] {_R2_RUN_ENV} not set — refusing any real call. Set {_R2_RUN_ENV}=1 "
            "plus the answerer (R2_ANSWERER_*) and judge (R2_JUDGE_*) env to run."
        )
        return 2

    from eval.apnews_corpus import load_articles, load_autoq

    answerer: BaseAnswerer = LLMAnswerer()
    if not answerer.available:
        answerer = NullAnswerer()
        print("[pilot] answerer LLM unavailable (R2_ANSWERER_* unset) — cannot answer; aborting.")
        return 3
    judge = LLMJudge()
    if not judge.available:
        print("[pilot] judge LLM unavailable (R2_JUDGE_* unset) — aborting.")
        return 3

    articles = load_articles()
    documents = {a.doc_id: a.body for a in articles}
    questions = load_autoq()
    answerer_family = family_of(answerer.model_id)
    families = {arm: answerer_family for arm in args.pair}

    report = run_pilot(
        answerer=answerer,
        judge=judge,
        documents=documents,
        questions=questions,
        families=families,
        pair=args.pair,
        limit=args.limit,
        n_runs=args.n_runs,
        answerer_model=args.answerer_model,
        k=args.k,
        checkpoint_path=args.checkpoint,
        resume=args.resume,
        max_usd=args.max_usd,
        target_questions=args.target_questions,
        cheap_validate=args.cheap_validate,
    )
    Path(args.out).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"[pilot] wrote {args.out} (status={report.get('status')})")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
