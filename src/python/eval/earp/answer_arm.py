"""S9 — the opt-in priced answer arm, behind the enforced D-3 budget gate.

A priced arm runs only when ALL FOUR gates hold, in order: declared (config,
with `max_queries` and `budget.estimated_usd`), opted in (`FDB_EARP_PRICED=1`
AND the wrapped protocol's own `available` property), under budget (the
authoritative ledger + the cumulative preflight + the estimate cross-check),
and cheap-validated ($0, witnessed in the SAME run before the first priced
call). A failed opt-in is a VISIBLE SKIP (D-2: never a pass, never a zero);
every budget failure is a typed blocker.

The one real adapter is the R2 identical-answerer (`eval.r2_parity_eval`'s
answerer protocol: `R2_RUN=1` + `R2_ANSWERER_BASE_URL` / `R2_ANSWERER_MODEL`,
same prompt template, same `available` property), scored by the protocol's own
$0 `PerClassScorer.score_answer`. Mem0 / extractor / GPU arms are catalog
UNSUPPORTED entries (`eval.earp.knobs`); commissioning them is an HITL scope
decision.

Cheap-validation, owned openly: repo precedent reads it as a cheap PRICED
model pass; S9 deliberately reinterprets it as a $0 stub/recorded-fixture pass
-- stricter in dollars, weaker in endpoint coverage -- and the first real
priced call remains the endpoint validation, bounded by the per-call guard.

Design of record: `dev/design/earp-slice-9-design.md`.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Mapping, Protocol, Sequence, runtime_checkable

from eval.earp._experiments import lib as _lib
from eval.earp.characterize import (
    _git_sha,
    _load_snapshot_shards,
    _metrics_document,
    execute_arm,
)
from eval.earp.config import ResolvedAnswerArm, resolve_config
from eval.earp.pricing import (
    D3_AUTHORIZED_USD,
    CallGuard,
    authoritative_root,
    preflight,
    price_for,
    read_cumulative_spend,
)
from eval.earp.schema.models import (
    SCHEMA_VERSION_PER_QUERY,
    SCHEMA_VERSION_RESULT,
    Blocker,
    CostLedger,
    RunVerdict,
    Witness,
    WitnessSource,
    WitnessStatus,
)
from eval.earp.writer import write_run
from eval.r2_parity_eval import (
    LLMAnswerer,
    PerClassScorer,
    StubAnswerer,
    normalize_answer,
)

#: The run-time opt-in (the FDB_EARP_INTEGRATION house pattern, S7).
FDB_EARP_PRICED_ENV = "FDB_EARP_PRICED"

#: Conservative per-call token bounds for the PRE-call worst case (the
#: `estimate_tokens` precedent: intentionally an over-estimate, so the guard
#: errs toward halting).
_EST_PROMPT_TOKENS = 2048
_EST_OUTPUT_TOKENS = 512

#: answer_outcome token -> its score under `PerClassScorer.score_answer`.
ANSWER_OUTCOME_SCORE: Mapping[str, float] = {
    "correct": 1.0,
    "incorrect": 0.0,
    "abstained_miss": 0.0,
    "false_positive": 0.0,
    "correct_abstention": 1.0,
}


@dataclass(frozen=True)
class AnswerTask:
    """One query the arm may answer: the question, its gold ground truth
    (empty for the negative class), and the RETRIEVED contexts it consumes --
    the arm executes after retrieval scoring, on retrieval's output."""

    query_id: str
    query_class: str
    question: str
    ground_truth: tuple[str, ...]
    context: tuple[str, ...]


@dataclass(frozen=True)
class AnswerCall:
    """One priced (or fake-priced) completion, as metered."""

    query_id: str
    answer_text: str | None
    cost_usd: float
    metered: bool
    error: str | None = None


@dataclass(frozen=True)
class AnswerRunResult:
    calls: tuple[AnswerCall, ...]
    actual_usd: float
    halted: Blocker | None
    #: "usage" when every cost came from the endpoint's usage body;
    #: "estimated-not-metered" when any fell back to chars/4; "stub" for the
    #: test adapter's fake costs.
    metering: str


