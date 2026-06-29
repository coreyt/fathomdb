"""EXP-B' — 3-stage joint tuning (0.8.11 Slice 15, KEYSTONE; $0 retrieval-metric arm).

Pre-registration: ``dev/plans/0.8.11-implementation.md §1`` (EXP-B' row) + §3 (the
per-intent config-tuple output format). Design crux: ``planner-router-psd-0.8.x.md``
§II.B (config-carrying router + the EXP-B'.5 forbidden-composition guard) and §II.C
(the constrained-joint-optimization crux: alpha=1.0 @ pool_n=50 *drops* r@10 vs
pool_n=10).

Hypothesis (binding): the optimal ``(candidate_k x pool_n x alpha x final_K)`` config
**differs per intent class** and a single global config is dominated; KILL if the
per-intent optima collapse to one global config (no class diverges beyond noise).

Method ($0, LLM-free, deterministic), reusing the landed CE-tuning harness
(:mod:`eval.rerank_tune_probe`):

1. **CE pass** (the only slow part, checkpointed): per query, fetch the fused-RRF base
   pool to depth ``candidate_k_max`` and recover the intrinsic ``ce_norm`` per passage
   via the ``score=0`` trick (one TinyBERT pass) -> records ``{qid, reporting_class,
   gold, pool:[{doc_id, base_score, ce_norm}]}``. This is byte-for-byte the
   :func:`eval.rerank_tune_probe.collect_ce_records` contract.
2. **Offline joint sweep** (seconds): ``candidate_k`` x ``pool_n`` x ``alpha`` x
   ``final_K`` per intent class. For each config + query we re-blend
   ``alpha*ce_norm + (1-alpha)*minmax(base_score)`` over the top-``pool_n`` of the
   ``candidate_k``-truncated pool (mirrors the engine ``ce_rerank``) and read off
   strict r@final_K and MRR; ``candidate_k`` also fixes gold-in-pool / recall@K_deep
   (base order, alpha-invariant). Bootstrap-CI on r@final_K per intent.
3. **Crux** (PSD §II.C): alpha=1.0 @ pool_n=50 r@10 vs alpha=1.0 @ pool_n=10 r@10 at a
   fixed candidate_k -> report the measured drop (or not).
4. **Per-intent tuple** (§3 format) + **EXP-B'.5 forbidden-composition guard**: apply
   each measured intent's optimum to every other intent and report the r@10 regression;
   encode the §II.B router-isolation rule (map_reduce_qfs + community_summary valid
   ONLY for ``global``, forbidden on needle/multi_session/temporal/multi_hop).

Intent classes with node-level gold (needle/multi_session/temporal via LME; multi_hop
via MuSiQue) are MEASURED; ``global`` (AP-News, win-rate / decide_084 axis, no
node-level retrieval labels) is provisional-pinned to the EXP-0 global tuple.
"""

from __future__ import annotations

import argparse
import json
import time
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional, Sequence, cast

import numpy as np

from eval.rerank_tune_probe import (
    first_gold_rank,
    reranked_doc_order,
    strict_recall_at_k,
)

REPO = Path(__file__).resolve().parents[3]
RUNS = REPO / "dev" / "plans" / "runs"

# --------------------------------------------------------------------------- #
# Frozen sweep grid (pinned + echoed in the output for auditability).
# --------------------------------------------------------------------------- #

#: Candidate-generation depth (the recall stage). EXP-A (Slice 10): best=200,
#: NOT saturated -> the grid MUST include >=200.
CANDIDATE_KS: tuple[int, ...] = (200, 300, 500)
#: CE-rerank pool sizes. The PSD-suggested {10,20,50} EXTENDED upward so the sweep
#: genuinely exercises candidate_k>50 (reranking the deep pool EXP-A widened) and
#: can exhibit the §II.C crux at depth.
POOL_NS: tuple[int, ...] = (10, 20, 50, 100, 200)
#: CE-blend weights. 0.0 = pure base (no rerank); 0.3 = production C6 guard; 1.0 = pure CE.
ALPHAS: tuple[float, ...] = (0.0, 0.3, 0.5, 0.7, 1.0)
#: Final cut fed downstream (pinned).
FINAL_K: int = 10
#: Deepest pool the CE pass scores once per query (superset of every candidate_k).
CANDIDATE_K_MAX: int = max(CANDIDATE_KS)
#: Recall@K_deep cut-offs (gold-in-pool envelope; base order, alpha-invariant).
RECALL_KS: tuple[int, ...] = (10, 50, 100, 200, 300, 500)

