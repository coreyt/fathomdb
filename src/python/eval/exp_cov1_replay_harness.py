#!/usr/bin/env python3
"""EXP-COV-1 replay harness — serves ``fathomdb.extract.v1`` from a cache ($0).

The engine's ``ingest_with_extractor`` spawns an extraction subprocess speaking the
``fathomdb.extract.v1`` NDJSON protocol. This harness answers those requests from a
PRE-COMPUTED extraction cache (produced by the resilient priced runner
``eval.exp_cov1_extract``) instead of calling any LLM — so the sweep's ingest is
deterministic, replayable, and spends nothing. This is the mechanism that keeps the
ONE priced call (the extraction pass) fully decoupled from the engine ingest.

Env:
  COV1_CACHE_PATH   path to the extraction cache NDJSON (required)
  COV1_MODEL        model key the cache was written under (required — cache-key scope)
  COV1_PROMPT_VER   prompt version (default: the module PROMPT_VERSION)

A doc absent from the cache (or present-but-failed) yields empty entities/edges plus a
warning — the ingest still succeeds; coverage for that doc is simply zero.
"""

from __future__ import annotations

import json
import os
import sys
from typing import Any

from eval.exp_cov1_common import PROMPT_VERSION, ExtractionCache, cache_key

_PROTOCOL = "fathomdb.extract.v1"
_SCHEMA_VERSION = 1


def _write(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def main() -> None:
    cache_path = os.environ.get("COV1_CACHE_PATH")
    model = os.environ.get("COV1_MODEL")
    prompt_ver = os.environ.get("COV1_PROMPT_VER", PROMPT_VERSION)
    if not cache_path or not model:
        print("[cov1-replay] COV1_CACHE_PATH and COV1_MODEL required", file=sys.stderr, flush=True)
        raise SystemExit(2)
    cache = ExtractionCache.load(cache_path)
    print(
        f"[cov1-replay] loaded {len(cache.records)} records from {cache_path} "
        f"(model={model} prompt={prompt_ver})",
        file=sys.stderr,
        flush=True,
    )

    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError as exc:
            print(f"[cov1-replay] bad JSON: {exc}", file=sys.stderr, flush=True)
            continue
        mtype = msg.get("type")
        if mtype == "hello":
            _write({
                "protocol": _PROTOCOL, "type": "ready", "schema_version": _SCHEMA_VERSION,
                "provider": "cov1-replay", "model": model, "supports": {},
                "max_docs_per_request": 64,
            })
        elif mtype == "extract":
            all_ent: list[dict] = []
            all_edge: list[dict] = []
            all_warn: list[dict] = []
            for doc in msg.get("documents", []):
                doc_id = doc.get("source_doc_id", "")
                rec = cache.records.get(cache_key(doc_id, model, prompt_ver))
                if rec and rec.get("status") == "ok":
                    all_ent.extend(rec.get("entities", []))
                    all_edge.extend(rec.get("edges", []))
                else:
                    all_warn.append({"kind": "cache_miss", "source_doc_id": doc_id})
            _write({
                "protocol": _PROTOCOL, "type": "result",
                "request_id": msg.get("request_id", ""),
                "entities": all_ent, "edges": all_edge, "warnings": all_warn,
            })
        else:
            print(f"[cov1-replay] unknown type {mtype!r}", file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
