"""0.8.4 — deterministic $0 cost projection for the AutoE powered run.

NO LLM call, NO spend. Measures the DOMINANT, most-uncertain cost — the answerer's
retrieved-context input tokens — EXACTLY from the real AP-News corpus, bounds the
second-order terms (answer / verdict lengths), and projects the total powered-run USD
across question-count and judge-price scenarios so the HITL can size the budget top-up.

Run from src/python:  python ../../dev/plans/runs/0.8.4-cost-probe.py
"""

from __future__ import annotations

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import build_pairwise_prompt, project_autoe_cost
from eval.autoe_pilot_run import _estimate_tokens
from eval.baselines_084 import LongContextAdapter, VectorRagAdapter
from eval.decision_rule_084 import HEADLINE_METRICS, MIN_RUNS

# --- prices ($/1M in, $/1M out) ------------------------------------------------ #
GPT54 = (1.25, 5.00)  # pinned in eval/gap_decomposition_run.py (the answerer)
# Cross-family Claude JUDGE — representative tiers (TO BE PINNED before the real run):
JUDGE_TIERS = {
    "claude-haiku-ish": (1.00, 5.00),
    "claude-sonnet-ish": (3.00, 15.00),
    "claude-opus-ish": (15.00, 75.00),
}

K = 10
LONG_CTX_CHARS = 48_000
ANSWER_TOKENS = 400          # assumed synthesized-answer length (2nd-order; Qwen can refine)
VERDICT_TOKENS = 80          # the judge verdict JSON (small, fixed-ish format)
N_SAMPLE_Q = 25

arts = load_articles()
docs = {a.doc_id: a.body for a in arts}
qs = load_autoq()
sample = qs[:: max(1, len(qs) // N_SAMPLE_Q)][:N_SAMPLE_Q]
print(f"corpus={len(arts)} articles  autoq={len(qs)}  sampled={len(sample)} questions\n")

vec = VectorRagAdapter(docs)
lon = LongContextAdapter(docs, char_budget=LONG_CTX_CHARS)

# --- measure answerer-leg INPUT tokens per arm (the dominant, exact term) ------- #
def arm_prompt_tokens(adapter, q_text: str) -> int:
    hits = adapter.retrieve(q_text, K)
    # answerer prompt ~= question + concatenated retrieved bodies + ~modest instructions
    ctx = "\n\n".join(h.body for h in hits)
    return _estimate_tokens(q_text + "\n\n" + ctx) + 60

vec_in = [arm_prompt_tokens(vec, q.question_text) for q in sample]
lon_in = [arm_prompt_tokens(lon, q.question_text) for q in sample]
avg_vec, avg_lon = sum(vec_in) // len(vec_in), sum(lon_in) // len(lon_in)
avg_ans_in = (avg_vec + avg_lon) // 2
print("ANSWERER leg (gpt-5.4), measured input tokens/call from real corpus:")
print(f"  vector_rag (k={K}):        ~{avg_vec:,} tok in")
print(f"  long_context ({LONG_CTX_CHARS//1000}k char):  ~{avg_lon:,} tok in")
print(f"  per-arm avg:               ~{avg_ans_in:,} tok in, ~{ANSWER_TOKENS} tok out\n")

# --- measure judge-leg prompt tokens (real prompt builder, assumed answer len) -- #
filler = "x " * (ANSWER_TOKENS * 4 // 2)  # ~ANSWER_TOKENS-token placeholder answer
judge_in = [
    _estimate_tokens(build_pairwise_prompt(q.question_text, filler, filler, HEADLINE_METRICS + ("directness",)))
    for q in sample
]
avg_judge_in = sum(judge_in) // len(judge_in)
print(f"JUDGE leg (cross-family Claude), prompt tokens/call: ~{avg_judge_in:,} tok in, ~{VERDICT_TOKENS} tok out")
print(f"  (1 judge call covers all {len(HEADLINE_METRICS)} headline metrics + directness)\n")

# --- project total = answerer leg (gpt-5.4) + judge leg (Claude) ---------------- #
PAIRS = 1            # the pilot pair vector_rag-vs-long_context; powered adds s1/graphrag pairs
ARMS = 2
def answerer_usd(nq: int, n_runs: int) -> float:
    # one shared-answerer call per (arm, question); runs/orders reuse the same answer.
    calls = nq * ARMS
    return calls * (avg_ans_in / 1e6 * GPT54[0] + ANSWER_TOKENS / 1e6 * GPT54[1])

print(f"{'='*78}\nPROJECTED POWERED-RUN COST (n_runs={MIN_RUNS}, {ARMS} arms, {PAIRS} pair, order-swap x2)\n{'='*78}")
for nq in (50, 100, 125):
    ans = answerer_usd(nq, MIN_RUNS)
    print(f"\n--- {nq} questions ---")
    print(f"  answerer leg (gpt-5.4): ${ans:6.2f}   ({nq*ARMS} calls)")
    for tier, (pin, pout) in JUDGE_TIERS.items():
        proj = project_autoe_cost(
            prompt_tokens=avg_judge_in, completion_tokens=VERDICT_TOKENS,
            n_calls=1, price_in_per_1m=pin, price_out_per_1m=pout,
            n_questions=nq, n_pairs=PAIRS, n_runs=MIN_RUNS, n_orders=2,
        )
        jusd = proj["projected_full_usd"]
        print(f"  + judge {tier:18s}: ${jusd:7.2f}  =>  TOTAL ${ans + jusd:7.2f}   ({proj['projected_full_calls']} judge calls)")

print("\nNOTE: answerer input tokens MEASURED from the real corpus (exact). Answer/verdict")
print("lengths assumed (2nd-order); local Qwen (vllm :8000) can refine them at $0. Claude")
print("judge prices are representative tiers — PIN the chosen judge's real rate before spend.")
print("Powered run adds s1-vs-graphrag pairs later (multiply judge leg by n_pairs).")
