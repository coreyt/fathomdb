"""S8 comparison and sweep tests — written RED, before `eval.earp.comparison`
and the resolver's arms support exist.

Resolver and pairing tests are pure (no SDK, no engine). End-to-end tests run
the real ingest machinery against a tiny corpus-shaped fixture with per-arm
`retrieve_override` callables supplying deterministic synthetic rankings, so
two-arm statistics are exercised at zero engine-search cost; they skip visibly
when the native binding is absent.
"""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp._experiments import lib as _lib
from eval.earp.config import resolve_config
from eval.earp.schema.models import Blocker, BlockerCode, RunVerdict

try:
    import fathomdb._fathomdb  # noqa: F401

    _HAS_BINDING = True
except ImportError:
    _HAS_BINDING = False

requires_binding = pytest.mark.skipif(not _HAS_BINDING, reason="native binding not built")

SHA = "a" * 64
TS = datetime(2026, 8, 7, 12, 0, tzinfo=timezone.utc)


def _codes(result: Any) -> set[BlockerCode]:
    return {b.code for b in result.blockers}


def _messages(result: Any) -> str:
    return "\n".join(b.message for b in result.blockers)


def _arm_scenario(**query: Any) -> dict[str, Any]:
    return {
        "engine": {"use_default_embedder": False},
        "query": {"call": "Engine.search_text_only", **query},
    }


def _doc(**over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.v1",
        "campaign": "comparison",
        "corpus": {"snapshot": "tests/corpus/snapshot.json", "data_root": "data/corpus-data"},
        "gold": {
            "path": "d/all.gold.json",
            "sha256": SHA,
            "corpus_hash": SHA,
            "qrels_version": "ir-c-reused-v2",
        },
        "metrics": {"evidence_recall_k": [5, 10]},
        "arms": [
            {"name": "control", "scenario": _arm_scenario()},
            {"name": "treatment", "scenario": _arm_scenario(limit=20)},
        ],
        "comparison": {
            "changed_knobs": ["scenario.query.limit"],
            "metric": "strict_evidence_recall@10",
            "ci_method": "paired_bootstrap",
            "seed": 42,
            "resamples": 200,
            "min_n": 1,
        },
    }
    for key, value in over.items():
        if value is None:
            doc.pop(key, None)
        else:
            doc[key] = value
    return doc


# --- resolver: arms/scenario mutual exclusion by kind ------------------------


def test_two_arm_comparison_resolves() -> None:
    result = resolve_config(_doc())
    assert result.blockers == ()
    assert result.scenario is None
    assert [arm.name for arm in result.arms] == ["control", "treatment"]
    assert result.comparison is not None
    assert result.comparison.metric == "strict_evidence_recall@10"
    assert result.comparison.ci_method == "paired_bootstrap"
    assert result.comparison.seed == 42
    assert result.comparison.resamples == 200
    assert result.comparison.min_n == 1
    assert result.comparison.changed_knobs == ("scenario.query.limit",)


def test_comparison_no_longer_inexpressible() -> None:
    result = resolve_config(_doc())
    assert BlockerCode.CONFIG_CAMPAIGN_INEXPRESSIBLE not in _codes(result)


def test_arm_resolutions_are_supplementary_labels_not_the_identity() -> None:
    """Whole-document config_sha256 is the run identity; per-arm synthesized
    hashes exist, differ per arm, and never equal the whole-doc hash."""
    doc = _doc()
    result = resolve_config(doc)
    whole = _lib.config_sha256(dict(doc))
    arm_hashes = {arm.scenario.config_sha256 for arm in result.arms}
    assert len(arm_hashes) == 2
    assert whole not in arm_hashes


def test_comparison_with_top_level_scenario_is_refused() -> None:
    doc = _doc()
    doc["scenario"] = _arm_scenario()
    result = resolve_config(doc)
    assert result.blockers != ()
    assert "scenario" in _messages(result)


def test_comparison_without_arms_is_refused_as_missing_key() -> None:
    doc = _doc(arms=None)
    doc["scenario"] = _arm_scenario()
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any(b.detail.get("path") == "arms" for b in result.blockers)


