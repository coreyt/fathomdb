"""0.8.4 Tier-1 FAIR re-run — FathomDB vs the preserved Microsoft GraphRAG, confounds removed.

The literal head-to-head (`0.8.4-vs-microsoft-graphrag.py`) found FathomDB losing
sensemaking 0.00-0.33, but with two self-inflicted confounds: the FathomDB reader was
capped at max_tokens=600 (GraphRAG ran to ~6000 chars) and top-K was hardcoded at 8.
This re-runs the SAME comparison on the SAME preserved index with the three Tier-1
fairness levers applied (design `dev/design/0.8.4-closing-graphrag-gap.md` §2):

  1. match generation budget   — FathomDB reader max_tokens 600 -> ~1500 (~6000 chars)
  2. configurable / raised k    — k=8 -> k=N_DOCS (full coverage for a global question)
  3. MMR diversification        — VectorRagAdapter.retrieve_mmr (lever A; moot at k=N,
                                   built for the powered run)

Per HITL 2026-06-23: use gpt-nano when possible, and keep FathomDB's gpt use EQUIVALENT
to GraphRAG's. So the shared *answer/synthesis* model is **gpt-5-nano on BOTH sides**
(GraphRAG global-search query LLM + FathomDB reader, identical max_tokens). GraphRAG keeps
its existing gpt-5.4-built community reports (its index investment; FathomDB's equivalent
index is Tier 2 — intentionally NOT fixed here). Judge = claude-haiku (cross-family; an
openai judge would break the self-preference control).

Resilient: all answers checkpoint to a JSON sidecar (idempotent resume); GraphRAG's side
is cached separately (fixed competitor -> future re-runs reuse it free); --max-usd guard.

Run:  set -a; . ~/projects/airlock/.env; set +a
      FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=. python ../../dev/plans/runs/0.8.4-tier1-fair-rerun.py
Flags: --validate (1 question, no judging, prints lengths + $0-ish smoke)
       --max-usd N (hard ledger ceiling; default 8.0)
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import (
    JUDGE_METRICS,
    assemble_bias_controls,
    assemble_length_corroboration,
    build_pairwise_prompt,
    compute_winrates,
    run_autoe,
)
from eval.baselines_084 import VectorRagAdapter
from eval.decision_rule_084 import HEADLINE_METRICS, decide_084

AIRLOCK_URL = "http://localhost:4000/v1/chat/completions"
AIRLOCK_KEY = os.environ.get("AIRLOCK_MASTER_KEY", "")
GRAG_PY = "/tmp/gtest-venv/bin/python"
GRAG_ROOT = "/tmp/grag"

READER = "gpt-5-nano"      # shared answer model (nano per HITL; equivalent on both sides)
GRAG_QUERY_MODEL = "gpt-5-nano"  # GraphRAG global-search synthesis model (matched to READER)
JUDGE_MODEL = "claude-haiku"     # cross-family judge (self-preference control)

N_DOCS = 15        # the docs Microsoft GraphRAG indexed (fair same corpus)
N_Q = 8
N_RUNS = 5
K = N_DOCS         # Tier-1 lever 2: raised k = full coverage for a global question
MMR_LAMBDA = 0.5   # Tier-1 lever 3
MAX_TOKENS = 1500  # Tier-1 lever 1: matched generation budget (~6000 chars)

# ~gpt-5-nano price (USD per 1M tokens); conservative, for the --max-usd ledger guard.
PRICE_IN_PER_1M = 0.05
PRICE_OUT_PER_1M = 0.40
PRICE_JUDGE_IN_PER_1M = 0.80   # claude-haiku, conservative
PRICE_JUDGE_OUT_PER_1M = 4.00

CKPT = Path(__file__).with_name("0.8.4-tier1-fair-rerun.ckpt.json")

_spent = {"usd": 0.0}


def _meter(usage: dict, *, judge: bool) -> None:
    pin = PRICE_JUDGE_IN_PER_1M if judge else PRICE_IN_PER_1M
    pout = PRICE_JUDGE_OUT_PER_1M if judge else PRICE_OUT_PER_1M
    it = float(usage.get("prompt_tokens", 0) or 0)
    ot = float(usage.get("completion_tokens", 0) or 0)
    _spent["usd"] += it / 1e6 * pin + ot / 1e6 * pout


def _chat(model: str, prompt: str, max_tokens: int, *, judge: bool = False, temp: float = 0.0) -> str:
    body = {"model": model, "messages": [{"role": "user", "content": prompt}],
            "temperature": temp, "max_tokens": max_tokens}
    req = urllib.request.Request(AIRLOCK_URL, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json",
                                          "Authorization": f"Bearer {AIRLOCK_KEY}"})
    with urllib.request.urlopen(req, timeout=240) as r:
        payload = json.loads(r.read())
    _meter(payload.get("usage", {}) or {}, judge=judge)
    return payload["choices"][0]["message"]["content"] or ""


def _guard(max_usd: float) -> None:
    if _spent["usd"] > max_usd:
        _save_ckpt()
        sys.exit(f"--max-usd ${max_usd} exceeded (spent ${_spent['usd']:.3f}); checkpoint saved, resume to continue")


# --- GraphRAG global search (preserved index; query LLM set to nano) ------------ #

def _ensure_grag_nano() -> None:
    """Point GraphRAG's completion model at nano for the query (index already built)."""
    sy = Path(GRAG_ROOT, "settings.yaml")
    txt = sy.read_text()
    if f"model: {GRAG_QUERY_MODEL}" not in txt:
        txt = txt.replace("model: gpt-5.4", f"model: {GRAG_QUERY_MODEL}")
        sy.write_text(txt)


