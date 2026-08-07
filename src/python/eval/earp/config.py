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

import json
import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Mapping

from eval.earp._experiments import lib as _lib
from eval.earp.depth import check_depth
from eval.earp.schema import CONFIG_SCHEMA_PATH
from eval.earp.schema.models import (
    ENGINE_DEFAULT_RESULT_LIMIT,
    ENGINE_MAX_RESULT_LIMIT,
    Blocker,
    BlockerCode,
    CampaignKind,
    DecisionRule,
    Direction,
    RetrievalMode,
)
from eval.earp.schema.validate import assert_supported, declared_paths, validate

_SCHEMA: dict[str, Any] = json.loads(CONFIG_SCHEMA_PATH.read_text(encoding="utf-8"))

#: (call, use_default_embedder) -> retrieval mode.
#:
#: Derived from BOTH, not from the call alone: `Engine.search` is hybrid only
#: when an embedder is configured. With none, the vector branch is skipped and
#: the run is pure node FTS, so deriving `hybrid` would record a mode the run
#: did not use.
#:
#: Mode determines cost and semantics, no longer depth (S6a): every search
#: verb takes the public `limit`, and @K is measurable exactly when
#: `K <= limit` -- so there is no per-call max-K column any more.
CALL_MODE: Mapping[tuple[str, bool], RetrievalMode] = {
    ("Engine.search_text_only", False): RetrievalMode.FTS_ONLY,
    ("Engine.search_text_only", True): RetrievalMode.FTS_ONLY,
    ("Engine.search", False): RetrievalMode.FTS_ONLY,
    ("Engine.search", True): RetrievalMode.HYBRID,
    ("Engine.search_projected_text", False): RetrievalMode.FTS_ONLY,
    ("Engine.search_projected_text", True): RetrievalMode.FTS_ONLY,
}

#: Which query knobs each call actually accepts.
CALL_PARAMS: Mapping[str, frozenset[str]] = {
    "Engine.search": frozenset({"rerank_depth", "use_graph_arm", "alpha", "pool_n", "limit"}),
    "Engine.search_projected_text": frozenset({"projection_name", "limit"}),
    "Engine.search_text_only": frozenset({"limit"}),
}

#: Campaign kinds `earp.v1` structurally cannot represent: it has exactly one
#: `scenario` object and no arms array.
#:
#: `replay` is NOT here. It needs a pointer to a prior run, but that pointer is
#: a CLI argument rather than a config key -- putting it in the config would
#: change `config_sha256`, so the config-drift axis would fire on every replay
#: including a perfect one, destroying the only case worth reporting.
INEXPRESSIBLE: Mapping[str, str] = {
    "comparison": "S8 (needs >= 2 arms; earp.v1 has one scenario and no arms array)",
    "sweep": "S8 (needs N arms; earp.v1 has one scenario and no arms array)",
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


def _diagnostic_only(doc: Mapping[str, Any]) -> bool:
    """`scenario.fixture` and `scenario.query.text` describe ONE authored query
    over an authored fixture. A characterization has 4,597 gold queries and
    takes its text from `GoldQuery.query`, so carrying either would be a
    declaration the run must silently ignore."""
    return doc.get("campaign") == "diagnostic"


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
    "scenario.fixture": Consumer("S5", _diagnostic_only),
    "scenario.query": Consumer("S3"),
    "scenario.query.call": Consumer("S3"),
    "scenario.query.text": Consumer("S5", _diagnostic_only),
    "scenario.query.limit": Consumer("S5"),
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
    #: The resolved public result limit (S6a) -- an `int`, never None:
    #: "unbounded" no longer exists. Kept under its historical name because it
    #: still answers the same question, the deepest honestly-measurable K.
    max_measurable_k: int
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
    # The signature accepts any Mapping, but the walker checks JSON `object` as
    # `dict` and `_lib._resolved_dict` raises on anything else -- so normalise
    # once, here, rather than letting a MappingProxyType surface as a bogus
    # "must be object" defect or a TypeError from a function documented to
    # return rather than raise.
    doc = dict(doc)
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
    if isinstance(call, str) and (call, use_default_embedder) in CALL_MODE:
        mode = CALL_MODE[(call, use_default_embedder)]

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

    # The public result limit (S6a). Absent means the engine default -- the
    # hash covers the RAW document, so absence must keep resolving to 10 or
    # every existing config's identity moves. The schema owns the RANGE window
    # (minimum 1 / maximum 100, the alpha precedent): an out-of-window or
    # mistyped value is already a collected walker defect above, so depth
    # checks proceed against the default rather than double-reporting.
    declared_limit = query.get("limit")
    limit = ENGINE_DEFAULT_RESULT_LIMIT
    if (
        isinstance(declared_limit, int)
        and not isinstance(declared_limit, bool)
        and 1 <= declared_limit <= ENGINE_MAX_RESULT_LIMIT
    ):
        limit = declared_limit

    if mode is not None:
        for k in ladder:
            if not isinstance(k, int) or isinstance(k, bool):
                continue
            blocker = check_depth(mode, k, limit)
            if blocker is not None:
                blockers.append(blocker)

    if campaign_raw == "diagnostic":
        # ALL THREE metric keys, not just the ladder. A diagnostic makes no
        # relevance claim, so document metrics and integrity metrics are as
        # inapplicable as recall -- and refusing only the ladder left the
        # "no metric under any configuration" guarantee unenforced.
        for key in ("evidence_recall_k", "document_metrics", "integrity"):
            if metrics.get(key):
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INAPPLICABLE_KNOB,
                        f"a diagnostic campaign runs without gold and makes no "
                        f"relevance claim, so it MUST NOT declare `{key}`",
                        f"metrics.{key}",
                    )
                )
        if "gold" in doc:
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_INAPPLICABLE_KNOB,
                    "a diagnostic campaign runs without gold",
                    "gold",
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
    assert mode is not None and isinstance(call, str)
    return ConfigResolution(
        scenario=ResolvedScenario(
            campaign=CampaignKind(campaign_raw),
            # ONE canonicalisation. A second implementation here diverged from
            # `_lib.canonical_json` on `ensure_ascii`, so any non-ASCII config
            # would hash differently -- S4 would stage the sidecar into one run
            # directory while write_record materialized into another. `dict()`
            # because `_lib._resolved_dict` raises on a non-dict Mapping and
            # this function documents that it returns rather than raises.
            config_sha256=_lib.config_sha256(dict(doc)),
            query_call=call,
            retrieval_mode=mode,
            max_measurable_k=limit,
            use_default_embedder=use_default_embedder,
            # The resolved limit is INJECTED here, exactly once, whether or not
            # the config declared it: query_params is the single source the
            # runner passes through, so no duplicate-kwarg path exists and the
            # runner needs no knowledge of the knob.
            query_params={
                **{k: v for k, v in query.items() if k != "call"},
                "limit": limit,
            },
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
