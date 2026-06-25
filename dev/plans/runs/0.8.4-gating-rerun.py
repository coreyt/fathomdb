"""0.8.4 GATING re-run: fair, at-power, entity-rich, full-strength Microsoft GraphRAG.

The gating experiment of plan-0.8.4 §7.1, run BEFORE any board/ledger lock-in. It
converts the provisional 200-doc surpass (`0.8.4-scale-powered-run-RESULT.md`,
NOT_REACHED on power AND GraphRAG hobbled to community-level 0) into a *registered*
verdict, removing both confounds:

  1. AT POWER       — N_Q=200 questions (global+linked AutoQ) so `decide_084`'s power
                       bar (mde ≤ ε=0.05) can be cleared.
  2. ENTITY-RICH    — AP-News BenchmarkQED (6,314 entities / 200 docs, full 4-level
                       Leiden hierarchy); questions = 150 global + 50 *linked*
                       (multi-hop entity-relationship) = 200.
  3. FULL-STRENGTH  — GraphRAG queried at **community-level 1** (468 finer reports),
     GRAPHRAG          not the root-only level-0 it was forced to on nano. A fast,
                       reasoning-OFF model (gemini-3.5-flash @ reasoning_effort=none,
                       ~2.3s/call) makes the finer level tractable.
  4. ARM STRATEGY   — **D2** (depth-1 coverage index) = product; **C** (map-reduce
                       QFS) = always-available fallback.

Equivalent model use on EVERY arm (HITL 2026-06-24): one model
(default gemini-3.5-flash, reasoning OFF) for ALL answer synthesis (GraphRAG global
query, C, D2) AND D2 cluster summaries AND GraphRAG's index-time community reports.
Cross-family judge: claude-haiku (anthropic) ≠ the answerer family (google). Bias
controls (order-swap, ≥5 runs, cross-family, length corroboration) all on. Resilient:
atomic checkpoint, idempotent resume, 429/5xx backoff, --max-usd guard. GraphRAG spend
is metered from its on-disk cache `usage` (the airlock LiteLLM proxy has no spend DB).

GraphRAG must be RE-INDEXED with the chosen model first (settings.yaml
completion model = gemini-3.5-flash + call_args.reasoning_effort=none) into --grag-root.

Run:  set -a; . ~/projects/airlock/.env; set +a
      FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=src/python \
        python dev/plans/runs/0.8.4-gating-rerun.py \
          --model gemini-3.5-flash --reasoning-effort none \
          --n-q 200 --community-level 1 --grag-root /tmp/grag-gemini \
          --max-usd 80 [--phase all|build-d2|answers|judge] [--project]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import numpy as np

from eval.apnews_corpus import load_articles, load_autoq
from eval.autoe_judge import (
    _ORDERS,
    ORDER_TC,
    JUDGE_METRICS,
    Judgment,
    JudgmentKey,
    _is_fully_absent,
    assemble_bias_controls,
    assemble_length_corroboration,
    build_pairwise_prompt,
    compute_winrates,
    parse_verdict,
)
from eval.decision_rule_084 import HEADLINE_METRICS, decide_084
from eval.tier2_coverage import (
    CoverageIndex,
    CoverageNode,
    build_coverage_index,
    chunk_corpus,
    default_n_clusters,
    global_answer_mapreduce,
)

AIRLOCK_URL = "http://localhost:4000/v1/chat/completions"
AIRLOCK_KEY = os.environ.get("AIRLOCK_MASTER_KEY", "")
GRAG_PY = os.environ.get("GRAG_PY", "/tmp/gtest-venv/bin/python")
# Per-call HTTP timeout. Kept tight so a rate-limit HANG (the AI Studio gemini path returns
# no clean 429 under burst — it just stalls) fails fast → backoff+retry, instead of pinning
# a worker for the old 300s. Overridable via --http-timeout.
HTTP_TIMEOUT = 60.0

# --- defaults (overridable by CLI) ------------------------------------------- #
DEFAULT_MODEL = "gemini-3.5-flash"
DEFAULT_JUDGE = "claude-haiku"
DEFAULT_REASONING = "minimal"  # "" omits the param; "minimal" disables gemini reasoning AND is a
#                                valid OpenAI enum (litellm-safe; "none" gets dropped by drop_params).
DEFAULT_GRAG_ROOT = "/tmp/grag-gemini"
N_DOCS = 200
K_D2 = 8
MAX_TOKENS = 1500
MAP_BATCH = 8
MAP_TOKENS = 300
SUMMARY_TOKENS = 500
# Concurrency kept LOW: each GraphRAG query itself fans out to `concurrent_requests`
# airlock sub-calls, so outer 8 × inner 6 saturated the airlock gemini upstream and calls
# HUNG (17-31s+ then timeouts). Low outer concurrency keeps the airlock responsive.
CONCURRENCY = 3
JUDGE_CONCURRENCY_DEFAULT = 4

#: Conservative USD per 1M tokens (in, out) for the --max-usd ledger. gemini-3.5-flash
#: is an ESTIMATE (the airlock has no spend DB to confirm) — deliberately high so the
#: guard stops early rather than overspends. Recorded as a caveat in the RESULT.
PRICE = {
    "gpt-5-nano": (0.05, 0.40),
    "gemini-3.5-flash": (0.30, 2.50),
    "gemini-3-flash": (0.15, 0.60),
    "claude-haiku": (0.80, 4.00),
}

#: Model-family map for the self-preference bias control (judge family ≠ system family).
def model_family(model: str) -> str:
    m = model.lower()
    if m.startswith("gemini") or m.startswith("gemma"):
        return "google"
    if m.startswith("gpt") or m.startswith("o1") or m.startswith("o3"):
        return "openai"
    if m.startswith("claude"):
        return "anthropic"
    if m.startswith("qwen"):
        return "alibaba"
    if m.startswith("mistral") or m.startswith("magistral") or m.startswith("codestral"):
        return "mistral"
    return m.split("-")[0]


HERE = Path(__file__).parent
CKPT = HERE / "0.8.4-gating-rerun.ckpt.json"
D2_CACHE = HERE / "0.8.4-gating-d2-index.json"
RESULT_JSON = HERE / "0.8.4-gating-rerun-RESULT.json"

# Mutable run config (filled by main() from CLI) -------------------------------- #
CFG: dict = {
    "model": DEFAULT_MODEL,
    "judge": DEFAULT_JUDGE,
    "reasoning_effort": DEFAULT_REASONING,
    "community_level": 1,
    "grag_root": DEFAULT_GRAG_ROOT,
    "n_q": 200,
    "n_runs": 5,
    "n_docs": N_DOCS,
    "scopes": ("global", "linked"),
    "concurrency": CONCURRENCY,
    "judge_concurrency": JUDGE_CONCURRENCY_DEFAULT,
}

_spent = {"usd": 0.0, "grag_usd": 0.0}
_SPEND_LOCK = threading.Lock()  # _spent is mutated from ThreadPoolExecutor workers


# --- spend metering ----------------------------------------------------------- #
def _meter(model: str, usage: dict) -> None:
    pin, pout = PRICE.get(model, (0.30, 2.50))  # unknown model → conservative
    delta = (float(usage.get("prompt_tokens", 0) or 0) / 1e6 * pin
             + float(usage.get("completion_tokens", 0) or 0) / 1e6 * pout)
    with _SPEND_LOCK:  # avoid lost-update races across concurrent completions
        _spent["usd"] += delta


def _meter_grag(prompt_tokens: int, completion_tokens: int, model: str) -> float:
    pin, pout = PRICE.get(model, (0.30, 2.50))
    delta = prompt_tokens / 1e6 * pin + completion_tokens / 1e6 * pout
    with _SPEND_LOCK:
        _spent["grag_usd"] += delta
    return delta


def _total_spent() -> float:
    with _SPEND_LOCK:
        return _spent["usd"] + _spent["grag_usd"]


def grag_index_usd(grag_root: str, model: str) -> float:
    """One-time GraphRAG INDEX cost: sum ``usage`` across the on-disk cache (index calls
    are cached; query calls are NOT — see _query_root metering below). The airlock
    LiteLLM proxy has no spend DB, so the cache is the index meter."""
    cache_dir = Path(grag_root) / "cache"
    prompt = completion = 0
    if not cache_dir.exists():
        return 0.0
    for f in cache_dir.rglob("*"):
        if not f.is_file():
            continue
        try:
            d = json.loads(f.read_text())
        except (OSError, ValueError):
            continue
        usage = (((d.get("result") or {}).get("response") or {}).get("usage")) or {}
        prompt += int(usage.get("prompt_tokens", 0) or 0)
        completion += int(usage.get("completion_tokens", 0) or 0)
    pin, pout = PRICE.get(model, (0.30, 2.50))
    return prompt / 1e6 * pin + completion / 1e6 * pout


def _make_query_root(grag_root: str) -> str:
    """A throwaway GraphRAG root for ONE query: own settings.yaml + logs/, symlinking the
    shared index (output/, prompts/, input/). Each query thus writes its OWN logs/query.log
    so concurrent queries don't clobber the token-stats file we parse to meter spend."""
    qroot = tempfile.mkdtemp(prefix="gquery-")
    shutil.copy(Path(grag_root) / "settings.yaml", Path(qroot) / "settings.yaml")
    for sub in ("output", "prompts", "input"):
        src = Path(grag_root) / sub
        if src.exists():
            os.symlink(src, Path(qroot) / sub)
    return qroot


