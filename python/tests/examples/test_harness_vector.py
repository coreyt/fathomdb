from __future__ import annotations

from pathlib import Path


def test_run_harness_vector_returns_expected_scenarios(tmp_path: Path) -> None:
    from examples.harness.app import run_harness
    from examples.harness.engine_factory import supports_vector_mode

    assert supports_vector_mode() is True, "python binding build must include sqlite-vec support"

    results = run_harness(tmp_path / "vector.db", mode="vector")

    assert [result.name for result in results] == [
        "canonical_node_chunk_fts",
        "node_upsert_supersession",
        "graph_edge_traversal",
        "edge_retire",
        "runtime_tables",
        "node_retire_clean",
        "node_retire_dangling",
        "provenance_warn_require",
        "trace_and_excise",
        "safe_export",
        "projection_rebuild",
        "restore_vector_profiles",
        "vector_insert_and_search",
    ]
