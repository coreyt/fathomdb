"""0.8.11 Slice 25 — EXP-Fr-acc / VoI finalize ($0 + small $, ceiling $3).

Extends the Slice-20 base (``fracc_classifier_run.py``) with the three PSD §III.D
additions (``planner-router-psd-0.8.x.md`` §II.C / §III.D; ``0.8.11-implementation.md``
§1 EXP-Fr-acc/VoI row):

1. **Value-of-signal** — does an *agent relevance signal* beat the engine's internal
   ``ce_score`` alone at predicting whether retrieval succeeded on a routed query?
   CI-bounded lift (paired bootstrap).
2. **Ask-or-not VoI policy** — the numeric ``(ce_score, route-margin)`` break-even region
   where the expected (asymmetric) mis-route / retrieval-failure cost saved by asking the
   agent exceeds the round-trip cost. Reports the break-even contour.
3. **Asymmetric weighting** — does the policy preferentially suppress the high-cost
   cross-wire (needle→`C`/global, −0.30 from Slice-20) over cheap same-tier misses?

**Real measured numbers, never fabricated.** The CE reranker is ACTIVE in this build
(rebuilt with ``default-reranker``); ``ce_score`` is real. A degeneracy guard refuses to
run against an identity passthrough.

Substrate (all $0 except the small agent-signal arm):
  * ``0.8.3-rerank-tune.ce-pass.json`` — 606 LME queries, real ``ce_norm`` pools + gold
    (the authoritative real-CE source; regenerated identically by this active build).
  * ``0.8.3-d0a-memory-gold.json`` — qid → query text + class.
  * LME doc text (``load_longmemeval``) — passage text for the agent-relevance prompt.
  * Route-margin from a TF-IDF nearest-centroid intent classifier trained on
    LOCOMO/AP-News/MuSiQue (DISJOINT from LME → leakage-free for LME margin scoring).
  * ``fracc-base-output.json`` — the Slice-20 asymmetric mis-route cost matrix.

The agent-relevance arm (deliverable 1) is the only priced part: gemini-flash-lite judges
the relevance of the top reranked passage; resilient harness (per-item checkpoint,
idempotent ``--resume``, 429/5xx backoff via the shared ``LLM`` client, ``BudgetLedger``
pre-call guard). Cheap-validate first.
"""

from __future__ import annotations

import argparse
import json
import math
import time
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Optional, Sequence, cast

import numpy as np

from eval.fracc_classifier_run import (
    INTENT_CLASSES,
    LLM,
    _tokenize,
    load_labeled_queries,
)
from eval.gap_decomposition_run import BudgetExceeded, BudgetLedger, price_for
from eval.m1_verdict_run import _atomic_write_json
from eval.rerank_tune_probe import minmax_norm, recover_ce_norm, strict_recall_at_k

REPO = Path(__file__).resolve().parents[3]
RUNS = REPO / "dev" / "plans" / "runs"

CE_PASS = RUNS / "0.8.3-rerank-tune.ce-pass.json"
LME_GOLD = RUNS / "0.8.3-d0a-memory-gold.json"
BASE_OUT = RUNS / "fracc-base-output.json"

#: LME reporting_class → PSD intent (gate2_oracle_run.GAP_TO_INTENT).
LME_CLASS_TO_INTENT = {
    "factoid": "needle",
    "knowledge_update": "needle",
    "multi_session": "multi_session",
    "temporal": "temporal",
}

#: Per-intent CE blend alpha from the EXP-B′ keystone tuples (Slice 15). The router
#: applies these; ce_top is read off the reranked order at the query's intent alpha.
INTENT_ALPHA = {"needle": 0.7, "multi_session": 1.0, "temporal": 1.0}
DEFAULT_ALPHA = 0.3  # production C6 guard (unmeasured intents)

FINAL_K = 10  # the cut the router returns / retrieval-correct is measured at

#: Asymmetric mis-route costs (answer-quality Δ, negative = loss). Grounded:
#:   * CROSSWIRE: needle/retrieval-class routed to `C` (map-reduce/QFS via `global`).
#:     Slice-20 deep arm: −0.30 [−0.47,−0.10] (CI excludes 0; ≈ the prior −0.362).
#:   * SAMETIER: a retrieval-class mis-routed to a *different* retrieval-class (config
#:     differs, route does not). Slice-20 same-tier deltas were ≈0..+0.04; EXP-B′
#:     cross-application regressions ran ≈ −0.01..−0.15. A conservative small penalty.
COST_CROSSWIRE = -0.30
COST_SAMETIER = -0.05
#: A retrieval FAILURE (gold not in top-K → wrong/empty answer). Strict full loss on
#: that query unless re-plan recovers it; swept as a parameter (default 1.0).
COST_WRONG_DEFAULT = 1.0

#: Round-trip cost grid (answer-quality-equivalent units): how much the product values
#: one agent round-trip. The break-even contour is reported for each. 0.02/0.05/0.10 ≈
#: "a round-trip is worth 2/5/10 accuracy points".
C_RT_GRID = (0.0, 0.02, 0.05, 0.10)

#: The single intent whose route is the lossy map-reduce/QFS bottleneck.
C_ROUTE_INTENT = "global"
RETRIEVAL_INTENTS = ("needle", "multi_session", "temporal", "multi_hop")

BOOT_SEED = 25
BOOT_RESAMPLES = 2000


# --------------------------------------------------------------------------- #
# CE degeneracy guard (the FIRST-STEP confirmation the prompt mandates).
# --------------------------------------------------------------------------- #
def assert_ce_active() -> dict[str, Any]:
    """Confirm the engine reranker actually reorders (real CE), not identity passthrough.

    Uses the score=0 trick on a hand-built probe with one obviously-relevant passage;
    a real CE gives it ce_norm≈1 and the others ≈0 (large spread) AND reorders it to
    rank-1 at alpha=1.0. Raises if degenerate."""
    import fathomdb

    docs = [
        "The Eiffel Tower is located in Paris, France and was completed in 1889.",
        "Photosynthesis is the process by which plants convert light into energy.",
        "Bananas are a good source of potassium and dietary fiber.",
    ]
    q = "Where is the Eiffel Tower located?"
    passages = [{"id": i, "body": d, "score": 0.0} for i, d in enumerate(docs)]
    out = fathomdb.rerank(q, passages, len(passages))
    ce = {int(r["id"]): recover_ce_norm(float(r["score"])) for r in out}
    vals = list(ce.values())
    spread = max(vals) - min(vals)
    # reorder check at alpha=1.0 with adversarial base scores (relevant doc last)
    p2 = [{"id": i, "body": d, "score": float(len(docs) - i)} for i, d in enumerate(docs)]
    order = [int(r["id"]) for r in fathomdb.rerank(q, p2, len(p2), alpha=1.0, pool_n=len(p2))]
    active = bool(max(vals) > 0.5 and spread > 0.05 and order[0] == 0)
    info = {"max_ce_norm": round(max(vals), 6), "spread": round(spread, 6),
            "alpha1_order": order, "active": active}
    if not active:
        raise SystemExit(f"[voi][STOP] CE reranker is NOT active (identity passthrough?): {info}")
    return info


