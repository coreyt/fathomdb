"""0.8.4 — $0 directional smoke: a GraphRAG-STYLE map-reduce reader vs baselines.

The defining GraphRAG query mechanism is map-reduce "local-to-global" query-focused
summarization (arXiv:2404.16130). This builds a MINIMAL version — map over corpus
chunks (Qwen extracts query-relevant points per chunk), reduce to a global answer —
needing NO Leiden/networkx (the GraphRAG paper itself includes source-text map-reduce
as a condition). It then runs the FIRST directional comparison of a GraphRAG-style arm
vs `vector_rag` and `long_context` on GLOBAL-sensemaking questions, judged by local Qwen.

> **NOT a registered verdict.** Same-family judge (Qwen judges Qwen); n small; corpus
> SUBSET for the map-reduce (not full-corpus global coverage — a real GraphRAG pre-builds
> community summaries offline to cover everything cheaply). A directional proof-of-mechanism
> + the first GraphRAG-style-vs-baseline numbers, never a parity claim.

Run from src/python:  FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=. python ../../dev/plans/runs/0.8.4-graphrag-style-smoke.py
"""

from __future__ import annotations

import json
import urllib.request

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import JUDGE_METRICS, compute_winrates, run_autoe
from eval.baselines_084 import LongContextAdapter, VectorRagAdapter
from eval.decision_rule_084 import HEADLINE_METRICS

QWEN_URL = "http://127.0.0.1:8000/v1/chat/completions"
QWEN_MODEL = "qwen3.6-27b"
K = 10
SUBSET = 40          # corpus subset the map-reduce covers (kept small for a $0 smoke)
BATCH = 5            # articles per map call
N_GLOBAL_Q = 4       # global-sensemaking questions


def qwen(prompt: str, max_tokens: int) -> str:
    payload = json.dumps({
        "model": QWEN_MODEL,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0,
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": False},
    }).encode("utf-8")
    req = urllib.request.Request(QWEN_URL, data=payload, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=180) as resp:
        return json.loads(resp.read())["choices"][0]["message"]["content"] or ""


def baseline_answer(question: str, hits) -> str:
    ctx = "\n\n".join(f"[{i + 1}] {h.body}" for i, h in enumerate(hits))
    prompt = (
        "Answer the question using ONLY the context passages. Be comprehensive but "
        f"factual.\n\nContext:\n{ctx}\n\nQuestion: {question}\n\nAnswer:"
    )
    return qwen(prompt, max_tokens=600).strip()


def graphrag_mapreduce_answer(question: str, subset_bodies: list[str]) -> str:
    """The GraphRAG-style query path: map over chunks → reduce to a global answer."""
    partials = []
    for i in range(0, len(subset_bodies), BATCH):
        chunk = "\n\n".join(f"[{j + 1}] {b}" for j, b in enumerate(subset_bodies[i : i + BATCH]))
        m = qwen(
            "From these news passages, extract ONLY points relevant to answering the "
            f"question (or reply 'NONE').\n\nQuestion: {question}\n\nPassages:\n{chunk}\n\n"
            "Relevant points:",
            max_tokens=300,
        ).strip()
        if m and "NONE" not in m[:8].upper():
            partials.append(m)
    reduce_ctx = "\n\n".join(f"- {p}" for p in partials)
    return qwen(
        "Synthesize a comprehensive, global answer to the question from these extracted "
        f"points across the corpus.\n\nQuestion: {question}\n\nExtracted points:\n{reduce_ctx}\n\n"
        "Comprehensive answer:",
        max_tokens=600,
    ).strip()


# --- load + sample GLOBAL questions --------------------------------------------- #
arts = load_articles()
docs = {a.doc_id: a.body for a in arts}
bodies = [a.body for a in arts]
subset = bodies[:SUBSET]
qs = [q for q in load_autoq() if q.scope == "global"]
sample = qs[:: max(1, len(qs) // N_GLOBAL_Q)][:N_GLOBAL_Q]
print(f"corpus={len(arts)} global_q={len(qs)} sampled={len(sample)} buckets={sorted({q.bucket for q in sample})}")
print(f"map-reduce over {SUBSET}-article subset, {BATCH}/batch = {-(-SUBSET // BATCH)} map calls/query\n")

vec = VectorRagAdapter(docs)
lon = LongContextAdapter(docs, char_budget=120_000)
questions = [(q.question_id or f"g{i}", q.question_text) for i, q in enumerate(sample)]

# --- generate answers for all 3 arms (real Qwen) -------------------------------- #
print("generating arm answers (local Qwen)...")
answers_by_arm: dict[str, dict[str, str]] = {"graphrag_mapreduce": {}, "vector_rag": {}, "long_context": {}}
for qid, qtext in questions:
    answers_by_arm["graphrag_mapreduce"][qid] = graphrag_mapreduce_answer(qtext, subset)
    answers_by_arm["vector_rag"][qid] = baseline_answer(qtext, vec.retrieve(qtext, K))
    answers_by_arm["long_context"][qid] = baseline_answer(qtext, lon.retrieve(qtext, 60))
    a = answers_by_arm
    print(f"  {qid}: graphrag={len(a['graphrag_mapreduce'][qid])}c vec={len(a['vector_rag'][qid])}c lon={len(a['long_context'][qid])}c")

# --- judge graphrag_mapreduce vs each baseline (Qwen, order-swapped) ------------- #
for comparator in ("vector_rag", "long_context"):
    pair = ("graphrag_mapreduce", comparator)
    print(f"\njudging {pair[0]} vs {pair[1]} (order-swapped)...")

    class _J:
        def judge_pair(self, q, a, b, metrics):
            from eval.autoe_judge import build_pairwise_prompt
            return qwen(build_pairwise_prompt(q, a, b, metrics), max_tokens=400)

    judgments = run_autoe(_J(), answers_by_arm, questions, pair, n_runs=1, metrics=JUDGE_METRICS)
    wr = compute_winrates(judgments.values(), pair, metrics=HEADLINE_METRICS, seed=0)
    print(f"=== {pair[0]} vs {pair[1]} (win-rate >0.5 = GraphRAG-style better) ===")
    for m in HEADLINE_METRICS:
        f = wr[m]
        print(f"  {m:18s}: win_rate={f['win_rate']:.3f}  CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}]  n={f['n']}")

print("\nNON-REGISTERED (Qwen judges Qwen; corpus subset; small n). Directional proof-of-mechanism:")
print("the FIRST GraphRAG-style (map-reduce QFS) arm measured vs baselines on global questions.")
