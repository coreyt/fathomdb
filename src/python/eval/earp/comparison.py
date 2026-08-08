"""S8 — comparison and sweep: a comparative claim is earned, not assumed.

A comparison is two named arms over the same frozen corpus and gold, whose
resolved configurations differ at exactly the declared knob paths, paired per
immutable gold `query_id`, with the CI method, seed, resample count, effect
metric, and power rule all fixed before the first retrieval. A sweep is N such
arms with outcomes recorded and NO comparative claim.

This module owns ALL writing for arms campaigns: the per-arm executor
(`characterize.execute_arm`) is pure -- it never writes -- so a comparison can
never leave standalone characterization records per arm, and a blocked arm
becomes a blocked COMPARISON run rather than a one-armed number.

Design of record: `dev/design/earp-slice-8-design.md`.
"""

from __future__ import annotations

import platform
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence

from eval.earp._experiments import lib as _lib
from eval.earp.characterize import (
    ArmExecution,
    _git_sha,
    _metrics_document,
    execute_arm,
)
from eval.earp.config import resolve_config
from eval.earp.schema.models import (
    SCHEMA_VERSION_RESULT,
    Blocker,
    RunVerdict,
)
from eval.earp.stats import paired_bootstrap_ci
from eval.earp.writer import write_run

#: metric base name -> the per-query row field carrying its value.
_METRIC_FIELDS: Mapping[str, str] = {
    "strict_evidence_recall": "strict",
    "graded_evidence_recall": "graded",
    "supporting_coverage": "supporting_coverage",
}


@dataclass(frozen=True)
class PairedMetric:
    """The paired set and its exclusions, reconciling exactly:
    `len(deltas) + sum(exclusions.values()) == gold query count`."""

    deltas: tuple[float, ...]
    exclusions: Mapping[str, int]

    @property
    def n(self) -> int:
        return len(self.deltas)


@dataclass(frozen=True)
class ArmsCampaignResult:
    verdict: RunVerdict
    run_id: str | None = None
    run_dir: Path | None = None
    n: int = 0
    effect: float | None = None
    ci_low: float | None = None
    ci_high: float | None = None
    underpowered: bool | None = None
    exclusions: Mapping[str, int] = field(default_factory=dict)
    #: The decision_rule.result token, or None when no rule was declared (or
    #: the run blocked before anything could be evaluated).
    decision: str | None = None
    blockers: tuple[Blocker, ...] = ()
    per_query_rows: list[dict[str, Any]] = field(default_factory=list)


def _values_by_query(
    rows: Sequence[Mapping[str, Any]], metric: str
) -> tuple[dict[str, float], set[str]]:
    """(query_id -> metric value, errored query_ids) for one arm's rows.

    A query is VALUED only when the row is scored and the metric field is
    non-null: negative-class rows score with `strict`/`graded` = None, so they
    enter only an abstention-metric comparison; positives carry no `abstained`,
    so they enter only recall/coverage comparisons.
    """
    base, _, suffix = metric.partition("@")
    valued: dict[str, float] = {}
    errored: set[str] = set()
    if base in _METRIC_FIELDS:
        k = int(suffix)
        for row in rows:
            if row.get("k") != k:
                continue
            query_id = str(row["query_id"])
            if row.get("outcome") == "error":
                errored.add(query_id)
                continue
            value = row.get(_METRIC_FIELDS[base])
            if value is not None:
                valued[query_id] = float(value)
    elif base == "abstention_rate":
        # Abstention is K-independent (top-K is empty iff the result set is),
        # so the first row per query carries the whole answer.
        seen: set[str] = set()
        for row in rows:
            query_id = str(row["query_id"])
            if query_id in seen:
                continue
            seen.add(query_id)
            if row.get("outcome") == "error":
                errored.add(query_id)
                continue
            abstained = row.get("abstained")
            if abstained is not None:
                valued[query_id] = 1.0 if abstained else 0.0
    return valued, errored