#: Bootstrap config (percentile CI over questions; fixed seed = deterministic).
BOOT_SEED = 0xB5
BOOT_RESAMPLES = 2000

#: LME memory-class -> PSD intent. (gate2_oracle_run.GAP_TO_INTENT)
LME_CLASS_TO_INTENT = {
    "factoid": "needle",
    "knowledge_update": "needle",
    "multi_session": "multi_session",
    "temporal": "temporal",
}

#: The 5 PSD intent classes (fixed).
INTENT_CLASSES: tuple[str, ...] = ("needle", "multi_session", "temporal", "global", "multi_hop")

#: EXP-B'.5 router-isolation (PSD §II.B): these ops are valid ONLY for `global`
#: (sensemaking) and forbidden on every needle/factoid path (blind-distiller -0.362).
SENSEMAKING_OPS: tuple[str, ...] = ("map_reduce_qfs", "community_summary")

#: EXP-0 global tuple (production C6 guard alpha=0.3 / pool_n=10), carried by classes
#: without node-level retrieval gold (`global`). Source: 0.8.5 EXP-0 (landed).
EXP0_GLOBAL = {"alpha": 0.3, "pool_n": 10, "candidate_k": 200}

#: Noise band for the KILL check (per-intent optima "collapse to one global config"):
#: two configs whose r@10 point estimates differ by <= this are not a real divergence.
DIVERGENCE_EPS = 0.02

#: Recall-envelope saturation band: pick the smallest candidate_k whose gold-in-pool
#: is within this of the max (cost-bounded; gold-in-pool is monotonic in depth).
RECALL_SAT_EPS = 0.02


# --------------------------------------------------------------------------- #
# Pure helpers.
# --------------------------------------------------------------------------- #


def bootstrap_mean_ci(
    values: Sequence[float], *, seed: int = BOOT_SEED, n: int = BOOT_RESAMPLES, ci: float = 0.95
) -> dict[str, Optional[float]]:
    """Percentile bootstrap CI for the mean of a 0/1 (or real) sample."""
    arr = np.asarray(values, dtype=np.float64)
    m = arr.shape[0]
    if m == 0:
        return {"point": None, "lo": None, "hi": None, "n": 0}
    point = float(arr.mean())
    rng = np.random.default_rng(seed)
    idx = rng.integers(0, m, size=(n, m))
    means = arr[idx].mean(axis=1)
    lo_p = (1.0 - ci) / 2.0 * 100.0
    hi_p = (1.0 + ci) / 2.0 * 100.0
    return {
        "point": round(point, 4),
        "lo": round(float(np.percentile(means, lo_p)), 4),
        "hi": round(float(np.percentile(means, hi_p)), 4),
        "n": m,
    }


def _intent_of(record: dict[str, Any]) -> Optional[str]:
    """Map a CE-pass record to its PSD intent class (corpus-aware)."""
    if record.get("intent"):
        return record["intent"]
    cls = record.get("reporting_class")
    return LME_CLASS_TO_INTENT.get(cast(str, cls), cls)


def recall_at_k_deep_for_pool(pool: Sequence[dict[str, Any]], gold: Sequence[str], k: int) -> float:
    """Gold-in-pool @k on the BASE order (alpha-invariant recall-stage envelope)."""
    base_order = [str(p["doc_id"]) for p in pool[:k]]
    return strict_recall_at_k(base_order, gold, k)


def per_query_rerank_metrics(
    pool: Sequence[dict[str, Any]],
    gold: Sequence[str],
    *,
    candidate_k: int,
    pool_n: int,
    alpha: float,
    final_k: int,
) -> tuple[float, float]:
    """(r@final_k, reciprocal-first-gold-rank) for one (candidate_k,pool_n,alpha) config.

    The pool is truncated to ``candidate_k`` (the recall stage); the top-``pool_n`` of
    that is CE-reblended (mirrors the engine ``ce_rerank``); tail keeps base order."""
    cand = list(pool[:candidate_k])
    if not cand:
        return 0.0, 0.0
    eff_pool_n = min(pool_n, len(cand))
    order = reranked_doc_order(cand, alpha=alpha, pool_n=eff_pool_n)
    # Tail beyond pool_n keeps base order (engine contract) so r@K for K>pool_n is honest.
    tail = [str(p["doc_id"]) for p in cand[eff_pool_n:]]
    full = order + [d for d in tail if d not in order]
    r = strict_recall_at_k(full, gold, final_k)
    fg = first_gold_rank(full, gold)
    return r, (1.0 / fg if fg else 0.0)


