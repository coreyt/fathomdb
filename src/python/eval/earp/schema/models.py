"""EARP v1 lock artifact — frozen dataclasses and pinned vocabularies.

This module is a DECLARATION, not an implementation. It defines the contracts
that `dev/design/earp.md` states in prose, in a form tests can assert against:
the sidecar shapes, the `Witness` data structure, the blocker codes, and the
verdict tokens. Resolver, runner, metric, and writer logic live elsewhere and
are built against these types (see `dev/plans/earp-foundation.md`, S3-S5).

Every vocabulary here is CLOSED. A value outside one of these enums is a
configuration or implementation error, never a silently-accepted string --
`experiments/_lib.Record.verdict` is an untyped `str`, so without pinning the
tokens "blocked" could be spelled three different ways across three slices.

Python >= 3.10, so string enums are spelled `class X(str, Enum)` rather than
`StrEnum` (3.11+).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Mapping

SCHEMA_VERSION_CONFIG = "earp.v1"
SCHEMA_VERSION_RESULT = "earp.result.v1"
SCHEMA_VERSION_PER_QUERY = "earp.per-query.v1"


# --- campaign kinds ---------------------------------------------------------


class CampaignKind(str, Enum):
    """The five declared campaign kinds. Each has an owning slice; see the
    campaign-kind coverage table in `dev/plans/earp-foundation.md`."""

    CHARACTERIZATION = "characterization"
    COMPARISON = "comparison"
    SWEEP = "sweep"
    REPLAY = "replay"
    DIAGNOSTIC = "diagnostic"


# --- verdicts and outcomes --------------------------------------------------


class QueryClass(str, Enum):
    """The six gold query classes.

    Declaration order is the reference's discriminant order, NOT alphabetical:
    the reference keys per-class aggregates in a `BTreeMap<QueryClass, _>`,
    which iterates by discriminant, so any per-class output that means to match
    it must iterate in this order.
    """

    COMMITMENT = "commitment"
    ACTION = "action"
    EXACT_FACT = "exact_fact"
    PREFERENCE = "preference"
    EXPLORATORY = "exploratory"
    NEGATIVE = "negative"


class RunVerdict(str, Enum):
    """The run-level verdict written into `Record.verdict` and the index row.

    Distinct from a per-query outcome: a run can be COMPLETE while individual
    queries missed, and a run is BLOCKED when a precondition refused it before
    any measurement could be trusted.
    """

    COMPLETE = "complete"
    BLOCKED = "blocked"
    FAILED = "failed"


class QueryOutcome(str, Enum):
    """Per-query disposition. `SCORED` is the only value that carries metric
    numbers; the rest carry a reason and no score. None of them is ever folded
    into a recall denominator as an ordinary miss."""

    SCORED = "scored"
    SKIPPED = "skipped"
    BLOCKED = "blocked"
    ERROR = "error"


# --- witnesses --------------------------------------------------------------


class WitnessSource(str, Enum):
    """Where a witness was actually captured from. These are NOT
    interchangeable: `vector_dense_readiness` comes only from
    `read.projections()`, `vector_unsupported_kinds` only from the
    `ProjectionDelta` that `configure_projections` returns, and
    `dense_disabled` only from `open_report`. Conflating them was a defect in
    the pre-review design."""

    OPEN_REPORT = "open_report"
    READ_PROJECTIONS = "read_projections"
    PROJECTION_DELTA = "projection_delta"
    SEARCH_RESULT = "search_result"
    STORE_QUERY = "store_query"
    #: `Engine.write`. Its own category rather than `store_query`, which
    #: reads: the enum's docstring warns these are not interchangeable.
    WRITE_RECEIPT = "write_receipt"
    FILESYSTEM = "filesystem"
    #: S9 — the priced answer arm. One source, three named witnesses: the
    #: visible-skip witness (D-2), the cheap-validate witness, and the
    #: ledger-preflight witness (D-3).
    ANSWER_ARM = "answer_arm"


class WitnessStatus(str, Enum):
    OBSERVED = "observed"
    ABSENT = "absent"
    ERROR = "error"


@dataclass(frozen=True)
class Witness:
    """A named, typed observation captured from a concrete SDK call.

    The design makes a witness an admission gate -- a knob cannot be
    `supported` without one -- so it must be a real data structure rather than
    a word. `call_path` is the concrete SDK call that produced it (for example
    ``Engine.open_report``), never a symbolic label.
    """

    name: str
    source: WitnessSource
    call_path: str
    status: WitnessStatus
    value: Any = None
    captured_at: str | None = None


# --- blockers ---------------------------------------------------------------


class BlockerCode(str, Enum):
    """Typed refusals. Each corresponds to a specific, verified precondition;
    none is ever converted into an empty retrieval result or a zero score."""

    #: Configured corpus/gold root is absent (the data is gitignored, so this
    #: is the normal worktree case).
    CORPUS_ROOT_ABSENT = "corpus_root_absent"
    GOLD_MISSING = "gold_missing"
    #: Pinned SHA-256 of the gold file does not match.
    GOLD_HASH_MISMATCH = "gold_hash_mismatch"
    #: Gold bytes are not valid JSON, or do not conform to the GoldSet shape --
    #: including an unknown `query_class`, `necessity`, or `query_origin`,
    #: each of which the Rust reference treats as a hard error.
    GOLD_MALFORMED = "gold_malformed"
    #: The corpus snapshot is absent, unreadable, or carries no `corpus_hash`,
    #: so the gold's corpus cross-check cannot be performed at all.
    SNAPSHOT_UNREADABLE = "snapshot_unreadable"
    #: Gold `corpus_hash` does not equal the frozen snapshot hash.
    GOLD_CORPUS_MISMATCH = "gold_corpus_mismatch"
    #: Cached gold is `ir-c-reused-v1`; the generator emits v2 (D-6.3).
    GOLD_STALE_QRELS_VERSION = "gold_stale_qrels_version"
    #: Requested K exceeds what the declared retrieval mode can measure (D-5).
    METRIC_NOT_MEASURABLE = "metric_not_measurable"
    #: `vector_dense_readiness` stayed "embedding" past the declared timeout.
    DENSE_READINESS_TIMEOUT = "dense_readiness_timeout"
    #: `open_report.dense_disabled` -- the vector-equivalence degraded open.
    DENSE_DISABLED = "dense_disabled"
    #: Kinds the vector writer can never embed, from the projection delta.
    VECTOR_UNSUPPORTED_KINDS = "vector_unsupported_kinds"
    #: The open fetched embedder weights rather than using a local cache.
    EMBEDDER_FETCHED = "embedder_fetched"
    #: Projected cumulative spend exceeds the D-3 authorization.
    BUDGET_EXCEEDED = "budget_exceeded"
    #: A run directory already exists for this run_id with a differing sidecar.
    RUN_ID_COLLISION = "run_id_collision"

    # -- S3 config resolution. Distinct from the data/run codes above: these
    # fire before anything is opened, read, or measured.
    #: A key the schema does not define.
    CONFIG_UNKNOWN_KEY = "config_unknown_key"
    #: A required key is absent.
    CONFIG_MISSING_KEY = "config_missing_key"
    #: A defined key carrying an out-of-domain value.
    CONFIG_INVALID_VALUE = "config_invalid_value"
    #: A declared knob the named search call does not accept.
    CONFIG_INAPPLICABLE_KNOB = "config_inapplicable_knob"
    #: A declared key inapplicable to THIS config -- not "never consumed".
    CONFIG_UNUSED_KEY = "config_unused_key"
    #: A campaign kind `earp.v1` structurally cannot represent.
    CONFIG_CAMPAIGN_INEXPRESSIBLE = "config_campaign_inexpressible"

    # -- S5 fixture. Distinct from CORPUS_ROOT_ABSENT, which is about the
    # gitignored corpus/gold tree: a diagnostic declares neither, so reusing
    # that code would conflate two different refusals.
    #: The declared fixture file is absent.
    FIXTURE_MISSING = "fixture_missing"
    #: The fixture parses but violates a precondition -- a missing or
    #: non-string body, a missing or duplicate logical_id, no source_id.
    FIXTURE_INVALID = "fixture_invalid"


@dataclass(frozen=True)
class Blocker:
    code: BlockerCode
    message: str
    stage: str
    detail: dict[str, Any] = field(default_factory=dict)


# --- projections (S7) -------------------------------------------------------


@dataclass(frozen=True)
class DeclaredProjection:
    """One `scenario.projections.declare[]` entry, resolved.

    The config-facing subset of the SDK's `ProjectionSpec`: tokenizer/embedder
    identities and `source` segments are deliberately inexpressible in earp.v1
    (each is a catalog UNSUPPORTED entry with its reason)."""

    name: str
    roles: tuple[str, ...]
    fts: bool = False
    vector: bool = False


@dataclass(frozen=True)
class ProjectionWitnesses:
    """The three projection-state signals, each under its OWN source's name.

    They come from three different APIs and are not interchangeable: a poll
    result never stands in for the delta, and the delta never stands in for the
    open report. `configure_delta` and `readiness` are None -- and OMITTED from
    the serialized value -- when the scenario declared no projections, so a
    sidecar reader can distinguish "not declared" from "not captured".
    """

    #: From `open_report()` at open time (+ the refusal count, an Engine method
    #: read by the runner at capture time): dense_disabled,
    #: dense_disabled_reason, query_backend, refusal_count.
    open_report: Mapping[str, Any]
    #: The `ProjectionDelta` exactly as `configure_projections` returned it,
    #: including the NON-DISJOINT built/deferred lists -- "in built" must never
    #: be read as "fully built"; the dense portion keys on `deferred`.
    configure_delta: Mapping[str, Any] | None = None
    #: name -> ready|embedding|not_declared, from polling `read.projections()`.
    readiness: Mapping[str, str] | None = None

    @staticmethod
    def readiness_state(*, vector: bool, vector_dense_readiness: str | None) -> str:
        """Derive the reported readiness from `(spec.vector, readiness)`.

        `None` is never reported bare: it means "no vector sub-target declared
        on this spec", and the disambiguator is the spec's own round-tripping
        `vector` flag. `None` WITH `vector=True` is outside the engine's
        binding-enforced contract (`read.py`), so it is an assertion failure,
        never a silently-recorded value.
        """
        if not vector:
            return "not_declared"
        if vector_dense_readiness not in ("ready", "embedding"):
            raise AssertionError(
                f"contract violation: a vector spec must read ready|embedding from "
                f"read.projections, got {vector_dense_readiness!r}"
            )
        return vector_dense_readiness

    def as_value(self) -> dict[str, Any]:
        """The sidecar mapping. Absent signals are OMITTED, never empty."""
        value: dict[str, Any] = {"open_report": dict(self.open_report)}
        if self.configure_delta is not None:
            value["configure_delta"] = dict(self.configure_delta)
        if self.readiness is not None:
            value["readiness"] = dict(self.readiness)
        return value


# --- knob catalog -----------------------------------------------------------


class KnobClass(str, Enum):
    SEMANTIC = "semantic"
    INDEXING = "indexing"
    RUNTIME = "runtime"
    OBSERVABILITY = "observability"
    HELD_CONSTANT = "held_constant"
    UNSUPPORTED = "unsupported"


@dataclass(frozen=True)
class KnobEntry:
    """One catalog entry.

    The catalog is keyed on whether a concrete SDK call path EXISTS -- not on
    whether the knob happens to be an `EngineConfig` field. Those are different
    questions, and conflating them writes a false statement about the SDK into
    a test: `slow_threshold_ms` is an `EngineConfig` field that `Engine.open`
    never forwards, yet it has its own live path and is therefore supported.

    `call_path` is None only when `classification` is UNSUPPORTED or
    HELD_CONSTANT; a supported knob without a call path is invalid.
    """

    name: str
    classification: KnobClass
    call_path: str | None
    witness: str | None
    reason: str


# --- metric eligibility -----------------------------------------------------


class MetricStatus(str, Enum):
    """A metric is EMITTED with a value, or explicitly NOT_APPLICABLE with a
    reason. It is never reported as zero because it could not be computed."""

    EMITTED = "emitted"
    NOT_APPLICABLE = "not_applicable"


class RetrievalMode(str, Enum):
    """Mode determines cost and semantics, no longer depth (S6a, the D-5
    successor): @K is measurable exactly when K <= the run's public result
    `limit`, for every mode, with the limit validated to the engine's own
    1..=100 window and recorded with every number."""

    FTS_ONLY = "fts_only"
    VECTOR_ONLY = "vector_only"
    HYBRID = "hybrid"


#: Mirrors of the engine's public result-limit window (0.8.22 Slice 18):
#: `DEFAULT_SEARCH_RESULT_LIMIT` / `MAX_SEARCH_RESULT_LIMIT`, enforced by
#: `validate_search_result_limit` as a typed REFUSAL outside 1..=100, never a
#: clamp. The S2 drift detector guards `ir_eval.rs`, NOT `lib.rs`, so these
#: mirrors are pinned by their own binding-present guard instead
#: (`test_limit_adoption.py`): every search verb's `limit` default must equal
#: ENGINE_DEFAULT_RESULT_LIMIT, and the window is pinned empirically --
#: limit=100 accepted, limit=101 refused.
ENGINE_DEFAULT_RESULT_LIMIT = 10
ENGINE_MAX_RESULT_LIMIT = 100


@dataclass(frozen=True)
class MetricValue:
    """A metric slot that can honestly be empty.

    `value` is None exactly when `status` is NOT_APPLICABLE -- **enforced**, not
    merely documented. This is how `supporting_coverage` arrives from the
    reference, which returns `Option<f64>` and serialises the unavailable case
    as JSON `null`.

    The enforcement matters precisely because ``MetricValue(EMITTED, 0.0)`` is a
    legitimate state: supporting coverage is `Some(0.0)` when supporting units
    exist and none were retrieved. Distinguishing that from "no supporting units
    at all" is the whole point of the upstream fix, so the invariant cannot be
    left to prose.

    Integer denominators (`required_n`, `required_hits`, `supporting_query_n`,
    `n`) do NOT live here -- `value` is a float. They live in the aggregates.
    """

    status: MetricStatus
    value: float | None = None
    reason: str | None = None

    def __post_init__(self) -> None:
        if self.status is MetricStatus.NOT_APPLICABLE:
            if self.value is not None:
                raise ValueError("a not_applicable metric must carry value=None")
            if not self.reason:
                raise ValueError("a not_applicable metric must carry a reason")
        elif self.value is None:
            raise ValueError("an emitted metric must carry a value")


# --- identities -------------------------------------------------------------


@dataclass(frozen=True)
class CorpusIdentity:
    """The snapshot is the GoldSet's identity; the acquisition manifest is
    raw-corpus provenance (D-6.2). Both are recorded; they are not
    interchangeable and must never be conflated into one 'corpus hash'."""

    snapshot_path: str
    snapshot_sha256: str
    manifest_path: str | None
    manifest_sha256: str | None
    data_root: str | None


@dataclass(frozen=True)
class GoldIdentity:
    """Pinned by content hash, cross-checked against the corpus, and version
    checked -- `ir-c-reused-v1` is refused as stale (D-6.1, D-6.3)."""

    path: str
    sha256: str
    corpus_hash: str
    qrels_version: str
    query_count: int
    #: Reuse-tier, document/body-level evidence gold -- not fresh
    #: FathomDB-specific human adjudication (D-6.4).
    tier: str = "ir-c-reuse"


# --- decision rule ----------------------------------------------------------


class Direction(str, Enum):
    GREATER = "greater"
    LESS = "less"


@dataclass(frozen=True)
class DecisionRule:
    """Predeclared before the run and persisted into the resolved config and
    the sidecar, so a threshold can never be chosen after seeing the result.

    Thresholds differ per experiment (D-4); there is no global table. A
    campaign with no declared rule may report metrics but may not claim one
    configuration is better than another.
    """

    metric: str
    direction: Direction
    threshold: float


# --- cost -------------------------------------------------------------------


@dataclass(frozen=True)
class CostLedger:
    """$5.00 is pre-authorized CUMULATIVELY across all priced runs (D-3), so
    the ceiling is enforced, not merely recorded: the preflight sums `cost_usd`
    across `experiments/index.jsonl`, adds `estimated_usd`, and refuses with
    `BUDGET_EXCEEDED` when the projection exceeds `authorized_usd`."""

    authorized_usd: float
    cumulative_spent_usd: float
    estimated_usd: float
    actual_usd: float | None = None


__all__ = [
    "ENGINE_DEFAULT_RESULT_LIMIT",
    "ENGINE_MAX_RESULT_LIMIT",
    "SCHEMA_VERSION_CONFIG",
    "SCHEMA_VERSION_PER_QUERY",
    "SCHEMA_VERSION_RESULT",
    "Blocker",
    "BlockerCode",
    "CampaignKind",
    "CorpusIdentity",
    "CostLedger",
    "DecisionRule",
    "DeclaredProjection",
    "Direction",
    "GoldIdentity",
    "KnobClass",
    "KnobEntry",
    "MetricStatus",
    "MetricValue",
    "ProjectionWitnesses",
    "QueryClass",
    "QueryOutcome",
    "RetrievalMode",
    "RunVerdict",
    "Witness",
    "WitnessSource",
    "WitnessStatus",
]
