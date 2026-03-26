from __future__ import annotations

from pathlib import Path


def test_run_harness_baseline_returns_expected_scenarios(tmp_path: Path) -> None:
    from examples.harness.app import run_harness

    results = run_harness(tmp_path / "baseline.db", mode="baseline")

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
        "vector_degradation",
    ]


def test_main_baseline_reports_pass_lines(tmp_path: Path, capsys) -> None:
    from examples.harness.app import main

    exit_code = main(["--db", str(tmp_path / "cli.db"), "--mode", "baseline"])
    output = capsys.readouterr().out

    assert exit_code == 0
    assert "PASS canonical_node_chunk_fts" in output
    assert "PASS vector_degradation" in output
    assert "12/12 scenarios passed" in output