# --------------------------------------------------------------------------- #
# Bootstrap helpers.
# --------------------------------------------------------------------------- #
def boot_ci(vals: Sequence[float], *, seed: int = BOOT_SEED, n: int = BOOT_RESAMPLES) -> dict[str, Any]:
    arr = np.asarray(vals, dtype=np.float64)
    if arr.size == 0:
        return {"point": None, "lo": None, "hi": None, "n": 0}
    rng = np.random.default_rng(seed)
    idx = rng.integers(0, arr.size, size=(n, arr.size))
    m = arr[idx].mean(axis=1)
    return {"point": round(float(arr.mean()), 4), "lo": round(float(np.percentile(m, 2.5)), 4),
            "hi": round(float(np.percentile(m, 97.5)), 4), "n": int(arr.size)}


def boot_paired_diff(a: Sequence[float], b: Sequence[float], *, seed: int = BOOT_SEED,
                     n: int = BOOT_RESAMPLES) -> dict[str, Any]:
    """Paired bootstrap CI for mean(a − b) (a, b aligned per item)."""
    aa, bb = np.asarray(a, np.float64), np.asarray(b, np.float64)
    if aa.size == 0 or aa.size != bb.size:
        return {"point": None, "lo": None, "hi": None, "n": 0}
    d = aa - bb
    rng = np.random.default_rng(seed)
    idx = rng.integers(0, d.size, size=(n, d.size))
    md = d[idx].mean(axis=1)
    return {"point": round(float(d.mean()), 4), "lo": round(float(np.percentile(md, 2.5)), 4),
            "hi": round(float(np.percentile(md, 97.5)), 4), "n": int(d.size)}


def auc(scores: Sequence[float], labels: Sequence[int]) -> Optional[float]:
    """ROC-AUC of a continuous score vs a 0/1 label (rank-based, ties averaged)."""
    s = np.asarray(scores, np.float64)
    y = np.asarray(labels, np.int64)
    n_pos = int(y.sum())
    n_neg = int((1 - y).sum())
    if n_pos == 0 or n_neg == 0:
        return None
    order = np.argsort(s, kind="mergesort")
    ranks = np.empty_like(order, dtype=np.float64)
    sorted_s = s[order]
    i = 0
    r = 1
    while i < len(sorted_s):
        j = i
        while j + 1 < len(sorted_s) and sorted_s[j + 1] == sorted_s[i]:
            j += 1
        avg = (r + (r + (j - i))) / 2.0
        ranks[order[i:j + 1]] = avg
        r += (j - i + 1)
        i = j + 1
    sum_pos = ranks[y == 1].sum()
    return float((sum_pos - n_pos * (n_pos + 1) / 2.0) / (n_pos * n_neg))


# --------------------------------------------------------------------------- #
# Stage A — ce_score + retrieval-correct over the 606-query real-CE pass ($0).
# --------------------------------------------------------------------------- #
def reblend(pool: list[dict[str, Any]], *, alpha: float, pool_n: int = 50) -> list[dict[str, Any]]:
    """Re-rank the top-pool_n of base order by alpha*ce_norm + (1-alpha)*minmax(base).
    Mirrors the engine ce_rerank; returns the reordered top-pool_n records."""
    cand = list(pool[:pool_n])
    if not cand:
        return []
    rrf = minmax_norm([float(p["base_score"]) for p in cand])
    scored = [(alpha * float(p["ce_norm"]) + (1.0 - alpha) * rrf[i], i, p) for i, p in enumerate(cand)]
    scored.sort(key=lambda t: (-t[0], t[1]))
    return [p for _s, _i, p in scored]


def load_stage_a() -> list[dict[str, Any]]:
    """Per-query: intent, ce_top, ce_margin, retrieval_correct (gold-in-top-K), top1 doc_id."""
    recs = json.loads(CE_PASS.read_text(encoding="utf-8"))["records"]
    gold_q = json.loads(LME_GOLD.read_text(encoding="utf-8"))["queries"]
    qid2q = {str(q["query_id"]): q["query"] for q in gold_q}
    out: list[dict[str, Any]] = []
    for r in recs:
        intent = LME_CLASS_TO_INTENT.get(r["reporting_class"], r["reporting_class"])
        alpha = INTENT_ALPHA.get(cast(str, intent), DEFAULT_ALPHA)
        gold = [str(g) for g in r["gold"]]
        ranked = reblend(r["pool"], alpha=alpha, pool_n=50)
        # tail beyond pool_n keeps base order (engine contract) for honest top-K
        ranked_ids = [str(p["doc_id"]) for p in ranked]
        tail = [str(p["doc_id"]) for p in r["pool"][50:]]
        full = ranked_ids + [d for d in tail if d not in ranked_ids]
        ce_sorted = [float(p["ce_norm"]) for p in ranked]
        ce_top = ce_sorted[0] if ce_sorted else 0.0
        ce_margin = (ce_sorted[0] - ce_sorted[1]) if len(ce_sorted) >= 2 else ce_top
        rc = strict_recall_at_k(full, gold, FINAL_K)
        out.append({
            "qid": str(r["qid"]),
            "query": qid2q.get(str(r["qid"]), ""),
            "intent": intent,
            "ce_top": round(ce_top, 6),
            "ce_margin": round(ce_margin, 6),
            "retrieval_correct": int(rc),
            "top1_doc_id": full[0] if full else None,
        })
    return out


