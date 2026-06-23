"""0.8.4 — $0 end-to-end PREMISE SMOKE on the local Qwen (vllm :8000).

NOT a registered verdict: the answerer AND judge are both local Qwen3.6-27B (same
family → the cross-family self-preference control is violated ON PURPOSE), so this is
a DIRECTIONAL premise read + a full-pipeline execution proof, never a parity claim.
Goal: (1) prove the AutoE pipeline runs end-to-end against a real OpenAI-compatible
endpoint at $0, (2) get an early read on whether `long_context` already saturates the
corpus vs a real-ish `vector_rag` (the Samsung "VectorRAG is almost enough" prior).

Run from src/python:  FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=. python ../../dev/plans/runs/0.8.4-premise-smoke.py
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
N_Q = 6           # tiny smoke: span buckets
K = 10
PAIR = ("vector_rag", "long_context")  # treatment vs the long-context control


def qwen(prompt: str, max_tokens: int) -> str:
    """Call local Qwen with thinking DISABLED → clean completion. $0 local."""
    payload = json.dumps({
        "model": QWEN_MODEL,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0,
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": False},
    }).encode("utf-8")
    req = urllib.request.Request(QWEN_URL, data=payload, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=180) as resp:
        body = json.loads(resp.read())
    return body["choices"][0]["message"]["content"] or ""


def answer(question: str, hits) -> str:
    ctx = "\n\n".join(f"[{i + 1}] {h.body}" for i, h in enumerate(hits))
    prompt = (
        "Answer the question using ONLY the context passages. Be comprehensive but "
        "factual; synthesize across passages where relevant.\n\n"
        f"Context:\n{ctx}\n\nQuestion: {question}\n\nAnswer:"
    )
    return qwen(prompt, max_tokens=600).strip()


class QwenJudge:
    """A PilotJudge: judge_pair(question, a, b, metrics) -> raw completion string."""

    def judge_pair(self, question: str, answer_a: str, answer_b: str, metrics) -> str:
        from eval.autoe_judge import build_pairwise_prompt
        return qwen(build_pairwise_prompt(question, answer_a, answer_b, metrics), max_tokens=400)


# --- load + sample -------------------------------------------------------------- #
arts = load_articles()
docs = {a.doc_id: a.body for a in arts}
qs = load_autoq()
sample = qs[:: max(1, len(qs) // N_Q)][:N_Q]
print(f"corpus={len(arts)} autoq={len(qs)} sampled={len(sample)} buckets={sorted({q.bucket for q in sample})}\n")

vec = VectorRagAdapter(docs)
lon = LongContextAdapter(docs, char_budget=120_000)  # ~30k tok ≈ ~30 articles, fits Qwen 64k window
questions = [(q.question_id or f"q{i}", q.question_text) for i, q in enumerate(sample)]

# --- generate arm answers (real Qwen) ------------------------------------------- #
print("generating arm answers (local Qwen, thinking off)...")
answers_by_arm = {"vector_rag": {}, "long_context": {}}
for qid, qtext in questions:
    answers_by_arm["vector_rag"][qid] = answer(qtext, vec.retrieve(qtext, K))
    # long_context: large k so the 120k-char budget binds (the honest "stuff-it-all-in" control).
    answers_by_arm["long_context"][qid] = answer(qtext, lon.retrieve(qtext, 60))
    print(f"  {qid}: vec={len(answers_by_arm['vector_rag'][qid])}c  lon={len(answers_by_arm['long_context'][qid])}c")

# --- judge (real Qwen, both orders) → win-rates --------------------------------- #
print("\njudging (local Qwen, order-swapped)...")
judgments = run_autoe(QwenJudge(), answers_by_arm, questions, PAIR, n_runs=1, metrics=JUDGE_METRICS)
decided = sum(1 for j in judgments.values() for m in HEADLINE_METRICS if j.verdicts.get(m, "ABSENT") != "ABSENT")
print(f"  {len(judgments)} judgments, {decided} decided headline cells")

wr = compute_winrates(judgments.values(), PAIR, metrics=HEADLINE_METRICS, seed=0)
print(f"\n=== DIRECTIONAL premise read: {PAIR[0]} vs {PAIR[1]} (win-rate >0.5 = vector_rag better) ===")
for m in HEADLINE_METRICS:
    f = wr[m]
    print(f"  {m:18s}: win_rate={f['win_rate']:.3f}  CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}]  n={f['n']}")
print("\nNOTE: NON-REGISTERED (Qwen judges Qwen — cross-family control violated on purpose).")
print("Directional only: a vector_rag win-rate well below 0.5 = long_context dominates =")
print("weak-premise warning. A real registered run uses gpt-5.4 answerer + cross-family Claude judge.")
