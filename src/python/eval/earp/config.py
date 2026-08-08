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
import re
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
    DeclaredProjection,
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

#: Campaign kinds `earp.v1` structurally cannot represent. EMPTY since S8: the
#: `arms` array made `comparison` and `sweep` expressible, so every declared
#: kind now has an owning, executable path. The mechanism stays for any future
#: declared-but-unbuildable kind.
#:
#: `replay` was never here. It needs a pointer to a prior run, but that pointer
#: is a CLI argument rather than a config key -- putting it in the config would
#: change `config_sha256`, so the config-drift axis would fire on every replay
#: including a perfect one, destroying the only case worth reporting.
INEXPRESSIBLE: Mapping[str, str] = {}

#: The S8 kind split. `scenario` and `arms` are mutually exclusive per kind
#: (resolver-enforced; the schema cannot express it): single-scenario kinds
#: require `scenario`, arms kinds require `arms` -- `comparison` with exactly
#: 2 (arms[0] is control, arms[1] is treatment), `sweep` with >= 2.
ARMS_CAMPAIGNS: frozenset[str] = frozenset({"comparison", "sweep"})
SINGLE_SCENARIO_CAMPAIGNS: frozenset[str] = frozenset(
    {"characterization", "replay", "diagnostic"}
)

#: The whole v1 strata vocabulary (scoped commitment, not vacuous): declaring
#: `query_class` sets each per-query row's `stratum` to its gold query_class.
STRATA_VOCABULARY: frozenset[str] = frozenset({"query_class"})

#: metric name -> whether `@k` is required, and its emitting condition.
METRIC_NAMES: Mapping[str, str] = {
    "strict_evidence_recall": "per_k",
    "graded_evidence_recall": "per_k",
    "supporting_coverage": "per_k",
    "abstention_rate": "k_free",
    #: S9 -- arm-implied, never config-requested: it lands in the result's
    #: free-keyed `metrics.document_metrics` map and emits ONLY from the
    #: declared answer arm's outcomes.
    "answer_accuracy": "k_free",
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


def _not_diagnostic(doc: Mapping[str, Any]) -> bool:
    """A diagnostic runs without gold, so an answer arm has no ground truth to
    score against; carrying one would be a declaration the run must silently
    ignore. (Arm-INTERNAL answer arms are refused separately in
    `_resolve_arms`: multi-arm priced execution is an HITL scope decision.)"""
    return doc.get("campaign") != "diagnostic"


def _comparison_only(doc: Mapping[str, Any]) -> bool:
    """`comparison.*` is consumable ONLY by a comparison campaign. A sweep may
    NOT carry the block in v1 -- sweep makes no claim and declares no knob
    axis, so a carried block would be a declaration the run must silently
    ignore. Same reasoning refuses it on characterizations, as before S8."""
    return doc.get("campaign") == "comparison"


def _arms_campaigns_only(doc: Mapping[str, Any]) -> bool:
    return doc.get("campaign") in ARMS_CAMPAIGNS


def _not_sweep(doc: Mapping[str, Any]) -> bool:
    """A sweep records outcomes and makes NO comparative claim (S8 rule 2 /
    D-4): a decision rule on a sweep is a claim path it must not have."""
    return doc.get("campaign") != "sweep"


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
    # THREE registrations, not one: the walker yields the object node itself as
    # well as its children (review finding 7).
    "scenario.projections": Consumer("S7"),
    "scenario.projections.declare": Consumer("S7"),
    "scenario.projections.readiness_timeout_s": Consumer("S7"),
    "scenario.fixture": Consumer("S5", _diagnostic_only),
    # FOUR registrations: `declared_paths` yields the object node itself plus
    # its three children (the S7/S8 lesson, enumerated at implementation time).
    "scenario.answer_arm": Consumer("S9", _not_diagnostic),
    "scenario.answer_arm.kind": Consumer("S9", _not_diagnostic),
    "scenario.answer_arm.answerer_model": Consumer("S9", _not_diagnostic),
    "scenario.answer_arm.max_queries": Consumer("S9", _not_diagnostic),
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
    # ONLY `arms` is registered for the arms array: `declared_paths` never
    # descends into arrays, so no `arms.*` path is derivable. Arm-internal
    # governance happens SOLELY in the per-arm resolution pass, which re-runs
    # every consumer/applicability check on a synthesized single-scenario
    # document.
    "arms": Consumer("S8", _arms_campaigns_only),
    "decision_rule": Consumer("S8", _not_sweep),
    "decision_rule.metric": Consumer("S3"),
    "decision_rule.direction": Consumer("S3"),
    "decision_rule.threshold": Consumer("S3"),
    "comparison": Consumer("S8", _comparison_only),
    "comparison.changed_knobs": Consumer("S8", _comparison_only),
    "comparison.metric": Consumer("S8", _comparison_only),
    "comparison.strata": Consumer("S8", _comparison_only),
    "comparison.ci_method": Consumer("S8", _comparison_only),
    "comparison.seed": Consumer("S8", _comparison_only),
    "comparison.resamples": Consumer("S8", _comparison_only),
    "comparison.min_n": Consumer("S8", _comparison_only),
    "budget": Consumer("S9"),
    #: Carried unconditionally: there is no priced-arm declaration in earp.v1,
    #: so any applicability predicate would be undecidable.
    "budget.estimated_usd": Consumer("S9"),
}


