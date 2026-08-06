"""S3 resolver tests — written RED, before `eval.earp.config` exists.

Pure: no SDK, no database, no network. Configs are built in-memory or written
to tmp_path; nothing here reads gold or corpus, because a config can be
well-formed while its data is absent and conflating the two would make
validation impossible in a worktree.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from eval.earp.config import (
    CALL_MODE,
    CONSUMER_REGISTRY,
    METRIC_NAMES,
    emits,
    resolve_config,
    schema_paths,
)
from eval.earp.knobs import CATALOG
from eval.earp.schema.models import BlockerCode, KnobClass, RetrievalMode

SHA = "a" * 64


def _config(**over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.v1",
        "campaign": "characterization",
        "corpus": {"snapshot": "tests/corpus/snapshot.json", "data_root": "data/corpus-data"},
        "gold": {
            "path": "d/all.gold.json",
            "sha256": SHA,
            "corpus_hash": SHA,
            "qrels_version": "ir-c-reused-v2",
        },
        "scenario": {
            "engine": {"use_default_embedder": True},
            "query": {"call": "Engine.search"},
        },
        "metrics": {"evidence_recall_k": [5, 10]},
    }
    for key, value in over.items():
        if value is None:
            doc.pop(key, None)
        else:
            doc[key] = value
    return doc


def _codes(result: Any) -> set[BlockerCode]:
    return {b.code for b in result.blockers}


# --- AC-1: the rejection classes ------------------------------------------


def test_good_config_resolves() -> None:
    result = resolve_config(_config())
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.retrieval_mode is RetrievalMode.HYBRID
    assert result.scenario.config_sha256


def test_unknown_key_is_refused() -> None:
    doc = _config()
    doc["corpuss"] = {}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNKNOWN_KEY in _codes(result)
    assert any("corpuss" in b.message for b in result.blockers)


def test_missing_required_key_is_refused() -> None:
    doc = _config()
    del doc["gold"]["sha256"]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)


def test_invalid_enum_value_is_refused() -> None:
    result = resolve_config(_config(campaign="benchmark"))
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_all_defects_are_collected_not_first_failure() -> None:
    """A config author needs every offending key in one pass. This is a
    deliberate departure from verify_gold, whose checks are ordered by trust."""
    doc = _config(campaign="benchmark")
    doc["nope"] = 1
    del doc["gold"]["sha256"]
    result = resolve_config(doc)
    assert {
        BlockerCode.CONFIG_UNKNOWN_KEY,
        BlockerCode.CONFIG_MISSING_KEY,
        BlockerCode.CONFIG_INVALID_VALUE,
    } <= _codes(result)


# --- the walker's YAML-specific traps --------------------------------------


def test_bool_is_not_accepted_as_integer() -> None:
    """PyYAML yields Python bool, and isinstance(True, int) is True, so
    `rerank_depth: true` would otherwise resolve as 1."""
    doc = _config()
    doc["scenario"]["query"]["rerank_depth"] = True
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_pattern_is_enforced() -> None:
    doc = _config()
    doc["gold"]["sha256"] = "abc"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_alpha_out_of_range_is_refused() -> None:
    """The engine CLAMPS alpha rather than refusing, so an unbounded value
    would run as 1.0 while the sidecar recorded 5.0."""
    doc = _config()
    doc["scenario"]["query"]["alpha"] = 5.0
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_non_finite_alpha_is_refused() -> None:
    """minimum/maximum cannot catch NaN: nan < 0 and nan > 1 are both False."""
    doc = _config()
    doc["scenario"]["query"]["alpha"] = float("nan")
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


# --- AC-2/3: consumption ---------------------------------------------------


def test_every_schema_path_has_a_registered_consumer() -> None:
    """The real anti-drift device: adding a schema key without registering an
    owning slice is a red test, and it works against unlanded slices."""
    unregistered = sorted(set(schema_paths()) - set(CONSUMER_REGISTRY))
    assert unregistered == []


def test_later_slice_paths_are_carried_not_refused() -> None:
    doc = _config()
    doc["scenario"]["store"] = {"mode": "canonical_docs"}
    doc["metrics"]["integrity"] = ["provenance"]
    result = resolve_config(doc)
    assert result.blockers == ()
    assert "scenario.store.mode" in result.scenario.carried_paths


def test_comparison_block_on_a_characterization_is_refused() -> None:
    """Precedence: the inexpressible-campaign refusal outranks carrying."""
    doc = _config()
    doc["comparison"] = {"changed_knobs": ["scenario.query.alpha"]}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNUSED_KEY in _codes(result)


def test_budget_is_carried_unconditionally() -> None:
    """There is no priced-arm declaration in earp.v1, so any applicability
    predicate for budget would be undecidable."""
    doc = _config()
    doc["budget"] = {"estimated_usd": 0.5}
    result = resolve_config(doc)
    assert result.blockers == ()


# --- AC-4: derived mode and depth ------------------------------------------


@pytest.mark.parametrize(
    ("call", "embedder", "mode", "max_k"),
    [
        ("Engine.search_text_only", False, RetrievalMode.FTS_ONLY, None),
        ("Engine.search_text_only", True, RetrievalMode.FTS_ONLY, None),
        ("Engine.search", False, RetrievalMode.FTS_ONLY, None),
        ("Engine.search", True, RetrievalMode.HYBRID, 10),
        ("Engine.search_projected_text", False, RetrievalMode.FTS_ONLY, 10),
        ("Engine.search_projected_text", True, RetrievalMode.FTS_ONLY, 10),
    ],
)
def test_mode_derives_from_call_and_embedder(
    call: str, embedder: bool, mode: RetrievalMode, max_k: int | None
) -> None:
    assert CALL_MODE[(call, embedder)] == (mode, max_k)


def test_search_without_embedder_is_fts_not_hybrid() -> None:
    """With no embedder the vector branch is skipped entirely, so recording
    `hybrid` would name a mode the run did not use."""
    doc = _config()
    doc["scenario"]["engine"]["use_default_embedder"] = False
    result = resolve_config(doc)
    assert result.scenario.retrieval_mode is RetrievalMode.FTS_ONLY


def test_deep_k_refused_for_hybrid() -> None:
    doc = _config()
    doc["metrics"]["evidence_recall_k"] = [5, 10, 20]
    result = resolve_config(doc)
    assert BlockerCode.METRIC_NOT_MEASURABLE in _codes(result)


def test_deep_k_refused_for_projected_text_despite_being_fts() -> None:
    """The property-FTS SQL carries no LIMIT, but the reader breaks at
    SEARCH_RERANK_LIMIT — so this verb truncates at 10 even though it is FTS."""
    doc = _config()
    doc["scenario"]["query"] = {"call": "Engine.search_projected_text", "projection_name": "body"}
    doc["metrics"]["evidence_recall_k"] = [5, 10, 50]
    result = resolve_config(doc)
    assert BlockerCode.METRIC_NOT_MEASURABLE in _codes(result)


def test_deep_k_allowed_for_text_only() -> None:
    doc = _config()
    doc["scenario"]["query"] = {"call": "Engine.search_text_only"}
    doc["scenario"]["engine"] = {"use_default_embedder": False}
    doc["metrics"]["evidence_recall_k"] = [5, 10, 20, 50]
    result = resolve_config(doc)
    assert result.blockers == ()


def test_removed_mode_key_gets_a_named_message() -> None:
    doc = _config()
    doc["scenario"]["query"]["mode"] = "vector_only"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNKNOWN_KEY in _codes(result)
    assert any("derive" in b.message.lower() for b in result.blockers)


def test_vector_only_retained_in_the_depth_table() -> None:
    """Unreachable from config by construction, but live in MAX_MEASURABLE_K,
    the result schema, and S2's parity tests."""
    assert RetrievalMode.VECTOR_ONLY in RetrievalMode


