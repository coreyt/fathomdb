"""Slice 10 / Phase-B — persistent + resumable corpus-keyed Mem0 ingest.

Backend-free: asserts on :func:`eval.mem0_local.build_local_mem0_config` and the
:class:`eval.r2_parity_eval.Mem0OSSAdapter` sidecar ingest logic directly with a
FAKE memory object that only records ``.add()`` calls (no ``mem0``, no chroma, no
airlock). Proves the OPT-IN persistence is:

* **corpus-keyed + deterministic** — ``persist=True`` + a ``corpus_hash`` yields a
  path/collection/user_id keyed by the corpus hash ALONE (no ``run_id``); two builds
  with the same hash are byte-identical; a different hash differs.
* **resumable + idempotent** — the ``<chroma_path>.ingested.json`` sidecar lets a
  second ingest ADD only the new doc_ids (already-ingested ones skipped), so a
  killed/relaunched ingest continues instead of re-paying for every doc.
* **non-regressing** — ``persist=False`` (the default, the codex Slice-5 [P1#2]
  per-run isolation) is byte-unchanged: a fresh ``run_id`` namespaces every build.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from eval.mem0_local import build_local_mem0_config
from eval.r2_parity_eval import Mem0OSSAdapter

CH = "1859817a32bca532b1bbd5238be05b955b57620bcb5667517a5976765e9ec4df"
CH2 = "ffffffff32bca532b1bbd5238be05b955b57620bcb5667517a5976765e9ec4df"


# --------------------------------------------------------------------------- #
# (a) corpus-keyed deterministic config in persist mode
# --------------------------------------------------------------------------- #
def test_persist_config_is_corpus_keyed_no_run_id() -> None:
    cfg = build_local_mem0_config(api_key="k", corpus_hash=CH, persist=True)
    ch12 = CH[:12]
    path = cfg["vector_store"]["config"]["path"]
    coll = cfg["vector_store"]["config"]["collection_name"]
    # keyed by the corpus hash ALONE — no 12-hex run_id appended
    assert path.endswith(f"mem0_chroma_{ch12}")
    assert coll == f"r2_eval_{ch12}"
    assert cfg["_user_id"] == f"r2-{ch12}"
    # the user_id the adapter searches must match the config namespace
    assert cfg["_persist"] is True
    assert cfg["_chroma_path"] == path


def test_persist_config_deterministic_same_hash_identical() -> None:
    a = build_local_mem0_config(api_key="k", corpus_hash=CH, persist=True)
    b = build_local_mem0_config(api_key="k", corpus_hash=CH, persist=True)
    for key in ("_user_id", "_chroma_path"):
        assert a[key] == b[key]
    assert a["vector_store"] == b["vector_store"]


def test_persist_config_different_hash_differs() -> None:
    a = build_local_mem0_config(api_key="k", corpus_hash=CH, persist=True)
    b = build_local_mem0_config(api_key="k", corpus_hash=CH2, persist=True)
    assert a["vector_store"]["config"]["path"] != b["vector_store"]["config"]["path"]
    assert a["_user_id"] != b["_user_id"]


def test_persist_requires_corpus_hash() -> None:
    with pytest.raises(ValueError):
        build_local_mem0_config(api_key="k", persist=True)


def test_default_non_persist_keeps_per_run_isolation() -> None:
    # codex Slice-5 [P1#2]: default is a fresh per-run run_id → two builds differ
    a = build_local_mem0_config(api_key="k", corpus_hash=CH)
    b = build_local_mem0_config(api_key="k", corpus_hash=CH)
    assert a["vector_store"]["config"]["path"] != b["vector_store"]["config"]["path"]
    assert a["_user_id"] != b["_user_id"]
    assert a.get("_persist") is False


# --------------------------------------------------------------------------- #
# (b) sidecar ingest is resumable + idempotent
# --------------------------------------------------------------------------- #
class _FakeMemory:
    """Records every ``.add()`` — no chroma, no LLM, no network."""

    def __init__(self) -> None:
        self.added: list[str] = []

    def add(self, body: str, user_id: str, metadata: dict) -> None:  # noqa: ANN001
        self.added.append(str(metadata["doc_id"]))


def test_sidecar_ingest_adds_only_new_docs(tmp_path: Path) -> None:
    chroma_path = str(tmp_path / "mem0_chroma_abc")
    sidecar = Path(chroma_path + ".ingested.json")

    mem = _FakeMemory()
    adapter = Mem0OSSAdapter(
        memory=mem, user_id="r2-abc", chroma_path=chroma_path, persist=True
    )
    adapter.ingest({"d1": "body1", "d2": "body2"})
    assert mem.added == ["d1", "d2"]
    assert set(json.loads(sidecar.read_text())) == {"d1", "d2"}

    # Second build (simulating a relaunch) over a superset — only d3 is new.
    mem2 = _FakeMemory()
    adapter2 = Mem0OSSAdapter(
        memory=mem2, user_id="r2-abc", chroma_path=chroma_path, persist=True
    )
    adapter2.ingest({"d1": "body1", "d2": "body2", "d3": "body3"})
    assert mem2.added == ["d3"]  # d1,d2 skipped (resumed)
    assert set(json.loads(sidecar.read_text())) == {"d1", "d2", "d3"}


def test_non_persist_ingest_writes_no_sidecar_and_adds_all(tmp_path: Path) -> None:
    chroma_path = str(tmp_path / "mem0_chroma_run")
    mem = _FakeMemory()
    adapter = Mem0OSSAdapter(memory=mem, user_id="r2-run")  # persist defaults False
    adapter.ingest({"d1": "b1", "d2": "b2"})
    adapter.ingest({"d1": "b1", "d2": "b2"})  # no sidecar → re-adds all
    assert mem.added == ["d1", "d2", "d1", "d2"]
    assert not Path(chroma_path + ".ingested.json").exists()