def test_characterization_with_arms_is_refused() -> None:
    doc = _doc(campaign="characterization", comparison=None)
    doc["scenario"] = _arm_scenario()
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNUSED_KEY in _codes(result)
    assert any(b.detail.get("path") == "arms" for b in result.blockers)


def test_single_scenario_campaign_still_requires_scenario() -> None:
    """`scenario` left the schema's required list so arms docs can validate;
    per-kind presence is the resolver's."""
    result = resolve_config(
        {
            "schema_version": "earp.v1",
            "campaign": "diagnostic",
        }
    )
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any(b.detail.get("path") == "scenario" for b in result.blockers)


# --- resolver: arm counts and names ------------------------------------------


def test_comparison_requires_exactly_two_arms() -> None:
    doc = _doc()
    doc["arms"] = doc["arms"] + [{"name": "third", "scenario": _arm_scenario(limit=30)}]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert "exactly 2" in _messages(result)


def test_sweep_admits_more_than_two_arms() -> None:
    doc = _doc(campaign="sweep", comparison=None)
    doc["arms"] = doc["arms"] + [{"name": "third", "scenario": _arm_scenario(limit=30)}]
    result = resolve_config(doc)
    assert result.blockers == ()
    assert len(result.arms) == 3
    assert result.comparison is None


def test_one_arm_is_refused_by_the_schema() -> None:
    doc = _doc()
    doc["arms"] = doc["arms"][:1]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_duplicate_arm_names_are_refused() -> None:
    doc = _doc()
    doc["arms"][1]["name"] = "control"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert "control" in _messages(result)


def test_empty_arm_name_is_refused() -> None:
    doc = _doc()
    doc["arms"][0]["name"] = ""
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


# --- resolver: symmetric changed_knobs ----------------------------------------


def test_undeclared_difference_is_refused_naming_the_path() -> None:
    doc = _doc()
    doc["arms"][1]["scenario"]["engine"]["use_default_embedder"] = True
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert "scenario.engine.use_default_embedder" in _messages(result)


def test_declared_knob_that_does_not_differ_is_refused() -> None:
    """The canonical resolved-layer case: one arm declares `limit: 10`, the
    other omits it. The raw docs differ; the resolved effect is identical
    (absence means the engine default of 10), so the declared knob does NOT
    differ and the declaration is a lie."""
    doc = _doc()
    doc["arms"][0]["scenario"] = _arm_scenario(limit=10)
    doc["arms"][1]["scenario"] = _arm_scenario()
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert "scenario.query.limit" in _messages(result)
    assert "does not differ" in _messages(result)


def test_derived_mode_divergence_attributes_to_its_config_path() -> None:
    """retrieval_mode is derived, never a diff path: arms differing only at
    `use_default_embedder` (which flips Engine.search fts_only->hybrid) must
    name `scenario.engine.use_default_embedder`, not `retrieval_mode`."""
    doc = _doc()
    doc["arms"][0]["scenario"] = {
        "engine": {"use_default_embedder": False},
        "query": {"call": "Engine.search"},
    }
    doc["arms"][1]["scenario"] = {
        "engine": {"use_default_embedder": True},
        "query": {"call": "Engine.search"},
    }
    doc["comparison"]["changed_knobs"] = ["scenario.query.limit"]
    result = resolve_config(doc)
    messages = _messages(result)
    assert "scenario.engine.use_default_embedder" in messages
    assert "retrieval_mode" not in messages


# --- resolver: the fixed statistics tuple -------------------------------------


@pytest.mark.parametrize("field", ["metric", "ci_method", "seed", "resamples", "min_n"])
def test_comparison_requires_every_stats_field_at_resolution(field: str) -> None:
    """The schema keeps them optional (the block is shared); a comparison
    campaign requires them all BEFORE the first retrieval."""
    doc = _doc()
    del doc["comparison"][field]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any(b.detail.get("path") == f"comparison.{field}" for b in result.blockers)


