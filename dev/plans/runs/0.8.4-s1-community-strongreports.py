"""0.8.4 — community-based S1 with STRONG (gpt-5.4) community reports — the decisive experiment.

Variant of 0.8.4-s1-community.py: community REPORTS now written by gpt-5.4 (not weak Qwen), to test
whether strong community summaries flip the community-S1 loss. If S1 STILL loses -> settle graph null.

Original:

Builds the defining GraphRAG structure faithfully, with NO heavy deps (pure-Python
community detection; PEP 668 blocks pip here):
  1. entity extraction per article (local Qwen, $0)
  2. an article graph: nodes=articles, edge weight = # shared entities
  3. **community detection** (pure-Python label propagation) → communities (the C-level)
  4. **community reports**: an LLM summary per community (local Qwen, $0)  [GraphRAG's core artifact]
  5. **global search**: map (score+extract per community report for the query) → reduce to a
     global answer (gpt-5.4, the strong reader) — GraphRAG's global-search query path.

Then a CROSS-FAMILY (claude-haiku) judged comparison: s1_community vs vector_rag & long_context
on global questions, >=5 runs, order-swapped, through decide_084.

> Honest scope: a SUBSET build (community detection over ~60 articles) — a real community-based
> GraphRAG mechanism, still not Microsoft's package over the full 1397, and underpowered. But it
> is an actual community-detection S1 arm, the structure the flat map-reduce lacked.

Run:  set -a; . ~/projects/airlock/.env; set +a;
  FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=. python ../../dev/plans/runs/0.8.4-s1-community.py
"""

from __future__ import annotations

import json
import os
import re
import urllib.request
from collections import defaultdict

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
JUDGE_MODEL = "claude-haiku"
READER = "gpt-5.4"        # strong reader for query-time answers (all arms)
SUBSET = 60               # articles the S1 graph/communities are built over
N_Q = 10
N_RUNS = 5
K = 10


def _chat(url, key, model, prompt, max_tokens, temp, qwen):
    body = {"model": model, "messages": [{"role": "user", "content": prompt}],
            "temperature": temp, "max_tokens": max_tokens}
    if qwen:
        body["chat_template_kwargs"] = {"enable_thinking": False}
    h = {"Content-Type": "application/json"}
    if key:
        h["Authorization"] = f"Bearer {key}"
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=h)
    with urllib.request.urlopen(req, timeout=240) as r:
        return json.loads(r.read())["choices"][0]["message"]["content"] or ""


def qwen(p, mt):
    return _chat(QWEN_URL, None, "qwen3.6-27b", p, mt, 0.0, True)


def reader(p, mt):
    return _chat(AIRLOCK_URL, AIRLOCK_KEY, READER, p, mt, 0.0, False)


# --------------------------------------------------------------------------- #
# 1. entity extraction (Qwen, $0)
# --------------------------------------------------------------------------- #
def extract_entities(body: str) -> set[str]:
    out = qwen("List the 3-8 most important named entities (people, orgs, places, topics) in this "
               f"news text as a comma-separated list, nothing else.\n\n{body[:2500]}\n\nEntities:", 120)
    ents = {re.sub(r"[^a-z0-9 ]", "", e.strip().lower()) for e in out.split(",")}
    return {e for e in ents if 2 < len(e) < 40}


# --------------------------------------------------------------------------- #
# 3. community detection — pure-Python label propagation (no deps)
# --------------------------------------------------------------------------- #
def label_propagation(nodes: list[str], adj: dict[str, dict[str, float]], iters: int = 20) -> dict[str, int]:
    label = {n: i for i, n in enumerate(nodes)}
    order = list(nodes)
    for _ in range(iters):
        changed = False
        for n in order:  # deterministic order (no RNG)
            if not adj[n]:
                continue
            weights: dict[int, float] = defaultdict(float)
            for nb, w in adj[n].items():
                weights[label[nb]] += w
            best = max(weights.items(), key=lambda kv: (kv[1], -kv[0]))[0]
            if label[n] != best:
                label[n] = best
                changed = True
        if not changed:
            break
    # renumber communities compactly
    remap: dict[int, int] = {}
    return {n: remap.setdefault(label[n], len(remap)) for n in nodes}


# --- build corpus + S1 graph ---------------------------------------------------- #
arts = load_articles()[:SUBSET]
docs_all = {a.doc_id: a.body for a in load_articles()}  # full corpus for baselines
print(f"S1 build over {len(arts)} articles | reader={READER} judge={JUDGE_MODEL} (xfamily) n_runs={N_RUNS}")
print("1) extracting entities (Qwen, $0)...")
ent_by_doc = {a.doc_id: extract_entities(a.body) for a in arts}
ent_to_docs: dict[str, set[str]] = defaultdict(set)
for d, es in ent_by_doc.items():
    for e in es:
        ent_to_docs[e].add(d)

