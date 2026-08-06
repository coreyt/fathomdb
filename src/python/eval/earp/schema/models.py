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
from typing import Any

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
    FILESYSTEM = "filesystem"


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


@dataclass(frozen=True)
class Blocker:
    code: BlockerCode
    message: str
    stage: str
    detail: dict[str, Any] = field(default_factory=dict)


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
    """Depth measurability is mode-dependent (D-5): the production rerank floor
    bounds only the vector path, so FTS-only admits @20/@50 while vector and
    hybrid do not, until the commissioned fanout control lands."""

    FTS_ONLY = "fts_only"
    VECTOR_ONLY = "vector_only"
    HYBRID = "hybrid"


#: Deepest K each mode can honestly measure today. `None` = unbounded.
MAX_MEASURABLE_K: dict[RetrievalMode, int | None] = {
    RetrievalMode.FTS_ONLY: None,
    RetrievalMode.VECTOR_ONLY: 10,
    RetrievalMode.HYBRID: 10,
}

#: `SEARCH_RERANK_LIMIT` in the engine. Recorded with every number, and named
#: in the `METRIC_NOT_MEASURABLE` blocker so the refusal is self-explaining.
PRODUCTION_RERANK_LIMIT = 10


@dataclass(frozen=True)
class MetricValue:
    """A metric slot that can honestly be empty.

    `value` is None exactly when `status` is NOT_APPLICABLE. This is how
    `supporting_coverage` arrives from the reference implementation, which
    returns `Option<f64>` and serialises the unavailable case as JSON `null`.
    """

    status: MetricStatus
    value: float | None = None
    reason: str | None = None


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
    "MAX_MEASURABLE_K",
    "PRODUCTION_RERANK_LIMIT",
    "SCHEMA_VERSION_CONFIG",
    "SCHEMA_VERSION_PER_QUERY",
    "SCHEMA_VERSION_RESULT",
    "Blocker",
    "BlockerCode",
    "CampaignKind",
    "CorpusIdentity",
    "CostLedger",
    "DecisionRule",
    "Direction",
    "GoldIdentity",
    "KnobClass",
    "KnobEntry",
    "MetricStatus",
    "MetricValue",
    "QueryOutcome",
    "RetrievalMode",
    "RunVerdict",
    "Witness",
    "WitnessSource",
    "WitnessStatus",
]