# --- AC-6: knobs the call accepts ------------------------------------------


def test_knob_the_call_does_not_accept_is_refused() -> None:
    doc = _config()
    doc["scenario"]["query"] = {"call": "Engine.search_text_only", "rerank_depth": 5}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INAPPLICABLE_KNOB in _codes(result)


def test_projection_name_required_for_projected_text() -> None:
    doc = _config()
    doc["scenario"]["query"] = {"call": "Engine.search_projected_text"}
    doc["metrics"]["evidence_recall_k"] = [5, 10]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)


def test_projection_name_rejected_for_other_calls() -> None:
    doc = _config()
    doc["scenario"]["query"] = {"call": "Engine.search", "projection_name": "body"}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INAPPLICABLE_KNOB in _codes(result)


# --- AC-8: inexpressible campaigns -----------------------------------------


@pytest.mark.parametrize("kind", ["comparison", "sweep", "replay"])
def test_inexpressible_campaigns_are_refused(kind: str) -> None:
    """earp.v1 has one scenario and no arms array, so these cannot be
    represented — accepting them silently is the worst option."""
    result = resolve_config(_config(campaign=kind))
    assert BlockerCode.CONFIG_CAMPAIGN_INEXPRESSIBLE in _codes(result)
    assert any("S8" in b.message or "S6" in b.message for b in result.blockers)


