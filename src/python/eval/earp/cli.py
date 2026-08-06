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
import sys

from eval.earp.config import load_config, resolve_config


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

    scenario = result.scenario
    assert scenario is not None
    print(f"earp: {args.path} resolves")
    print(f"  campaign        {scenario.campaign.value}")
    print(f"  call            {scenario.query_call}")
    print(f"  retrieval mode  {scenario.retrieval_mode.value}")
    print(f"  max measurable  {scenario.max_measurable_k or 'unbounded'}")
    print(f"  evidence@K      {list(scenario.evidence_recall_k) or '-'}")
    print(f"  decision rule   {scenario.decision_rule or 'none (no better-than claim)'}")
    print(f"  config sha256   {scenario.config_sha256}")
    if scenario.carried_paths:
        print(f"  carried         {len(scenario.carried_paths)} path(s) for later slices")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