@dataclass(frozen=True)
class ResolvedAnswerArm:
    """The declared priced answer arm (S9). `answerer_model` is None exactly
    when the config left it to the `R2_ANSWERER_MODEL` env default -- legal
    only for claim-free runs (resolver-enforced), and the sidecar marks the
    resolved value `env-resolved`."""

    kind: str
    max_queries: int
    answerer_model: str | None = None


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
    #: S7 -- the declared projection matrix, applied by the runner via
    #: `Engine.configure_projections` BEFORE ingest. Empty means "no block".
    projections: tuple[DeclaredProjection, ...] = ()
    #: S7 -- bound on the `read.projections()` readiness poll.
    readiness_timeout_s: float = 30.0
    #: S9 -- the declared priced answer arm, or None.
    answer_arm: ResolvedAnswerArm | None = None


@dataclass(frozen=True)
class ResolvedArm:
    """One named arm, resolved through the full single-scenario machinery on a
    synthesized document. `scenario.config_sha256` is the synthesized-document
    hash -- a supplementary label, NEVER the run identity (the whole-document
    hash is; `make_run_id` keys on it and it covers both arms)."""

    name: str
    scenario: ResolvedScenario


@dataclass(frozen=True)
class ResolvedComparison:
    """The comparison contract, fixed before the first retrieval (S8 rule 2)."""

    metric: str
    ci_method: str
    seed: int
    resamples: int
    min_n: int
    changed_knobs: tuple[str, ...]
    strata: tuple[str, ...] = ()


@dataclass(frozen=True)
class ConfigResolution:
    """`blockers` is empty iff the config resolved: to `scenario` for
    single-scenario campaigns, or to `arms` (with `comparison` for the
    comparison kind) for arms campaigns -- never both."""

    blockers: tuple[Blocker, ...] = ()
    scenario: ResolvedScenario | None = None
    #: Non-fatal notes, e.g. paths carried for a later slice.
    notes: tuple[str, ...] = field(default_factory=tuple)
    #: S8 -- arms campaigns only. (name, resolved scenario) in declared order;
    #: for a comparison, arms[0] is control and arms[1] is treatment.
    arms: tuple[ResolvedArm, ...] = ()
    #: S8 -- comparison campaigns only; None for sweep.
    comparison: ResolvedComparison | None = None
    #: S8 -- arms campaigns only; a single-scenario campaign's rule lives on
    #: its ResolvedScenario, as before.
    decision_rule: DecisionRule | None = None


def schema_paths() -> tuple[str, ...]:
    """Every dotted path `earp.config.v1` declares."""
    assert_supported(_SCHEMA)
    return tuple(declared_paths(_SCHEMA))


