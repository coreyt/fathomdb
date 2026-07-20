"""Pure decision logic for the dev-only `test-hooks` binding gate (TC-27).

`pyproject.toml [tool.maturin] features` deliberately ships only the
release feature set, so an editable install built by the documented
`pip install -e 'src/python[dev]'` has NO `test-hooks` surface
(`Engine._write_vector_for_test`, `force_panic_for_test`, ...). A handful
of tests need it.

`conftest.py` may rebuild the binding with `maturin develop`, but that
command **rebinds the active virtualenv to the source tree it is run
from**. Firing it unattended from an agent worktree would silently
repoint every other consumer of a SHARED `.venv` at unreviewed code —
that is TC-27.

The policy is therefore a three-way decision, kept here as a **pure
function** so it is unit-testable without a binding, a venv, or a
subprocess (see `test_test_hooks_gate.py`):

* ``PROCEED``  — nothing to do (hooks present, or not a source checkout).
* ``REBUILD``  — a rebuild is both authorized and safe; run it.
* ``DEGRADED`` — hooks are missing and a rebuild is not authorized/not
  safe. The suite still runs; the tests that need the hook surface are
  SKIPPED with `reason`, never silently passed.
* ``CONTRADICTORY`` — the environment both requests and forbids a
  rebuild. A configuration error; raise.

`DEGRADED` is what keeps the default `pytest` path green-but-honest: a
clean editable checkout collects and runs, and the hook-dependent tests
report as real skips carrying `reason`.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

#: Opt in to letting conftest run `maturin develop` for you.
REBUILD_OPT_IN = "FATHOMDB_TESTS_ALLOW_REBUILD"

#: Legacy opt-out. Predates `REBUILD_OPT_IN`; now merely reasserts the default.
REBUILD_OPT_OUT = "FATHOMDB_TESTS_NO_REBUILD"

PROCEED = "proceed"
REBUILD = "rebuild"
DEGRADED = "degraded"
CONTRADICTORY = "contradictory"

#: Marker applied to tests that need the `test-hooks` surface. Registered in
#: `pyproject.toml [tool.pytest.ini_options] markers`.
REQUIRES_HOOKS_MARKER = "requires_test_hooks"

_MANUAL_BUILD_HINT = (
    "python -m maturin develop --features "
    "pyo3/extension-module,test-hooks,default-embedder,default-reranker"
)


@dataclass(frozen=True)
class Decision:
    """What `conftest.py` should do about a missing `test-hooks` surface."""

    action: str
    reason: str

    @property
    def is_degraded(self) -> bool:
        return self.action == DEGRADED


def venv_belongs_to_source_tree(venv_prefix: str | Path, python_src_dir: str | Path) -> bool:
    """True if the running interpreter's environment lives INSIDE the repo that
    owns `python_src_dir` (i.e. `src/python`).

    This is the TC-27 test, stated positively: `maturin develop` rebinds
    `venv_prefix` to `python_src_dir`. That is only ever intended when the two
    already belong together. A venv rooted anywhere else — the canonical
    example being the primary checkout's `/…/fathomdb/.venv` while
    `python_src_dir` is `/…/fathomdb-worktrees/<slice>/src/python` — is a
    SHARED environment we must not silently repoint.

    A system interpreter (`sys.prefix == /usr`) is likewise "not ours" and
    correctly returns False.
    """

    repo_root = Path(python_src_dir).resolve().parents[1]
    try:
        Path(venv_prefix).resolve().relative_to(repo_root)
    except ValueError:
        return False
    return True


def decide(
    *,
    is_source_tree: bool,
    hooks_present: bool,
    allow_rebuild: bool,
    forbid_rebuild: bool,
    venv_owned_by_source_tree: bool,
) -> Decision:
    """Pure `test-hooks` policy. No I/O, no environment access, no subprocess.

    Callers supply the five observations; `conftest.py` gathers them.
    """

    if not is_source_tree:
        return Decision(
            PROCEED,
            "not an editable-install checkout (release-surface tests run against a "
            "pip-installed wheel); nothing to build",
        )
    if hooks_present:
        return Decision(PROCEED, "the installed binding already exposes the test-hooks surface")
    if allow_rebuild and forbid_rebuild:
        return Decision(
            CONTRADICTORY,
            f"contradictory configuration: {REBUILD_OPT_IN}=1 asks for a rebuild while "
            f"{REBUILD_OPT_OUT}=1 forbids one. Unset one of them.",
        )
    if forbid_rebuild:
        return Decision(
            DEGRADED,
            f"the installed `fathomdb` binding was built WITHOUT `test-hooks`, and "
            f"{REBUILD_OPT_OUT}=1 forbids rebuilding it. "
            f"Build it yourself to run these: {_MANUAL_BUILD_HINT}",
        )
    if not allow_rebuild:
        return Decision(
            DEGRADED,
            "the installed `fathomdb` binding was built WITHOUT `test-hooks`, and this "
            "conftest will not rebuild it unattended: `maturin develop` rebinds the active "
            "virtualenv to this source tree, which silently repoints every other consumer "
            "of a SHARED venv (TC-27).\n"
            f"  * authorize it here, from an environment you intend to rebind:  "
            f"{REBUILD_OPT_IN}=1 pytest ...\n"
            f"  * or build it yourself:  {_MANUAL_BUILD_HINT}\n"
            "  * `scripts/agent-test.sh` authorizes this automatically when it runs the "
            "in-tree `.venv`.",
        )
    if not venv_owned_by_source_tree:
        return Decision(
            DEGRADED,
            f"{REBUILD_OPT_IN}=1 authorized a rebuild, but the active Python environment "
            "does NOT live inside this source tree, so `maturin develop` would rebind a "
            "SHARED or system environment to this checkout (TC-27). Refusing.\n"
            "  * run the tests from a venv created inside this checkout, or\n"
            f"  * build it yourself from the tree that owns that venv:  {_MANUAL_BUILD_HINT}",
        )
    return Decision(
        REBUILD,
        "authorized, and the active virtualenv belongs to this source tree",
    )


def skip_reason(reason: str) -> str:
    """Render `Decision.reason` as the skip message attached to hook-dependent tests."""

    return f"test-hooks surface unavailable — {reason}"
