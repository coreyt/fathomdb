"""0.8.11 Slice 30 — EXP-AF agent-feedback value test ($0 + small $, ceiling $5).

The decisive HITL-gated experiment (KILL/GO). Tests the EXP-AF hypothesis
(``planner-router-psd-0.8.x.md`` §III.D; ``0.8.11-implementation.md`` §1 EXP-AF):

  *An agent relevance/intent signal beats the engine's internal ``ce_score``-only
   routing NET of round-trip cost, on the existing substrate (no fresh rebuild),
   within the 1–2 re-plan depth bound.*

**What Slice 25 found (shapes this test).** A CHEAP agent (``gemini-flash-lite``)
re-judging the relevance of the *top-1* passage was DOMINATED by the free internal
``ce_score`` (lift −0.138; AUC 0.667 vs 0.545). The potential value (if any) lives in
the **break-even cells**: low-``ce_top`` (<0.2) queries where CE confidence is weak.
EXP-AF therefore tests a **STRONGER agent** (default ``claude-sonnet``) that sees the
**full candidate pool** (not just top-1) and is used to **actually re-rank** (a real
Rocchio-style relevance-feedback signal), focused on those break-even cells.

**Three measured arms, all from one agent call per query (real numbers, never fabricated):**

  1. **Reranking lift (PRIMARY).** Promote agent-flagged-relevant passages above the
     ``ce_score`` order over the shown top-N; measure the change in strict
     retrieval-success (all-gold-in-top-K). This is the deployment mechanism: the agent
     signal recovers precision the engine's ce_score missed. Paired bootstrap CI; then
     subtract the round-trip cost ``c_rt`` → **lift NET of round-trip** (the decisive
     KILL/GO number).
  2. **Detection lift (Slice-25-comparable).** Does the stronger agent's relevance flag
     on the top-1 passage beat ``ce_top`` at predicting retrieval-success? Directly
     comparable to the Slice-25 cheap-agent lift (−0.138) — does a stronger agent close
     the gap?
  3. **One-shot vs iterative (depth 1 vs 2).** Depth 1 shows the agent the top-N1 pool;
     depth 2 (the single allowed re-plan) expands to top-N2 on depth-1 failures and asks
     again. Reports the incremental lift of iterating once and whether it is justified
     within the 1–2 depth bound (net of the doubled round-trip).

A $0 **headroom** pre-gate (computed before any spend) bounds the maximum lift any
reranking agent could realize (all-gold reachable in top-N but not top-K) — if it is ~0
the arm is structurally KILLed without spending.

The CE reranker is ACTIVE in this build (``default-reranker``); ``ce_score`` is real,
confirmed by a degeneracy guard (reused from Slice 25) before any measurement. Resilient
harness: per-item checkpoint, idempotent ``--resume``, 429/5xx backoff (shared ``LLM``
client), ``BudgetLedger`` pre-call $-guard, running tally. Cheap-validate first.
"""

from __future__ import annotations

import argparse
import json
import re
import time
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional, Sequence, cast

import numpy as np

from eval.fracc_classifier_run import LLM
from eval.fracc_voi_run import (
    DEFAULT_ALPHA,
    INTENT_ALPHA,
    LME_CLASS_TO_INTENT,
    assert_ce_active,
    auc,
    boot_ci,
    boot_paired_diff,
    reblend,
)
from eval.gap_decomposition_run import (
    BudgetExceeded,
    BudgetLedger,
    estimate_tokens,
    price_for,
)
from eval.m1_verdict_run import _atomic_write_json
from eval.rerank_tune_probe import strict_recall_at_k

REPO = Path(__file__).resolve().parents[3]
RUNS = REPO / "dev" / "plans" / "runs"

CE_PASS = RUNS / "0.8.3-rerank-tune.ce-pass.json"
LME_GOLD = RUNS / "0.8.3-d0a-memory-gold.json"

FINAL_K = 10        # retrieval-success cut (gold-in-top-K), matches Slice 25
POOL_N = 50         # ce reblend pool depth (engine contract), matches Slice 25
N1 = 20             # depth-1: passages shown to the agent
N2 = 40             # depth-2: expanded window on depth-1 failures
BE_CE_THRESHOLD = 0.2  # break-even cell: low ce_top (Slice-25 VoI concentration)
PASSAGE_CHARS = 350    # per-passage truncation (bounds prompt tokens)

#: Round-trip cost grid (answer-quality-equivalent units; same convention as Slice 25
#: deliverable 2): how much the product values one agent round-trip. The decisive
#: net-lift is reported for each. 0.02/0.05/0.10 ≈ "a round-trip is worth 2/5/10 pts".
C_RT_GRID = (0.0, 0.02, 0.05, 0.10)

INTENTS = ("needle", "multi_session", "temporal")
BOOT_SEED = 30


