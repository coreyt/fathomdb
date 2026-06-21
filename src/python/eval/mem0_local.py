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

from typing import Any

#: The local LLM the de-risk verified is loaded on the airlock (qwen3-32b was NOT
#: loaded at Slice 5 — use this one).
LOCAL_LLM_MODEL = "qwen3.6-27b"
#: The airlock OpenAI-compatible base URL (LiteLLM proxy in front of the local vLLM).
LOCAL_BASE_URL = "http://localhost:4000/v1"
#: Same embedder family FathomDB uses, for a fair retrieval axis. CPU/local.
LOCAL_EMBED_MODEL = "BAAI/bge-small-en-v1.5"


def build_local_mem0_config(
    *,
    api_key: str,
    llm_model: str = LOCAL_LLM_MODEL,
    base_url: str = LOCAL_BASE_URL,
    embed_model: str = LOCAL_EMBED_MODEL,
    chroma_path: str = "/tmp/mem0_chroma",
    collection_name: str = "r2_eval",
) -> dict[str, Any]:
    """Return the Mem0 ``config`` dict for an all-local, $0, no-cloud backend.

    Pass this to ``mem0.Memory.from_config(build_local_mem0_config(api_key=...))`` at
    the D0b stand-up. Every provider here is local; there is no Mem0 cloud client
    anywhere (ADR §3.6)."""
    return {
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
