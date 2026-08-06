"""S3 — the strict `earp.v1` resolver.

Makes it impossible to express a run that cannot be honestly executed. Pure: no
SDK calls, no database, no network, no filesystem beyond the config file and
EARP's own packaged schema.

Rejections are RETURNED, never raised, matching S1's `GoldVerification` and
S2's `check_depth`. They are also COLLECTED rather than first-failure -- a
deliberate departure from `verify_gold`, whose checks are ordered by trust
because an unverified file's fields cannot be trusted enough to name a second
defect. Config keys carry no such dependency.

Design of record: `dev/design/earp-slice-3-design.md`.
"""

from __future__ import annotations

import hashlib
import json
import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Mapping

from eval.earp.depth import check_depth
from eval.earp.schema import CONFIG_SCHEMA_PATH
from eval.earp.schema.models import (
    Blocker,
    BlockerCode,
    CampaignKind,
    DecisionRule,
    Direction,
    RetrievalMode,
)
from eval.earp.schema.validate import assert_supported, declared_paths, validate

_SCHEMA: dict[str, Any] = json.loads(CONFIG_SCHEMA_PATH.read_text(encoding="utf-8"))

#: (call, use_default_embedder) -> (mode, max measurable K).
#:
#: Derived from BOTH, not from the call alone: `Engine.search` is hybrid only
#: when an embedder is configured. With none, the vector branch is skipped and
#: the run is pure node FTS, so deriving `hybrid` would record a mode the run
#: did not use AND refuse depths it could honestly measure.
#:
#: `search_projected_text` is the exception that shows depth is a CALL-SITE
#: property, not a mode property: it is FTS by mechanism, yet its reader breaks
#: at `results.len() >= limit` with `limit = max(override, SEARCH_RERANK_LIMIT)`,
#: so it truncates at 10 despite its SQL carrying no LIMIT.
CALL_MODE: Mapping[tuple[str, bool], tuple[RetrievalMode, int | None]] = {
    ("Engine.search_text_only", False): (RetrievalMode.FTS_ONLY, None),
    ("Engine.search_text_only", True): (RetrievalMode.FTS_ONLY, None),
    ("Engine.search", False): (RetrievalMode.FTS_ONLY, None),
    ("Engine.search", True): (RetrievalMode.HYBRID, 10),
    ("Engine.search_projected_text", False): (RetrievalMode.FTS_ONLY, 10),
    ("Engine.search_projected_text", True): (RetrievalMode.FTS_ONLY, 10),
}

#: Which query knobs each call actually accepts.
CALL_PARAMS: Mapping[str, frozenset[str]] = {
    "Engine.search": frozenset({"rerank_depth", "use_graph_arm", "alpha", "pool_n"}),
    "Engine.search_projected_text": frozenset({"projection_name"}),
    "Engine.search_text_only": frozenset(),
}

#: Campaign kinds `earp.v1` structurally cannot represent: it has exactly one
#: `scenario` object and no arms array, and no key referencing a prior run.
INEXPRESSIBLE: Mapping[str, str] = {
    "comparison": "S8 (needs >= 2 arms; earp.v1 has one scenario and no arms array)",
    "sweep": "S8 (needs N arms; earp.v1 has one scenario and no arms array)",
    "replay": "S6 (needs a reference to a prior run_id; earp.v1 has no such key)",
}

#: metric name -> whether `@k` is required, and its emitting condition.
METRIC_NAMES: Mapping[str, str] = {
    "strict_evidence_recall": "per_k",
    "graded_evidence_recall": "per_k",
    "supporting_coverage": "per_k",
    "abstention_rate": "k_free",
    "mrr": "never",
    "ndcg": "never",
}


@dataclass(frozen=True)
class Consumer:
    slice_id: str
    applies: Callable[[Mapping[str, Any]], bool] = lambda _doc: True


def _always(_doc: Mapping[str, Any]) -> bool:
    return True


def _never(_doc: Mapping[str, Any]) -> bool:
    """`comparison.*` is owned by S8 AND unusable in v1, because the only
    campaign that could consume it is itself inexpressible. The
    inexpressible-campaign refusal outranks carrying."""
    return False