# --------------------------------------------------------------------------- #
# Substrate ($0): per-query ce order + gold + pool, restricted to break-even cells.
# --------------------------------------------------------------------------- #
def load_pool_records() -> list[dict[str, Any]]:
    """Per LME query: intent, ce_top, ce_margin, gold, and the full ce-reblended doc
    order (top-POOL_N reranked at the intent alpha + base-order tail). Keeps the order
    so a reranking agent can be applied to it. Mirrors ``fracc_voi_run.load_stage_a``
    but retains the FULL ordered id list (needed for the reranking arm)."""
    recs = json.loads(CE_PASS.read_text(encoding="utf-8"))["records"]
    gold_q = json.loads(LME_GOLD.read_text(encoding="utf-8"))["queries"]
    qid2q = {str(q["query_id"]): q["query"] for q in gold_q}
    out: list[dict[str, Any]] = []
    for r in recs:
        intent = LME_CLASS_TO_INTENT.get(r["reporting_class"], r["reporting_class"])
        alpha = INTENT_ALPHA.get(cast(str, intent), DEFAULT_ALPHA)
        gold = [str(g) for g in r["gold"]]
        ranked = reblend(r["pool"], alpha=alpha, pool_n=POOL_N)
        ranked_ids = [str(p["doc_id"]) for p in ranked]
        tail = [str(p["doc_id"]) for p in r["pool"][POOL_N:]]
        full = ranked_ids + [d for d in tail if d not in ranked_ids]
        ce_sorted = [float(p["ce_norm"]) for p in ranked]
        ce_top = ce_sorted[0] if ce_sorted else 0.0
        ce_margin = (ce_sorted[0] - ce_sorted[1]) if len(ce_sorted) >= 2 else ce_top
        out.append({
            "qid": str(r["qid"]),
            "query": qid2q.get(str(r["qid"]), ""),
            "intent": intent,
            "ce_top": round(ce_top, 6),
            "ce_margin": round(ce_margin, 6),
            "gold": gold,
            "ce_order": full,
            "ce_rc": int(strict_recall_at_k(full, gold, FINAL_K)),
        })
    return out


def _all_in(order: Sequence[str], gold: Sequence[str], n: int) -> bool:
    top = set(order[:n])
    return bool(gold) and all(g in top for g in gold)


def headroom(records: list[dict[str, Any]]) -> dict[str, Any]:
    """$0 ceiling on reranking lift: fraction of queries where all gold is reachable in
    top-N (1/2) but NOT in top-K under ce — the max a perfect reranker could promote."""
    def frac(sel: list[dict[str, Any]], n: int) -> float:
        if not sel:
            return 0.0
        return float(np.mean([
            1 if (not _all_in(r["ce_order"], r["gold"], FINAL_K)
                  and _all_in(r["ce_order"], r["gold"], n)) else 0
            for r in sel
        ]))
    out: dict[str, Any] = {"overall": {}, "by_intent": {}}
    out["overall"] = {
        "n": len(records),
        "base_retrieval_success": round(float(np.mean([r["ce_rc"] for r in records])), 4),
        "depth1_ceiling_top%d_into_top%d" % (N1, FINAL_K): round(frac(records, N1), 4),
        "depth2_ceiling_top%d_into_top%d" % (N2, FINAL_K): round(frac(records, N2), 4),
    }
    for it in INTENTS:
        sel = [r for r in records if r["intent"] == it]
        out["by_intent"][it] = {
            "n": len(sel),
            "base_retrieval_success": round(float(np.mean([r["ce_rc"] for r in sel])), 4) if sel else None,
            "depth1_ceiling": round(frac(sel, N1), 4),
            "depth2_ceiling": round(frac(sel, N2), 4),
        }
    return out


# --------------------------------------------------------------------------- #
# Agent prompt + parsing (one call returns relevance over ALL shown passages).
# --------------------------------------------------------------------------- #
def _rerank_prompt(question: str, passages: list[str], offset: int = 0) -> str:
    body = "\n\n".join(
        f"[{offset + i + 1}] {p[:PASSAGE_CHARS]}" for i, p in enumerate(passages)
    )
    lo, hi = offset + 1, offset + len(passages)
    return (
        "You are selecting passages that answer a user's question. Below are candidate "
        "passages, each with a number. Identify EVERY passage that contains or directly "
        "supports the answer to the question. Be precise: include a passage only if it "
        "genuinely helps answer the question.\n\n"
        f"Respond with ONLY a JSON object on one line: "
        f'{{"relevant": [passage numbers from {lo} to {hi}], "answerable": true or false}}. '
        '"answerable" is true iff at least one listed passage supports the answer; if none '
        "do, use an empty list and false.\n\n"
        f"Question: {question}\n\nPassages:\n{body}\n\nJSON:"
    )


_JSON_RE = re.compile(r"\{.*?\}", re.DOTALL)


def _parse_relevance(raw: str, valid: set[int]) -> tuple[list[int], bool, bool]:
    """Return (relevant_indices(1-based, filtered to valid), answerable, parse_ok)."""
    if not raw:
        return [], False, False
    m = _JSON_RE.search(raw)
    if not m:
        return [], False, False
    try:
        obj = json.loads(m.group(0))
    except Exception:  # noqa: BLE001
        return [], False, False
    rel_raw = obj.get("relevant", [])
    rel: list[int] = []
    if isinstance(rel_raw, list):
        for x in rel_raw:
            try:
                xi = int(x)
            except (TypeError, ValueError):
                continue
            if xi in valid:
                rel.append(xi)
    answerable = bool(obj.get("answerable", len(rel) > 0))
    return sorted(set(rel)), answerable, True


