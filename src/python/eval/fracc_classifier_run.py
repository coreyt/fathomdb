"""0.8.11 Slice 20 — EXP-Fr-acc *base* (classifier accuracy + asymmetric mis-route cost).

Two deliverables (pre-registered: `0.8.11-implementation.md` §1 EXP-Fr-acc base;
PSD `planner-router-psd-0.8.x.md` §II.A / §II.D / §III.D):

1. **5-class intent-classifier accuracy** over ``{needle | multi_session | temporal
   | global | multi_hop}``. This measures the *internal classifier fallback*
   (PSD §II.A preference #3 — the agent-passed label is preferred when present).
   Inference is **$0**: a pure-numpy lexical TF-IDF nearest-centroid (Rocchio)
   classifier over the existing labeled corpora (LME / LOCOMO / AP-News / MuSiQue),
   evaluated with stratified k-fold cross-validation. Per-class accuracy + macro
   with bootstrap CI + confusion matrix. (No torch/sklearn in this env — the
   lexical classifier is the honest $0 fallback proxy; reported as such.)

2. **Asymmetric mis-route cost matrix** — per (intent, chosen-route) answer-quality
   accuracy delta vs the correct route. The load-bearing cell: routing a **needle**
   to ``C`` (map-reduce / query-focused-summarization) summarizes the needle away —
   prior **-0.362 + an LLM call** (PSD §II.D). This arm is the priced part (the
   map-reduce summarizer + a correctness judge); it runs on the **resilient harness**
   (per-item checkpoint, idempotent ``--resume``, 429/5xx backoff, ``BudgetLedger``
   pre-call guard) modeled on :mod:`eval.autoe_pilot_run`. Cheap-validate first.

$0 + small $ (ceiling **$3**). The classifier section makes **no** network call.
The mis-route section is gated by ``R2_RUN=1`` (else it is reported as deferred).
Budget tracked via :class:`eval.gap_decomposition_run.BudgetLedger`; running ``$``
in the output. KILL check: classifier accuracy at chance (0.20) for >=2 classes →
register the floor, prototype leans on the agent-passed label.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import re
import time
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Optional

import numpy as np

from eval.gap_decomposition_run import BudgetExceeded, BudgetLedger, price_for
from eval.locomo_loader import load_locomo
from eval.m1_baseline_run import _is_retryable, _retry_after_seconds
from eval.m1_verdict_run import _atomic_write_json

REPO = Path(__file__).resolve().parents[3]
RUNS = REPO / "dev" / "plans" / "runs"
DATA = REPO / "data" / "corpus-data"

INTENT_CLASSES = ("needle", "multi_session", "temporal", "global", "multi_hop")
#: Intents that carry gold short answers + retrievable context → measurable in the
#: mis-route arm. ``global`` is reference-free (decide_084 win-rate) → deferred here.
MISROUTE_CLASSES = ("needle", "multi_session", "temporal", "multi_hop")
CHANCE = 1.0 / len(INTENT_CLASSES)

LME_GOLD = RUNS / "0.8.3-d0a-memory-gold.json"
APNEWS_V1 = DATA / "raw" / "apnews_benchmarkqed" / "generated_questions_v1"
MUSIQUE = DATA / "raw" / "musique_dev.jsonl"

_R2_RUN_ENV = "R2_RUN"
_TOKEN_RE = re.compile(r"[a-z0-9]+")
_RETRY_HARD_CAP = 600.0


# --------------------------------------------------------------------------- #
# Part 1 — labeled query loading (for the $0 classifier)
# --------------------------------------------------------------------------- #
def _load_json(p: Path) -> Any:
    return json.loads(p.read_text(encoding="utf-8"))


def load_labeled_queries(*, seed: int = 0) -> dict[str, list[str]]:
    """Return ``{intent_class: [query_text, ...]}`` from the existing corpora.

    Ground-truth labels are the corpus-derived intent classes (Gate-0 map):
    LME factoid+knowledge_update + LOCOMO factoid → ``needle``; LME/LOCOMO
    ``multi_session`` / ``temporal`` 1:1; AP-News global → ``global``;
    MuSiQue answerable → ``multi_hop``. Order is made deterministic (sorted +
    seeded shuffle) so the balanced sample + folds are reproducible.
    """
    rng = random.Random(seed)
    out: dict[str, list[str]] = {c: [] for c in INTENT_CLASSES}

    # LOCOMO (clean short-answer memory corpus)
    _docs, locomo_gold = load_locomo()
    for g in locomo_gold:
        cls = g["query_class"]
        intent = "needle" if cls == "factoid" else cls
        if intent in out:
            out[intent].append(g["query"])

    # LME (LongMemEval-cleaned)
    lme = _load_json(LME_GOLD)["queries"]
    for q in lme:
        cls = q.get("query_class")
        intent = "needle" if cls in ("factoid", "knowledge_update") else cls
        if intent in out:
            out[intent].append(q["query"])

    # AP-News global (sensemaking)
    for fn in ("activity_global_questions_text.json", "data_global_questions_text.json"):
        p = APNEWS_V1 / fn
        if p.exists():
            out["global"].extend(str(x) for x in _load_json(p))

    # MuSiQue answerable (multi-hop)
    with MUSIQUE.open(encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            r = json.loads(line)
            if r.get("answerable"):
                out["multi_hop"].append(r["question"])

    for c in out:
        out[c] = sorted(set(out[c]))
        rng.shuffle(out[c])
    return out


def balanced_sample(
    by_class: dict[str, list[str]], *, n_per_class: Optional[int] = None
) -> tuple[list[str], list[str]]:
    """Balanced ``(texts, labels)`` with ``n_per_class`` per class (default = the
    smallest class size, so chance = 1/n_classes exactly)."""
    cap = min(len(v) for v in by_class.values())
    n = cap if n_per_class is None else min(n_per_class, cap)
    texts: list[str] = []
    labels: list[str] = []
    for c in INTENT_CLASSES:
        for t in by_class[c][:n]:
            texts.append(t)
            labels.append(c)
    return texts, labels


# --------------------------------------------------------------------------- #
# Part 1 — pure-numpy TF-IDF nearest-centroid (Rocchio) classifier
# --------------------------------------------------------------------------- #
def _tokenize(text: str) -> list[str]:
    return _TOKEN_RE.findall(text.lower())


def _fit_tfidf(train_texts: list[str]) -> tuple[dict[str, int], np.ndarray]:
    """Fit a vocab + IDF on the training texts (so test leakage is impossible)."""
    df: Counter[str] = Counter()
    for t in train_texts:
        for tok in set(_tokenize(t)):
            df[tok] += 1
    vocab = {tok: i for i, tok in enumerate(sorted(df))}
    n_docs = len(train_texts)
    idf = np.zeros(len(vocab), dtype=np.float64)
    for tok, i in vocab.items():
        idf[i] = math.log((1.0 + n_docs) / (1.0 + df[tok])) + 1.0
    return vocab, idf


def _vectorize(texts: list[str], vocab: dict[str, int], idf: np.ndarray) -> np.ndarray:
    mat = np.zeros((len(texts), len(vocab)), dtype=np.float64)
    for r, t in enumerate(texts):
        tf: Counter[str] = Counter(tok for tok in _tokenize(t) if tok in vocab)
        for tok, c in tf.items():
            mat[r, vocab[tok]] = c
    mat *= idf[None, :]
    norms = np.linalg.norm(mat, axis=1, keepdims=True)
    norms[norms == 0.0] = 1.0
    return mat / norms


def _nearest_centroid_predict(
    train_texts: list[str], train_labels: list[str], test_texts: list[str]
) -> list[str]:
    vocab, idf = _fit_tfidf(train_texts)
    xtr = _vectorize(train_texts, vocab, idf)
    classes = list(INTENT_CLASSES)
    centroids = np.zeros((len(classes), xtr.shape[1]), dtype=np.float64)
    for ci, c in enumerate(classes):
        rows = [i for i, lab in enumerate(train_labels) if lab == c]
        if rows:
            v = xtr[rows].mean(axis=0)
            nrm = np.linalg.norm(v)
            centroids[ci] = v / nrm if nrm else v
    xte = _vectorize(test_texts, vocab, idf)
    sims = xte @ centroids.T  # cosine (both L2-normalized)
    return [classes[int(j)] for j in sims.argmax(axis=1)]


def stratified_folds(labels: list[str], *, k: int, seed: int) -> list[list[int]]:
    rng = random.Random(seed)
    folds: list[list[int]] = [[] for _ in range(k)]
    by_label: dict[str, list[int]] = defaultdict(list)
    for i, lab in enumerate(labels):
        by_label[lab].append(i)
    for lab in sorted(by_label):
        idxs = by_label[lab][:]
        rng.shuffle(idxs)
        for j, idx in enumerate(idxs):
            folds[j % k].append(idx)
    return folds


def run_classifier(*, n_per_class: Optional[int], k_folds: int, seed: int, n_boot: int) -> dict[str, Any]:
    by_class = load_labeled_queries(seed=seed)
    pool_sizes = {c: len(v) for c, v in by_class.items()}
    texts, labels = balanced_sample(by_class, n_per_class=n_per_class)
    n_each = labels.count(INTENT_CLASSES[0])

    folds = stratified_folds(labels, k=k_folds, seed=seed)
    preds: list[Optional[str]] = [None] * len(labels)
    for fold in folds:
        test_idx = set(fold)
        tr_i = [i for i in range(len(labels)) if i not in test_idx]
        te_i = list(fold)
        fold_preds = _nearest_centroid_predict(
            [texts[i] for i in tr_i], [labels[i] for i in tr_i], [texts[i] for i in te_i]
        )
        for i, p in zip(te_i, fold_preds):
            preds[i] = p
    assert all(p is not None for p in preds)

    # confusion matrix + per-class accuracy (recall) + macro
    confusion = {a: Counter() for a in INTENT_CLASSES}
    correct_by_class: dict[str, list[int]] = {c: [] for c in INTENT_CLASSES}
    for lab, pred in zip(labels, preds):
        confusion[lab][pred] += 1
        correct_by_class[lab].append(1 if pred == lab else 0)

    rng = np.random.default_rng(seed)

    def boot_ci(vals: list[int]) -> dict[str, float]:
        arr = np.asarray(vals, dtype=np.float64)
        if arr.size == 0:
            return {"point": float("nan"), "lo": float("nan"), "hi": float("nan")}
        idx = rng.integers(0, arr.size, size=(n_boot, arr.size))
        means = arr[idx].mean(axis=1)
        return {
            "point": float(arr.mean()),
            "lo": float(np.percentile(means, 2.5)),
            "hi": float(np.percentile(means, 97.5)),
            "n": int(arr.size),
        }

    per_class = {c: boot_ci(correct_by_class[c]) for c in INTENT_CLASSES}
    macro_vals = [v for c in INTENT_CLASSES for v in correct_by_class[c]]
    macro = boot_ci(macro_vals)

    at_chance = sorted(c for c in INTENT_CLASSES if per_class[c]["lo"] <= CHANCE)
    kill = len(at_chance) >= 2

    return {
        "method": "lexical TF-IDF nearest-centroid (Rocchio), pure-numpy, stratified %d-fold CV" % k_folds,
        "note_env": "no torch/sklearn in venv; embedding-based variant unavailable → lexical $0 fallback proxy measured",
        "cost_usd": 0.0,
        "chance": CHANCE,
        "n_per_class": n_each,
        "n_total": len(labels),
        "pool_sizes": pool_sizes,
        "k_folds": k_folds,
        "seed": seed,
        "per_class_accuracy": per_class,
        "macro_accuracy": macro,
        "confusion_matrix": {a: dict(confusion[a]) for a in INTENT_CLASSES},
        "kill_check": {
            "rule": "accuracy at chance (CI lo <= %.3f) for >=2 classes" % CHANCE,
            "classes_at_chance": at_chance,
            "kill": kill,
            "disposition": (
                "KILL: prototype leans on the agent-passed label (PSD §II.A pref #1); "
                "internal classifier is a low-confidence fallback only."
                if kill
                else "PASS: classifier usable as a fallback above chance for >=4 classes."
            ),
        },
    }


# --------------------------------------------------------------------------- #
# Part 2 — mis-route arm context builders (oracle context isolates the route effect)
# --------------------------------------------------------------------------- #
def _locomo_items(intent: str, *, n: int, distractors: int, seed: int) -> list[dict[str, Any]]:
    """needle/multi_session/temporal items from LOCOMO: gold session(s) + same-conv
    distractor sessions. Oracle context isolates the *route* (summarize-away) effect
    from retrieval noise — both arms get identical raw chunks."""
    docs, gold = load_locomo()
    cls = "factoid" if intent == "needle" else intent
    # Stable per-intent seed offset (hashlib, NOT builtin hash() — which is
    # PYTHONHASHSEED-randomized per process and would break idempotent resume).
    offset = int(hashlib.sha256(intent.encode()).hexdigest(), 16) % 1000
    rng = random.Random(seed + offset)
    # group session docs by conversation for distractor sampling
    by_conv: dict[str, list[str]] = defaultdict(list)
    for did in docs:
        by_conv[did.split(":session_")[0]].append(did)
    cand = [g for g in gold if g["query_class"] == cls and g.get("answers")]
    rng.shuffle(cand)
    items: list[dict[str, Any]] = []
    for g in cand:
        gold_ids = [e["doc_id"] for e in g["required_evidence"] if e["doc_id"] in docs]
        if not gold_ids:
            continue
        conv = gold_ids[0].split(":session_")[0]
        pool = [d for d in by_conv[conv] if d not in gold_ids]
        rng.shuffle(pool)
        dist = pool[:distractors]
        # Gold chunks kept fuller (the needle must survive into the correct-route
        # baseline); distractors bounded tighter to keep per-call tokens small.
        chunks = [docs[d][:4000] for d in gold_ids] + [docs[d][:1500] for d in dist]
        items.append(
            {
                "qid": g["query_id"],
                "question": g["query"],
                "answers": g["answers"],
                "chunks": chunks,
                "n_gold_chunks": len(gold_ids),
            }
        )
        if len(items) >= n:
            break
    return items


def _musique_items(*, n: int, distractors: int, seed: int) -> list[dict[str, Any]]:
    rng = random.Random(seed + 7)
    rows: list[dict[str, Any]] = []
    with MUSIQUE.open(encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            r = json.loads(line)
            if r.get("answerable") and any(p.get("is_supporting") for p in r.get("paragraphs", [])):
                rows.append(r)
            if len(rows) >= n * 6:
                break
    rng.shuffle(rows)
    items: list[dict[str, Any]] = []
    for r in rows:
        sup = [p for p in r["paragraphs"] if p.get("is_supporting")]
        non = [p for p in r["paragraphs"] if not p.get("is_supporting")]
        rng.shuffle(non)
        chunks = [f"{p['title']}. {p['text']}"[:1800] for p in sup + non[:distractors]]
        answers = [r["answer"]] + list(r.get("answer_aliases") or [])
        items.append(
            {
                "qid": r["id"],
                "question": r["question"],
                "answers": [a for a in answers if str(a).strip()],
                "chunks": chunks,
                "n_gold_chunks": len(sup),
            }
        )
        if len(items) >= n:
            break
    return items


def build_misroute_items(intent: str, *, n: int, distractors: int, seed: int) -> list[dict[str, Any]]:
    if intent == "multi_hop":
        return _musique_items(n=n, distractors=distractors, seed=seed)
    return _locomo_items(intent, n=n, distractors=distractors, seed=seed)


# --------------------------------------------------------------------------- #
# Part 2 — LLM seam (gemini-flash-lite by default; backoff; ABSENT-safe)
# --------------------------------------------------------------------------- #
class LLM:
    """Minimal OpenAI-compatible ``/chat/completions`` client (stdlib urllib),
    temp 0 + fixed seed, with the 429/5xx backoff of :mod:`eval.m1_baseline_run`.
    Token usage accumulated for the BudgetLedger. Returns ``None`` on a failed /
    empty call (never a fabricated string)."""

    def __init__(self, *, model: str, max_tokens: int = 256, max_retries: int = 5) -> None:
        self.base_url = os.environ.get("R2_JUDGE_BASE_URL", "").rstrip("/")
        self.api_key = os.environ.get("R2_JUDGE_API_KEY", "")
        self.model = model
        self.max_tokens = max_tokens
        self.max_retries = max_retries
        self.last_prompt_tokens = 0
        self.last_completion_tokens = 0
        self.n_calls = 0
        self.n_errors = 0

    @property
    def available(self) -> bool:
        return os.environ.get(_R2_RUN_ENV) == "1" and bool(self.base_url)

    def complete(self, prompt: str) -> Optional[str]:
        payload = json.dumps(
            {
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0,
                "seed": 0,
                "max_completion_tokens": self.max_tokens,
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            self.base_url + "/chat/completions",
            data=payload,
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {self.api_key}"},
        )
        attempt = 0
        while True:
            try:
                with urllib.request.urlopen(req, timeout=120) as resp:  # noqa: S310
                    body = json.loads(resp.read().decode("utf-8"))
                break
            except Exception as exc:  # noqa: BLE001
                if attempt < self.max_retries and _is_retryable(exc):
                    cd = _retry_after_seconds(exc)
                    delay = min(cd + 5.0, _RETRY_HARD_CAP) if cd is not None else min(2.0**attempt, 30.0)
                    time.sleep(delay)
                    attempt += 1
                    continue
                self.n_errors += 1
                self.last_prompt_tokens = 0
                self.last_completion_tokens = 0
                return None
        usage = body.get("usage") or {}
        self.last_prompt_tokens = int(usage.get("prompt_tokens", 0))
        self.last_completion_tokens = int(usage.get("completion_tokens", 0))
        self.n_calls += 1
        try:
            content = body["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError):
            return None
        return str(content) if content and str(content).strip() else None


def _est_tokens(text: str) -> int:
    return max(1, len(text) // 4)


# --- prompts (tight to bound tokens) --------------------------------------- #
def _direct_prompt(question: str, chunks: list[str]) -> str:
    ctx = "\n\n".join(f"[{i+1}] {c}" for i, c in enumerate(chunks))
    return (
        "Answer the question using ONLY the context. Reply with the answer phrase "
        "only, no explanation. If the answer is not present, reply 'unknown'.\n\n"
        f"Context:\n{ctx}\n\nQuestion: {question}\nAnswer:"
    )


def _map_prompt(question: str, chunk: str) -> str:
    return (
        "Summarize the passage below as it relates to the question. Be concise "
        "(1-2 sentences). If the passage is not relevant, reply 'not relevant'.\n\n"
        f"Question: {question}\n\nPassage: {chunk}\n\nSummary:"
    )


def _reduce_prompt(question: str, summaries: list[str]) -> str:
    body = "\n".join(f"- {s}" for s in summaries)
    return (
        "Using ONLY the summaries below, answer the question. Reply with the answer "
        "phrase only, no explanation. If not answerable, reply 'unknown'.\n\n"
        f"Summaries:\n{body}\n\nQuestion: {question}\nAnswer:"
    )


def _judge_prompt(question: str, gold: list[str], candidate: str) -> str:
    golds = " | ".join(gold)
    return (
        "Judge whether the candidate answer is correct. It is CORRECT if it conveys "
        "any one of the gold answers (paraphrase/superset allowed). Reply with exactly "
        "one word: CORRECT or INCORRECT.\n\n"
        f"Question: {question}\nGold answer(s): {golds}\nCandidate: {candidate}\nVerdict:"
    )


def _norm(s: str) -> str:
    return " ".join(_TOKEN_RE.findall(s.lower()))


def _contains_match(gold: list[str], candidate: str) -> int:
    cand = _norm(candidate)
    return int(any(_norm(g) and _norm(g) in cand for g in gold))


def _call(llm: LLM, ledger: BudgetLedger, prompt: str, cost: dict[str, int]) -> Optional[str]:
    ledger.guard(llm.model, _est_tokens(prompt))
    out = llm.complete(prompt)
    pt = llm.last_prompt_tokens or _est_tokens(prompt)
    ct = llm.last_completion_tokens or 8
    ledger.record(llm.model, pt, ct)
    cost["prompt_tokens"] += pt
    cost["completion_tokens"] += ct
    cost["n_calls"] += 1
    return out


def run_misroute(
    *,
    llm: LLM,
    n_per_class: int,
    distractors: int,
    seed: int,
    n_boot: int,
    ledger: BudgetLedger,
    checkpoint: Optional[Path],
    resume: Optional[Path],
    classes: tuple[str, ...] = MISROUTE_CLASSES,
) -> dict[str, Any]:
    """Per (intent, route) answer accuracy + delta_C (route C vs correct retrieval).

    Routes: ``retrieval`` (correct for these classes — answer directly from the raw
    chunks) and ``C`` (map-reduce/QFS — per-chunk query-focused summary → reduce →
    answer). Both arms see identical chunks; the delta isolates the summarize-away
    cost. Resilient: per-record checkpoint, idempotent resume, BudgetLedger guard.
    """
    # resume state
    records: dict[str, dict[str, Any]] = {}
    cost = {"n_calls": 0, "prompt_tokens": 0, "completion_tokens": 0}
    src = resume or (checkpoint if (checkpoint and checkpoint.exists()) else None)
    if src and src.exists():
        blob = json.loads(src.read_text(encoding="utf-8"))
        records = blob.get("records", {})
        prior = blob.get("cost", {})
        for kk in cost:
            cost[kk] = int(prior.get(kk, 0))
        ledger.restore_spent(float(blob.get("spent_usd", 0.0)))

    def persist() -> None:
        if checkpoint is not None:
            _atomic_write_json(
                checkpoint,
                {"records": records, "cost": cost, "spent_usd": ledger.spent},
            )

    for intent in classes:
        items = build_misroute_items(intent, n=n_per_class, distractors=distractors, seed=seed)
        for it in items:
            key = f"{intent}||{it['qid']}"
            if key in records and records[key].get("done"):
                continue
            q, gold, chunks = it["question"], it["answers"], it["chunks"]
            # --- route: retrieval (direct) ---
            direct = _call(llm, ledger, _direct_prompt(q, chunks), cost) or ""
            # --- route: C (map-reduce / QFS) ---
            summaries = []
            for c in chunks:
                s = _call(llm, ledger, _map_prompt(q, c), cost) or ""
                if s and "not relevant" not in s.lower():
                    summaries.append(s)
            if not summaries:
                summaries = ["(no relevant summaries)"]
            reduced = _call(llm, ledger, _reduce_prompt(q, summaries), cost) or ""
            # --- grade both arms (same judge) ---
            jr = _call(llm, ledger, _judge_prompt(q, gold, direct), cost) or ""
            jc = _call(llm, ledger, _judge_prompt(q, gold, reduced), cost) or ""
            records[key] = {
                "intent": intent,
                "qid": it["qid"],
                "n_gold_chunks": it["n_gold_chunks"],
                "n_chunks": len(chunks),
                "retrieval_answer": direct,
                "c_answer": reduced,
                "retrieval_correct_judge": int(jr.strip().upper().startswith("CORRECT")),
                "c_correct_judge": int(jc.strip().upper().startswith("CORRECT")),
                "retrieval_correct_contains": _contains_match(gold, direct),
                "c_correct_contains": _contains_match(gold, reduced),
                "done": True,
            }
            persist()

    # --- aggregate ---
    rng = np.random.default_rng(seed)

    def acc_ci(vals: list[int]) -> dict[str, float]:
        arr = np.asarray(vals, dtype=np.float64)
        if arr.size == 0:
            return {"point": float("nan"), "lo": float("nan"), "hi": float("nan"), "n": 0}
        idx = rng.integers(0, arr.size, size=(n_boot, arr.size))
        m = arr[idx].mean(axis=1)
        return {"point": float(arr.mean()), "lo": float(np.percentile(m, 2.5)),
                "hi": float(np.percentile(m, 97.5)), "n": int(arr.size)}

    matrix: dict[str, Any] = {}
    for intent in classes:
        recs = [r for r in records.values() if r["intent"] == intent]
        rj = [r["retrieval_correct_judge"] for r in recs]
        cj = [r["c_correct_judge"] for r in recs]
        rc = [r["retrieval_correct_contains"] for r in recs]
        cc = [r["c_correct_contains"] for r in recs]
        # paired bootstrap on the judge delta
        pairs = np.array([(a, b) for a, b in zip(cj, rj)], dtype=np.float64) if recs else np.zeros((0, 2))
        if pairs.shape[0]:
            bidx = rng.integers(0, pairs.shape[0], size=(n_boot, pairs.shape[0]))
            deltas = (pairs[bidx, 0] - pairs[bidx, 1]).mean(axis=1)
            delta_ci = {
                "point": float((pairs[:, 0] - pairs[:, 1]).mean()),
                "lo": float(np.percentile(deltas, 2.5)),
                "hi": float(np.percentile(deltas, 97.5)),
            }
        else:
            delta_ci = {"point": float("nan"), "lo": float("nan"), "hi": float("nan")}
        matrix[intent] = {
            "correct_route": "retrieval",
            "route_retrieval_acc_judge": acc_ci(rj),
            "route_C_acc_judge": acc_ci(cj),
            "route_retrieval_acc_contains": acc_ci(rc),
            "route_C_acc_contains": acc_ci(cc),
            "delta_C_minus_retrieval_judge": delta_ci,
        }

    needle = matrix.get("needle", {}).get("delta_C_minus_retrieval_judge", {})
    return {
        "method": "oracle-context answer-quality: retrieval(direct) vs C(map-reduce/QFS); same judge both arms",
        "model": llm.model,
        "n_per_class": n_per_class,
        "distractors": distractors,
        "seed": seed,
        "classes_measured": list(classes),
        "deferred_cells": {
            "global": "reference-free sensemaking → decide_084 win-rate (EXP-B' priced arm), not gold-answer accuracy; "
            "global×retrieval (mis-route) + global×C (correct) deferred to the decide_084 axis",
            "within_retrieval_config": "needle→multi_session/temporal stacks differ by CONFIG (alpha/pool_n), not route → "
            "low-cost same-tier; the EXP-B'.5 forbidden-composition matrix owns those",
        },
        "matrix": matrix,
        "load_bearing_needle_to_C": {
            "prior": -0.362,
            "measured_delta": needle.get("point"),
            "ci": [needle.get("lo"), needle.get("hi")],
            "confirmed_negative": (needle.get("hi") is not None and needle.get("hi") < 0),
        },
        "cost": cost,
        "spent_usd": round(ledger.spent, 4),
    }


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #
def main(argv: Optional[list[str]] = None) -> int:
    p = argparse.ArgumentParser(description="EXP-Fr-acc base — classifier accuracy + mis-route cost")
    p.add_argument("--mode", default="base", choices=["base"])
    p.add_argument("--n-per-class", type=int, default=None, help="classifier balanced N/class (default=min class)")
    p.add_argument("--k-folds", type=int, default=5)
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--n-boot", type=int, default=2000)
    # mis-route arm
    p.add_argument("--misroute-n", type=int, default=25)
    p.add_argument("--distractors", type=int, default=3)
    p.add_argument(
        "--classes",
        default=None,
        help="comma-list to restrict mis-route classes (e.g. 'needle' for the "
        "needle-deep sensitivity arm: --classes needle --misroute-n 40 --distractors 8)",
    )
    p.add_argument("--model", default="gemini-flash-lite")
    p.add_argument("--max-usd", type=float, default=3.0)
    p.add_argument("--cheap-validate", action="store_true", help="tiny-N mis-route pipeline+cost probe")
    p.add_argument("--skip-misroute", action="store_true", help="classifier only ($0)")
    p.add_argument("--checkpoint", default=str(RUNS / "fracc-base.checkpoint.json"))
    p.add_argument("--resume", default=None)
    p.add_argument("--out", default=str(RUNS / "fracc-base-output.json"))
    args = p.parse_args(argv)

    print("[fracc] running $0 classifier (stratified CV)...")
    classifier = run_classifier(
        n_per_class=args.n_per_class, k_folds=args.k_folds, seed=args.seed, n_boot=args.n_boot
    )
    print(
        "[fracc] classifier macro=%.3f CI[%.3f,%.3f]; at-chance classes=%s; KILL=%s"
        % (
            classifier["macro_accuracy"]["point"],
            classifier["macro_accuracy"]["lo"],
            classifier["macro_accuracy"]["hi"],
            classifier["kill_check"]["classes_at_chance"],
            classifier["kill_check"]["kill"],
        )
    )

    misroute: dict[str, Any]
    if args.skip_misroute:
        misroute = {"status": "SKIPPED", "reason": "--skip-misroute"}
    else:
        llm = LLM(model=args.model)
        if not llm.available:
            misroute = {
                "status": "DEFERRED",
                "reason": "R2_RUN!=1 or judge env unset — priced mis-route arm not run",
            }
            print("[fracc] mis-route DEFERRED (R2_RUN!=1)")
        else:
            n = 2 if args.cheap_validate else args.misroute_n
            if args.cheap_validate:
                classes = ("needle",)
            elif args.classes:
                classes = tuple(c.strip() for c in args.classes.split(",") if c.strip())
            else:
                classes = MISROUTE_CLASSES
            ledger = BudgetLedger(opening_balance_usd=0.0, hard_cap_usd=args.max_usd, max_output_tokens=256)
            price_for(args.model)  # fail closed if unpinned
            ck = Path(args.checkpoint) if args.checkpoint else None
            rs = Path(args.resume) if args.resume else None
            print(f"[fracc] mis-route arm: model={args.model} n/class={n} classes={classes} cap=${args.max_usd}")
            try:
                misroute = run_misroute(
                    llm=llm, n_per_class=n, distractors=args.distractors, seed=args.seed,
                    n_boot=args.n_boot, ledger=ledger, checkpoint=ck, resume=rs, classes=classes,
                )
                misroute["status"] = "cheap_validate" if args.cheap_validate else "OK"
            except BudgetExceeded as e:
                misroute = {"status": "BUDGET_EXCEEDED", "reason": str(e), "spent_usd": round(ledger.spent, 4)}
            lb = misroute.get("load_bearing_needle_to_C", {})
            print(
                "[fracc] mis-route status=%s spent=$%s needle→C delta=%s"
                % (misroute.get("status"), misroute.get("spent_usd"), lb.get("measured_delta"))
            )

    report = {
        "schema": "0.8.11-fracc-base-v1",
        "slice": 20,
        "experiment": "EXP-Fr-acc base",
        "intent_classes": list(INTENT_CLASSES),
        "classifier": classifier,
        "misroute": misroute,
        "total_spent_usd": round(misroute.get("spent_usd", 0.0) or 0.0, 4),
    }
    Path(args.out).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"[fracc] wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
