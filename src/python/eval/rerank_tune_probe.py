"""0.8.3 Slice-20 CE-rerank TUNING probe ($0 / CPU / LLM-free).

The CE-rerank ACCURACY arm (``eval.rerank_accuracy_run``) returned a citable
**PASS but marginal NO-GO**: the lever is real (+0.0495 pooled answer accuracy,
powered) but closes only ~27% of the Mem0 gap at the *production* config
(``ALPHA=0.3`` CE-blend, ``pool_n=50``, ``K=10``). Before spending another priced
arm, this probe collects **$0** data on the rerank knobs to pick the best config.

The production blend (``fathomdb-engine`` ``ce_rerank``) is::

    ce_norm  = sigmoid(raw_logit)                 # intrinsic per (query, passage)
    rrf_norm = minmax(base_score) over the top-N pool
    blended  = ALPHA * ce_norm + (1 - ALPHA) * rrf_norm   # ALPHA = 0.3 (hardcoded)

ALPHA (the CE-blend weight) is the dominant knob and is **not** tunable from the
Python ``rerank`` binding. But it can be swept **offline from a single CE pass**:
calling ``fathomdb.rerank`` with every passage ``score = 0.0`` forces ``rrf_span = 0``
⇒ ``rrf_norm = 1.0`` ⇒ ``blended = 0.3 * ce_norm + 0.7`` ⇒
``ce_norm = (blended - 0.7) / 0.3`` — the pure CE signal, recovered exactly. With
``ce_norm`` per passage AND the real base scores in hand, ANY ``(alpha, pool_n, K)``
config is re-blended + scored offline with no further model calls.

Phase 1 (priced $0, ~CPU): one CE pass over each query's base top-``MAX_POOL`` pool
→ checkpointed per-query ``[(doc_id, base_score, ce_norm, is_gold)]`` (resumable;
the CE pass is the only slow part).

Phase 2 (offline, seconds): sweep ``alpha`` x ``pool_n`` and report, per
(reporting_class, pooled), strict Recall@K for K in :data:`KS`, mean reciprocal
first-gold rank (MRR), and the mean rank needed to capture ALL gold. ``alpha=0.0``
is the pure-base baseline (== no rerank); ``alpha=0.3`` is production.

This is an exploratory $0 probe whose conclusion is RE-VALIDATED by the next priced
accuracy arm — it informs which config that arm runs, it does not itself gate a GO.
"""

from __future__ import annotations

import argparse
import json
import time
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Optional

# Production blend constants (mirror fathomdb-engine ``ce_rerank``; design above).
PROD_ALPHA: float = 0.3
#: The fixed offset when every passage score is 0.0: blended = ALPHA*ce + (1-ALPHA)*1.
_BASE_OFFSET: float = 1.0 - PROD_ALPHA  # 0.7

#: Top-N pool the CE pass scores once per query (the superset of every swept pool_n).
MAX_POOL: int = 100
#: Swept candidate-pool sizes (base top-pool_n the rerank reorders).
POOL_NS: tuple[int, ...] = (10, 20, 50, 100)
#: Swept CE-blend weights. 0.0 = pure base (no rerank / baseline); 0.3 = production;
#: 1.0 = pure CE.
ALPHAS: tuple[float, ...] = (0.0, 0.1, 0.2, 0.3, 0.5, 0.7, 1.0)
#: Recall@K cut-offs read off each ranking (order-sensitive within the pool).
KS: tuple[int, ...] = (1, 3, 5, 10, 20)

#: The four agentic-memory classes (== decision_rule_083 / the accuracy arm).
from eval.decision_rule_083 import MEMORY_CLASSES  # noqa: E402

GAP_CLASSES: tuple[str, ...] = MEMORY_CLASSES


# --------------------------------------------------------------------------- #
# Pure helpers (backend-free; unit-testable with fakes).
# --------------------------------------------------------------------------- #


def recover_ce_norm(blended_at_zero: float) -> float:
    """Recover the intrinsic ``ce_norm`` from a ``score=0`` rerank output.

    With every passage score 0, ``rrf_norm = 1.0`` so
    ``blended = PROD_ALPHA * ce_norm + (1 - PROD_ALPHA)``. Inverts + clamps to
    ``[0, 1]`` (float noise can push a hair outside)."""
    ce = (blended_at_zero - _BASE_OFFSET) / PROD_ALPHA
    return 0.0 if ce < 0.0 else 1.0 if ce > 1.0 else ce


