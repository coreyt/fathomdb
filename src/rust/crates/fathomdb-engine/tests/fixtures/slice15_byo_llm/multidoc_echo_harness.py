#!/usr/bin/env python3
"""Stub BYO-LLM harness that ECHOES a per-entity `source_doc_id` on multi-doc batches.

TEST-ONLY fixture for 0.8.20 Slice 5 fix-4 (R-20-E2, multi-document arm). It
speaks the `fathomdb.extract.v1` protocol and emits one entity + one self-edge
per document in the request, attributing each to a `source_doc_id`.

Two modes, selected by argv[1]:

  ``valid``    — echo each element's own caller-supplied `source_doc_id`. The
                 engine must accept this and store the CALLER's copy of the
                 string.
  ``foreign``  — echo an id that is NOT in the batch. The engine must REJECT
                 this with `EngineError::Extractor` rather than storing the
                 model-chosen value; accepting it as a value is exactly how a
                 row acquires provenance no `excise_source` call can reach.

Transport: NDJSON over stdio (stdin -> stdout). No network calls, no LLM.
"""
import json
import sys

FOREIGN_ID = "doc-not-in-this-batch"


def log(msg: str) -> None:
    """Write diagnostic to stderr (never stdout per protocol)."""
    print(f"[multidoc_echo_harness] {msg}", file=sys.stderr, flush=True)


def _build(documents: list, mode: str) -> tuple[list, list]:
    """Emit one entity and one self-edge per document, carrying an echo."""
    entities = []
    edges = []
    for doc in documents:
        doc_id = doc.get("source_doc_id", "")
        echo = doc_id if mode == "valid" else FOREIGN_ID
        # A distinct entity name per document, so the rows do NOT collapse onto
        # a single logical_id and each keeps its own provenance.
        name = f"Entity-{doc_id}"
        entities.append(
            {"name": name, "type": "person", "aliases": [], "source_doc_id": echo}
        )
        edges.append(
            {
                "from_entity": name,
                "relation": "appears_in",
                "to_entity": name,
                "body": f"{name} appears in {doc_id}",
                "t_valid": None,
                "t_invalid": None,
                "confidence": 0.9,
                "source_doc_id": echo,
            }
        )
    return entities, edges


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "valid"
    if mode not in ("valid", "foreign"):
        log(f"unknown mode {mode!r}; expected 'valid' or 'foreign'")
        sys.exit(2)
    log(f"started (mode={mode})")

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue

        try:
            msg = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            log(f"JSON parse error: {exc}")
            continue

        if msg.get("protocol", "") != "fathomdb.extract.v1":
            print(json.dumps({
                "protocol": "fathomdb.extract.v1",
                "type": "error",
                "error_code": "invalid_request",
                "detail": "unknown protocol",
            }), flush=True)
            continue

        msg_type = msg.get("type", "")

        if msg_type == "hello":
            print(json.dumps({
                "protocol": "fathomdb.extract.v1",
                "type": "ready",
                "schema_version": 1,
                "provider": "stub",
                "model": f"multidoc-echo-{mode}",
                "supports": {},
                "max_docs_per_request": 8,
            }), flush=True)

        elif msg_type == "extract":
            request_id = msg.get("request_id", "unknown")
            entities, edges = _build(msg.get("documents", []), mode)
            print(json.dumps({
                "protocol": "fathomdb.extract.v1",
                "type": "result",
                "request_id": request_id,
                "entities": entities,
                "edges": edges,
                "warnings": [],
            }), flush=True)

        else:
            print(json.dumps({
                "protocol": "fathomdb.extract.v1",
                "type": "error",
                "error_code": "invalid_request",
                "detail": f"unknown type: {msg_type}",
            }), flush=True)

    log("stdin closed, exiting")


if __name__ == "__main__":
    main()