@runtime_checkable
class AnswerAdapter(Protocol):
    """The adapter protocol: a worst-case estimate, a $0 cheap-validation, and
    the guarded run."""

    @property
    def model_id(self) -> str: ...

    @property
    def model_source(self) -> str: ...

    @property
    def available(self) -> bool: ...

    def estimate(self, n_queries: int) -> float | Blocker: ...

    def cheap_validate(self, tasks: Sequence[AnswerTask]) -> Witness | Blocker: ...

    def run(self, tasks: Sequence[AnswerTask], guard: CallGuard) -> AnswerRunResult: ...


def _cheap_validate_witness(model_id: str, tasks: Sequence[AnswerTask]) -> Witness:
    """The $0 validation pass: the deterministic stub answerer over the real
    tasks, witnessed BEFORE any priced call in the same run."""
    stub = StubAnswerer()
    answered = 0
    for task in tasks:
        if stub.answer(task.question, list(task.context)) is not None:
            answered += 1
    return Witness(
        name="answer_arm_cheap_validate",
        source=WitnessSource.ANSWER_ARM,
        call_path="eval.r2_parity_eval.StubAnswerer.answer",
        status=WitnessStatus.OBSERVED,
        value={
            "cost_usd": 0.0,
            "n": len(tasks),
            "answered": answered,
            "model": model_id,
            "passed": True,
        },
    )


@dataclass
class StubAnswerAdapter:
    """The test adapter: `r2_parity_eval.StubAnswerer` behind the full gate
    machinery, at $0 real spend -- including a FAKE nonzero cost path so
    budget arithmetic is testable without any network call.

    `invocations` records every protocol method reached; `forbid` turns a
    reached method into an assertion failure, which is how gate-ordering tests
    PROVE later stages are unreached rather than merely unobserved.
    """

    cost_per_call_usd: float = 0.0
    #: Worst case handed to the CallGuard per call; defaults to the fake cost.
    guard_worst_case_usd: float | None = None
    #: When set, `estimate` returns this instead of n * cost_per_call.
    estimate_override: float | None = None
    available_override: bool = True
    cheap_validate_blocker: Blocker | None = None
    forbid: frozenset[str] = frozenset()
    model_id: str = "stub-deterministic-v1"
    model_source: str = "config"
    invocations: list[str] = field(default_factory=list)

    def _reached(self, method: str) -> None:
        if method in self.forbid:
            raise AssertionError(
                f"gate ordering violated: adapter.{method} was reached but this "
                f"test declared it unreachable"
            )
        self.invocations.append(method)

    @property
    def available(self) -> bool:
        return self.available_override

    def estimate(self, n_queries: int) -> float | Blocker:
        self._reached("estimate")
        if self.estimate_override is not None:
            return self.estimate_override
        return n_queries * self.cost_per_call_usd

    def cheap_validate(self, tasks: Sequence[AnswerTask]) -> Witness | Blocker:
        self._reached("cheap_validate")
        if self.cheap_validate_blocker is not None:
            return self.cheap_validate_blocker
        return _cheap_validate_witness(self.model_id, tasks)

    def run(self, tasks: Sequence[AnswerTask], guard: CallGuard) -> AnswerRunResult:
        self._reached("run")
        stub = StubAnswerer()
        worst = (
            self.guard_worst_case_usd
            if self.guard_worst_case_usd is not None
            else self.cost_per_call_usd
        )
        calls: list[AnswerCall] = []
        halted: Blocker | None = None
        spent = 0.0
        for task in tasks:
            blocker = guard.guard(worst)
            if blocker is not None:
                halted = blocker
                break
            text = stub.answer(task.question, list(task.context))
            guard.record(self.cost_per_call_usd)
            spent += self.cost_per_call_usd
            calls.append(
                AnswerCall(
                    query_id=task.query_id,
                    answer_text=text,
                    cost_usd=self.cost_per_call_usd,
                    metered=True,
                )
            )
        return AnswerRunResult(
            calls=tuple(calls), actual_usd=spent, halted=halted, metering="stub"
        )


