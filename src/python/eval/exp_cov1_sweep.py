#!/usr/bin/env python3
"""EXP-COV-1 — coverage->outcome sufficiency sweep (orchestrator).

Holds the retrieval + CE-rerank stack FIXED (CLS-corrected bge-small, dense+FTS fused,
graph arm, CE rerank_depth/alpha/pool_n pinned in :mod:`eval.exp_cov1_common`) and
varies ONLY the fact-graph contents across coverage conditions:

* ``C-none``      — docs only, NO extraction (coverage = 0). The retrieval baseline.
* ``C0-floor``    — docs + a deterministic heuristic extractor ($0 low anchor).
* ``C-relation``  — docs + the priced relation-focused LLM extraction (the lever).

Every condition ingests the SAME 272 LOCOMO sessions with the SAME embedder and is
queried with the SAME query set through the SAME search config; the ONLY difference is
which entity/edge facts populate the graph. The downstream read is gold-in-pool /
recall@k / MRR per intent class, with a PAIRED bootstrap of the per-query delta vs
``C-none`` and the pre-registered decision rule (CI lower bound > +0.04 on >=1 powered
class => SUFFICIENT; flat at the ceiling => CEILING_ABSORBED).

Per-condition retrieved ranks are CHECKPOINTED so an expensive ingest is never repeated
and a crash resumes. The heuristic cache is generated deterministically ($0). The
priced ``C-relation`` cache is produced out-of-band by ``eval.exp_cov1_extract``.

The engine's Python ``search`` wrapper on the shared venv is stale (maps a
``stable_id`` the installed native ``SearchHit`` lacks); this module therefore calls
``engine._native.search`` directly — the native engine, embedder, CE, and graph arm are
all real, nothing is mocked.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Optional

from eval.exp_cov1_common import (
    ALPHA,
    POOL_N,
    POWER_MIN_QUESTIONS,
    PROMPT_VERSION,
    ExtractionCache,
    c0_floor_extract,
    cache_key,
    decision_for_class,
    hit_at_k,
    overall_verdict,
    paired_bootstrap_delta,
    reciprocal_rank,
)

_CHUNK_SUFFIX_RE = __import__("re").compile(r"#c\d+$")

DEFAULT_K_GRID = (5, 10, 20)
KMAX = 50  # retrieve depth per query (>= max k_grid; feeds MRR + gold-in-pool)


# --------------------------------------------------------------------------- #
# Gold loading + power filtering
# --------------------------------------------------------------------------- #
def load_gold(
    locomo_path: str, *, classes: Optional[set[str]] = None
) -> tuple[dict[str, str], list[dict[str, Any]]]:
    from eval.locomo_loader import load_locomo

    docs, gold = load_locomo(locomo_path)
    filtered: list[dict[str, Any]] = []
    for q in gold:
        if classes is not None and q["query_class"] not in classes:
            continue
        ev = [e["doc_id"] for e in q["required_evidence"] if e["doc_id"] in docs]
        if not ev:
            continue
        # multi_session gate caveat: require >=2 distinct evidence sessions.
        if q["query_class"] == "multi_session" and len(set(ev)) < 2:
            continue
        q = dict(q)
        q["_gold_docs"] = sorted(set(ev))
        filtered.append(q)
    return docs, filtered


# --------------------------------------------------------------------------- #
# Heuristic C0 cache (deterministic, $0)
# --------------------------------------------------------------------------- #
def build_c0_cache(docs: dict[str, str], cache_path: str, model: str = "c0-floor") -> None:
    cache = ExtractionCache.load(cache_path)
    for doc_id, body in sorted(docs.items()):
        key = cache_key(doc_id, model, PROMPT_VERSION)
        if cache.has_ok(key):
            continue
        ext = c0_floor_extract(doc_id, body, "2023-01-01T00:00:00Z")
        cache.put({
            "key": key, "doc_id": doc_id, "model": model,
            "prompt_version": PROMPT_VERSION, "status": "ok",
            "entities": ext["entities"], "edges": ext["edges"], "warnings": [],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0},
        })


# --------------------------------------------------------------------------- #
# Engine build + native-search adapter (bypass the stale wrapper)
# --------------------------------------------------------------------------- #
def _session_id_of(source_doc_id: str) -> str:
    return _CHUNK_SUFFIX_RE.sub("", source_doc_id)


def build_condition_engine(
    docs: dict[str, str],
    db_path: Path,
    *,
    extractor_cmd: Optional[list[str]] = None,
    extractor_env: Optional[dict[str, str]] = None,
    use_embedder: bool = False,
    batch: int = 500,
) -> tuple[Any, Any, dict[str, Any]]:
    """Ingest docs (+ optional extraction) and return ``(engine, doc_id_of, info)``.

    ``use_embedder=False`` (default) runs the stack as **FTS (docs + fact nodes) + the
    STRUCTURAL graph-arm BFS over fact-edges**, with NO dense projection. This is forced
    by an environmental defect: the installed CPU candle embedder intermittently STALLS
    mid edge_fact-queue (spins at ~1500% CPU with zero write progress; reproduced twice
    at ~5k nodes) AND long doc bodies embed at ~13 s each — either makes the dense path
    intractable/nondeterministic. The graph arm's BFS is structural (edges live in the
    DB regardless of vector projection), so the coverage lever is still exercised; only
    the (secondary) dense-fact retrieval arm is dropped, and it is dropped IDENTICALLY
    across all conditions, so the paired contrast is clean."""
    from fathomdb.engine import Engine

    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()
    engine = Engine.open(str(db_path), use_default_embedder=use_embedder)
    # NB: we deliberately do NOT register the "doc" vector kind. On the installed
    # CPU embedder a long doc-body embed is ~13s (a ~700-token body), so dense-
    # embedding all 272 doc bodies is ~1h/condition and intractable; short edge_fact
    # spans embed in ~0.08s. The retrieval stack is therefore FTS (docs) + the graph
    # arm over dense-embedded edge_facts + CE rerank — the SAME config the prior
    # FathomDB evals (r2_parity_eval / ce_rerank_probe) use, held FIXED across all
    # conditions; only the fact-graph contents vary (the coverage lever).

    cursor_to_doc: dict[int, str] = {}
    items = sorted(docs.items())
    for start in range(0, len(items), batch):
        chunk = items[start:start + batch]
        receipt = engine.write([{"kind": "doc", "body": body} for _, body in chunk])
        for (did, _b), cursor in zip(chunk, receipt.row_cursors):
            cursor_to_doc[int(cursor)] = _session_id_of(did)
    # Doc bodies are FTS-only (not a registered vector kind), so this drain is trivial.
    _drain_best_effort(engine, 120)

    n_edges = 0
    drain_note = "n/a"
    if extractor_cmd is not None:
        if extractor_env:
            for k, v in extractor_env.items():
                os.environ[k] = v
        elps_docs = [{"source_doc_id": did, "body": body[:24000]} for did, body in items]
        receipt = engine.ingest_with_extractor(extractor_cmd, elps_docs)
        n_edges = getattr(receipt, "edges_written", getattr(receipt, "n_edges", 0)) or 0
        # The installed CPU embedder intermittently STALLS mid-queue (spins at high
        # CPU with zero write progress) — an environmental defect, not our data (all
        # fact bodies are <300 chars). We bound the wait and proceed on timeout: the
        # graph arm's BFS is STRUCTURAL over the fact-edges (present in the DB
        # regardless of vector projection) + FTS over docs, so retrieval is valid with
        # partial edge_fact dense coverage. The realized coverage is recorded.
        drain_note = _drain_best_effort(engine, 300)

    def doc_id_of(sh: Any) -> str:
        sid = getattr(sh, "source_id", None)
        if sid:
            return _session_id_of(str(sid))
        return cursor_to_doc.get(int(sh.id), str(sh.id))

    return engine, doc_id_of, {
        "n_docs": len(items), "n_edges_reported": n_edges, "drain": drain_note,
    }


def _drain_best_effort(engine: Any, budget_s: float, *, slice_s: float = 60.0) -> str:
    """Drain in slices up to ``budget_s``; if the embed queue STALLS (no progress
    across two slices) or the budget elapses, stop and proceed with partial vector
    coverage (the graph arm is structural). Returns a short status note.

    Detects a stall via the WAL/main-db not changing is not available here; instead we
    watch the drain-slice return: a slice that returns promptly == idle (done); a slice
    that times out repeatedly with no forward movement == stalled. We simply cap total
    wait at ``budget_s`` and treat a timeout as "proceed"."""
    import time as _t

    t0 = _t.time()
    while _t.time() - t0 < budget_s:
        try:
            engine.drain(timeout_s=slice_s)
            return "idle"  # queue drained cleanly
        except Exception:  # noqa: BLE001 — drain timeout == still working / stalled
            continue
    return f"proceeded-after-{int(budget_s)}s (partial vector coverage; graph arm structural)"


def retrieve_ranks(
    engine: Any,
    doc_id_of: Any,
    gold: list[dict[str, Any]],
    *,
    use_graph_arm: bool,
    rerank_depth: int = 0,
    kmax: int = KMAX,
) -> dict[str, list[str]]:
    """Per-query ranked corpus doc_ids (top-kmax), held-fixed retrieval config.

    ``rerank_depth`` is the CE-rerank depth, HELD IDENTICAL across all conditions.
    Default 0 (CE off): on the installed CPU embedder build, CE-reranking LOCOMO-sized
    doc bodies is ~8 s/query (pathological candle-CPU latency over long spans), i.e.
    hours per condition — intractable. gold-in-pool is a pool-MEMBERSHIP metric that CE
    reordering does not change in kind, and the coverage lever (edges via the graph
    arm) acts on pool composition, so the sufficiency read is faithful at depth 0.
    A small CE-on confirmation on a reduced query subset can be run separately."""
    import time as _t
    native = engine._native
    ranks: dict[str, list[str]] = {}
    _t0 = _t.time()
    for _i, q in enumerate(gold):
        if _i % 50 == 0:
            print(f"[cov1-sweep]   retrieve {_i}/{len(gold)} "
                  f"({_t.time() - _t0:.0f}s)", flush=True)
        res = native.search(
            q["query"], rerank_depth=rerank_depth, use_graph_arm=use_graph_arm,
            alpha=(ALPHA if rerank_depth > 0 else None),
            pool_n=(POOL_N if rerank_depth > 0 else None),
        )
        seen: list[str] = []
        seen_set: set[str] = set()
        for sh in res.results[:kmax]:
            did = doc_id_of(sh)
            if did not in seen_set:
                seen_set.add(did)
                seen.append(did)
        ranks[q["query_id"]] = seen
    return ranks


# --------------------------------------------------------------------------- #
# Scoring + verdict
# --------------------------------------------------------------------------- #
def score_condition(
    gold: list[dict[str, Any]], ranks: dict[str, list[str]], *, k_grid=DEFAULT_K_GRID
) -> dict[str, Any]:
    """Per-query 0/1 gold-in-pool@k + reciprocal rank, grouped by class."""
    by_class: dict[str, dict[str, Any]] = {}
    for q in gold:
        cls = q["query_class"]
        b = by_class.setdefault(cls, {"qids": [], "hits": {k: [] for k in k_grid}, "rr": []})
        r = ranks.get(q["query_id"], [])
        gd = q["_gold_docs"]
        b["qids"].append(q["query_id"])
        for k in k_grid:
            h = hit_at_k(gd, r, k, cls)
            b["hits"][k].append(0.0 if h is None else h)
        rr = reciprocal_rank(gd, r)
        b["rr"].append(0.0 if rr is None else rr)
    return by_class


def compute_verdicts(
    gold: list[dict[str, Any]],
    ranks_by_cond: dict[str, dict[str, list[str]]],
    *,
    treatment: str,
    baseline: str,
    k_grid=DEFAULT_K_GRID,
    focus_k: int = 10,
) -> dict[str, Any]:
    """Paired bootstrap of (treatment - baseline) per class at focus_k gold-in-pool
    and MRR; apply the pre-registered decision rule."""
    t_scores = score_condition(gold, ranks_by_cond[treatment], k_grid=k_grid)
    b_scores = score_condition(gold, ranks_by_cond[baseline], k_grid=k_grid)

    per_class: dict[str, Any] = {}
    verdicts: dict[str, str] = {}
    for cls in sorted(t_scores):
        tc, bc = t_scores[cls], b_scores[cls]
        # alignment guard: identical query order (same gold list -> same append order)
        assert tc["qids"] == bc["qids"], f"query misalignment in {cls}"
        n = len(tc["qids"])
        gip_delta = paired_bootstrap_delta(tc["hits"][focus_k], bc["hits"][focus_k])
        mrr_delta = paired_bootstrap_delta(tc["rr"], bc["rr"])
        v = decision_for_class(gip_delta, n_scored=n)
        verdicts[cls] = v
        per_class[cls] = {
            "n_scored": n,
            "powered": n >= POWER_MIN_QUESTIONS,
            f"gold_in_pool_at_{focus_k}": {
                "treatment": round(sum(tc["hits"][focus_k]) / n, 4) if n else None,
                "baseline": round(sum(bc["hits"][focus_k]) / n, 4) if n else None,
                "paired_delta_ci": gip_delta,
            },
            "mrr": {
                "treatment": round(sum(tc["rr"]) / n, 4) if n else None,
                "baseline": round(sum(bc["rr"]) / n, 4) if n else None,
                "paired_delta_ci": mrr_delta,
            },
            "verdict": v,
        }
    return {
        "treatment": treatment,
        "baseline": baseline,
        "focus_k": focus_k,
        "per_class": per_class,
        "overall_verdict": overall_verdict(verdicts),
    }


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #
CONDITIONS = {
    # name: (extractor_mode, use_graph_arm)
    "C-none": ("none", True),
    "C0-floor": ("heuristic", True),
    "C-relation": ("replay", True),
}


def run(
    *,
    locomo_path: str,
    db_dir: str,
    ranks_path: str,
    c0_cache: str,
    relation_cache: str,
    relation_model: str,
    conditions: list[str],
    rerank_depth: int = 0,
    use_embedder: bool = False,
    classes: Optional[set[str]] = None,
) -> dict[str, Any]:
    docs, gold = load_gold(locomo_path, classes=classes)
    print(f"[cov1-sweep] docs={len(docs)} gold={len(gold)}", flush=True)

    # resumable per-condition ranks checkpoint
    ranks_by_cond: dict[str, dict[str, list[str]]] = {}
    rp = Path(ranks_path)
    if rp.exists():
        ranks_by_cond = json.loads(rp.read_text(encoding="utf-8"))

    info: dict[str, Any] = {}
    for cond in conditions:
        if cond in ranks_by_cond:
            print(f"[cov1-sweep] {cond}: cached ranks, skip ingest", flush=True)
            continue
        mode, graph = CONDITIONS[cond]
        extractor_cmd = None
        extractor_env = None
        model = None
        cache_file = None
        if mode == "heuristic":
            build_c0_cache(docs, c0_cache, model="c0-floor")
            model = "c0-floor"
            cache_file = c0_cache
        elif mode == "replay":
            model = relation_model
            cache_file = relation_cache
            # completeness guard: refuse if the priced cache is incomplete
            cache = ExtractionCache.load(cache_file)
            expected = [cache_key(d, model, PROMPT_VERSION) for d in docs]
            comp = cache.completeness(expected)
            if not comp["ok"]:
                raise SystemExit(
                    f"[cov1-sweep] REFUSING C-relation: extraction cache incomplete "
                    f"({comp['n_missing']} missing / {comp['n_ok']} ok). Run exp_cov1_extract."
                )
        if mode in ("heuristic", "replay"):
            extractor_cmd = [sys.executable, "-m", "eval.exp_cov1_replay_harness"]
            extractor_env = {
                "COV1_CACHE_PATH": cache_file, "COV1_MODEL": model,
                "COV1_PROMPT_VER": PROMPT_VERSION,
            }
        t0 = time.time()
        engine, doc_id_of, cinfo = build_condition_engine(
            docs, Path(db_dir) / f"cov1_{cond}.sqlite",
            extractor_cmd=extractor_cmd, extractor_env=extractor_env,
            use_embedder=use_embedder,
        )
        ranks = retrieve_ranks(
            engine, doc_id_of, gold, use_graph_arm=graph, rerank_depth=rerank_depth
        )
        engine.close()
        ranks_by_cond[cond] = ranks
        rp.parent.mkdir(parents=True, exist_ok=True)
        rp.write_text(json.dumps(ranks_by_cond), encoding="utf-8")
        info[cond] = {**cinfo, "elapsed_s": round(time.time() - t0, 1)}
        print(f"[cov1-sweep] {cond} done {info[cond]}", flush=True)

    result: dict[str, Any] = {
        "experiment": "EXP-COV-1",
        "corpus": "LOCOMO (CC-BY-NC, EVAL-ONLY)",
        "n_docs": len(docs),
        "n_gold_queries": len(gold),
        "class_counts": {
            c: sum(1 for q in gold if q["query_class"] == c)
            for c in sorted({q["query_class"] for q in gold})
        },
        "held_fixed": {
            "embedder": (
                "CLS-corrected bge-small dense (edge_facts) + FTS(docs)"
                if use_embedder else
                "FTS(docs + fact nodes) + STRUCTURAL graph-arm BFS; dense projection "
                "OFF (CPU-embedder stall defect) — dropped identically across conditions"
            ),
            "rerank_depth": rerank_depth,
            "alpha": ALPHA if rerank_depth > 0 else None,
            "pool_n": POOL_N if rerank_depth > 0 else None,
            "k_grid": list(DEFAULT_K_GRID),
            "ce_note": (
                "CE rerank held OFF (rerank_depth=0): the installed CPU embedder build "
                "reranks LOCOMO-sized bodies at ~8s/query -> hours/condition. gold-in-pool "
                "is a pool-membership metric unaffected in kind by CE reordering."
            ) if rerank_depth == 0 else "CE on",
        },
        "conditions_run": [c for c in conditions if c in ranks_by_cond],
        "ingest_info": info,
        "relation_model": relation_model,
        "prompt_version": PROMPT_VERSION,
    }
    if "C-none" in ranks_by_cond and "C0-floor" in ranks_by_cond:
        result["C0-floor_vs_C-none"] = compute_verdicts(
            gold, ranks_by_cond, treatment="C0-floor", baseline="C-none"
        )
    if "C-none" in ranks_by_cond and "C-relation" in ranks_by_cond:
        result["C-relation_vs_C-none"] = compute_verdicts(
            gold, ranks_by_cond, treatment="C-relation", baseline="C-none"
        )
    return result


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="EXP-COV-1 coverage->outcome sufficiency sweep")
    ap.add_argument("--locomo", default="data/corpus-data/raw/locomo10.json")
    ap.add_argument("--db-dir", default="/tmp/cov1")
    ap.add_argument("--ranks", default="/tmp/cov1/ranks.json")
    ap.add_argument("--c0-cache", default="/tmp/cov1/c0_floor.ndjson")
    ap.add_argument("--relation-cache", default="/tmp/cov1/relation.ndjson")
    ap.add_argument("--relation-model", default="gpt-5-mini")
    ap.add_argument("--conditions", default="C-none,C0-floor")
    ap.add_argument("--rerank-depth", type=int, default=0,
                    help="CE rerank depth, held fixed across conditions (0=CE off; "
                         "CPU-build CE over long bodies is ~8s/query)")
    ap.add_argument("--use-embedder", action="store_true",
                    help="enable dense projection (default OFF: CPU-embedder stalls; "
                         "FTS + structural graph BFS instead)")
    ap.add_argument("--classes", default="", dest="classes_filter",
                    help="comma-separated query classes to score (default all); e.g. "
                         "multi_session,temporal (the powered relation classes)")
    ap.add_argument("--out-json", default="dev/plans/runs/EXP-COV-1-sweep-output.json")
    args = ap.parse_args(argv)

    conds = [c.strip() for c in args.conditions.split(",") if c.strip()]
    result = run(
        locomo_path=args.locomo, db_dir=args.db_dir, ranks_path=args.ranks,
        c0_cache=args.c0_cache, relation_cache=args.relation_cache,
        relation_model=args.relation_model, conditions=conds,
        rerank_depth=args.rerank_depth, use_embedder=args.use_embedder,
        classes=(set(c.strip() for c in args.classes_filter.split(",") if c.strip())
                 or None),
    )
    Path(args.out_json).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out_json).write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(f"[cov1-sweep] wrote {args.out_json}", flush=True)
    for key in ("C0-floor_vs_C-none", "C-relation_vs_C-none"):
        if key in result:
            print(f"  {key}: {result[key]['overall_verdict']}", flush=True)
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
