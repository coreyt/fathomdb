#!/usr/bin/env python3
"""ELPS live harness — fathomdb.extract.v1 subprocess protocol.

Spawned by the FathomDB engine (or tests) as a child process. Communicates
over stdin/stdout via NDJSON (one JSON object per line). Diagnostics go to stderr.

Environment variables:
  ELPS_LLM_BASE_URL   (default: http://localhost:4000/v1)
  ELPS_LLM_API_KEY    (default: sk-airlock-mk)
  ELPS_LLM_MODEL      (default: claude-haiku)
  ELPS_STUB_MODE      (default: 0; set to 1 to return canned responses without LLM)
"""

from __future__ import annotations

import json
import os
import sys
from typing import Any

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

_BASE_URL = os.environ.get("ELPS_LLM_BASE_URL", "http://localhost:4000/v1")
_API_KEY = os.environ.get("ELPS_LLM_API_KEY", "sk-airlock-mk")
_MODEL = os.environ.get("ELPS_LLM_MODEL", "claude-haiku")
_STUB_MODE = os.environ.get("ELPS_STUB_MODE", "0") == "1"
_MAX_DOCS_PER_REQUEST = 8
_PROTOCOL = "fathomdb.extract.v1"
_SCHEMA_VERSION = 1

# ---------------------------------------------------------------------------
# Memex extraction system prompt (verbatim from §3)
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT = """\
You are an information-extraction engine. From the single document supplied by the user, extract knowledge as a graph of entities and directed, dated fact-edges, and return it as structured JSON matching the required schema.

ENTITIES
- Identify the real-world entities mentioned (people, organizations, places, artifacts, concepts, etc.).
- Each entity has a `name`, a `type`, optional `aliases` (other surface forms used for the same entity in this document), and a `source_doc_id`.
- Be conservative about identity: when two mentions MIGHT refer to different real-world entities, emit them as DISTINCT entities rather than merging them. Do not collapse homonyms.

ATTRIBUTION (load-bearing — a missing value fails the whole ingest)
- EVERY entity AND every edge MUST carry a `source_doc_id` set to the exact `doc_id` given in the user message. Copy it verbatim; never invent, abbreviate or alter it.
- This is how the store knows which document each fact came from, and therefore which facts to delete when that document is erased. An element without it cannot be attributed and is REJECTED — the store will not guess an attribution, because guessing would file the fact under the wrong document and leave it behind on erasure.

EDGES (directed, dated facts)
- Each edge is a single fact: `from_entity` -> `to_entity` with a short, lowercase `relation` label (e.g. `works_at`, `founded`, `located_in`).
- `body` is the minimal span of source text that states the fact.
- Reference entities by the exact `name` you used in the entities list.

TEMPORAL RULES (load-bearing)
- `t_valid` is the time the fact BECAME TRUE as stated in the text. If the text states no such time, use the document's `created_at`. NEVER use the current time or ingestion time ("now").
- Set `t_invalid` ONLY when the text gives explicit evidence the fact ended; otherwise leave it null.

SUPERSEDES (contradicting / replacing an earlier fact)
- When a fact in this document SUPERSEDES or CONTRADICTS an earlier fact (e.g. a new role, status, or value that replaces a prior one), emit the NEW fact with its own `t_valid` and set `supersedes_prior` to true, with a short `prior_body` describing the prior fact it replaces.
- Do NOT delete or invalidate the prior fact yourself — only emit the new fact and flag that it supersedes a prior one.

CONFIDENCE
- `confidence` in [0,1] is the calibrated probability that the fact is correct AND supported by the cited span. Emit ALL facts you find — do not self-censor low-confidence facts behind a threshold; report the low confidence instead.

SOURCE SPANS
- When you are confident, provide `source_span` as [start, end) character offsets into the supplied document body that locate the supporting text. If you cannot locate it reliably, omit `source_span`.

Return ONLY valid JSON matching this exact schema — no markdown fences, no prose:
{
  "entities": [{"name": str, "type": str, "aliases": list[str], "source_doc_id": str}],
  "edges": [
    {
      "from_entity": str,
      "to_entity": str,
      "relation": str,
      "body": str,
      "t_valid": str,
      "t_invalid": str | null,
      "confidence": float,
      "source_doc_id": str,
      "source_span": [int, int] | null
    }
  ],
  "warnings": []
}"""