def test_comparison_without_the_comparison_block_is_refused() -> None:
    result = resolve_config(_doc(comparison=None))
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any(b.detail.get("path") == "comparison" for b in result.blockers)


def test_percentile_bootstrap_is_refused_for_comparison() -> None:
    """Schema-legal, resolver-refused: one method for v1, deliberately."""
    doc = _doc()
    doc["comparison"]["ci_method"] = "percentile_bootstrap"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert any(b.detail.get("path") == "comparison.ci_method" for b in result.blockers)


def test_strata_vocabulary_is_query_class_only() -> None:
    doc = _doc()
    doc["comparison"]["strata"] = ["query_class"]
    assert resolve_config(doc).blockers == ()
    doc["comparison"]["strata"] = ["source"]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert any(b.detail.get("path") == "comparison.strata" for b in result.blockers)


# --- resolver: comparison.metric eligibility ----------------------------------


def test_metric_outside_the_ladder_is_refused() -> None:
    doc = _doc()
    doc["comparison"]["metric"] = "strict_evidence_recall@50"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert any(b.detail.get("path") == "comparison.metric" for b in result.blockers)


def test_per_k_metric_without_a_k_is_refused() -> None:
    doc = _doc()
    doc["comparison"]["metric"] = "strict_evidence_recall"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_abstention_rate_is_an_eligible_comparison_metric() -> None:
    doc = _doc()
    doc["comparison"]["metric"] = "abstention_rate"
    assert resolve_config(doc).blockers == ()


def test_arm_limit_shallower_than_the_ladder_is_refused_per_arm() -> None:
    """Per-arm resolution re-runs the depth rule: a treatment limit of 5 under
    a (5, 10) ladder refuses @10 for that arm."""
    doc = _doc()
    doc["arms"][1]["scenario"] = _arm_scenario(limit=5)
    result = resolve_config(doc)
    assert BlockerCode.METRIC_NOT_MEASURABLE in _codes(result)
    assert "treatment" in _messages(result)


# --- resolver: per-arm governance re-runs everything ---------------------------


def test_arm_internal_diagnostic_only_key_is_refused_with_an_indexed_path() -> None:
    doc = _doc()
    doc["arms"][1]["scenario"]["fixture"] = "tests/earp/fixtures/nope.jsonl"
    doc["comparison"]["changed_knobs"] = ["scenario.query.limit"]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNUSED_KEY in _codes(result)
    assert "arms[1]" in _messages(result)


def test_arm_internal_inapplicable_knob_is_refused() -> None:
    doc = _doc()
    doc["arms"][0]["scenario"]["query"]["rerank_depth"] = 5
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INAPPLICABLE_KNOB in _codes(result)
    assert "arms[0]" in _messages(result)


def test_walker_reports_arm_internal_shape_defects_with_indexed_paths() -> None:
    doc = _doc()
    doc["arms"][1]["scenario"]["query"]["limit"] = 101
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert "arms" in _messages(result) and "[1]" in _messages(result)


# --- resolver: sweep makes no claim -------------------------------------------


def test_sweep_resolves_without_the_stats_tuple() -> None:
    result = resolve_config(_doc(campaign="sweep", comparison=None))
    assert result.blockers == ()
    assert result.comparison is None


def test_sweep_forbids_the_comparison_block() -> None:
    result = resolve_config(_doc(campaign="sweep"))
    assert BlockerCode.CONFIG_UNUSED_KEY in _codes(result)
    assert any(b.detail.get("path") == "comparison" for b in result.blockers)


def test_sweep_forbids_a_decision_rule() -> None:
    doc = _doc(campaign="sweep", comparison=None)
    doc["decision_rule"] = {
        "metric": "strict_evidence_recall@10",
        "direction": "greater",
        "threshold": 0.4,
    }
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNUSED_KEY in _codes(result)
    assert any(b.detail.get("path") == "decision_rule" for b in result.blockers)


# --- pairing: both-scored intersection, exclusions by reason -------------------