def _parse_query_log_usage(qroot: str) -> tuple[int, int]:
    """Parse (prompt_tokens, completion_tokens) from a query subprocess's logs/query.log
    stats block (GraphRAG writes them there). Returns (0, 0) if absent."""
    log = Path(qroot) / "logs" / "query.log"
    if not log.exists():
        return 0, 0
    try:
        text = log.read_text()
    except OSError:
        return 0, 0
    # The stats block is a JSON object; pull the two integer fields directly.
    import re
    p = re.search(r'"prompt_tokens"\s*:\s*(\d+)', text)
    c = re.search(r'"completion_tokens"\s*:\s*(\d+)', text)
    return (int(p.group(1)) if p else 0, int(c.group(1)) if c else 0)


def _chat(model: str, prompt: str, max_tokens: int, *, temp: float = 0.0, retries: int = 8,
          reasoning_effort: str = "") -> str:
    """Sync /chat/completions with backoff on 429/5xx, honoring server Retry-After.

    ``reasoning_effort`` (e.g. "minimal") is forwarded ONLY when explicitly passed — so the
    gemini *answerer* disables reasoning (avoids the degenerate all-reasoning-tokens trap)
    while the claude-haiku *judge* is called WITHOUT it (Anthropic rejects the param → HTTP
    400; the judge must not carry the answerer's reasoning knob).

    Resilience ([[priced-runs-need-resilience-before-spend]]): the airlock interactive
    endpoint TPM-quarantines under load and RE-ARMS on naive retries. Honoring the server
    ``Retry-After`` header (capped) is how we escape that spiral; backoff is also longer
    (max 120s) and there are more retries, so a transient 429 storm recovers instead of
    failing the answer."""
    body: dict = {"model": model, "messages": [{"role": "user", "content": prompt}],
                  "temperature": temp, "max_tokens": max_tokens}
    if reasoning_effort:
        body["reasoning_effort"] = reasoning_effort
    data = json.dumps(body).encode()
    for attempt in range(retries):
        try:
            req = urllib.request.Request(AIRLOCK_URL, data=data,
                                         headers={"Content-Type": "application/json",
                                                  "Authorization": f"Bearer {AIRLOCK_KEY}"})
            with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT) as r:
                payload = json.loads(r.read())
            _meter(model, payload.get("usage", {}) or {})
            return payload["choices"][0]["message"]["content"] or ""
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as e:
            code = getattr(e, "code", None)
            if attempt == retries - 1 or (code is not None and code not in (429, 500, 502, 503, 504)):
                raise
            retry_after = 0.0
            hdrs = getattr(e, "headers", None)
            if hdrs is not None:
                try:
                    retry_after = float(hdrs.get("Retry-After", "0") or "0")
                except (TypeError, ValueError):
                    retry_after = 0.0
            # honor Retry-After (capped at 120s) or exponential backoff, whichever is larger
            time.sleep(min(max(retry_after, min(2 ** attempt, 120.0)), 120.0))
    return ""