def test_diagnostic_must_not_declare_evidence_recall() -> None:
    doc = _config(campaign="diagnostic", gold=None)
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INAPPLICABLE_KNOB in _codes(result)


def test_diagnostic_without_metrics_resolves_without_gold() -> None:
    doc = _config(campaign="diagnostic", gold=None, metrics=None)
    result = resolve_config(doc)
    assert result.blockers == ()


# --- AC-9: gold is required by metrics, not by campaign kind ---------------


def test_metrics_without_gold_is_refused() -> None:
    result = resolve_config(_config(gold=None))
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)


# --- AC-5: the metric namespace --------------------------------------------


def test_decision_rule_on_an_unemittable_metric_is_refused() -> None:
    doc = _config()
    doc["decision_rule"] = {"metric": "ndcg", "direction": "greater", "threshold": 0.4}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_decision_rule_on_a_good_metric_resolves() -> None:
    doc = _config()
    doc["decision_rule"] = {
        "metric": "strict_evidence_recall@10",
        "direction": "greater",
        "threshold": 0.4,
    }
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario.decision_rule is not None


def test_absent_decision_rule_stays_absent() -> None:
    result = resolve_config(_config())
    assert result.scenario.decision_rule is None


def test_decision_rule_at_an_undeclared_k_is_refused() -> None:
    doc = _config()
    doc["decision_rule"] = {
        "metric": "strict_evidence_recall@50",
        "direction": "greater",
        "threshold": 0.4,
    }
    result = resolve_config(doc)
    assert result.blockers != ()


@pytest.mark.parametrize("name", ["mrr", "ndcg"])
def test_document_metrics_never_emit(name: str) -> None:
    """ndcg has no graded relevance anywhere; mrr has no implementation in any
    landed or planned slice, so gating on it would await a number that never
    arrives."""
    assert emits(name, evidence_recall_k=(5, 10), has_negatives=True) is False


def test_at_k_required_for_per_k_metrics() -> None:
    assert emits("strict_evidence_recall", evidence_recall_k=(5, 10), has_negatives=True) is False
    assert emits("strict_evidence_recall@10", evidence_recall_k=(5, 10), has_negatives=True) is True


def test_at_k_forbidden_for_k_free_metrics() -> None:
    assert emits("abstention_rate@10", evidence_recall_k=(5, 10), has_negatives=True) is False
    assert emits("abstention_rate", evidence_recall_k=(5, 10), has_negatives=True) is True
    assert emits("abstention_rate", evidence_recall_k=(5, 10), has_negatives=False) is False


def test_metric_names_cover_the_result_schema_fields() -> None:
    assert {"strict_evidence_recall", "graded_evidence_recall", "supporting_coverage"} <= set(
        METRIC_NAMES
    )


# --- AC-10: catalog completeness -------------------------------------------


def _engine_config_fields() -> set[str]:
    """Load `fathomdb/config.py` as a standalone module.

    A plain `from fathomdb.config import EngineConfig` runs the package
    `__init__`, which loads the native extension and fails in a worktree with no
    built binding. The module itself imports only `dataclasses`, so loading the
    file directly keeps this assertion genuinely pure.
    """
    import importlib.util  # noqa: PLC0415
    import sys  # noqa: PLC0415

    name = "_earp_engine_config"
    path = Path(__file__).parents[2] / "fathomdb/config.py"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    # @dataclass resolves its module through sys.modules, so registering before
    # exec is required, not cosmetic.
    sys.modules[name] = module
    try:
        spec.loader.exec_module(module)
        return set(module.EngineConfig.__dataclass_fields__)
    finally:
        sys.modules.pop(name, None)


