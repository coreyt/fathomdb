"""Footprint-safe LOCAL Mem0-OSS backend config (0.8.3 Slice 5 de-risk output).

``Mem0OSSAdapter.try_build()`` returned None for two reasons (see
``dev/design/0.8.3-slice-5-design.md`` §A): ``mem0ai`` was not installed, and even
when installed ``Memory()`` needs a configured LLM + embedder + vector store (the
default config targets the OpenAI cloud + a Qdrant server). **Mem0 cloud is forbidden
(ADR §3.6).** This module pins the exact LOCAL, $0 backend the D0b stand-up uses, as
code, so the config is reviewable and cannot drift:

* LLM            — the airlock OpenAI-compatible endpoint, model ``qwen3.6-27b``
                   (local vLLM; the currently-loaded local model verified at Slice 5);
* Embedder       — ``huggingface`` ``BAAI/bge-small-en-v1.5`` (CPU, local);
* Vector store   — ``chroma`` (embedded, on-disk; no server).

This module performs NO install and NO network call at import — the unit tests stay
backend-free. The D0b install (``pip install mem0ai chromadb sentence-transformers``)
is deferred to Slice 10 so this eval-infra slice does not pollute the shared ``.venv``.
"""

from __future__ import annotations

import tempfile
import uuid
from pathlib import Path
from typing import Any

#: The local LLM the de-risk verified is loaded on the airlock (qwen3-32b was NOT
#: loaded at Slice 5 — use this one).
LOCAL_LLM_MODEL = "qwen3.6-27b"
#: The airlock OpenAI-compatible base URL (LiteLLM proxy in front of the local vLLM).
LOCAL_BASE_URL = "http://localhost:4000/v1"
#: Same embedder family FathomDB uses, for a fair retrieval axis. CPU/local.
LOCAL_EMBED_MODEL = "BAAI/bge-small-en-v1.5"


def new_run_id() -> str:
    """A fresh per-run id (codex §9 [P1]). Used to namespace the Chroma store and
    derive a non-fixed ``user_id`` so a re-run cannot read stale memories."""
    return uuid.uuid4().hex[:12]


def run_user_id(run_id: str) -> str:
    """Per-run Mem0 ``user_id`` derived from ``run_id`` — NOT a fixed value, so a
    new run searches its own namespace, never a prior run's memories."""
    return f"r2-{run_id}"


def build_local_mem0_config(
    *,
    api_key: str,
    corpus_hash: str | None = None,
    run_id: str | None = None,
    llm_model: str = LOCAL_LLM_MODEL,
    base_url: str = LOCAL_BASE_URL,
    embed_model: str = LOCAL_EMBED_MODEL,
    chroma_root: str | None = None,
) -> dict[str, Any]:
    """Return the Mem0 ``config`` dict for an all-local, $0, no-cloud backend.

    Pass this to ``mem0.Memory.from_config(build_local_mem0_config(...))`` at the
    D0b stand-up. Every provider here is local; there is no Mem0 cloud client
    anywhere (ADR §3.6).

    Per-run isolation (codex §9 [P1#2]): both the Chroma **collection name** and
    the on-disk **path** are keyed by a per-run ``run_id`` (a fresh one is minted
    when omitted) — optionally prefixed by ``corpus_hash`` when one is supplied —
    so re-running on a different corpus/branch cannot reopen a previous run's
    collection and search stale memories. Pair this with :func:`run_user_id`
    (``run_user_id(run_id)``) for the ``add``/``search`` ``user_id``.
    Reproducible-by-construction; no manual ``/tmp/mem0_chroma`` wipe required.

    ``corpus_hash`` is OPTIONAL (codex §9 fix-2 [P1]): the only production caller,
    ``Mem0OSSAdapter.try_build()``, builds this config from runtime env (api_key /
    optional llm_model / base_url) and does NOT know the corpus_hash. The per-run
    ``run_id`` alone guarantees uniqueness; when ``corpus_hash`` is given it is
    folded into the key so a different corpus is also distinct."""
    rid = run_id or new_run_id()
    key = f"{str(corpus_hash)[:12]}_{rid}" if corpus_hash else rid
    if chroma_root is None:
        chroma_root = tempfile.gettempdir()
    chroma_path = str(Path(chroma_root) / f"mem0_chroma_{key}")
    collection_name = f"r2_eval_{key}"
    return {
        "_run_id": rid,
        "_user_id": run_user_id(rid),
        "llm": {
            "provider": "openai",
            "config": {
                "model": llm_model,
                "openai_base_url": base_url,
                "api_key": api_key,
                "temperature": 0,
            },
        },
        "embedder": {
            "provider": "huggingface",
            "config": {"model": embed_model},
        },
        "vector_store": {
            "provider": "chroma",
            "config": {"path": chroma_path, "collection_name": collection_name},
        },
    }
