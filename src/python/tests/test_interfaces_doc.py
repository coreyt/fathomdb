"""EU-6 FIX-1 — interface-doc content assertions (RED).

AC-FIX1-10: ``dev/interfaces/python.md`` and ``dev/interfaces/typescript.md``
must state explicitly that released wheels/binaries are compiled with
``default-embedder``, AND that the dev-only ``test-hooks`` surface does
not ship to consumers.

Regex matches are intentionally tolerant of small wording drift but
require the load-bearing phrases.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
PYTHON_INTERFACE_DOC = REPO_ROOT / "dev" / "interfaces" / "python.md"
TS_INTERFACE_DOC = REPO_ROOT / "dev" / "interfaces" / "typescript.md"

# "compiled with `default-embedder`" (tolerant of backticks / wording).
COMPILED_WITH_DEFAULT_EMBEDDER = re.compile(
    r"compiled with [^\n]{0,40}default-embedder", re.IGNORECASE
)
# "test-hooks ... do not ship" / "never installed" / similar.
TEST_HOOKS_NEVER_SHIPS = re.compile(
    r"test-hooks[^\n]{0,80}(do not|don't|never)[^\n]{0,40}(ship|exist|installed|present)",
    re.IGNORECASE,
)


@pytest.mark.parametrize("doc_path", [PYTHON_INTERFACE_DOC, TS_INTERFACE_DOC])
def test_interface_doc_documents_default_embedder_compiled_in(doc_path: Path) -> None:
    text = doc_path.read_text(encoding="utf-8")
    assert COMPILED_WITH_DEFAULT_EMBEDDER.search(text), (
        f"{doc_path} is missing a 'compiled with default-embedder' statement "
        f"required by AC-FIX1-10."
    )


@pytest.mark.parametrize("doc_path", [PYTHON_INTERFACE_DOC, TS_INTERFACE_DOC])
def test_interface_doc_documents_test_hooks_never_ships(doc_path: Path) -> None:
    text = doc_path.read_text(encoding="utf-8")
    assert TEST_HOOKS_NEVER_SHIPS.search(text), (
        f"{doc_path} is missing a 'test-hooks does not ship / never installed' "
        f"clarification required by AC-FIX1-10."
    )
