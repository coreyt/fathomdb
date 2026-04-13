"""Cross-language test driver for the Python fathomdb SDK.

Reads scenarios from scenarios.json, writes data to a database,
queries it back, and emits a normalized JSON manifest to stdout.

Design, scenario format, and instructions for adding new scenarios
are documented in tests/cross-language/README.md.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

from fathomdb import (
    ActionInsert,
    ChunkInsert,
    ChunkPolicy,
    EdgeInsert,
    Engine,
    FtsPropertyPathMode,
    FtsPropertyPathSpec,
    NodeInsert,
    NodeRetire,
    RunInsert,
    StepInsert,
    WriteRequest,
)

SCENARIOS_PATH = Path(__file__).resolve().parent.parent / "scenarios.json"


def load_scenarios_file() -> dict:
    """Load the shared scenarios JSON file."""
    with open(SCENARIOS_PATH) as f:
        return json.load(f)


def load_scenarios() -> list[dict]:
    """Return the list of scenario definitions from the shared file."""
    return load_scenarios_file()["scenarios"]


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
            content_ref=n.get("content_ref"),
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
            content_hash=c.get("content_hash"),
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


WRITTEN_AT_RECENT_WINDOW_SECONDS = 300


def _actual_hit(hit, with_attribution: bool) -> dict:
    """Normalize a SearchHit into a comparable dict.

    Drops non-deterministic fields (``score``, raw ``written_at``,
    raw ``projection_row_id``) and substitutes deterministic booleans so
    the Python and TypeScript drivers can diff their manifests byte for
    byte while still asserting the fields exist. Adds
    ``attribution_matched_paths`` only when attribution was requested.
    """
    now = int(time.time())
    entry = {
        "logical_id": hit.node.logical_id,
        "kind": hit.node.kind,
        "source": hit.source.value if hasattr(hit.source, "value") else str(hit.source),
        "match_mode": hit.match_mode.value if hasattr(hit.match_mode, "value") else str(hit.match_mode),
        "snippet_non_empty": bool(hit.snippet and hit.snippet.strip()),
        "written_at_recent": (
            hit.written_at > 0
            and hit.written_at <= now
            and hit.written_at >= now - WRITTEN_AT_RECENT_WINDOW_SECONDS
        ),
        "projection_row_id_present": hit.projection_row_id is not None,
    }
    if with_attribution:
        paths = []
        if hit.attribution is not None:
            paths = list(hit.attribution.matched_paths)
        entry["attribution_matched_paths"] = paths
    return entry


def _search_rows_to_actual(rows, with_attribution: bool) -> dict:
    raw_projection_ids = [h.projection_row_id for h in rows.hits]
    present_ids = [pid for pid in raw_projection_ids if pid is not None]
    all_projection_ids_present = len(present_ids) == len(raw_projection_ids)
    projection_ids_unique = (
        all_projection_ids_present and len(set(present_ids)) == len(present_ids)
    )
    return {
        "hit_count": len(rows.hits),
        "strict_hit_count": rows.strict_hit_count,
        "relaxed_hit_count": rows.relaxed_hit_count,
        "fallback_used": rows.fallback_used,
        "was_degraded": rows.was_degraded,
        "projection_row_ids_unique": projection_ids_unique,
        "hits": [_actual_hit(h, with_attribution) for h in rows.hits],
    }


def _evaluate_search_expectations(query_def: dict, actual: dict) -> list[str]:
    """Return a list of human-readable failure strings; empty means pass."""
    failures: list[str] = []
    hits = actual["hits"]

    if "expect_hit_logical_ids" in query_def:
        want = list(query_def["expect_hit_logical_ids"])
        got = [h["logical_id"] for h in hits]
        if got != want:
            failures.append(f"expect_hit_logical_ids: want {want}, got {got}")

    if "expect_hit_sources" in query_def:
        want = list(query_def["expect_hit_sources"])
        got = [h["source"] for h in hits]
        if got != want:
            failures.append(f"expect_hit_sources: want {want}, got {got}")

    if "expect_match_modes" in query_def:
        want = list(query_def["expect_match_modes"])
        got = [h["match_mode"] for h in hits]
        if got != want:
            failures.append(f"expect_match_modes: want {want}, got {got}")

    if query_def.get("expect_snippets_non_empty"):
        if not all(h["snippet_non_empty"] for h in hits):
            failures.append("expect_snippets_non_empty: some hit had empty snippet")

    if query_def.get("expect_written_at_seconds_recent"):
        if not all(h["written_at_recent"] for h in hits):
            failures.append("expect_written_at_seconds_recent: some hit written_at out of window")

    if query_def.get("expect_projection_row_ids_unique"):
        ids_present = [h["projection_row_id_present"] for h in hits]
        if not all(ids_present):
            failures.append("expect_projection_row_ids_unique: some hit missing projection_row_id")
        elif not actual.get("projection_row_ids_unique", False):
            failures.append(
                "expect_projection_row_ids_unique: duplicate projection_row_ids across hits"
            )

    if "expect_strict_hit_count" in query_def:
        if actual["strict_hit_count"] != query_def["expect_strict_hit_count"]:
            failures.append(
                f"expect_strict_hit_count: want {query_def['expect_strict_hit_count']}, got {actual['strict_hit_count']}"
            )

    if "expect_strict_hit_count_min" in query_def:
        if actual["strict_hit_count"] < query_def["expect_strict_hit_count_min"]:
            failures.append(
                f"expect_strict_hit_count_min: want >= {query_def['expect_strict_hit_count_min']}, got {actual['strict_hit_count']}"
            )

    if "expect_relaxed_hit_count" in query_def:
        if actual["relaxed_hit_count"] != query_def["expect_relaxed_hit_count"]:
            failures.append(
                f"expect_relaxed_hit_count: want {query_def['expect_relaxed_hit_count']}, got {actual['relaxed_hit_count']}"
            )

    if "expect_relaxed_hit_count_min" in query_def:
        if actual["relaxed_hit_count"] < query_def["expect_relaxed_hit_count_min"]:
            failures.append(
                f"expect_relaxed_hit_count_min: want >= {query_def['expect_relaxed_hit_count_min']}, got {actual['relaxed_hit_count']}"
            )

    if "expect_fallback_used" in query_def:
        if actual["fallback_used"] != query_def["expect_fallback_used"]:
            failures.append(
                f"expect_fallback_used: want {query_def['expect_fallback_used']}, got {actual['fallback_used']}"
            )

    if "expect_was_degraded" in query_def:
        if actual["was_degraded"] != query_def["expect_was_degraded"]:
            failures.append(
                f"expect_was_degraded: want {query_def['expect_was_degraded']}, got {actual['was_degraded']}"
            )

    if "expect_min_count" in query_def:
        if actual["hit_count"] < query_def["expect_min_count"]:
            failures.append(
                f"expect_min_count: want >= {query_def['expect_min_count']}, got {actual['hit_count']}"
            )

    if "expect_matched_paths" in query_def:
        for item in query_def["expect_matched_paths"]:
            idx = item["hit_index"]
            want_paths = list(item["paths"])
            if idx >= len(hits):
                failures.append(f"expect_matched_paths: hit_index {idx} out of range")
                continue
            got_paths = hits[idx].get("attribution_matched_paths", [])
            if sorted(got_paths) != sorted(want_paths):
                failures.append(
                    f"expect_matched_paths[{idx}]: want {want_paths}, got {got_paths}"
                )

    return failures


def execute_text_search(engine: Engine, query_def: dict) -> dict:
    """Run a text_search scenario query, normalize the result, evaluate expectations."""
    with_attribution = bool(query_def.get("with_match_attribution"))
    builder = engine.nodes(query_def["kind"]).text_search(
        query_def["query"], limit=query_def["limit"]
    )
    if with_attribution:
        builder = builder.with_match_attribution()

    repeat_runs = int(query_def.get("repeat_runs", 1))
    runs_actual = [_search_rows_to_actual(builder.execute(), with_attribution) for _ in range(repeat_runs)]
    actual = runs_actual[0]

    failures: list[str] = []
    if query_def.get("expect_deterministic_across_runs"):
        first_json = sorted_json(runs_actual[0])
        for i, r in enumerate(runs_actual[1:], start=2):
            if sorted_json(r) != first_json:
                failures.append(f"expect_deterministic_across_runs: run {i} differs from run 1")

    failures.extend(_evaluate_search_expectations(query_def, actual))
    result: dict = {
        "type": "text_search",
        "name": query_def.get("name"),
        "actual": actual,
        "pass": not failures,
        "failures": failures,
    }
    if repeat_runs > 1:
        result["repeat_runs"] = repeat_runs
    return result


def execute_fallback_search(engine: Engine, query_def: dict) -> dict:
    """Run a fallback_search scenario query and emit the same rich result shape."""
    with_attribution = bool(query_def.get("with_match_attribution"))
    builder = engine.fallback_search(
        query_def["strict_query"],
        query_def.get("relaxed_query"),
        int(query_def.get("limit", 10)),
    )
    if query_def.get("kind_filter"):
        builder = builder.filter_kind_eq(query_def["kind_filter"])
    if with_attribution:
        builder = builder.with_match_attribution()

    rows = builder.execute()
    actual = _search_rows_to_actual(rows, with_attribution)
    failures = _evaluate_search_expectations(query_def, actual)
    return {
        "type": "fallback_search",
        "name": query_def.get("name"),
        "actual": actual,
        "pass": not failures,
        "failures": failures,
    }


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
        return execute_text_search(engine, query_def)

    if qtype == "fallback_search":
        return execute_fallback_search(engine, query_def)

    if qtype == "filter_content_ref_not_null":
        rows = engine.nodes(query_def["kind"]).filter_content_ref_not_null().limit(query_def.get("limit", 100)).execute()
        found_ids = sorted([n.logical_id for n in rows.nodes])
        return {"type": qtype, "count": len(rows.nodes), "found_ids": found_ids}

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

    atype = admin_def["type"]

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

    if atype == "register_fts_property_schema":
        record = engine.admin.register_fts_property_schema(
            admin_def["kind"], admin_def["property_paths"], admin_def.get("separator"))
        return {
            "type": "register_fts_property_schema",
            "kind": record.kind,
            "property_paths": list(record.property_paths),
            "separator": record.separator,
        }

    if atype == "register_fts_property_schema_with_entries":
        entries = [
            FtsPropertyPathSpec(
                path=str(e["path"]),
                mode=FtsPropertyPathMode(str(e.get("mode", "scalar"))),
            )
            for e in admin_def.get("entries", [])
        ]
        record = engine.admin.register_fts_property_schema_with_entries(
            admin_def["kind"],
            entries,
            admin_def.get("separator", " "),
            admin_def.get("exclude_paths", []),
        )
        return {
            "type": "register_fts_property_schema_with_entries",
            "kind": record.kind,
            "entries": [
                {"path": e.path, "mode": e.mode.value if hasattr(e.mode, "value") else str(e.mode)}
                for e in record.entries
            ],
            "separator": record.separator,
            "exclude_paths": list(record.exclude_paths),
        }

    if atype == "describe_fts_property_schema":
        record = engine.admin.describe_fts_property_schema(admin_def["kind"])
        if record is None:
            return {"type": "describe_fts_property_schema", "kind": admin_def["kind"], "found": False}
        return {
            "type": "describe_fts_property_schema",
            "kind": record.kind,
            "property_paths": record.property_paths,
            "separator": record.separator,
            "found": True,
        }

    if atype == "list_fts_property_schemas":
        schemas = engine.admin.list_fts_property_schemas()
        return {
            "type": "list_fts_property_schemas",
            "count": len(schemas),
            "kinds": sorted(s.kind for s in schemas),
        }

    raise ValueError(f"unknown admin type: {atype}")


def run_driver(db_path: str, mode: str) -> dict:
    """Run the driver in the specified mode and return the manifest."""
    raw = load_scenarios_file()
    scenarios = raw["scenarios"]
    engine = Engine.open(db_path)

    if mode == "write":
        # Run global setup_admin before any writes so schemas are in place.
        for admin_def in raw.get("setup_admin", []):
            execute_admin(engine, admin_def)
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
    """CLI entry point for the Python cross-language driver."""
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