def pair_rows(
    metric: str,
    query_ids: Sequence[str],
    control_rows: Sequence[Mapping[str, Any]],
    treatment_rows: Sequence[Mapping[str, Any]],
) -> PairedMetric:
    """Pair per-query rows on immutable gold `query_id`, in gold order.

    A query enters the paired set only with a VALUE on the metric in both
    arms. Every other query is excluded by reason: `error` when either arm's
    retrieval failed for it, `metric_inapplicable` when it scored but carries
    no value on this metric (e.g. a negative under a recall metric). The
    buckets partition the gold set, so `n + sum(exclusions)` reconciles
    exactly with the query count -- enforced, not assumed.
    """
    control_values, control_errors = _values_by_query(control_rows, metric)
    treatment_values, treatment_errors = _values_by_query(treatment_rows, metric)
    deltas: list[float] = []
    exclusions: dict[str, int] = {}
    for query_id in query_ids:
        if query_id in control_errors or query_id in treatment_errors:
            exclusions["error"] = exclusions.get("error", 0) + 1
        elif query_id in control_values and query_id in treatment_values:
            deltas.append(treatment_values[query_id] - control_values[query_id])
        else:
            exclusions["metric_inapplicable"] = (
                exclusions.get("metric_inapplicable", 0) + 1
            )
    if len(deltas) + sum(exclusions.values()) != len(query_ids):
        raise RuntimeError(
            f"pairing invariant violated: {len(deltas)} paired + "
            f"{sum(exclusions.values())} excluded != {len(query_ids)} gold queries"
        )
    return PairedMetric(deltas=tuple(deltas), exclusions=exclusions)


def _decision_token(*, n: int, min_n: int, effect: float, rule: Any) -> str:
    """The decision_rule.result mapping (design § comparison.py).

    `withheld` is checked at n == 0 BEFORE the underpowered rule: the schema
    floors `min_n` at 1, so a literal n < min_n precedence would make
    `withheld` unreachable for every legal config -- and an empty paired set
    is not thin data, it is NO data, which is exactly what `withheld` says.
    `underpowered` covers 0 < n < min_n and takes precedence over evaluating
    the threshold. The boolean `comparison.underpowered` stays `n < min_n`
    exactly (AC-6), including at n == 0.
    """
    if n == 0:
        return "withheld"
    if n < min_n:
        return "underpowered"
    if rule.direction.value == "greater":
        return "pass" if effect > rule.threshold else "fail"
    return "pass" if effect < rule.threshold else "fail"


def run_comparison(
    *,
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    manifest_path: Path | None = None,
    retrieve_overrides: Mapping[str, Callable[[str], Any]] | None = None,
    arm_executor: Callable[..., ArmExecution] | None = None,
    blank_provenance: bool = False,
) -> ArmsCampaignResult:
    """Run a two-arm comparison campaign: arms[0] is control, arms[1] is
    treatment; effect = mean of per-query (treatment - control) deltas."""
    return _run_arms(
        config_doc=config_doc,
        expected_campaign="comparison",
        experiments_root=experiments_root,
        experiment=experiment,
        ts=ts,
        manifest_path=manifest_path,
        retrieve_overrides=retrieve_overrides,
        arm_executor=arm_executor,
        blank_provenance=blank_provenance,
    )


def run_sweep(
    *,
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    manifest_path: Path | None = None,
    retrieve_overrides: Mapping[str, Callable[[str], Any]] | None = None,
    arm_executor: Callable[..., ArmExecution] | None = None,
    blank_provenance: bool = False,
) -> ArmsCampaignResult:
    """Run a sweep: N arms, per-arm outcomes and blockers recorded,
    `comparison: null` in the sidecar -- no deltas, no CI, no claim."""
    return _run_arms(
        config_doc=config_doc,
        expected_campaign="sweep",
        experiments_root=experiments_root,
        experiment=experiment,
        ts=ts,
        manifest_path=manifest_path,
        retrieve_overrides=retrieve_overrides,
        arm_executor=arm_executor,
        blank_provenance=blank_provenance,
    )


