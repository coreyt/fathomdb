from __future__ import annotations

import argparse
from pathlib import Path
from typing import Callable

from .engine_factory import DEFAULT_VECTOR_DIMENSION, open_engine, supports_vector_mode
from .models import HarnessContext, ScenarioResult
from .scenarios import (
    canonical_node_chunk_fts,
    edge_retire,
    graph_edge_traversal,
    node_retire_clean,
    node_retire_dangling,
    node_upsert_supersession,
    projection_rebuild,
    provenance_warn_require,
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
) -> list[ScenarioResult]:
    path = Path(db_path)
    if mode == "vector" and not supports_vector_mode():
        raise RuntimeError("vector mode requires a sqlite-vec-enabled Python binding build")
    context = HarnessContext(
        engine=open_engine(path, mode=mode, vector_dimension=vector_dimension),
        db_path=path,
        mode=mode,
        vector_dimension=vector_dimension,
    )
    return [scenario(context) for scenario in _scenario_functions(mode)]


def main(argv: list[str] | None = None) -> int:
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
    args = parser.parse_args(argv)

    path = Path(args.db)
    if args.mode == "vector" and not supports_vector_mode():
        print("FAIL vector_support vector mode requires a sqlite-vec-enabled Python binding build")
        return 1

    context = HarnessContext(
        engine=open_engine(path, mode=args.mode, vector_dimension=args.vector_dimension),
        db_path=path,
        mode=args.mode,
        vector_dimension=args.vector_dimension,
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
