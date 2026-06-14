#!/usr/bin/env python3
"""Memory-class QA gold generator for R2 parity eval.

Generates temporal / multi_hop / knowledge_update / multi_session QA pairs
from a corpus sample using claude-sonnet (or any compatible model) via an
OpenAI-compatible /chat/completions endpoint (e.g. the airlock).

Usage:
    python src/python/eval/gold_gen.py \\
      --docs-dir <raw-dir>       \\
      --limit 100                \\
      --out <path>               \\
      --base-url http://localhost:4000/v1 \\
      --api-key sk-airlock-mk    \\
      --model claude-sonnet
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Counter (module-level; single-threaded script only)
# ---------------------------------------------------------------------------

_QUERY_COUNTER = 0


def _next_query_id() -> str:
    global _QUERY_COUNTER
    _QUERY_COUNTER += 1
    return f"mc-{_QUERY_COUNTER:03d}"


# ---------------------------------------------------------------------------
# System / user prompts
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT = """\
You are a question-generation assistant for evaluating memory retrieval systems.
Generate questions that require temporal reasoning, multi-hop inference, or
knowledge-update awareness to answer correctly. Focus on questions whose
answers require retrieving specific documents.
Return ONLY valid JSON. No prose."""

_USER_TEMPLATE = """\
Below are {n} documents. Generate memory-class QA pairs for these documents.
Generate 1-2 questions per class where possible.

Classes to generate:
- "temporal": questions about WHEN something happened (requires a specific date or time period in the answer)
- "multi_hop": questions requiring information from TWO OR MORE of these documents combined
- "knowledge_update": questions where the answer in one document SUPERSEDES or CONTRADICTS an earlier doc
- "multi_session": questions that make sense only when reading across multiple separate conversations or sources

For each question:
- "query_id": unique string like "mc-001"
- "query": the question text
- "query_class": one of the four classes above
- "required_evidence": list of {{"doc_id": "<id>"}} for each doc needed to answer
- "answers": list of short answer strings (1-3 words or a date)

Return:
{{
  "queries": [
    {{
      "query_id": "mc-001",
      "query": "When did Alice Smith join Acme Corp?",
      "query_class": "temporal",
      "required_evidence": [{{"doc_id": "doc-001"}}],
      "answers": ["March 2023", "2023-03"]
    }},
    ...
  ]
}}

Documents:
{docs_block}"""


# ---------------------------------------------------------------------------
# Document loading
# ---------------------------------------------------------------------------


def _load_documents(raw_dir: Path, limit: int) -> dict[str, str]:
    """Load up to `limit` documents from *.jsonl files in raw_dir."""
    docs: dict[str, str] = {}
    for jsonl in sorted(raw_dir.glob("*.jsonl")):
        if len(docs) >= limit:
            break
        with jsonl.open(encoding="utf-8") as fh:
            for line in fh:
                if len(docs) >= limit:
                    break
                line = line.strip()
                if not line:
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                doc_id = rec.get("doc_id") or rec.get("id")
                if doc_id is None:
                    continue
                docs[str(doc_id)] = str(rec.get("body", ""))
    return docs


# ---------------------------------------------------------------------------
# LLM call
# ---------------------------------------------------------------------------


def _strip_fences(text: str) -> str:
    text = text.strip()
    if text.startswith("```"):
        lines = text.splitlines()
        inner = lines[1:-1] if lines[-1].strip() == "```" else lines[1:]
        text = "\n".join(inner).strip()
    return text


def _call_llm(
    system: str,
    user: str,
    *,
    base_url: str,
    api_key: str,
    model: str,
    timeout: float = 120.0,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_tokens": 4096,
        "temperature": 0,
        "response_format": {"type": "json_object"},
    }

    try:
        import httpx  # type: ignore[import-not-found]

        with httpx.Client(timeout=timeout) as client:
            resp = client.post(
                base_url.rstrip("/") + "/chat/completions",
                json=payload,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {api_key}",
                },
            )
            resp.raise_for_status()
            body = resp.json()
    except ImportError:
        import urllib.request  # noqa: PLC0415

        encoded = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            base_url.rstrip("/") + "/chat/completions",
            data=encoded,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {api_key}",
            },
        )
        with urllib.request.urlopen(req, timeout=int(timeout)) as r:  # noqa: S310
            body = json.loads(r.read().decode("utf-8"))

    text = body["choices"][0]["message"]["content"]
    return json.loads(_strip_fences(text))


# ---------------------------------------------------------------------------
# Gold generation
# ---------------------------------------------------------------------------

_MEMORY_CLASSES = frozenset(["temporal", "multi_hop", "knowledge_update", "multi_session"])


def _generate_batch(
    batch: list[tuple[str, str]],
    *,
    base_url: str,
    api_key: str,
    model: str,
) -> list[dict[str, Any]]:
    """Generate QA pairs for a batch of (doc_id, body) pairs."""
    docs_block = "\n".join(f"--- doc_id: {did}\n{body}" for did, body in batch)
    user = _USER_TEMPLATE.format(n=len(batch), docs_block=docs_block)
    try:
        result = _call_llm(
            _SYSTEM_PROMPT,
            user,
            base_url=base_url,
            api_key=api_key,
            model=model,
        )
        return result.get("queries", [])
    except Exception as exc:  # noqa: BLE001
        print(f"[gold_gen] batch generation failed: {exc}", file=sys.stderr, flush=True)
        return []


def _deduplicate(queries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Remove near-duplicate queries (same normalized text)."""
    seen: set[str] = set()
    out: list[dict[str, Any]] = []
    for q in queries:
        key = re.sub(r"\s+", " ", str(q.get("query", "")).lower().strip())
        if key and key not in seen:
            seen.add(key)
            out.append(q)
    return out