def _run_arms(
    *,
    config_doc: Mapping[str, Any],
    expected_campaign: str,
    experiments_root: Path,
    experiment: str,
    ts: datetime,
    manifest_path: Path | None,
    retrieve_overrides: Mapping[str, Callable[[str], Any]] | None,
    arm_executor: Callable[..., ArmExecution] | None,
    blank_provenance: bool,
) -> ArmsCampaignResult:
    resolution = resolve_config(config_doc)
    if resolution.blockers:
        # A config that does not resolve never produces a run record: refusal
        # at resolution is S3's contract, and writing a blocked run for it
        # would mint a run identity for a document that cannot execute.
        raise ValueError(
            f"config does not resolve: "
            f"{[b.message for b in resolution.blockers][:4]}"
        )
    if config_doc.get("campaign") != expected_campaign or not resolution.arms:
        raise ValueError(f"expected a resolved `{expected_campaign}` arms campaign")

    corpus = config_doc.get("corpus")
    gold = config_doc.get("gold")
    if not isinstance(corpus, Mapping) or not isinstance(gold, Mapping):
        raise ValueError("arms campaigns require `corpus` and `gold` blocks")

    executor = arm_executor if arm_executor is not None else execute_arm
    overrides = dict(retrieve_overrides or {})
    executions: dict[str, ArmExecution] = {}
    for arm in resolution.arms:
        executions[arm.name] = executor(
            scenario=arm.scenario,
            data_root=Path(str(corpus.get("data_root") or "")),
            snapshot_path=Path(str(corpus.get("snapshot") or "")),
            gold_path=Path(str(gold.get("path") or "")),
            gold_sha256=str(gold.get("sha256") or ""),
            corpus_hash=str(gold.get("corpus_hash") or ""),
            qrels_version=str(gold.get("qrels_version") or ""),
            manifest_path=manifest_path,
            retrieve_override=overrides.get(arm.name),
        )

    strata = resolution.comparison.strata if resolution.comparison else ()
    rows: list[dict[str, Any]] = []
    for arm in resolution.arms:
        for source_row in executions[arm.name].rows:
            row = dict(source_row)
            row["arm"] = arm.name
            if "query_class" in strata:
                row["stratum"] = row.get("query_class")
            rows.append(row)

    blockers: list[Blocker] = []
    for arm in resolution.arms:
        arm_blocker = executions[arm.name].blocker
        if arm_blocker is not None:
            blockers.append(
                Blocker(
                    code=arm_blocker.code,
                    message=f"arm `{arm.name}`: {arm_blocker.message}",
                    stage=arm_blocker.stage,
                    detail={**arm_blocker.detail, "arm": arm.name},
                )
            )
    verdict = RunVerdict.BLOCKED if blockers else RunVerdict.COMPLETE

    comparison_value: dict[str, Any] | None = None
    rule_value: dict[str, Any] | None = None
    n = 0
    effect: float | None = None
    ci_low: float | None = None
    ci_high: float | None = None
    underpowered: bool | None = None
    exclusions: dict[str, int] = {}
    decision: str | None = None

    comparison = resolution.comparison
    if comparison is not None and verdict is RunVerdict.COMPLETE:
        control, treatment = resolution.arms[0], resolution.arms[1]
        query_ids = [query.query_id for query in executions[control.name].queries]
        paired = pair_rows(
            comparison.metric,
            query_ids,
            executions[control.name].rows,
            executions[treatment.name].rows,
        )
        n = paired.n
        exclusions = dict(paired.exclusions)
        # Sequential f64 summation in gold order -- the same semantics the
        # CI's resample means use; a "better" sum would break determinism
        # parity with the pinned bootstrap.
        total = 0.0
        for delta in paired.deltas:
            total += delta
        effect = total / n if n else 0.0
        ci_low, ci_high = paired_bootstrap_ci(
            paired.deltas, seed=comparison.seed, resamples=comparison.resamples
        )
        underpowered = n < comparison.min_n
        comparison_value = {
            "n": n,
            "effect": effect,
            "ci_low": ci_low,
            "ci_high": ci_high,
            "ci_method": comparison.ci_method,
            "seed": comparison.seed,
            "changed_knobs": list(comparison.changed_knobs),
            "underpowered": underpowered,
            "metric": comparison.metric,
            "exclusions": exclusions,
        }
        rule = resolution.decision_rule
        if rule is not None:
            decision = _decision_token(
                n=n, min_n=comparison.min_n, effect=effect, rule=rule
            )
            rule_value = {
                "metric": rule.metric,
                "direction": rule.direction.value,
                "threshold": rule.threshold,
                "result": decision,
            }

    control_arm = resolution.arms[0]
    control_execution = executions[control_arm.name]
    arms_value: dict[str, Any] = {}
    for arm in resolution.arms:
        execution = executions[arm.name]
        entry: dict[str, Any] = {
            "query_call": arm.scenario.query_call,
            "retrieval_mode": arm.scenario.retrieval_mode.value,
            "fanout_used": arm.scenario.max_measurable_k,
            "arm_config_sha256": arm.scenario.config_sha256,
            "blockers": [
                {
                    "code": execution.blocker.code.value,
                    "message": execution.blocker.message,
                    "stage": execution.blocker.stage,
                    "detail": execution.blocker.detail,
                }
            ]
            if execution.blocker is not None
            else [],
        }
        if execution.blocker is None:
            entry["metrics"] = _metrics_document(execution.per_k)
        arms_value[arm.name] = entry

    sha = _lib.config_sha256(dict(config_doc))
    run_id = _lib.make_run_id(experiment, ts, sha)
    sidecar: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION_RESULT,
        "run_id": run_id,
        "campaign": expected_campaign,
        "verdict": verdict.value,
        # The scenario block is written from the CONTROL arm (arms[0]); the
        # `arms` object is the authoritative per-arm record. config_sha256 is
        # the WHOLE-document hash -- the run identity, covering both arms.
        "scenario": {
            "config_sha256": sha,
            "query_call": control_arm.scenario.query_call,
            "retrieval_mode": control_arm.scenario.retrieval_mode.value,
            "fanout_used": control_arm.scenario.max_measurable_k,
        },
        "arms": arms_value,
        "metrics": (
            _metrics_document(control_execution.per_k)
            if control_execution.blocker is None
            else {}
        ),
        "comparison": comparison_value,
        "witnesses": [],
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
    if expected_campaign == "comparison":
        # Explicit null when no rule was declared (or the run blocked): effect
        # and CI are the comparison's OUTPUT and are always recorded; the
        # claim token exists only under a predeclared rule. Sweeps omit the
        # key entirely -- they have no claim path at all.
        sidecar["decision_rule"] = rule_value

    read = (
        f"{expected_campaign} over {len(resolution.arms)} arms: "
        + (
            f"n={n}, effect={effect}"
            if comparison_value is not None
            else ("blocked" if blockers else "outcomes recorded, no claim")
        )
    )
    outcome = write_run(
        experiment=experiment,
        ts=ts,
        config_doc=config_doc,
        experiments_root=experiments_root,
        verdict=verdict,
        read=read,
        metrics=dict(sidecar["metrics"]),
        per_query=rows,
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
        seeds=(
            {"comparison_seed": comparison.seed} if comparison is not None else {}
        ),
        cost_usd=0.0,
        n=n if comparison_value is not None else None,
    )
    return ArmsCampaignResult(
        verdict=verdict,
        run_id=outcome.run_id,
        run_dir=outcome.run_dir,
        n=n,
        effect=effect,
        ci_low=ci_low,
        ci_high=ci_high,
        underpowered=underpowered,
        exclusions=exclusions,
        decision=decision,
        blockers=tuple(blockers),
        per_query_rows=rows,
    )


__all__ = [
    "ArmsCampaignResult",
    "PairedMetric",
    "pair_rows",
    "run_comparison",
    "run_sweep",
]
