"""M1 strong-baseline harness (Slice 5) — THE BAR for the 0.8.2 graph study.

Binding spec: ``dev/plans/plan-0.8.2.md`` §4 (Slice 5) and the SIGNED
``dev/design/0.8.2-m1-multihop-harness.md`` (arms, endpoint, baseline, power
plan). This module establishes the strong-baseline answer-accuracy numbers the
graph arm (Slice 15/20) must beat: the four baseline arms over the per-question
MuSiQue-Ans distractor pool, the identical-answerer protocol, and the EM/F1
scorer stratified per hop (2/3/4) + pooled ≥3-hop + the unanswerable contrast set.

The four baseline arms (design §2):
  * ``bm25``          — lexical BM25 over the question's ~20 paragraphs.
  * ``passage_dense`` — passage-level dense retrieval (bge-small-en-v1.5, the
                        engine's pinned embedder), run **in-harness** (see below).
  * ``fused``         — RRF(bm25, passage_dense), **k=60 pinned**.
  * ``fused_rerank``  — the **live engine cross-encoder** (TinyBERT-L2) via
                        ``Engine.search(rerank_depth=200)`` — the **fixed
                        comparator** (design amendment 6).

**Why the dense / fused arms are in-harness (justified deviation, logged).**
The canonical extension built for this slice carries the ``default-reranker``
feature (so the CE is live) but **not** ``test-hooks``; there is also no
*production* Python surface to register a node vector kind (the p0a
``vector-kind-binding-gap`` reserved-followup). Consequently the engine
vector-projects **zero** ``doc`` nodes (``_fathomdb_vector_kinds`` is empty),
so ``Engine.search`` returns a **text-branch-only** pool and a real dense arm
cannot be isolated from the engine. Rebuilding the ``.so`` with ``test-hooks``
would drop ``default-reranker`` and silently kill the comparator — exactly what
the slice prompt forbids. The design explicitly sanctions building an arm
in-harness when it cannot be isolated from the engine; we therefore build the
dense arm in-harness with a pure-numpy forward pass of the **same** pinned model
(``bge-small-en-v1.5``, CLS-pooled + L2-normalised), and reuse the **live engine
CE** for ``fused_rerank``. Because a cross-encoder scores each (query, passage)
pair independently, the reranked order over a fixed candidate **set** is
input-order-independent; the engine's text pool ≈ the full per-question passage
set, so ``fused_rerank`` == "the fused pool re-ordered by the CE" faithfully.
``n_pool`` per question is recorded (no silent cap).

Footprint: CPU-only, offline, deterministic; the answerer LLM is the one priced
seam (gated, reused from ``r2_parity_eval`` / ``p0a_base_retrieval``).
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
import struct
import tempfile
from collections import defaultdict
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional, Protocol, runtime_checkable

import numpy as np

from eval.p0a_base_retrieval import AirlockAnswerer  # noqa: F401 (re-exported)
from eval.r2_parity_eval import BaseAnswerer, StubAnswerer  # noqa: F401 (re-exported)

# --------------------------------------------------------------------------- #
# Frozen provenance + arm constants
# --------------------------------------------------------------------------- #

#: The Slice-4 pinned MuSiQue corpus hash. Asserted on load — a wrong/unpinned
#: corpus must never yield a citable baseline number (the R2 COR-2 invariant).
MUSIQUE_HASH = "3cff37fd7221506a343a125cf7ca20aab7cd09877e376122da9627e1b935b26f"

#: RRF fusion constant — pinned at k=60 across every fused arm (design §3).
RRF_K = 60

#: Cross-encoder rerank depth — the 0.8.1 R0 recommendation (design §2).
RERANK_DEPTH = 200

#: The four baseline arms; ``fused_rerank`` is the fixed comparator.
ARM_NAMES: tuple[str, ...] = ("bm25", "passage_dense", "fused", "fused_rerank")
COMPARATOR_ARM = "fused_rerank"

#: Default answerer model ids. The slice prompt names ``gemini-3.1-pro-preview``
#: (strong) and ``gemini-2.5-flash-lite`` (cheap-validate); neither exact id is
#: served by the local airlock proxy, so the closest available ids are used and
#: this mapping is recorded in every artifact (env-overridable).
STRONG_READER_DEFAULT = os.environ.get("M1_STRONG_READER", "gemini-3.1-pro")
CHEAP_READER_DEFAULT = os.environ.get("M1_CHEAP_READER", "gemini-3.1-flash-lite")

#: The pinned passage-dense embedder (the engine's default), used in-harness.
_EMBEDDER_CACHE = Path.home() / ".cache" / "fathomdb" / "embedders" / "0b2926f8a9b1"
#: BGE query instruction prefix (bge-small-en-v1.5 retrieval convention).
_BGE_QUERY_PREFIX = "Represent this sentence for searching relevant passages: "


# --------------------------------------------------------------------------- #
# Corpus
# --------------------------------------------------------------------------- #


@runtime_checkable
class EncoderProtocol(Protocol):
    """Minimal passage encoder seam (so tests can inject a deterministic fake)."""

    def encode(self, text: str) -> "np.ndarray": ...


@runtime_checkable
class RerankerProtocol(Protocol):
    """Per-question CE reranker seam: returns (ranked passage idx, n_pool)."""

    def rank(self, query: str, passages: Sequence["Paragraph"]) -> tuple[list[int], int]: ...


@dataclass(frozen=True)
class Paragraph:
    idx: int
    title: str
    text: str
    is_supporting: bool

    @property
    def body(self) -> str:
        return f"{self.title}. {self.text}"


@dataclass(frozen=True)
class Question:
    id: str
    question: str
    hop_count: int
    answer: str
    answer_aliases: tuple[str, ...]
    answerable: bool
    paragraphs: tuple[Paragraph, ...]

    @property
    def golds(self) -> tuple[str, ...]:
        return (self.answer, *self.answer_aliases) if self.answer else self.answer_aliases


def corpus_hash(path: str | Path) -> str:
    """sha256 over the raw JSONL bytes (the acquire-script line hashing)."""
    h = hashlib.sha256()
    with Path(path).open("rb") as fh:
        for line in fh:
            h.update(line)
    return h.hexdigest()


def load_musique(path: str | Path, *, assert_hash: bool = True) -> list[Question]:
    """Load the materialized MuSiQue corpus, asserting the pinned ``musique_hash``.

    Refuses to return questions on an unpinned corpus (the COR-2 invariant) so a
    baseline number can never be cited against the wrong corpus.
    """
    p = Path(path)
    if assert_hash:
        actual = corpus_hash(p)
        if actual != MUSIQUE_HASH:
            raise ValueError(
                f"musique_hash {actual!r} != pinned {MUSIQUE_HASH!r} "
                "(refusing to produce baseline numbers on an unpinned corpus)"
            )
    out: list[Question] = []
    with p.open(encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            paras = tuple(
                Paragraph(
                    idx=int(pp["idx"]),
                    title=str(pp["title"]),
                    text=str(pp["text"]),
                    is_supporting=bool(pp["is_supporting"]),
                )
                for pp in r["paragraphs"]
            )
            out.append(
                Question(
                    id=str(r["id"]),
                    question=str(r["question"]),
                    hop_count=int(r["hop_count"]),
                    answer=str(r["answer"]),
                    answer_aliases=tuple(str(a) for a in (r.get("answer_aliases") or [])),
                    answerable=bool(r["answerable"]),
                    paragraphs=paras,
                )
            )
    return out


# --------------------------------------------------------------------------- #
# Arm 1 — BM25 (in-harness lexical)
# --------------------------------------------------------------------------- #


def _tokenize(text: str) -> list[str]:
    return [t for t in re.findall(r"[a-z0-9]+", text.lower()) if len(t) >= 2]


def bm25_rank(query: str, passages: Sequence[Paragraph], *, k1: float = 1.5, b: float = 0.75) -> list[int]:
    """Rank passage indices (into ``passages``) by BM25. Deterministic."""
    n = max(len(passages), 1)
    doc_toks = [_tokenize(p.body) for p in passages]
    doc_len = [len(t) for t in doc_toks]
    avgdl = max(sum(doc_len) / n, 1e-9)
    df: dict[str, int] = defaultdict(int)
    for toks in doc_toks:
        for t in set(toks):
            df[t] += 1
    idf = {t: math.log(1 + (n - d + 0.5) / (d + 0.5)) for t, d in df.items()}
    q_terms = set(_tokenize(query))
    scores: list[tuple[float, int]] = []
    for i, toks in enumerate(doc_toks):
        counts: dict[str, int] = defaultdict(int)
        for t in toks:
            counts[t] += 1
        s = 0.0
        for t in q_terms:
            f = counts.get(t, 0)
            if f == 0:
                continue
            denom = f + k1 * (1 - b + b * doc_len[i] / avgdl)
            s += idf.get(t, 0.0) * (f * (k1 + 1)) / denom
        scores.append((s, i))
    # Stable: higher score first, ties broken by original index.
    scores.sort(key=lambda si: (-si[0], si[1]))
    return [i for _, i in scores]


# --------------------------------------------------------------------------- #
# Arm 2 — passage-dense (in-harness bge-small-en-v1.5, pure numpy)
# --------------------------------------------------------------------------- #


class BGEEncoder:
    """Pure-numpy bge-small-en-v1.5 forward pass over the cached safetensors.

    Same pinned model the engine uses (CLS pooling + L2 normalise). CPU-only,
    offline, deterministic. Lazily loads the weights + tokenizer on first encode.
    """

    H = 384
    NUM_HEADS = 12
    NUM_LAYERS = 12
    EPS = 1e-12

    def __init__(self, cache_dir: Path = _EMBEDDER_CACHE) -> None:
        self._dir = cache_dir
        self._w: Optional[dict[str, np.ndarray]] = None
        self._tok: Any = None

    @property
    def available(self) -> bool:
        return (self._dir / "model.safetensors").exists() and (self._dir / "tokenizer.json").exists()

    def _ensure(self) -> None:
        if self._w is not None:
            return
        self._w = self._load_safetensors(self._dir / "model.safetensors")
        from tokenizers import Tokenizer  # type: ignore[import]  # harness dep

        self._tok = Tokenizer.from_file(str(self._dir / "tokenizer.json"))

    @staticmethod
    def _load_safetensors(p: Path) -> dict[str, np.ndarray]:
        data = p.read_bytes()
        n = struct.unpack("<Q", data[:8])[0]
        header = json.loads(data[8 : 8 + n])
        base = 8 + n
        dtmap = {"F32": np.float32, "F16": np.float16, "I64": np.int64}
        out: dict[str, np.ndarray] = {}
        for key, meta in header.items():
            if key == "__metadata__":
                continue
            s, e = meta["data_offsets"]
            arr = np.frombuffer(data[base + s : base + e], dtype=dtmap[meta["dtype"]]).reshape(
                meta["shape"]
            )
            out[key] = arr.astype(np.float32)
        return out

    def _ln(self, x: np.ndarray, w: np.ndarray, b: np.ndarray) -> np.ndarray:
        m = x.mean(-1, keepdims=True)
        v = x.var(-1, keepdims=True)
        return (x - m) / np.sqrt(v + self.EPS) * w + b

    @staticmethod
    def _gelu(x: np.ndarray) -> np.ndarray:
        return 0.5 * x * (1 + np.tanh(np.sqrt(2 / np.pi) * (x + 0.044715 * x**3)))

    def _lin(self, x: np.ndarray, name: str) -> np.ndarray:
        w = self._w  # type: ignore[assignment]
        assert w is not None
        return x @ w[name + ".weight"].T + w[name + ".bias"]

    def encode(self, text: str) -> np.ndarray:
        self._ensure()
        w = self._w
        assert w is not None and self._tok is not None
        ids = self._tok.encode(text).ids[:512]
        idsa = np.array(ids)
        x = w["embeddings.word_embeddings.weight"][idsa]
        x = x + w["embeddings.position_embeddings.weight"][: len(ids)]
        x = x + w["embeddings.token_type_embeddings.weight"][0]
        x = self._ln(x, w["embeddings.LayerNorm.weight"], w["embeddings.LayerNorm.bias"])
        t = len(ids)
        for layer in range(self.NUM_LAYERS):
            p = f"encoder.layer.{layer}."
            q = self._lin(x, p + "attention.self.query").reshape(t, self.NUM_HEADS, -1).transpose(1, 0, 2)
            k = self._lin(x, p + "attention.self.key").reshape(t, self.NUM_HEADS, -1).transpose(1, 0, 2)
            v = self._lin(x, p + "attention.self.value").reshape(t, self.NUM_HEADS, -1).transpose(1, 0, 2)
            hd = q.shape[-1]
            sc = q @ k.transpose(0, 2, 1) / np.sqrt(hd)
            sc = sc - sc.max(-1, keepdims=True)
            a = np.exp(sc)
            a = a / a.sum(-1, keepdims=True)
            ctx = (a @ v).transpose(1, 0, 2).reshape(t, self.H)
            ao = self._lin(ctx, p + "attention.output.dense")
            x = self._ln(
                x + ao, w[p + "attention.output.LayerNorm.weight"], w[p + "attention.output.LayerNorm.bias"]
            )
            inter = self._gelu(self._lin(x, p + "intermediate.dense"))
            o = self._lin(inter, p + "output.dense")
            x = self._ln(x + o, w[p + "output.LayerNorm.weight"], w[p + "output.LayerNorm.bias"])
        cls = x[0]
        return cls / (np.linalg.norm(cls) + 1e-9)


def dense_rank(query: str, passages: Sequence[Paragraph], encoder: EncoderProtocol) -> list[int]:
    """Rank passage indices by cosine of the bge-small query/passage embeddings."""
    qv = encoder.encode(_BGE_QUERY_PREFIX + query)
    pv = np.array([encoder.encode(p.body) for p in passages])
    sims = pv @ qv
    order = np.argsort(-sims, kind="stable")
    return [int(i) for i in order]


# --------------------------------------------------------------------------- #
# Arm 3 — fused (RRF k=60, in-harness)
# --------------------------------------------------------------------------- #


def rrf_fuse(rankings: Sequence[Sequence[int]], *, k: int = RRF_K) -> list[int]:
    """Reciprocal-rank fusion of several rankings (lists of item ids). k pinned."""
    score: dict[int, float] = defaultdict(float)
    for ranking in rankings:
        for rank, item in enumerate(ranking):
            score[item] += 1.0 / (k + rank + 1)
    # higher fused score first; tie-break by smallest id for determinism.
    return [i for i, _ in sorted(score.items(), key=lambda kv: (-kv[1], kv[0]))]


# --------------------------------------------------------------------------- #
# Arm 4 — fused + CE rerank (the LIVE engine cross-encoder)
# --------------------------------------------------------------------------- #


class EngineReranker:
    """Reranks a question's passages via the live engine cross-encoder.

    Indexes the passages into a throwaway engine and calls
    ``search(rerank_depth=RERANK_DEPTH)``. Returns ranked passage indices plus
    the realised candidate-pool size (``n_pool``) for the no-silent-cap log.
    The CE scores each (query, passage) pair independently, so the order over the
    pool is input-order-independent — this is "the fused pool re-ordered by CE".
    """

    def __init__(self, *, rerank_depth: int = RERANK_DEPTH) -> None:
        self._depth = rerank_depth
        self._engine_cls: Any = None

    @property
    def available(self) -> bool:
        try:
            from fathomdb.engine import Engine  # noqa: F401
        except Exception:
            return False
        return True

    def rank(self, query: str, passages: Sequence[Paragraph]) -> tuple[list[int], int]:
        from fathomdb.engine import Engine

        body_to_idx: dict[str, int] = {p.body: i for i, p in enumerate(passages)}
        db_path = tempfile.mktemp(suffix=".sqlite")
        eng = Engine.open(db_path, use_default_embedder=False)
        try:
            eng.write([{"kind": "doc", "body": p.body} for p in passages])
            eng.drain(timeout_s=60)
            res = eng.search(query, rerank_depth=self._depth)
            ranked: list[int] = []
            seen: set[int] = set()
            for hit in res.results:
                i = body_to_idx.get(hit.body)
                if i is None or i in seen:
                    continue
                seen.add(i)
                ranked.append(i)
            n_pool = len(ranked)
            # Append any passages the text pool dropped (kept last, original order)
            # so the arm always returns a full ranking; counted out of n_pool.
            for i in range(len(passages)):
                if i not in seen:
                    ranked.append(i)
            return ranked, n_pool
        finally:
            eng.close()
            with __import__("contextlib").suppress(FileNotFoundError):
                os.unlink(db_path)


# --------------------------------------------------------------------------- #
# Retrieval — all four arms, identical passage pool per question
# --------------------------------------------------------------------------- #


def retrieve_arms(
    question: Question,
    encoder: EncoderProtocol,
    reranker: RerankerProtocol,
) -> dict[str, Any]:
    """Return the ranked passage-index list for each of the four arms + n_pool.

    Every arm ranks the *identical* per-question passage pool (retrieval is the
    only variable); the answerer is applied identically downstream.
    """
    paras = question.paragraphs
    bm = bm25_rank(question.question, paras)
    dn = dense_rank(question.question, paras, encoder)
    fused = rrf_fuse([bm, dn])
    rer, n_pool = reranker.rank(question.question, paras)
    return {
        "bm25": bm,
        "passage_dense": dn,
        "fused": fused,
        "fused_rerank": rer,
        "n_pool": n_pool,
        "n_paragraphs": len(paras),
    }


# --------------------------------------------------------------------------- #
# Scorer — SQuAD-style EM / token-F1, stratified per hop + pooled ≥3-hop
# --------------------------------------------------------------------------- #

_ARTICLES = re.compile(r"\b(a|an|the)\b")


def normalize_squad(text: str) -> str:
    s = text.lower()
    s = re.sub(r"[^a-z0-9 ]+", " ", s)
    s = _ARTICLES.sub(" ", s)
    return " ".join(s.split())


def em_score(pred: Optional[str], golds: Sequence[str]) -> float:
    if pred is None:
        return 0.0
    p = normalize_squad(pred)
    return 1.0 if any(p == normalize_squad(g) for g in golds if g) else 0.0


def f1_score(pred: Optional[str], golds: Sequence[str]) -> float:
    if pred is None:
        return 0.0
    p_toks = normalize_squad(pred).split()
    best = 0.0
    for g in golds:
        if not g:
            continue
        g_toks = normalize_squad(g).split()
        if not p_toks or not g_toks:
            best = max(best, 1.0 if p_toks == g_toks else 0.0)
            continue
        gc: dict[str, int] = defaultdict(int)
        for t in g_toks:
            gc[t] += 1
        overlap = 0
        pc: dict[str, int] = defaultdict(int)
        for t in p_toks:
            pc[t] += 1
        for t, c in pc.items():
            overlap += min(c, gc.get(t, 0))
        if overlap == 0:
            continue
        prec = overlap / len(p_toks)
        rec = overlap / len(g_toks)
        best = max(best, 2 * prec * rec / (prec + rec))
    return best


def is_confident_answer(pred: Optional[str]) -> bool:
    """An unanswerable-set question is answered *confidently* when the reader did
    not abstain (``normalize_answer`` already mapped abstentions to ``None``)."""
    return pred is not None


# --------------------------------------------------------------------------- #
# Pipeline
# --------------------------------------------------------------------------- #


@dataclass
class QuestionResult:
    qid: str
    hop_count: int
    answerable: bool
    n_pool: int
    n_paragraphs: int
    answers: dict[str, Optional[str]] = field(default_factory=dict)  # arm -> answer
    em: dict[str, float] = field(default_factory=dict)
    f1: dict[str, float] = field(default_factory=dict)
    confident: dict[str, bool] = field(default_factory=dict)


def run_baseline(
    questions: Sequence[Question],
    answerer: BaseAnswerer,
    *,
    k: int = 10,
    encoder: Optional[EncoderProtocol] = None,
    reranker: Optional[RerankerProtocol] = None,
    arms: Sequence[str] = ARM_NAMES,
    progress: Any = None,
) -> dict[str, Any]:
    """Run the four-arm strong baseline over ``questions`` with one shared answerer.

    The **identical-answerer protocol**: the same ``answerer`` instance and the
    same top-``k`` passage budget are used for every arm — retrieval is the only
    variable. Returns a structured artifact with per-question paired records,
    per-hop (2/3/4) + pooled ≥3-hop EM/F1, and the unanswerable confident-answer
    rate.
    """
    encoder = encoder or BGEEncoder()
    reranker = reranker or EngineReranker()
    answerer_available = answerer.available
    results: list[QuestionResult] = []

    for n, q in enumerate(questions):
        arm_rankings = retrieve_arms(q, encoder, reranker)
        qr = QuestionResult(
            qid=q.id,
            hop_count=q.hop_count,
            answerable=q.answerable,
            n_pool=int(arm_rankings["n_pool"]),
            n_paragraphs=int(arm_rankings["n_paragraphs"]),
        )
        for arm in arms:
            ranked_idx = arm_rankings[arm][:k]
            context = [q.paragraphs[i].body for i in ranked_idx]
            ans = answerer.answer(q.question, context) if answerer_available else None
            qr.answers[arm] = ans
            if q.answerable:
                qr.em[arm] = em_score(ans, q.golds)
                qr.f1[arm] = f1_score(ans, q.golds)
            else:
                qr.confident[arm] = is_confident_answer(ans)
        results.append(qr)
        if progress is not None:
            progress(n + 1, len(questions), qr)

    return _aggregate(results, answerer, k=k, arms=arms)


def _mean(values: list[float]) -> Optional[float]:
    return round(sum(values) / len(values), 6) if values else None


def _var(values: list[float]) -> Optional[float]:
    if len(values) < 2:
        return None
    m = sum(values) / len(values)
    return round(sum((v - m) ** 2 for v in values) / (len(values) - 1), 6)


def _aggregate(
    results: list[QuestionResult],
    answerer: BaseAnswerer,
    *,
    k: int,
    arms: Sequence[str],
) -> dict[str, Any]:
    answerable = [r for r in results if r.answerable]
    unanswerable = [r for r in results if not r.answerable]

    def cell(rs: list[QuestionResult], arm: str) -> dict[str, Any]:
        ems = [r.em[arm] for r in rs if arm in r.em]
        f1s = [r.f1[arm] for r in rs if arm in r.f1]
        return {
            "n": len(rs),
            "em": _mean(ems),
            "f1": _mean(f1s),
            "em_var": _var(ems),
            "f1_var": _var(f1s),
        }

    per_hop: dict[str, dict[str, Any]] = {}
    for hop in (2, 3, 4):
        rs = [r for r in answerable if r.hop_count == hop]
        per_hop[str(hop)] = {arm: cell(rs, arm) for arm in arms}

    pooled3 = [r for r in answerable if r.hop_count >= 3]
    pooled_ge3 = {arm: cell(pooled3, arm) for arm in arms}

    # Unanswerable contrast set — confident-answer rate per arm.
    unans = {
        arm: {
            "n": len(unanswerable),
            "confident_answer_rate": _mean(
                [1.0 if r.confident.get(arm, False) else 0.0 for r in unanswerable]
            ),
        }
        for arm in arms
    }

    # Per-question paired records (so Slice 20 can paired-bootstrap).
    paired = [
        {
            "qid": r.qid,
            "hop_count": r.hop_count,
            "answerable": r.answerable,
            "n_pool": r.n_pool,
            "n_paragraphs": r.n_paragraphs,
            "em": r.em,
            "f1": r.f1,
            "confident": r.confident,
            "answers": r.answers,
        }
        for r in results
    ]

    return {
        "schema": "0.8.2-m1-baseline-v1",
        "musique_hash": MUSIQUE_HASH,
        "arms": list(arms),
        "comparator_arm": COMPARATOR_ARM,
        "rrf_k": RRF_K,
        "rerank_depth": RERANK_DEPTH,
        "top_k": k,
        "answerer_model": answerer.model_id,
        "answerer_available": answerer.available,
        "identical_answerer": True,
        "n_questions": len(results),
        "n_answerable": len(answerable),
        "n_unanswerable": len(unanswerable),
        "primary_cell_pooled_ge3hop": pooled_ge3,
        "per_hop": per_hop,
        "unanswerable_contrast": unans,
        "paired_records": paired,
    }