_USER_TEMPLATE = """\
Extract from this single document.
doc_id: {doc_id}
kind: doc
created_at: {created_at}
body:
{body}"""

# ---------------------------------------------------------------------------
# Canned stub response for ELPS_STUB_MODE=1
# ---------------------------------------------------------------------------

def _stub_entities(doc_id: str) -> list[dict[str, Any]]:
    """Canned entities, attributed to `doc_id`.

    A FUNCTION returning fresh dicts, not a module-level constant: the engine
    batches several documents into one extract request, and a shared list would
    be handed out (and then attributed) once per document, so the last document
    processed would silently overwrite every earlier document's provenance.
    """
    return [{"name": "TestEntity", "type": "concept", "aliases": [], "source_doc_id": doc_id}]


def _stub_edges(doc_id: str) -> list[dict[str, Any]]:
    return [
        {
            "from_entity": "TestEntity",
            "to_entity": "TestEntity",
            "relation": "self_ref",
            "body": "stub",
            "t_valid": "2025-01-01T00:00:00Z",
            "t_invalid": None,
            "confidence": 0.5,
            "source_doc_id": doc_id,
            "source_span": None,
        }
    ]


# ---------------------------------------------------------------------------
# LLM call
# ---------------------------------------------------------------------------


def _strip_fences(text: str) -> str:
    """Strip markdown code fences if the model wraps its JSON in them."""
    text = text.strip()
    if text.startswith("```"):
        lines = text.splitlines()
        # drop first line (``` or ```json) and last line (```)
        inner = lines[1:-1] if lines[-1].strip() == "```" else lines[1:]
        text = "\n".join(inner).strip()
    return text


def _call_llm_httpx(
    doc: dict[str, Any],
) -> dict[str, Any]:
    """Call the LLM via httpx (preferred) or urllib.request (stdlib fallback)."""
    user_msg = _USER_TEMPLATE.format(
        doc_id=doc["source_doc_id"],
        created_at=doc.get("created_at", "2025-01-01T00:00:00Z"),
        body=doc.get("body", ""),
    )
    payload: dict[str, Any] = {
        "model": _MODEL,
        "messages": [
            {"role": "system", "content": _SYSTEM_PROMPT},
            {"role": "user", "content": user_msg},
        ],
        "max_tokens": 4096,
        "temperature": 0,
        "response_format": {"type": "json_object"},
    }

    try:
        import httpx  # type: ignore[import-not-found]

        with httpx.Client(timeout=60.0) as client:
            resp = client.post(
                _BASE_URL.rstrip("/") + "/chat/completions",
                json=payload,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {_API_KEY}",
                },
            )
            resp.raise_for_status()
            body = resp.json()
    except ImportError:
        # Fall back to stdlib urllib.request
        import urllib.request  # noqa: PLC0415

        encoded = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            _BASE_URL.rstrip("/") + "/chat/completions",
            data=encoded,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {_API_KEY}",
            },
        )
        with urllib.request.urlopen(req, timeout=60) as r:  # noqa: S310
            body = json.loads(r.read().decode("utf-8"))

    text = body["choices"][0]["message"]["content"]
    return json.loads(_strip_fences(text))