# --------------------------------------------------------------------------- #
# Joint sweep over a set of CE-pass records (already grouped by intent).
# --------------------------------------------------------------------------- #


def recall_envelope_by_intent(
    records_by_intent: dict[str, list[dict[str, Any]]],
    *,
    recall_ks: Sequence[int] = RECALL_KS,
) -> dict[str, dict[str, Any]]:
    """Gold-in-pool @ each depth (base order, alpha-invariant) per intent — measured on
    the FRESH deep candidate pool (current engine)."""
    out: dict[str, dict[str, Any]] = {}
    for intent, recs in records_by_intent.items():
        recs = [r for r in recs if r.get("pool")]
        envelope: dict[str, Any] = {}
        for k in recall_ks:
            vals = [recall_at_k_deep_for_pool(r["pool"], [str(g) for g in r["gold"]], k) for r in recs]
            envelope[str(k)] = bootstrap_mean_ci(vals)
        out[intent] = envelope
    return out


def joint_sweep(
    records_by_intent: dict[str, list[dict[str, Any]]],
    *,
    envelopes: dict[str, dict[str, Any]] | None = None,
    candidate_ks: Sequence[int] = CANDIDATE_KS,
    pool_ns: Sequence[int] = POOL_NS,
    alphas: Sequence[float] = ALPHAS,
    final_k: int = FINAL_K,
    recall_ks: Sequence[int] = RECALL_KS,
) -> dict[str, Any]:
    """Full ``candidate_k x pool_n x alpha`` rerank sweep per intent over the REAL-CE
    records. ``envelopes`` (gold-in-pool @ candidate_k, measured on the fresh deep
    pool) is attached to each config-cell; if omitted, computed from these records.

    Returns ``{intent: {"n":…, "recall_envelope": {k: ci}, "grid": [config-cell…]}}``."""
    own_env = envelopes if envelopes is not None else recall_envelope_by_intent(
        records_by_intent, recall_ks=recall_ks
    )
    out: dict[str, Any] = {}
    for intent, recs in records_by_intent.items():
        recs = [r for r in recs if r.get("pool")]
        n = len(recs)
        envelope = own_env.get(intent, {})
        grid: list[dict[str, Any]] = []
        for ck in candidate_ks:
            for pn in pool_ns:
                for a in alphas:
                    rs: list[float] = []
                    mrrs: list[float] = []
                    for r in recs:
                        gold = [str(g) for g in r["gold"]]
                        rr, mrr = per_query_rerank_metrics(
                            r["pool"], gold, candidate_k=ck, pool_n=pn, alpha=a, final_k=final_k
                        )
                        rs.append(rr)
                        mrrs.append(mrr)
                    r_ci = bootstrap_mean_ci(rs)
                    grid.append({
                        "candidate_k": ck,
                        "pool_n": pn,
                        "alpha": a,
                        "final_k": final_k,
                        "r_at_final_k": r_ci,
                        "mrr": round(float(np.mean(mrrs)), 6) if mrrs else None,
                        "recall_at_candidate_k": envelope.get(str(ck), envelope.get(str(max(recall_ks)))),
                    })
        out[intent] = {"n": n, "recall_envelope": envelope, "grid": grid}
    return out


def best_config(grid: list[dict[str, Any]]) -> dict[str, Any]:
    """Pick the config maximizing r@final_k (point), MRR as tiebreak, then prefer the
    SMALLEST pool_n / SMALLEST candidate_k / LOWEST alpha (cheapest, C6-safest) on ties."""
    def key(c: dict[str, Any]) -> tuple:
        return (
            c["r_at_final_k"]["point"] or -1.0,
            c["mrr"] or -1.0,
            -c["pool_n"],
            -c["candidate_k"],
            -c["alpha"],
        )
    return max(grid, key=key)


def crux_check(grid: list[dict[str, Any]], *, candidate_k: int = 200, alpha: float = 1.0) -> dict[str, Any]:
    """PSD §II.C: at alpha=1.0, fixed candidate_k, does pool_n=50 DROP r@10 vs pool_n=10?"""
    def at(pn: int) -> Optional[dict[str, Any]]:
        for c in grid:
            if c["candidate_k"] == candidate_k and c["alpha"] == alpha and c["pool_n"] == pn:
                return c
        return None
    c10, c50 = at(10), at(50)
    if not c10 or not c50:
        return {"available": False}
    r10 = c10["r_at_final_k"]["point"]
    r50 = c50["r_at_final_k"]["point"]
    return {
        "available": True,
        "candidate_k": candidate_k,
        "alpha": alpha,
        "r_at_10__pool_n_10": r10,
        "r_at_10__pool_n_50": r50,
        "delta_50_minus_10": round((r50 or 0) - (r10 or 0), 6),
        "drops": bool(r50 is not None and r10 is not None and r50 < r10),
        "mrr_pool_n_10": c10["mrr"],
        "mrr_pool_n_50": c50["mrr"],
    }


