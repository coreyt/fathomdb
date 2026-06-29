"""EXP-A — recall generation / candidate-breadth sweep (0.8.11 Slice 10, $0).

Pre-registration: ``dev/plans/0.8.11-implementation.md §1`` (EXP-A row).

Hypothesis (binding): wider candidate generation lifts **F2 multi_session
recall@K_deep / gold-in-pool** above the shipped fused-RRF candidate set (the pool
EXP-B' then reranks within). KILL: if no breadth setting lifts gold-in-pool with a
CI clearing noise -> register the recall envelope as-is.

Method ($0, LLM-free, deterministic):
* Reuse the P0-A LME loader (:func:`eval.p0a_base_retrieval.load_lme_smoke`) and
  the P0-A retrieval variants (:func:`eval.p0a_base_retrieval.build_variants`):
  ``naive_bm25`` (pure-Python BM25), ``fathomdb_fts_only`` (the engine FTS5
  lexical candidate generator), and optionally ``fathomdb_fused`` (FTS5 + dense
  ANN RRF — the SHIPPED candidate set; needs the CPU embedder, gated by --with-fused).
* Sweep candidate breadth ``K`` over a grid and score **gold-in-pool** at each K
  (the multi_session full-gold-set rule from P0-A: hit iff ALL gold sessions are in
  top-K). recall@10 == the shipped final_K view; recall@K_deep == the candidate pool.
* Per (class, arm, K): mean gold-in-pool + a percentile bootstrap CI over questions.
* **Per-query arm logging** (the deferred Slice-5 oracle enabler): for every
  (question, arm) we persist the rank at which each gold session is surfaced, so the
  per-query arm-selection oracle becomes computable offline.

This module writes ``expa-recall-output.json`` + a companion markdown report.
"""

from __future__ import annotations

import argparse
import json
import time
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional, Sequence

import numpy as np

from eval.p0a_base_retrieval import (
    SMOKE_CLASSES,
    SmokeSet,
    build_variants,
    hit_at_k,
    load_lme_smoke,
)

#: Candidate-breadth grid (gold-in-pool measured at each depth). 10 == shipped
#: final_K; the larger depths == widening candidate generation before rerank.
DEFAULT_K_GRID: tuple[int, ...] = (10, 20, 50, 100, 200)

#: Bootstrap config (percentile CI over questions; fixed seed = deterministic).
BOOT_SEED = 0xEA
BOOT_RESAMPLES = 2000


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


def gold_ranks(retrieved_ids: Sequence[str], gold: Sequence[str]) -> dict[str, Optional[int]]:
    """0-based rank of each gold id in ``retrieved_ids`` (None if absent)."""
    pos = {d: i for i, d in enumerate(retrieved_ids)}
    return {g: (pos[g] if g in pos else None) for g in gold}


def run_expa(
    smoke: SmokeSet,
    systems: dict[str, Any],
    *,
    k_grid: tuple[int, ...] = DEFAULT_K_GRID,
) -> dict[str, Any]:
    """Candidate-breadth gold-in-pool sweep + per-query arm logging.

    Retrieves once at ``max(k_grid)`` per (arm, question), scores gold-in-pool at
    every K (multi_session full-gold rule), bootstraps a per-(class,arm,K) CI, and
    records the per-query gold ranks per arm.
    """
    kmax = max(k_grid)
    classes = sorted({q.reporting_class for q in smoke.questions})

    # hits[arm][k][class] -> list of 0/1 gold-in-pool per question (abstention-excluded)
    hits: dict[str, dict[int, dict[str, list[float]]]] = {
        arm: {k: defaultdict(list) for k in k_grid} for arm in systems
    }
    per_query_log: list[dict[str, Any]] = []

    for q in smoke.questions:
        if not q.gold_sessions:  # abstention — excluded from recall
            continue
        qlog: dict[str, Any] = {
            "qid": q.qid,
            "reporting_class": q.reporting_class,
            "n_gold": len(q.gold_sessions),
            "arms": {},
        }
        for arm, adapter in systems.items():
            ranked = [h.doc_id for h in adapter.retrieve(q.question, kmax)]
            for k in k_grid:
                h = hit_at_k(q.gold_sessions, ranked, k, q.reporting_class)
                if h is not None:
                    hits[arm][k][q.reporting_class].append(h)
            ranks = gold_ranks(ranked, q.gold_sessions)
            finite = [r for r in ranks.values() if r is not None]
            qlog["arms"][arm] = {
                "gold_ranks": ranks,  # the per-query arm-selection oracle input
                "min_gold_rank": (min(finite) if finite else None),
                "all_gold_found": len(finite) == len(q.gold_sessions),
            }
        per_query_log.append(qlog)

    # Aggregate per (arm, class, K) with bootstrap CI.
    sweep: dict[str, Any] = {}
    for arm in systems:
        sweep[arm] = {}
        for cls in classes:
            sweep[arm][cls] = {}
            for k in k_grid:
                vals = hits[arm][k][cls]
                sweep[arm][cls][str(k)] = bootstrap_mean_ci(vals)

    return {"k_grid": list(k_grid), "classes": classes, "sweep": sweep, "per_query_log": per_query_log}