def minmax_norm(values: Sequence[float]) -> list[float]:
    """Min-max normalize ``values`` to ``[0, 1]`` over the pool (matches the engine's
    ``rrf_norm``: a zero span ⇒ all ``1.0``)."""
    if not values:
        return []
    lo = min(values)
    hi = max(values)
    span = hi - lo
    if span <= 0.0:
        return [1.0 for _ in values]
    return [(v - lo) / span for v in values]


def reranked_doc_order(
    pool: Sequence[Mapping[str, Any]], *, alpha: float, pool_n: int
) -> list[str]:
    """The reranked unique doc-id order for one query at ``(alpha, pool_n)``.

    Takes the base top-``pool_n`` passages (``pool`` is in base-rank order), blends
    ``alpha * ce_norm + (1 - alpha) * rrf_norm`` (``rrf_norm`` = min-max of the base
    score over that sub-pool), sorts descending (stable within ties ⇒ base order
    breaks ties, mirroring the engine's stable sort), and dedupes doc-ids in rank
    order. ``alpha == 0.0`` ⇒ pure base order (the no-rerank baseline)."""
    cand = list(pool[:pool_n])
    rrf = minmax_norm([float(p["base_score"]) for p in cand])
    scored = [
        (alpha * float(p["ce_norm"]) + (1.0 - alpha) * rrf[i], i, p)
        for i, p in enumerate(cand)
    ]
    # Sort by blended desc; ties broken by original base index (stable ascending i).
    scored.sort(key=lambda t: (-t[0], t[1]))
    seen: list[str] = []
    for _score, _i, p in scored:
        did = str(p["doc_id"])
        if did not in seen:
            seen.append(did)
    return seen


def strict_recall_at_k(retrieved: Sequence[str], gold: Sequence[str], k: int) -> float:
    """All-or-nothing Recall@K (mirror :func:`eval.d0b_powered_recall`): 1.0 iff every
    gold id is in the top-K, else 0.0. Empty gold never scores 1.0."""
    if not gold:
        return 0.0
    top = set(retrieved[:k])
    return 1.0 if all(g in top for g in gold) else 0.0


def first_gold_rank(retrieved: Sequence[str], gold: Sequence[str]) -> Optional[int]:
    """1-based rank of the FIRST gold id in the order, or ``None`` if no gold present."""
    gset = set(gold)
    for i, did in enumerate(retrieved, start=1):
        if did in gset:
            return i
    return None


def full_gold_rank(retrieved: Sequence[str], gold: Sequence[str]) -> Optional[int]:
    """1-based rank by which ALL gold ids are captured, or ``None`` if not all present
    in the order (strict all-gold answerability depth)."""
    if not gold:
        return None
    need = set(gold)
    seen = 0
    for i, did in enumerate(retrieved, start=1):
        if did in need:
            need.discard(did)
            seen += 1
            if not need:
                return i
    return None


def _mean(xs: Sequence[float]) -> Optional[float]:
    return round(sum(xs) / len(xs), 6) if xs else None


# --------------------------------------------------------------------------- #
# Reblend retrieval adapter — lets the PRICED accuracy arm test any (alpha, pool_n)
# with NO Rust rebuild (alpha is hardcoded 0.3 in the engine). Same retrieve(q, k)
# interface as eval.ce_rerank_probe.FathomDBRerankAdapter so it drops into the
# rerank_accuracy_run harness.
# --------------------------------------------------------------------------- #

from dataclasses import dataclass  # noqa: E402

from eval.r2_parity_eval import Hit  # noqa: E402