def graphrag_global(question: str) -> str:
    env = dict(os.environ, GRAPHRAG_API_KEY=AIRLOCK_KEY)
    p = subprocess.run([GRAG_PY, "-m", "graphrag", "query", "-r", GRAG_ROOT, "-m", "global", question],
                       capture_output=True, text=True, env=env, timeout=300)
    out = p.stdout
    marker = "Global Search Response:"
    ans = (out.split(marker, 1)[1].strip() if marker in out else out.strip())[:6000]
    if not ans:
        sys.stderr.write(f"[graphrag] empty answer; stderr tail:\n{p.stderr[-800:]}\n")
    return ans


# --- checkpoint ---------------------------------------------------------------- #

def _load_ckpt() -> dict:
    if CKPT.exists():
        return json.loads(CKPT.read_text())
    return {"answers": {"microsoft_graphrag": {}, "fathomdb_vector": {}, "fathomdb_mapreduce": {}},
            "spent_usd": 0.0}


def _save_ckpt() -> None:
    _STATE["spent_usd"] = round(_spent["usd"], 4)
    CKPT.write_text(json.dumps(_STATE, indent=2))


_STATE = _load_ckpt()
_spent["usd"] = float(_STATE.get("spent_usd", 0.0))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--validate", action="store_true", help="1 question, no judging — smoke")
    ap.add_argument("--max-usd", type=float, default=8.0)
    args = ap.parse_args()

    if not AIRLOCK_KEY:
        sys.exit("AIRLOCK_MASTER_KEY not set — `set -a; . ~/projects/airlock/.env; set +a` first")

    _ensure_grag_nano()
    arts = load_articles()[:N_DOCS]
    docs15 = {a.doc_id: a.body for a in arts}
    vec = VectorRagAdapter(docs15)
    qs = [q for q in load_autoq() if q.scope == "global"]
    nq = 1 if args.validate else N_Q
    sample = qs[:: max(1, len(qs) // N_Q)][:nq]
    questions = [(q.question_id if getattr(q, "question_id", None) else f"g{i}", q.question_text)
                 for i, q in enumerate(sample)]

    print(f"Tier-1 FAIR re-run | reader={READER} grag_query={GRAG_QUERY_MODEL} judge={JUDGE_MODEL} "
          f"| k={K} max_tokens={MAX_TOKENS} mmr_lambda={MMR_LAMBDA} | n_q={len(questions)} n_runs={N_RUNS}")

    def fathomdb_vector(q: str) -> str:
        hits = vec.retrieve_mmr(q, K, lambda_=MMR_LAMBDA)
        ctx = "\n\n".join(f"[{i + 1}] {h.body}" for i, h in enumerate(hits))
        return _chat(READER, f"Answer using ONLY the context. Be comprehensive but factual.\n\n"
                             f"Context:\n{ctx}\n\nQuestion: {q}\n\nAnswer:", MAX_TOKENS).strip()

    def fathomdb_mapreduce(q: str) -> str:
        bodies = list(docs15.values())
        partials = []
        for i in range(0, len(bodies), 5):
            chunk = "\n\n".join(f"[{j + 1}] {b}" for j, b in enumerate(bodies[i:i + 5]))
            m = _chat(READER, f"Extract points relevant to the question (or 'NONE').\n\n"
                              f"Question: {q}\n\n{chunk}\n\nPoints:", 400).strip()
            if m and "NONE" not in m[:8].upper():
                partials.append(m)
        return _chat(READER, f"Synthesize a comprehensive global answer.\n\nQuestion: {q}\n\nPoints:\n"
                             + "\n\n".join(partials) + "\n\nAnswer:", MAX_TOKENS).strip()

    ans = _STATE["answers"]
    gens = {"microsoft_graphrag": graphrag_global,
            "fathomdb_vector": fathomdb_vector,
            "fathomdb_mapreduce": fathomdb_mapreduce}
    for qid, qt in questions:
        for arm, fn in gens.items():
            if qid not in ans[arm]:                      # idempotent resume
                ans[arm][qid] = fn(qt)
                _save_ckpt()
                _guard(args.max_usd)
        print(f"  {qid}: msGRAG={len(ans['microsoft_graphrag'][qid])}c "
              f"fdb_vec={len(ans['fathomdb_vector'][qid])}c fdb_mr={len(ans['fathomdb_mapreduce'][qid])}c "
              f"| spent=${_spent['usd']:.3f}")

    if args.validate:
        print(f"\nVALIDATE OK — answers generated, no judging. spent=${_spent['usd']:.3f}")
        return

    class ClaudeJudge:
        def judge_pair(self, q, a, b, metrics):
            return _chat(JUDGE_MODEL, build_pairwise_prompt(q, a, b, metrics), 400, judge=True, temp=0.7)

    bias = assemble_bias_controls(judge_family="anthropic", system_families=["openai"], n_runs=N_RUNS)
    print("\n=== FathomDB (FAIR) vs Microsoft GraphRAG — win-rate >0.5 = FathomDB better ===")
    results = {}
    for fdb in ("fathomdb_mapreduce", "fathomdb_vector"):
        pair = (fdb, "microsoft_graphrag")
        j = run_autoe(ClaudeJudge(), ans, questions, pair, n_runs=N_RUNS, metrics=JUDGE_METRICS)
        _guard(args.max_usd)
        jl = list(j.values())
        wr = compute_winrates(jl, pair, metrics=HEADLINE_METRICS, seed=0)
        length = assemble_length_corroboration(jl, pair, ran=True)
        res = decide_084(wr, bias, length)
        results[fdb] = {"winrates": wr, "verdict": res}
        print(f"\n  {fdb} vs microsoft_graphrag:")
        for m in HEADLINE_METRICS:
            f = wr[m]
            print(f"    {m:18s}: win_rate={f['win_rate']:.3f} CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}] "
                  f"mde={f['mde']:.3f} n={f['n']}")
        print(f"    decide_084: {res['verdict']} binding={res['binding_constraint']} "
              f"surpass={res['surpass_candidates']}")

    _save_ckpt()
    (Path(__file__).with_name("0.8.4-tier1-fair-rerun-RESULT.json")).write_text(
        json.dumps({"config": {"reader": READER, "grag_query_model": GRAG_QUERY_MODEL,
                               "judge": JUDGE_MODEL, "k": K, "max_tokens": MAX_TOKENS,
                               "mmr_lambda": MMR_LAMBDA, "n_q": len(questions), "n_runs": N_RUNS},
                    "results": {a: {m: results[a]["winrates"][m] for m in HEADLINE_METRICS}
                                | {"verdict": results[a]["verdict"]["verdict"],
                                   "binding": results[a]["verdict"]["binding_constraint"]}
                                for a in results},
                    "spent_usd": round(_spent["usd"], 4)}, indent=2))
    print(f"\nTotal spent ${_spent['usd']:.3f}. Result JSON + checkpoint written.")


if __name__ == "__main__":
    main()