CONSUMER_REGISTRY: Mapping[str, Consumer] = {
    "schema_version": Consumer("S3"),
    "campaign": Consumer("S3"),
    "corpus": Consumer("S1"),
    "corpus.snapshot": Consumer("S1"),
    "corpus.manifest": Consumer("S1"),
    "corpus.data_root": Consumer("S1"),
    "gold": Consumer("S1"),
    "gold.path": Consumer("S1"),
    "gold.sha256": Consumer("S1"),
    "gold.corpus_hash": Consumer("S1"),
    "gold.qrels_version": Consumer("S1"),
    "scenario": Consumer("S3"),
    "scenario.engine": Consumer("S5"),
    "scenario.engine.use_default_embedder": Consumer("S3"),
    "scenario.store": Consumer("S5"),
    "scenario.store.mode": Consumer("S5"),
    "scenario.query": Consumer("S3"),
    "scenario.query.call": Consumer("S3"),
    "scenario.query.rerank_depth": Consumer("S5"),
    "scenario.query.use_graph_arm": Consumer("S5"),
    "scenario.query.alpha": Consumer("S5"),
    "scenario.query.pool_n": Consumer("S5"),
    "scenario.query.projection_name": Consumer("S5"),
    "metrics": Consumer("S6"),
    "metrics.evidence_recall_k": Consumer("S6"),
    "metrics.document_metrics": Consumer("S6"),
    "metrics.integrity": Consumer("S6"),
    "decision_rule": Consumer("S8"),
    "decision_rule.metric": Consumer("S3"),
    "decision_rule.direction": Consumer("S3"),
    "decision_rule.threshold": Consumer("S3"),
    "comparison": Consumer("S8", _never),
    "comparison.changed_knobs": Consumer("S8", _never),
    "comparison.strata": Consumer("S8", _never),
    "comparison.ci_method": Consumer("S8", _never),
    "comparison.seed": Consumer("S8", _never),
    "comparison.resamples": Consumer("S8", _never),
    "comparison.min_n": Consumer("S8", _never),
    "budget": Consumer("S9"),
    #: Carried unconditionally: there is no priced-arm declaration in earp.v1,
    #: so any applicability predicate would be undecidable.
    "budget.estimated_usd": Consumer("S9"),
}


@dataclass(frozen=True)
class ResolvedScenario:
    campaign: CampaignKind
    config_sha256: str
    query_call: str
    retrieval_mode: RetrievalMode
    max_measurable_k: int | None
    use_default_embedder: bool
    query_params: Mapping[str, Any]
    evidence_recall_k: tuple[int, ...]
    document_metrics: tuple[str, ...]
    corpus: Mapping[str, str] | None
    gold: Mapping[str, str] | None
    decision_rule: DecisionRule | None
    consumed_paths: frozenset[str]
    carried_paths: frozenset[str]


@dataclass(frozen=True)
class ConfigResolution:
    """`blockers` is empty iff `scenario` is not None."""

    blockers: tuple[Blocker, ...] = ()
    scenario: ResolvedScenario | None = None
    #: Non-fatal notes, e.g. paths carried for a later slice.
    notes: tuple[str, ...] = field(default_factory=tuple)


def schema_paths() -> tuple[str, ...]:
    """Every dotted path `earp.config.v1` declares."""
    assert_supported(_SCHEMA)
    return tuple(declared_paths(_SCHEMA))


def emits(name: str, *, evidence_recall_k: tuple[int, ...], has_negatives: bool) -> bool:
    """Whether a campaign can emit the named metric.

    Grammar: `<metric>@<k>`. `@k` is REQUIRED for the three per-K names, since
    only `metrics.per_k` is K-keyed, and FORBIDDEN for the rest.
    """
    base, _, suffix = name.partition("@")
    kind = METRIC_NAMES.get(base)
    if kind is None:
        return False
    if kind == "never":
        # ndcg: no graded relevance exists anywhere in this repo.
        # mrr: no landed or planned slice computes it, so gating on it would
        # await a number that never arrives.
        return False
    if kind == "per_k":
        if not suffix.isdigit():
            return False
        return int(suffix) in evidence_recall_k
    return suffix == "" and has_negatives