@dataclass
class ReblendRerankAdapter:
    """Wrap a base FathomDB adapter with a CE rerank at a CHOSEN ``alpha``/``pool_n``.

    ``retrieve(question, k)`` fetches the base top-``pool_n`` pool, recovers the pure
    ``ce_norm`` per passage via the ``score=0`` trick (one CE pass), re-blends
    ``alpha * ce_norm + (1 - alpha) * minmax(base_score)`` over the pool, sorts
    descending (stable; base order breaks ties — mirrors the engine), and returns the
    reordered :class:`~eval.r2_parity_eval.Hit` objects (the harness cuts to top-K).

    ``alpha == PROD_ALPHA`` reproduces production ordering; ``alpha == 1.0`` is pure CE.
    The score=0 extraction is exact only because the engine blend is the pinned
    ``0.3*ce + 0.7*rrf`` form; this adapter asserts that contract implicitly."""

    base: Any
    rerank_fn: Any  # fathomdb.rerank: (query, [{id,body,score}], depth) -> [{id,score}]
    alpha: float = 1.0
    pool_n: int = 10

    def retrieve(self, question: str, k: int) -> list[Hit]:
        n = max(k, self.pool_n)
        pool = self.base.retrieve(question, n)
        cand = list(pool[: self.pool_n])
        if not cand:
            return []
        passages = [{"id": i, "body": h.body, "score": 0.0} for i, h in enumerate(cand)]
        ce = {
            int(r["id"]): recover_ce_norm(float(r["score"]))
            for r in self.rerank_fn(question, passages, len(passages))
        }
        rrf = minmax_norm([float(h.score) for h in cand])
        scored = [
            (self.alpha * ce.get(i, 0.0) + (1.0 - self.alpha) * rrf[i], i, h)
            for i, h in enumerate(cand)
        ]
        scored.sort(key=lambda t: (-t[0], t[1]))
        ranked = [Hit(doc_id=h.doc_id, body=h.body, score=float(s)) for s, _i, h in scored]
        # Mirror the engine's ``ce_rerank``: hits beyond the reranked pool keep their
        # original base order + score. Only relevant when k > pool_n (the harness then
        # fills the rest of the top-K context from the base tail); a no-op when pool_n
        # covers the whole fetched pool. (codex §9 P2)
        tail = [Hit(doc_id=h.doc_id, body=h.body, score=float(h.score)) for h in pool[self.pool_n :]]
        return ranked + tail


def sweep_config(
    records: Sequence[Mapping[str, Any]],
    *,
    alpha: float,
    pool_n: int,
    classes: Sequence[str] = GAP_CLASSES,
    ks: Sequence[int] = KS,
) -> dict[str, Any]:
    """Aggregate metrics for one ``(alpha, pool_n)`` config over all ``records``.

    Per record we compute the reranked doc order then read recall@K + first/full gold
    rank off it. Returns per-class + pooled means. ``mrr`` = mean of
    ``1 / first_gold_rank`` (0 when no gold in pool)."""
    by_class: dict[str, dict[str, list[float]]] = {
        c: {"mrr": [], "full": [], **{f"r@{k}": [] for k in ks}} for c in classes
    }
    for r in records:
        cls = r.get("reporting_class")
        if cls not in by_class:
            continue
        gold = [str(g) for g in (r.get("gold") or [])]
        order = reranked_doc_order(r["pool"], alpha=alpha, pool_n=pool_n)
        acc = by_class[cls]
        for k in ks:
            acc[f"r@{k}"].append(strict_recall_at_k(order, gold, k))
        fg = first_gold_rank(order, gold)
        acc["mrr"].append(1.0 / fg if fg else 0.0)
        full = full_gold_rank(order, gold)
        # full-gold rank: pool_n+1 sentinel when gold is not fully captured (so the
        # mean is comparable across configs — a missed config is penalized, not dropped).
        acc["full"].append(float(full) if full is not None else float(pool_n + 1))

    per_class: dict[str, Any] = {}
    pooled_acc: dict[str, list[float]] = {"mrr": [], "full": [], **{f"r@{k}": [] for k in ks}}
    for c in classes:
        m = {key: _mean(vals) for key, vals in by_class[c].items()}
        m["n"] = len(by_class[c]["mrr"])
        per_class[c] = m
        for key, vals in by_class[c].items():
            pooled_acc[key].extend(vals)
    pooled = {key: _mean(vals) for key, vals in pooled_acc.items()}
    pooled["n"] = len(pooled_acc["mrr"])
    return {"alpha": alpha, "pool_n": pool_n, "per_class": per_class, "pooled": pooled}


