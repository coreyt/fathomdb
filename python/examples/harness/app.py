"""CLI entry point and runner for the fathomdb example harness."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Callable

from fathomdb import FeedbackConfig, ResponseCycleEvent, ResponseCyclePhase

from .engine_factory import DEFAULT_VECTOR_DIMENSION, open_engine, supports_vector_mode
from .models import HarnessContext, ScenarioResult
from .scenarios import (
    adaptive_search_mixed_chunk_and_property,
    adaptive_search_recursive_nested_payload,
    adaptive_search_recursive_rebuild_restore,
    adaptive_search_strict_hit_only,
    adaptive_search_strict_miss_relaxed_recovery,
    canonical_node_chunk_fts,
    edge_retire,
    graph_edge_traversal,
    node_retire_clean,
    node_retire_dangling,
    node_upsert_supersession,
    projection_rebuild,
    provenance_warn_require,
    restore_vector_profiles,
    runtime_tables,
    safe_export,
    trace_and_excise,
    vector_degradation,
    vector_insert_and_search,
)

ScenarioFn = Callable[[HarnessContext], ScenarioResult]


def _scenario_functions(mode: str) -> list[ScenarioFn]:
    scenarios: list[ScenarioFn] = [
        canonical_node_chunk_fts,
        node_upsert_supersession,
        graph_edge_traversal,
        edge_retire,
        runtime_tables,
        node_retire_clean,
        node_retire_dangling,
        provenance_warn_require,
        trace_and_excise,
        safe_export,
        projection_rebuild,
        restore_vector_profiles,
        adaptive_search_strict_hit_only,
        adaptive_search_strict_miss_relaxed_recovery,
        adaptive_search_mixed_chunk_and_property,
        adaptive_search_recursive_nested_payload,
        adaptive_search_recursive_rebuild_restore,
    ]
    if mode == "baseline":
        scenarios.append(vector_degradation)
        return scenarios
    if mode == "vector":
        scenarios.append(vector_insert_and_search)
        return scenarios
    raise ValueError(f"unsupported harness mode: {mode}")


def run_harness(
    db_path: str | Path,
    *,
    mode: str,
    vector_dimension: int = DEFAULT_VECTOR_DIMENSION,
    telemetry_callback: Callable[[ResponseCycleEvent], None] | None = None,
    feedback_config: FeedbackConfig | None = None,
) -> list[ScenarioResult]:
    """Execute all harness scenarios and return their results."""
    path = Path(db_path)
    if mode == "vector" and not supports_vector_mode():
        raise RuntimeError("vector mode requires a sqlite-vec-enabled Python binding build")
    context = HarnessContext(
        engine=open_engine(
            path,
            mode=mode,
            vector_dimension=vector_dimension,
            progress_callback=telemetry_callback,
            feedback_config=feedback_config,
        ),
        db_path=path,
        mode=mode,
        vector_dimension=vector_dimension,
        progress_callback=telemetry_callback,
        feedback_config=feedback_config,
    )
    return [scenario(context) for scenario in _scenario_functions(mode)]


def _make_cli_telemetry_callback(mode: str) -> Callable[[ResponseCycleEvent], None] | None:
    if mode == "off":
        return None

    def callback(event: ResponseCycleEvent) -> None:
        if mode == "slow" and event.phase not in {
            ResponseCyclePhase.SLOW,
            ResponseCyclePhase.HEARTBEAT,
            ResponseCyclePhase.FAILED,
        }:
            return
        metadata = " ".join(
            f"{key}={value}" for key, value in sorted(event.metadata.items())
        )
        details = f" {metadata}" if metadata else ""
        print(
            f"TELEMETRY phase={event.phase.value} op={event.operation_kind} "
            f"surface={event.surface} elapsed_ms={event.elapsed_ms}{details}"
        )

    return callback


def main(argv: list[str] | None = None) -> int:
    """Parse CLI arguments and run the harness, printing results to stdout."""
    parser = argparse.ArgumentParser(description="Run the fathomdb Python example harness")
    parser.add_argument("--db", required=True, help="Path to the harness database")
    parser.add_argument(
        "--mode",
        choices=["baseline", "vector"],
        default="baseline",
        help="Harness profile to execute",
    )
    parser.add_argument(
        "--vector-dimension",
        type=int,
        default=DEFAULT_VECTOR_DIMENSION,
        help="Vector dimension to use in vector mode",
    )
    parser.add_argument(
        "--telemetry",
        choices=["off", "slow", "all"],
        default="off",
        help="Telemetry output mode for response-cycle feedback",
    )
    parser.add_argument(
        "--slow-threshold-ms",
        type=int,
        default=500,
        help="Slow threshold for telemetry in milliseconds",
    )
    parser.add_argument(
        "--heartbeat-interval-ms",
        type=int,
        default=2000,
        help="Heartbeat interval for telemetry in milliseconds",
    )
    args = parser.parse_args(argv)

    path = Path(args.db)
    if args.mode == "vector" and not supports_vector_mode():
        print("FAIL vector_support vector mode requires a sqlite-vec-enabled Python binding build")
        return 1

    feedback_config = FeedbackConfig(
        slow_threshold_ms=args.slow_threshold_ms,
        heartbeat_interval_ms=args.heartbeat_interval_ms,
    )
    telemetry_callback = _make_cli_telemetry_callback(args.telemetry)

    context = HarnessContext(
        engine=open_engine(
            path,
            mode=args.mode,
            vector_dimension=args.vector_dimension,
            progress_callback=telemetry_callback,
            feedback_config=feedback_config,
        ),
        db_path=path,
        mode=args.mode,
        vector_dimension=args.vector_dimension,
        progress_callback=telemetry_callback,
        feedback_config=feedback_config,
    )

    scenarios = _scenario_functions(args.mode)
    passed = 0
    for scenario in scenarios:
        try:
            scenario(context)
        except Exception as exc:
            print(f"FAIL {scenario.__name__} {exc}")
            return 1
        print(f"PASS {scenario.__name__}")
        passed += 1

    print(f"{passed}/{len(scenarios)} scenarios passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