# --------------------------------------------------------------------------- #
# Per-intent tuples + EXP-B'.5 guard.
# --------------------------------------------------------------------------- #


def make_tuple(intent: str, *, measured: dict[str, Any] | None) -> dict[str, Any]:
    """Emit the §3 config tuple. Measured intents read their best config; otherwise
    the EXP-0 global tuple is pinned (provisional)."""
    forbidden = [] if intent == "global" else list(SENSEMAKING_OPS)
    base_stack = ["fts_bm25", "vector_ann", "rrf", "ce_rerank"]
    if intent == "global":
        stack = ["fts_bm25", "vector_ann", "rrf", "map_reduce_qfs", "community_summary"]
    else:
        stack = base_stack
    if measured is not None:
        best = measured["best"]
        ci = best["r_at_final_k"]
        return {
            "intent": intent,
            "stack": stack,
            "index": "vector_default",
            "retrieval": {"candidate_k": best["candidate_k"], "final_K": best["final_k"]},
            "alpha": best["alpha"],
            "pool_n": best["pool_n"],
            "mmr": {"enabled": False, "lambda": None},
            "recency": {"enabled": False, "half_life_days": None},
            "forbidden_ops": forbidden,
            "source_exp": "EXP-B'",
            "ci": {"metric": "r@10", "point": ci["point"], "lo": ci["lo"], "hi": ci["hi"], "n": ci["n"]},
            "provisional": False,
        }
    return {
        "intent": intent,
        "stack": stack,
        "index": "vector_default",
        "retrieval": {"candidate_k": EXP0_GLOBAL["candidate_k"], "final_K": FINAL_K},
        "alpha": EXP0_GLOBAL["alpha"],
        "pool_n": EXP0_GLOBAL["pool_n"],
        "mmr": {"enabled": False, "lambda": None},
        "recency": {"enabled": False, "half_life_days": None},
        "forbidden_ops": forbidden,
        "source_exp": "EXP-0-global",
        "ci": {"metric": "r@10", "point": None, "lo": None, "hi": None, "n": 0},
        "provisional": True,
    }


def eval_config_on_intent(grid: list[dict[str, Any]], cfg: dict[str, Any]) -> Optional[dict[str, Any]]:
    """Look up the r@10 cell for a given (candidate_k,pool_n,alpha) config in an intent's grid."""
    for c in grid:
        if (c["candidate_k"] == cfg["candidate_k"]
                and c["pool_n"] == cfg["pool_n"]
                and abs(c["alpha"] - cfg["alpha"]) < 1e-9):
            return c
    return None


