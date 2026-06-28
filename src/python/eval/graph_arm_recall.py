"""C1 graph-arm recall@K vs BM25 — extract a real entity/edge graph (local
Qwen3.6-27B via the Airlock vLLM batch gateway), write it into a FathomDB engine,
and compare ``use_graph_arm`` retrieval against BM25 / FTS-only on LongMemEval gold.

Graph model (Slice-30/ELPS): GLOBAL entities (``logical_id`` = normalized name,
``source_id`` = first session it appeared in) + per-session EDGES (``source_id`` =
the session the relation was extracted from, ``body`` = the relation triple text).
C1 seeds the BFS frontier from query-matched **entity-node FTS (source B)**, traverses
edges, and emits reached neighbors carrying the traversed edge's ``source_id`` — which
resolves to a gold session id (doc keys == gold session ids). NOTE: **source-A (edge-fact
FTS) seeding is OFF** here — setting edge bodies triggers `edge_fact` vector projection,
which hits an engine scale bug at ~10k+ edges (FOLLOW-UP). So this is a graph-arm recall
**lower bound** (source-B + traversal only).

The load-bearing comparison is **graph-arm ON vs OFF on the same engine**
(`fathomdb_graph` vs `fathomdb_graph_off`), which isolates the arm; `naive_bm25` and
`fathomdb_fts_only` (docs-only) are external references.

LLM is used ONLY for extraction (local, $0). recall@K is LLM-free.

Usage:
    python -m eval.graph_arm_recall --per-class 1 --db-dir /tmp/gar_dry --output /tmp/gar_dry.json
"""

from __future__ import annotations

import argparse
import io
import json
import os
import re
import time
from pathlib import Path
from typing import Any, Optional

# NOTE: `httpx` is an EVAL-ONLY dependency (not in `[dev]` extras) and is imported
# lazily inside extract_graph(), the only function that uses it. A module-level
# import broke `[dev]` test collection (test_m1_graph_build.py imports the offline
# `_norm`/`extract_graph` helpers and never makes an HTTP call) — 0.8.9.2.

from eval.p0a_base_retrieval import (
    DEFAULT_DATASET,
    DEFAULT_SPLIT,
    SMOKE_CLASSES,
    build_variants,
    load_lme_smoke,
    run_retrieval_loop,
)
from fathomdb import SearchFilter
from eval.r2_parity_eval import FathomDBAdapter, _make_doc_id_of

_AIRLOCK = "http://localhost:4000"
_KEY = os.environ.get("AIRLOCK_MASTER_KEY", "sk-airlock-mk")
_PROV = "vllm"
_ALIAS = "qwen36-27b-vllm-batch"
_EXTRACT_PROMPT = (
    "Extract the key entities and relationships from this chat session for a "
    "knowledge graph. Output ONLY compact JSON (no prose, no markdown): "
    '{"entities":[{"name":"..","type":".."}],'
    '"relations":[{"subject":"..","predicate":"..","object":".."}]}.\n\nSession:\n'
)


# --------------------------------------------------------------------------- #
# Extraction (local Qwen3.6-27B via Airlock vLLM batch — $0)
# --------------------------------------------------------------------------- #
def _salvage_json(content: str) -> Optional[dict]:
    """Parse extraction JSON; on truncation, salvage the complete prefix by
    closing the arrays/object at the last complete element."""
    c = content.strip()
    c = re.sub(r"^```(json)?", "", c).strip()
    c = re.sub(r"```$", "", c).strip()
    try:
        return json.loads(c)
    except Exception:
        pass
    # Truncation salvage: cut back to the last complete top-level "}," in a list
    # and close the structure. Best-effort — recovers complete entities/relations.
    start = c.find("{")
    if start < 0:
        return None
    frag = c[start:]
    last = frag.rfind("},")
    if last < 0:
        return None
    repaired = frag[: last + 1] + "]}"
    # We may have cut inside "relations": try closing entities-only too.
    for candidate in (repaired, frag[: last + 1] + "]}}"):
        try:
            return json.loads(candidate)
        except Exception:
            continue
    return None