def _rerank_order(ce_order: list[str], shown_n: int, relevant_1based: list[int]) -> list[str]:
    """Stable promotion: agent-flagged passages (in ce-relative order) to the front,
    then the remaining ce order. Only the top-``shown_n`` positions were shown."""
    rel_idx = {i - 1 for i in relevant_1based if 1 <= i <= shown_n}
    promoted = [ce_order[i] for i in range(min(shown_n, len(ce_order))) if i in rel_idx]
    rest = [d for j, d in enumerate(ce_order) if j not in rel_idx]
    return promoted + rest


# --------------------------------------------------------------------------- #
# Priced agent arm (resilient).
# --------------------------------------------------------------------------- #
def run_agent_arm(
    sample: list[dict[str, Any]], doc_text: dict[str, str], *, llm: LLM,
    ledger: BudgetLedger, do_depth2: bool, checkpoint: Optional[Path],
    resume: Optional[Path],
) -> dict[str, dict[str, Any]]:
    """Per query: depth-1 agent relevance over top-N1; if depth-1 fails to capture all
    gold and depth2 is on, a single re-plan over top-N2. Per-item checkpoint/resume."""
    records: dict[str, dict[str, Any]] = {}
    src = resume or (checkpoint if (checkpoint and checkpoint.exists()) else None)
    if src and src.exists():
        blob = json.loads(src.read_text(encoding="utf-8"))
        records = blob.get("records", {})
        ledger.restore_spent(float(blob.get("spent_usd", 0.0)))

    def persist() -> None:
        if checkpoint is not None:
            _atomic_write_json(checkpoint, {"records": records, "spent_usd": ledger.spent})

    def call(prompt: str) -> tuple[str, int, int]:
        ledger.guard(llm.model, estimate_tokens(prompt))
        out = llm.complete(prompt) or ""
        pt = llm.last_prompt_tokens or estimate_tokens(prompt)
        ct = llm.last_completion_tokens or 16
        ledger.record(llm.model, pt, ct)
        return out, pt, ct

    for it in sample:
        key = it["qid"]
        rec = records.get(key)
        order = it["ce_order"]
        gold = it["gold"]

        # ---- depth 1 ----
        if not (rec and rec.get("d1_done")):
            shown1 = order[:N1]
            passages1 = [doc_text.get(d, "") for d in shown1]
            valid1 = set(range(1, len(shown1) + 1))
            raw1, pt1, ct1 = call(_rerank_prompt(it["query"], passages1))
            rel1, ans1, ok1 = _parse_relevance(raw1, valid1)
            order1 = _rerank_order(order, len(shown1), rel1)
            rec = {
                "qid": key, "intent": it["intent"], "ce_top": it["ce_top"],
                "ce_margin": it["ce_margin"], "ce_rc": it["ce_rc"],
                "d1_relevant": rel1, "d1_answerable": ans1, "d1_parse_ok": ok1,
                "d1_top1_relevant": int(1 in rel1),
                "d1_rc": int(strict_recall_at_k(order1, gold, FINAL_K)),
                "d1_pt": pt1, "d1_ct": ct1, "d1_done": True,
                "d2_done": False,
            }
            records[key] = rec
            persist()

        # ---- depth 2 (single re-plan, only on depth-1 failures) ----
        if do_depth2 and not rec.get("d2_done"):
            if rec["d1_rc"] == 1:
                # already captured at depth 1 — no re-plan needed; carry forward.
                rec["d2_rc"] = 1
                rec["d2_triggered"] = False
                rec["d2_done"] = True
            else:
                shown2 = order[:N2]
                extra = order[N1:N2]
                passages2 = [doc_text.get(d, "") for d in extra]
                valid2 = set(range(N1 + 1, N1 + len(extra) + 1))
                raw2, pt2, ct2 = call(_rerank_prompt(it["query"], passages2, offset=N1))
                rel2_extra, ans2, ok2 = _parse_relevance(raw2, valid2)
                # merge depth-1 + depth-2 relevance over the top-N2 window
                merged = sorted(set(rec.get("d1_relevant", [])) | set(rel2_extra))
                order2 = _rerank_order(order, len(shown2), merged)
                rec["d2_relevant_extra"] = rel2_extra
                rec["d2_answerable"] = ans2
                rec["d2_parse_ok"] = ok2
                rec["d2_rc"] = int(strict_recall_at_k(order2, gold, FINAL_K))
                rec["d2_triggered"] = True
                rec["d2_pt"] = pt2
                rec["d2_ct"] = ct2
                rec["d2_done"] = True
                records[key] = rec
                persist()

    return records