def kill_check(sweep: dict[str, Any], k_grid: Sequence[int], *, focus_class: str = "multi_session") -> dict[str, Any]:
    """EXP-A KILL/verdict: per arm, does deep-K lift gold-in-pool over K=10 with a
    CI clearing noise? Report, per arm, the lift (recall@maxK - recall@10) with the
    K that maximizes gold-in-pool (the candidate_k that feeds EXP-B')."""
    k_lo = str(min(k_grid))
    out: dict[str, Any] = {"focus_class": focus_class, "k_floor": int(k_lo), "per_arm": {}}
    any_material_lift = False
    for arm, by_cls in sweep.items():
        cls = by_cls.get(focus_class)
        if not cls:
            continue
        base = cls[k_lo]
        # best K by point gold-in-pool
        best_k = max(k_grid, key=lambda k: (cls[str(k)]["point"] or -1.0))
        best = cls[str(best_k)]
        lift = None
        if best["point"] is not None and base["point"] is not None:
            lift = round(best["point"] - base["point"], 4)
        # "CI clears noise": best K's CI-lo strictly above the K=10 point estimate.
        clears = bool(
            best["lo"] is not None and base["point"] is not None and best["lo"] > base["point"]
        )
        if clears and (lift or 0) > 0:
            any_material_lift = True
        out["per_arm"][arm] = {
            "recall_at_floor": base,
            "best_k": best_k,
            "recall_at_best_k": best,
            "lift_over_floor": lift,
            "ci_clears_floor_noise": clears,
        }
    out["any_material_lift"] = any_material_lift
    out["verdict"] = (
        "GO — wider candidate generation lifts gold-in-pool with CI clearing the K=10 floor; "
        "EXP-B' should rerank a widened pool."
        if any_material_lift
        else "KILL — no breadth setting lifts gold-in-pool with a CI clearing noise; register "
        "the recall envelope as-is (EXP-B' tunes within the shipped pool)."
    )
    return out


