from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "verify-release-gates.py"


def load_script_module():
    spec = importlib.util.spec_from_file_location("verify_release_gates", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def make_run(
    *,
    conclusion: str = "success",
    head_branch: str = "main",
    head_sha: str = "deadbeef",
    status: str = "completed",
    updated_at: datetime,
    url: str = "https://example.invalid/run",
) -> dict[str, str]:
    return {
        "conclusion": conclusion,
        "headBranch": head_branch,
        "headSha": head_sha,
        "status": status,
        "updatedAt": updated_at.astimezone(timezone.utc).isoformat().replace("+00:00", "Z"),
        "url": url,
    }


def test_verify_release_gates_accepts_recent_successes() -> None:
    module = load_script_module()
    commit = "abc123"
    tag = "v0.1.0"
    seen_tags: list[str] = []
    now = datetime.now(timezone.utc)

    responses = {
        "CI": [make_run(head_sha=commit, updated_at=now - timedelta(hours=1))],
        "Python": [make_run(head_sha=commit, updated_at=now - timedelta(hours=2))],
        module.DEFAULT_BENCHMARK_WORKFLOW: [
            make_run(head_sha="main-sha", head_branch="main", updated_at=now - timedelta(days=2))
        ],
    }

    def runner(args, cwd=None):
        workflow = args[args.index("--workflow") + 1]
        expected = responses[workflow]
        # gh_run_list() filters commit / branch client-side (to stay
        # portable across older gh versions that lack --commit /
        # --branch flags on `gh run list`). The script therefore
        # never passes those flags to gh; assert absence instead
        # of presence so a regression that resurrects server-side
        # filtering gets caught here rather than at preflight time.
        assert "--commit" not in args
        assert "--branch" not in args
        return subprocess.CompletedProcess(args, 0, stdout=json.dumps(expected), stderr="")

    def version_checker(received_tag: str) -> None:
        seen_tags.append(received_tag)

    module.verify_release_gates(
        repo="coreyt/fathomdb",
        commit=commit,
        tag=tag,
        freshness_days=10,
        runner=runner,
        version_checker=version_checker,
    )

    assert seen_tags == [tag]


def test_verify_release_gates_rejects_stale_benchmark_run() -> None:
    module = load_script_module()
    commit = "abc123"
    tag = "v0.1.0"
    stale_run = make_run(
        head_sha="main-sha",
        head_branch="main",
        updated_at=datetime.now(timezone.utc) - timedelta(days=11),
    )

    def runner(args, cwd=None):
        workflow = args[args.index("--workflow") + 1]
        if workflow in {"CI", "Python"}:
            payload = [make_run(head_sha=commit, updated_at=datetime.now(timezone.utc))]
        else:
            payload = [stale_run]
        return subprocess.CompletedProcess(args, 0, stdout=json.dumps(payload), stderr="")

    with pytest.raises(module.ReleaseGateError, match="too old"):
        module.verify_release_gates(
            repo="coreyt/fathomdb",
            commit=commit,
            tag=tag,
            freshness_days=10,
            runner=runner,
            version_checker=lambda _: None,
        )
