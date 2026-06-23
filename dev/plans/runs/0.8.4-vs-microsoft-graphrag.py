"""0.8.4 — the LITERAL head-to-head: FathomDB vs a RUNNING Microsoft GraphRAG.

Microsoft GraphRAG 3.1.0 (pip-installed in /tmp/gtest-venv) indexed 15 AP-News articles
(entity extraction -> Leiden communities -> community reports, LLM=gpt-5.4 via airlock,
embeddings via a local shim). This runs its **global-search** query path and compares it
head-to-head against FathomDB arms over the SAME 15 documents (fair corpus), cross-family
judged (claude-haiku), >=5 runs, order-swapped, through decide_084.

FathomDB arms: vector_rag (FathomDB retrieval) + flat map-reduce over raw text (the config
that won the earlier pilots). Question: does FathomDB reach parity-or-better vs the actual
Microsoft GraphRAG on global sensemaking?

> Honest scope: 15-doc index (tiny), underpowered. But it IS the literal running Microsoft
> GraphRAG, the head-to-head the goal names.

Run:  set -a; . ~/projects/airlock/.env; set +a;
  FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=. python ../../dev/plans/runs/0.8.4-vs-microsoft-graphrag.py
"""

from __future__ import annotations

import json
import os
import subprocess
import urllib.request

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import (
    JUDGE_METRICS,
    assemble_bias_controls,
    assemble_length_corroboration,
    compute_winrates,
    run_autoe,
)
from eval.baselines_084 import VectorRagAdapter
from eval.decision_rule_084 import HEADLINE_METRICS, decide_084

AIRLOCK_URL = "http://localhost:4000/v1/chat/completions"
AIRLOCK_KEY = os.environ["AIRLOCK_MASTER_KEY"]
GRAG_PY = "/tmp/gtest-venv/bin/python"
GRAG_ROOT = "/tmp/grag"
READER = "gpt-5.4"
JUDGE_MODEL = "claude-haiku"
N_DOCS = 15      # the docs Microsoft GraphRAG indexed (fair corpus for both sides)
N_Q = 8
N_RUNS = 5
K = 8


