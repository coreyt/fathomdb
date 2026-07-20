#!/usr/bin/env python3
"""Stub BYO-LLM harness that OMITS `source_doc_id` from every result element.

TEST-ONLY fixture for 0.8.20 Slice 5c work item 10 (R-20-E2). It speaks the
`fathomdb.extract.v1` protocol correctly in every other respect, but never
echoes the `source_doc_id` back — modelling an LLM that drops, forgets or
refuses the field.

The requirement under test: FathomDB must take each ingested row's provenance
from the CALLER's `ExtractDocument.source_doc_id`, never from this echo. A model
must not be able to make a row un-erasable by omitting a field.

Transport: NDJSON over stdio (stdin -> stdout). No network calls, no LLM.
"""
import json
import sys


def log(msg: str) -> None:
    """Write diagnostic to stderr (never stdout per protocol)."""
    print(f"[provenance_omitting_harness] {msg}", file=sys.stderr, flush=True)


# Deliberately NO "source_doc_id" key anywhere in these payloads.
ENTITIES = [
    {"name": "Alice", "type": "person", "aliases": []},
    {"name": "Project X", "type": "project", "aliases": []},
]
EDGES = [
    {
        "from_entity": "Alice",
        "relation": "owns",
        "to_entity": "Project X",
        "body": "Alice owns the project",
        "t_valid": None,
        "t_invalid": None,
        "confidence": 0.95,
    }
]


def main() -> None:
    log("started (source_doc_id will be omitted from every element)")

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
                "model": "omitting-v1",
                "supports": {},
                "max_docs_per_request": 8,
            }), flush=True)

        elif msg_type == "extract":
            request_id = msg.get("request_id", "unknown")
            print(json.dumps({
                "protocol": "fathomdb.extract.v1",
                "type": "result",
                "request_id": request_id,
                "entities": ENTITIES,
                "edges": EDGES,
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
