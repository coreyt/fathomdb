from __future__ import annotations

import os
import subprocess
import sys
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


def test_run_harness_baseline_collects_telemetry_events(tmp_path: Path) -> None:
    from examples.harness.app import run_harness

    events = []
    run_harness(
        tmp_path / "telemetry.db",
        mode="baseline",
        telemetry_callback=events.append,
    )

    phases = {event.phase.value for event in events}
    operation_kinds = {event.operation_kind for event in events}

    assert "started" in phases
    assert "finished" in phases
    assert "engine.open" in operation_kinds
    assert "write.submit" in operation_kinds
    assert "query.execute" in operation_kinds
    assert "admin.check_integrity" in operation_kinds


def test_main_baseline_emits_telemetry_lines_when_enabled(tmp_path: Path, capsys) -> None:
    from examples.harness.app import main

    exit_code = main(
        [
            "--db",
            str(tmp_path / "telemetry-cli.db"),
            "--mode",
            "baseline",
            "--telemetry",
            "all",
        ]
    )
    output = capsys.readouterr().out

    assert exit_code == 0
    assert "TELEMETRY phase=started op=engine.open" in output
    assert "TELEMETRY phase=finished op=write.submit" in output


def test_python_module_entrypoint_has_no_runpy_warning(tmp_path: Path) -> None:
    env = os.environ.copy()
    pythonpath = env.get("PYTHONPATH")
    env["PYTHONPATH"] = "python" if not pythonpath else f"python:{pythonpath}"

    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "examples.harness.app",
            "--db",
            str(tmp_path / "module.db"),
            "--mode",
            "baseline",
        ],
        cwd=Path(__file__).resolve().parents[3],
        env=env,
        capture_output=True,
        text=True,
        check=True,
    )

    assert "RuntimeWarning" not in result.stderr
    assert "12/12 scenarios passed" in result.stdout