def build_tuning_report(
    records: Sequence[Mapping[str, Any]],
    *,
    alphas: Sequence[float] = ALPHAS,
    pool_ns: Sequence[int] = POOL_NS,
    classes: Sequence[str] = GAP_CLASSES,
    ks: Sequence[int] = KS,
    max_pool: int = MAX_POOL,
) -> dict[str, Any]:
    """Full ``alpha x pool_n`` sweep + the best-config picks on the headline metrics
    (pooled Recall@5, Recall@10, MRR). Pure given ``records``."""
    # A swept pool_n cannot exceed the CE-scored superset each record holds
    # (``max_pool``); a larger grid row would only contain ``max_pool`` candidates yet
    # be labelled with the bigger pool — misleading metadata + best-config (codex §9 P2).
    # Clamp the sweep (and the prod/baseline anchor) to the pools the records support.
    eff_pool_ns = sorted({min(pn, max_pool) for pn in pool_ns})
    anchor_pool = min(50, max_pool)
    grid = [
        sweep_config(records, alpha=a, pool_n=pn, classes=classes, ks=ks)
        for a in alphas
        for pn in eff_pool_ns
    ]

    def _best(metric: str) -> dict[str, Any]:
        cand = [g for g in grid if g["pooled"].get(metric) is not None]
        best = max(cand, key=lambda g: g["pooled"][metric])
        return {"alpha": best["alpha"], "pool_n": best["pool_n"], metric: best["pooled"][metric]}

    def _at(alpha: float, pool_n: int) -> Optional[dict[str, Any]]:
        for g in grid:
            if g["alpha"] == alpha and g["pool_n"] == pool_n:
                return g["pooled"]
        return None

    return {
        "schema": "0.8.3-rerank-tune-v1",
        "prod_alpha": PROD_ALPHA,
        "max_pool": max_pool,
        "alphas": list(alphas),
        "pool_ns": list(eff_pool_ns),
        "ks": list(ks),
        "n_records": len(records),
        "n_per_class": {c: sum(1 for r in records if r.get("reporting_class") == c) for c in classes},
        "grid": grid,
        "production_config": {"alpha": PROD_ALPHA, "pool_n": anchor_pool, "pooled": _at(PROD_ALPHA, anchor_pool)},
        "baseline_config": {"alpha": 0.0, "pool_n": anchor_pool, "pooled": _at(0.0, anchor_pool)},
        "best": {m: _best(m) for m in ("r@5", "r@10", "mrr")},
    }


# --------------------------------------------------------------------------- #
# Phase 1 — the single CE pass (checkpointed; the only slow part).
# --------------------------------------------------------------------------- #


def collect_ce_records(
    *,
    queries: Sequence[Any],
    base_adapter: Any,
    rerank_fn: Any,
    max_pool: int,
    checkpoint_path: Path,
    checkpoint_every: int = 25,
    ce_depth: Optional[int] = None,
) -> list[dict[str, Any]]:
    """One CE pass per query → ``[{qid, reporting_class, gold, pool:[{doc_id,
    base_score, ce_norm}]}]``, resumable from ``checkpoint_path`` (keyed by qid).

    For each query: fetch the base top-``max_pool`` pool, call ``rerank_fn(question,
    [{id,body,score=0.0}], max_pool)`` ONCE (score=0 ⇒ recoverable pure ce_norm), and
    store the pool in base-rank order. A query whose pool is empty is still recorded
    (empty pool ⇒ all-miss in the sweep, never fabricated)."""
    done: dict[str, dict[str, Any]] = {}
    if checkpoint_path.exists():
        prior = json.loads(checkpoint_path.read_text(encoding="utf-8"))
        # codex §9 P2: the CE pass is keyed by qid alone, but each cached record's pool
        # holds exactly the prior run's top-`max_pool` candidates. Reusing it under a
        # DIFFERENT max_pool mislabels the sweep (a top-50 cache resumed at max_pool=100
        # reports pool_n=100 rows backed by only 50 candidates), skewing recall/full-rank
        # + best-config. Refuse loudly on mismatch; a fresh --checkpoint recomputes.
        prior_max_pool = prior.get("max_pool")
        if prior_max_pool is not None and int(prior_max_pool) != int(max_pool):
            raise ValueError(
                f"[rerank-tune] CE-pass checkpoint {checkpoint_path} was written under "
                f"max_pool={prior_max_pool} but this run uses max_pool={max_pool}; refusing "
                f"to resume (cached pools would be mislabeled). Use a fresh --checkpoint."
            )
        for r in prior.get("records") or []:
            done[str(r["qid"])] = r

    records: list[dict[str, Any]] = []
    t0 = time.time()
    for i, q in enumerate(queries, start=1):
        qid = str(q.query_id)
        if qid in done:
            records.append(done[qid])
            continue
        hits = base_adapter.retrieve(q.question, max_pool)
        passages = [{"id": j, "body": h.body, "score": 0.0} for j, h in enumerate(hits)]
        # ce_depth caps the CE pass to the top-`ce_depth` candidates. The offline sweep
        # only reblends the top-`pool_n` (<= max(POOL_NS)); CE on deeper ranks is never
        # read, so capping at ce_depth >= max(POOL_NS) is EXACT for the grid (not an
        # approximation) while cutting the slow CE cost. None = score the whole pool.
        ce_passages = passages[:ce_depth] if ce_depth else passages
        ce_by_id: dict[int, float] = {}
        if ce_passages:
            for r in rerank_fn(q.question, ce_passages, len(ce_passages)):
                ce_by_id[int(r["id"])] = recover_ce_norm(float(r["score"]))
        pool = [
            {"doc_id": h.doc_id, "base_score": float(h.score), "ce_norm": ce_by_id.get(j, 0.0)}
            for j, h in enumerate(hits)
        ]
        rec = {
            "qid": qid,
            "reporting_class": str(q.reporting_class),
            "gold": [str(g) for g in q.gold_doc_ids],
            "pool": pool,
        }
        records.append(rec)
        done[qid] = rec
        if i % checkpoint_every == 0 or i == len(queries):
            tmp = checkpoint_path.with_suffix(".tmp")
            tmp.write_text(
                json.dumps({"records": records, "max_pool": max_pool}, default=str),
                encoding="utf-8",
            )
            tmp.replace(checkpoint_path)
            print(f"[rerank-tune] CE pass {i}/{len(queries)} (elapsed {time.time()-t0:.0f}s)", flush=True)
    return records