def emits(
    name: str,
    *,
    evidence_recall_k: tuple[int, ...],
    has_negatives: bool,
    has_answer_arm: bool = False,
) -> bool:
    """Whether a campaign can emit the named metric.

    Grammar: `<metric>@<k>`. `@k` is REQUIRED for the three per-K names, since
    only `metrics.per_k` is K-keyed, and FORBIDDEN for the rest.

    The `k_free` branch dispatches PER METRIC (S9): `answer_accuracy` keys on
    the declared answer arm, NOT on `has_negatives` -- that coupling belongs to
    `abstention_rate` alone.
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
    if suffix != "":
        return False
    if base == "answer_accuracy":
        return has_answer_arm
    return has_negatives


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

    # S8: `scenario` and `arms` are mutually exclusive per kind. The schema
    # dropped `scenario` from its required list (an arms-only document must be
    # able to validate), so per-kind presence is enforced here. An `arms` key
    # on a single-scenario campaign is refused by the consumer loop below.
    if isinstance(campaign_raw, str) and campaign_raw in ARMS_CAMPAIGNS:
        if "scenario" in doc:
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_UNUSED_KEY,
                    (
                        f"a `{campaign_raw}` campaign declares per-arm scenarios "
                        f"under `arms`; a top-level `scenario` and `arms` are "
                        f"mutually exclusive"
                    ),
                    "scenario",
                )
            )
        if "arms" not in doc:
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_MISSING_KEY,
                    (
                        f"a `{campaign_raw}` campaign requires `arms` "
                        f"({'exactly 2' if campaign_raw == 'comparison' else '>= 2'} "
                        f"named arms, each with a full scenario)"
                    ),
                    "arms",
                )
            )
    elif isinstance(campaign_raw, str) and campaign_raw in SINGLE_SCENARIO_CAMPAIGNS:
        if "scenario" not in doc:
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_MISSING_KEY,
                    (
                        f"a `{campaign_raw}` campaign requires `scenario`; the schema "
                        f"keeps it optional only so arms-only documents can validate"
                    ),
                    "scenario",
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
                        f"({consumer.slice_id}) does not consume it from campaign "
                        f"`{doc.get('campaign')}`, so carrying it would be a "
                        f"declaration the run must silently ignore"
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

    # --- S9: the priced answer arm's collected couplings. -------------------
    answer_arm_doc = _mapping(scenario, "answer_arm")
    has_answer_arm = "answer_arm" in scenario
    if has_answer_arm and "budget" not in doc:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_MISSING_KEY,
                (
                    "`scenario.answer_arm` declares a priced arm, which requires "
                    "`budget.estimated_usd`: without a declared worst case the "
                    "cumulative D-3 projection cannot be computed"
                ),
                "budget",
            )
        )
    if has_answer_arm and "answerer_model" not in answer_arm_doc:
        # Explicit-in-config whenever a claim consumes answer_accuracy:
        # config_sha256 covers the document, so an env-defaulted model would
        # let two different-model runs share a sha and poison pairing/replay.
        claim_docs = (doc.get("decision_rule"), doc.get("comparison"))
        if any(
            isinstance(claim, dict) and claim.get("metric") == "answer_accuracy"
            for claim in claim_docs
        ):
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_MISSING_KEY,
                    (
                        "a decision_rule/comparison consumes `answer_accuracy`, so "
                        "`answerer_model` must be explicit in the config (config "
                        "identity: an env-defaulted model would let two "
                        "different-model runs share a config_sha256); the "
                        "R2_ANSWERER_MODEL env default is legal only for "
                        "claim-free runs"
                    ),
                    "scenario.answer_arm.answerer_model",
                )
            )
    resolved_answer_arm: ResolvedAnswerArm | None = None
    if has_answer_arm:
        arm_kind = answer_arm_doc.get("kind")
        arm_max_queries = answer_arm_doc.get("max_queries")
        arm_model = answer_arm_doc.get("answerer_model")
        if (
            arm_kind == "r2_identical_answerer"
            and isinstance(arm_max_queries, int)
            and not isinstance(arm_max_queries, bool)
            and arm_max_queries >= 1
        ):
            resolved_answer_arm = ResolvedAnswerArm(
                kind=arm_kind,
                max_queries=arm_max_queries,
                answerer_model=arm_model if isinstance(arm_model, str) and arm_model else None,
            )

    # --- S7: the projection matrix is validated BEFORE it is run. -----------
    projections_doc = _mapping(scenario, "projections")
    declare_raw = projections_doc.get("declare")
    declared_projections: list[DeclaredProjection] = []
    if isinstance(declare_raw, list):
        for index, item in enumerate(declare_raw):
            if not isinstance(item, dict):
                continue  # the walker already recorded the type defect
            item_path = f"scenario.projections.declare[{index}]"
            name = item.get("name")
            roles_raw = item.get("roles")
            roles = (
                tuple(role for role in roles_raw if isinstance(role, str))
                if isinstance(roles_raw, list)
                else ()
            )
            fts = item.get("fts") is True
            vector = item.get("vector") is True
            if not isinstance(name, str) or not name:
                # The walker cannot express minLength, so non-emptiness is a
                # resolver-collected error rather than a schema one.
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INVALID_VALUE,
                        "projection `name` must be a non-empty string",
                        f"{item_path}.name",
                    )
                )
            for flag, flag_name in ((fts, "fts"), (vector, "vector")):
                if flag and "searchable" not in roles:
                    # Mirror of the engine's own sub-target rule, which refuses
                    # typed: an FTS/vector sub-target hangs off `searchable`.
                    blockers.append(
                        _blocker(
                            BlockerCode.CONFIG_INVALID_VALUE,
                            f"`{flag_name}: true` requires the `searchable` role",
                            f"{item_path}.{flag_name}",
                        )
                    )
            if vector and not use_default_embedder:
                # With NO embedder the engine reports the dense sub-target
                # `unavailable` (0.8.22 Slice 21) and vector-dependent queries
                # refuse at query time -- the config declares a dense arm the
                # run can never exercise, so resolution refuses it up front.
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INVALID_VALUE,
                        (
                            "`vector: true` requires `scenario.engine."
                            "use_default_embedder: true`; without an embedder the "
                            "engine reports the dense sub-target `unavailable`, so "
                            "the declared dense arm can never be exercised -- an "
                            "honest config declares only what it measures"
                        ),
                        f"{item_path}.vector",
                    )
                )
            declared_projections.append(
                DeclaredProjection(
                    name=name if isinstance(name, str) else "",
                    roles=roles,
                    fts=fts,
                    vector=vector,
                )
            )

    readiness_timeout_raw = projections_doc.get("readiness_timeout_s")
    readiness_timeout_s = 30.0
    if (
        isinstance(readiness_timeout_raw, (int, float))
        and not isinstance(readiness_timeout_raw, bool)
        and 0.1 <= readiness_timeout_raw <= 300
    ):
        readiness_timeout_s = float(readiness_timeout_raw)

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
        projection_name = query.get("projection_name")
        if call == "Engine.search_projected_text" and isinstance(projection_name, str):
            # S7 matrix coherence (owned behaviour change): the scenario owns a
            # FRESH database, so a projection the config does not declare can
            # never exist -- pre-S7 such a config resolved and then died at run
            # time with InvalidFilterError on every run. The error moves to
            # resolution, naming the declared set.
            fts_bearing = sorted(
                p.name
                for p in declared_projections
                if p.fts and "searchable" in p.roles
            )
            if projection_name not in fts_bearing:
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INVALID_VALUE,
                        (
                            f"`Engine.search_projected_text` names projection "
                            f"`{projection_name}`, which `scenario.projections` does "
                            f"not declare as FTS-bearing (`fts: true` + role "
                            f"`searchable`); declared: {fts_bearing}"
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
        if not emits(
            metric,
            evidence_recall_k=ladder,
            has_negatives=has_negatives,
            has_answer_arm=has_answer_arm,
        ):
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

    if isinstance(campaign_raw, str) and campaign_raw in ARMS_CAMPAIGNS:
        resolved_arms = _resolve_arms(doc, blockers)
        resolved_comparison = (
            _resolve_comparison(doc, ladder, resolved_arms, blockers)
            if campaign_raw == "comparison"
            else None
        )
        if blockers:
            return ConfigResolution(blockers=tuple(blockers))
        return ConfigResolution(
            arms=resolved_arms,
            comparison=resolved_comparison,
            decision_rule=decision_rule,
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
            projections=tuple(declared_projections),
            readiness_timeout_s=readiness_timeout_s,
            answer_arm=resolved_answer_arm,
        )
    )


#: Walker array paths read `arms.[0]scenario...` (with a trailing dot on value
#: defects); resolver paths read `arms[0].scenario...`. One normal form, so
#: per-arm re-resolution can be deduplicated against whole-document walker
#: findings instead of reporting the same defect in two spellings.
_ARM_INDEX_RE = re.compile(r"\.\[(\d+)\]")


def _norm_path(path: str) -> str:
    return _ARM_INDEX_RE.sub(r"[\1].", path).rstrip(".")


def _resolve_arms(doc: Mapping[str, Any], blockers: list[Blocker]) -> tuple[ResolvedArm, ...]:
    """Resolve every arm as a synthesized single-scenario document.

    This re-runs the EXISTING full resolution machinery per arm -- every
    consumer/applicability check, S6a limit injection, S7 projection coherence
    -- so `scenario.fixture` inside an arm is caught exactly as it would be at
    top level. Shared sections (corpus/gold/metrics) are copied verbatim into
    the synthesized document; defects in them are reported once, at the parent
    level, and their per-arm duplicates are dropped.
    """
    arms_raw = doc.get("arms")
    if not isinstance(arms_raw, list):
        return ()

    campaign_raw = doc.get("campaign")
    if campaign_raw == "comparison" and len(arms_raw) != 2:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_INVALID_VALUE,
                (
                    f"a comparison requires exactly 2 arms (arms[0] is control, "
                    f"arms[1] is treatment); got {len(arms_raw)}"
                ),
                "arms",
            )
        )

    seen = {
        (b.code, _norm_path(str(b.detail["path"])))
        for b in blockers
        if "path" in b.detail
    }
    resolved: list[ResolvedArm] = []
    names: list[str] = []
    for index, arm in enumerate(arms_raw):
        if not isinstance(arm, dict):
            continue  # the walker already recorded the type defect
        name_raw = arm.get("name")
        name = name_raw if isinstance(name_raw, str) else ""
        if "name" in arm and not name:
            # Present but empty or mistyped; absence is the walker's finding.
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_INVALID_VALUE,
                    "arm `name` must be a non-empty string",
                    f"arms[{index}].name",
                )
            )
        if name:
            names.append(name)
        scenario_raw = arm.get("scenario")
        if not isinstance(scenario_raw, dict):
            continue  # the walker already recorded the missing/mistyped scenario
        if "answer_arm" in scenario_raw:
            # S9's money gate is single-scenario, deliberately: a per-arm
            # priced arm multiplies the priced call count by the arm count,
            # and commissioning that spend shape is an HITL scope decision.
            # Refused typed here -- the synthesized per-arm document reads as
            # a characterization, so the consumer registry cannot see it.
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_UNUSED_KEY,
                    (
                        "a priced `answer_arm` inside an arms campaign is not "
                        "consumable in v1; S9 delivers the single-scenario priced "
                        "path only, and multi-arm priced execution stays an HITL "
                        "scope decision"
                    ),
                    f"arms[{index}].scenario.answer_arm",
                )
            )
        synthesized: dict[str, Any] = {
            "schema_version": "earp.v1",
            # `characterization` is the single-scenario kind with this arm's
            # semantics: gold-measured retrieval, no fixture, no authored query
            # text -- so the diagnostic-only consumers refuse inside an arm
            # exactly as they would on a top-level characterization.
            "campaign": "characterization",
            "scenario": scenario_raw,
        }
        for key in ("corpus", "gold", "metrics"):
            if key in doc:
                synthesized[key] = doc[key]
        sub = resolve_config(synthesized)
        for sub_blocker in sub.blockers:
            path = sub_blocker.detail.get("path")
            if path is None:
                # e.g. a depth blocker: arm-specific, path-less; keep, labeled.
                blockers.append(
                    Blocker(
                        code=sub_blocker.code,
                        message=f"arms[{index}] (`{name}`): {sub_blocker.message}",
                        stage=sub_blocker.stage,
                        detail={**sub_blocker.detail, "arm": name},
                    )
                )
            elif str(path).startswith("scenario"):
                new_path = f"arms[{index}].{path}"
                if (sub_blocker.code, _norm_path(new_path)) in seen:
                    continue  # the whole-document walker already reported it
                blockers.append(
                    Blocker(
                        code=sub_blocker.code,
                        message=f"arms[{index}]: {sub_blocker.message}",
                        stage=sub_blocker.stage,
                        detail={**sub_blocker.detail, "path": new_path, "arm": name},
                    )
                )
            # else: a corpus/gold/metrics defect -- shared content, already
            # reported once at the parent level.
        if sub.scenario is not None and name:
            resolved.append(ResolvedArm(name=name, scenario=sub.scenario))

    duplicates = sorted({n for n in names if names.count(n) > 1})
    if duplicates:
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_INVALID_VALUE,
                f"arm names must be unique; duplicated: {duplicates}",
                "arms",
            )
        )
    return tuple(resolved)


#: Sentinel for the symmetric diff: an absent path never equals a present one.
_ABSENT = object()


def _knob_projection(scenario: ResolvedScenario) -> dict[str, Any]:
    """An arm's RESOLVED representation projected back to config paths.

    Resolver defaults are materialized (`limit` 10 via query_params injection,
    `use_default_embedder` false, `readiness_timeout_s` 30), so declaring a
    default explicitly in one arm and omitting it in the other is NOT a
    difference. Derived fields (`retrieval_mode`, `max_measurable_k`) are not
    diffable paths: a derived divergence attributes to its causal config path.
    """
    projection: dict[str, Any] = {
        "scenario.query.call": scenario.query_call,
        "scenario.engine.use_default_embedder": scenario.use_default_embedder,
        "scenario.projections.readiness_timeout_s": scenario.readiness_timeout_s,
    }
    for key, value in scenario.query_params.items():
        projection[f"scenario.query.{key}"] = value
    if scenario.projections:
        projection["scenario.projections.declare"] = tuple(
            (p.name, tuple(p.roles), p.fts, p.vector) for p in scenario.projections
        )
    return projection


def _resolve_comparison(
    doc: Mapping[str, Any],
    ladder: tuple[Any, ...],
    resolved_arms: tuple[ResolvedArm, ...],
    blockers: list[Blocker],
) -> ResolvedComparison | None:
    """Enforce S8 rules 1 and 2: the effect is defined before it is seen, and
    one-knob honesty is symmetric."""
    comparison_raw = doc.get("comparison")
    if not isinstance(comparison_raw, dict):
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_MISSING_KEY,
                (
                    "a comparison campaign requires the `comparison` block: metric, "
                    "ci_method, seed, resamples, and min_n are fixed before the "
                    "first retrieval"
                ),
                "comparison",
            )
        )
        return None

    complete = True
    for field_name in ("metric", "ci_method", "seed", "resamples", "min_n"):
        if field_name not in comparison_raw:
            complete = False
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_MISSING_KEY,
                    (
                        "required for comparison campaigns at resolution (the schema "
                        "keeps it optional only because the block is shared); the "
                        "effect is defined before it is seen"
                    ),
                    f"comparison.{field_name}",
                )
            )

    ci_method = comparison_raw.get("ci_method")
    if ci_method == "percentile_bootstrap":
        complete = False
        blockers.append(
            _blocker(
                BlockerCode.CONFIG_INVALID_VALUE,
                (
                    "v1 comparisons run exactly one method, `paired_bootstrap`; the "
                    "unpaired percentile bootstrap stays schema-legal for future "
                    "kinds and is refused here, deliberately"
                ),
                "comparison.ci_method",
            )
        )

    strata: tuple[str, ...] = ()
    strata_raw = comparison_raw.get("strata")
    if isinstance(strata_raw, list):
        unknown = [s for s in strata_raw if s not in STRATA_VOCABULARY]
        if unknown:
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_INVALID_VALUE,
                    (
                        f"v1 strata vocabulary is exactly "
                        f"{sorted(STRATA_VOCABULARY)}; got {unknown}"
                    ),
                    "comparison.strata",
                )
            )
        else:
            strata = tuple(str(s) for s in strata_raw)

    metric = comparison_raw.get("metric")
    if isinstance(metric, str):
        if not emits(metric, evidence_recall_k=ladder, has_negatives=True):
            complete = False
            blockers.append(
                _blocker(
                    BlockerCode.CONFIG_INVALID_VALUE,
                    (
                        f"comparison metric `{metric}` is not emittable by this "
                        f"campaign (ladder {list(ladder)}); a comparison on an "
                        f"unemittable metric would never produce a paired value"
                    ),
                    "comparison.metric",
                )
            )
        else:
            _base, _, suffix = metric.partition("@")
            if suffix.isdigit():
                for arm in resolved_arms:
                    depth_blocker = check_depth(
                        arm.scenario.retrieval_mode,
                        int(suffix),
                        arm.scenario.max_measurable_k,
                    )
                    if depth_blocker is not None:
                        complete = False
                        blockers.append(
                            _blocker(
                                BlockerCode.CONFIG_INVALID_VALUE,
                                f"arm `{arm.name}` cannot measure `{metric}`: "
                                f"{depth_blocker.message}",
                                "comparison.metric",
                            )
                        )

    changed_raw = comparison_raw.get("changed_knobs")
    changed = (
        tuple(k for k in changed_raw if isinstance(k, str))
        if isinstance(changed_raw, list)
        else ()
    )
    if len(resolved_arms) == 2 and isinstance(changed_raw, list):
        control, treatment = (
            _knob_projection(resolved_arms[0].scenario),
            _knob_projection(resolved_arms[1].scenario),
        )
        differing = sorted(
            path
            for path in set(control) | set(treatment)
            if control.get(path, _ABSENT) != treatment.get(path, _ABSENT)
        )
        for path in differing:
            if path not in changed:
                complete = False
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INVALID_VALUE,
                        (
                            f"the resolved arms differ at `{path}`, which "
                            f"`comparison.changed_knobs` does not declare; every "
                            f"differing path must be declared"
                        ),
                        "comparison.changed_knobs",
                    )
                )
        for path in changed:
            if path not in differing:
                complete = False
                blockers.append(
                    _blocker(
                        BlockerCode.CONFIG_INVALID_VALUE,
                        (
                            f"`comparison.changed_knobs` declares `{path}`, but the "
                            f"resolved arms do not differ there (defaults "
                            f"materialized: an omitted `limit` resolves to 10, "
                            f"`use_default_embedder` to false, `readiness_timeout_s` "
                            f"to 30); a declared knob that does not differ is as "
                            f"much a lie as an undeclared one that does"
                        ),
                        "comparison.changed_knobs",
                    )
                )

    seed = comparison_raw.get("seed")
    resamples = comparison_raw.get("resamples")
    min_n = comparison_raw.get("min_n")
    if not complete or not (
        isinstance(metric, str)
        and ci_method == "paired_bootstrap"
        and isinstance(seed, int)
        and not isinstance(seed, bool)
        and isinstance(resamples, int)
        and not isinstance(resamples, bool)
        and isinstance(min_n, int)
        and not isinstance(min_n, bool)
    ):
        return None
    return ResolvedComparison(
        metric=metric,
        ci_method=ci_method,
        seed=seed,
        resamples=resamples,
        min_n=min_n,
        changed_knobs=changed,
        strata=strata,
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
    "ARMS_CAMPAIGNS",
    "CALL_MODE",
    "CALL_PARAMS",
    "CONSUMER_REGISTRY",
    "INEXPRESSIBLE",
    "METRIC_NAMES",
    "SINGLE_SCENARIO_CAMPAIGNS",
    "STRATA_VOCABULARY",
    "ConfigResolution",
    "Consumer",
    "ResolvedAnswerArm",
    "ResolvedArm",
    "ResolvedComparison",
    "ResolvedScenario",
    "emits",
    "load_config",
    "resolve_config",
    "schema_paths",
]
