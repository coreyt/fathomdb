"""Cross-language test driver for the Python fathomdb SDK.

Reads scenarios from scenarios.json, writes data to a database,
queries it back, and emits a normalized JSON manifest to stdout.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from fathomdb import (
    ActionInsert,
    ChunkInsert,
    ChunkPolicy,
    EdgeInsert,
    Engine,
    NodeInsert,
    NodeRetire,
    RunInsert,
    StepInsert,
    WriteRequest,
)

SCENARIOS_PATH = Path(__file__).resolve().parent.parent / "scenarios.json"


def load_scenarios() -> list[dict]:
    with open(SCENARIOS_PATH) as f:
        return json.load(f)["scenarios"]


def sorted_json(obj: object) -> str:
    """Serialize to compact JSON with sorted keys for deterministic output."""
    return json.dumps(obj, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def normalize_properties(props: object) -> object:
    """Round-trip properties through sorted JSON to normalize key order."""
    return json.loads(sorted_json(props))


def build_write_request(write_def: dict) -> WriteRequest:
    """Build a WriteRequest from a scenario write definition."""
    nodes = []
    for n in write_def.get("nodes", []):
        nodes.append(NodeInsert(
            row_id=n["row_id"],
            logical_id=n["logical_id"],
            kind=n["kind"],
            properties=n["properties"],
            source_ref=n.get("source_ref"),
            upsert=n.get("upsert", False),
            chunk_policy=ChunkPolicy(n["chunk_policy"]) if "chunk_policy" in n else ChunkPolicy.PRESERVE,
        ))

    node_retires = []
    for nr in write_def.get("node_retires", []):
        node_retires.append(NodeRetire(
            logical_id=nr["logical_id"],
            source_ref=nr.get("source_ref"),
        ))

    edges = []
    for e in write_def.get("edges", []):
        edges.append(EdgeInsert(
            row_id=e["row_id"],
            logical_id=e["logical_id"],
            source_logical_id=e["source_logical_id"],
            target_logical_id=e["target_logical_id"],
            kind=e["kind"],
            properties=e.get("properties", {}),
            source_ref=e.get("source_ref"),
            upsert=e.get("upsert", False),
        ))

    chunks = []
    for c in write_def.get("chunks", []):
        chunks.append(ChunkInsert(
            id=c["id"],
            node_logical_id=c["node_logical_id"],
            text_content=c["text_content"],
            byte_start=c.get("byte_start"),
            byte_end=c.get("byte_end"),
        ))

    runs = []
    for r in write_def.get("runs", []):
        runs.append(RunInsert(
            id=r["id"],
            kind=r["kind"],
            status=r["status"],
            properties=r["properties"],
            source_ref=r.get("source_ref"),
            upsert=r.get("upsert", False),
            supersedes_id=r.get("supersedes_id"),
        ))

    steps = []
    for s in write_def.get("steps", []):
        steps.append(StepInsert(
            id=s["id"],
            run_id=s["run_id"],
            kind=s["kind"],
            status=s["status"],
            properties=s["properties"],
            source_ref=s.get("source_ref"),
            upsert=s.get("upsert", False),
            supersedes_id=s.get("supersedes_id"),
        ))

    actions = []
    for a in write_def.get("actions", []):
        actions.append(ActionInsert(
            id=a["id"],
            step_id=a["step_id"],
            kind=a["kind"],
            status=a["status"],
            properties=a["properties"],
            source_ref=a.get("source_ref"),
            upsert=a.get("upsert", False),
            supersedes_id=a.get("supersedes_id"),
        ))

    return WriteRequest(
        label=write_def["label"],
        nodes=nodes,
        node_retires=node_retires,
        edges=edges,
        chunks=chunks,
        runs=runs,
        steps=steps,
        actions=actions,
    )


def execute_query(engine: Engine, query_def: dict) -> dict:
    """Execute a single query from a scenario definition and return normalized result."""
    qtype = query_def["type"]

    if qtype == "filter_logical_id":
        rows = engine.nodes(query_def["kind"]).filter_logical_id_eq(query_def["logical_id"]).execute()
        nodes = sorted(
            [{"logical_id": n.logical_id, "kind": n.kind, "properties": normalize_properties(n.properties)}
             for n in rows.nodes],
            key=lambda n: n["logical_id"],
        )
        result: dict = {"type": qtype, "count": len(rows.nodes), "nodes": nodes}
        if query_def.get("expect_runs"):
            result["run_count"] = len(rows.runs)
        if query_def.get("expect_steps"):
            result["step_count"] = len(rows.steps)
        if query_def.get("expect_actions"):
            result["action_count"] = len(rows.actions)
        return result

    if qtype == "text_search":
        rows = engine.nodes(query_def["kind"]).text_search(query_def["query"], limit=query_def["limit"]).execute()
        return {"type": qtype, "count": len(rows.nodes)}

    if qtype == "traverse":
        q = engine.nodes(query_def["kind"]).filter_logical_id_eq(query_def["start_logical_id"])
        q = q.traverse(direction=query_def["direction"], label=query_def["label"], max_depth=query_def["max_depth"])
        rows = q.limit(10).execute()
        found_ids = sorted([n.logical_id for n in rows.nodes])
        return {"type": qtype, "found_ids": found_ids}

    raise ValueError(f"unknown query type: {qtype}")


def execute_admin(engine: Engine, admin_def) -> dict:
    """Execute a single admin operation and return normalized result."""
    if isinstance(admin_def, str):
        admin_def = {"type": admin_def}

    atype = admin_def.get("type", admin_def) if isinstance(admin_def, dict) else admin_def

    if atype == "check_integrity":
        report = engine.admin.check_integrity()
        return {
            "type": "check_integrity",
            "physical_ok": report.physical_ok,
            "foreign_keys_ok": report.foreign_keys_ok,
            "missing_fts_rows": report.missing_fts_rows,
            "duplicate_active_logical_ids": report.duplicate_active_logical_ids,
        }

    if atype == "check_semantics":
        report = engine.admin.check_semantics()
        return {
            "type": "check_semantics",
            "orphaned_chunks": report.orphaned_chunks,
            "dangling_edges": report.dangling_edges,
            "broken_step_fk": report.broken_step_fk,
            "broken_action_fk": report.broken_action_fk,
        }

    if atype == "trace_source":
        report = engine.admin.trace_source(admin_def["source_ref"])
        return {
            "type": "trace_source",
            "source_ref": admin_def["source_ref"],
            "node_rows": report.node_rows,
            "edge_rows": report.edge_rows,
            "action_rows": report.action_rows,
        }

    raise ValueError(f"unknown admin type: {atype}")


def run_driver(db_path: str, mode: str) -> dict:
    """Run the driver in the specified mode and return the manifest."""
    scenarios = load_scenarios()
    engine = Engine.open(db_path)

    if mode == "write":
        for scenario in scenarios:
            for write_def in scenario["writes"]:
                request = build_write_request(write_def)
                engine.write(request)

    results = {}
    for scenario in scenarios:
        queries = [execute_query(engine, q) for q in scenario.get("queries", [])]
        admin = [execute_admin(engine, a) for a in scenario.get("admin", [])]
        results[scenario["name"]] = {"queries": queries, "admin": admin}

    engine.close()
    return {"results": results}


def main() -> int:
    parser = argparse.ArgumentParser(description="Cross-language Python driver")
    parser.add_argument("--db", required=True, help="Database file path")
    parser.add_argument("--mode", choices=["write", "read"], required=True,
                        help="write: write+read, read: read only")
    args = parser.parse_args()

    manifest = run_driver(args.db, args.mode)
    print(sorted_json(manifest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
