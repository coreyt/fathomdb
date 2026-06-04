"""EU-6 FIX-2 — pyright narrowing on the ``EmbedderEvent`` typed union.

Per AC-FIX2-3 (see ``dev/plans/prompts/0.7.1-EU-6-FIX-plan.md``) and
``dev/design/0.7.1-EU-6-FIX-2-design.md`` §5.1, this test runs pyright as
a subprocess over a small fixture
(``_pyright_narrowing_fixture.py``) and asserts the ``reveal_type``
output proves narrowing on ``if event["kind"] == "..."`` branches.

On current ``main``, ``fathomdb.types.EmbedderEvent`` does not exist as a
discriminated TypedDict union — the fixture's import line itself fails
under pyright, so the test fails. FIX-2 GREEN makes this PASS by landing
the TypedDict variants + union.

If ``pyright`` is not on ``$PATH`` the test SKIPs (design §5.1 — pyright
ships in ``[project.optional-dependencies] typecheck``).
"""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

_FIXTURE = (
    Path(__file__).resolve().parents[1]
    / "_typecheck_fixtures"
    / "_pyright_narrowing_fixture.py"
)
# The Python SDK's `pyproject.toml` carries the pyright config
# (pythonVersion = "3.10", include paths). Without `--project`, pyright
# loads the workspace-root cwd defaults and cannot resolve
# `fathomdb.types` on a non-editable checkout.
_PYRIGHT_PROJECT = Path(__file__).resolve().parents[1] / "pyproject.toml"

# Expected ``reveal_type`` outputs. Pyright emits a message of the form
# ``Type of "expr" is "Type"`` as an information diagnostic. We assert
# each expected (expression, type) pair appears in the fixture's
# diagnostics.
_EXPECTED_REVEALS: list[tuple[str, str]] = [
    # DefaultEmbedderDownload branch
    ('event["file"]', "str"),
    ('event["url"]', "str"),
    ('event["bytes"]', "int"),
    ('event["sha256"]', "str"),
    ('event["cache_path"]', "str"),
    ('event["duration_ms"]', "int"),
    # DefaultEmbedderCacheHit branch — file/sha256/cache_path already
    # covered above (same expression text, same expected type), so we
    # don't double-list them.
    # MeanVecPinned branch
    ('event["dim"]', "int"),
    ('event["doc_count"]', "int"),
]


def _run_pyright() -> dict:
    pyright = shutil.which("pyright")
    if pyright is None:
        pytest.skip("pyright not installed; install via `pip install pyright`")

    proc = subprocess.run(
        [
            pyright,
            "--project",
            str(_PYRIGHT_PROJECT),
            "--outputjson",
            str(_FIXTURE),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    # Pyright exits non-zero when diagnostics are present. We always
    # parse the JSON output (which it writes on success and failure).
    if not proc.stdout.strip():
        pytest.fail(
            f"pyright produced no JSON output (exit {proc.returncode}); "
            f"stderr:\n{proc.stderr}"
        )
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        pytest.fail(
            f"could not parse pyright JSON output: {exc}\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )
        raise  # unreachable; satisfies type checkers


def test_pyright_narrows_embedder_event_variants() -> None:
    """Pyright must narrow each branch's payload keys to their concrete
    types. The fixture's ``reveal_type`` calls drive the assertion."""

    report = _run_pyright()
    diagnostics = report.get("generalDiagnostics", [])

    # Collect the ``Type of "..." is "..."`` informational messages.
    reveal_messages: list[str] = [
        d.get("message", "")
        for d in diagnostics
        if d.get("severity") == "information"
        and d.get("message", "").startswith("Type of ")
    ]

    missing: list[str] = []
    for expression, expected_type in _EXPECTED_REVEALS:
        # The exact pyright wording is:
        #   Type of "event["bytes"]" is "int"
        needle = f'Type of "{expression}" is "{expected_type}"'
        if not any(needle in msg for msg in reveal_messages):
            missing.append(needle)

    assert not missing, (
        "pyright did not narrow as expected. Missing reveals:\n  "
        + "\n  ".join(missing)
        + "\n\nAll reveal_type messages pyright produced:\n  "
        + "\n  ".join(reveal_messages or ["<none>"])
    )


def test_pyright_flags_unnarrowed_variant_key_access() -> None:
    """Without narrowing, ``event["bytes"]`` on a raw ``EmbedderEvent``
    must be a type error — the TypedDict key is not present in every
    union member."""

    report = _run_pyright()
    diagnostics = report.get("generalDiagnostics", [])

    errors_on_unsafe = [
        d
        for d in diagnostics
        if d.get("severity") == "error"
        and "bytes" in d.get("message", "")
        # The fixture's marker line:
        and d.get("range", {}).get("start", {}).get("line") is not None
    ]

    # We don't bind on an exact line; we assert that at least one error
    # diagnostic mentions ``bytes`` on the union (the un-narrowed
    # access). On RED — current main — there is no union, so the access
    # is allowed by the `dict[str, Any]` type, and this assertion fails.
    assert errors_on_unsafe, (
        "pyright did not flag un-narrowed `event[\"bytes\"]` access as a "
        "type error. Diagnostics:\n  "
        + "\n  ".join(d.get("message", "") for d in diagnostics)
    )