def _row(qid: str, k: int, **over: Any) -> dict[str, Any]:
    row: dict[str, Any] = {
        "schema_version": "earp.per-query.v1",
        "query_id": qid,
        "query_class": over.pop("query_class", "exact_fact"),
        "k": k,
        "outcome": "scored",
        "strict": 1.0,
        "graded": 1.0,
        "required_n": 1,
        "required_hits": 1,
    }
    row.update(over)
    return row


def test_pairing_is_the_valued_in_both_arms_intersection() -> None:
    from eval.earp.comparison import pair_rows  # noqa: PLC0415

    query_ids = ["q1", "qn", "q3"]
    control = [
        _row("q1", 10, strict=0.0),
        _row("qn", 10, query_class="negative", strict=None, graded=None, abstained=True),
        _row("q3", 10, strict=1.0),
    ]
    treatment = [
        _row("q1", 10, strict=1.0),
        _row("qn", 10, query_class="negative", strict=None, graded=None, abstained=True),
        {
            "schema_version": "earp.per-query.v1",
            "query_id": "q3",
            "k": 10,
            "outcome": "error",
            "reason": "RuntimeError: boom",
        },
    ]
    paired = pair_rows("strict_evidence_recall@10", query_ids, control, treatment)
    assert paired.deltas == (1.0,)
    assert dict(paired.exclusions) == {"metric_inapplicable": 1, "error": 1}
    assert len(paired.deltas) + sum(paired.exclusions.values()) == len(query_ids)


def test_negatives_enter_only_an_abstention_comparison() -> None:
    from eval.earp.comparison import pair_rows  # noqa: PLC0415

    query_ids = ["q1", "qn"]
    control = [
        _row("q1", 10, strict=0.0),
        _row("qn", 10, query_class="negative", strict=None, graded=None, abstained=True),
    ]
    treatment = [
        _row("q1", 10, strict=1.0),
        _row("qn", 10, query_class="negative", strict=None, graded=None, abstained=False),
    ]
    paired = pair_rows("abstention_rate", query_ids, control, treatment)
    assert paired.deltas == (-1.0,)
    assert dict(paired.exclusions) == {"metric_inapplicable": 1}


# --- end to end ---------------------------------------------------------------

DOCS = [
    {"doc_id": "d1", "body": "the deal sheet is missing for March", "source_type": "email"},
    {"doc_id": "d2", "body": "parking arrangements for the annual meeting", "source_type": "note"},
    {"doc_id": "d3", "body": "quarterly revenue rose after the deal closed", "source_type": "article"},
]

Q1, Q2, Q3, QN = "deal sheet", "parking annual meeting", "quarterly revenue", "zzznothingzzz"


def _bed(tmp_path: Path) -> dict[str, Any]:
    raw = tmp_path / "raw"
    raw.mkdir(exist_ok=True)
    shard = raw / "synthetic_notes.jsonl"
    shard.write_text("".join(json.dumps(d) + "\n" for d in DOCS), encoding="utf-8")
    digest = hashlib.sha256(shard.read_bytes()).hexdigest()
    snapshot = tmp_path / "snapshot.json"
    snapshot.write_text(
        json.dumps(
            {
                "corpus_hash": "c" * 64,
                "total_docs": len(DOCS),
                "per_source_sha256": [
                    {"source": "synthetic_notes", "sha256": digest, "doc_count": len(DOCS)}
                ],
            }
        ),
        encoding="utf-8",
    )
    gold = tmp_path / "gold.json"
    queries = [
        {
            "query_id": "q1",
            "query": Q1,
            "query_class": "exact_fact",
            "required_evidence": [
                {"evidence_id": "q1#e0", "doc_id": "d1", "necessity": "required"}
            ],
            "expected_top_k_doc_ids": ["d1"],
        },
        {
            "query_id": "q2",
            "query": Q2,
            "query_class": "action",
            "required_evidence": [
                {"evidence_id": "q2#e0", "doc_id": "d2", "necessity": "required"}
            ],
            "expected_top_k_doc_ids": ["d2"],
        },
        {
            "query_id": "q3",
            "query": Q3,
            "query_class": "exact_fact",
            "required_evidence": [
                {"evidence_id": "q3#e0", "doc_id": "d3", "necessity": "required"}
            ],
            "expected_top_k_doc_ids": ["d3"],
        },
        {
            "query_id": "qn",
            "query": QN,
            "query_class": "negative",
            "required_evidence": [],
            "expected_top_k_doc_ids": [],
        },
    ]
    gold.write_text(
        json.dumps(
            {"corpus_hash": "c" * 64, "qrels_version": "ir-c-reused-v2", "queries": queries}
        ),
        encoding="utf-8",
    )
    return {
        "snapshot": snapshot,
        "gold": gold,
        "gold_sha256": hashlib.sha256(gold.read_bytes()).hexdigest(),
    }