def extract_graph(
    documents: dict[str, str], *, max_tokens: int = 3072, char_cap: int = 6000, log=print
) -> dict[str, dict]:
    """Batch-extract entities/relations for each session. Returns
    {session_id: {"entities":[...], "relations":[...]}}. Salvages truncations."""
    # httpx is eval-only (not in `[dev]`); imported lazily so `[dev]` collection
    # of the offline graph-build helpers doesn't require it (0.8.9.2).
    import httpx  # noqa: PLC0415 — intentional on-demand import

    items = list(documents.items())
    H = {"Authorization": f"Bearer {_KEY}"}
    P = {"custom_llm_provider": _PROV}
    cli = httpx.Client(timeout=1800.0)

    def mk(sid: str, body: str) -> dict:
        return {
            "custom_id": sid,
            "method": "POST",
            "url": "/v1/chat/completions",
            "body": {
                "model": _ALIAS,
                "messages": [{"role": "user", "content": _EXTRACT_PROMPT + body[:char_cap]}],
                "max_tokens": max_tokens,
                "chat_template_kwargs": {"enable_thinking": False},
            },
        }

    jsonl = "".join(json.dumps(mk(s, b)) + "\n" for s, b in items).encode()
    r = cli.post(
        f"{_AIRLOCK}/v1/files", headers=H, params=P, data={"purpose": "batch"},
        files={"file": ("extract.jsonl", io.BytesIO(jsonl), "application/jsonl")},
    )
    r.raise_for_status()
    fid = r.json()["id"]
    t0 = time.time()
    r = cli.post(
        f"{_AIRLOCK}/v1/batches", headers={**H, "Content-Type": "application/json"}, params=P,
        json={"input_file_id": fid, "endpoint": "/v1/chat/completions",
              "completion_window": "24h", "model": _ALIAS},
    )
    r.raise_for_status()
    bid = r.json()["id"]
    st: dict = {}
    status = None
    for _ in range(4000):  # ~5.5h cap at 5s — bounds a hung batch
        st = cli.get(f"{_AIRLOCK}/v1/batches/{bid}", headers=H, params=P).json()
        status = st.get("status")
        if status in ("completed", "failed", "cancelled"):
            break
        time.sleep(5)
    log(f"[extract] {status} counts={st.get('request_counts')} wall={time.time() - t0:.0f}s")
    if status != "completed" or not st.get("output_file_id"):
        raise RuntimeError(f"extraction batch did not complete: status={status} errors={st.get('errors')}")
    out = cli.get(f"{_AIRLOCK}/v1/files/{st['output_file_id']}/content", headers=H, params=P)
    graphs: dict[str, dict] = {}
    ok = salvaged = failed = 0
    for line in out.text.strip().splitlines():
        o = json.loads(line)
        sid = o["custom_id"]
        if o.get("error") or not o.get("response"):
            failed += 1
            continue
        try:
            content = o["response"]["body"]["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError):
            failed += 1
            continue
        try:
            j = json.loads(content.strip())
            ok += 1
        except Exception:
            j = _salvage_json(content)
            if j is None:
                failed += 1
                continue
            salvaged += 1
        graphs[sid] = {"entities": j.get("entities", []) or [], "relations": j.get("relations", []) or []}
    log(f"[extract] parsed clean={ok} salvaged={salvaged} failed={failed} -> {len(graphs)} graphs")
    return graphs


# --------------------------------------------------------------------------- #
# Graph build
# --------------------------------------------------------------------------- #
def _norm(name: Any) -> Optional[str]:
    if not isinstance(name, str):
        return None
    s = re.sub(r"\s+", " ", name.strip().lower())
    return f"ent:{s}" if s else None


def build_graph_engine(documents: dict[str, str], graphs: dict[str, dict], db_path: Path, *, log=print):
    """Write docs (cursor-resolved) + global entities + per-session edges. Returns
    (engine, cursor_to_doc)."""
    from fathomdb.engine import Engine

    if db_path.exists():
        db_path.unlink()
    # FTS-only (no embedder). NOTE: we deliberately write edges WITHOUT a body so the
    # engine does NOT auto-enqueue `edge_fact` vector projection (lib.rs:8594). That
    # path requires an embedder (else SchedulerError) AND hits a StorageError in the
    # edge_fact embedding pipeline under heavy write load (~8-14k edges) — an engine
    # scale bug (FOLLOW-UP). Consequence: C1 **source-A (edge-fact FTS) seeding is OFF**
    # here; the graph arm runs on **source-B (entity-node FTS) + edge traversal** only —
    # the primary mechanism, and a recall LOWER BOUND. The edge-body binding fix stands;
    # it's the engine edge_fact pipeline that can't yet take this scale.
    engine = Engine.open(str(db_path), use_default_embedder=False)
    cursor_to_doc: dict[int, str] = {}

    # Docs (session bodies) — source_id None → resolve via cursor map.
    items = list(documents.items())
    for i in range(0, len(items), 64):
        chunk = items[i : i + 64]
        receipt = engine.write([{"kind": "doc", "body": b} for _, b in chunk])
        for (sid, _b), cur in zip(chunk, receipt.row_cursors):
            cursor_to_doc[int(cur)] = sid

    # Global entities: logical_id = normalized name; source_id = first session seen.
    ent_first: dict[str, str] = {}
    ent_name: dict[str, str] = {}
    for sid, g in graphs.items():
        for e in g["entities"]:
            lid = _norm(e.get("name") if isinstance(e, dict) else e)
            if lid and lid not in ent_first:
                ent_first[lid] = sid
                ent_name[lid] = (e.get("name") if isinstance(e, dict) else e) or lid
    # kind="entity" (NOT "doc") so the two-arm doc-retrieval can filter them out
    # (kind="doc") — they pollute recall otherwise — while source-B graph seeding
    # (which queries the FTS index directly, unfiltered) still finds them.
    ent_writes = [
        {"kind": "entity", "body": ent_name[lid], "logical_id": lid, "source_id": ent_first[lid]}
        for lid in ent_first
    ]
    for i in range(0, len(ent_writes), 64):
        engine.write(ent_writes[i : i + 64])

    # Edges: from/to known entities; source_id = the session the relation came from.
    edge_writes = []
    n_edge = 0
    for sid, g in graphs.items():
        for r in g["relations"]:
            if not isinstance(r, dict):
                continue
            subj, obj = r.get("subject"), r.get("object")
            f, t = _norm(subj), _norm(obj)
            if not f or not t or f == t or f not in ent_first or t not in ent_first:
                continue
            n_edge += 1
            # NO body — avoids edge_fact vector projection (see build_graph_engine note).
            # The relation still connects the two entities for BFS traversal; provenance
            # rides the edge's source_id. (Source-A edge-fact FTS is OFF: engine follow-up.)
            edge_writes.append({"edge": {
                "kind": "rel", "from": f, "to": t, "source_id": sid,
                "logical_id": f"e:{sid}:{n_edge}",
            }})
    for i in range(0, len(edge_writes), 64):
        engine.write(edge_writes[i : i + 64])

    engine.drain(timeout_s=300)
    log(f"[graph] docs={len(items)} entities={len(ent_first)} edges={len(edge_writes)}")
    return engine, cursor_to_doc


# --------------------------------------------------------------------------- #
# Main
# --------------------------------------------------------------------------- #
def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="C1 graph-arm recall@K vs BM25 (local-extracted graph)")
    ap.add_argument("--per-class", type=int, default=1)
    ap.add_argument("--seed", type=int, default=20260614)
    ap.add_argument("--max-tokens", type=int, default=3072)
    ap.add_argument("--db-dir", default="/tmp/gar")
    ap.add_argument("--output", required=True)
    args = ap.parse_args(argv)

    db_dir = Path(args.db_dir)
    db_dir.mkdir(parents=True, exist_ok=True)
    smoke = load_lme_smoke(DEFAULT_DATASET, DEFAULT_SPLIT, per_class=args.per_class,
                           seed=args.seed, classes=SMOKE_CLASSES)
    print(f"[load] {len(smoke.questions)} questions | {len(smoke.documents)} haystack sessions")

    # Extraction cache (incremental) — reuse any already-extracted sessions and
    # extract only the MISSING ones, then merge. Extraction is the wall-clock cost;
    # the graph build + recall iterate fast.
    cache = db_dir / "extractions.json"
    cached = json.loads(cache.read_text()) if cache.exists() else {}
    cached = {k: v for k, v in cached.items() if k in smoke.documents}
    missing = {k: b for k, b in smoke.documents.items() if k not in cached}
    if missing:
        print(f"[extract] {len(cached)} cached, extracting {len(missing)} new")
        new = extract_graph(missing, max_tokens=args.max_tokens)
        graphs = {**cached, **new}
        cache.write_text(json.dumps(graphs))
        print(f"[extract] cached -> {cache} ({len(graphs)} total)")
    else:
        graphs = cached
        print(f"[extract] reused cache ({len(graphs)} graphs, 0 new)")
    g_engine, cursor_to_doc = build_graph_engine(smoke.documents, graphs, db_dir / "graph.sqlite")

    # Resolution sanity: a query for a known entity must yield graph_arm hits whose
    # source_id is a real haystack session id.
    sample_q = smoke.questions[0].question
    res = g_engine.search(sample_q, use_graph_arm=True)
    ga = [(h.body[:40], h.source_id) for h in res.results if h.branch == "graph_arm"]
    resolvable = sum(1 for _, sid in ga if sid in smoke.documents)
    print(f"[sanity] q={sample_q[:60]!r} -> {len(ga)} graph_arm hits, "
          f"{resolvable} resolve to a haystack session; sample={ga[:3]}")

    # bm25 + fts_only (docs-only) = external references; the load-bearing comparison
    # is graph-arm ON vs OFF on the SAME graph engine — that isolates the arm from the
    # entity-FTS-pollution confound (the graph engine's two-arm result includes entity
    # hits regardless of the arm, so `graph_off` is the correct apples-to-apples baseline).
    systems, _ = build_variants(smoke.documents, db_dir, include_fused=False)
    g_doc_id = _make_doc_id_of(cursor_to_doc)
    # kind="doc" filter on the two-arm excludes entity nodes (anti-pollution); graph
    # seeding is unaffected (queries the FTS index directly).
    doc_filter = SearchFilter(kind="doc")
    systems["fathomdb_graph_off"] = FathomDBAdapter(
        g_engine, doc_id_of=g_doc_id, use_graph_arm=False, search_filter=doc_filter)
    systems["fathomdb_graph"] = FathomDBAdapter(
        g_engine, doc_id_of=g_doc_id, use_graph_arm=True, search_filter=doc_filter)
    retrieval = run_retrieval_loop(smoke, systems)

    result = {
        "mode": "graph-arm-recall",
        "extractor": "qwen3.6-27b (local vLLM, enable_thinking=false)",
        "n_questions": len(smoke.questions),
        "n_haystack": len(smoke.documents),
        "n_graphs": len(graphs),
        "variants": sorted(systems.keys()),
        "sanity_graph_hits": len(ga),
        "sanity_resolvable": resolvable,
        "retrieval_loop": retrieval,
    }
    Path(args.output).write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(f"[gar] wrote {args.output}")
    # Compact readout — pool R@10 over SCORED questions per class (recall_at_10 is
    # None for an all-abstention class; weight by n_scored, skip None).
    for v in ("naive_bm25", "fathomdb_fts_only", "fathomdb_graph_off", "fathomdb_graph"):
        blk = retrieval.get(v, {}).get("per_class", {})
        if not blk:
            continue
        num = den = 0.0
        for c, m in blk.items():
            rv, ns = m.get("recall_at_10"), m.get("n_scored", 0)
            if rv is not None and ns:
                num += rv * ns
                den += ns
        pooled = round(num / den, 3) if den else None
        print(f"  {v}: pooled R@10={pooled}")
    g_engine.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
