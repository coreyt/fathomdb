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

import json
import os
import subprocess
import sys
from pathlib import Path

from _test_hooks_gate import (
    ALL_HOOK_SYMBOL_NAMES,
    CONTRADICTORY,
    DEGRADED,
    PROCEED,
    REBUILD,
    REBUILD_OPT_IN,
    TEST_HOOK_SYMBOLS,
    decide,
    missing_symbols_from_probe,
    partial_binding_note,
    probe_source,
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


# --------------------------------------------------------------------------
# The probe surface (0.8.20 Slice 5 fix-7).
#
# The probe used to check ONLY `Engine._write_vector_for_test`, while the gate
# it drives protects all three `test-hooks` symbols. A binding with the two
# `Engine` methods but no module-level `force_panic_for_test` therefore read as
# "hooks present" -> PROCEED, the `requires_test_hooks` skips did not apply, and
# `test_panic_surfaces_as_python_exception` failed on a missing import instead
# of skipping cleanly. These tests run the probe against SYNTHETIC bindings, so
# they need no compiled extension.
# --------------------------------------------------------------------------


def _fake_binding(root: Path, *, engine_attrs: tuple[str, ...], module_attrs: tuple[str, ...],
                  import_raises: bool = False) -> Path:
    """Materialize a stand-in `fathomdb._fathomdb` exposing exactly these names."""

    pkg = root / "fathomdb"
    pkg.mkdir(parents=True, exist_ok=True)
    (pkg / "__init__.py").write_text("")
    if import_raises:
        (pkg / "_fathomdb.py").write_text("raise ImportError('no compiled extension')\n")
        return root
    body = ["class Engine:", "    pass"]
    body += [f"Engine.{name} = lambda self, *a, **k: None" for name in engine_attrs]
    body += [f"def {name}():\n    return None" for name in module_attrs]
    (pkg / "_fathomdb.py").write_text("\n".join(body) + "\n")
    return root


def _run_probe(root: Path) -> tuple[str, ...]:
    env = dict(os.environ)
    env["PYTHONPATH"] = str(root)
    proc = subprocess.run(
        [sys.executable, "-c", probe_source()],
        cwd=str(root), env=env, capture_output=True, text=True,
    )
    return missing_symbols_from_probe(proc.returncode, proc.stdout)


def test_probe_accepts_a_complete_binding(tmp_path: Path) -> None:
    root = _fake_binding(
        tmp_path,
        engine_attrs=tuple(a for owner, a in TEST_HOOK_SYMBOLS if owner == "Engine"),
        module_attrs=tuple(a for owner, a in TEST_HOOK_SYMBOLS if owner is None),
    )
    assert _run_probe(root) == ()


def test_probe_detects_a_partial_binding(tmp_path: Path) -> None:
    """The observed failure mode: both `Engine` hooks present, module-level
    `force_panic_for_test` absent. Must NOT read as "hooks present"."""

    root = _fake_binding(
        tmp_path,
        engine_attrs=("_configure_vector_kind_for_test", "_write_vector_for_test"),
        module_attrs=(),
    )
    assert _run_probe(root) == ("force_panic_for_test",)


def test_a_partial_binding_degrades_rather_than_proceeds(tmp_path: Path) -> None:
    """End-to-end over the seam conftest uses: probe -> `hooks_present` -> decide."""

    root = _fake_binding(
        tmp_path,
        engine_attrs=("_configure_vector_kind_for_test", "_write_vector_for_test"),
        module_attrs=(),
    )
    missing = _run_probe(root)
    decision = _decide(hooks_present=not missing, allow_rebuild=False)
    assert missing
    assert decision.action == DEGRADED, "a partial binding must never PROCEED"


def test_probe_reports_the_whole_surface_when_the_import_fails(tmp_path: Path) -> None:
    root = _fake_binding(tmp_path, engine_attrs=(), module_attrs=(), import_raises=True)
    assert _run_probe(root) == ALL_HOOK_SYMBOL_NAMES


def test_probe_output_that_cannot_be_parsed_fails_safe() -> None:
    """A crashed or garbled probe means DEGRADED, never PROCEED."""

    for stdout in ("", "Traceback (most recent call last):", "{not json", "[]", "null"):
        assert missing_symbols_from_probe(1, stdout) == ALL_HOOK_SYMBOL_NAMES
    assert missing_symbols_from_probe(0, "") == ()


def test_probe_reader_round_trips_the_json_contract() -> None:
    payload = json.dumps({"missing": ["force_panic_for_test"]})
    assert missing_symbols_from_probe(1, payload) == ("force_panic_for_test",)


def test_partial_note_only_fires_for_a_genuinely_partial_surface() -> None:
    assert partial_binding_note(()) is None
    assert partial_binding_note(ALL_HOOK_SYMBOL_NAMES) is None, (
        "a wholly absent surface is 'built WITHOUT test-hooks', not a partial build"
    )
    note = partial_binding_note(("force_panic_for_test",))
    assert note is not None and "force_panic_for_test" in note


def test_probed_symbols_match_the_rust_cfg_gates() -> None:
    """The probed set must not drift from the `#[cfg(any(test, feature =
    "test-hooks"))]` surface in `fathomdb-py/src/lib.rs`."""

    lib_rs = (
        Path(__file__).resolve().parents[2]
        / "rust" / "crates" / "fathomdb-py" / "src" / "lib.rs"
    )
    lines = lib_rs.read_text().splitlines()
    for _owner, attribute in TEST_HOOK_SYMBOLS:
        declarations = [i for i, line in enumerate(lines) if f"fn {attribute}(" in line]
        assert declarations, f"{attribute} is no longer declared in lib.rs"
        assert any(
            'feature = "test-hooks"' in line
            for i in declarations
            for line in lines[max(0, i - 4) : i]
        ), f"{attribute} is probed but no longer gated by the `test-hooks` feature"