def _e2e_doc(tmp_path: Path, bed: dict[str, Any], **over: Any) -> dict[str, Any]:
    return _doc(
        corpus={"snapshot": str(bed["snapshot"]), "data_root": str(tmp_path)},
        gold={
            "path": str(bed["gold"]),
            "sha256": bed["gold_sha256"],
            "corpus_hash": "c" * 64,
            "qrels_version": "ir-c-reused-v2",
        },
        **over,
    )


class _Id:
    def __init__(self, value: str) -> None:
        self.space = "logical"
        self.value = value


class _Hit:
    def __init__(self, value: str) -> None:
        self.id = _Id(value)


class _SearchResult:
    def __init__(self, ids: list[str]) -> None:
        self.results = [_Hit(i) for i in ids]


def _override(hits: dict[str, list[str]]) -> Any:
    def call(query_text: str) -> _SearchResult:
        return _SearchResult(hits.get(query_text, []))

    return call


#: control finds only d1; treatment finds all three. Deltas on
#: strict_evidence_recall@10 in gold order: q1 0, q2 +1, q3 +1; qn is a
#: negative (strict None in both arms) -> metric_inapplicable.
CONTROL_HITS = {Q1: ["d1"]}
TREATMENT_HITS = {Q1: ["d1"], Q2: ["d2"], Q3: ["d3"]}


def _overrides() -> dict[str, Any]:
    return {"control": _override(CONTROL_HITS), "treatment": _override(TREATMENT_HITS)}


def _sidecar(run_dir: Path) -> dict[str, Any]:
    return json.loads((run_dir / "earp.result.v1.json").read_text(encoding="utf-8"))


def _per_query(run_dir: Path) -> list[dict[str, Any]]:
    text = (run_dir / "earp.per-query.v1.jsonl").read_text(encoding="utf-8")
    return [json.loads(line) for line in text.strip().splitlines()]


@requires_binding
def test_two_arm_comparison_end_to_end(tmp_path: Path) -> None:
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed)
    result = run_comparison(
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-comparison",
        ts=TS,
        retrieve_overrides=_overrides(),
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.n == 3
    assert result.effect == pytest.approx(2.0 / 3.0)
    assert result.run_dir is not None

    sidecar = _sidecar(result.run_dir)
    assert sidecar["campaign"] == "comparison"
    comparison = sidecar["comparison"]
    assert comparison["n"] == 3
    assert comparison["metric"] == "strict_evidence_recall@10"
    assert comparison["ci_method"] == "paired_bootstrap"
    assert comparison["seed"] == 42
    assert comparison["changed_knobs"] == ["scenario.query.limit"]
    assert comparison["underpowered"] is False
    assert comparison["exclusions"] == {"metric_inapplicable": 1}
    #: n + sum(exclusions) reconciles exactly with the gold query count.
    assert comparison["n"] + sum(comparison["exclusions"].values()) == 4
    assert comparison["ci_low"] <= comparison["effect"] <= comparison["ci_high"]

    #: The whole-document hash is the run identity; arm hashes are labels.
    assert sidecar["scenario"]["config_sha256"] == _lib.config_sha256(dict(doc))
    arms = sidecar["arms"]
    assert set(arms) == {"control", "treatment"}
    for entry in arms.values():
        assert entry["arm_config_sha256"] != sidecar["scenario"]["config_sha256"]
    assert arms["control"]["fanout_used"] == 10
    assert arms["treatment"]["fanout_used"] == 20
    assert arms["treatment"]["metrics"]["per_k"]["10"]["strict_evidence_recall"]["value"] == 1.0

    #: No decision rule was declared: effect and CI are recorded (they are the
    #: comparison's output), but no better-than claim token is emitted.
    assert sidecar["decision_rule"] is None


@requires_binding
def test_per_query_rows_carry_arm_and_declared_stratum(tmp_path: Path) -> None:
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed)
    doc["comparison"]["strata"] = ["query_class"]
    result = run_comparison(
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-comparison",
        ts=TS,
        retrieve_overrides=_overrides(),
    )
    assert result.run_dir is not None
    rows = _per_query(result.run_dir)
    assert {row["arm"] for row in rows} == {"control", "treatment"}
    #: 4 queries x 2 K rungs x 2 arms.
    assert len(rows) == 16
    for row in rows:
        assert row["stratum"] == row["query_class"]


