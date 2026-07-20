"""Unit tests for the `test-hooks` gate policy (TC-27, 0.8.20 Slice 5 fix-6).

These pin the decision table that `conftest.py` acts on. They are pure: no
binding, no venv, no subprocess — so they run identically on a hook-less
editable install, which is precisely the configuration the policy exists to
handle.

Two regressions are locked here:

* fix-4 made a hook-less editable checkout raise at conftest import time, so the
  documented `pip install -e 'src/python[dev]'` + `scripts/agent-test.sh` loop
  was always red. `test_default_editable_checkout_degrades_never_raises` forbids
  any verdict other than DEGRADED on that path.
* The original defect fix-4 was closing: an unattended `maturin develop` firing
  from an agent worktree into the primary checkout's shared `.venv`.
  `test_authorized_rebuild_still_refused_when_venv_is_foreign` and
  `test_worktree_against_shared_primary_venv_is_not_owned` forbid REBUILD there.
"""

from __future__ import annotations

from pathlib import Path

from _test_hooks_gate import (
    CONTRADICTORY,
    DEGRADED,
    PROCEED,
    REBUILD,
    REBUILD_OPT_IN,
    decide,
    venv_belongs_to_source_tree,
)

# The "everything is fine, rebuild is safe" baseline; each test overrides the
# one or two observations it is about.
_SAFE = {
    "is_source_tree": True,
    "hooks_present": False,
    "allow_rebuild": True,
    "forbid_rebuild": False,
    "venv_owned_by_source_tree": True,
}


def _decide(**overrides: bool):
    return decide(**{**_SAFE, **overrides})


def test_authorized_rebuild_in_owned_venv_proceeds_to_rebuild() -> None:
    assert _decide().action == REBUILD


def test_hooks_already_present_short_circuits() -> None:
    """The common warm path: never consults the opt-in at all."""

    for allow in (True, False):
        for owned in (True, False):
            decision = _decide(hooks_present=True, allow_rebuild=allow,
                               venv_owned_by_source_tree=owned)
            assert decision.action == PROCEED


def test_wheel_context_proceeds_without_a_source_tree() -> None:
    """Release-surface tests run the test files against a pip-installed wheel."""

    assert _decide(is_source_tree=False, allow_rebuild=False).action == PROCEED


def test_default_editable_checkout_degrades_never_raises() -> None:
    """codex §9 [P2] regression: the DOCUMENTED default path must stay runnable.

    A clean `pip install -e 'src/python[dev]'` has no hooks and sets no env
    vars. That must degrade (suite runs, marked tests skip), NOT raise.
    """

    decision = _decide(allow_rebuild=False)
    assert decision.action == DEGRADED
    assert decision.is_degraded
    # The message has to tell the reader how to fix it.
    assert REBUILD_OPT_IN in decision.reason
    assert "maturin develop" in decision.reason


def test_authorized_rebuild_still_refused_when_venv_is_foreign() -> None:
    """TC-27: authorization alone does not make a rebuild safe.

    `scripts/agent-test.sh` sets the opt-in automatically, so the ownership
    check — not the env var — is what stops a shared venv being rebound.
    """

    decision = _decide(venv_owned_by_source_tree=False)
    assert decision.action == DEGRADED
    assert "TC-27" in decision.reason


def test_opt_out_degrades_and_is_not_an_error() -> None:
    decision = _decide(allow_rebuild=False, forbid_rebuild=True)
    assert decision.action == DEGRADED


def test_contradictory_opt_in_and_opt_out_is_an_error() -> None:
    assert _decide(allow_rebuild=True, forbid_rebuild=True).action == CONTRADICTORY


def test_no_input_combination_yields_an_unsafe_rebuild() -> None:
    """Exhaustive sweep of the 32-cell decision table: REBUILD requires a source
    tree, absent hooks, explicit authorization, no opt-out, AND an owned venv."""

    for bits in range(32):
        obs = {
            "is_source_tree": bool(bits & 1),
            "hooks_present": bool(bits & 2),
            "allow_rebuild": bool(bits & 4),
            "forbid_rebuild": bool(bits & 8),
            "venv_owned_by_source_tree": bool(bits & 16),
        }
        decision = decide(**obs)
        assert decision.reason, f"every verdict must carry a reason: {obs}"
        if decision.action != REBUILD:
            continue
        assert obs["is_source_tree"]
        assert not obs["hooks_present"]
        assert obs["allow_rebuild"]
        assert not obs["forbid_rebuild"]
        assert obs["venv_owned_by_source_tree"]


def test_in_tree_venv_is_owned() -> None:
    repo = Path("/repo")
    assert venv_belongs_to_source_tree(repo / ".venv", repo / "src" / "python")


def test_worktree_against_shared_primary_venv_is_not_owned() -> None:
    """The exact TC-27 shape: pytest launched from a worktree while the
    interpreter is the primary checkout's shared `.venv`."""

    assert not venv_belongs_to_source_tree(
        "/home/u/projects/fathomdb/.venv",
        "/home/u/projects/fathomdb-worktrees/slice-5/src/python",
    )


def test_system_interpreter_is_not_owned() -> None:
    assert not venv_belongs_to_source_tree("/usr", "/repo/src/python")


def test_real_repo_layout_resolves() -> None:
    """Guards the `parents[1]` hop: `src/python` -> repo root, not `src`."""

    python_src_dir = Path(__file__).resolve().parent.parent
    repo_root = python_src_dir.parents[1]
    assert (repo_root / "src" / "python").resolve() == python_src_dir
    assert venv_belongs_to_source_tree(repo_root / ".venv", python_src_dir)