def _guard(max_usd: float) -> None:
    if _total_spent() > max_usd:
        _save_ckpt()
        sys.exit(f"--max-usd ${max_usd} exceeded (spent ${_total_spent():.3f} = "
                 f"fathomdb ${_spent['usd']:.3f} + graphrag ${_spent['grag_usd']:.3f}); "
                 f"checkpoint saved, resume to continue")


def reader_fn(prompt: str, max_tokens: int) -> str:
    # The gemini answerer carries reasoning_effort (disable reasoning); the judge does NOT.
    return _chat(CFG["model"], prompt, max_tokens, reasoning_effort=CFG["reasoning_effort"]).strip()


# --- checkpoint --------------------------------------------------------------- #
def _load_ckpt() -> dict:
    if CKPT.exists():
        return json.loads(CKPT.read_text())
    return {"answers": {"microsoft_graphrag": {}, "fathomdb_c": {}, "fathomdb_d2": {}},
            "spent_usd": 0.0, "grag_usd": 0.0}


_STATE = _load_ckpt()
_spent["usd"] = float(_STATE.get("spent_usd", 0.0))
_spent["grag_usd"] = float(_STATE.get("grag_usd", 0.0))


def _save_ckpt() -> None:
    _STATE["spent_usd"] = round(_spent["usd"], 4)
    _STATE["grag_usd"] = round(_spent["grag_usd"], 4)
    tmp = CKPT.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(_STATE, indent=2))
    tmp.replace(CKPT)  # atomic


