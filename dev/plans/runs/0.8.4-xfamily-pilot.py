"""0.8.4 — CROSS-FAMILY registered-design pilot ($ small): GraphRAG-style arm vs baselines.

Fixes the #1 validity caveat of the prior $0 smokes: the JUDGE is now a real
**cross-family Claude (claude-haiku via the airlock)** — Anthropic ≠ the Qwen answerer
family — with **≥5 runs** (stochasticity, temp 0.7) and **order-swap** (position). So this
clears three of the four `decide_084` bias controls AS A REAL MEASUREMENT, and the verdict
is scored through the frozen rule.

Remaining honest caveats (so this is a registered-DESIGN PILOT, not THE resolution): the
GraphRAG-style arm is a minimal map-reduce QFS over a 40-article SUBSET (not Microsoft
GraphRAG / a full-corpus S1 with offline community summaries); the answerer is the local
Qwen ($0), not gpt-5.4; n is a modest pilot. It answers "does a GraphRAG-style synthesis
reach parity-or-better vs a strong baseline, under a cross-family judge?" — the premise
question — on the real harness.

Run (loads airlock creds):  set -a; . ~/projects/airlock/.env; set +a;
  FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=. python ../../dev/plans/runs/0.8.4-xfamily-pilot.py
"""

from __future__ import annotations

import json
import os
import urllib.request

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import (
    JUDGE_METRICS,
    assemble_bias_controls,
    assemble_length_corroboration,
    compute_winrates,
    run_autoe,
)
from eval.baselines_084 import LongContextAdapter, VectorRagAdapter
from eval.decision_rule_084 import HEADLINE_METRICS, decide_084

QWEN_URL = "http://127.0.0.1:8000/v1/chat/completions"
AIRLOCK_URL = "http://localhost:4000/v1/chat/completions"
AIRLOCK_KEY = os.environ["AIRLOCK_MASTER_KEY"]
JUDGE_MODEL = "claude-haiku"   # cross-family (Anthropic) vs the Qwen answerer
ANSWERER_FAMILY = "qwen"
K = 10
SUBSET = 40
BATCH = 5
N_Q = 8          # global-sensemaking questions
N_RUNS = 5       # stochasticity control (MIN_RUNS); judge temp>0 for genuine run variance


def _chat(url: str, key: str | None, model: str, prompt: str, max_tokens: int, temp: float, qwen: bool) -> str:
    body: dict = {"model": model, "messages": [{"role": "user", "content": prompt}],
                  "temperature": temp, "max_tokens": max_tokens}
    if qwen:
        body["chat_template_kwargs"] = {"enable_thinking": False}
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = f"Bearer {key}"
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=headers)
    with urllib.request.urlopen(req, timeout=240) as resp:
        return json.loads(resp.read())["choices"][0]["message"]["content"] or ""


def qwen(prompt: str, max_tokens: int) -> str:
    return _chat(QWEN_URL, None, "qwen3.6-27b", prompt, max_tokens, 0.0, qwen=True)


def baseline_answer(question: str, hits) -> str:
    ctx = "\n\n".join(f"[{i + 1}] {h.body}" for i, h in enumerate(hits))
    return qwen(f"Answer the question using ONLY the context passages. Be comprehensive but "
               f"factual.\n\nContext:\n{ctx}\n\nQuestion: {question}\n\nAnswer:", 600).strip()


def graphrag_answer(question: str, subset_bodies: list[str]) -> str:
    partials = []
    for i in range(0, len(subset_bodies), BATCH):
        chunk = "\n\n".join(f"[{j + 1}] {b}" for j, b in enumerate(subset_bodies[i : i + BATCH]))
        m = qwen("From these news passages, extract ONLY points relevant to answering the "
                 f"question (or 'NONE').\n\nQuestion: {question}\n\nPassages:\n{chunk}\n\nRelevant points:", 300).strip()
        if m and "NONE" not in m[:8].upper():
            partials.append(m)
    pts = "\n\n".join(f"- {p}" for p in partials)
    return qwen("Synthesize a comprehensive, global answer to the question from these extracted "
                f"points.\n\nQuestion: {question}\n\nExtracted points:\n{pts}\n\nComprehensive answer:", 600).strip()


class ClaudeJudge:
    """Cross-family judge: claude-haiku via airlock, temp>0 for genuine ≥5-run variance."""

    def judge_pair(self, q, a, b, metrics):
        from eval.autoe_judge import build_pairwise_prompt
        return _chat(AIRLOCK_URL, AIRLOCK_KEY, JUDGE_MODEL,
                     build_pairwise_prompt(q, a, b, metrics), 400, 0.7, qwen=False)


# --- load + sample global questions --------------------------------------------- #
arts = load_articles()
docs = {a.doc_id: a.body for a in arts}
subset = [a.body for a in arts][:SUBSET]
qs = [q for q in load_autoq() if q.scope == "global"]
sample = qs[:: max(1, len(qs) // N_Q)][:N_Q]
print(f"corpus={len(arts)} global_q={len(qs)} sampled={len(sample)} | judge={JUDGE_MODEL} (xfamily) n_runs={N_RUNS}\n")

vec = VectorRagAdapter(docs)
lon = LongContextAdapter(docs, char_budget=120_000)
questions = [(q.question_id or f"g{i}", q.question_text) for i, q in enumerate(sample)]

print("generating 3-arm answers (local Qwen, $0)...")
abya: dict[str, dict[str, str]] = {"graphrag_mapreduce": {}, "vector_rag": {}, "long_context": {}}
for qid, qt in questions:
    abya["graphrag_mapreduce"][qid] = graphrag_answer(qt, subset)
    abya["vector_rag"][qid] = baseline_answer(qt, vec.retrieve(qt, K))
    abya["long_context"][qid] = baseline_answer(qt, lon.retrieve(qt, 60))
    print(f"  {qid}: grag={len(abya['graphrag_mapreduce'][qid])}c vec={len(abya['vector_rag'][qid])}c lon={len(abya['long_context'][qid])}c")

bias = assemble_bias_controls(judge_family="anthropic", system_families=[ANSWERER_FAMILY], n_runs=N_RUNS)

for comparator in ("long_context", "vector_rag"):
    pair = ("graphrag_mapreduce", comparator)
    print(f"\njudging {pair[0]} vs {pair[1]} — claude-haiku, {N_RUNS} runs, order-swapped...")
    judgments = run_autoe(ClaudeJudge(), abya, questions, pair, n_runs=N_RUNS, metrics=JUDGE_METRICS)
    jlist = list(judgments.values())
    wr = compute_winrates(jlist, pair, metrics=HEADLINE_METRICS, seed=0)
    length = assemble_length_corroboration(jlist, pair, ran=True)
    print(f"=== CROSS-FAMILY win-rate: {pair[0]} vs {pair[1]} (>0.5 = GraphRAG-style better) ===")
    for m in HEADLINE_METRICS:
        f = wr[m]
        print(f"  {m:18s}: win_rate={f['win_rate']:.3f} CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}] mde={f['mde']:.3f} n={f['n']}")
    res = decide_084(wr, bias, length)
    print(f"  decide_084 verdict: {res['verdict']}  binding={res['binding_constraint']}  blocked_by={res['blocked_by']}  surpass={res['surpass_candidates']}")

print("\nREGISTERED-DESIGN PILOT: cross-family judge + >=5 runs + order-swap (3/4 bias controls real).")
print("Caveats: minimal map-reduce arm over a 40-article subset (not Microsoft GraphRAG); Qwen")
print("answerer ($0); pilot n. Answers the PREMISE on the real harness, not the S1-vs-GraphRAG resolution.")
