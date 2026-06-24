"""0.8.4 SCALE powered run: C vs D2 vs Microsoft GraphRAG @ 200 docs, 50 questions.

The decisive test the Tier-1 fair re-run pointed to (`0.8.4-tier1-fair-rerun-RESULT.md`):
at SCALE (200 docs, where flat map-reduce gets lossy and GraphRAG's community structure
should earn its keep), does FathomDB's almost-graph-free Tier-2 (C map-reduce QFS, D2
depth-1 coverage index) reach parity-or-better vs a running Microsoft GraphRAG?

Per HITL (2026-06-23): **gpt-5-nano** for all answer synthesis (GraphRAG global-query,
C, D2) AND D2 cluster summaries — equivalent gpt use on every arm. **claude-haiku**
cross-family judge. **NO batch API** — concurrent *sync* /chat/completions (a thread
pool, not /v1/batches). D2 embeds with FathomDB's OWN engine embedder (Engine.embed,
real CLS-corrected bge-small).

Phases (each checkpointed → idempotent resume; --max-usd ledger guard):
  build-d2   : chunk 200 docs -> Engine.embed -> k-means -> nano cluster summaries
  answers    : GraphRAG global (CLI) + C + D2, 50 global questions, matched max_tokens
  judge      : claude-haiku, 5 runs, order-swapped, decide_084 + length corroboration

Run:  set -a; . ~/projects/airlock/.env; set +a
      FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=src/python \
        python dev/plans/runs/0.8.4-scale-powered-run.py [--max-usd 15] [--phase all|build-d2|answers|judge]
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import numpy as np

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import (
    JUDGE_METRICS,
    assemble_bias_controls,
    assemble_length_corroboration,
    build_pairwise_prompt,
    compute_winrates,
    run_autoe,
)
from eval.decision_rule_084 import HEADLINE_METRICS, decide_084
from eval.tier2_coverage import (
    build_coverage_index,
    chunk_corpus,
    default_n_clusters,
    global_answer_mapreduce,
)
from eval.tier2_coverage import CoverageIndex, CoverageNode

AIRLOCK_URL = "http://localhost:4000/v1/chat/completions"
AIRLOCK_KEY = os.environ.get("AIRLOCK_MASTER_KEY", "")
GRAG_PY = "/tmp/gtest-venv/bin/python"
GRAG_ROOT = "/tmp/grag200"

READER = "gpt-5-nano"       # all answer synthesis + D2 summaries (equivalent gpt use)
JUDGE_MODEL = "claude-haiku"  # cross-family judge
N_DOCS = 200
N_Q = 50
N_RUNS = 5
K_D2 = 8
MAX_TOKENS = 1500
MAP_BATCH = 8
SUMMARY_TOKENS = 500
CONCURRENCY = 8

PRICE = {  # USD per 1M tokens (conservative) for the --max-usd ledger
    "gpt-5-nano": (0.05, 0.40),
    "claude-haiku": (0.80, 4.00),
}

HERE = Path(__file__).parent
CKPT = HERE / "0.8.4-scale-powered-run.ckpt.json"
D2_CACHE = HERE / "0.8.4-scale-d2-index.json"
RESULT = HERE / "0.8.4-scale-powered-run-RESULT.json"

_spent = {"usd": 0.0}


def _meter(model: str, usage: dict) -> None:
    pin, pout = PRICE.get(model, (0.0, 0.0))
    _spent["usd"] += float(usage.get("prompt_tokens", 0) or 0) / 1e6 * pin
    _spent["usd"] += float(usage.get("completion_tokens", 0) or 0) / 1e6 * pout


def _chat(model: str, prompt: str, max_tokens: int, *, temp: float = 0.0, retries: int = 5) -> str:
    """Sync /chat/completions with exponential backoff on 429/5xx (resilience)."""
    body = {"model": model, "messages": [{"role": "user", "content": prompt}],
            "temperature": temp, "max_tokens": max_tokens}
    data = json.dumps(body).encode()
    for attempt in range(retries):
        try:
            req = urllib.request.Request(AIRLOCK_URL, data=data,
                                         headers={"Content-Type": "application/json",
                                                  "Authorization": f"Bearer {AIRLOCK_KEY}"})
            with urllib.request.urlopen(req, timeout=300) as r:
                payload = json.loads(r.read())
            _meter(model, payload.get("usage", {}) or {})
            return payload["choices"][0]["message"]["content"] or ""
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as e:
            code = getattr(e, "code", None)
            if attempt == retries - 1 or (code is not None and code not in (429, 500, 502, 503, 504)):
                raise
            time.sleep(min(2 ** attempt, 30))
    return ""


def _guard(max_usd: float) -> None:
    if _spent["usd"] > max_usd:
        _save_ckpt()
        sys.exit(f"--max-usd ${max_usd} exceeded (spent ${_spent['usd']:.3f}); checkpoint saved, resume to continue")


def reader_fn(prompt: str, max_tokens: int) -> str:
    return _chat(READER, prompt, max_tokens).strip()


# --- checkpoint --------------------------------------------------------------- #
def _load_ckpt() -> dict:
    if CKPT.exists():
        return json.loads(CKPT.read_text())
    return {"answers": {"microsoft_graphrag": {}, "fathomdb_c": {}, "fathomdb_d2": {}}, "spent_usd": 0.0}


_STATE = _load_ckpt()
_spent["usd"] = float(_STATE.get("spent_usd", 0.0))


def _save_ckpt() -> None:
    _STATE["spent_usd"] = round(_spent["usd"], 4)
    CKPT.write_text(json.dumps(_STATE, indent=2))


# --- corpus + questions ------------------------------------------------------- #
def load_corpus():
    arts = load_articles()[:N_DOCS]
    docs = {a.doc_id: a.body for a in arts}
    qs = [q for q in load_autoq() if q.scope == "global"]
    step = max(1, len(qs) // N_Q)
    sample = qs[::step][:N_Q]
    questions = [(q.question_id if getattr(q, "question_id", None) else f"g{i}", q.question_text)
                 for i, q in enumerate(sample)]
    return docs, questions


# --- engine embedder (real bge-small) ----------------------------------------- #
def open_embedder():
    import tempfile
    from fathomdb import Engine
    eng = Engine.open(os.path.join(tempfile.mkdtemp(), "embed.sqlite"), use_default_embedder=True)

    def embed(text: str) -> np.ndarray:
        v = np.asarray(eng.embed(text), dtype=np.float64)
        n = float(np.linalg.norm(v))
        return v / n if n > 0.0 else v

    return embed, eng


# --- phase: build D2 (cached) ------------------------------------------------- #
def build_d2(docs, max_usd: float) -> CoverageIndex:
    if D2_CACHE.exists():
        raw = json.loads(D2_CACHE.read_text())
        nodes = [CoverageNode(n["node_id"], n["summary"], np.asarray(n["embedding"]), tuple(n["member_chunk_ids"]))
                 for n in raw["nodes"]]
        print(f"[build-d2] loaded cached index: {len(nodes)} coverage nodes")
        return CoverageIndex(nodes)

    chunks = chunk_corpus(docs)
    nclust = default_n_clusters(len(chunks))
    print(f"[build-d2] {len(docs)} docs -> {len(chunks)} chunks -> {nclust} clusters; embedding (real bge-small)...")
    embed, _eng = open_embedder()
    index = build_coverage_index(chunks, embed, reader_fn, n_clusters=nclust, summary_tokens=SUMMARY_TOKENS)
    _guard(max_usd)
    D2_CACHE.write_text(json.dumps({"nodes": [
        {"node_id": n.node_id, "summary": n.summary, "embedding": [float(x) for x in n.embedding],
         "member_chunk_ids": list(n.member_chunk_ids)} for n in index.nodes]}, indent=2))
    print(f"[build-d2] built + cached {len(index.nodes)} coverage nodes; spent=${_spent['usd']:.3f}")
    return index


def d2_answer(question: str, index: CoverageIndex, embed) -> str:
    hits = index.retrieve(embed(question), K_D2)
    ctx = "\n\n".join(f"[Theme {i + 1}] {h.summary}" for i, h in enumerate(hits))
    return reader_fn(f"Using these whole-corpus thematic reports, synthesize a comprehensive global "
                     f"answer.\n\nQuestion: {question}\n\nThemes:\n{ctx}\n\nAnswer:", MAX_TOKENS)


# --- phase: answers ----------------------------------------------------------- #
def graphrag_global(question: str) -> str:
    # community-level 0 (59 root communities — the right granularity for GLOBAL sensemaking)
    # + --no-dynamic-selection: dynamic selection LLM-rates all 1492 communities/query (>7min on
    # nano -> timeouts); level-0 map-reduce runs ~40s. The CLI prints the markdown answer directly
    # (no "Global Search Response:" marker on this version) after LiteLLM warning lines.
    env = dict(os.environ, GRAPHRAG_API_KEY=AIRLOCK_KEY)
    p = subprocess.run([GRAG_PY, "-m", "graphrag", "query", "-r", GRAG_ROOT, "-m", "global",
                        "--community-level", "0", "--no-dynamic-selection", question],
                       capture_output=True, text=True, env=env, timeout=240)
    lines = [ln for ln in p.stdout.splitlines() if "LiteLLM" not in ln and "botocore" not in ln]
    ans = "\n".join(lines).strip()[:6000]
    if not ans:
        sys.stderr.write(f"[graphrag] empty; stderr tail:\n{p.stderr[-400:]}\n")
    return ans


def gen_answers(docs, questions, index, max_usd: float) -> None:
    chunks = chunk_corpus(docs)
    embed, _eng = open_embedder()
    ans = _STATE["answers"]

    def c_answer(q):
        return global_answer_mapreduce(q, chunks, reader_fn, map_batch=MAP_BATCH,
                                       map_tokens=300, answer_tokens=MAX_TOKENS)

    jobs = []  # (arm, qid, callable)
    for qid, qt in questions:
        if qid not in ans["microsoft_graphrag"]:
            jobs.append(("microsoft_graphrag", qid, lambda qt=qt: graphrag_global(qt)))
        if qid not in ans["fathomdb_c"]:
            jobs.append(("fathomdb_c", qid, lambda qt=qt: c_answer(qt)))
        if qid not in ans["fathomdb_d2"]:
            jobs.append(("fathomdb_d2", qid, lambda qt=qt: d2_answer(qt, index, embed)))

    print(f"[answers] {len(jobs)} answer-jobs to run (concurrency={CONCURRENCY})...")
    done = 0
    with ThreadPoolExecutor(max_workers=CONCURRENCY) as ex:
        futs = {ex.submit(fn): (arm, qid) for arm, qid, fn in jobs}
        for fut in as_completed(futs):
            arm, qid = futs[fut]
            try:
                ans[arm][qid] = fut.result()
            except Exception as e:  # noqa: BLE001 — record + continue, resume picks up the rest
                print(f"  [warn] {arm}/{qid} failed: {type(e).__name__}: {str(e)[:120]}")
                continue
            done += 1
            if done % 10 == 0:
                _save_ckpt()
                print(f"  {done}/{len(jobs)} answers; spent=${_spent['usd']:.3f}")
            _guard(max_usd)
    _save_ckpt()
    print(f"[answers] complete; spent=${_spent['usd']:.3f}")


# --- phase: judge ------------------------------------------------------------- #
class ClaudeJudge:
    def judge_pair(self, q, a, b, metrics):
        return _chat(JUDGE_MODEL, build_pairwise_prompt(q, a, b, metrics), 400, temp=0.7)


def judge(questions, max_usd: float) -> dict:
    ans = _STATE["answers"]
    bias = assemble_bias_controls(judge_family="anthropic", system_families=["openai"], n_runs=N_RUNS)
    results = {}
    for fdb in ("fathomdb_c", "fathomdb_d2"):
        pair = (fdb, "microsoft_graphrag")
        print(f"[judge] {fdb} vs microsoft_graphrag — claude-haiku, {N_RUNS} runs, order-swapped...")
        j = run_autoe(ClaudeJudge(), ans, questions, pair, n_runs=N_RUNS, metrics=JUDGE_METRICS)
        _guard(max_usd)
        jl = list(j.values())
        wr = compute_winrates(jl, pair, metrics=HEADLINE_METRICS, seed=0)
        length = assemble_length_corroboration(jl, pair, ran=True)
        res = decide_084(wr, bias, length)
        results[fdb] = {"winrates": wr, "verdict": res, "length": length}
        for m in HEADLINE_METRICS:
            f = wr[m]
            print(f"    {m:18s}: win_rate={f['win_rate']:.3f} CI[{f['ci_lo']:.3f},{f['ci_hi']:.3f}] "
                  f"mde={f['mde']:.3f} n={f['n']}")
        print(f"    decide_084: {res['verdict']} binding={res['binding_constraint']} "
              f"surpass={res['surpass_candidates']} length_contradicts={length['contradicts']}")
    _save_ckpt()
    return results


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-usd", type=float, default=15.0)
    ap.add_argument("--phase", choices=["all", "build-d2", "answers", "judge"], default="all")
    args = ap.parse_args()
    if not AIRLOCK_KEY:
        sys.exit("AIRLOCK_MASTER_KEY not set — `set -a; . ~/projects/airlock/.env; set +a` first")

    docs, questions = load_corpus()
    print(f"SCALE run | reader={READER} judge={JUDGE_MODEL} | docs={len(docs)} q={len(questions)} "
          f"runs={N_RUNS} | spent so far=${_spent['usd']:.3f}")

    index = None
    if args.phase in ("all", "build-d2", "answers"):
        index = build_d2(docs, args.max_usd)
    if args.phase in ("all", "answers"):
        gen_answers(docs, questions, index, args.max_usd)
    if args.phase in ("all", "judge"):
        results = judge(questions, args.max_usd)
        RESULT.write_text(json.dumps({
            "config": {"reader": READER, "judge": JUDGE_MODEL, "n_docs": len(docs),
                       "n_q": len(questions), "n_runs": N_RUNS, "k_d2": K_D2, "max_tokens": MAX_TOKENS},
            "results": {a: {m: results[a]["winrates"][m] for m in HEADLINE_METRICS}
                        | {"verdict": results[a]["verdict"]["verdict"],
                           "binding": results[a]["verdict"]["binding_constraint"],
                           "surpass": results[a]["verdict"]["surpass_candidates"],
                           "length_contradicts": results[a]["length"]["contradicts"]}
                        for a in results},
            "spent_usd": round(_spent["usd"], 4)}, indent=2))
        print(f"\nSCALE RESULT written. Total spent ${_spent['usd']:.3f}.")


if __name__ == "__main__":
    main()