def _extract_single_doc(doc: dict[str, Any]) -> tuple[list[dict], list[dict], list[dict]]:
    """Extract entities and edges from a single document.

    Returns (entities, edges, warnings).  On any error returns ([], [], [warning]).
    """
    doc_id = doc["source_doc_id"]

    if _STUB_MODE:
        return _stub_entities(doc_id), _stub_edges(doc_id), []

    try:
        result = _call_llm_httpx(doc)
        entities = result.get("entities", [])
        edges = result.get("edges", [])
        # Rust derive_logical_id rejects empty names or kinds containing ':'.
        # Filter those out here rather than aborting the whole extraction.
        entities = [
            e for e in entities
            if isinstance(e, dict)
            and str(e.get("name", "")).strip()
            and ":" not in str(e.get("type", ""))
        ]
        edges = [
            e for e in edges
            if isinstance(e, dict)
            and str(e.get("from_entity", "")).strip()
            and str(e.get("to_entity", "")).strip()
        ]
        # Attribution backfill (R-20-E2). Every element we hand back must name
        # the document it came from, because the engine batches documents and
        # then requires each entity/edge to select its own source among the
        # batch's caller-supplied ids -- an unattributed element fails the whole
        # ingest with EngineError::Extractor. This is safe to do here and ONLY
        # here: `_extract_single_doc` is scoped to exactly one document, so
        # `doc_id` is the true source, not a guess. The engine deliberately
        # refuses to make that guess itself.
        for entity in entities:
            if not entity.get("source_doc_id"):
                entity["source_doc_id"] = doc_id
        for edge in edges:
            if not edge.get("source_doc_id"):
                edge["source_doc_id"] = doc_id
            # Rust rejects confidence outside [0.0, 1.0] with EngineError::Extractor.
            c = edge.get("confidence")
            if c is None:
                pass
            elif isinstance(c, (int, float)) and c == c:  # c == c is False for NaN
                edge["confidence"] = max(0.0, min(1.0, float(c)))
            else:
                edge["confidence"] = 0.5
        warnings = result.get("warnings", [])
        return entities, edges, warnings
    except Exception as exc:  # noqa: BLE001
        print(f"[elps_harness] extraction failed for {doc_id}: {exc}", file=sys.stderr, flush=True)
        return [], [], [{"kind": "extraction_failed", "source_doc_id": doc_id, "detail": str(exc)}]


# ---------------------------------------------------------------------------
# Protocol implementation
# ---------------------------------------------------------------------------


def _write_json(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def _handle_extract(msg: dict[str, Any]) -> None:
    """Process an extract request and write the result."""
    request_id = msg.get("request_id", "")
    documents: list[dict[str, Any]] = msg.get("documents", [])

    all_entities: list[dict] = []
    all_edges: list[dict] = []
    all_warnings: list[dict] = []

    for doc in documents:
        entities, edges, warnings = _extract_single_doc(doc)
        all_entities.extend(entities)
        all_edges.extend(edges)
        all_warnings.extend(warnings)

    _write_json(
        {
            "protocol": _PROTOCOL,
            "type": "result",
            "request_id": request_id,
            "entities": all_entities,
            "edges": all_edges,
            "warnings": all_warnings,
        }
    )


def main() -> None:
    print(
        f"[elps_harness] starting (model={_MODEL} base_url={_BASE_URL} stub={_STUB_MODE})",
        file=sys.stderr,
        flush=True,
    )

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            msg = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            print(f"[elps_harness] invalid JSON on stdin: {exc}", file=sys.stderr, flush=True)
            continue

        msg_type = msg.get("type")

        if msg_type == "hello":
            _write_json(
                {
                    "protocol": _PROTOCOL,
                    "type": "ready",
                    "schema_version": _SCHEMA_VERSION,
                    "provider": "airlock-claude-haiku",
                    "model": _MODEL,
                    "supports": {},
                    "max_docs_per_request": _MAX_DOCS_PER_REQUEST,
                }
            )
        elif msg_type == "extract":
            try:
                _handle_extract(msg)
            except Exception as exc:  # noqa: BLE001
                request_id = msg.get("request_id", "")
                print(
                    f"[elps_harness] fatal error handling extract {request_id}: {exc}",
                    file=sys.stderr,
                    flush=True,
                )
                _write_json(
                    {
                        "protocol": _PROTOCOL,
                        "type": "error",
                        "request_id": request_id,
                        "error_code": "extraction_failed",
                        "detail": str(exc),
                    }
                )
        else:
            print(
                f"[elps_harness] unknown message type: {msg_type!r}",
                file=sys.stderr,
                flush=True,
            )


if __name__ == "__main__":
    main()