def write_md(result: dict[str, Any], path: Path) -> None:
    L: list[str] = []
    L.append("# EXP-A — recall generation / candidate-breadth sweep (0.8.11 Slice 10)")
    L.append("")
    L.append(f"- mode: **{result['mode']}** · $0 / LLM-free / deterministic")
    L.append(f"- dataset: `{result['dataset']}` split `{result['split']}` seed `{result['seed']}`")
    L.append(f"- questions: {result['n_questions']} · arms: {', '.join(result['arms'])}")
    L.append(f"- union corpus (LME sessions): {result['union_docs']}")
    L.append(f"- candidate-breadth grid (gold-in-pool @K): {result['expa']['k_grid']}")
    L.append(f"- elapsed: {result['elapsed_s']}s")
    if result.get("blockers"):
        L.append(f"- blockers: {[b.get('id') for b in result['blockers']]}")
    L.append("")
    L.append("## Gold-in-pool vs candidate breadth (point [CI lo, hi], n)")
    L.append("")
    kg = result["expa"]["k_grid"]
    for arm in result["arms"]:
        L.append(f"### arm: `{arm}`")
        L.append("")
        L.append("| class | " + " | ".join(f"@{k}" for k in kg) + " |")
        L.append("|---|" + "---|" * len(kg))
        for cls in result["expa"]["classes"]:
            cells = []
            for k in kg:
                c = result["expa"]["sweep"][arm][cls][str(k)]
                if c["point"] is None:
                    cells.append("—")
                else:
                    cells.append(f"{c['point']:.3f} [{c['lo']:.3f},{c['hi']:.3f}] (n={c['n']})")
            L.append(f"| {cls} | " + " | ".join(cells) + " |")
        L.append("")
    L.append("## KILL check (focus: multi_session)")
    L.append("")
    kc = result["kill_check"]
    L.append(f"- floor K = {kc['k_floor']} (shipped final_K view)")
    L.append("")
    L.append("| arm | recall@floor | best K | recall@bestK | lift | CI clears floor? |")
    L.append("|---|---|---|---|---|---|")
    for arm, m in kc["per_arm"].items():
        rf, rb = m["recall_at_floor"], m["recall_at_best_k"]
        rfs = "—" if rf["point"] is None else f"{rf['point']:.3f}"
        rbs = "—" if rb["point"] is None else f"{rb['point']:.3f} [{rb['lo']:.3f},{rb['hi']:.3f}]"
        L.append(
            f"| {arm} | {rfs} | {m['best_k']} | {rbs} | "
            f"{m['lift_over_floor']} | {m['ci_clears_floor_noise']} |"
        )
    L.append("")
    L.append(f"**Verdict.** {kc['verdict']}")
    L.append("")
    L.append(
        f"**candidate_k that maximizes gold-in-pool (feeds EXP-B'):** "
        f"{result['best_candidate_k_for_expb']}"
    )
    L.append("")
    L.append("## Per-query arm logging (deferred Slice-5 oracle enabler)")
    L.append("")
    L.append(
        f"Per-query gold ranks for every arm are persisted in the JSON "
        f"(`per_query_log`, {len(result['expa']['per_query_log'])} questions). Each entry carries, "
        f"per arm, the 0-based rank of each gold session (None if absent), the min gold rank, and "
        f"whether all gold was found within K={max(kg)} — making the per-query arm-selection oracle "
        f"from Slice 5 Gate-2 computable offline (it was previously deferred for lack of per-query "
        f"per-arm cells)."
    )
    path.write_text("\n".join(L) + "\n", encoding="utf-8")


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="EXP-A candidate-breadth recall sweep (0.8.11 Slice 10, $0)")
    ap.add_argument("--per-class", type=int, default=40)
    ap.add_argument("--seed", type=int, default=20260614)
    ap.add_argument("--with-fused", action="store_true", help="also build the dense+FTS fused arm (CPU embedder; slow)")
    ap.add_argument("--db-dir", default="/tmp/expa-recall")
    ap.add_argument("--k-grid", default=",".join(str(k) for k in DEFAULT_K_GRID))
    ap.add_argument("--out-json", default="dev/plans/runs/expa-recall-output.json")
    ap.add_argument("--out-md", default="dev/plans/runs/expa-recall.md")
    args = ap.parse_args(argv)

    t0 = time.time()
    k_grid = tuple(int(x) for x in args.k_grid.split(",") if x.strip())
    smoke = load_lme_smoke(per_class=args.per_class, classes=SMOKE_CLASSES, seed=args.seed)

    db_dir = Path(args.db_dir)
    db_dir.mkdir(parents=True, exist_ok=True)
    systems, blockers = build_variants(smoke.documents, db_dir, include_fused=args.with_fused)

    expa = run_expa(smoke, systems, k_grid=k_grid)
    kc = kill_check(expa["sweep"], k_grid)

    # best candidate_k for EXP-B': the smallest K within 1pt of the max gold-in-pool
    # on multi_session for the best lexical arm (saturation point).
    def saturating_k(arm: str) -> Optional[int]:
        cls = expa["sweep"].get(arm, {}).get("multi_session")
        if not cls:
            return None
        pts = {k: (cls[str(k)]["point"] or 0.0) for k in k_grid}
        mx = max(pts.values())
        for k in k_grid:
            if pts[k] >= mx - 0.01:
                return k
        return max(k_grid, key=lambda k: pts[k])

    pref_arm = "fathomdb_fused" if "fathomdb_fused" in systems else (
        "fathomdb_fts_only" if "fathomdb_fts_only" in systems else next(iter(systems))
    )
    best_k = saturating_k(pref_arm)

    from collections import Counter

    result: dict[str, Any] = {
        "experiment": "EXP-A",
        "slice": "0.8.11/slice-10",
        "mode": "full" if args.per_class >= 40 else "smoke",
        "cost": "$0 (LLM-free, deterministic)",
        "dataset": "xiaowu0162/longmemeval-cleaned",
        "split": "longmemeval_s_cleaned",
        "seed": args.seed,
        "n_questions": len(smoke.questions),
        "class_counts": dict(Counter(q.reporting_class for q in smoke.questions)),
        "arms": sorted(systems.keys()),
        "union_docs": len(smoke.documents),
        "blockers": blockers,
        "expa": expa,
        "kill_check": kc,
        "best_candidate_k_for_expb": {"arm": pref_arm, "candidate_k": best_k},
        "elapsed_s": round(time.time() - t0, 1),
    }
    out_json = Path(args.out_json)
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(result, indent=2), encoding="utf-8")
    write_md(result, Path(args.out_md))
    print(
        f"[expa] arms={sorted(systems)} n={len(smoke.questions)} union={len(smoke.documents)} "
        f"best_k({pref_arm})={best_k} verdict={kc['verdict'][:40]} elapsed={result['elapsed_s']}s"
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
