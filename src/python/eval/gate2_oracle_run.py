"""0.8.11 Gate-2 — oracle-routing upper bound (eval FOUNDATION, $0 EVAL-ONLY).

Computes the best-plan-per-query *ceiling* that bounds the maximum value dynamic
routing could ever buy (PSD §III.B; `0.8.11-implementation.md` §1 Gate-2). **No LLM
calls** — a *fresh* oracle-context decomposition would need the priced gpt-5.4 reader
(`gap_decomposition_run.py`), so this Gate-2 reuses the already-paid, byte-verified
artifacts and computes the $0/local recall-arm oracle on top.

Two distinct, reconciled ceilings:

1. **Oracle-CONTEXT bound** (REUSE `0.8.3-gap-decomposition-n606.json`): the RETRIEVAL
   component ``acc_oracle_raw - acc_fathomdb`` = the answer-accuracy lift from *perfect*
   retrieval (gold docs in context). Pooled **+0.392 [0.346, 0.436]** — the headroom any
   retrieval improvement (incl. routing) could capture in principle. A fresh recompute
   would be priced (reader LLM) → deferred; reused here at $0.

2. **Oracle-ARM-selection bound** ($0, recall-based, this run): for each intent class,
   ``max_arm recall@10 - fused_RRF recall@10`` from existing per-arm recall runs — what
   *static-arm switching* alone (the narrowest router) could buy over the shipped
   fused-RRF baseline. (Per-class best-arm is a LOWER bound on the true per-query oracle,
   which is >= the per-class best; flagged as such — per-query per-arm cells were not
   persisted in the source runs.)

The **KILL check** (PSD §III.B / §1): is the ceiling within the noise band of fused-RRF
for every class? Emits ``dev/plans/runs/gate2-oracle-output.json``.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[3]
RUNS = REPO / "dev" / "plans" / "runs"

# Map the gap-decomposition memory classes to the PSD intent classes.
GAP_TO_INTENT = {
    "factoid": "needle",
    "knowledge_update": "needle",
    "multi_session": "multi_session",
    "temporal": "temporal",
}


def _load(p: Path) -> Any:
    return json.loads(p.read_text(encoding="utf-8"))


def oracle_context_bound() -> dict[str, Any]:
    """Per-class RETRIEVAL component (oracle_raw - fathomdb) from the n606 artifact."""
    d = _load(RUNS / "0.8.3-gap-decomposition-n606.json")
    comp = d["component_deltas"]
    out: dict[str, Any] = {}
    for cls in ("factoid", "knowledge_update", "multi_session", "temporal", "pooled"):
        r = comp[cls]["RETRIEVAL"]
        out[cls] = {
            "intent": GAP_TO_INTENT.get(cls, cls),
            "point": r["point"],
            "ci_lo": r["ci_lo"],
            "ci_hi": r["ci_hi"],
            "mde": r["mde"],
            "n": r["n"],
        }
    out["_source"] = "dev/plans/runs/0.8.3-gap-decomposition-n606.json (gpt-5.4 reader, n606, already paid)"
    out["_metric"] = "answer-accuracy lift from perfect (gold-doc) retrieval"
    return out


def arm_selection_bound() -> dict[str, Any]:
    """Class-level best-arm minus fused-RRF recall@10, LME (p0a n160) + MuSiQue (m1)."""
    p0a = _load(RUNS / "0.8.1-p0a-fused-recall-n160.json")
    rl = p0a["retrieval_loop"]
    arms = ["naive_bm25", "fathomdb_fts_only", "fathomdb_fused"]
    classes = ["factoid", "knowledge_update", "multi_session", "temporal"]
    lme: dict[str, Any] = {}
    for cls in classes:
        per = {a: rl[a]["per_class"][cls]["recall_at_10"] for a in arms}
        fused = per["fathomdb_fused"]
        best_arm = max(per, key=per.get)
        best = per[best_arm]
        lme[cls] = {
            "intent": GAP_TO_INTENT.get(cls, cls),
            "recall_at_10_per_arm": per,
            "fused_rrf": fused,
            "best_arm": best_arm,
            "best_recall_at_10": best,
            "oracle_arm_headroom": round(best - fused, 4),
        }
    # MuSiQue multi-hop: 5-arm F1 (>=3hop pooled), "fused" is the static RRF baseline.
    m1 = _load(RUNS / "0.8.2-m1-verdict-gpt54.json")
    fa = m1["five_arm_pooled_ge3hop"]
    f1 = {k: v["f1"] for k, v in fa.items()}
    fused_f1 = f1["fused"]
    best_arm_mh = max(f1, key=f1.get)
    multihop = {
        "intent": "multi_hop",
        "metric": "answer F1 (>=3-hop pooled, n=144)",
        "f1_per_arm": f1,
        "fused_rrf": fused_f1,
        "best_arm": best_arm_mh,
        "best_f1": f1[best_arm_mh],
        "oracle_arm_headroom": round(f1[best_arm_mh] - fused_f1, 4),
        "_note": "m1 primary endpoint ppr_fusion-vs-fused was a tie (ΔF1 -0.0405, CI[-0.116,+0.031]); dense > fused held the multi-hop signal",
    }
    return {
        "lme_recall_at_10": lme,
        "multi_hop_f1": multihop,
        "_metric": "best static arm minus fused-RRF (class-level; lower bound on per-query oracle)",
        "_caveat": "per-query per-arm cells not persisted in source runs → class-level best-arm only",
        "_sources": [
            "dev/plans/runs/0.8.1-p0a-fused-recall-n160.json (LME n160)",
            "dev/plans/runs/0.8.2-m1-verdict-gpt54.json (MuSiQue >=3hop n144)",
        ],
    }


def cost_tiers() -> dict[str, Any]:
    """Per-arm relative cost/latency tiers from existing measurements."""
    return {
        "fts_bm25": {
            "tier": "low",
            "evidence": "CPU; p50<1ms / p99 4ms @10k (0.8.0 tokenizer experiment)",
        },
        "vector_ann": {
            "tier": "low-medium",
            "evidence": "1-bit quant + f32 rerank; p50 25ms / p99 40ms (eu7, 0.7.x)",
        },
        "rrf": {
            "tier": "low",
            "evidence": "CPU rank-merge; negligible over the arms it fuses",
        },
        "ce_rerank": {
            "tier": "medium",
            "evidence": "TinyBERT-L-2 1.54ms/pair → 308ms @K=200 (fits budget; IR-C R0). "
                        "MiniLM-L12 16.82ms/pair → 3364ms @K=200 = HIGH (exceeds).",
        },
        "map_reduce_qfs": {
            "tier": "high",
            "evidence": "LLM tier, reads everything; per-query LLM call ($). 0.8.4 C-arm run >= $21. "
                        "F4(global)-only; router-isolated from needle paths.",
        },
        "graph_bfs": {
            "tier": "low-compute / ~0-value",
            "evidence": "CPU but measured-REFUTED x2 (ΔF1 -0.0405); default-OFF",
        },
    }


def kill_check(ctx: dict[str, Any], arm: dict[str, Any]) -> dict[str, Any]:
    """Is the oracle ceiling within the fused-RRF noise band for every class?"""
    # Oracle-context: clearly outside noise for every class (CI lower >> 0).
    ctx_classes = {
        c: {"point": ctx[c]["point"], "ci_lo": ctx[c]["ci_lo"], "outside_noise": ctx[c]["ci_lo"] > 0}
        for c in ("factoid", "knowledge_update", "multi_session", "temporal", "pooled")
    }
    ctx_all_outside = all(v["outside_noise"] for v in ctx_classes.values())

    # Arm-selection: headroom vs the per-class recall MDE proxy (small/within noise?).
    arm_head = {c: arm["lme_recall_at_10"][c]["oracle_arm_headroom"]
                for c in arm["lme_recall_at_10"]}
    arm_head["multi_hop"] = arm["multi_hop_f1"]["oracle_arm_headroom"]
    # n160/n300 per-class recall MDE ~ 0.11-0.17 (40/class LME; ~0.07 pooled MuSiQue);
    # treat headroom below this MDE proxy as within the measurement noise band.
    RECALL_MDE_PROXY = 0.11
    arm_within_noise = {c: (abs(h) < RECALL_MDE_PROXY) for c, h in arm_head.items()}

    return {
        "oracle_context": {
            "per_class": ctx_classes,
            "all_classes_outside_fused_noise": ctx_all_outside,
            "kill": not ctx_all_outside,
        },
        "oracle_arm_selection": {
            "headroom_per_class": arm_head,
            "within_fused_noise": arm_within_noise,
            "all_within_noise": all(arm_within_noise.values()),
        },
        "verdict": (
            "NO KILL on the oracle-CONTEXT axis: the perfect-retrieval ceiling is +0.25..+0.53 "
            "per class (pooled +0.392, CI lower 0.346 >> 0) — far outside fused-RRF noise; large "
            "routing-relevant headroom EXISTS. BUT static ARM-SELECTION headroom is small "
            "(recall +0.00..+0.05; multi_session = 0.00 since fused is already best; multi-hop "
            "+0.037 F1) and within the recall noise band for every class. CONCLUSION: the "
            "realizable headroom is in recall/precision GENERATION (EXP-A wider candidate gen, "
            "EXP-B' per-intent α/pool_n/candidate_k), captured by a CONFIG-CARRYING router — NOT "
            "by switching which static arm runs. The program is not killed; its value locus is "
            "config-carrying per-intent tuning, not arm routing."
        ),
    }


def build() -> dict[str, Any]:
    ctx = oracle_context_bound()
    arm = arm_selection_bound()
    return {
        "schema": "0.8.11-gate2-oracle-v1",
        "slice": 5,
        "cost_usd": 0.0,
        "method": (
            "Reuse the already-paid n606 oracle-context decomposition (fresh recompute would be "
            "priced → deferred) + compute the $0 recall-arm-selection oracle from existing per-arm "
            "recall runs. Attach per-arm cost tiers from prior latency/$ measurements."
        ),
        "oracle_context_bound": ctx,
        "oracle_arm_selection_bound": arm,
        "per_arm_cost_tiers": cost_tiers(),
        "kill_check": kill_check(ctx, arm),
        "reconciliation": {
            "prior_pooled_retrieval": "+0.392 [0.346, 0.436] (0.8.3 ledger, n606)",
            "this_pooled_retrieval": f"+{ctx['pooled']['point']:.4f} [{ctx['pooled']['ci_lo']:.4f}, {ctx['pooled']['ci_hi']:.4f}]",
            "reconciles": True,
            "why_exact": "same n606 artifact reused (a fresh gpt-5.4 recompute is deferred under the $0 constraint), so the oracle-context number is identical by construction, not an independent re-measurement.",
        },
    }


def main() -> int:
    art = build()
    out = RUNS / "gate2-oracle-output.json"
    out.write_text(json.dumps(art, indent=2), encoding="utf-8")
    p = art["oracle_context_bound"]["pooled"]
    print(
        f"[GATE2] wrote {out} | $0 | oracle-context pooled +{p['point']:.4f} "
        f"[{p['ci_lo']:.4f},{p['ci_hi']:.4f}] | KILL(context)="
        f"{art['kill_check']['oracle_context']['kill']} | "
        f"arm-selection all_within_noise={art['kill_check']['oracle_arm_selection']['all_within_noise']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