# --------------------------------------------------------------------------- #
# Analysis.
# --------------------------------------------------------------------------- #
def _net_grid(lift_ci: dict[str, Any], depth_calls: float) -> dict[str, Any]:
    """Lift NET of round-trip cost over the c_rt grid. depth_calls = round-trips per
    query for this arm (1 for depth-1; ~1+trigger_rate for depth-2)."""
    out: dict[str, Any] = {}
    pt = lift_ci.get("point")
    lo = lift_ci.get("lo")
    hi = lift_ci.get("hi")
    for crt in C_RT_GRID:
        cost = round(crt * depth_calls, 4)
        out[f"{crt:.2f}"] = {
            "c_rt_per_call": crt,
            "round_trips_per_query": round(depth_calls, 3),
            "net_lift_point": round(pt - cost, 4) if pt is not None else None,
            "net_lift_lo": round(lo - cost, 4) if lo is not None else None,
            "net_lift_hi": round(hi - cost, 4) if hi is not None else None,
            "go": bool(lo is not None and (lo - cost) > 0),
        }
    return out


def analyse(records: dict[str, dict[str, Any]], sample: list[dict[str, Any]],
            do_depth2: bool) -> dict[str, Any]:
    rows = [records[it["qid"]] for it in sample if it["qid"] in records and records[it["qid"]].get("d1_done")]
    ce = [r["ce_rc"] for r in rows]
    d1 = [r["d1_rc"] for r in rows]

    # PRIMARY: depth-1 reranking lift (agent − ce), paired bootstrap.
    lift_d1 = boot_paired_diff(d1, ce, seed=BOOT_SEED)
    net_d1 = _net_grid(lift_d1, depth_calls=1.0)

    # Detection (Slice-25-comparable): agent top-1 relevance vs ce_top at predicting ce_rc.
    y = [r["ce_rc"] for r in rows]
    ce_top = [r["ce_top"] for r in rows]
    agent_top1 = [r["d1_top1_relevant"] for r in rows]

    def bal_acc(pred: Sequence[int], lab: Sequence[int]) -> float:
        pred_a, lab_a = np.asarray(pred), np.asarray(lab)
        pos, neg = lab_a == 1, lab_a == 0
        tpr = float(pred_a[pos].mean()) if pos.any() else 0.0
        tnr = float((1 - pred_a[neg]).mean()) if neg.any() else 0.0
        return (tpr + tnr) / 2.0

    thr_grid = sorted(set(round(c, 4) for c in ce_top))
    best_thr, best_ba = 0.5, -1.0
    for thr in thr_grid:
        ba = bal_acc([1 if c >= thr else 0 for c in ce_top], y)
        if ba > best_ba:
            best_ba, best_thr = ba, thr
    ce_pred_best = [1 if c >= best_thr else 0 for c in ce_top]
    ce_corr = [int(p == t) for p, t in zip(ce_pred_best, y)]
    ag_corr = [int(p == t) for p, t in zip(agent_top1, y)]
    detection = {
        "n": len(rows),
        "method": "agent top-1 relevance flag vs internal ce_top (oracle threshold) at "
                  "predicting retrieval-success (all-gold-in-top-10). Directly comparable "
                  "to Slice-25 cheap-agent lift (−0.138).",
        "agent_relevance_rate_top1": round(float(np.mean(agent_top1)), 4),
        "ce_threshold_best": best_thr,
        "balanced_acc_agent_top1": round(bal_acc(agent_top1, y), 4),
        "balanced_acc_ce_best": round(best_ba, 4),
        "acc_agent_top1": boot_ci(ag_corr, seed=BOOT_SEED),
        "acc_ce_best": boot_ci(ce_corr, seed=BOOT_SEED),
        "lift_agent_minus_ce_acc": boot_paired_diff(ag_corr, ce_corr, seed=BOOT_SEED),
        "auc_ce_top": round(cast(float, auc(ce_top, y)), 4) if auc(ce_top, y) is not None else None,
        "auc_agent_top1_binary": round(cast(float, auc(agent_top1, y)), 4) if auc(agent_top1, y) is not None else None,
        "slice25_cheap_agent_lift": -0.1378,
    }

    # Per-intent depth-1 lift.
    by_intent: dict[str, Any] = {}
    for it_name in INTENTS:
        sel = [r for r in rows if r["intent"] == it_name]
        if sel:
            by_intent[it_name] = {
                "n": len(sel),
                "ce_rc": round(float(np.mean([r["ce_rc"] for r in sel])), 4),
                "d1_rc": round(float(np.mean([r["d1_rc"] for r in sel])), 4),
                "lift": boot_paired_diff([r["d1_rc"] for r in sel], [r["ce_rc"] for r in sel], seed=BOOT_SEED),
            }

    # Promotion / demotion accounting (mechanism transparency).
    promoted = sum(1 for r in rows if r["ce_rc"] == 0 and r["d1_rc"] == 1)
    demoted = sum(1 for r in rows if r["ce_rc"] == 1 and r["d1_rc"] == 0)

    result: dict[str, Any] = {
        "n_evaluated": len(rows),
        "by_intent_n": {i: sum(1 for r in rows if r["intent"] == i) for i in INTENTS},
        "base_retrieval_success_ce": round(float(np.mean(ce)), 4) if rows else None,
        "agent_retrieval_success_depth1": round(float(np.mean(d1)), 4) if rows else None,
        "primary_reranking_lift_depth1": lift_d1,
        "net_lift_depth1_by_c_rt": net_d1,
        "promoted_gold_into_topk": promoted,
        "demoted_gold_out_of_topk": demoted,
        "depth1_by_intent": by_intent,
        "detection_slice25_comparable": detection,
    }

    # Depth-2 (iterative) analysis.
    if do_depth2:
        d2rows = [r for r in rows if r.get("d2_done")]
        if d2rows:
            ce2 = [r["ce_rc"] for r in d2rows]
            d1_2 = [r["d1_rc"] for r in d2rows]
            d2 = [r.get("d2_rc", r["d1_rc"]) for r in d2rows]
            trig_rate = float(np.mean([1 if r.get("d2_triggered") else 0 for r in d2rows]))
            lift_d2_vs_ce = boot_paired_diff(d2, ce2, seed=BOOT_SEED)
            lift_d2_vs_d1 = boot_paired_diff(d2, d1_2, seed=BOOT_SEED)
            # depth-2 pays ~ (1 + trigger_rate) round-trips/query.
            net_d2 = _net_grid(lift_d2_vs_ce, depth_calls=1.0 + trig_rate)
            promoted2 = sum(1 for r in d2rows if r["d1_rc"] == 0 and r.get("d2_rc") == 1)
            result["depth2_iterative"] = {
                "n": len(d2rows),
                "replan_trigger_rate": round(trig_rate, 4),
                "agent_retrieval_success_depth2": round(float(np.mean(d2)), 4),
                "incremental_lift_depth2_vs_depth1": lift_d2_vs_d1,
                "total_lift_depth2_vs_ce": lift_d2_vs_ce,
                "net_lift_depth2_by_c_rt": net_d2,
                "extra_gold_recovered_by_replan": promoted2,
                "round_trips_per_query": round(1.0 + trig_rate, 3),
            }
    return result