@requires_binding
def test_comparison_is_deterministic_across_runs(tmp_path: Path) -> None:
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed)
    outcomes = []
    for minutes in (0, 1):
        result = run_comparison(
            config_doc=doc,
            experiments_root=tmp_path / "experiments",
            experiment="earp-comparison",
            ts=TS + timedelta(minutes=minutes),
            retrieve_overrides=_overrides(),
        )
        outcomes.append((result.effect, result.ci_low, result.ci_high))
    assert outcomes[0] == outcomes[1]


@requires_binding
def test_decision_rule_pass_and_fail_follow_the_predeclared_threshold(tmp_path: Path) -> None:
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    bed = _bed(tmp_path)
    for minutes, threshold, expected in ((0, 0.5, "pass"), (1, 0.9, "fail")):
        doc = _e2e_doc(tmp_path, bed)
        doc["decision_rule"] = {
            "metric": "strict_evidence_recall@10",
            "direction": "greater",
            "threshold": threshold,
        }
        result = run_comparison(
            config_doc=doc,
            experiments_root=tmp_path / "experiments",
            experiment="earp-comparison",
            ts=TS + timedelta(minutes=minutes),
            retrieve_overrides=_overrides(),
        )
        assert result.decision == expected
        assert result.run_dir is not None
        assert _sidecar(result.run_dir)["decision_rule"]["result"] == expected


@requires_binding
def test_underpowered_is_n_below_min_n_against_the_declared_rule(tmp_path: Path) -> None:
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed)
    doc["comparison"]["min_n"] = 10
    doc["decision_rule"] = {
        "metric": "strict_evidence_recall@10",
        "direction": "greater",
        "threshold": 0.5,
    }
    result = run_comparison(
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-comparison",
        ts=TS,
        retrieve_overrides=_overrides(),
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.underpowered is True
    assert result.decision == "underpowered"
    assert result.run_dir is not None
    sidecar = _sidecar(result.run_dir)
    assert sidecar["comparison"]["underpowered"] is True
    assert sidecar["decision_rule"]["result"] == "underpowered"


@requires_binding
def test_zero_paired_queries_withholds_the_claim(tmp_path: Path) -> None:
    """Every control retrieval errors -> every query is excluded, n == 0, the
    run is COMPLETE, and the declared rule reports `withheld` (there is nothing
    to compare; `underpowered` would imply data that was merely thin)."""
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    def boom(_query: str) -> Any:
        raise RuntimeError("retrieval exploded")

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed)
    doc["decision_rule"] = {
        "metric": "strict_evidence_recall@10",
        "direction": "greater",
        "threshold": 0.5,
    }
    result = run_comparison(
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-comparison",
        ts=TS,
        retrieve_overrides={"control": boom, "treatment": _override(TREATMENT_HITS)},
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.n == 0
    assert result.decision == "withheld"
    assert result.run_dir is not None
    sidecar = _sidecar(result.run_dir)
    assert sidecar["comparison"]["exclusions"] == {"error": 4}
    assert sidecar["decision_rule"]["result"] == "withheld"


@requires_binding
def test_blocked_arm_blocks_the_comparison_never_a_one_armed_number(tmp_path: Path) -> None:
    from eval.earp.characterize import ArmExecution, execute_arm  # noqa: PLC0415
    from eval.earp.comparison import run_comparison  # noqa: PLC0415

    injected = Blocker(
        code=BlockerCode.CORPUS_ROOT_ABSENT,
        message="injected: treatment arm blocked",
        stage="test.arm",
    )

    def executor(**kwargs: Any) -> ArmExecution:
        if kwargs["scenario"].max_measurable_k == 20:  # the treatment arm
            return ArmExecution(blocker=injected)
        return execute_arm(**kwargs)

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed)
    result = run_comparison(
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-comparison",
        ts=TS,
        retrieve_overrides=_overrides(),
        arm_executor=executor,
    )
    assert result.verdict is RunVerdict.BLOCKED
    assert result.blockers != ()
    assert result.run_dir is not None
    sidecar = _sidecar(result.run_dir)
    assert sidecar["verdict"] == "blocked"
    #: Never a one-armed number.
    assert sidecar["comparison"] is None
    #: The surviving arm's partials live under its arm name.
    control_entry = sidecar["arms"]["control"]
    assert control_entry["metrics"]["per_k"]["10"]["n"] == 3
    treatment_entry = sidecar["arms"]["treatment"]
    assert treatment_entry["blockers"]
    assert treatment_entry["blockers"][0]["message"].startswith("injected")