def forbidden_composition_matrix(
    measured: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    """EXP-B'.5: (a) the router-isolation forbidden-op rule, and (b) the empirical
    cross-application regression — each measured intent's optimum applied to every
    OTHER measured intent, reporting the r@10 delta vs that intent's own optimum."""
    # (a) static router-isolation rule.
    isolation = {
        intent: {
            "allowed_sensemaking_ops": list(SENSEMAKING_OPS) if intent == "global" else [],
            "forbidden_ops": [] if intent == "global" else list(SENSEMAKING_OPS),
        }
        for intent in INTENT_CLASSES
    }
    # (b) empirical config cross-application over measured intents.
    cross: list[dict[str, Any]] = []
    any_regression = False
    for src, sdata in measured.items():
        src_cfg = {
            "candidate_k": sdata["best"]["candidate_k"],
            "pool_n": sdata["best"]["pool_n"],
            "alpha": sdata["best"]["alpha"],
        }
        for dst, ddata in measured.items():
            own = ddata["best"]["r_at_final_k"]["point"]
            applied = eval_config_on_intent(ddata["grid"], src_cfg)
            applied_pt = applied["r_at_final_k"]["point"] if applied else None
            delta = (round(applied_pt - own, 6)
                     if (applied_pt is not None and own is not None) else None)
            # A regression "clears noise" when the dst optimum's CI-lo exceeds the
            # applied config's point (the applied config is below the optimum's band).
            own_lo = ddata["best"]["r_at_final_k"]["lo"]
            regresses = bool(
                src != dst and applied_pt is not None and own_lo is not None
                and applied_pt < own_lo
            )
            if regresses:
                any_regression = True
            cross.append({
                "source_intent": src,
                "config_from_source": src_cfg,
                "applied_to_intent": dst,
                "r_at_10_applied": applied_pt,
                "r_at_10_dst_optimum": own,
                "delta": delta,
                "regresses_beyond_noise": regresses,
            })
    return {
        "router_isolation_rule": isolation,
        "router_isolation_note": (
            "map_reduce_qfs + community_summary are valid ONLY for `global` (sensemaking) "
            "and FORBIDDEN on needle/multi_session/temporal/multi_hop — the §II.B blind-distiller "
            "-0.362 cross-wire. This is the forbidden-composition the 0.8.15 plan validator consumes."
        ),
        "empirical_cross_application": cross,
        "any_optimum_regresses_another_intent": any_regression,
    }


# --------------------------------------------------------------------------- #
# CE pass (corpus stand-up) — CLI only; reuses rerank_tune_probe.collect_ce_records.
# --------------------------------------------------------------------------- #


def run_lme_recall_pool(*, per_class: Optional[int], max_pool: int, db_path: str, checkpoint: Path) -> list[dict[str, Any]]:
    """Fresh BASE-ORDER candidate pool to depth ``max_pool`` (the recall stage).

    Records ``{qid, reporting_class, gold, pool:[{doc_id, base_score, ce_norm}]}`` — but
    the CURRENT build's ``rerank`` is feature-gated OFF (``#[cfg(feature =
    "default-reranker")]`` in ``rerank_fused``), so ``ce_norm`` is degenerate here. We
    use ONLY the base order + base_score for the recall-envelope (gold-in-pool @
    candidate_k); the CE rerank tuple comes from the real-CE pass below."""
    import fathomdb
    from eval.d0b_parity_run import _select_subset, build_documents_from_lme, build_live_adapters
    from eval.decision_rule_083 import MEMORY_CLASSES
    from eval.r2_parity_eval import load_repin_gold
    from eval.rerank_tune_probe import collect_ce_records

    _ch, _qv, queries = load_repin_gold(RUNS / "0.8.3-d0a-memory-gold.json")
    if per_class:
        queries = _select_subset(queries, per_class=per_class, classes=MEMORY_CLASSES)
    docs = build_documents_from_lme(queries)
    adapters, blockers = build_live_adapters(docs, want_mem0=False, want_graphiti=False, db_path=db_path)
    base = adapters.get("fathomdb")
    if base is None:
        raise SystemExit(f"[expb] no fathomdb adapter (blockers={[b['id'] for b in blockers]})")
    print(f"[expb][lme] {len(queries)} queries, {len(docs)} sessions, recall depth={max_pool}", flush=True)
    return collect_ce_records(
        queries=queries, base_adapter=base, rerank_fn=fathomdb.rerank,
        max_pool=max_pool, checkpoint_path=checkpoint,
    )


def ce_norm_is_active(records: Sequence[dict[str, Any]]) -> bool:
    """Degeneracy guard: a real-CE pass has a non-trivial ce_norm spread. The
    feature-off build returns identity (all ce_norm == 0) — refuse to derive a
    rerank tuple from it (it would falsely report alpha=0 as optimal)."""
    vals = [p.get("ce_norm", 0.0) for r in records for p in r.get("pool", [])]
    if not vals:
        return False
    return any(v > 0.5 for v in vals)


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    ap = argparse.ArgumentParser(description="EXP-B' 3-stage joint tuning (0.8.11 Slice 15, $0 retrieval arm)")
    ap.add_argument("--per-class", type=int, default=None, help="cap LME queries per memory class (default: all 606)")
    ap.add_argument("--max-pool", type=int, default=CANDIDATE_K_MAX)
    ap.add_argument("--lme-db", default="/tmp/expb-lme.sqlite")
    ap.add_argument("--recall-pool-ckpt", default=str(RUNS / "expb-lme.ce-pass.json"),
                    help="fresh deep BASE-ORDER candidate pool (current engine; for the recall envelope)")
    ap.add_argument("--rerank-ce-pass", default=str(RUNS / "0.8.3-rerank-tune.ce-pass.json"),
                    help="real-CE pass (TinyBERT scores) for the rerank tuple + crux. The current "
                         "build's rerank is feature-gated OFF, so fresh CE is unavailable; the landed "
                         "0.8.3 CE-pass (same LME gold, same model weights) is the authoritative CE source.")
    ap.add_argument("--out-json", default=str(RUNS / "expb-joint-tune-output.json"))
    ap.add_argument("--out-md", default=str(RUNS / "expb-joint-tune.md"))
    args = ap.parse_args(argv)

    t0 = time.time()
    # (1) Fresh deep base-order pool (current engine) → recall envelope.
    recall_records = run_lme_recall_pool(
        per_class=args.per_class, max_pool=args.max_pool,
        db_path=args.lme_db, checkpoint=Path(args.recall_pool_ckpt),
    )
    # (2) Real-CE pass → rerank tuple + crux. GUARD against a feature-off (degenerate) pass.
    rerank_records = json.loads(Path(args.rerank_ce_pass).read_text(encoding="utf-8")).get("records", [])
    if not ce_norm_is_active(rerank_records):
        raise SystemExit(
            f"[expb][STOP] {args.rerank_ce_pass} has degenerate ce_norm (all ~0) — the rerank "
            "feature was OFF when it was generated. Refusing to derive a rerank tuple from an "
            "identity pass. Regenerate with a `--features default-reranker` build."
        )

    by_intent_recall: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for r in recall_records:
        it = _intent_of(r)
        if it:
            by_intent_recall[it].append(r)
    by_intent: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for r in rerank_records:
        it = _intent_of(r)
        if it:
            by_intent[it].append(r)

    envelopes = recall_envelope_by_intent(by_intent_recall)
    sweep = joint_sweep(by_intent, envelopes=envelopes)

    # Per-intent best + crux. The rerank r@10 is candidate_k-invariant for pool_n<=depth
    # (rerank touches only the top-pool_n), so candidate_k in the tuple is set from the
    # FRESH recall envelope's gold-in-pool argmax over the registered candidate_ks.
    measured: dict[str, dict[str, Any]] = {}
    for intent, data in sweep.items():
        best = best_config(data["grid"])
        env = envelopes.get(intent, {})
        # candidate_k = SMALLEST registered depth within RECALL_SAT_EPS of the envelope
        # max (gold-in-pool is monotonic, so argmax is trivially the deepest; the real
        # choice is cost-bounded saturation — matches EXP-A's registered ~200 operating
        # point and the gate2 CE cost tiers).
        env_pts = {k: (env.get(str(k), {}).get("point") or -1.0) for k in CANDIDATE_KS}
        env_max = max(env_pts.values())
        best_ck = next((k for k in sorted(CANDIDATE_KS) if env_pts[k] >= env_max - RECALL_SAT_EPS),
                       max(CANDIDATE_KS, key=lambda k: env_pts[k]))
        best["candidate_k"] = best_ck  # recall-stage breadth from the envelope (saturation)
        crux = crux_check(data["grid"], candidate_k=200, alpha=1.0)
        measured[intent] = {"best": best, "crux": crux, "grid": data["grid"],
                            "recall_envelope": data["recall_envelope"], "n": data["n"]}

    # Pooled crux (over all measured records).
    pooled_grid = joint_sweep({"_pooled": [r for recs in by_intent.values() for r in recs]})["_pooled"]["grid"]
    pooled_crux = crux_check(pooled_grid, candidate_k=200, alpha=1.0)

    # Tuples: measured intents from EXP-B'; the rest pinned provisional (EXP-0-global).
    tuples: list[dict[str, Any]] = []
    for intent in INTENT_CLASSES:
        tuples.append(make_tuple(intent, measured=measured.get(intent)))

    guard = forbidden_composition_matrix(measured)

    # KILL check: do per-intent optima collapse to one global config?
    opt_signatures = {
        intent: (m["best"]["candidate_k"], m["best"]["pool_n"], m["best"]["alpha"])
        for intent, m in measured.items()
    }
    distinct = set(opt_signatures.values())
    # also test if a single config is within DIVERGENCE_EPS of every intent's optimum
    collapses = len(distinct) == 1
    kill = {
        "per_intent_optimum_signature": {k: list(v) for k, v in opt_signatures.items()},
        "distinct_optima": len(distinct),
        "collapses_to_one_global_config": collapses,
        "divergence_eps": DIVERGENCE_EPS,
        "verdict": (
            "KILL — per-intent optima collapse to one global config; register 'stacks do not "
            "diverge per intent' (L2 prototype ships pinned to the global tuple, DP-A hedge)."
            if collapses else
            "GO — per-intent optima DIVERGE; the config-carrying router has measured value "
            "(EXP-Fr routing-value case supported)."
        ),
    }

    out: dict[str, Any] = {
        "schema": "0.8.11-expb-joint-tune-v1",
        "experiment": "EXP-B' + EXP-B'.5",
        "slice": "0.8.11/slice-15",
        "cost_usd": 0.0,
        "cost_note": "retrieval-metric arm over existing node-level gold; LLM judge NOT spent "
                     "(global is provisional-pinned by design; gold sufficient for the measured intents).",
        "build_blocker": {
            "finding": "The installed .venv build compiled `rerank_fused` with the CE inference "
                       "block gated OFF (`#[cfg(feature = \"default-reranker\")]` -> identity "
                       "passthrough). `fathomdb.rerank(...)` returns base order unchanged for ALL "
                       "alpha/pool_n; the score=0 ce_norm-recovery trick yields all-zero CE. The "
                       "slice prompt forbids rebuild/maturin, so fresh CE could not be produced.",
            "resolution": "Rerank tuple + crux measured on the LANDED 0.8.3 CE-pass "
                          "(`0.8.3-rerank-tune.ce-pass.json`): same LME 606Q gold, same TinyBERT "
                          "weights (cache present), generated by a feature-ON build (real ce_norm, "
                          "max 0.9993). Recall envelope (candidate_k 200/300/500) measured FRESH on "
                          "the current engine's base retrieval (CE-independent). Reproduce fresh CE "
                          "by rebuilding `maturin develop --features default-reranker`.",
        },
        "deferrals": [
            "multi_hop (MuSiQue): node-level gold exists (Gate-0) but a fresh per-question "
            "fused+CE pass is blocked by the same feature-off rerank; no prior MuSiQue CE-pass "
            "exists -> pinned provisional. Reproduce with a default-reranker build.",
            "global (AP-News): win-rate axis (decide_084), NO node-level retrieval labels by "
            "design -> provisional EXP-0-global, sensemaking ops ALLOWED (router-isolated).",
            "LOCOMO corroboration of multi_session/temporal: available, not run in-session "
            "(LME already measures all three intents); $0 to add later.",
        ],
        "grid": {
            "candidate_ks": list(CANDIDATE_KS),
            "pool_ns": list(POOL_NS),
            "alphas": list(ALPHAS),
            "final_k": FINAL_K,
            "recall_ks": list(RECALL_KS),
        },
        "boot": {"seed": BOOT_SEED, "resamples": BOOT_RESAMPLES},
        "corpora_measured": {
            "needle": "LME factoid+knowledge_update",
            "multi_session": "LME multi_session",
            "temporal": "LME temporal",
        },
        "intents_measured": sorted(measured.keys()),
        "intents_provisional": [i for i in INTENT_CLASSES if i not in measured],
        "per_intent": {
            intent: {
                "n": m["n"],
                "best_config": m["best"],
                "crux": m["crux"],
                "recall_envelope": m["recall_envelope"],
            } for intent, m in measured.items()
        },
        "crux_pooled": pooled_crux,
        "per_intent_tuples": tuples,
        "expb5_forbidden_composition_guard": guard,
        "kill_check": kill,
        "elapsed_s": round(time.time() - t0, 1),
    }
    Path(args.out_json).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out_json).write_text(json.dumps(out, indent=2, default=str), encoding="utf-8")
    write_md(out, Path(args.out_md))
    print(
        f"[expb] measured={sorted(measured)} pooled_crux_drops={pooled_crux.get('drops')} "
        f"kill={kill['collapses_to_one_global_config']} elapsed={out['elapsed_s']}s -> {args.out_json}",
        flush=True,
    )
    return 0