# --------------------------------------------------------------------------- #
# Orchestration.
# --------------------------------------------------------------------------- #
def select_breakeven(records: list[dict[str, Any]], *, threshold: float,
                     max_per_class: Optional[int], doc_text: dict[str, str],
                     seed: int) -> list[dict[str, Any]]:
    """Break-even cells: low ce_top. Require the top-1 doc to have text (agent needs
    passages). Balanced sample per intent (capped)."""
    import random as _random
    rng = _random.Random(seed)
    by_intent: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for r in records:
        if r["ce_top"] >= threshold:
            continue
        if r["intent"] not in INTENTS:
            continue
        if not r["ce_order"] or r["ce_order"][0] not in doc_text:
            continue
        by_intent[r["intent"]].append(r)
    sample: list[dict[str, Any]] = []
    for it in INTENTS:
        items = by_intent.get(it, [])
        rng.shuffle(items)
        sample.extend(items[: max_per_class] if max_per_class else items)
    return sample


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="EXP-AF agent-feedback value test (0.8.11 Slice 30)")
    ap.add_argument("--model", default="claude-sonnet", help="stronger agent (pinned-price id)")
    ap.add_argument("--be-threshold", type=float, default=BE_CE_THRESHOLD)
    ap.add_argument("--max-per-class", type=int, default=None, help="cap break-even sample/class")
    ap.add_argument("--max-usd", type=float, default=5.0)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--depth2", action="store_true", help="run the depth-2 iterative re-plan arm")
    ap.add_argument("--cheap-validate", type=int, default=0, help="run only N agent calls (probe)")
    ap.add_argument("--skip-agent", action="store_true", help="$0 headroom only")
    ap.add_argument("--checkpoint", default=str(RUNS / "expaf-value.checkpoint.json"))
    ap.add_argument("--resume", default=None)
    ap.add_argument("--out", default=str(RUNS / "expaf-value-output.json"))
    ap.add_argument("--out-md", default=str(RUNS / "expaf-value.md"))
    args = ap.parse_args(argv)

    t0 = time.time()
    print("[expaf] confirming CE reranker is ACTIVE ...")
    ce_guard = assert_ce_active()
    print(f"[expaf] CE active: max_ce={ce_guard['max_ce_norm']} spread={ce_guard['spread']} "
          f"order={ce_guard['alpha1_order']}")

    print("[expaf] loading substrate ($0) — 606 LME ce-pass queries ...")
    records = load_pool_records()
    head = headroom(records)
    print(f"[expaf] headroom (break-even subset computed below). overall base "
          f"rc={head['overall']['base_retrieval_success']}")

    # Break-even headroom (the decisive $0 pre-gate).
    be_all = [r for r in records if r["ce_top"] < args.be_threshold and r["intent"] in INTENTS]
    be_head = headroom(be_all)
    print(f"[expaf] break-even (ce_top<{args.be_threshold}) n={be_head['overall']['n']} "
          f"depth1_ceiling={be_head['overall'][f'depth1_ceiling_top{N1}_into_top{FINAL_K}']} "
          f"depth2_ceiling={be_head['overall'][f'depth2_ceiling_top{N2}_into_top{FINAL_K}']}")

    agent: dict[str, Any]
    analysis: dict[str, Any] = {}
    sample: list[dict[str, Any]] = []
    if args.skip_agent:
        agent = {"status": "SKIPPED", "reason": "--skip-agent ($0 headroom only)"}
    else:
        llm = LLM(model=args.model, max_tokens=120)
        if not llm.available:
            agent = {"status": "DEFERRED", "reason": "R2_RUN!=1 or judge env unset"}
        else:
            from eval.r2_parity_eval import load_longmemeval
            doc_text, _q = load_longmemeval("xiaowu0162/longmemeval-cleaned", "oracle")
            price_for(args.model)  # fail closed if unpinned
            ledger = BudgetLedger(opening_balance_usd=0.0, hard_cap_usd=args.max_usd,
                                  max_output_tokens=120)
            sample = select_breakeven(records, threshold=args.be_threshold,
                                      max_per_class=args.max_per_class, doc_text=doc_text,
                                      seed=args.seed)
            if args.cheap_validate:
                sample = sample[: args.cheap_validate]
            print(f"[expaf] agent arm: model={args.model} n={len(sample)} depth2={args.depth2} "
                  f"cap=${args.max_usd}")
            try:
                recs = run_agent_arm(
                    sample, doc_text, llm=llm, ledger=ledger,
                    do_depth2=args.depth2 and not args.cheap_validate,
                    checkpoint=Path(args.checkpoint),
                    resume=Path(args.resume) if args.resume else None,
                )
                analysis = analyse(recs, sample, do_depth2=args.depth2 and not args.cheap_validate)
                status = "cheap_validate" if args.cheap_validate else "OK"
                agent = {
                    "status": status, "model": args.model,
                    "n_sample": len(sample), "spent_usd": round(ledger.spent, 4),
                    "n_calls": llm.n_calls, "n_errors": llm.n_errors,
                    **analysis,
                }
            except BudgetExceeded as e:
                agent = {"status": "BUDGET_EXCEEDED", "reason": str(e),
                         "spent_usd": round(ledger.spent, 4)}
            print(f"[expaf] agent arm: status={agent.get('status')} spent=${agent.get('spent_usd')} "
                  f"primary_lift={agent.get('primary_reranking_lift_depth1')}")

    # ---- KILL/GO verdict ----
    verdict = build_verdict(agent, be_head)

    out = {
        "schema": "0.8.11-expaf-value-v1",
        "slice": 30,
        "experiment": "EXP-AF — agent-feedback value test (KILL/GO → HITL #4)",
        "ce_active_guard": ce_guard,
        "design": {
            "hypothesis": "an agent relevance/intent signal beats internal ce_score-only "
                          "routing NET of round-trip cost, on the existing substrate, within "
                          "the 1-2 re-plan depth bound (PSD §III.D).",
            "break_even_cells": f"low ce_top (<{args.be_threshold}) — where Slice-25 located the "
                                "potential VoI (cheap agent there was dominated by ce_score).",
            "mechanism": f"stronger agent ({args.model}) sees the top-{N1} ce-reranked pool (NOT "
                         f"just top-1) and flags relevant passages; promote them above ce order; "
                         f"measure strict retrieval-success (all-gold-in-top-{FINAL_K}) lift, then "
                         "subtract round-trip cost. Depth-2 expands to "
                         f"top-{N2} on depth-1 failures (the single allowed re-plan).",
            "final_K": FINAL_K, "N1": N1, "N2": N2, "pool_n": POOL_N,
            "c_rt_grid": list(C_RT_GRID),
        },
        "headroom_breakeven": be_head,
        "headroom_full_corpus": head,
        "agent_arm": agent,
        "verdict": verdict,
        "total_spent_usd": round(agent.get("spent_usd", 0.0) or 0.0, 4) if isinstance(agent, dict) else 0.0,
        "elapsed_s": round(time.time() - t0, 1),
    }
    Path(args.out).write_text(json.dumps(out, indent=2, default=str), encoding="utf-8")
    write_md(out, Path(args.out_md))
    print(f"[expaf] wrote {args.out} + {args.out_md} (elapsed {out['elapsed_s']}s, "
          f"spent ${out['total_spent_usd']}) VERDICT={verdict['decision']}")
    return 0