# 2) article graph: edge weight = # shared entities
ids = [a.doc_id for a in arts]
adj: dict[str, dict[str, float]] = {d: defaultdict(float) for d in ids}
for _e, ds in ent_to_docs.items():
    ds = list(ds)
    for i in range(len(ds)):
        for j in range(i + 1, len(ds)):
            adj[ds[i]][ds[j]] += 1.0
            adj[ds[j]][ds[i]] += 1.0

comm = label_propagation(ids, adj)
communities: dict[int, list[str]] = defaultdict(list)
for d, c in comm.items():
    communities[c].append(d)
sizes = sorted((len(v) for v in communities.values()), reverse=True)
print(f"2-3) {len(communities)} communities detected (sizes: {sizes[:10]}{'...' if len(sizes) > 10 else ''})")

# 4) community reports (Qwen, $0): summarize each community's articles
body_by_doc = {a.doc_id: a.body for a in arts}
reports: dict[int, str] = {}
print("4) writing community reports (gpt-5.4 STRONG reports)...")
for c, members in communities.items():
    joined = "\n\n".join(body_by_doc[d][:1200] for d in members[:8])
    reports[c] = reader("Write a concise report (5-8 sentences) capturing the key themes, entities, and "
                        f"claims shared across these related news articles.\n\n{joined}\n\nCommunity report:", 350).strip()
print(f"   {len(reports)} community reports written")


# --------------------------------------------------------------------------- #
# 5. S1 global search (gpt-5.4 reader): map over community reports → reduce
# --------------------------------------------------------------------------- #
def s1_answer(question: str) -> str:
    partials = []
    for c, rep in reports.items():
        m = reader(f"From this community report, extract points relevant to the question (or 'NONE').\n\n"
                   f"Question: {question}\n\nReport:\n{rep}\n\nRelevant points:", 200).strip()
        if m and "NONE" not in m[:8].upper():
            partials.append(m)
    pts = "\n\n".join(f"- {p}" for p in partials) or "(no community had relevant material)"
    return reader("Synthesize a comprehensive, global answer to the question from these points drawn "
                  f"from community reports across the corpus.\n\nQuestion: {question}\n\nPoints:\n{pts}\n\nAnswer:", 600).strip()


def baseline_answer(question, hits):
    ctx = "\n\n".join(f"[{i + 1}] {h.body}" for i, h in enumerate(hits))
    return reader(f"Answer using ONLY the context. Be comprehensive but factual.\n\nContext:\n{ctx}\n\n"
                  f"Question: {question}\n\nAnswer:", 600).strip()


vec = VectorRagAdapter(docs_all)
lon = LongContextAdapter(docs_all, char_budget=120_000)
qs = [q for q in load_autoq() if q.scope == "global"]
sample = qs[:: max(1, len(qs) // N_Q)][:N_Q]
questions = [(q.question_id or f"g{i}", q.question_text) for i, q in enumerate(sample)]

print(f"\n5) generating answers for {len(questions)} questions (S1 reader=gpt-5.4)...")
abya = {"s1_community": {}, "vector_rag": {}, "long_context": {}}
for qid, qt in questions:
    abya["s1_community"][qid] = s1_answer(qt)
    abya["vector_rag"][qid] = baseline_answer(qt, vec.retrieve(qt, K))
    abya["long_context"][qid] = baseline_answer(qt, lon.retrieve(qt, 60))
    print(f"  {qid}: s1={len(abya['s1_community'][qid])}c vec={len(abya['vector_rag'][qid])}c lon={len(abya['long_context'][qid])}c")


class ClaudeJudge:
    def judge_pair(self, q, a, b, metrics):
        from eval.autoe_judge import build_pairwise_prompt
        return _chat(AIRLOCK_URL, AIRLOCK_KEY, JUDGE_MODEL, build_pairwise_prompt(q, a, b, metrics), 400, 0.7, False)


bias = assemble_bias_controls(judge_family="anthropic", system_families=["openai"], n_runs=N_RUNS)
for comparator in ("long_context", "vector_rag"):
    pair = ("s1_community", comparator)
    print(f"\njudging {pair[0]} vs {pair[1]} — claude-haiku, {N_RUNS} runs, order-swapped...")
    j = run_autoe(ClaudeJudge(), abya, questions, pair, n_runs=N_RUNS, metrics=JUDGE_METRICS)
    jl = list(j.values())
    wr = compute_winrates(jl, pair, metrics=HEADLINE_METRICS, seed=0)
    length = assemble_length_corroboration(jl, pair, ran=True)
    print(f"=== CROSS-FAMILY: {pair[0]} (community-based GraphRAG) vs {pair[1]} (>0.5 = S1 better) ===")
    for m in HEADLINE_METRICS:
        f = wr[m]
        print(f"  {m:18s}: win_rate={f['win_rate']:.3f} CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}] mde={f['mde']:.3f} n={f['n']}")
    res = decide_084(wr, bias, length)
    print(f"  decide_084: {res['verdict']} binding={res['binding_constraint']} surpass={res['surpass_candidates']}")

print("\nACTUAL community-based GraphRAG/S1 arm (entity graph + label-propagation communities + LLM")
print("community reports + global map-reduce), cross-family judged. Subset build (60 arts), underpowered.")