# --------------------------------------------------------------------------- #
# CLI (live backend — not exercised by unit tests).
# --------------------------------------------------------------------------- #


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    ap = argparse.ArgumentParser(description="0.8.3 CE-rerank TUNING probe ($0 / CPU / LLM-free)")
    ap.add_argument("--gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    ap.add_argument("--output", required=True)
    ap.add_argument("--max-pool", type=int, default=MAX_POOL)
    ap.add_argument("--per-class", type=int, default=None)
    ap.add_argument("--fathomdb-db", default="/tmp/rerank-tune-fathomdb.sqlite")
    ap.add_argument("--checkpoint", default=None)
    args = ap.parse_args(argv)

    import fathomdb

    from eval.d0b_parity_run import _select_subset, build_documents_from_lme, build_live_adapters
    from eval.r2_parity_eval import load_repin_gold

    corpus_hash, _qv, queries = load_repin_gold(Path(args.gold))
    if args.per_class:
        queries = _select_subset(queries, per_class=args.per_class, classes=GAP_CLASSES)

    documents = build_documents_from_lme(queries)
    print(f"[rerank-tune] {len(queries)} queries, {len(documents)} sessions | corpus={corpus_hash[:12]}", flush=True)

    adapters, blockers = build_live_adapters(
        documents, want_mem0=False, want_graphiti=False, db_path=args.fathomdb_db,
    )
    base = adapters.get("fathomdb")
    if base is None:
        raise SystemExit(f"[rerank-tune][STOP] no fathomdb adapter (blockers={[b['id'] for b in blockers]})")

    out = Path(args.output)
    ckpt = Path(args.checkpoint) if args.checkpoint else out.with_suffix(".ce-pass.json")
    # Create the output/checkpoint parent BEFORE the CE pass — its first checkpoint write
    # would otherwise FileNotFoundError on a not-yet-existing --output dir (codex §9 P3).
    out.parent.mkdir(parents=True, exist_ok=True)
    ckpt.parent.mkdir(parents=True, exist_ok=True)
    records = collect_ce_records(
        queries=queries, base_adapter=base, rerank_fn=fathomdb.rerank,
        max_pool=args.max_pool, checkpoint_path=ckpt,
    )

    report = build_tuning_report(records, max_pool=args.max_pool)
    report["corpus_hash"] = corpus_hash
    report["blockers"] = blockers
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, default=str), encoding="utf-8")
    b = report["best"]
    print(
        f"[rerank-tune] wrote {out} | best r@10 alpha={b['r@10']['alpha']} pool_n={b['r@10']['pool_n']} "
        f"({b['r@10']['r@10']}) | best mrr alpha={b['mrr']['alpha']} ({b['mrr']['mrr']})",
        flush=True,
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