def build_verdict(agent: dict[str, Any], be_head: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(agent, dict) or agent.get("status") not in ("OK",):
        return {
            "decision": "INCONCLUSIVE",
            "reason": f"agent arm status={agent.get('status') if isinstance(agent, dict) else None}",
        }
    # Decisive number: depth-1 reranking lift net of round-trip at the modest c_rt=0.02
    # (Slice-25 reference round-trip cost). GO iff net-lift CI lower bound > 0.
    lift = agent.get("primary_reranking_lift_depth1", {}) or {}
    net = agent.get("net_lift_depth1_by_c_rt", {}) or {}
    net02 = net.get("0.02", {})
    net05 = net.get("0.05", {})
    go_d1 = bool(net02.get("go"))
    # Depth-2 (if run): justified iff its incremental lift over depth-1 clears noise AND
    # its total net lift beats depth-1's at the same c_rt.
    d2 = agent.get("depth2_iterative")
    depth_decision: dict[str, Any] = {}
    if d2:
        inc = d2.get("incremental_lift_depth2_vs_depth1", {}) or {}
        net2 = (d2.get("net_lift_depth2_by_c_rt", {}) or {}).get("0.02", {})
        inc_positive = bool(inc.get("lo") is not None and inc["lo"] > 0)
        d2_net_beats_d1 = bool(net2.get("net_lift_point") is not None
                               and net02.get("net_lift_point") is not None
                               and net2["net_lift_point"] > net02["net_lift_point"])
        depth_decision = {
            "incremental_lift_depth2_vs_depth1": inc,
            "incremental_positive_ci": inc_positive,
            "depth2_net_beats_depth1_at_c_rt_0.02": d2_net_beats_d1,
            "recommended_depth": 2 if (inc_positive and d2_net_beats_d1) else 1,
            "rationale": ("depth-2 (one re-plan) recovers gold beyond depth-1 net of its "
                          "doubled round-trip" if (inc_positive and d2_net_beats_d1) else
                          "iterating once does NOT pay net of the extra round-trip → stay at "
                          "depth 1 (one-shot)"),
        }
    else:
        depth_decision = {"recommended_depth": 1, "rationale": "depth-2 arm not run"}

    decision = "GO" if go_d1 else "KILL"
    return {
        "decision": decision,
        "decisive_number": {
            "primary_reranking_lift_depth1": lift,
            "net_lift_at_c_rt_0.02": net02,
            "net_lift_at_c_rt_0.05": net05,
            "rule": "GO iff the depth-1 reranking-lift CI lower bound, NET of one round-trip "
                    "(c_rt=0.02 accuracy-equivalent), exceeds 0.",
        },
        "depth_decision": depth_decision,
        "detection_comparison": {
            "stronger_agent_detection_lift": agent.get("detection_slice25_comparable", {}).get("lift_agent_minus_ce_acc"),
            "slice25_cheap_agent_lift": -0.1378,
        },
        "implications": (
            ("GO — the stronger agent's relevance signal beats internal ce_score net of "
             "round-trip on the break-even cells. The L2 prototype (Slice 35) KEEPS the "
             "feedback arm (feedback_arm=True); F-8b should PROMOTE record_feedback to a "
             "governed command (reserved-gap patch to Slice 40).")
            if decision == "GO" else
            ("KILL — even a stronger agent on the break-even cells does NOT beat ce_score net "
             "of round-trip. The L2 prototype (Slice 35) DROPS the agent-signal loop "
             "(feedback_arm=False; router stays on internal ce_score); record_feedback STAYS "
             "instrumentation (overrides any F-8b promote).")
        ),
    }


def write_md(out: dict[str, Any], path: Path) -> None:
    L: list[str] = []
    A = L.append
    a = out["agent_arm"]
    v = out["verdict"]
    bh = out["headroom_breakeven"]["overall"]
    A("# EXP-AF — agent-feedback value test (0.8.11 Slice 30, KILL/GO → HITL #4)")
    A("")
    A("> The decisive HITL-gated experiment. **Real measured numbers** — the CE reranker "
      "is ACTIVE (`default-reranker`); `ce_score` is real, confirmed by a degeneracy guard.")
    A("")
    A(f"- **Hypothesis:** {out['design']['hypothesis']}")
    A(f"- **Break-even cells:** {out['design']['break_even_cells']}")
    A(f"- **Mechanism:** {out['design']['mechanism']}")
    A(f"- **CE-active guard:** max ce_norm={out['ce_active_guard']['max_ce_norm']}, "
      f"spread={out['ce_active_guard']['spread']}, alpha=1.0 reorders relevant→rank1 "
      f"(order={out['ce_active_guard']['alpha1_order']}). PASS.")
    if isinstance(a, dict):
        A(f"- **Agent / spend:** model `{a.get('model')}`, status {a.get('status')}, "
          f"${out['total_spent_usd']} of $5 (n_calls {a.get('n_calls')}, errors {a.get('n_errors')}).")
    A("")
    A("## $0 headroom pre-gate (break-even cells)")
    A("")
    A(f"Of n={bh['n']} break-even queries (base retrieval-success {bh['base_retrieval_success']}), "
      f"the MAX lift any reranker could realize (all-gold reachable in the shown window but not "
      f"top-{FINAL_K} under ce):")
    A("")
    A(f"- depth-1 ceiling (top-{N1}→top-{FINAL_K}): "
      f"**{bh[f'depth1_ceiling_top{N1}_into_top{FINAL_K}']}**")
    A(f"- depth-2 ceiling (top-{N2}→top-{FINAL_K}): "
      f"**{bh[f'depth2_ceiling_top{N2}_into_top{FINAL_K}']}**")
    A("")
    if isinstance(a, dict) and a.get("status") in ("OK", "cheap_validate"):
        A("## Arm 1 — reranking lift (PRIMARY, decisive)")
        A("")
        A(f"- n={a['n_evaluated']} ({a['by_intent_n']}); ce retrieval-success "
          f"{a['base_retrieval_success_ce']} → agent depth-1 {a['agent_retrieval_success_depth1']}.")
        lift = a["primary_reranking_lift_depth1"]
        A(f"- **reranking lift (agent − ce, paired):** **{lift['point']} "
          f"[{lift['lo']},{lift['hi']}]** (n={lift['n']}).")
        A(f"- mechanism: promoted {a['promoted_gold_into_topk']} gold into top-{FINAL_K}, "
          f"demoted {a['demoted_gold_out_of_topk']} out.")
        A("")
        A("**Lift NET of round-trip cost (decisive KILL/GO number):**")
        A("")
        A("| c_rt (per round-trip) | net lift point | net lift CI | GO? |")
        A("|---|---|---|---|")
        for crt, d in a["net_lift_depth1_by_c_rt"].items():
            A(f"| {crt} | {d['net_lift_point']} | [{d['net_lift_lo']},{d['net_lift_hi']}] | {d['go']} |")
        A("")
        A("**Depth-1 lift by intent:**")
        A("")
        A("| intent | n | ce rc | agent rc | lift [CI] |")
        A("|---|---|---|---|---|")
        for it, d in a.get("depth1_by_intent", {}).items():
            lf = d["lift"]
            A(f"| {it} | {d['n']} | {d['ce_rc']} | {d['d1_rc']} | {lf['point']} [{lf['lo']},{lf['hi']}] |")
        A("")
        det = a["detection_slice25_comparable"]
        A("## Arm 2 — detection lift (Slice-25-comparable)")
        A("")
        A("Does the stronger agent's top-1 relevance flag beat `ce_top` at predicting "
          "retrieval-success? (Slice-25 cheap agent: lift −0.138.)")
        A("")
        dl = det["lift_agent_minus_ce_acc"]
        A(f"- agent top-1 relevance rate {det['agent_relevance_rate_top1']}; "
          f"balanced-acc agent {det['balanced_acc_agent_top1']} vs ce@best {det['balanced_acc_ce_best']}.")
        A(f"- **detection lift (agent − ce, paired acc):** **{dl['point']} [{dl['lo']},{dl['hi']}]** "
          f"(n={dl['n']}); AUC ce {det['auc_ce_top']} vs agent(binary) {det['auc_agent_top1_binary']}.")
        A(f"- vs Slice-25 cheap-agent lift {det['slice25_cheap_agent_lift']}.")
        A("")
        if a.get("depth2_iterative"):
            d2 = a["depth2_iterative"]
            A("## Arm 3 — one-shot vs iterative (depth 1 vs 2)")
            A("")
            inc = d2["incremental_lift_depth2_vs_depth1"]
            tot = d2["total_lift_depth2_vs_ce"]
            A(f"- depth-2 re-plan trigger rate {d2['replan_trigger_rate']} "
              f"(~{d2['round_trips_per_query']} round-trips/query); recovered "
              f"{d2['extra_gold_recovered_by_replan']} extra gold.")
            A(f"- **incremental lift (depth-2 − depth-1):** {inc['point']} [{inc['lo']},{inc['hi']}].")
            A(f"- total lift depth-2 vs ce: {tot['point']} [{tot['lo']},{tot['hi']}].")
            A("")
            A("**Depth-2 lift NET of round-trip:**")
            A("")
            A("| c_rt | net lift point | net lift CI | GO? |")
            A("|---|---|---|---|")
            for crt, d in d2["net_lift_depth2_by_c_rt"].items():
                A(f"| {crt} | {d['net_lift_point']} | [{d['net_lift_lo']},{d['net_lift_hi']}] | {d['go']} |")
            A("")
    A("## KILL/GO verdict (HITL #4)")
    A("")
    A(f"- **DECISION: {v['decision']}**")
    if "decisive_number" in v:
        dn = v["decisive_number"]
        net02 = dn["net_lift_at_c_rt_0.02"]
        A(f"- decisive number — depth-1 reranking lift NET of one round-trip (c_rt=0.02): "
          f"**{net02.get('net_lift_point')} [{net02.get('net_lift_lo')},{net02.get('net_lift_hi')}]**, "
          f"GO={net02.get('go')}.")
        A(f"- rule: {dn['rule']}")
        dd = v.get("depth_decision", {})
        A(f"- **recommended depth:** {dd.get('recommended_depth')} — {dd.get('rationale')}")
        dc = v.get("detection_comparison", {})
        A(f"- detection: stronger-agent lift {dc.get('stronger_agent_detection_lift')} vs "
          f"Slice-25 cheap-agent {dc.get('slice25_cheap_agent_lift')}.")
    A(f"- **implications:** {v.get('implications','')}")
    A("")
    path.write_text("\n".join(L) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