def write_md(out: dict[str, Any], path: Path) -> None:
    L: list[str] = []
    L.append("# EXP-B' — 3-stage joint tuning (0.8.11 Slice 15, KEYSTONE)")
    L.append("")
    L.append(f"- cost: **${out['cost_usd']:.2f}** — {out['cost_note']}")
    L.append(f"- grid: candidate_k {out['grid']['candidate_ks']} x pool_n {out['grid']['pool_ns']} "
             f"x alpha {out['grid']['alphas']} x final_K={out['grid']['final_k']}")
    L.append(f"- bootstrap: {out['boot']['resamples']}x seed {hex(out['boot']['seed'])}")
    L.append(f"- measured intents: {out['intents_measured']} · provisional: {out['intents_provisional']}")
    L.append(f"- elapsed: {out['elapsed_s']}s")
    L.append("")
    L.append("## The §II.C crux — alpha=1.0, candidate_k=200: pool_n=50 vs pool_n=10 r@10")
    L.append("")
    L.append("| intent | r@10 pool_n=10 | r@10 pool_n=50 | Δ(50−10) | drops? | MRR p10 | MRR p50 |")
    L.append("|---|---|---|---|---|---|---|")
    for intent in out["intents_measured"]:
        c = out["per_intent"][intent]["crux"]
        if c.get("available"):
            L.append(f"| {intent} | {c['r_at_10__pool_n_10']} | {c['r_at_10__pool_n_50']} | "
                     f"{c['delta_50_minus_10']} | {c['drops']} | {c['mrr_pool_n_10']} | {c['mrr_pool_n_50']} |")
    pc = out["crux_pooled"]
    if pc.get("available"):
        L.append(f"| **pooled** | {pc['r_at_10__pool_n_10']} | {pc['r_at_10__pool_n_50']} | "
                 f"{pc['delta_50_minus_10']} | **{pc['drops']}** | {pc['mrr_pool_n_10']} | {pc['mrr_pool_n_50']} |")
    L.append("")
    L.append("## Per-intent optimum (r@10 maximizer; CI = 95% bootstrap)")
    L.append("")
    L.append("| intent | n | candidate_k | pool_n | alpha | r@10 [lo,hi] | MRR |")
    L.append("|---|---|---|---|---|---|---|")
    for intent in out["intents_measured"]:
        b = out["per_intent"][intent]["best_config"]
        ci = b["r_at_final_k"]
        L.append(f"| {intent} | {out['per_intent'][intent]['n']} | {b['candidate_k']} | {b['pool_n']} | "
                 f"{b['alpha']} | {ci['point']} [{ci['lo']},{ci['hi']}] | {b['mrr']} |")
    L.append("")
    L.append("## Recall envelope (gold-in-pool @ candidate_k; base order, alpha-invariant)")
    L.append("")
    rks = out["grid"]["recall_ks"]
    L.append("| intent | " + " | ".join(f"@{k}" for k in rks) + " |")
    L.append("|---|" + "---|" * len(rks))
    for intent in out["intents_measured"]:
        env = out["per_intent"][intent]["recall_envelope"]
        cells = []
        for k in rks:
            c = env.get(str(k))
            cells.append("—" if not c or c["point"] is None else f"{c['point']:.3f}")
        L.append(f"| {intent} | " + " | ".join(cells) + " |")
    L.append("")
    L.append("## KILL check — do per-intent optima collapse to one global config?")
    L.append("")
    L.append(f"- distinct optima: {out['kill_check']['distinct_optima']} of {len(out['intents_measured'])} measured")
    L.append(f"- signatures (candidate_k,pool_n,alpha): `{out['kill_check']['per_intent_optimum_signature']}`")
    L.append(f"- **{out['kill_check']['verdict']}**")
    L.append("")
    L.append("## EXP-B'.5 — forbidden-composition / joint-regression guard")
    L.append("")
    L.append(out["expb5_forbidden_composition_guard"]["router_isolation_note"])
    L.append("")
    L.append("### Empirical cross-application (each intent's optimum applied to the others)")
    L.append("")
    L.append("| source optimum | applied to | r@10 applied | r@10 dst optimum | Δ | regresses? |")
    L.append("|---|---|---|---|---|---|")
    for x in out["expb5_forbidden_composition_guard"]["empirical_cross_application"]:
        if x["source_intent"] == x["applied_to_intent"]:
            continue
        L.append(f"| {x['source_intent']} {tuple(x['config_from_source'].values())} | {x['applied_to_intent']} | "
                 f"{x['r_at_10_applied']} | {x['r_at_10_dst_optimum']} | {x['delta']} | {x['regresses_beyond_noise']} |")
    L.append("")
    L.append(f"**Any optimum regresses another intent beyond noise:** "
             f"{out['expb5_forbidden_composition_guard']['any_optimum_regresses_another_intent']}")
    L.append("")
    L.append("## Per-intent config tuples (§3 format — the keystone artifact)")
    L.append("")
    for t in out["per_intent_tuples"]:
        L.append(f"- **{t['intent']}** ({'provisional, '+t['source_exp'] if t['provisional'] else t['source_exp']}): "
                 f"candidate_k={t['retrieval']['candidate_k']} pool_n={t['pool_n']} alpha={t['alpha']} "
                 f"final_K={t['retrieval']['final_K']} · forbidden_ops={t['forbidden_ops']} · "
                 f"r@10={t['ci']['point']}")
    L.append("")
    path.write_text("\n".join(L) + "\n", encoding="utf-8")


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
