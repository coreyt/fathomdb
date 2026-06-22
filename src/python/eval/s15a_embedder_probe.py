"""0.8.3 Slice-15a — embedder-ceiling probe (runner + pure helpers).

This module makes the §5-frozen 15a probe (design
``dev/design/0.8.3-slice-15a-embedder-probe.md``) executable, deterministic, and
$0/CPU/offline. It **consumes** the frozen gate
:func:`eval.decision_rule_083.probe_15a_pass` / :data:`eval.decision_rule_083.EU7_FLOOR`
and must NOT reimplement or relax them.

Design map (authoritative = the on-branch design doc):
* §3 — base (CLS-corrected bge-small) + 4 candidates, per-model pooling/prefix.
* §4 — eu8 doc-id recall@10 over the frozen IR snapshot; the model-INDEPENDENT
  hard subset (BM25 Okapi k1=1.2/b=0.75, cap=50, strict all-of).
* §5 — per-embedder metrics, projected_eu7 (mean-centering ON, Hamming K=192),
  paired bootstrap margin CIs (the ONLY gated quantity).
* §9 — outputs.

Heavy deps (``torch`` / ``transformers``) are imported **lazily inside**
:func:`embed_texts` so the pure-logic helpers + ``pyright`` run without them. The
pure helpers below use only stdlib + numpy.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import statistics
import time
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any

import numpy as np

from eval.decision_rule_083 import probe_15a_pass

# --------------------------------------------------------------------------- #
# Paths (frozen).
# --------------------------------------------------------------------------- #

#: Worktree root (src/python/eval/this -> parents[3]).
_WORKTREE_ROOT = Path(__file__).resolve().parents[3]

#: The frozen IR snapshot (committed in the worktree).
SNAPSHOT_PATH = _WORKTREE_ROOT / "tests" / "corpus" / "snapshot.json"

#: The frozen corpus hash the snapshot+gold both pin (design §4).
EXPECTED_CORPUS_HASH = (
    "fe973fcd49fbbda083158f69fe720f17858ab8528e171fa2188eec84131c7d4e"
)

#: The negative class (excluded from the recall denominator, design §4/§d).
NEGATIVE_CLASS = "negative"

#: Hard-subset frozen knobs (design §4).
HARD_CAP = 50
BM25_K1 = 1.2
BM25_B = 0.75

#: 1-bit eu7 proxy fanout (production ADR-0.7.0).
PROJECTED_EU7_K = 192

#: cpu_feasible multiplier vs the base (design §5).
CPU_FEASIBLE_MULT = 3.0

#: Default fixed seeds (design §5/§7).
DEFAULT_BOOTSTRAP_SEED = 0x15A
DEFAULT_BOOTSTRAP_RESAMPLES = 1000


def _resolve_data_root() -> Path:
    """Resolve the corpus-data root.

    The corpus payloads (``data/corpus-data/**``) are EVAL-ONLY and gitignored, so
    they are NOT physically present in an ephemeral worktree. Resolve in order:
    ``$FDB_DATA_ROOT`` env, then the worktree itself (if a developer placed data
    there), then the canonical checkout. The chosen root must contain
    ``data/corpus-data/raw``.
    """
    candidates: list[Path] = []
    env = os.environ.get("FDB_DATA_ROOT")
    if env:
        candidates.append(Path(env))
    candidates.append(_WORKTREE_ROOT)
    candidates.append(Path("/home/coreyt/projects/fathomdb"))
    for root in candidates:
        if (root / "data" / "corpus-data" / "raw").is_dir():
            return root
    raise FileNotFoundError(
        "could not locate data/corpus-data/raw; set $FDB_DATA_ROOT "
        f"(tried: {[str(c) for c in candidates]})"
    )


# --------------------------------------------------------------------------- #
# Model configuration table (design §3).
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class ModelCfg:
    """One embedder's frozen pooling/prefix/feasibility config (design §3)."""

    name: str
    hf_id: str
    dim: int
    pooling: str  # "cls" | "mean"
    query_prefix: str
    passage_prefix: str
    in_library_feasible: bool
    trust_remote_code: bool = False
    revision: str | None = None
    is_base: bool = False


#: The BGE query prefix (design §3).
_BGE_QUERY_PREFIX = "Represent this sentence for searching relevant passages: "

#: Base + 4 candidates, exactly per design §3. The base is the CLS-CORRECTED
#: bge-small (the Slice-20 bug was MEAN pooling for a CLS-trained BGE).
MODELS: dict[str, ModelCfg] = {
    "bge-small": ModelCfg(
        name="bge-small",
        hf_id="BAAI/bge-small-en-v1.5",
        dim=384,
        pooling="cls",
        query_prefix=_BGE_QUERY_PREFIX,
        passage_prefix="",
        in_library_feasible=True,
        is_base=True,
    ),
    "bge-base": ModelCfg(
        name="bge-base",
        hf_id="BAAI/bge-base-en-v1.5",
        dim=768,
        pooling="cls",
        query_prefix=_BGE_QUERY_PREFIX,
        passage_prefix="",
        in_library_feasible=True,
    ),
    "gte-base": ModelCfg(
        name="gte-base",
        hf_id="Alibaba-NLP/gte-base-en-v1.5",
        dim=768,
        pooling="cls",
        query_prefix="",
        passage_prefix="",
        # No candle-native GTE encoder -> measured for attribution but NOT
        # selectable as the Slice-20 embedder (design §2/§3, codex Q5).
        in_library_feasible=False,
        trust_remote_code=True,
    ),
    "e5-base-v2": ModelCfg(
        name="e5-base-v2",
        hf_id="intfloat/e5-base-v2",
        dim=768,
        pooling="mean",
        query_prefix="query: ",
        passage_prefix="passage: ",
        # BERT-base checkpoint loadable via candle's BERT path with mean pooling;
        # VERIFY at pin time, else downgrade (design §3 note 2).
        in_library_feasible=True,
    ),
    "nomic": ModelCfg(
        name="nomic",
        hf_id="nomic-ai/nomic-embed-text-v1.5",
        dim=768,
        pooling="mean",
        query_prefix="search_query: ",
        passage_prefix="search_document: ",
        in_library_feasible=True,
        trust_remote_code=True,
    ),
}

BASE_NAME = "bge-small"
CANDIDATE_NAMES = ("bge-base", "gte-base", "e5-base-v2", "nomic")


def pooling_prefix_table() -> dict[str, dict[str, Any]]:
    """The frozen pooling/prefix/feasibility table (for the output record + tests)."""
    return {
        cfg.name: {
            "hf_id": cfg.hf_id,
            "dim": cfg.dim,
            "pooling": cfg.pooling,
            "query_prefix": cfg.query_prefix,
            "passage_prefix": cfg.passage_prefix,
            "in_library_feasible": cfg.in_library_feasible,
            "trust_remote_code": cfg.trust_remote_code,
        }
        for cfg in MODELS.values()
    }


# --------------------------------------------------------------------------- #
# Pure vector / recall helpers (numpy, no torch).
# --------------------------------------------------------------------------- #


def l2_normalize(mat: np.ndarray) -> np.ndarray:
    """L2-normalize rows; zero rows pass through unchanged (no div-by-zero)."""
    arr = np.asarray(mat, dtype=np.float32)
    norms = np.linalg.norm(arr, axis=1, keepdims=True)
    norms = np.where(norms == 0.0, 1.0, norms)
    return (arr / norms).astype(np.float32)


def cosine_topk(query_vecs: np.ndarray, doc_vecs: np.ndarray, k: int) -> np.ndarray:
    """Top-``k`` doc indices per query by cosine (rows assumed L2-normalized).

    Returns an ``(n_queries, min(k, n_docs))`` int array; ties broken by index
    (stable) so the ranking is deterministic.
    """
    docs = np.asarray(doc_vecs, dtype=np.float32)
    queries = np.asarray(query_vecs, dtype=np.float32)
    n_docs = docs.shape[0]
    kk = min(k, n_docs)
    if kk <= 0:
        return np.empty((queries.shape[0], 0), dtype=np.int64)
    sims = queries @ docs.T
    # stable descending sort -> deterministic ties.
    order = np.argsort(-sims, axis=1, kind="stable")
    return order[:, :kk].astype(np.int64)


def strict_recall_hit(retrieved_ids: Sequence[str], required_ids: Sequence[str]) -> float:
    """Strict all-of recall: 1.0 iff EVERY required id is in ``retrieved_ids``.

    Mirrors ``support/ir_eval.rs::evidence_recall_at_k`` (``required_n == 0`` or
    all-hit => 1.0). An empty required set (a negative-class query) returns 1.0,
    but callers exclude negatives from the recall mean (design §4/§d).
    """
    required = list(required_ids)
    if not required:
        return 1.0
    top = set(retrieved_ids)
    return 1.0 if all(d in top for d in required) else 0.0


def graded_rank_of(retrieved_ids: Sequence[str], required_ids: Sequence[str]) -> float:
    """Median 0-based rank of the required docs in ``retrieved_ids``.

    A required doc absent from ``retrieved_ids`` contributes ``inf``. Returns the
    median over the required docs (used for the hard-subset median-gold-rank
    report). ``inf`` if no required docs given.
    """
    required = list(required_ids)
    if not required:
        return math.inf
    pos = {d: i for i, d in enumerate(retrieved_ids)}
    ranks = [float(pos.get(d, math.inf)) for d in required]
    return float(statistics.median(ranks))


# --------------------------------------------------------------------------- #
# BM25 (Okapi k1=1.2, b=0.75) — deterministic, pure-Python (design §4).
# --------------------------------------------------------------------------- #

#: The pinned tokenizer: lowercase + regex ``[a-z0-9]+`` (design §4). NOT FTS5;
#: the FTS5-tokenizer-mismatch caveat is recorded, not gated (design §11.1).
_TOKEN_RE = re.compile(r"[a-z0-9]+")


def tokenize(text: str) -> list[str]:
    """Lowercase + regex ``[a-z0-9]+`` tokenizer (frozen, design §4)."""
    return _TOKEN_RE.findall(text.lower())


class BM25Okapi:
    """Deterministic Okapi BM25 (k1, b) over a fixed tokenized corpus."""

    def __init__(
        self,
        corpus_tokens: Sequence[Sequence[str]],
        *,
        k1: float = BM25_K1,
        b: float = BM25_B,
    ) -> None:
        self.k1 = k1
        self.b = b
        self.corpus_tokens = [list(d) for d in corpus_tokens]
        self.n_docs = len(self.corpus_tokens)
        self.doc_len = [len(d) for d in self.corpus_tokens]
        self.avgdl = (sum(self.doc_len) / self.n_docs) if self.n_docs else 0.0
        # Inverted index: term -> list of (doc_idx, tf). Scoring touches only the
        # docs that actually contain a query term (O(postings), not O(n_docs)).
        self.postings: dict[str, list[tuple[int, int]]] = {}
        df: dict[str, int] = {}
        for i, toks in enumerate(self.corpus_tokens):
            freqs: dict[str, int] = {}
            for t in toks:
                freqs[t] = freqs.get(t, 0) + 1
            for t, f in freqs.items():
                self.postings.setdefault(t, []).append((i, f))
                df[t] = df.get(t, 0) + 1
        self._dl = np.asarray(self.doc_len, dtype=np.float64)
        # Okapi idf with the +1 floor (non-negative; ATIRE/standard form).
        self.idf: dict[str, float] = {
            t: math.log(1.0 + (self.n_docs - n + 0.5) / (n + 0.5)) for t, n in df.items()
        }

    def scores(self, query_tokens: Sequence[str]) -> np.ndarray:
        """BM25 score per doc for ``query_tokens`` (deterministic float64 array)."""
        scores = np.zeros(self.n_docs, dtype=np.float64)
        if self.avgdl == 0.0:
            return scores
        norm = 1.0 - self.b + self.b * self._dl / self.avgdl  # per-doc length norm
        qtf: dict[str, int] = {}
        for t in query_tokens:
            qtf[t] = qtf.get(t, 0) + 1
        for t, qn in qtf.items():
            idf = self.idf.get(t)
            posting = self.postings.get(t)
            if idf is None or not posting:
                continue
            idxs = np.fromiter((i for i, _ in posting), dtype=np.int64, count=len(posting))
            fs = np.fromiter((f for _, f in posting), dtype=np.float64, count=len(posting))
            denom = fs + self.k1 * norm[idxs]
            # qn preserves the per-occurrence sum of the classic loop (repeated
            # query terms count, matching rank_bm25's BM25Okapi).
            scores[idxs] += qn * idf * (fs * (self.k1 + 1.0)) / denom
        return scores

    def topk(self, query_tokens: Sequence[str], k: int) -> list[int]:
        """Top-``k`` doc indices by BM25 (descending; ties broken by index)."""
        scores = self.scores(query_tokens)
        kk = min(k, self.n_docs)
        if kk <= 0:
            return []
        order = np.argsort(-scores, kind="stable")
        return order[:kk].tolist()


# --------------------------------------------------------------------------- #
# 1-bit eu7 proxy (design §5.4) — mean-centering ON, Hamming K, f32 rerank.
# --------------------------------------------------------------------------- #


def projected_eu7(
    doc_vecs: np.ndarray,
    query_vecs: np.ndarray,
    *,
    hamming_k: int = PROJECTED_EU7_K,
    top_k: int = 10,
    mean_center: bool = True,
) -> float:
    """1-bit survivability proxy: recall@``top_k`` of the sign-quant Hamming→f32
    rerank path vs the model's OWN f32-exact top-``top_k`` (design §5.4).

    Mean-centering ON (default): subtract the per-corpus mean f32 vector from BOTH
    doc and query vectors BEFORE sign-quant; the f32 rerank uses the UN-centered
    vectors (centering is a sign-quant bias correction only). Inputs are taken
    as-is (callers pass the L2-normalized f32 vectors); no re-normalization here.
    """
    docs = np.asarray(doc_vecs, dtype=np.float32)
    queries = np.asarray(query_vecs, dtype=np.float32)
    n_docs = docs.shape[0]
    if n_docs == 0 or queries.shape[0] == 0:
        return 0.0
    k = min(top_k, n_docs)
    kk = min(hamming_k, n_docs)

    # f32-exact ground truth (un-centered cosine; rows L2-normalized -> dot).
    sims = queries @ docs.T
    exact = np.argsort(-sims, axis=1, kind="stable")[:, :k]

    if mean_center:
        mu = docs.mean(axis=0)
        dcen = docs - mu
        qcen = queries - mu
    else:
        dcen = docs
        qcen = queries
    dbits = dcen >= 0.0
    qbits = qcen >= 0.0

    recalls: list[float] = []
    for i in range(queries.shape[0]):
        ham = np.count_nonzero(dbits ^ qbits[i], axis=1)
        if kk >= n_docs:
            cand_idx = np.arange(n_docs)
        else:
            cand_idx = np.argpartition(ham, kk - 1)[:kk]
        # f32 rerank over the Hamming candidates (un-centered cosine).
        rer_scores = docs[cand_idx] @ queries[i]
        rer = cand_idx[np.argsort(-rer_scores, kind="stable")][:k]
        exact_set = set(exact[i].tolist())
        hit = len(exact_set.intersection(rer.tolist()))
        recalls.append(hit / len(exact_set))
    return float(np.mean(recalls))


# --------------------------------------------------------------------------- #
# Paired bootstrap margin CI (the gated quantity, design §5).
# --------------------------------------------------------------------------- #


def paired_bootstrap_ci(
    cand_hits: Sequence[float],
    base_hits: Sequence[float],
    *,
    seed: int = DEFAULT_BOOTSTRAP_SEED,
    n_resamples: int = DEFAULT_BOOTSTRAP_RESAMPLES,
    ci: float = 0.95,
) -> dict[str, float]:
    """Paired (cand − base) per-query delta bootstrap CI (percentile method).

    Resamples QUERY INDICES with replacement (the same indices for cand & base —
    paired), computes the mean delta per resample, returns ``{lo, hi, point}`` at
    the ``ci`` percentile bounds. Deterministic for a fixed ``seed``. Mirrors
    ``eu8_ir_validation.rs``'s paired bootstrap.
    """
    cand = np.asarray(cand_hits, dtype=np.float64)
    base = np.asarray(base_hits, dtype=np.float64)
    if cand.shape != base.shape:
        raise ValueError(f"paired arrays must align: {cand.shape} vs {base.shape}")
    deltas = cand - base
    n = deltas.shape[0]
    if n == 0:
        return {"lo": 0.0, "hi": 0.0, "point": 0.0}
    point = float(deltas.mean())
    rng = np.random.default_rng(seed)
    idx = rng.integers(0, n, size=(n_resamples, n))
    means = deltas[idx].mean(axis=1)
    lo_p = (1.0 - ci) / 2.0 * 100.0
    hi_p = (1.0 + ci) / 2.0 * 100.0
    return {
        "lo": float(np.percentile(means, lo_p)),
        "hi": float(np.percentile(means, hi_p)),
        "point": point,
    }


# --------------------------------------------------------------------------- #
# Probe wiring (consume the FROZEN gate; never reimplement, design §2).
# --------------------------------------------------------------------------- #


def build_probe_dicts(
    cand_metrics: Mapping[str, Any],
    base_metrics: Mapping[str, Any],
) -> tuple[dict[str, Any], dict[str, Any]]:
    """Build the ``(cand, base)`` dicts that :func:`probe_15a_pass` consumes.

    ``cand_metrics`` must carry ``eu8``, ``hard`` (point estimates),
    ``eu8_margin_ci_lo``, ``hard_margin_ci_lo``, ``projected_eu7``,
    ``cpu_feasible``. ``base_metrics`` carries ``eu8``, ``hard``.
    """
    cand = {
        "eu8": float(cand_metrics["eu8"]),
        "hard": float(cand_metrics["hard"]),
        "eu8_margin_ci_lo": float(cand_metrics["eu8_margin_ci_lo"]),
        "hard_margin_ci_lo": float(cand_metrics["hard_margin_ci_lo"]),
        "projected_eu7": float(cand_metrics["projected_eu7"]),
        "cpu_feasible": bool(cand_metrics["cpu_feasible"]),
    }
    base = {"eu8": float(base_metrics["eu8"]), "hard": float(base_metrics["hard"])}
    return cand, base


def candidate_passes(
    cand_metrics: Mapping[str, Any],
    base_metrics: Mapping[str, Any],
) -> bool:
    """Mechanical verdict via the REAL frozen :func:`probe_15a_pass`."""
    cand, base = build_probe_dicts(cand_metrics, base_metrics)
    return probe_15a_pass(cand, base)


def binding_margin(cand_metrics: Mapping[str, Any]) -> float:
    """Ranking key = min(eu8_margin_ci_lo, hard_margin_ci_lo) (design §2)."""
    return min(
        float(cand_metrics["eu8_margin_ci_lo"]),
        float(cand_metrics["hard_margin_ci_lo"]),
    )


def choose_embedder(per_candidate: Mapping[str, Mapping[str, Any]]) -> dict[str, Any]:
    """Pick the Slice-20 embedder (design §2): the largest-headroom
    ``probe_15a_pass`` passer that is ALSO ``in_library_feasible``; else
    ``bge-small`` (no swap).

    Each ``per_candidate`` value carries ``probe_15a_pass`` (bool),
    ``in_library_feasible`` (bool), ``binding_margin`` (float), and
    ``projected_eu7`` (float, the tiebreak headroom).
    """
    qualifying = [
        (name, m)
        for name, m in per_candidate.items()
        if bool(m.get("probe_15a_pass")) and bool(m.get("in_library_feasible"))
    ]
    # Rank by binding margin desc, projected_eu7 headroom as the tiebreak.
    ranking = sorted(
        per_candidate.items(),
        key=lambda kv: (
            bool(kv[1].get("probe_15a_pass")),
            float(kv[1].get("binding_margin", -math.inf)),
            float(kv[1].get("projected_eu7", -math.inf)),
        ),
        reverse=True,
    )
    if qualifying:
        qualifying.sort(
            key=lambda kv: (
                float(kv[1].get("binding_margin", -math.inf)),
                float(kv[1].get("projected_eu7", -math.inf)),
            ),
            reverse=True,
        )
        chosen = qualifying[0][0]
        no_swap = False
    else:
        chosen = BASE_NAME
        no_swap = True
    return {
        "chosen_embedder": chosen,
        "no_swap": no_swap,
        "ranking": [name for name, _ in ranking],
    }


# --------------------------------------------------------------------------- #
# Model-revision resolution (local HF cache snapshot → pinned 40-hex SHA).
# --------------------------------------------------------------------------- #

#: A bare 40-hex git commit SHA (HF snapshot id).
_SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def _hf_cache_dir() -> Path:
    """The active HF hub cache dir (honours ``$HF_HUB_CACHE`` / ``$HF_HOME``).

    Mirrors ``huggingface_hub``'s resolution order without importing it (keeps
    this resolver torch/transformers-free and offline): explicit hub-cache env,
    then ``$HF_HOME/hub``, then the default ``~/.cache/huggingface/hub``.
    """
    for env in ("HF_HUB_CACHE", "HUGGINGFACE_HUB_CACHE"):
        v = os.environ.get(env)
        if v:
            return Path(v)
    hf_home = os.environ.get("HF_HOME")
    if hf_home:
        return Path(hf_home) / "hub"
    return Path.home() / ".cache" / "huggingface" / "hub"


def resolve_revision(hf_id: str) -> str | None:
    """Resolve ``hf_id``'s pinned 40-hex commit SHA from the local HF cache.

    Reads ``models--<org>--<name>/refs/main`` (the resolved ``main`` commit),
    falling back to a 40-hex-named ``snapshots/<sha>`` directory. Returns the SHA
    string, or ``None`` if the model is not present in the local cache (no
    network; reproducibility-record only). The resolved SHA is the current cache
    default, so it does NOT change the ``.npy`` vector cache key (design §5/§7).
    """
    model_dir = _hf_cache_dir() / f"models--{hf_id.replace('/', '--')}"
    ref = model_dir / "refs" / "main"
    if ref.is_file():
        sha = ref.read_text(encoding="utf-8").strip()
        if _SHA_RE.match(sha):
            return sha
    snap_dir = model_dir / "snapshots"
    if snap_dir.is_dir():
        shas = sorted(p.name for p in snap_dir.iterdir() if _SHA_RE.match(p.name))
        if shas:
            return shas[0]
    return None


def build_model_revisions(model_names: Sequence[str]) -> dict[str, str]:
    """Map each model name → its resolved 40-hex SHA (``"default"`` if uncached).

    This is the exact producer of the §9 ``model_revisions`` record: a pinned SHA
    when the weights are in the local HF cache, else the ``"default"`` placeholder
    (recorded honestly rather than faked).
    """
    out: dict[str, str] = {}
    for name in model_names:
        sha = resolve_revision(MODELS[name].hf_id)
        out[name] = sha if sha is not None else "default"
    return out


def pinned_cfg(cfg: ModelCfg) -> ModelCfg:
    """Return ``cfg`` with ``revision`` pinned to the resolved cache SHA.

    If the SHA cannot be resolved (model uncached), ``cfg`` is returned unchanged
    (``revision`` stays as configured, typically ``None``).
    """
    sha = resolve_revision(cfg.hf_id)
    return replace(cfg, revision=sha) if sha is not None else cfg


# --------------------------------------------------------------------------- #
# Embedding (lazy torch/transformers).
# --------------------------------------------------------------------------- #

#: In-process (tokenizer, model) cache keyed by (hf_id, revision, pooling). A
#: loaded checkpoint is reused across :func:`embed_texts` calls so latency
#: measures STEADY-STATE embedding, not repeated ``from_pretrained`` reloads
#: (codex §9 [P2]#1). Caching changes only caching, not numerics.
_MODEL_CACHE: dict[tuple[str, str, str], tuple[Any, Any]] = {}


def _load_model_uncached(model_cfg: ModelCfg) -> tuple[Any, Any]:
    """Load ``(tokenizer, model)`` via ``from_pretrained`` (no caching)."""
    from transformers import (  # type: ignore[import-not-found]  # noqa: PLC0415
        AutoModel,
        AutoTokenizer,
    )

    load_kw: dict[str, Any] = {"trust_remote_code": model_cfg.trust_remote_code}
    if model_cfg.revision:
        load_kw["revision"] = model_cfg.revision
    tokenizer = AutoTokenizer.from_pretrained(model_cfg.hf_id, **load_kw)
    model = AutoModel.from_pretrained(model_cfg.hf_id, **load_kw)
    model.eval()
    return tokenizer, model


def _load_model(model_cfg: ModelCfg) -> tuple[Any, Any]:
    """Return the cached ``(tokenizer, model)`` for ``model_cfg``; load once.

    Keyed by ``(hf_id, revision, pooling)`` so a checkpoint loaded for one
    :func:`embed_texts` call is reused by every subsequent call (steady-state).
    """
    key = (model_cfg.hf_id, model_cfg.revision or "default", model_cfg.pooling)
    cached = _MODEL_CACHE.get(key)
    if cached is None:
        cached = _load_model_uncached(model_cfg)
        _MODEL_CACHE[key] = cached
    return cached


def embed_texts(
    model_cfg: ModelCfg,
    texts: Sequence[str],
    *,
    is_query: bool,
    batch_size: int = 32,
    max_length: int = 512,
    num_threads: int | None = None,
) -> np.ndarray:
    """Embed ``texts`` on CPU → L2-normalized f32 ``(len(texts), dim)`` array.

    Lazy-imports torch/transformers. The ``(tokenizer, model)`` is loaded ONCE
    and reused via :data:`_MODEL_CACHE` (so repeated calls do not reload the
    checkpoint). Applies the per-model prefix (query vs passage) and pooling (cls
    vs mean), runs in ``eval`` mode under ``torch.no_grad()``. Deterministic for a
    fixed model revision + thread count (the determinism ASSERTION pins
    ``num_threads=1``; design §7). Caching changes only caching, not numerics —
    vectors stay byte-identical.
    """
    import torch  # type: ignore[import-not-found]  # noqa: PLC0415 — heavy lazy dep

    if num_threads is not None:
        torch.set_num_threads(num_threads)

    prefix = model_cfg.query_prefix if is_query else model_cfg.passage_prefix
    tokenizer, model = _load_model(model_cfg)

    out: list[np.ndarray] = []
    with torch.no_grad():
        for start in range(0, len(texts), batch_size):
            batch = [prefix + t for t in texts[start : start + batch_size]]
            enc = tokenizer(
                batch,
                padding=True,
                truncation=True,
                max_length=max_length,
                return_tensors="pt",
            )
            res = model(**enc)
            hidden = res.last_hidden_state  # (B, T, H)
            if model_cfg.pooling == "cls":
                pooled = hidden[:, 0]
            elif model_cfg.pooling == "mean":
                mask = enc["attention_mask"].unsqueeze(-1).type_as(hidden)
                summed = (hidden * mask).sum(dim=1)
                counts = mask.sum(dim=1).clamp(min=1e-9)
                pooled = summed / counts
            else:  # pragma: no cover - guarded by config
                raise ValueError(f"unknown pooling {model_cfg.pooling!r}")
            vecs = torch.nn.functional.normalize(pooled, p=2, dim=1)
            out.append(vecs.cpu().numpy().astype(np.float32))
    return np.concatenate(out, axis=0) if out else np.zeros((0, model_cfg.dim), np.float32)


# --------------------------------------------------------------------------- #
# Corpus + gold loading (re-verify the frozen hash before embedding).
# --------------------------------------------------------------------------- #


@dataclass
class Corpus:
    """Resolved frozen corpus: aligned doc ids + bodies + verified hash."""

    doc_ids: list[str]
    bodies: list[str]
    corpus_hash: str
    resolved_count: int
    sources: list[str]


def _sha256_lines(path: Path) -> str:
    """SHA-256 over a file's raw bytes (line by line, matches freeze_corpus)."""
    h = hashlib.sha256()
    with path.open("rb") as f:
        for line in f:
            h.update(line)
    return h.hexdigest()


def load_snapshot() -> dict[str, Any]:
    return json.loads(SNAPSHOT_PATH.read_text(encoding="utf-8"))


def load_corpus(
    *,
    data_root: Path | None = None,
    verify_hash: bool = True,
    subset_ids: set[str] | None = None,
) -> Corpus:
    """Load doc_id→body for the frozen snapshot members; re-verify the hash.

    Re-verifies each source file's sha256 against ``snapshot.json`` and the
    combined ``corpus_hash`` against :data:`EXPECTED_CORPUS_HASH` BEFORE returning
    (design §4 — a relevance number across a drifted corpus is meaningless). When
    ``subset_ids`` is given (``--smoke``), the corpus is restricted AFTER the hash
    verification (the hash always pins the full frozen corpus).
    """
    root = data_root or _resolve_data_root()
    raw_dir = root / "data" / "corpus-data" / "raw"
    snapshot = load_snapshot()
    members = sorted(snapshot["per_source_sha256"], key=lambda e: e["source"])

    hash_lines: list[str] = []
    doc_ids: list[str] = []
    bodies: list[str] = []
    sources: list[str] = []
    for entry in members:
        src = entry["source"]
        path = raw_dir / f"{src}.jsonl"
        sha = _sha256_lines(path)
        if verify_hash and sha != entry["sha256"]:
            raise ValueError(
                f"corpus source {src} sha256 mismatch: {sha} != {entry['sha256']}"
            )
        hash_lines.append(f"{src}:{sha}")
        sources.append(src)
        with path.open("r", encoding="utf-8") as f:
            for line in f:
                if not line.strip():
                    continue
                doc = json.loads(line)
                did = str(doc["doc_id"])
                doc_ids.append(did)
                bodies.append(str(doc.get("body", "")))

    combined = hashlib.sha256("\n".join(hash_lines).encode("utf-8")).hexdigest()
    if verify_hash and combined != EXPECTED_CORPUS_HASH:
        raise ValueError(
            f"combined corpus_hash mismatch: {combined} != {EXPECTED_CORPUS_HASH}"
        )

    if subset_ids is not None:
        keep = [(d, b) for d, b in zip(doc_ids, bodies) if d in subset_ids]
        doc_ids = [d for d, _ in keep]
        bodies = [b for _, b in keep]

    return Corpus(
        doc_ids=doc_ids,
        bodies=bodies,
        corpus_hash=combined,
        resolved_count=len(doc_ids),
        sources=sources,
    )


@dataclass
class GoldQuery:
    """One eu8 gold query (design §4)."""

    query_id: str
    text: str
    query_class: str
    required_doc_ids: list[str]

    @property
    def is_negative(self) -> bool:
        return self.query_class == NEGATIVE_CLASS or not self.required_doc_ids


def load_gold(gold_path: Path) -> tuple[str, str, list[GoldQuery]]:
    """Load ``all.gold.json`` → ``(corpus_hash, qrels_version, queries)``.

    Required docs = ``expected_top_k_doc_ids`` (the doc-id qrels, design §4).
    """
    raw = json.loads(gold_path.read_text(encoding="utf-8"))
    queries: list[GoldQuery] = []
    for q in raw.get("queries", []):
        req = [str(d) for d in (q.get("expected_top_k_doc_ids") or [])]
        queries.append(
            GoldQuery(
                query_id=str(q.get("query_id", "")),
                text=str(q.get("query", "")),
                query_class=str(q.get("query_class", "")).strip(),
                required_doc_ids=req,
            )
        )
    return str(raw.get("corpus_hash", "")), str(raw.get("qrels_version", "")), queries


def default_gold_path(data_root: Path | None = None) -> Path:
    root = data_root or _resolve_data_root()
    return root / "data" / "corpus-data" / "eval" / "ir_gold" / "all.gold.json"


# --------------------------------------------------------------------------- #
# Hard subset (model-independent, BM25, design §4).
# --------------------------------------------------------------------------- #


@dataclass
class HardSubset:
    """The frozen hard-query-id set + its provenance (design §4)."""

    qids: list[str]
    cap: int
    k1: float
    b: float
    tokenizer: str
    count: int
    fts5_caveat: str


def compute_hard_subset(
    corpus: Corpus,
    queries: Sequence[GoldQuery],
    *,
    cap: int = HARD_CAP,
) -> HardSubset:
    """Freeze ``hard_qids`` = non-negative queries whose required docs are NOT
    ALL within the BM25 top-``cap`` (strict all-of, design §4).
    """
    corpus_tokens = [tokenize(b) for b in corpus.bodies]
    bm25 = BM25Okapi(corpus_tokens, k1=BM25_K1, b=BM25_B)
    id_at = corpus.doc_ids
    qids: list[str] = []
    for q in queries:
        if q.is_negative:
            continue
        top_idx = bm25.topk(tokenize(q.text), cap)
        top_ids = {id_at[i] for i in top_idx}
        solved = all(d in top_ids for d in q.required_doc_ids)
        if not solved:
            qids.append(q.query_id)
    return HardSubset(
        qids=qids,
        cap=cap,
        k1=BM25_K1,
        b=BM25_B,
        tokenizer="lowercase + regex [a-z0-9]+",
        count=len(qids),
        fts5_caveat=(
            "pure-Python BM25 tokenizer is NOT byte-identical to the engine FTS5 "
            "tokenizer; the subset is deterministic + model-independent for a fair "
            "A/B (design §11.1), not a production-FTS bucket."
        ),
    )


# --------------------------------------------------------------------------- #
# Per-model metrics + the full run.
# --------------------------------------------------------------------------- #


def _atomic_save_npy(path: Path, arr: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    # Pass an open handle so np.save does NOT append a second ".npy" suffix.
    with tmp.open("wb") as fh:
        np.save(fh, arr)
    os.replace(tmp, path)


def embed_model(
    cfg: ModelCfg,
    corpus: Corpus,
    queries: Sequence[GoldQuery],
    *,
    cache_dir: Path,
    num_threads: int,
    batch_size: int,
    resume: bool,
) -> tuple[np.ndarray, np.ndarray]:
    """Embed (or resume) doc + query f32 vectors for ``cfg``; checkpoint to .npy."""
    doc_npy = cache_dir / f"{cfg.name}.docs.npy"
    qry_npy = cache_dir / f"{cfg.name}.queries.npy"
    if resume and doc_npy.exists() and qry_npy.exists():
        docs = np.load(doc_npy)
        qvecs = np.load(qry_npy)
        if docs.shape[0] == len(corpus.doc_ids) and qvecs.shape[0] == len(queries):
            return docs, qvecs
    docs = embed_texts(
        cfg, corpus.bodies, is_query=False, batch_size=batch_size, num_threads=num_threads
    )
    qvecs = embed_texts(
        cfg,
        [q.text for q in queries],
        is_query=True,
        batch_size=batch_size,
        num_threads=num_threads,
    )
    _atomic_save_npy(doc_npy, docs)
    _atomic_save_npy(qry_npy, qvecs)
    return docs, qvecs


def measure_latency(
    cfg: ModelCfg,
    sample_texts: Sequence[str],
    *,
    is_query: bool,
    num_threads: int,
    batch_size: int = 1,
) -> float:
    """Median per-item STEADY-STATE embed latency in ms (design §5.5).

    The model is loaded ONCE (via :data:`_MODEL_CACHE`) and a single warmup
    forward is run BEFORE timing, so the reported latency EXCLUDES checkpoint
    load (codex §9 [P2]#1 — load time dwarfs CPU embedding and would wrongly fail
    ``cpu_feasible``). ``batch_size=1`` gives single-text ``ms_per_query``; a
    larger ``batch_size`` gives batched per-item ``ms_per_doc`` (total batch time
    / batch length).
    """
    texts = list(sample_texts)
    if not texts:
        return math.inf
    bs = max(1, batch_size)
    # Warmup: make the model resident + prime lazy torch init; EXCLUDED from timing.
    embed_texts(cfg, texts[:bs], is_query=is_query, batch_size=bs, num_threads=num_threads)
    per_item_ms: list[float] = []
    for start in range(0, len(texts), bs):
        chunk = texts[start : start + bs]
        t0 = time.perf_counter()
        embed_texts(cfg, chunk, is_query=is_query, batch_size=bs, num_threads=num_threads)
        per_item_ms.append((time.perf_counter() - t0) * 1000.0 / len(chunk))
    return float(statistics.median(per_item_ms)) if per_item_ms else math.inf


def eu8_hits(
    doc_ids: Sequence[str],
    doc_vecs: np.ndarray,
    query_vecs: np.ndarray,
    queries: Sequence[GoldQuery],
    *,
    k: int = 10,
) -> list[float]:
    """Per non-negative query strict all-of hit ∈ {0,1} at top-``k``."""
    nonneg = [i for i, q in enumerate(queries) if not q.is_negative]
    if not nonneg:
        return []
    top = cosine_topk(query_vecs[nonneg], doc_vecs, k)
    hits: list[float] = []
    for row, qi in enumerate(nonneg):
        retrieved = [doc_ids[int(j)] for j in top[row]]
        hits.append(strict_recall_hit(retrieved, queries[qi].required_doc_ids))
    return hits


def hard_hits(
    doc_ids: Sequence[str],
    doc_vecs: np.ndarray,
    query_vecs: np.ndarray,
    queries: Sequence[GoldQuery],
    hard_qids: set[str],
    *,
    k: int = 10,
) -> tuple[list[float], list[float], list[float]]:
    """Per hard-subset query: (r@10 hit, r@50 hit, median gold rank)."""
    idx = [i for i, q in enumerate(queries) if q.query_id in hard_qids]
    if not idx:
        return [], [], []
    top50 = cosine_topk(query_vecs[idx], doc_vecs, 50)
    r10: list[float] = []
    r50: list[float] = []
    ranks: list[float] = []
    for row, qi in enumerate(idx):
        retrieved50 = [doc_ids[int(j)] for j in top50[row]]
        req = queries[qi].required_doc_ids
        r10.append(strict_recall_hit(retrieved50[:k], req))
        r50.append(strict_recall_hit(retrieved50, req))
        ranks.append(graded_rank_of(retrieved50, req))
    return r10, r50, ranks


def _mean(xs: Sequence[float]) -> float:
    return float(np.mean(xs)) if len(xs) else 0.0


def _median_finite(xs: Sequence[float]) -> float | None:
    finite = [x for x in xs if math.isfinite(x)]
    return float(statistics.median(finite)) if finite else None


def run_probe(
    *,
    model_names: Sequence[str],
    data_root: Path | None,
    cache_dir: Path,
    num_threads: int,
    batch_size: int,
    latency_sample: int,
    bootstrap_seed: int,
    bootstrap_resamples: int,
    resume: bool,
    smoke: bool,
    smoke_doc_cap: int,
) -> dict[str, Any]:
    """Run the full 15a probe → the §9 result dict (mechanical, deterministic)."""
    root = data_root or _resolve_data_root()
    gold_path = default_gold_path(root)
    gold_hash, qrels_version, all_queries = load_gold(gold_path)

    subset_ids: set[str] | None = None
    if smoke:
        # Smoke: a small corpus subset (the union of some gold docs) + a few
        # queries, to prove the pipeline fast — NOT a measurement.
        corpus_full = load_corpus(data_root=root, verify_hash=True)
        nonneg = [q for q in all_queries if not q.is_negative]
        sub_queries = nonneg[: max(8, latency_sample)]
        subset_ids = {d for q in sub_queries for d in q.required_doc_ids}
        # pad with extra docs up to the cap for a non-trivial ranking.
        for did in corpus_full.doc_ids:
            if len(subset_ids) >= smoke_doc_cap:
                break
            subset_ids.add(did)
        queries = sub_queries
    else:
        queries = all_queries

    corpus = load_corpus(data_root=root, verify_hash=True, subset_ids=subset_ids)
    if gold_hash != EXPECTED_CORPUS_HASH:
        raise ValueError(
            f"gold corpus_hash {gold_hash} != expected {EXPECTED_CORPUS_HASH}"
        )

    hard = compute_hard_subset(corpus, queries)
    hard_set = set(hard.qids)

    # Embed + measure each model.
    per_model_raw: dict[str, dict[str, Any]] = {}
    sample_q = [q.text for q in queries[:latency_sample]] or [q.text for q in queries[:1]]
    sample_d = corpus.bodies[:latency_sample] or corpus.bodies[:1]
    for name in model_names:
        # Pin the revision to the resolved local-cache SHA (reproducibility,
        # codex §9 [P2]#2). The SHA == the current cache default, so the warm
        # .npy vectors stay valid (the .npy key is the model NAME, not revision).
        cfg = pinned_cfg(MODELS[name])
        docs, qvecs = embed_model(
            cfg,
            corpus,
            queries,
            cache_dir=cache_dir,
            num_threads=num_threads,
            batch_size=batch_size,
            resume=resume,
        )
        eu8 = eu8_hits(corpus.doc_ids, docs, qvecs, queries)
        h10, h50, hranks = hard_hits(
            corpus.doc_ids, docs, qvecs, queries, hard_set
        )
        proj = projected_eu7(docs, qvecs, hamming_k=PROJECTED_EU7_K, mean_center=True)
        # ms_per_query: single-text; ms_per_doc: batched (per-item). Both measured
        # steady-state (model resident + warmup), excluding checkpoint load.
        ms_q = measure_latency(
            cfg, sample_q, is_query=True, num_threads=num_threads, batch_size=1
        )
        ms_d = measure_latency(
            cfg, sample_d, is_query=False, num_threads=num_threads, batch_size=batch_size
        )
        per_model_raw[name] = {
            "cfg": cfg,
            "eu8_hits": eu8,
            "hard_r10_hits": h10,
            "hard_r50_hits": h50,
            "hard_ranks": hranks,
            "projected_eu7": proj,
            "ms_per_query": ms_q,
            "ms_per_doc": ms_d,
        }

    base_raw = per_model_raw[BASE_NAME]
    base_ms = base_raw["ms_per_query"]
    base_eu8 = _mean(base_raw["eu8_hits"])
    base_hard = _mean(base_raw["hard_r10_hits"])

    per_candidate: dict[str, Any] = {}
    for name in model_names:
        if name == BASE_NAME:
            continue
        raw = per_model_raw[name]
        cfg: ModelCfg = raw["cfg"]
        eu8_ci = paired_bootstrap_ci(
            raw["eu8_hits"],
            base_raw["eu8_hits"],
            seed=bootstrap_seed,
            n_resamples=bootstrap_resamples,
        )
        hard_ci = paired_bootstrap_ci(
            raw["hard_r10_hits"],
            base_raw["hard_r10_hits"],
            seed=bootstrap_seed,
            n_resamples=bootstrap_resamples,
        )
        cpu_feasible = raw["ms_per_query"] <= CPU_FEASIBLE_MULT * base_ms
        cand_metrics = {
            "eu8": _mean(raw["eu8_hits"]),
            "hard": _mean(raw["hard_r10_hits"]),
            "eu8_margin_ci_lo": eu8_ci["lo"],
            "hard_margin_ci_lo": hard_ci["lo"],
            "projected_eu7": raw["projected_eu7"],
            "cpu_feasible": cpu_feasible,
        }
        passed = candidate_passes(cand_metrics, {"eu8": base_eu8, "hard": base_hard})
        per_candidate[name] = {
            "eu8": cand_metrics["eu8"],
            "hard": {
                "r@10": cand_metrics["hard"],
                "r@50": _mean(raw["hard_r50_hits"]),
                "median_rank": _median_finite(raw["hard_ranks"]),
            },
            "memory_class_recall": None,
            "projected_eu7": raw["projected_eu7"],
            "cpu_latency": {
                "ms_per_query": raw["ms_per_query"],
                "ms_per_doc": raw["ms_per_doc"],
                "threads": num_threads,
                "feasible": cpu_feasible,
                "base_ms_per_query": base_ms,
                "ac012_p50_ms": 20.0,
                "ac012_p99_ms": 150.0,
                "ac013_p50_ms": 80.0,
                "ac013_p99_ms": 300.0,
            },
            "eu8_margin_ci": eu8_ci,
            "hard_margin_ci": hard_ci,
            "probe_15a_pass": passed,
            "in_library_feasible": cfg.in_library_feasible,
            "binding_margin": binding_margin(cand_metrics),
        }

    choice = choose_embedder(per_candidate)
    # Provisional surpass flag: a passer with a large projected eu8/hard lift is a
    # candidate surpass lever; the gap-relative finalization defers to Stage 2.
    surpass_flag = {
        name: bool(m["probe_15a_pass"]) and m["binding_margin"] > 0.0
        for name, m in per_candidate.items()
    }

    return {
        "phase": "0.8.3-slice-15a",
        "smoke": smoke,
        "base": {
            "name": BASE_NAME,
            "eu8": base_eu8,
            "hard_r@10": base_hard,
            "ms_per_query": base_ms,
        },
        "per_candidate": per_candidate,
        "chosen_embedder": choice["chosen_embedder"],
        "no_swap": choice["no_swap"],
        "ranking": choice["ranking"],
        "surpass_flag": surpass_flag,
        "corpus_hash": corpus.corpus_hash,
        "corpus_resolved_count": corpus.resolved_count,
        "qrels_version": qrels_version,
        "hard_subset": {
            "cap": hard.cap,
            "count": hard.count,
            "k1": hard.k1,
            "b": hard.b,
            "tokenizer": hard.tokenizer,
            "definition": (
                "non-negative qids whose required docs are NOT all within BM25 "
                "top-cap (strict all-of)"
            ),
            "fts5_caveat": hard.fts5_caveat,
        },
        "model_revisions": build_model_revisions(model_names),
        "seeds": {
            "bootstrap_seed": bootstrap_seed,
            "bootstrap_resamples": bootstrap_resamples,
        },
        "pooling_prefix_table": pooling_prefix_table(),
        "num_threads": num_threads,
        "stage_split": (
            "Stage 1 (this slice): mechanical probe_15a_pass verdict + ranking + "
            "chosen embedder. Stage 2 (after D0b/HITL): gap-relative surpass "
            "finalization (design §6)."
        ),
        "caveats": [
            "torch-CPU vs ONNX-CPU produce f32-equivalent vectors; ceiling is a "
            "model-weights property (design §7).",
            "memory_class_recall not computed — LME corpus is separate from the IR "
            "snapshot; deferred to Slice 20 re-embed (design §4).",
            "projected_eu7 GATE is the frozen POINT >= 0.90; the reported CI is "
            "transparency-only (design §5.4/§11.4).",
        ],
    }


def _record_runtime() -> dict[str, str]:
    """Record torch/transformers/numpy versions for the determinism claim (§7)."""
    info: dict[str, str] = {"numpy": np.__version__}
    try:  # pragma: no cover - environment dependent
        import torch  # type: ignore[import-not-found]  # noqa: PLC0415

        info["torch"] = torch.__version__
    except ImportError:  # pragma: no cover
        info["torch"] = "absent"
    try:  # pragma: no cover
        import transformers  # type: ignore[import-not-found]  # noqa: PLC0415

        info["transformers"] = transformers.__version__
    except ImportError:  # pragma: no cover
        info["transformers"] = "absent"
    return info


def write_outputs(result: dict[str, Any], *, out_json: Path, out_md: Path) -> None:
    """Write the §9 json + markdown report."""
    result = dict(result)
    result["runtime"] = _record_runtime()
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    lines: list[str] = []
    lines.append("# 0.8.3 Slice-15a — embedder-ceiling probe report")
    lines.append("")
    mode = "SMOKE (pipeline proof, NOT a measurement)" if result["smoke"] else "FULL"
    lines.append(f"- mode: **{mode}**")
    lines.append(f"- corpus_hash: `{result['corpus_hash']}` "
                 f"({result['corpus_resolved_count']} docs)")
    lines.append(f"- qrels_version: `{result['qrels_version']}`")
    hs = result["hard_subset"]
    lines.append(
        f"- hard subset: cap={hs['cap']} count={hs['count']} "
        f"(BM25 k1={hs['k1']} b={hs['b']}, tokenizer: {hs['tokenizer']})"
    )
    base = result["base"]
    lines.append(
        f"- base ({base['name']}): eu8={base['eu8']:.4f} hard@10={base['hard_r@10']:.4f}"
    )
    lines.append("")
    lines.append("## Per-candidate verdict")
    lines.append("")
    lines.append("| candidate | eu8 | hard@10 | proj_eu7 | eu8_ci_lo | hard_ci_lo | "
                 "cpu | in_lib | PASS |")
    lines.append("|---|---|---|---|---|---|---|---|---|")
    for name, m in result["per_candidate"].items():
        lines.append(
            f"| {name} | {m['eu8']:.4f} | {m['hard']['r@10']:.4f} | "
            f"{m['projected_eu7']:.4f} | {m['eu8_margin_ci']['lo']:+.4f} | "
            f"{m['hard_margin_ci']['lo']:+.4f} | "
            f"{m['cpu_latency']['feasible']} | {m['in_library_feasible']} | "
            f"{'PASS' if m['probe_15a_pass'] else 'fail'} |"
        )
    lines.append("")
    lines.append(f"## Chosen Slice-20 embedder: **{result['chosen_embedder']}**"
                 + (" (no swap)" if result["no_swap"] else ""))
    lines.append("")
    lines.append(f"- ranking: {', '.join(result['ranking'])}")
    lines.append(f"- surpass_flag (provisional): {result['surpass_flag']}")
    lines.append("")
    lines.append(result["stage_split"])
    lines.append("")
    lines.append("### Caveats")
    for c in result["caveats"]:
        lines.append(f"- {c}")
    out_md.parent.mkdir(parents=True, exist_ok=True)
    out_md.write_text("\n".join(lines) + "\n", encoding="utf-8")


# --------------------------------------------------------------------------- #
# CLI.
# --------------------------------------------------------------------------- #


def _default_cache_dir() -> Path:
    return Path(os.environ.get("FDB_S15A_CACHE", "/tmp/fdb-s15a-cache"))


def _build_argparser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="0.8.3 Slice-15a embedder-ceiling probe")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--smoke", action="store_true", help="small subset + base + 1 candidate")
    mode.add_argument("--full", action="store_true", help="full measurement (orchestrator-run)")
    p.add_argument("--data-root", default=None, help="root containing data/corpus-data")
    p.add_argument("--cache-dir", default=None, help="checkpoint dir for .npy vectors")
    p.add_argument("--models", default=None, help="comma-separated model names override")
    p.add_argument("--threads", type=int, default=1, help="torch.set_num_threads")
    p.add_argument("--batch-size", type=int, default=32)
    p.add_argument("--latency-sample", type=int, default=20)
    p.add_argument("--bootstrap-seed", type=int, default=DEFAULT_BOOTSTRAP_SEED)
    p.add_argument("--bootstrap-resamples", type=int, default=DEFAULT_BOOTSTRAP_RESAMPLES)
    p.add_argument("--smoke-doc-cap", type=int, default=120)
    p.add_argument("--resume", action="store_true", help="reuse .npy checkpoints")
    p.add_argument(
        "--out-json",
        default=str(_WORKTREE_ROOT / "dev/plans/runs/0.8.3-s15a-embedder.json"),
    )
    p.add_argument(
        "--out-md",
        default=str(_WORKTREE_ROOT / "dev/plans/runs/0.8.3-s15a-report.md"),
    )
    return p


def _select_models(args: argparse.Namespace) -> list[str]:
    if args.models:
        return [m.strip() for m in args.models.split(",") if m.strip()]
    if args.smoke:
        return [BASE_NAME, "bge-base"]
    return [BASE_NAME, *CANDIDATE_NAMES]


def main(argv: Sequence[str] | None = None) -> int:
    args = _build_argparser().parse_args(argv)
    data_root = Path(args.data_root) if args.data_root else None
    cache_dir = Path(args.cache_dir) if args.cache_dir else _default_cache_dir()
    result = run_probe(
        model_names=_select_models(args),
        data_root=data_root,
        cache_dir=cache_dir,
        num_threads=args.threads,
        batch_size=args.batch_size,
        latency_sample=args.latency_sample,
        bootstrap_seed=args.bootstrap_seed,
        bootstrap_resamples=args.bootstrap_resamples,
        resume=args.resume,
        smoke=args.smoke,
        smoke_doc_cap=args.smoke_doc_cap,
    )
    write_outputs(result, out_json=Path(args.out_json), out_md=Path(args.out_md))
    print(
        f"[s15a] mode={'smoke' if args.smoke else 'full'} "
        f"chosen={result['chosen_embedder']} no_swap={result['no_swap']} "
        f"hard_count={result['hard_subset']['count']} "
        f"corpus_docs={result['corpus_resolved_count']}"
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
