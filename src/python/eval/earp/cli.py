"""EARP command line.

`python -m eval.earp.cli validate <path>` -- the repo has no
`[project.scripts]` and `eval/` is not installed, so this follows the existing
harness convention of `main(argv) -> int` plus a `__main__` guard, reachable
because the source root is on `sys.path` under pytest and `python -m`.

`validate` touches no gold, no corpus, and no engine: a config can be
well-formed while its data is absent, and conflating the two would make
validation impossible in a worktree.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

from eval.earp.config import ResolvedScenario, load_config, resolve_config
from eval.earp.schema.models import Blocker


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="earp", description="EARP developer harness")
    sub = parser.add_subparsers(dest="command", required=True)
    validate_cmd = sub.add_parser("validate", help="resolve a campaign configuration")
    validate_cmd.add_argument("path", help="path to an earp.v1 config (YAML or JSON)")
    args = parser.parse_args(argv)

    if args.command != "validate":  # pragma: no cover - argparse enforces this
        parser.error(f"unknown command {args.command}")

    try:
        doc = load_config(args.path)
    except (OSError, ValueError) as exc:
        print(f"earp: cannot read config: {exc}", file=sys.stderr)
        return 2

    result = resolve_config(doc)
    if result.blockers:
        print(f"earp: {len(result.blockers)} defect(s) in {args.path}", file=sys.stderr)
        for blocker in result.blockers:
            path = blocker.detail.get("path", "?")
            print(f"  [{blocker.code.value}] {path}: {blocker.message}", file=sys.stderr)
        return 1

    if result.arms:
        print(f"earp: {args.path} resolves")
        print(f"  campaign        {doc.get('campaign')}")
        print(f"  arms            {len(result.arms)} ({', '.join(a.name for a in result.arms)})")
        for arm in result.arms:
            print(
                f"    {arm.name}: {arm.scenario.query_call} "
                f"[{arm.scenario.retrieval_mode.value}, limit {arm.scenario.max_measurable_k}]"
            )
        comparison = result.comparison
        if comparison is not None:
            print(f"  changed knobs   {list(comparison.changed_knobs)}")
            print(
                f"  statistics      metric={comparison.metric} "
                f"method={comparison.ci_method} seed={comparison.seed} "
                f"resamples={comparison.resamples} min_n={comparison.min_n}"
            )
        else:
            print("  statistics      none (sweep: outcomes only, no claim)")
        print(f"  decision rule   {result.decision_rule or 'none (no better-than claim)'}")
        return 0

    scenario = result.scenario
    assert scenario is not None
    print(f"earp: {args.path} resolves")
    print(f"  campaign        {scenario.campaign.value}")
    print(f"  call            {scenario.query_call}")
    print(f"  retrieval mode  {scenario.retrieval_mode.value}")
    print(f"  result limit    {scenario.max_measurable_k}")
    print(f"  evidence@K      {list(scenario.evidence_recall_k) or '-'}")
    print(f"  decision rule   {scenario.decision_rule or 'none (no better-than claim)'}")
    print(f"  config sha256   {scenario.config_sha256}")
    if scenario.carried_paths:
        print(f"  carried         {len(scenario.carried_paths)} path(s) for later slices")
    _print_budget(scenario, doc)
    return 0


def _print_budget(scenario: ResolvedScenario, doc: object) -> None:
    """S9: for an answer-arm config, show the money the run would put at
    stake -- the declared estimate, the current cumulative spend, and the
    projection against the D-3 authorization."""
    from eval.earp.pricing import (  # noqa: PLC0415 -- keep validate import-light
        D3_AUTHORIZED_USD,
        LEDGER_ROOT_ENV,
        read_cumulative_spend,
    )

    arm = scenario.answer_arm
    if arm is None:
        return
    estimated = 0.0
    if isinstance(doc, dict):
        budget = doc.get("budget")
        if isinstance(budget, dict) and isinstance(budget.get("estimated_usd"), (int, float)):
            estimated = float(budget["estimated_usd"])
    declared_root = os.environ.get(LEDGER_ROOT_ENV)
    if declared_root:
        root = Path(declared_root)
        root_note = f"{root} ({LEDGER_ROOT_ENV})"
    else:
        from eval.earp._experiments import REPO_ROOT  # noqa: PLC0415

        root = REPO_ROOT / "experiments"
        root_note = f"{root} ({LEDGER_ROOT_ENV} unset; priced runs will refuse)"
    cumulative = read_cumulative_spend(root)
    print(
        f"  answer arm      {arm.kind} "
        f"(model {arm.answerer_model or 'env-resolved (claim-free only)'}, "
        f"max_queries {arm.max_queries})"
    )
    if isinstance(cumulative, Blocker):
        print(f"  budget          MALFORMED LEDGER: {cumulative.message}")
        return
    print(
        f"  budget          estimate ${estimated:.2f} + cumulative ${cumulative:.2f} "
        f"= projected ${cumulative + estimated:.2f} of ${D3_AUTHORIZED_USD:.2f} authorized"
    )
    print(f"  ledger          {root_note}")


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