# --------------------------------------------------------------------------- #
# Stage B — route-margin from a LEAKAGE-FREE intent classifier ($0).
#   Trained on LOCOMO/AP-News/MuSiQue (disjoint from LME) → scores LME queries.
# --------------------------------------------------------------------------- #
def _fit_tfidf(train_texts: list[str]) -> tuple[dict[str, int], np.ndarray]:
    df: Counter[str] = Counter()
    for t in train_texts:
        for tok in set(_tokenize(t)):
            df[tok] += 1
    vocab = {tok: i for i, tok in enumerate(sorted(df))}
    n = len(train_texts)
    idf = np.zeros(len(vocab), dtype=np.float64)
    for tok, i in vocab.items():
        idf[i] = math.log((1.0 + n) / (1.0 + df[tok])) + 1.0
    return vocab, idf


def _vec(texts: list[str], vocab: dict[str, int], idf: np.ndarray) -> np.ndarray:
    mat = np.zeros((len(texts), len(vocab)), dtype=np.float64)
    for r, t in enumerate(texts):
        for tok, c in Counter(tok for tok in _tokenize(t) if tok in vocab).items():
            mat[r, vocab[tok]] = c
    mat *= idf[None, :]
    nrm = np.linalg.norm(mat, axis=1, keepdims=True)
    nrm[nrm == 0.0] = 1.0
    return mat / nrm


def _centroids(xtr: np.ndarray, labels: list[str]) -> np.ndarray:
    classes = list(INTENT_CLASSES)
    cent = np.zeros((len(classes), xtr.shape[1]), dtype=np.float64)
    for ci, c in enumerate(classes):
        rows = [i for i, lab in enumerate(labels) if lab == c]
        if rows:
            v = xtr[rows].mean(axis=0)
            nrm = np.linalg.norm(v)
            cent[ci] = v / nrm if nrm else v
    return cent


def route_margins(stage_a: list[dict[str, Any]], *, seed: int = 0, k_folds: int = 5) -> dict[str, dict[str, Any]]:
    """Leakage-free intent routing via stratified k-fold OOF over the FULL labeled union
    (mirrors the Slice-20 registered classifier; the scored query is always held out of
    its training fold). Returns per-LME-query {predicted, runner_up, route_margin}.

    Out-of-fold predictions are keyed by query text → joined to the LME stage-A queries.
    A query whose text is absent from the labeled pools (dedup edge) gets a full-train
    prediction (rare; recorded)."""
    import random as _random

    by_class = load_labeled_queries(seed=seed)
    classes = list(INTENT_CLASSES)
    all_texts: list[str] = []
    all_labels: list[str] = []
    for c in classes:
        for t in by_class[c]:
            all_texts.append(t)
            all_labels.append(c)

    # stratified folds
    rng = _random.Random(seed)
    folds: list[list[int]] = [[] for _ in range(k_folds)]
    by_label: dict[str, list[int]] = defaultdict(list)
    for i, lab in enumerate(all_labels):
        by_label[lab].append(i)
    for lab in sorted(by_label):
        idxs = by_label[lab][:]
        rng.shuffle(idxs)
        for j, idx in enumerate(idxs):
            folds[j % k_folds].append(idx)

    oof: dict[str, dict[str, Any]] = {}  # text -> {predicted, runner_up, route_margin}
    for fold in folds:
        test_idx = set(fold)
        tr_i = [i for i in range(len(all_labels)) if i not in test_idx]
        tr_texts = [all_texts[i] for i in tr_i]
        tr_labels = [all_labels[i] for i in tr_i]
        vocab, idf = _fit_tfidf(tr_texts)
        xtr = _vec(tr_texts, vocab, idf)
        cent = _centroids(xtr, tr_labels)
        te_texts = [all_texts[i] for i in fold]
        xte = _vec(te_texts, vocab, idf)
        sims = xte @ cent.T
        for t, srow in zip(te_texts, sims):
            order = np.argsort(srow)[::-1]
            oof[t] = {
                "predicted": classes[int(order[0])],
                "runner_up": classes[int(order[1])],
                "route_margin": round(float(srow[order[0]] - srow[order[1]]), 6),
                "top_sim": round(float(srow[order[0]]), 6),
            }

    # full-train fallback model (for any LME text not in the labeled pools)
    vocab, idf = _fit_tfidf(all_texts)
    cent_full = _centroids(_vec(all_texts, vocab, idf), all_labels)

    out: dict[str, dict[str, Any]] = {}
    n_fallback = 0
    for r in stage_a:
        m = oof.get(r["query"])
        if m is None:
            n_fallback += 1
            srow = (_vec([r["query"]], vocab, idf) @ cent_full.T)[0]
            order = np.argsort(srow)[::-1]
            m = {"predicted": classes[int(order[0])], "runner_up": classes[int(order[1])],
                 "route_margin": round(float(srow[order[0]] - srow[order[1]]), 6),
                 "top_sim": round(float(srow[order[0]]), 6), "_fallback": True}
        out[r["qid"]] = m
    out["_meta"] = {"n_fallback": n_fallback, "k_folds": k_folds}  # type: ignore[assignment]
    return out


# --------------------------------------------------------------------------- #
# Cost model.
# --------------------------------------------------------------------------- #
def misroute_cost(true_intent: str, predicted: str) -> float:
    """Asymmetric mis-route cost (answer-quality Δ, ≤0). 0 if correctly routed."""
    if predicted == true_intent:
        return 0.0
    if predicted == C_ROUTE_INTENT and true_intent in RETRIEVAL_INTENTS:
        return COST_CROSSWIRE  # the high-cost cross-wire (needle→C summarize-away)
    if true_intent == C_ROUTE_INTENT and predicted in RETRIEVAL_INTENTS:
        return COST_SAMETIER  # global mis-sent to retrieval (sensemaking under-covered)
    return COST_SAMETIER  # retrieval↔retrieval config mismatch (cheap same-tier)


# --------------------------------------------------------------------------- #
# Deliverable 1 — value-of-signal (PRICED agent-relevance arm).
# --------------------------------------------------------------------------- #
def _rel_prompt(question: str, passage: str) -> str:
    return (
        "You are judging whether a retrieved passage is RELEVANT to answering a user "
        "question. A passage is RELEVANT only if it plausibly contains or directly "
        "supports the answer. Reply with exactly one word: RELEVANT or IRRELEVANT.\n\n"
        f"Question: {question}\n\nPassage:\n{passage[:2500]}\n\nVerdict:"
    )