@requires_binding
def test_sweep_records_per_arm_outcomes_and_no_claim(tmp_path: Path) -> None:
    from eval.earp.comparison import run_sweep  # noqa: PLC0415

    bed = _bed(tmp_path)
    doc = _e2e_doc(tmp_path, bed, campaign="sweep", comparison=None)
    doc["arms"] = [
        {"name": "a", "scenario": _arm_scenario()},
        {"name": "b", "scenario": _arm_scenario(limit=20)},
        {"name": "c", "scenario": _arm_scenario(limit=30)},
    ]
    result = run_sweep(
        config_doc=doc,
        experiments_root=tmp_path / "experiments",
        experiment="earp-sweep",
        ts=TS,
        retrieve_overrides={
            "a": _override(CONTROL_HITS),
            "b": _override(TREATMENT_HITS),
            "c": _override(TREATMENT_HITS),
        },
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.run_dir is not None
    sidecar = _sidecar(result.run_dir)
    assert sidecar["campaign"] == "sweep"
    #: Outcomes recorded, no comparative claim: no deltas, no CI, no rule.
    assert sidecar["comparison"] is None
    assert "decision_rule" not in sidecar
    assert set(sidecar["arms"]) == {"a", "b", "c"}
    strict_a = sidecar["arms"]["a"]["metrics"]["per_k"]["10"]["strict_evidence_recall"]["value"]
    strict_b = sidecar["arms"]["b"]["metrics"]["per_k"]["10"]["strict_evidence_recall"]["value"]
    assert strict_a == pytest.approx(1.0 / 3.0)
    assert strict_b == 1.0
    rows = _per_query(result.run_dir)
    assert {row["arm"] for row in rows} == {"a", "b", "c"}


# --- the CLI ------------------------------------------------------------------


def test_cli_prints_the_arms_summary(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    from eval.earp.cli import main  # noqa: PLC0415

    path = tmp_path / "c.json"
    path.write_text(json.dumps(_doc()), encoding="utf-8")
    assert main(["validate", str(path)]) == 0
    out = capsys.readouterr().out
    assert "2" in out  # arm count
    assert "scenario.query.limit" in out  # changed knobs
    for token in ("strict_evidence_recall@10", "paired_bootstrap", "42", "200"):
        assert token in out  # the fixed statistics tuple


def test_cli_still_refuses_a_bad_arms_config(tmp_path: Path) -> None:
    from eval.earp.cli import main  # noqa: PLC0415

    path = tmp_path / "c.json"
    doc = _doc()
    del doc["comparison"]["metric"]
    path.write_text(json.dumps(doc), encoding="utf-8")
    assert main(["validate", str(path)]) != 0