# --- corpus + questions ------------------------------------------------------- #
def load_corpus():
    arts = load_articles()[: CFG["n_docs"]]
    docs = {a.doc_id: a.body for a in arts}
    scopes = set(CFG["scopes"])
    qs = [q for q in load_autoq() if q.scope in scopes]
    n_q = CFG["n_q"]
    if len(qs) < n_q:
        sys.exit(f"FATAL: only {len(qs)} AutoQ questions in scopes {sorted(scopes)} "
                 f"(<{n_q} required) — fail loudly, do not silently under-power")
    step = max(1, len(qs) // n_q)
    sample = qs[::step][:n_q]
    questions = [(q.question_id if getattr(q, "question_id", None) else f"q{i}", q.question_text)
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
    """GraphRAG global search at the configured community level (full-strength, not root-only).

    --no-dynamic-selection: dynamic selection LLM-rates ALL communities/query (the flagged-cost
    upgrade we STOP before, per §7.1). Metered separately from the on-disk cache."""
    grag_root = CFG["grag_root"]
    env = dict(os.environ, GRAPHRAG_API_KEY=AIRLOCK_KEY)
    qroot = _make_query_root(grag_root)  # own logs/ so concurrent queries don't clobber stats
    try:
        p = subprocess.run([GRAG_PY, "-m", "graphrag", "query", "-r", qroot, "-m", "global",
                            "--community-level", str(CFG["community_level"]), "--no-dynamic-selection", question],
                           capture_output=True, text=True, env=env, timeout=900)
        # Query calls are NOT cached by GraphRAG; meter from this query's own query.log
        # stats (exact prompt/completion tokens) immediately so the --max-usd guard sees
        # GraphRAG spend live and resume preserves it (checkpointed by the caller).
        pt, ct = _parse_query_log_usage(qroot)
        _meter_grag(pt, ct, CFG["model"])
    finally:
        shutil.rmtree(qroot, ignore_errors=True)
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
                                       map_tokens=MAP_TOKENS, answer_tokens=MAX_TOKENS)

    jobs = []  # (arm, qid, callable)
    for qid, qt in questions:
        if qid not in ans["microsoft_graphrag"]:
            jobs.append(("microsoft_graphrag", qid, lambda qt=qt: graphrag_global(qt)))
        if qid not in ans["fathomdb_c"]:
            jobs.append(("fathomdb_c", qid, lambda qt=qt: c_answer(qt)))
        if qid not in ans["fathomdb_d2"]:
            jobs.append(("fathomdb_d2", qid, lambda qt=qt: d2_answer(qt, index, embed)))

    conc = CFG["concurrency"]
    print(f"[answers] {len(jobs)} answer-jobs to run (concurrency={conc}); "
          f"GraphRAG metered per-query from query.log...")
    done = 0
    # NB: explicit try/finally + shutdown(cancel_futures=True) — NOT `with ... as ex`. A
    # `with` block's __exit__ calls shutdown(wait=True), which would let every queued paid
    # job finish AFTER _guard() trips --max-usd. cancel_futures kills not-yet-started jobs,
    # capping overspend to the ≤CONCURRENCY already in flight.
    ex = ThreadPoolExecutor(max_workers=conc)
    try:
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
                print(f"  {done}/{len(jobs)} answers; spent=${_total_spent():.3f} "
                      f"(fdb ${_spent['usd']:.3f} + grag ${_spent['grag_usd']:.3f})")
            _guard(max_usd)  # raises SystemExit over budget; finally cancels queued jobs
    finally:
        ex.shutdown(wait=False, cancel_futures=True)
    _save_ckpt()
    print(f"[answers] complete; spent=${_total_spent():.3f} "
          f"(fdb ${_spent['usd']:.3f} + grag ${_spent['grag_usd']:.3f})")


# --- phase: judge ------------------------------------------------------------- #
JUDGE_CONCURRENCY = JUDGE_CONCURRENCY_DEFAULT  # claude-haiku via airlock; low to dodge the TPM quarantine


class ClaudeJudge:
    def judge_pair(self, q, a, b, metrics):
        return _chat(CFG["judge"], build_pairwise_prompt(q, a, b, metrics), 400, temp=0.7)


def _persist_judgments(judgments: dict) -> None:
    """Serialize {JudgmentKey: Judgment} to the checkpoint as {custom_id: verdicts}."""
    _STATE["judgments"] = {k.to_custom_id(): dict(j.verdicts) for k, j in judgments.items()}


def _load_judgments() -> dict:
    out: dict = {}
    for cid, verdicts in (_STATE.get("judgments") or {}).items():
        key = JudgmentKey.from_custom_id(cid)
        out[key] = Judgment(key=key, verdicts=dict(verdicts))
    return out


def _judge_pair_concurrent(judge_obj, ans, questions, pair, n_runs, metrics, judgments, max_usd) -> None:
    """Judge every (question, run, order) for ``pair`` CONCURRENTLY, mirroring run_autoe's
    EXACT key/order semantics (frozen harness contract) but adding a thread pool +
    incremental checkpointing so a long sync judge survives process limits / TPM re-arm.

    Idempotent resume: keys already present (and not fully-ABSENT) in ``judgments`` are
    skipped; results accumulate into ``judgments`` and persist to the checkpoint."""
    treatment, comparator = pair
    t_answers, c_answers = ans[treatment], ans[comparator]
    jobs: list[tuple[JudgmentKey, str, str, str]] = []
    for qid, text in questions:
        t_ans, c_ans = t_answers[qid], c_answers[qid]
        for run_idx in range(n_runs):
            for order in _ORDERS:
                key = JudgmentKey(question_id=qid, pair=pair, run_idx=run_idx, order=order)
                if key in judgments and not _is_fully_absent(judgments[key], metrics):
                    continue  # idempotent resume (re-judge only dead/fully-ABSENT cells)
                a, b = (t_ans, c_ans) if order == ORDER_TC else (c_ans, t_ans)
                jobs.append((key, text, a, b))

    jconc = CFG["judge_concurrency"]
    print(f"[judge]   {len(jobs)} judge-calls for {pair[0]} (concurrency={jconc})...")
    done = 0
    # Same budget-cap discipline as gen_answers: cancel_futures on a guard trip so a
    # --max-usd breach can't keep paying for hundreds of queued judge calls.
    ex = ThreadPoolExecutor(max_workers=jconc)
    try:
        futs = {ex.submit(judge_obj.judge_pair, text, a, b, metrics): key
                for key, text, a, b in jobs}
        for fut in as_completed(futs):
            key = futs[fut]
            try:
                completion = fut.result()
            except Exception as e:  # noqa: BLE001 — record + continue; resume re-judges this key
                print(f"  [warn] judge {key.question_id}/{key.order} failed: {type(e).__name__}: {str(e)[:100]}")
                continue
            judgments[key] = Judgment(key=key, verdicts=parse_verdict(completion, metrics))
            done += 1
            if done % 50 == 0:
                _persist_judgments(judgments)
                _save_ckpt()
                print(f"    {done}/{len(jobs)} judged; spent=${_total_spent():.3f}")
            _guard(max_usd)
    finally:
        # Persist completed judgments even on a _guard() SystemExit — otherwise the up-to-50
        # judged since the last checkpoint are lost and re-paid on resume (codex3 P2).
        ex.shutdown(wait=False, cancel_futures=True)
        _persist_judgments(judgments)
        _save_ckpt()


def _degenerate_guard(questions) -> None:
    """Fail loudly if any arm has empty/degenerate answers (would poison the judge)."""
    ans = _STATE["answers"]
    for arm in ("microsoft_graphrag", "fathomdb_c", "fathomdb_d2"):
        empties = [qid for qid, _ in questions if len((ans[arm].get(qid) or "").strip()) < 40]
        if empties:
            sys.exit(f"FATAL: arm {arm} has {len(empties)} empty/degenerate answers "
                     f"(e.g. {empties[:5]}) — refuse to judge a poisoned arm; re-run answers")


def judge(questions, max_usd: float) -> dict:
    _degenerate_guard(questions)
    ans = _STATE["answers"]
    sys_family = model_family(CFG["model"])
    judge_family = model_family(CFG["judge"])
    bias = assemble_bias_controls(judge_family=judge_family, system_families=[sys_family],
                                  n_runs=CFG["n_runs"])
    judgments = _load_judgments()  # resume across pairs
    judge_obj = ClaudeJudge()
    results = {}
    for fdb in ("fathomdb_d2", "fathomdb_c"):  # D2 (product) first, then C (fallback)
        pair = (fdb, "microsoft_graphrag")
        print(f"[judge] {fdb} vs microsoft_graphrag — {CFG['judge']}, {CFG['n_runs']} runs, order-swapped...")
        _judge_pair_concurrent(judge_obj, ans, questions, pair, CFG["n_runs"], JUDGE_METRICS,
                               judgments, max_usd)
        jl = [j for j in judgments.values() if tuple(j.key.pair) == pair]
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


def write_result(results) -> None:
    RESULT_JSON.write_text(json.dumps({
        "config": {"model": CFG["model"], "reasoning_effort": CFG["reasoning_effort"],
                   "judge": CFG["judge"], "n_docs": CFG["n_docs"], "n_q": CFG["n_q"],
                   "n_runs": CFG["n_runs"], "k_d2": K_D2, "max_tokens": MAX_TOKENS,
                   "community_level": CFG["community_level"], "grag_root": CFG["grag_root"],
                   "question_scopes": list(CFG["scopes"]),
                   "judge_family": model_family(CFG["judge"]),
                   "system_families": [model_family(CFG["model"])]},
        "results": {a: {m: results[a]["winrates"][m] for m in HEADLINE_METRICS}
                    | {"verdict": results[a]["verdict"]["verdict"],
                       "binding": results[a]["verdict"]["binding_constraint"],
                       "surpass": results[a]["verdict"]["surpass_candidates"],
                       "blocked_by": results[a]["verdict"]["blocked_by"],
                       "length_contradicts": results[a]["length"]["contradicts"]}
                    for a in results},
        "spend": {"fathomdb_usd": round(_spent["usd"], 4),
                  "graphrag_usd": round(_spent["grag_usd"], 4),
                  "total_usd": round(_total_spent(), 4)}}, indent=2))
    print(f"\nGATING RESULT written to {RESULT_JSON.name}. "
          f"Total spent ${_total_spent():.3f} "
          f"(fathomdb ${_spent['usd']:.3f} + graphrag ${_spent['grag_usd']:.3f}).")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--max-usd", type=float, default=80.0)
    ap.add_argument("--phase", choices=["all", "build-d2", "answers", "judge"], default="all")
    ap.add_argument("--model", default=DEFAULT_MODEL)
    ap.add_argument("--judge-model", default=DEFAULT_JUDGE)
    ap.add_argument("--reasoning-effort", default=DEFAULT_REASONING,
                    help='reasoning_effort forwarded to the model ("" to omit; "minimal" disables gemini reasoning)')
    ap.add_argument("--n-q", type=int, default=200)
    ap.add_argument("--n-runs", type=int, default=5)
    ap.add_argument("--n-docs", type=int, default=N_DOCS)
    ap.add_argument("--community-level", type=int, default=1)
    ap.add_argument("--grag-root", default=DEFAULT_GRAG_ROOT)
    ap.add_argument("--scopes", default="global,linked", help="comma-separated AutoQ scopes")
    ap.add_argument("--concurrency", type=int, default=CONCURRENCY,
                    help="outer answer/judge thread-pool size (airlock budget, not TPM, was the blocker)")
    args = ap.parse_args()
    if not AIRLOCK_KEY:
        sys.exit("AIRLOCK_MASTER_KEY not set — `set -a; . ~/projects/airlock/.env; set +a` first")
    # P3: a paid measurement below the frozen ≥5-run bias control is invalid — fail BEFORE spend.
    if args.n_runs < 5:
        sys.exit(f"--n-runs {args.n_runs} < 5 violates the frozen MIN_RUNS stochasticity control "
                 f"(decide_084 would BLOCK) — refuse to spend on an invalid measurement")

    CFG.update(model=args.model, judge=args.judge_model, reasoning_effort=args.reasoning_effort,
               community_level=args.community_level, grag_root=args.grag_root,
               n_q=args.n_q, n_runs=args.n_runs, n_docs=args.n_docs,
               concurrency=args.concurrency,
               judge_concurrency=max(JUDGE_CONCURRENCY_DEFAULT, args.concurrency),
               scopes=tuple(s.strip() for s in args.scopes.split(",") if s.strip()))

    global GRAG_PY
    GRAG_PY = os.environ.get("GRAG_PY", "/tmp/gtest-venv/bin/python")

    # One-time GraphRAG index cost (cached calls); added once, guarded by a checkpoint flag
    # so resume never double-charges it. Only marks metered once the index actually exists.
    if not _STATE.get("grag_index_metered"):
        idx_usd = grag_index_usd(CFG["grag_root"], CFG["model"])
        if idx_usd > 0:
            with _SPEND_LOCK:
                _spent["grag_usd"] += idx_usd
            _STATE["grag_index_metered"] = True
            _save_ckpt()
            print(f"[grag] one-time index cost metered from cache: ${idx_usd:.3f}")

    docs, questions = load_corpus()
    print(f"GATING run | model={CFG['model']} reasoning={CFG['reasoning_effort'] or 'default'} "
          f"judge={CFG['judge']} | docs={len(docs)} q={len(questions)} scopes={CFG['scopes']} "
          f"runs={CFG['n_runs']} clvl={CFG['community_level']} grag={CFG['grag_root']} "
          f"| spent so far=${_total_spent():.3f}")

    index = None
    if args.phase in ("all", "build-d2", "answers"):
        index = build_d2(docs, args.max_usd)
    if args.phase in ("all", "answers"):
        gen_answers(docs, questions, index, args.max_usd)
    if args.phase in ("all", "judge"):
        results = judge(questions, args.max_usd)
        write_result(results)


if __name__ == "__main__":
    main()