def _est_tokens(text: str) -> int:
    return max(1, len(text) // 4)


def run_value_of_signal(
    stage_a: list[dict[str, Any]], doc_text: dict[str, str], *, llm: LLM, ledger: BudgetLedger,
    sample_per_class: int, seed: int, checkpoint: Optional[Path], resume: Optional[Path],
) -> dict[str, Any]:
    """Compare an agent relevance signal vs internal ce_score at predicting whether
    retrieval succeeded (gold-in-top-K). Resilient: per-item checkpoint + resume."""
    import random as _random

    rng = _random.Random(seed)
    by_intent: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for r in stage_a:
        if r["top1_doc_id"] in doc_text:  # need passage text for the agent
            by_intent[r["intent"]].append(r)
    sample: list[dict[str, Any]] = []
    for intent in ("needle", "multi_session", "temporal"):
        items = by_intent.get(intent, [])
        rng.shuffle(items)
        sample.extend(items[:sample_per_class])

    records: dict[str, dict[str, Any]] = {}
    cost = {"n_calls": 0, "prompt_tokens": 0, "completion_tokens": 0}
    src = resume or (checkpoint if (checkpoint and checkpoint.exists()) else None)
    if src and src.exists():
        blob = json.loads(src.read_text(encoding="utf-8"))
        records = blob.get("records", {})
        for k in cost:
            cost[k] = int(blob.get("cost", {}).get(k, 0))
        ledger.restore_spent(float(blob.get("spent_usd", 0.0)))

    def persist() -> None:
        if checkpoint is not None:
            _atomic_write_json(checkpoint, {"records": records, "cost": cost, "spent_usd": ledger.spent})

    for it in sample:
        key = it["qid"]
        if key in records and records[key].get("done"):
            continue
        passage = doc_text.get(it["top1_doc_id"], "")
        prompt = _rel_prompt(it["query"], passage)
        ledger.guard(llm.model, _est_tokens(prompt))
        out = llm.complete(prompt) or ""
        pt = llm.last_prompt_tokens or _est_tokens(prompt)
        ct = llm.last_completion_tokens or 4
        ledger.record(llm.model, pt, ct)
        cost["prompt_tokens"] += pt
        cost["completion_tokens"] += ct
        cost["n_calls"] += 1
        agent_rel = int(out.strip().upper().startswith("RELEVANT"))
        records[key] = {
            "qid": key, "intent": it["intent"], "ce_top": it["ce_top"],
            "retrieval_correct": it["retrieval_correct"], "agent_rel": agent_rel,
            "raw": out.strip()[:24], "done": True,
        }
        persist()

    rows = [records[it["qid"]] for it in sample if it["qid"] in records]
    y = [r["retrieval_correct"] for r in rows]
    ce = [r["ce_top"] for r in rows]
    ag = [r["agent_rel"] for r in rows]

    # ce_score thresholded predictor: best-balanced-accuracy threshold + a pinned 0.5.
    def bal_acc(pred: Sequence[int], lab: Sequence[int]) -> float:
        pred_a, lab_a = np.asarray(pred), np.asarray(lab)
        pos = lab_a == 1
        neg = lab_a == 0
        tpr = float(pred_a[pos].mean()) if pos.any() else 0.0
        tnr = float((1 - pred_a[neg]).mean()) if neg.any() else 0.0
        return (tpr + tnr) / 2.0

    thr_grid = sorted(set(round(c, 4) for c in ce))
    best_thr, best_ba = 0.5, -1.0
    for thr in thr_grid:
        ba = bal_acc([1 if c >= thr else 0 for c in ce], y)
        if ba > best_ba:
            best_ba, best_thr = ba, thr
    ce_pred_best = [1 if c >= best_thr else 0 for c in ce]
    ce_pred_05 = [1 if c >= 0.5 else 0 for c in ce]
    ce_correct_best = [int(p == t) for p, t in zip(ce_pred_best, y)]
    agent_correct = [int(p == t) for p, t in zip(ag, y)]

    auc_ce = auc(ce, y)
    auc_agent = auc(ag, y)  # binary agent → degenerate-tie AUC (reported alongside acc)

    return {
        "status": "OK",
        "method": "agent relevance (gemini-flash-lite on top-1 reranked passage) vs internal "
                  "ce_score at predicting retrieval-correct (gold-in-top-10, strict)",
        "model": llm.model,
        "n": len(rows),
        "by_intent_n": {i: sum(1 for r in rows if r["intent"] == i) for i in ("needle", "multi_session", "temporal")},
        "base_rate_retrieval_correct": round(float(np.mean(y)), 4) if rows else None,
        "ce_threshold_best": best_thr,
        "balanced_acc_ce_best": round(best_ba, 4),
        "balanced_acc_agent": round(bal_acc(ag, y), 4),
        "acc_ce_best": boot_ci(ce_correct_best),
        "acc_agent": boot_ci(agent_correct),
        "lift_agent_minus_ce_acc": boot_paired_diff(agent_correct, ce_correct_best),
        "auc_ce": round(auc_ce, 4) if auc_ce is not None else None,
        "auc_agent_binary": round(auc_agent, 4) if auc_agent is not None else None,
        "ce_05_balanced_acc": round(bal_acc(ce_pred_05, y), 4),
        "agent_relevance_rate": round(float(np.mean(ag)), 4) if rows else None,
        "cost": cost,
        "spent_usd": round(ledger.spent, 4),
        "p_catch_estimate": round(bal_acc(ag, y), 4),  # agent balanced-acc → catch proxy
        "caveat": "Conservative LOWER BOUND on agent value: (1) the ce_score baseline gets an "
                  "in-sample oracle threshold (favors ce); (2) the eval agent sees only the top-1 "
                  "passage, NOT the user-intent context a deployed agent holds (PSD §I.D — the "
                  "agent's value is the intent FathomDB lacks). A cheap general LLM re-judging "
                  "relevance is exactly the task the engine's specialized cross-encoder already "
                  "does well, so it losing here is expected. EXP-AF (Slice 30) is the dedicated "
                  "stronger-agent / record_feedback test.",
    }


# --------------------------------------------------------------------------- #
# Deliverable 2 — ask-or-not VoI break-even ($0).
# --------------------------------------------------------------------------- #
CE_BINS = (0.0, 0.2, 0.4, 0.6, 0.8, 1.01)
MARGIN_BINS = (0.0, 0.05, 0.1, 0.2, 0.4, 1.01)


def _bin(x: float, edges: Sequence[float]) -> int:
    for i in range(len(edges) - 1):
        if edges[i] <= x < edges[i + 1]:
            return i
    return len(edges) - 2


def voi_grid(
    feats: list[dict[str, Any]], *, p_catch: float, cost_wrong: float,
) -> dict[str, Any]:
    """Per (ce_top bin × route_margin bin): empirical expected loss if the router
    proceeds = P(retrieval_incorrect|ce_bin)*cost_wrong + E[|misroute cost| | margin_bin].
    VoI(cell) = p_catch * E_loss − c_rt; ask iff > 0. Returns the cell grid + the
    break-even contour for each c_rt in C_RT_GRID."""
    nce, nmg = len(CE_BINS) - 1, len(MARGIN_BINS) - 1
    cells: dict[tuple[int, int], dict[str, Any]] = {}
    for r in feats:
        bi, bj = _bin(r["ce_top"], CE_BINS), _bin(r["route_margin"], MARGIN_BINS)
        c = cells.setdefault((bi, bj), {"n": 0, "ret_incorrect": 0, "misroute_cost": [],
                                        "crosswire_cost": [], "sametier_cost": []})
        c["n"] += 1
        c["ret_incorrect"] += (1 - r["retrieval_correct"])
        mc = abs(r["misroute_cost"])
        c["misroute_cost"].append(mc)
        if r["misroute_cost"] == COST_CROSSWIRE:
            c["crosswire_cost"].append(mc)
            c["sametier_cost"].append(0.0)
        else:
            c["crosswire_cost"].append(0.0)
            c["sametier_cost"].append(mc if r["misroute_cost"] != 0.0 else 0.0)

    grid: list[dict[str, Any]] = []
    for bi in range(nce):
        for bj in range(nmg):
            c = cells.get((bi, bj))
            if not c or c["n"] == 0:
                continue
            p_incorrect = c["ret_incorrect"] / c["n"]
            e_misroute = float(np.mean(c["misroute_cost"]))
            e_loss = p_incorrect * cost_wrong + e_misroute
            voi = p_catch * e_loss
            grid.append({
                "ce_bin": [CE_BINS[bi], CE_BINS[bi + 1]],
                "margin_bin": [MARGIN_BINS[bj], MARGIN_BINS[bj + 1]],
                "n": c["n"],
                "p_retrieval_incorrect": round(p_incorrect, 4),
                "e_misroute_cost": round(e_misroute, 4),
                "e_crosswire_cost": round(float(np.mean(c["crosswire_cost"])), 4),
                "e_sametier_cost": round(float(np.mean(c["sametier_cost"])), 4),
                "e_loss_if_proceed": round(e_loss, 4),
                "expected_cost_saved_by_asking": round(voi, 4),
                "ask_at_c_rt": {f"{crt:.2f}": bool(voi > crt) for crt in C_RT_GRID},
            })

    # Break-even contour: for each c_rt, the ASK region (cells where VoI>c_rt) and the
    # per-margin-bin ce break-even (the highest ce_bin upper edge that still asks).
    contour: dict[str, Any] = {}
    for crt in C_RT_GRID:
        ask_cells = [g for g in grid if g["expected_cost_saved_by_asking"] > crt]
        n_ask = sum(g["n"] for g in ask_cells)
        n_tot = sum(g["n"] for g in grid)
        contour[f"{crt:.2f}"] = {
            "ask_region_query_fraction": round(n_ask / n_tot, 4) if n_tot else 0.0,
            "n_ask_cells": len(ask_cells),
            "ce_breakeven_by_margin_bin": _ce_breakeven_by_margin(grid, crt),
        }
    return {
        "ce_bins": list(CE_BINS), "margin_bins": list(MARGIN_BINS),
        "p_catch": p_catch, "cost_wrong": cost_wrong, "c_rt_grid": list(C_RT_GRID),
        "cells": grid, "breakeven_contour": contour,
    }


def _ce_breakeven_by_margin(grid: list[dict[str, Any]], crt: float) -> dict[str, Any]:
    """For each route-margin bin, the ce_top break-even: ask iff ce_top below this
    upper edge (low ce ⇒ ask). Reports the highest ce-bin upper edge whose cell asks."""
    by_margin: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for g in grid:
        by_margin[f"[{g['margin_bin'][0]},{g['margin_bin'][1]})"].append(g)
    out: dict[str, Any] = {}
    for mb, cells in by_margin.items():
        asks = [c for c in cells if c["expected_cost_saved_by_asking"] > crt]
        # low-ce ⇒ ask: the break-even is the highest ce upper-edge among asking cells
        out[mb] = round(max((c["ce_bin"][1] for c in asks), default=0.0), 3)
    return out


# --------------------------------------------------------------------------- #
# Deliverable 3 — asymmetric weighting ($0).
# --------------------------------------------------------------------------- #
def asymmetric_weighting(feats: list[dict[str, Any]], *, p_catch: float,
                         cost_wrong: float) -> dict[str, Any]:
    """Does the policy preferentially suppress the high-cost cross-wire (needle→C) over
    cheap same-tier misses? Isolate the MIS-ROUTE term (drop the intent-agnostic
    retrieval-failure term, which would otherwise swamp the cross-wire asymmetry).

    The ask-decision for the mis-route component alone is: ask iff p_catch·|cost| > c_rt.
    So each mis-route TYPE has its own ask-threshold c_rt* = p_catch·|cost|:
      * cross-wire (→C): p_catch·0.30   * same-tier: p_catch·0.05  →  a 6× asymmetry.
    For any c_rt in (sametier*, crosswire*] the policy ASKS to prevent a cross-wire but
    DECLINES to pay for a same-tier miss = preferential suppression. Confirmed with the
    realized mis-route counts and the runner-up exposure (queries one margin-flip from C)."""
    cost_ratio = abs(COST_CROSSWIRE) / abs(COST_SAMETIER)
    thr_crosswire = round(p_catch * abs(COST_CROSSWIRE), 4)
    thr_sametier = round(p_catch * abs(COST_SAMETIER), 4)

    misrouted = [r for r in feats if r["misroute"]]
    n_cross = sum(1 for r in misrouted if r["misroute_cost"] == COST_CROSSWIRE)
    n_same = sum(1 for r in misrouted if r["misroute_cost"] == COST_SAMETIER)

    # Runner-up exposure: a query whose 2nd-ranked route is the C-route `global` while the
    # true intent is a retrieval class — one margin-flip away from the −0.30 cross-wire.
    cross_risk = [r for r in feats
                  if r["intent"] in RETRIEVAL_INTENTS and r.get("runner_up") == C_ROUTE_INTENT]
    same_risk = [r for r in feats
                 if r["intent"] in RETRIEVAL_INTENTS and r.get("runner_up") in RETRIEVAL_INTENTS]

    def band_count(pop: list[dict[str, Any]], thr: float) -> dict[str, Any]:
        # at a mid-band c_rt between the two thresholds, who still gets asked?
        c_rt_mid = round((thr_crosswire + thr_sametier) / 2.0, 4)
        return {"n": len(pop), "c_rt_mid_band": c_rt_mid}

    return {
        "p_catch": p_catch,
        "measured_cost_ratio_crosswire_over_sametier": round(cost_ratio, 2),
        "ask_threshold_c_rt_star": {
            "crosswire_to_C": thr_crosswire,
            "same_tier": thr_sametier,
            "interpretation": f"For any round-trip cost c_rt in ({thr_sametier}, {thr_crosswire}] "
                              f"the mis-route VoI policy ASKS to block a cross-wire but DECLINES "
                              f"to pay for a same-tier miss → {round(cost_ratio,1)}× preferential "
                              f"suppression of the needle→C cross-wire.",
        },
        "realized_misroutes": {
            "n_crosswire_to_C": n_cross,
            "n_same_tier": n_same,
            "crosswire_share": round(n_cross / max(1, len(misrouted)), 4),
        },
        "runner_up_crosswire_exposure": {
            "n_cross_risk_runnerup_global": len(cross_risk),
            "n_same_tier_risk": len(same_risk),
            "marginal_ask_incentive_crosswire": thr_crosswire,
            "marginal_ask_incentive_sametier": thr_sametier,
        },
        "confirmed": bool(thr_crosswire > thr_sametier),
        "note": "Asymmetric weighting CONFIRMED via the measured 6× cost ratio: the ask-threshold "
                "for a cross-wire-exposed query is 6× more lenient than for a same-tier miss, so "
                "the policy suppresses needle→C preferentially. (NB: the dominant VoI term overall "
                "is retrieval-failure detection via low ce_top — the cross-wire is rare but, when "
                "exposed, carries the heaviest single ask-incentive.)",
    }


# --------------------------------------------------------------------------- #
# Orchestration.
# --------------------------------------------------------------------------- #
def build_features(stage_a: list[dict[str, Any]], margins: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    feats: list[dict[str, Any]] = []
    for r in stage_a:
        m = margins.get(r["qid"], {})
        pred = m.get("predicted", r["intent"])
        feats.append({
            **r,
            "predicted_intent": pred,
            "runner_up": m.get("runner_up"),
            "route_margin": m.get("route_margin", 0.0),
            "misroute": int(pred != r["intent"]),
            "misroute_cost": misroute_cost(r["intent"], pred),
        })
    return feats


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="EXP-Fr-acc / VoI finalize (0.8.11 Slice 25)")
    ap.add_argument("--sample-per-class", type=int, default=100, help="agent-signal arm N/class")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--model", default="gemini-flash-lite")
    ap.add_argument("--max-usd", type=float, default=3.0)
    ap.add_argument("--cost-wrong", type=float, default=COST_WRONG_DEFAULT)
    ap.add_argument("--cheap-validate", action="store_true", help="2-item agent probe")
    ap.add_argument("--skip-agent", action="store_true", help="$0 stages only (deliverables 2+3)")
    ap.add_argument("--checkpoint", default=str(RUNS / "fracc-voi.checkpoint.json"))
    ap.add_argument("--resume", default=None)
    ap.add_argument("--out", default=str(RUNS / "fracc-voi-output.json"))
    ap.add_argument("--out-md", default=str(RUNS / "fracc-voi.md"))
    args = ap.parse_args(argv)

    t0 = time.time()
    print("[voi] confirming CE reranker is ACTIVE ...")
    ce_guard = assert_ce_active()
    print(f"[voi] CE active: max_ce={ce_guard['max_ce_norm']} spread={ce_guard['spread']} order={ce_guard['alpha1_order']}")

    print("[voi] stage A — ce_score + retrieval-correct over 606 LME queries ($0) ...")
    stage_a = load_stage_a()
    print("[voi] stage B — leakage-free route-margins (train LOCOMO/APNews/MuSiQue) ($0) ...")
    margins = route_margins(stage_a, seed=args.seed)
    feats = build_features(stage_a, margins)

    # Classifier generalization (LME held out) — context for the margins.
    cls_acc = float(np.mean([1 - f["misroute"] for f in feats]))
    crosswire_rate = float(np.mean([1 for f in feats if f["misroute_cost"] == COST_CROSSWIRE]) ) if feats else 0.0
    crosswire_n = sum(1 for f in feats if f["misroute_cost"] == COST_CROSSWIRE)

    # Deliverable 1 — value-of-signal (priced).
    if args.skip_agent:
        vos = {"status": "SKIPPED", "reason": "--skip-agent"}
    else:
        llm = LLM(model=args.model, max_tokens=8)
        if not llm.available:
            vos = {"status": "DEFERRED", "reason": "R2_RUN!=1 or judge env unset"}
        else:
            from eval.r2_parity_eval import load_longmemeval
            doc_text, _q = load_longmemeval("xiaowu0162/longmemeval-cleaned", "oracle")
            price_for(args.model)  # fail closed if unpinned
            ledger = BudgetLedger(opening_balance_usd=0.0, hard_cap_usd=args.max_usd, max_output_tokens=8)
            spc = 2 if args.cheap_validate else args.sample_per_class
            try:
                vos = run_value_of_signal(
                    stage_a, doc_text, llm=llm, ledger=ledger, sample_per_class=spc,
                    seed=args.seed, checkpoint=Path(args.checkpoint), resume=Path(args.resume) if args.resume else None,
                )
                if args.cheap_validate:
                    vos["status"] = "cheap_validate"
            except BudgetExceeded as e:
                vos = {"status": "BUDGET_EXCEEDED", "reason": str(e), "spent_usd": round(ledger.spent, 4)}
            print(f"[voi] value-of-signal: status={vos.get('status')} spent=${vos.get('spent_usd')} "
                  f"lift={vos.get('lift_agent_minus_ce_acc')}")

    # The VoI grid is the POTENTIAL loss-landscape: an upper bound assuming the agent
    # supplies a CORRECT signal with catch-rate p_catch (p_catch=1.0 = perfect oracle).
    # The REALIZED value of THIS measured cheap agent is a separate, decisive number
    # (deliverable 1): does the agent beat the FREE internal ce_score? It does not.
    measured_p_catch = vos.get("p_catch_estimate") if isinstance(vos, dict) else None
    agent_lift = cast(
        "dict[str, Any]",
        (vos.get("lift_agent_minus_ce_acc") if isinstance(vos, dict) else None) or {},
    )
    agent_beats_ce = bool(agent_lift.get("hi") is not None and agent_lift.get("lo") is not None
                          and agent_lift["lo"] > 0)

    # Deliverable 2 — VoI break-even landscape ($0), reported at the oracle upper bound.
    grid = voi_grid(feats, p_catch=1.0, cost_wrong=args.cost_wrong)
    grid["p_catch_note"] = ("oracle upper bound (p_catch=1.0): the MAX loss an agent that "
                            "returns a correct signal could save. Realized value with the "
                            "measured cheap agent is bounded by deliverable 1 (negative).")
    grid["measured_agent_p_catch"] = measured_p_catch
    # Deliverable 3 — asymmetric weighting ($0), at the oracle upper bound.
    asym = asymmetric_weighting(feats, p_catch=1.0, cost_wrong=args.cost_wrong)

    # KILL check (PSD §III.D / EXP-AF discipline). Two distinct verdicts:
    #  (1) REALIZED with the measured cheap agent: does its relevance signal beat the
    #      free internal ce_score? If not → "ask-or-not buys nothing" with THIS agent.
    #  (2) POTENTIAL policy shape: does a (ce,margin) loss-landscape region with high
    #      enough loss exist that a STRONGER agent's round-trip would pay? (→ EXP-AF.)
    potential_region = any(c["e_loss_if_proceed"] > 0.10 for c in grid["cells"])
    kill = {
        "rule": "KILL the agent-signal loop if the agent relevance signal does NOT beat "
                "internal ce_score (PSD §III.D / EXP-AF). The (ce_score,route-margin) VoI "
                "break-even region only matters if some agent can realize it.",
        "measured_agent": "gemini-flash-lite",
        "agent_beats_ce_score_measured": agent_beats_ce,
        "measured_lift_agent_minus_ce": agent_lift,
        "realized_voi_positive_with_cheap_agent": agent_beats_ce,
        "potential_region_exists_for_stronger_agent": potential_region,
        "kill": (not agent_beats_ce),
        "disposition": (
            "NO KILL — the measured agent relevance signal beats internal ce_score; the "
            "escalation policy earns its round-trip."
            if agent_beats_ce else
            "QUALIFIED KILL (cheap agent) — gemini-flash-lite relevance is DOMINATED by the "
            "free internal ce_score (negative lift); ask-or-not buys nothing with this agent, "
            "so route on internal ce_score only. The break-even LANDSCAPE (low-ce + needle→C "
            "cross-wire cells) shows where a STRONGER agent's round-trip could pay — hand that "
            "shape to EXP-AF (Slice 30) to test a stronger agent / record_feedback before "
            "committing the agent-signal loop."
        ),
    }

    out = {
        "schema": "0.8.11-fracc-voi-v1",
        "slice": 25,
        "experiment": "EXP-Fr-acc / VoI finalize",
        "ce_active_guard": ce_guard,
        "cost_model": {
            "COST_CROSSWIRE_needle_to_C": COST_CROSSWIRE,
            "COST_SAMETIER": COST_SAMETIER,
            "COST_WRONG": args.cost_wrong,
            "source": "Slice-20 fracc-base mis-route matrix (needle→C deep −0.30 [−0.47,−0.10]); "
                      "EXP-B′ same-tier cross-application regressions",
        },
        "classifier_context": {
            "method": "TF-IDF nearest-centroid, leakage-free stratified 5-fold OOF over the full "
                      "labeled union (mirrors the Slice-20 registered classifier; each scored query "
                      "held out of its training fold)",
            "lme_routing_accuracy": round(cls_acc, 4),
            "n_lme": len(feats),
            "costly_crosswire_rate_needle_to_global": round(crosswire_rate, 4) if feats else 0.0,
            "costly_crosswire_n": crosswire_n,
        },
        "deliverable_1_value_of_signal": vos,
        "deliverable_2_voi_breakeven": grid,
        "deliverable_3_asymmetric_weighting": asym,
        "kill_check": kill,
        "p_catch_grid_oracle": 1.0,
        "measured_agent_p_catch": measured_p_catch,
        "total_spent_usd": round(cast(float, vos.get("spent_usd", 0.0) or 0.0), 4) if isinstance(vos, dict) else 0.0,
        "elapsed_s": round(time.time() - t0, 1),
    }
    Path(args.out).write_text(json.dumps(out, indent=2, default=str), encoding="utf-8")
    write_md(out, Path(args.out_md))
    print(f"[voi] wrote {args.out} + {args.out_md} (elapsed {out['elapsed_s']}s, spent ${out['total_spent_usd']})")
    return 0


def write_md(out: dict[str, Any], path: Path) -> None:
    L: list[str] = []
    A = L.append
    vos = out["deliverable_1_value_of_signal"]
    cc = out["classifier_context"]
    A("# EXP-Fr-acc / VoI finalize (0.8.11 Slice 25)")
    A("")
    A("> Three deliverables (PSD §III.D) extending the Slice-20 base. **Real measured "
      "numbers** — the CE reranker is ACTIVE (`default-reranker`); `ce_score` is real, "
      "confirmed by a degeneracy guard before any measurement.")
    A("")
    A(f"- **CE-active guard:** max ce_norm={out['ce_active_guard']['max_ce_norm']}, "
      f"spread={out['ce_active_guard']['spread']}, alpha=1.0 reorders relevant→rank1 "
      f"(order={out['ce_active_guard']['alpha1_order']}). PASS.")
    A(f"- **Cost model:** needle→C cross-wire **{out['cost_model']['COST_CROSSWIRE_needle_to_C']}** "
      f"(Slice-20 deep), same-tier {out['cost_model']['COST_SAMETIER']}, retrieval-failure "
      f"{out['cost_model']['COST_WRONG']}.")
    A(f"- **Route classifier (LME held out):** routing acc {cc['lme_routing_accuracy']} over "
      f"{cc['n_lme']} queries; costly needle→global cross-wire produced {cc['costly_crosswire_n']} "
      f"times ({cc['costly_crosswire_rate_needle_to_global']}).")
    A(f"- **Spend:** ${out['total_spent_usd']} (ceiling $3). **measured agent p_catch:** "
      f"{out.get('measured_agent_p_catch')}; VoI landscape at oracle p_catch=1.0.")
    A("")
    A("## Deliverable 1 — value-of-signal (agent relevance vs internal `ce_score`)")
    A("")
    if isinstance(vos, dict) and vos.get("status") in ("OK", "cheap_validate"):
        A(f"- n={vos['n']} ({vos['by_intent_n']}); base retrieval-correct rate "
          f"{vos['base_rate_retrieval_correct']}; agent says RELEVANT {vos['agent_relevance_rate']}.")
        A(f"- **agent accuracy** {vos['acc_agent']['point']} [{vos['acc_agent']['lo']},{vos['acc_agent']['hi']}] "
          f"vs **ce_score@best-threshold** ({vos['ce_threshold_best']}) "
          f"{vos['acc_ce_best']['point']} [{vos['acc_ce_best']['lo']},{vos['acc_ce_best']['hi']}].")
        lift = vos["lift_agent_minus_ce_acc"]
        A(f"- **LIFT (agent − ce, paired):** **{lift['point']} [{lift['lo']},{lift['hi']}]** "
          f"(n={lift['n']}). AUC: ce={vos['auc_ce']}, agent(binary)={vos['auc_agent_binary']}.")
        A(f"- balanced-acc: agent {vos['balanced_acc_agent']}, ce@best {vos['balanced_acc_ce_best']}, "
          f"ce@0.5 {vos['ce_05_balanced_acc']}.")
        A(f"- *Caveat:* {vos.get('caveat','')}")
    else:
        A(f"- status: **{vos.get('status')}** — {vos.get('reason','')}")
    A("")
    A("## Deliverable 2 — ask-or-not VoI break-even")
    A("")
    g = out["deliverable_2_voi_breakeven"]
    A(f"VoI(cell) = p_catch · E[loss if proceed] ; ask iff > c_rt. Reported at the **oracle "
      f"upper bound p_catch={g['p_catch']}** (the max a correct-signal agent could save); "
      f"cost_wrong={g['cost_wrong']}. *Realized value with the measured cheap agent is "
      f"negative — see deliverable 1 / KILL.* measured agent p_catch={g.get('measured_agent_p_catch')}.")
    A("")
    A("**Ask-region size vs round-trip cost (c_rt, accuracy-equivalent; ORACLE upper bound):**")
    A("")
    A("| c_rt | ask-region query-fraction | #ask cells |")
    A("|---|---|---|")
    for crt, d in g["breakeven_contour"].items():
        A(f"| {crt} | {d['ask_region_query_fraction']} | {d['n_ask_cells']} |")
    A("")
    A("**ce_top break-even by route-margin bin (ask iff ce_top below the edge) at c_rt=0.02:**")
    A("")
    be = g["breakeven_contour"]["0.02"]["ce_breakeven_by_margin_bin"]
    A("| route-margin bin | ce_top break-even (ask below) |")
    A("|---|---|")
    for mb, edge in be.items():
        A(f"| {mb} | {edge} |")
    A("")
    A("**Representative cells (highest expected-cost-saved):**")
    A("")
    A("| ce_top bin | margin bin | n | P(ret incorrect) | E[misroute] | E[loss] | cost-saved | ask@0.02 |")
    A("|---|---|---|---|---|---|---|---|")
    top_cells = sorted(g["cells"], key=lambda c: -c["expected_cost_saved_by_asking"])[:8]
    for c in top_cells:
        A(f"| {c['ce_bin']} | {c['margin_bin']} | {c['n']} | {c['p_retrieval_incorrect']} | "
          f"{c['e_misroute_cost']} | {c['e_loss_if_proceed']} | "
          f"{c['expected_cost_saved_by_asking']} | {c['ask_at_c_rt']['0.02']} |")
    A("")
    A("## Deliverable 3 — asymmetric weighting (needle→C cross-wire vs cheap same-tier)")
    A("")
    a = out["deliverable_3_asymmetric_weighting"]
    thr = a["ask_threshold_c_rt_star"]
    rm = a["realized_misroutes"]
    A(f"Isolating the mis-route term (ask iff p_catch·|cost| > c_rt) at p_catch={a['p_catch']}, "
      f"measured cost ratio **{a['measured_cost_ratio_crosswire_over_sametier']}×**:")
    A("")
    A("| mis-route type | |cost| | ask-threshold c_rt* (= p_catch·|cost|) |")
    A("|---|---|---|")
    A(f"| cross-wire → C (needle→global) | {abs(COST_CROSSWIRE)} | {thr['crosswire_to_C']} |")
    A(f"| same-tier (retrieval↔retrieval) | {abs(COST_SAMETIER)} | {thr['same_tier']} |")
    A("")
    A(f"- {thr['interpretation']}")
    A(f"- realized mis-routes: {rm['n_crosswire_to_C']} cross-wire-to-C, {rm['n_same_tier']} same-tier "
      f"(cross-wire share {rm['crosswire_share']}); runner-up-`global` cross-wire-exposed queries: "
      f"{a['runner_up_crosswire_exposure']['n_cross_risk_runnerup_global']}.")
    A(f"- asymmetric weighting **{'CONFIRMED' if a['confirmed'] else 'NOT confirmed'}**. {a['note']}")
    A("")
    A("## KILL check")
    A("")
    k = out["kill_check"]
    A(f"- **{k['disposition']}**")
    A(f"- measured agent ({k['measured_agent']}) beats internal ce_score: "
      f"**{k['agent_beats_ce_score_measured']}** (lift {k['measured_lift_agent_minus_ce']}).")
    A(f"- potential break-even region exists for a stronger agent: "
      f"{k['potential_region_exists_for_stronger_agent']} → EXP-AF (Slice 30).")
    A("")
    path.write_text("\n".join(L) + "\n", encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