def generate_memory_gold(
    docs: dict[str, str],
    *,
    base_url: str,
    api_key: str,
    model: str,
    batch_size: int = 6,
) -> tuple[list[dict[str, Any]], int]:
    """Generate memory-class QA pairs for the given docs.

    Returns (queries, n_failed_batches).
    """
    items = list(docs.items())
    all_queries: list[dict[str, Any]] = []
    n_failed = 0

    for start in range(0, len(items), batch_size):
        batch = items[start : start + batch_size]
        queries = _generate_batch(batch, base_url=base_url, api_key=api_key, model=model)
        if not queries:
            n_failed += 1
        all_queries.extend(queries)

    # Deduplicate and assign fresh sequential query_ids
    deduped = _deduplicate(all_queries)
    for q in deduped:
        q["query_id"] = _next_query_id()

    return deduped, n_failed


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Memory-class QA gold generator")
    parser.add_argument("--docs-dir", required=True, help="Directory of .jsonl corpus files")
    parser.add_argument("--limit", type=int, default=100, help="Max docs to sample")
    parser.add_argument("--out", required=True, help="Output gold JSON file")
    parser.add_argument(
        "--base-url",
        default="http://localhost:4000/v1",
        help="OpenAI-compatible base URL",
    )
    parser.add_argument("--api-key", default="sk-airlock-mk")
    parser.add_argument("--model", default="claude-sonnet")
    parser.add_argument("--batch-size", type=int, default=6, help="Docs per LLM call")
    args = parser.parse_args(argv)

    raw_dir = Path(args.docs_dir)
    if not raw_dir.exists():
        print(f"[gold_gen] ERROR: --docs-dir does not exist: {raw_dir}", file=sys.stderr)
        return 1

    print(f"[gold_gen] loading up to {args.limit} docs from {raw_dir}", file=sys.stderr, flush=True)
    docs = _load_documents(raw_dir, args.limit)
    print(f"[gold_gen] loaded {len(docs)} documents", file=sys.stderr, flush=True)

    if not docs:
        print("[gold_gen] ERROR: no documents found", file=sys.stderr)
        return 1

    queries, n_failed = generate_memory_gold(
        docs,
        base_url=args.base_url,
        api_key=args.api_key,
        model=args.model,
        batch_size=args.batch_size,
    )

    if n_failed > 0:
        print(f"[gold_gen] WARNING: {n_failed} batches failed", file=sys.stderr, flush=True)

    output = {
        "version": "memory-class-gen-v1",
        "generator_model": args.model,
        "n_docs_sampled": len(docs),
        "queries": queries,
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(output, indent=2), encoding="utf-8")
    print(f"[gold_gen] wrote {len(queries)} queries to {out_path}", file=sys.stderr, flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