def _blocker(code: BlockerCode, message: str, path: str) -> Blocker:
    # The path is in the message as well as the detail: AC-1 requires the
    # message to name the offending key, and a caller reading only the message
    # (the CLI's stderr, a log line) must still be able to find it.
    return Blocker(
        code=code, message=f"{path}: {message}", stage="config.resolve", detail={"path": path}
    )


_FINDING_CODE = {
    "unknown": BlockerCode.CONFIG_UNKNOWN_KEY,
    "missing": BlockerCode.CONFIG_MISSING_KEY,
    "invalid": BlockerCode.CONFIG_INVALID_VALUE,
}

#: Keys removed from the schema, with the reason, so their rejection is named
#: rather than a bare "unknown key".
_REMOVED_KEYS = {
    "scenario.query.mode": (
        "removed: the retrieval mode is derived from `call` (and the embedder), "
        "because `call` already determines it and `vector_only` has no SDK entry point"
    ),
}


def _declared_paths_of(doc: Mapping[str, Any], prefix: str = "") -> set[str]:
    found: set[str] = set()
    for key, value in doc.items():
        path = f"{prefix}{key}"
        found.add(path)
        if isinstance(value, dict):
            found |= _declared_paths_of(value, f"{path}.")
    return found


def resolve_config(doc: Mapping[str, Any]) -> ConfigResolution:
    """Resolve a config document, collecting every defect."""
    blockers: list[Blocker] = []

    for finding in validate(doc, _SCHEMA):
        message = _REMOVED_KEYS.get(finding.path, finding.message)
        blockers.append(_blocker(_FINDING_CODE[finding.kind], message, finding.path))

    campaign_raw = doc.get("campaign")
    if isinstance(campaign_raw, str) and campaign_raw in INEXPRESSIBLE:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_CAMPAIGN_INEXPRESSIBLE,
                (
                    f"campaign `{campaign_raw}` cannot be represented in earp.v1; "
                    f"owned by {INEXPRESSIBLE[campaign_raw]}"
                ),
                "campaign",
            )
        )

    declared = _declared_paths_of(doc)
    for path in sorted(declared):
        consumer = CONSUMER_REGISTRY.get(path)
        if consumer is not None and not consumer.applies(doc):
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_UNUSED_KEY,
                    (
                        f"`{path}` is inapplicable to this config; its owning slice "
                        f"({consumer.slice_id}) can only consume it from a campaign "
                        f"kind earp.v1 cannot express"
                    ),
                    path,
                )
            )

    def _mapping(source: Mapping[str, Any], key: str) -> dict[str, Any]:
        """A defective section resolves to empty rather than crashing the pass:
        the schema walker has already recorded the defect, and every other rule
        must still get its chance to report."""
        value = source.get(key)
        return dict(value) if isinstance(value, dict) else {}

    scenario = _mapping(doc, "scenario")
    query = _mapping(scenario, "query")
    engine = _mapping(scenario, "engine")
    metrics = _mapping(doc, "metrics")

    call = query.get("call")
    use_default_embedder = bool(engine.get("use_default_embedder", False))
    mode: RetrievalMode | None = None
    max_k: int | None = None
    if isinstance(call, str) and (call, use_default_embedder) in CALL_MODE:
        mode, max_k = CALL_MODE[(call, use_default_embedder)]

        accepted = CALL_PARAMS[call]
        for knob in query:
            if knob != "call" and knob in CALL_PARAMS_ALL and knob not in accepted:
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INAPPLICABLE_KNOB,
                        f"`{call}` does not accept `{knob}`",
                        f"scenario.query.{knob}",
                    )
                )
        if call == "Engine.search_projected_text" and "projection_name" not in query:
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_MISSING_KEY,
                    (
                        "`Engine.search_projected_text` takes a required positional "
                        "`name`; without `projection_name` the config resolves and "
                        "cannot run"
                    ),
                    "scenario.query.projection_name",
                )
            )

    ladder = tuple(metrics.get("evidence_recall_k", ()) or ())
    document_metrics = tuple(metrics.get("document_metrics", ()) or ())

    if mode is not None:
        for k in ladder:
            if not isinstance(k, int) or isinstance(k, bool):
                continue
            blocker = check_depth(mode, k) if max_k is None else None
            if max_k is not None and k > max_k:
                blocker = check_depth(RetrievalMode.HYBRID, k)
            if blocker is not None:
                blockers.append(blocker)

    if campaign_raw == "diagnostic" and ladder:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_INAPPLICABLE_KNOB,
                "a diagnostic campaign runs without gold, so it MUST NOT declare "
                "`evidence_recall_k`",
                "metrics.evidence_recall_k",
            )
        )

    if (ladder or document_metrics) and "gold" not in doc:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_MISSING_KEY,
                "declaring recall or document metrics requires `gold`",
                "gold",
            )
        )
    if (ladder or document_metrics) and "corpus" not in doc:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_MISSING_KEY,
                "declaring recall or document metrics requires `corpus`",
                "corpus",
            )
        )

    alpha = query.get("alpha")
    if isinstance(alpha, float) and not math.isfinite(alpha):
        # minimum/maximum cannot catch NaN: nan < 0 and nan > 1 are both False.
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_INVALID_VALUE,
                "`alpha` must be finite; the engine would clamp or misread it",
                "scenario.query.alpha",
            )
        )

    rule_doc = doc.get("decision_rule")
    decision_rule: DecisionRule | None = None
    if isinstance(rule_doc, dict) and isinstance(rule_doc.get("metric"), str):
        metric = rule_doc["metric"]
        has_negatives = True  # gold-dependent; S6 re-checks against real gold
        if not emits(metric, evidence_recall_k=ladder, has_negatives=has_negatives):
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_INVALID_VALUE,
                    (
                        f"decision rule names `{metric}`, which this campaign cannot "
                        f"emit; a rule on an unemittable metric would never evaluate"
                    ),
                    "decision_rule.metric",
                )
            )
        elif rule_doc.get("direction") in {"greater", "less"}:
            decision_rule = DecisionRule(
                metric=metric,
                direction=Direction(rule_doc["direction"]),
                threshold=float(rule_doc["threshold"]),
            )

    if blockers:
        return ConfigResolution(blockers=tuple(blockers))

    carried = {
        path
        for path in declared
        if path in CONSUMER_REGISTRY and CONSUMER_REGISTRY[path].slice_id != "S3"
    }
    canonical = json.dumps(doc, sort_keys=True, separators=(",", ":"))
    assert mode is not None and isinstance(call, str)
    return ConfigResolution(
        scenario=ResolvedScenario(
            campaign=CampaignKind(campaign_raw),
            config_sha256=hashlib.sha256(canonical.encode("utf-8")).hexdigest(),
            query_call=call,
            retrieval_mode=mode,
            max_measurable_k=max_k,
            use_default_embedder=use_default_embedder,
            query_params={k: v for k, v in query.items() if k != "call"},
            evidence_recall_k=ladder,
            document_metrics=document_metrics,
            corpus=doc.get("corpus"),
            gold=doc.get("gold"),
            decision_rule=decision_rule,
            consumed_paths=frozenset(declared - carried),
            carried_paths=frozenset(carried),
        )
    )


CALL_PARAMS_ALL = frozenset().union(*CALL_PARAMS.values())


def load_config(path: Path | str) -> Mapping[str, Any]:
    """Load a config file. JSON is a YAML subset, so `.json` parses either way."""
    text = Path(path).read_text(encoding="utf-8")
    try:
        import yaml  # noqa: PLC0415 -- declared only in the test/dev extras
    except ImportError:  # pragma: no cover - pyyaml is a declared dev dep
        return json.loads(text)
    loaded = yaml.safe_load(text)
    if not isinstance(loaded, dict):
        raise ValueError("config must be a mapping")
    return loaded


__all__ = [
    "CALL_MODE",
    "CALL_PARAMS",
    "CONSUMER_REGISTRY",
    "INEXPRESSIBLE",
    "METRIC_NAMES",
    "ConfigResolution",
    "Consumer",
    "ResolvedScenario",
    "emits",
    "load_config",
    "resolve_config",
    "schema_paths",
]
