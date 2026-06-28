#!/usr/bin/env python3
"""Stub BYO-LLM harness for the ELPS conformance golden fixture.

This is a TEST-ONLY harness for the Slice 30 conformance gate. It reads
`golden.jsonl` (the real cross-repo ELPS golden artifact vendored from
`~/projects/memex/src/memex/elps/fixtures/golden.jsonl`) and returns the
`expected` field for each case, keyed by source_doc_id.

The engine sends extract requests with `source_doc_id` in the documents field.
The golden.jsonl `expected` field contains edges/warnings with `source_doc_id`.
This stub maps `source_doc_id -> expected_result`.

Transport: NDJSON over stdio (stdin -> stdout).
Protocol: fathomdb.extract.v1 (schema_version 1).
No network calls. No LLM.
"""
import json
import sys
from pathlib import Path

# Force UTF-8 stdio: golden responses include non-ASCII and are emitted with
# `ensure_ascii=False`, which would otherwise encode via the platform default
# (cp1252 on Windows) and corrupt the NDJSON the engine reads. (0.8.9 Slice 20, F-9.)
sys.stdout.reconfigure(encoding="utf-8")
sys.stdin.reconfigure(encoding="utf-8")

GOLDEN_FILE = Path(__file__).parent / "golden.jsonl"


def log(msg: str) -> None:
    """Write diagnostic to stderr (never stdout per protocol)."""
    print(f"[elps_conformance_stub] {msg}", file=sys.stderr, flush=True)


def get_source_doc_id_from_expected(expected: dict) -> str | None:
    """Extract source_doc_id from an expected result envelope.

    Checks edges first (non-empty cases), then warnings (no_facts cases).
    """
    # Non-empty cases: first edge's source_doc_id
    edges = expected.get("edges", [])
    if edges:
        return edges[0].get("source_doc_id")
    # No-facts / empty cases: first warning's source_doc_id
    warnings = expected.get("warnings", [])
    if warnings:
        return warnings[0].get("source_doc_id")
    return None


def load_golden():
    """Load golden.jsonl and build a source_doc_id -> expected_result mapping.

    Each line in golden.jsonl has:
      {"request": "<json string>", "expected": "<json string of result>"}

    We build: {source_doc_id: expected_result_dict}
    """
    results = {}
    if not GOLDEN_FILE.exists():
        log(f"FATAL: golden.jsonl not found at {GOLDEN_FILE}")
        return results

    with open(GOLDEN_FILE, encoding="utf-8") as f:
        for line_num, raw_line in enumerate(f, 1):
            raw_line = raw_line.strip()
            if not raw_line:
                continue
            try:
                entry = json.loads(raw_line)
                expected = json.loads(entry["expected"])
                src_id = get_source_doc_id_from_expected(expected)
                if src_id is None:
                    log(f"Line {line_num}: cannot determine source_doc_id from expected")
                    continue
                results[src_id] = expected
            except (json.JSONDecodeError, KeyError) as e:
                log(f"Line {line_num}: parse error: {e}")
                continue

    log(f"Loaded {len(results)} golden cases: {sorted(results.keys())}")
    return results


def main() -> None:
    golden = load_golden()

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

        msg_type = msg.get("type", "")

        if msg_type == "hello":
            response = {
                "protocol": "fathomdb.extract.v1",
                "type": "ready",
                "schema_version": 1,
                "provider": "elps_conformance_stub",
                "model": "golden-v1",
                "supports": {},
                "max_docs_per_request": 1,
            }
            log("sending ready")
            print(json.dumps(response), flush=True)

        elif msg_type == "extract":
            request_id = msg.get("request_id", "unknown")
            documents = msg.get("documents", [])

            if not documents:
                log(f"WARN: extract with no documents for request_id={request_id}")
                response = {
                    "protocol": "fathomdb.extract.v1",
                    "type": "result",
                    "request_id": request_id,
                    "entities": [],
                    "edges": [],
                    "warnings": [],
                }
                print(json.dumps(response), flush=True)
                continue

            # The engine sends source_doc_id in the document object.
            src_id = documents[0].get("source_doc_id", "")

            if src_id not in golden:
                log(f"WARN: source_doc_id={src_id!r} not in golden; returning no_facts")
                response = {
                    "protocol": "fathomdb.extract.v1",
                    "type": "result",
                    "request_id": request_id,
                    "entities": [],
                    "edges": [],
                    "warnings": [{"kind": "no_facts", "source_doc_id": src_id,
                                   "detail": None, "dropped": None, "kept": None,
                                   "prior_body": None, "raw_t_valid": None,
                                   "substituted_t_valid": None, "supersedes_hint": None}],
                }
            else:
                # Return the golden expected result with the engine's request_id.
                response = dict(golden[src_id])
                response["request_id"] = request_id

            log(f"sending result for source_doc_id={src_id!r} request_id={request_id}: "
                f"{len(response.get('entities', []))} entities, "
                f"{len(response.get('edges', []))} edges")
            print(json.dumps(response, ensure_ascii=False), flush=True)

        else:
            log(f"unknown message type: {msg_type!r}")
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