def test_every_engine_config_field_has_a_verdict() -> None:
    assert _engine_config_fields() <= {entry.name for entry in CATALOG}


def test_engine_config_has_exactly_the_five_known_fields() -> None:
    """Guards the bound: if the SDK grows a sixth field, the catalog's coverage
    claim must be re-examined rather than silently under-covering."""
    assert _engine_config_fields() == {
        "embedder_pool_size",
        "scheduler_runtime_threads",
        "provenance_row_cap",
        "embedder_call_timeout_ms",
        "slow_threshold_ms",
    }


def test_supported_entries_carry_a_call_path_and_witness() -> None:
    supported = {
        KnobClass.SEMANTIC,
        KnobClass.INDEXING,
        KnobClass.RUNTIME,
        KnobClass.OBSERVABILITY,
    }
    for entry in CATALOG:
        if entry.classification in supported:
            assert entry.call_path, entry.name
            assert entry.witness, entry.name
        else:
            assert entry.call_path is None, entry.name
            assert entry.reason, entry.name


def test_slow_threshold_ms_is_supported_not_unsupported() -> None:
    """An EngineConfig field Engine.open never forwards, yet independently
    supported — the case that breaks a dataclass-membership-keyed catalog."""
    entry = next(e for e in CATALOG if e.name == "slow_threshold_ms")
    assert entry.classification is KnobClass.RUNTIME
    assert entry.call_path == "Engine.set_slow_threshold_ms"


def test_catalog_covers_the_search_signatures() -> None:
    """Bounded introspection asserting coverage, not reflection defining the
    surface. Skips visibly when the native binding is absent."""
    import inspect  # noqa: PLC0415

    try:
        from fathomdb import engine  # noqa: PLC0415
    except ImportError as exc:
        # A visible skip, never a silent pass. importorskip is not enough: the
        # package __init__ raises ImportError (not ModuleNotFoundError) when the
        # extension is absent.
        pytest.skip(f"native binding not built: {exc}")

    import re  # noqa: PLC0415

    # A catalog entry's `name` is EARP's CONFIG-facing name, which is not always
    # the SDK parameter name -- `projection_name` covers `Engine
    # .search_projected_text(name=)`. The call path already records the real
    # parameter, so coverage is checked against both.
    covered = {entry.name for entry in CATALOG}
    for entry in CATALOG:
        match = re.search(r"\((\w+)=\)$", entry.call_path or "")
        if match:
            covered.add(match.group(1))

    ignored = {"self", "engine", "query"}
    for func in (
        engine.Engine.search,
        engine.Engine.search_projected_text,
        engine.Engine.search_text_only,
    ):
        for param in inspect.signature(func).parameters:
            if param not in ignored:
                assert param in covered, f"{func.__name__}:{param}"


# --- AC-11: the undeclared-dependency guard --------------------------------


def test_no_module_imports_jsonschema() -> None:
    """jsonschema is importable here but declared in no extra — exactly how the
    repo previously shipped a harness that failed a clean install."""
    for path in Path(__file__).parents[2].joinpath("eval/earp").rglob("*.py"):
        assert "import jsonschema" not in path.read_text(encoding="utf-8"), path


# --- AC-12: the CLI ---------------------------------------------------------


def test_cli_exits_zero_on_a_good_config(tmp_path: Path) -> None:
    from eval.earp.cli import main  # noqa: PLC0415

    path = tmp_path / "c.json"
    path.write_text(json.dumps(_config()), encoding="utf-8")
    assert main(["validate", str(path)]) == 0


def test_cli_exits_nonzero_on_a_bad_config(tmp_path: Path) -> None:
    from eval.earp.cli import main  # noqa: PLC0415

    path = tmp_path / "c.json"
    path.write_text(json.dumps(_config(campaign="benchmark")), encoding="utf-8")
    assert main(["validate", str(path)]) != 0