class _UsageCapturingAnswerer(LLMAnswerer):
    """The protocol's answerer with the completion WRAPPED to capture `usage`.

    `LLMAnswerer._complete` discards the response's usage body, which makes a
    $-cap unmeterable -- so this subclass performs the identical request (same
    endpoint shape, same temperature-0/seed-0 determinism, same
    `normalize_answer`) and keeps `last_usage`. Wrapping, not module-patching:
    the standing `r2_parity_eval` module is untouched.
    """

    def __init__(self) -> None:
        super().__init__()
        self.last_usage: Mapping[str, Any] | None = None
        self.last_prompt_chars: int = 0
        self.last_answer_chars: int = 0

    def _complete(self, prompt: str, question: str, context: list[str]) -> str | None:
        if os.environ.get("R2_RUN") != "1":
            raise RuntimeError("R2_RUN not set; set to 1 to run the priced answer arm")
        if not self.available:
            raise RuntimeError(
                "LLMAnswerer not configured: set R2_ANSWERER_BASE_URL + R2_ANSWERER_MODEL"
            )
        payload = json.dumps(
            {
                "model": self.model_id,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0,
                "seed": 0,
            }
        ).encode("utf-8")
        request = urllib.request.Request(
            self.base_url.rstrip("/") + "/chat/completions",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        with urllib.request.urlopen(request, timeout=120) as response:  # noqa: S310
            body = json.loads(response.read().decode("utf-8"))
        raw = body["choices"][0]["message"]["content"]
        usage = body.get("usage")
        self.last_usage = usage if isinstance(usage, Mapping) else None
        self.last_prompt_chars = len(prompt)
        self.last_answer_chars = len(raw or "")
        return normalize_answer(raw)


class R2IdenticalAnswerer:
    """The one commissioned priced adapter: the R2 identical-answerer protocol
    behind the S9 money gate.

    Credentials are the protocol's OWN (`R2_RUN=1` + `R2_ANSWERER_BASE_URL` +
    `R2_ANSWERER_MODEL`, via the wrapped `available` property). Cost is pinned
    `price_for` rates x the captured usage body; when the endpoint reports no
    usage, a declared chars/4 estimate is booked and the run is marked
    `estimated-not-metered`. Scoring is NOT here: the runner scores with the
    protocol's `PerClassScorer.score_answer`, deterministic and $0.
    """

    def __init__(self, answerer_model: str | None = None) -> None:
        self._answerer = _UsageCapturingAnswerer()
        if answerer_model is not None:
            self._answerer.model_id = answerer_model
            self.model_source = "config"
        else:
            #: The env default is legal only for claim-free runs (resolver
            #: rule); the sidecar records the resolved value marked as such.
            self.model_source = "env-resolved"

    @property
    def model_id(self) -> str:
        return self._answerer.model_id

    @property
    def available(self) -> bool:
        return self._answerer.available

    def _rates(self) -> tuple[float, float] | Blocker:
        return price_for(self.model_id)

    def _per_call_worst_case(self) -> float | Blocker:
        rates = self._rates()
        if isinstance(rates, Blocker):
            return rates
        in_rate, out_rate = rates
        return (in_rate * _EST_PROMPT_TOKENS + out_rate * _EST_OUTPUT_TOKENS) / 1_000_000

    def estimate(self, n_queries: int) -> float | Blocker:
        worst = self._per_call_worst_case()
        if isinstance(worst, Blocker):
            return worst
        return n_queries * worst

    def cheap_validate(self, tasks: Sequence[AnswerTask]) -> Witness | Blocker:
        return _cheap_validate_witness(self.model_id, tasks)

    def run(self, tasks: Sequence[AnswerTask], guard: CallGuard) -> AnswerRunResult:
        rates = self._rates()
        worst = self._per_call_worst_case()
        assert not isinstance(rates, Blocker) and not isinstance(worst, Blocker), (
            "run() is unreachable without a passed preflight, which prices the model"
        )
        in_rate, out_rate = rates
        calls: list[AnswerCall] = []
        halted: Blocker | None = None
        actual = 0.0
        metering = "usage"
        for task in tasks:
            blocker = guard.guard(worst)
            if blocker is not None:
                halted = blocker
                break
            try:
                text = self._answerer.answer(task.question, list(task.context))
            except Exception as exc:  # noqa: BLE001 -- a typed per-call failure
                calls.append(
                    AnswerCall(
                        query_id=task.query_id,
                        answer_text=None,
                        cost_usd=0.0,
                        metered=False,
                        error=f"{type(exc).__name__}: {exc}",
                    )
                )
                continue
            usage = self._answerer.last_usage
            if usage is not None:
                prompt_tokens = float(usage.get("prompt_tokens") or 0)
                completion_tokens = float(usage.get("completion_tokens") or 0)
                metered = True
            else:
                # chars/4, recorded as estimated-not-metered in the sidecar.
                prompt_tokens = self._answerer.last_prompt_chars / 4
                completion_tokens = self._answerer.last_answer_chars / 4
                metered = False
                metering = "estimated-not-metered"
            cost = (prompt_tokens * in_rate + completion_tokens * out_rate) / 1_000_000
            guard.record(cost)
            actual += cost
            calls.append(
                AnswerCall(
                    query_id=task.query_id,
                    answer_text=text,
                    cost_usd=cost,
                    metered=metered,
                )
            )
        return AnswerRunResult(
            calls=tuple(calls), actual_usd=actual, halted=halted, metering=metering
        )


@dataclass(frozen=True)
class AnswerArmOutcome:
    """One arm execution through the four gates. Either `skipped` (visible-
    skip witness, no blockers, nothing priced), or blocked (typed blockers),
    or executed (calls + rows + actual cost, possibly halted mid-run with
    partials kept)."""

    skipped: bool
    skip_reason: str | None = None
    witnesses: tuple[Witness, ...] = ()
    blockers: tuple[Blocker, ...] = ()
    calls: tuple[AnswerCall, ...] = ()
    rows: tuple[dict[str, Any], ...] = ()
    cumulative_spent_usd: float = 0.0
    actual_usd: float | None = None
    accuracy: float | None = None
    metering: str | None = None
    answerer_model: str = ""
    model_source: str = ""


def _outcome_token(ground_truth: tuple[str, ...], answer_text: str | None, score: float) -> str:
    if ground_truth:
        if answer_text is None:
            return "abstained_miss"
        return "correct" if score == 1.0 else "incorrect"
    return "correct_abstention" if answer_text is None else "false_positive"


def _answer_rows(
    tasks: Sequence[AnswerTask], calls: Sequence[AnswerCall]
) -> tuple[tuple[dict[str, Any], ...], float | None]:
    """Per-query answer rows (named additive fields, `k: null`) plus the
    aggregate accuracy over the SCORED rows."""
    by_id = {task.query_id: task for task in tasks}
    scorer = PerClassScorer()
    rows: list[dict[str, Any]] = []
    scores: list[float] = []
    for call in calls:
        task = by_id[call.query_id]
        row: dict[str, Any] = {
            "schema_version": SCHEMA_VERSION_PER_QUERY,
            "query_id": call.query_id,
            "query_class": task.query_class,
            "k": None,
        }
        if call.error is not None:
            row.update(
                {"outcome": "error", "reason": call.error, "answer_reason": call.error}
            )
            rows.append(row)
            continue
        score = scorer.score_answer(list(task.ground_truth), call.answer_text)
        scores.append(score)
        row.update(
            {
                "outcome": "scored",
                "strict": None,
                "graded": None,
                "required_n": None,
                "required_hits": None,
                "answer_outcome": _outcome_token(task.ground_truth, call.answer_text, score),
                "answer_text_sha": (
                    hashlib.sha256(call.answer_text.encode("utf-8")).hexdigest()
                    if call.answer_text is not None
                    else None
                ),
                "answer_reason": None,
            }
        )
        rows.append(row)
    accuracy = sum(scores) / len(scores) if scores else None
    return tuple(rows), accuracy


def run_answer_arm(
    *,
    arm: ResolvedAnswerArm,
    budget_estimated_usd: float,
    tasks: Sequence[AnswerTask],
    experiments_root: Path,
    adapter: AnswerAdapter | None = None,
) -> AnswerArmOutcome:
    """Gates 2-4, in the pinned order: opted-in -> budget (authoritative
    ledger, then the preflight with its cross-check) -> cheap-validate -> the
    guarded run. Each failing stage yields its own typed outcome and every
    later stage is UNREACHED (gate 1, `declared`, is the resolver's)."""
    live: AnswerAdapter = (
        adapter if adapter is not None else R2IdenticalAnswerer(arm.answerer_model)
    )

    # Gate 2 -- opted in: the env gate AND the protocol's own availability.
    if os.environ.get(FDB_EARP_PRICED_ENV) != "1":
        missing = f"{FDB_EARP_PRICED_ENV}=1 not set"
    elif not live.available:
        missing = (
            "answerer credentials absent (the protocol's own gate: R2_RUN=1 + "
            "R2_ANSWERER_BASE_URL + R2_ANSWERER_MODEL)"
        )
    else:
        missing = None
    if missing is not None:
        reason = f"answer arm skipped: {missing}"
        return AnswerArmOutcome(
            skipped=True,
            skip_reason=reason,
            witnesses=(
                Witness(
                    name="answer_arm_skipped",
                    source=WitnessSource.ANSWER_ARM,
                    call_path="eval.earp.answer_arm.run_answer_arm",
                    status=WitnessStatus.ABSENT,
                    value={"skipped": True, "missing_gate": missing, "reason": reason},
                ),
            ),
            answerer_model=live.model_id,
            model_source=live.model_source,
        )

    def _blocked(blocker: Blocker, cumulative: float = 0.0) -> AnswerArmOutcome:
        return AnswerArmOutcome(
            skipped=False,
            blockers=(blocker,),
            cumulative_spent_usd=cumulative,
            answerer_model=live.model_id,
            model_source=live.model_source,
        )

    # Gate 3a -- ONE authoritative ledger.
    root = authoritative_root(experiments_root)
    if isinstance(root, Blocker):
        return _blocked(root)
    cumulative = read_cumulative_spend(root)
    if isinstance(cumulative, Blocker):
        return _blocked(cumulative)
    ledger_path = str(root / "index.jsonl")

    # Gate 3b -- the preflight, cross-checked against the adapter's own
    # computed worst case over the BOUNDED call count.
    bounded_tasks = list(tasks[: arm.max_queries])
    computed = live.estimate(len(bounded_tasks))
    if isinstance(computed, Blocker):
        return _blocked(computed, cumulative)
    ledger = CostLedger(
        authorized_usd=D3_AUTHORIZED_USD,
        cumulative_spent_usd=cumulative,
        estimated_usd=budget_estimated_usd,
    )
    refusal = preflight(ledger, computed, ledger_path)
    if refusal is not None:
        return _blocked(refusal, cumulative)
    preflight_witness = Witness(
        name="answer_arm_ledger_preflight",
        source=WitnessSource.ANSWER_ARM,
        call_path="eval.earp.pricing.preflight",
        status=WitnessStatus.OBSERVED,
        value={
            "ledger_path": ledger_path,
            "cumulative_spent_usd": cumulative,
            "estimated_usd": budget_estimated_usd,
            "computed_estimate_usd": computed,
            "authorized_usd": D3_AUTHORIZED_USD,
            "projected_usd": cumulative + budget_estimated_usd,
        },
    )

    # Gate 4 -- cheap-validation, witnessed in the SAME run BEFORE any priced
    # call.
    validation = live.cheap_validate(bounded_tasks)
    if isinstance(validation, Blocker):
        return AnswerArmOutcome(
            skipped=False,
            witnesses=(preflight_witness,),
            blockers=(validation,),
            cumulative_spent_usd=cumulative,
            answerer_model=live.model_id,
            model_source=live.model_source,
        )

    # The guarded run. A halt is a typed blocker with the partials KEPT.
    guard = CallGuard(authorized_usd=D3_AUTHORIZED_USD, cumulative_spent_usd=cumulative)
    result = live.run(bounded_tasks, guard)
    rows, accuracy = _answer_rows(bounded_tasks, result.calls)
    outcome_witness = Witness(
        name="answer_arm_outcome",
        source=WitnessSource.ANSWER_ARM,
        call_path="eval.earp.answer_arm.AnswerAdapter.run",
        status=WitnessStatus.OBSERVED,
        value={
            "answered": len(result.calls),
            "of": len(bounded_tasks),
            "actual_usd": result.actual_usd,
            "metering": result.metering,
            "answerer_model": live.model_id,
            "model_source": live.model_source,
            "halted": result.halted is not None,
        },
    )
    return AnswerArmOutcome(
        skipped=False,
        witnesses=(preflight_witness, validation, outcome_witness),
        blockers=(result.halted,) if result.halted is not None else (),
        calls=result.calls,
        rows=rows,
        cumulative_spent_usd=cumulative,
        actual_usd=result.actual_usd,
        accuracy=accuracy if result.halted is None else None,
        metering=result.metering,
        answerer_model=live.model_id,
        model_source=live.model_source,
    )


@dataclass(frozen=True)
class AnswerCampaignResult:
    verdict: RunVerdict
    run_id: str | None = None
    run_dir: Path | None = None
    skipped: bool = False
    skip_reason: str | None = None
    blockers: tuple[Blocker, ...] = ()
    witnesses: tuple[Witness, ...] = ()
    per_query_rows: list[dict[str, Any]] = field(default_factory=list)
    accuracy: float | None = None
    cost: CostLedger | None = None


def run_answer_campaign(
    *,
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    adapter: AnswerAdapter | None = None,
    retrieve_override: Callable[[str], Any] | None = None,
    blank_provenance: bool = False,
) -> AnswerCampaignResult:
    """A characterization campaign whose scenario declares the priced answer
    arm: retrieval scoring first (the S8 arm executor), then the arm over the
    retrieved contexts, then ONE durable record carrying the cost block."""
    resolution = resolve_config(config_doc)
    if resolution.blockers:
        raise ValueError(
            f"config does not resolve: {[b.message for b in resolution.blockers][:4]}"
        )
    scenario = resolution.scenario
    if scenario is None or scenario.answer_arm is None:
        raise ValueError(
            "expected a resolved single-scenario campaign with `scenario.answer_arm`"
        )
    corpus = config_doc.get("corpus")
    gold = config_doc.get("gold")
    budget_doc = config_doc.get("budget")
    if not isinstance(corpus, Mapping) or not isinstance(gold, Mapping):
        raise ValueError("a priced answer campaign requires `corpus` and `gold` blocks")
    assert isinstance(budget_doc, Mapping)  # resolver-collected: arm => budget
    budget_estimated_usd = float(budget_doc["estimated_usd"])
    data_root = Path(str(corpus.get("data_root") or ""))
    snapshot_path = Path(str(corpus.get("snapshot") or ""))

    execution = execute_arm(
        scenario=scenario,
        data_root=data_root,
        snapshot_path=snapshot_path,
        gold_path=Path(str(gold.get("path") or "")),
        gold_sha256=str(gold.get("sha256") or ""),
        corpus_hash=str(gold.get("corpus_hash") or ""),
        qrels_version=str(gold.get("qrels_version") or ""),
        retrieve_override=retrieve_override,
    )
    if execution.blocker is not None:
        cost = CostLedger(
            authorized_usd=D3_AUTHORIZED_USD,
            cumulative_spent_usd=0.0,
            estimated_usd=budget_estimated_usd,
        )
        outcome = _write(
            config_doc, scenario, experiments_root, experiment, ts,
            RunVerdict.BLOCKED, {}, [], (), (execution.blocker,), cost,
            execution.blocker.message, blank_provenance,
        )
        return AnswerCampaignResult(
            verdict=RunVerdict.BLOCKED,
            run_id=outcome.run_id,
            run_dir=outcome.run_dir,
            blockers=(execution.blocker,),
            cost=cost,
        )

    # The arm consumes what retrieval returned: doc bodies, in retrieved order.
    items, _ = _load_snapshot_shards(data_root, snapshot_path)
    bodies = {item["logical_id"]: str(item["body"]) for item in items}
    tasks = [
        AnswerTask(
            query_id=query.query_id,
            query_class=query.query_class,
            question=query.query,
            ground_truth=tuple(
                str(answer) for answer in (query.extra.get("answers") or ())
            ),
            context=tuple(
                bodies[doc_id]
                for doc_id in execution.cache.get(query.query_id, [])
                if doc_id in bodies
            ),
        )
        for query in execution.queries
    ]

    arm_outcome = run_answer_arm(
        arm=scenario.answer_arm,
        budget_estimated_usd=budget_estimated_usd,
        tasks=tasks,
        experiments_root=Path(experiments_root),
        adapter=adapter,
    )
    verdict = RunVerdict.BLOCKED if arm_outcome.blockers else RunVerdict.COMPLETE
    cost = CostLedger(
        authorized_usd=D3_AUTHORIZED_USD,
        cumulative_spent_usd=arm_outcome.cumulative_spent_usd,
        estimated_usd=budget_estimated_usd,
        actual_usd=arm_outcome.actual_usd,
    )

    metrics: dict[str, Any] = {}
    if verdict is RunVerdict.COMPLETE:
        metrics = _metrics_document(execution.per_k)
        if arm_outcome.accuracy is not None:
            metrics["document_metrics"]["answer_accuracy"] = {
                "status": "emitted",
                "value": arm_outcome.accuracy,
            }
        else:
            # D-2: a skipped arm is NEVER a pass and NEVER a zero -- the slot
            # is explicitly inapplicable, with the missing gate named.
            metrics["document_metrics"]["answer_accuracy"] = {
                "status": "not_applicable",
                "value": None,
                "reason": arm_outcome.skip_reason
                or "the answer arm produced no scored outcomes",
            }

    rows = list(execution.rows) + [dict(row) for row in arm_outcome.rows]
    read = (
        arm_outcome.skip_reason
        if arm_outcome.skipped
        else (
            f"priced answer arm: {len(arm_outcome.calls)} call(s), "
            f"actual ${arm_outcome.actual_usd or 0.0:.4f}"
            if verdict is RunVerdict.COMPLETE
            else arm_outcome.blockers[0].message
        )
    )
    outcome = _write(
        config_doc, scenario, experiments_root, experiment, ts,
        verdict, metrics, rows, arm_outcome.witnesses, arm_outcome.blockers,
        cost, str(read), blank_provenance,
    )
    return AnswerCampaignResult(
        verdict=verdict,
        run_id=outcome.run_id,
        run_dir=outcome.run_dir,
        skipped=arm_outcome.skipped,
        skip_reason=arm_outcome.skip_reason,
        blockers=arm_outcome.blockers,
        witnesses=arm_outcome.witnesses,
        per_query_rows=rows,
        accuracy=arm_outcome.accuracy,
        cost=cost,
    )


def _write(
    config_doc: Mapping[str, Any],
    scenario: Any,
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    verdict: RunVerdict,
    metrics: Mapping[str, Any],
    rows: Sequence[Mapping[str, Any]],
    witnesses: tuple[Witness, ...],
    blockers: tuple[Blocker, ...],
    cost: CostLedger,
    read: str,
    blank_provenance: bool,
) -> Any:
    sha = _lib.config_sha256(dict(config_doc))
    run_id = _lib.make_run_id(experiment, ts, sha)
    cost_value: dict[str, Any] = {
        # The constant is the ONLY source of authorized_usd (D-3).
        "authorized_usd": D3_AUTHORIZED_USD,
        "cumulative_spent_usd": cost.cumulative_spent_usd,
        "estimated_usd": cost.estimated_usd,
        "actual_usd": cost.actual_usd,
    }
    sidecar: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION_RESULT,
        "run_id": run_id,
        "campaign": "characterization",
        "verdict": verdict.value,
        "scenario": {
            "config_sha256": sha,
            "query_call": scenario.query_call,
            "retrieval_mode": scenario.retrieval_mode.value,
            "fanout_used": scenario.max_measurable_k,
        },
        "metrics": dict(metrics),
        "cost": cost_value,
        "witnesses": [
            {
                "name": witness.name,
                "source": witness.source.value,
                "call_path": witness.call_path,
                "status": witness.status.value,
                "value": witness.value,
            }
            for witness in witnesses
        ],
        "blockers": [
            {
                "code": blocker.code.value,
                "message": blocker.message,
                "stage": blocker.stage,
                "detail": blocker.detail,
            }
            for blocker in blockers
        ],
    }
    return write_run(
        experiment=experiment,
        ts=ts,
        config_doc=config_doc,
        experiments_root=Path(experiments_root),
        verdict=verdict,
        read=read,
        metrics=dict(metrics),
        per_query=list(rows),
        sidecar=sidecar,
        code={
            "git_sha": _git_sha(blank_provenance),
            "dirty": False,
            "branch": "",
            "baseline_commit": None,
        },
        env={
            "python": "" if blank_provenance else platform.python_version(),
            "lockfile_sha256": None,
            "gpu": None,
            "key_deps": {},
        },
        corpus={"source": None, "manifest_sha256": None, "datasets": []},
        seeds={},
        # Actual spend feeds the index row -- which is what makes the D-3
        # ledger self-feeding for the NEXT run's preflight.
        cost_usd=cost.actual_usd if cost.actual_usd is not None else 0.0,
    )


__all__ = [
    "ANSWER_OUTCOME_SCORE",
    "FDB_EARP_PRICED_ENV",
    "AnswerAdapter",
    "AnswerArmOutcome",
    "AnswerCall",
    "AnswerCampaignResult",
    "AnswerRunResult",
    "AnswerTask",
    "R2IdenticalAnswerer",
    "StubAnswerAdapter",
    "run_answer_arm",
    "run_answer_campaign",
]
