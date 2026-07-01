#!/usr/bin/env python3
"""Stub BYO-LLM harness for the fathomdb.consolidate.v1 protocol.

0.8.12 Slice 15 (OPP-2, ADR-0.8.12). TEST-ONLY, DETERMINISTIC consolidation
provider. CALLER-SIDE BYO-LLM / OFFLINE-BUILD: this is a local Python script,
NOT an LLM and NOT a network client. It rides the SAME NDJSON-over-stdio
transport as the extract stub; only the protocol string and payload differ.

Transport: NDJSON over stdio (stdin -> stdout, diagnostics -> stderr).
Protocol:  fathomdb.consolidate.v1 (schema_version 1).

Deterministic recency rule (no LLM, no randomness):
  Within a candidate cluster of competing fact-edges for one
  (subject, relation) axis, KEEP the edge with the latest `t_valid`
  (ISO-8601 strings sort chronologically) and INVALIDATE every other edge,
  setting its `t_invalid` to the kept edge's `t_valid`. Edges with no
  `t_valid` are treated as oldest. Ties keep the first-presented edge
  (stable insertion order). Bodies are NEVER rewritten; the harness NEVER
  deletes.
"""
import json
import sys

PROTOCOL = "fathomdb.consolidate.v1"


def log(msg: str) -> None:
    """Write diagnostics to stderr (never stdout per protocol)."""
    print(f"[stub_consolidate_harness] {msg}", file=sys.stderr, flush=True)


def consolidate(cluster: dict) -> list:
    """Apply the deterministic recency rule; return a list of verdicts."""
    edges = cluster.get("edges", [])
    if not edges:
        return []

    # Pick the winner = latest t_valid (None sorts as oldest = ""). Stable:
    # enumerate index is the tiebreaker so the first-presented edge wins a tie.
    def sort_key(item):
        idx, e = item
        tv = e.get("t_valid") or ""
        return (tv, -idx)  # latest t_valid; on tie, smallest index

    winner_idx, winner = max(enumerate(edges), key=sort_key)
    winner_t_valid = winner.get("t_valid")

    verdicts = []
    for idx, e in enumerate(edges):
        ref = e.get("edge_ref")
        if idx == winner_idx:
            verdicts.append({"edge_ref": ref, "verdict": "keep"})
        else:
            # Older/competing fact: invalidate at the winner's valid-time.
            # Fall back to a fixed sentinel if the winner has no t_valid.
            t_invalid = winner_t_valid or "1970-01-01T00:00:00Z"
            verdicts.append(
                {"edge_ref": ref, "verdict": "invalidate", "t_invalid": t_invalid}
            )
    return verdicts


def main() -> None:
    log("stub consolidate harness started")
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

        if protocol != PROTOCOL:
            log(f"unknown protocol: {protocol}")
            print(
                json.dumps(
                    {
                        "protocol": PROTOCOL,
                        "type": "error",
                        "error_code": "invalid_request",
                        "detail": f"unknown protocol: {protocol}",
                    }
                ),
                flush=True,
            )
            continue

        if msg_type == "hello":
            # Advertise `consolidate` in supported_tasks so FathomDB's
            # negotiation admits the dispatch (ADR-0.8.6 s2.2 / ADR-0.8.12 s2).
            print(
                json.dumps(
                    {
                        "protocol": PROTOCOL,
                        "type": "ready",
                        "schema_version": 1,
                        "provider": "stub",
                        "model": "stub-consolidate-v1",
                        "supported_tasks": ["consolidate"],
                        "max_docs_per_request": 8,
                    }
                ),
                flush=True,
            )
            log("sent ready: model=stub-consolidate-v1")

        elif msg_type == "consolidate":
            request_id = msg.get("request_id", "unknown")
            cluster = msg.get("cluster", {})
            verdicts = consolidate(cluster)
            print(
                json.dumps(
                    {
                        "protocol": PROTOCOL,
                        "type": "result",
                        "request_id": request_id,
                        "verdicts": verdicts,
                        "body": "recency rule: kept latest t_valid, invalidated older",
                    }
                ),
                flush=True,
            )
            log(f"sent result for request_id={request_id}: {len(verdicts)} verdicts")

        else:
            log(f"unknown message type: {msg_type}")
            print(
                json.dumps(
                    {
                        "protocol": PROTOCOL,
                        "type": "error",
                        "error_code": "invalid_request",
                        "detail": f"unknown type: {msg_type}",
                    }
                ),
                flush=True,
            )

    log("stdin closed, exiting")


if __name__ == "__main__":
    main()