def reader(prompt, mt, temp=0.0):
    body = {"model": READER, "messages": [{"role": "user", "content": prompt}], "temperature": temp, "max_tokens": mt}
    req = urllib.request.Request(AIRLOCK_URL, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {AIRLOCK_KEY}"})
    with urllib.request.urlopen(req, timeout=240) as r:
        return json.loads(r.read())["choices"][0]["message"]["content"] or ""


def graphrag_global(question: str) -> str:
    """Microsoft GraphRAG global-search via its CLI."""
    env = dict(os.environ, GRAPHRAG_API_KEY=AIRLOCK_KEY)
    p = subprocess.run([GRAG_PY, "-m", "graphrag", "query", "-r", GRAG_ROOT, "-m", "global", question],
                       capture_output=True, text=True, env=env, timeout=300)
    out = p.stdout
    # CLI prints "SUCCESS: Global Search Response:" then the answer
    marker = "Global Search Response:"
    return (out.split(marker, 1)[1].strip() if marker in out else out.strip())[:6000]


# --- corpus: the SAME 15 docs Microsoft GraphRAG indexed ------------------------ #
arts = load_articles()[:N_DOCS]
docs15 = {a.doc_id: a.body for a in arts}
vec = VectorRagAdapter(docs15)
qs = [q for q in load_autoq() if q.scope == "global"]
sample = qs[:: max(1, len(qs) // N_Q)][:N_Q]
questions = [(q.question_id or f"g{i}", q.question_text) for i, q in enumerate(sample)]
print(f"Microsoft GraphRAG (15-doc index) vs FathomDB | reader={READER} judge={JUDGE_MODEL} (xfam) n_runs={N_RUNS}")


def fathomdb_vector(question):
    hits = vec.retrieve(question, K)
    ctx = "\n\n".join(f"[{i + 1}] {h.body}" for i, h in enumerate(hits))
    return reader(f"Answer using ONLY the context. Be comprehensive but factual.\n\nContext:\n{ctx}\n\n"
                  f"Question: {question}\n\nAnswer:", 600).strip()


def fathomdb_mapreduce(question):
    bodies = list(docs15.values())
    partials = []
    for i in range(0, len(bodies), 5):
        chunk = "\n\n".join(f"[{j + 1}] {b}" for j, b in enumerate(bodies[i:i + 5]))
        m = reader(f"Extract points relevant to the question (or 'NONE').\n\nQuestion: {question}\n\n{chunk}\n\nPoints:", 250).strip()
        if m and "NONE" not in m[:8].upper():
            partials.append(m)
    return reader(f"Synthesize a comprehensive global answer.\n\nQuestion: {question}\n\nPoints:\n" +
                  "\n\n".join(partials) + "\n\nAnswer:", 600).strip()


print(f"generating answers for {len(questions)} global questions (Microsoft GraphRAG + 2 FathomDB arms)...")
abya = {"microsoft_graphrag": {}, "fathomdb_vector": {}, "fathomdb_mapreduce": {}}
for qid, qt in questions:
    abya["microsoft_graphrag"][qid] = graphrag_global(qt)
    abya["fathomdb_vector"][qid] = fathomdb_vector(qt)
    abya["fathomdb_mapreduce"][qid] = fathomdb_mapreduce(qt)
    a = abya
    print(f"  {qid}: msGraphRAG={len(a['microsoft_graphrag'][qid])}c fdb_vec={len(a['fathomdb_vector'][qid])}c fdb_mr={len(a['fathomdb_mapreduce'][qid])}c")


class ClaudeJudge:
    def judge_pair(self, q, a, b, metrics):
        from eval.autoe_judge import build_pairwise_prompt
        body = {"model": JUDGE_MODEL, "messages": [{"role": "user", "content": build_pairwise_prompt(q, a, b, metrics)}],
                "temperature": 0.7, "max_tokens": 400}
        req = urllib.request.Request(AIRLOCK_URL, data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json", "Authorization": f"Bearer {AIRLOCK_KEY}"})
        with urllib.request.urlopen(req, timeout=240) as r:
            return json.loads(r.read())["choices"][0]["message"]["content"] or ""


bias = assemble_bias_controls(judge_family="anthropic", system_families=["openai"], n_runs=N_RUNS)
# FathomDB arm vs Microsoft GraphRAG: win-rate >0.5 = FathomDB better => parity-or-better iff >=~0.5
for fdb in ("fathomdb_mapreduce", "fathomdb_vector"):
    pair = (fdb, "microsoft_graphrag")
    print(f"\njudging {pair[0]} vs {pair[1]} (Microsoft GraphRAG) — claude-haiku, {N_RUNS} runs, order-swapped...")
    j = run_autoe(ClaudeJudge(), abya, questions, pair, n_runs=N_RUNS, metrics=JUDGE_METRICS)
    jl = list(j.values())
    wr = compute_winrates(jl, pair, metrics=HEADLINE_METRICS, seed=0)
    length = assemble_length_corroboration(jl, pair, ran=True)
    print(f"=== {pair[0]} (FathomDB) vs {pair[1]} (Microsoft GraphRAG) — >0.5 = FathomDB better ===")
    for m in HEADLINE_METRICS:
        f = wr[m]
        print(f"  {m:18s}: win_rate={f['win_rate']:.3f} CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}] mde={f['mde']:.3f} n={f['n']}")
    res = decide_084(wr, bias, length)
    print(f"  decide_084: {res['verdict']} binding={res['binding_constraint']} surpass={res['surpass_candidates']}")

print("\nLITERAL FathomDB vs RUNNING Microsoft GraphRAG (15-doc index, fair same-corpus), cross-family judged.")
print("Parity-or-better for FathomDB iff win-rate-vs-GraphRAG CI lower bound >= 0.45 (near-parity band).")
