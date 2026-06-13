#!/usr/bin/env python3
"""Stub BYO-LLM harness for the fathomdb.extract.v1 protocol.

This is a TEST-ONLY harness that acts as a deterministic extraction provider
for the Slice 15 conformance tests. It responds to hello/extract with
deterministic JSON from the fixture. No network calls, no LLM.

Transport: NDJSON over stdio (stdin → stdout).
Protocol: fathomdb.extract.v1 (schema_version 1).
"""
import json
import sys
from pathlib import Path

FIXTURE_FILE = Path(__file__).parent / "fixture_result.json"

def log(msg: str) -> None:
    """Write diagnostic to stderr (never stdout per protocol)."""
    print(f"[stub_harness] {msg}", file=sys.stderr, flush=True)


def main() -> None:
    # Load fixture results keyed by source_doc_id.
    if FIXTURE_FILE.exists():
        with open(FIXTURE_FILE) as f:
            fixture_data = json.load(f)
    else:
        fixture_data = {}

    log("stub harness started")

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue

        try:
            msg = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            log(f"JSON parse error: {exc}")
            continue

        protocol = msg.get("protocol", "")
        msg_type = msg.get("type", "")

        if protocol != "fathomdb.extract.v1":
            log(f"unknown protocol: {protocol}")
            response = {
                "protocol": "fathomdb.extract.v1",
                "type": "error",
                "error_code": "invalid_request",
                "detail": f"unknown protocol: {protocol}",
            }
            print(json.dumps(response), flush=True)
            continue

        if msg_type == "hello":
            response = {
                "protocol": "fathomdb.extract.v1",
                "type": "ready",
                "schema_version": 1,
                "provider": "stub",
                "model": "stub-v1",
                "supports": {},
                "max_docs_per_request": 8,
            }
            log("sending ready: model=stub-v1")
            print(json.dumps(response), flush=True)

        elif msg_type == "extract":
            request_id = msg.get("request_id", "unknown")
            documents = msg.get("documents", [])

            # Build result from fixture data.
            all_entities = []
            all_edges = []
            warnings = []

            for doc in documents:
                doc_id = doc.get("source_doc_id", "")
                if doc_id in fixture_data:
                    doc_result = fixture_data[doc_id]
                    all_entities.extend(doc_result.get("entities", []))
                    all_edges.extend(doc_result.get("edges", []))
                    if doc_result.get("warnings"):
                        warnings.extend(doc_result["warnings"])
                else:
                    # Unknown document → emit no_facts warning.
                    warnings.append({
                        "kind": "no_facts",
                        "source_doc_id": doc_id,
                    })

            response = {
                "protocol": "fathomdb.extract.v1",
                "type": "result",
                "request_id": request_id,
                "entities": all_entities,
                "edges": all_edges,
                "warnings": warnings,
            }
            log(f"sending result for request_id={request_id}: "
                f"{len(all_entities)} entities, {len(all_edges)} edges")
            print(json.dumps(response), flush=True)

        else:
            log(f"unknown message type: {msg_type}")
            response = {
                "protocol": "fathomdb.extract.v1",
                "type": "error",
                "error_code": "invalid_request",
                "detail": f"unknown type: {msg_type}",
            }
            print(json.dumps(response), flush=True)

    log("stdin closed, exiting")


if __name__ == "__main__":
    main()
