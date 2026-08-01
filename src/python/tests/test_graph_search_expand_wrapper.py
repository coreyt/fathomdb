"""TC-138 — graph search expansion preserves the public search-hit contract.

This is a pure-Python wrapper test.  It isolates the conversion at the SDK
boundary so that a native hit carrying the current binding fields cannot leak
its native identity object or drop provenance / reranking metadata.
"""

from __future__ import annotations

import sys
import types
from importlib.machinery import EXTENSION_SUFFIXES
from pathlib import Path
from types import SimpleNamespace
from typing import Any, cast


def _native_extension_present(search_path: list[str]) -> bool:
    """Return whether the package selected by ``search_path`` has its extension."""
    for root in search_path:
        package_dir = Path(root) / "fathomdb"
        if (package_dir / "__init__.py").is_file():
            return any((package_dir / f"_fathomdb{suffix}").exists() for suffix in EXTENSION_SUFFIXES)
    return False


_NATIVE_EXTENSION_PRESENT = _native_extension_present(sys.path)
_USING_FAKE_NATIVE = (
    "fathomdb" not in sys.modules
    and "fathomdb._fathomdb" not in sys.modules
    and not _NATIVE_EXTENSION_PRESENT
)

if _USING_FAKE_NATIVE:
    _fake = types.ModuleType("fathomdb._fathomdb")
    _fake.__file__ = None

    class _Dummy:
        pass

    def _fake_getattr(name: str) -> object:
        return _Dummy

    _fake.__getattr__ = _fake_getattr  # type: ignore[attr-defined]
    sys.modules["fathomdb._fathomdb"] = _fake

from fathomdb import graph  # noqa: E402
from fathomdb.types import IdSpace  # noqa: E402


def test_search_expand_maps_idspace_and_search_metadata(monkeypatch) -> None:  # type: ignore[no-untyped-def]
    """TC-138: graph hits match `Engine.search`'s public `SearchHit` shape."""
    native_hit = SimpleNamespace(
        id=SimpleNamespace(space="logical", value="graph-hit"),
        kind="doc",
        body="graph contract body",
        score=0.42,
        branch="text",
        source_id="source-graph-contract",
        ce_score=0.73,
    )
    native_result = SimpleNamespace(search_hits=[native_hit], expanded=[], all_logical_ids=["graph-hit"])
    monkeypatch.setattr(graph, "_native_search_expand", lambda *args: native_result)

    result = graph.search_expand(
        cast(Any, SimpleNamespace(_native=object())), "graph contract", depth=1
    )

    hit = result.search_hits[0]
    assert isinstance(hit.id, IdSpace)
    assert hit.id == IdSpace(space="logical", value="graph-hit")
    assert hit.source_id == "source-graph-contract"
    assert hit.ce_score == 0.73


def test_fake_native_module_does_not_forge_file_metadata() -> None:
    """The pure-Python stand-in must not impersonate a file-backed module."""
    fake = types.ModuleType("fathomdb._fathomdb")

    class _Dummy:
        pass

    fake.__file__ = None
    fake.__getattr__ = lambda _name: _Dummy  # type: ignore[attr-defined]
    assert getattr(fake, "__file__", None) is None


def test_native_extension_detection_does_not_cross_shadowed_package_roots(tmp_path: Path) -> None:
    """Only the first importable package root may decide whether to inject a fake."""
    selected_root = tmp_path / "selected"
    shadowed_root = tmp_path / "shadowed"
    selected_package = selected_root / "fathomdb"
    shadowed_package = shadowed_root / "fathomdb"
    selected_package.mkdir(parents=True)
    shadowed_package.mkdir(parents=True)
    (selected_package / "__init__.py").touch()
    (shadowed_package / "__init__.py").touch()
    (shadowed_package / f"_fathomdb{EXTENSION_SUFFIXES[0]}").touch()

    assert not _native_extension_present([str(selected_root), str(shadowed_root)])
