#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import Callable, Iterable


REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_BENCHMARK_WORKFLOW = "Benchmark And Robustness"
DEFAULT_CI_WORKFLOW = "CI"
DEFAULT_PYTHON_WORKFLOW = "Python"
DEFAULT_FRESHNESS_DAYS = 10
GH_RUN_FIELDS = "conclusion,headBranch,headSha,status,updatedAt,url"


class ReleaseGateError(RuntimeError):
    pass


@dataclass(frozen=True)
class WorkflowRun:
    conclusion: str
    head_branch: str
    head_sha: str
    status: str
    updated_at: datetime
    url: str


def run_command(
    args: list[str],
    *,
    cwd: pathlib.Path | None = None,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def parse_rfc3339(timestamp: str) -> datetime:
    normalized = timestamp.replace("Z", "+00:00")
    parsed = datetime.fromisoformat(normalized)
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def load_runs(payload: str) -> list[WorkflowRun]:
    try:
        raw_runs = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise ReleaseGateError(f"failed to parse gh JSON output: {exc}") from exc

    runs: list[WorkflowRun] = []
    for raw in raw_runs:
        try:
            runs.append(
                WorkflowRun(
                    conclusion=raw["conclusion"],
                    head_branch=raw["headBranch"],
                    head_sha=raw["headSha"],
                    status=raw["status"],
                    updated_at=parse_rfc3339(raw["updatedAt"]),
                    url=raw["url"],
                )
            )
        except KeyError as exc:
            raise ReleaseGateError(f"gh JSON output missing field: {exc.args[0]}") from exc
    return runs


def gh_run_list(
    repo: str,
    workflow: str,
    *,
    commit: str | None = None,
    branch: str | None = None,
    status: str | None = None,
    runner: Callable[..., subprocess.CompletedProcess[str]] = run_command,
) -> list[WorkflowRun]:
    # Older gh versions (pre-2.29) do not support `--commit`, `--branch`,
    # or `--status` as filter flags on `gh run list`. To stay portable
    # across gh versions — this script is invoked both from CI runners
    # (newer gh) and local dev machines (sometimes older gh like
    # 2.4.0 on stock Debian / Ubuntu) — fetch the most recent 20 runs
    # for the given workflow and do all filtering client-side on the
    # returned JSON payload. The `--repo`, `--workflow`, `--limit`,
    # and `--json` flags are supported back to the earliest gh releases
    # we care about.
    args = [
        "gh",
        "run",
        "list",
        "--repo",
        repo,
        "--workflow",
        workflow,
        "--limit",
        "20",
        "--json",
        GH_RUN_FIELDS,
    ]

    try:
        completed = runner(args, cwd=REPO_ROOT)
    except FileNotFoundError as exc:
        raise ReleaseGateError("gh command not found") from exc
    if completed.returncode != 0:
        raise ReleaseGateError(
            f"gh run list failed for workflow {workflow!r}: {completed.stderr.strip()}"
        )

    runs = load_runs(completed.stdout)
    if commit:
        runs = [r for r in runs if r.head_sha == commit]
    if branch:
        runs = [r for r in runs if r.head_branch == branch]
    if status:
        # gh's --status accepts completion conclusions like "success"
        # and run statuses like "in_progress". Match on either field
        # so the client-side filter preserves the original semantics.
        runs = [r for r in runs if status in (r.conclusion, r.status)]
    return runs


def require_successful_commit_run(
    repo: str,
    workflow: str,
    commit: str,
    *,
    poll_interval: int = 30,
    poll_timeout: int = 600,
    runner: Callable[..., subprocess.CompletedProcess[str]] = run_command,
) -> WorkflowRun:
    deadline = time.monotonic() + poll_timeout
    while True:
        runs = gh_run_list(repo, workflow, commit=commit, status="success", runner=runner)
        for run in runs:
            if run.conclusion == "success" and run.status == "completed" and run.head_sha == commit:
                return run
        if time.monotonic() >= deadline:
            break
        # Check if there are any in-progress runs worth waiting for.
        all_runs = gh_run_list(repo, workflow, commit=commit, runner=runner)
        pending = [r for r in all_runs if r.head_sha == commit and r.status in ("in_progress", "queued", "waiting")]
        if not pending:
            break
        remaining = int(deadline - time.monotonic())
        print(f"waiting for {workflow} on {commit[:12]}... ({remaining}s remaining)")
        time.sleep(min(poll_interval, max(1, remaining)))
    raise ReleaseGateError(
        f"no successful {workflow} workflow run found for commit {commit}"
    )


def require_recent_successful_run_on_main(
    repo: str,
    workflow: str,
    *,
    freshness_days: int,
    runner: Callable[..., subprocess.CompletedProcess[str]] = run_command,
) -> WorkflowRun:
    runs = gh_run_list(repo, workflow, branch="main", status="success", runner=runner)
    for run in runs:
        if run.conclusion != "success" or run.status != "completed":
            continue
        age = datetime.now(timezone.utc) - run.updated_at
        if age <= timedelta(days=freshness_days):
            return run
        raise ReleaseGateError(
            f"latest successful {workflow} run on main is too old: {run.updated_at.isoformat()}"
        )
    raise ReleaseGateError(f"no successful {workflow} workflow run found on main")


def run_version_check(tag: str) -> None:
    completed = run_command(
        [sys.executable, str(REPO_ROOT / "scripts" / "check-version-consistency.py"), "--tag", tag],
        cwd=REPO_ROOT,
    )
    if completed.returncode != 0:
        message = completed.stderr.strip() or completed.stdout.strip() or "version check failed"
        raise ReleaseGateError(message)


def verify_release_gates(
    *,
    repo: str,
    commit: str,
    tag: str,
    freshness_days: int = DEFAULT_FRESHNESS_DAYS,
    benchmark_workflow: str = DEFAULT_BENCHMARK_WORKFLOW,
    ci_workflow: str = DEFAULT_CI_WORKFLOW,
    python_workflow: str = DEFAULT_PYTHON_WORKFLOW,
    runner: Callable[..., subprocess.CompletedProcess[str]] = run_command,
    version_checker: Callable[[str], None] = run_version_check,
) -> None:
    version_checker(tag)
    require_successful_commit_run(repo, ci_workflow, commit, runner=runner)
    require_successful_commit_run(repo, python_workflow, commit, runner=runner)
    require_recent_successful_run_on_main(
        repo,
        benchmark_workflow,
        freshness_days=freshness_days,
        runner=runner,
    )


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Verify release gates before publishing.")
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY"), help="owner/repo")
    parser.add_argument("--commit", default=os.environ.get("GITHUB_SHA"), help="release commit SHA")
    parser.add_argument("--tag", default=os.environ.get("GITHUB_REF_NAME"), help="release tag")
    parser.add_argument(
        "--freshness-days",
        type=int,
        default=DEFAULT_FRESHNESS_DAYS,
        help="maximum age in days for the benchmark workflow run on main",
    )
    parser.add_argument(
        "--benchmark-workflow",
        default=DEFAULT_BENCHMARK_WORKFLOW,
        help="benchmark workflow name to validate",
    )
    parser.add_argument(
        "--ci-workflow",
        default=DEFAULT_CI_WORKFLOW,
        help="CI workflow name to validate",
    )
    parser.add_argument(
        "--python-workflow",
        default=DEFAULT_PYTHON_WORKFLOW,
        help="Python workflow name to validate",
    )
    args = parser.parse_args(list(argv) if argv is not None else None)

    missing = [name for name, value in (("repo", args.repo), ("commit", args.commit), ("tag", args.tag)) if not value]
    if missing:
        print(f"missing required release gate input(s): {', '.join(missing)}", file=sys.stderr)
        return 2

    try:
        verify_release_gates(
            repo=args.repo,
            commit=args.commit,
            tag=args.tag,
            freshness_days=args.freshness_days,
            benchmark_workflow=args.benchmark_workflow,
            ci_workflow=args.ci_workflow,
            python_workflow=args.python_workflow,
        )
    except ReleaseGateError as exc:
        print(f"release gate verification failed: {exc}", file=sys.stderr)
        return 1

    print("release gates verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
