"""0.8.4 Tier-2 (almost graph-free) global-sensemaking prototype: C + D2.

Design: ``dev/design/0.8.4-closing-graphrag-gap.md`` §3. Two additive capabilities that deliver
whole-corpus coverage **without** entity extraction / relationship graph / Leiden communities —
they win sensemaking on FathomDB's *embedding* mechanism, not the (measured-weak) graph one:

* **C — map-reduce QFS reader mode** (:func:`global_answer_mapreduce`): map over *all* chunks
  (extract relevant points), then **hierarchically reduce** (reduce in fan-in batches, then
  reduce-of-reduces) so the final synthesis is never a single overloaded pass. Coverage-complete by
  construction (reads everything); no index. The always-available fallback; per-query LLM cost grows
  with corpus size — the reason D2 exists.
* **D2 — depth-1 cluster-summary coverage index** (:func:`build_coverage_index` +
  :func:`global_answer_d2`): cluster all chunk embeddings once (k-means), LLM-summarize each cluster
  into a *coverage node*, store the summaries as retrievable vectors. A global query retrieves over
  the **coverage nodes** (pre-summarized whole-corpus themes) — "embedding-based communities",
  GraphRAG's hierarchy benefit manufactured by clustering. Build cost is paid **once at ingest**
  (the rate-adaptive queue, design §3b); query is CPU-only ANN over the coverage vectors.

This module is **engine-independent** (pure-Python + numpy, no native extension, no DB) and
**embedder/LLM-agnostic**: callers inject ``embed`` and ``llm``. The default :func:`bow_embedder`
is the deterministic hashing bag-of-words from :mod:`eval.baselines_084` (crude, lexical) — fine to
prove the pipeline and for the $0 sanity run, but the **scale measurement needs a real semantic
embedder** (the standing Slice-5 [P2]); D2's clustering quality is embedder-bound.

Determinism: k-means is seeded (k-means++ init with a seeded RNG); identical inputs → identical
clusters → identical coverage nodes.
"""

from __future__ import annotations

import hashlib
import re
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field

import numpy as np

#: ``text -> dense L2-normalized embedding`` (shape ``(dim,)``).
EmbedFn = Callable[[str], np.ndarray]
#: ``(prompt, max_tokens) -> completion`` — the injected summarizer/reader (local Qwen $0 by default).
LlmFn = Callable[[str, int], str]

_TOKEN_RE = re.compile(r"[a-z0-9]+")


# --------------------------------------------------------------------------- #
# Embedding (default: deterministic hashing BoW — swap for a real embedder at scale)
# --------------------------------------------------------------------------- #
def bow_embedder(dim: int = 512) -> EmbedFn:
    """Return a deterministic hashing bag-of-words ``EmbedFn`` (dense, L2-normalized).

    Mirrors :func:`eval.baselines_084._embed` (sha1-hashed token buckets, stable across processes)
    but densified to a numpy vector for k-means. Crude/lexical — the prototype default; the scale
    measurement should inject a real semantic embedder (Slice-5 [P2])."""

    def embed(text: str) -> np.ndarray:
        v = np.zeros(dim, dtype=np.float64)
        for tok in _TOKEN_RE.findall(text.lower()):
            if len(tok) >= 2:
                v[int(hashlib.sha1(tok.encode("utf-8")).hexdigest(), 16) % dim] += 1.0
        n = float(np.linalg.norm(v))
        return v / n if n > 0.0 else v

    return embed


# --------------------------------------------------------------------------- #
# Chunking
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class Chunk:
    """A unit of corpus text. At AP-News scale a chunk is an article; long docs split on char budget."""

    chunk_id: str
    doc_id: str
    text: str


def chunk_corpus(documents: Mapping[str, str], *, max_chars: int = 6000) -> list[Chunk]:
    """Split each document into ≤``max_chars`` chunks (whole-doc when it fits), deterministic order.

    Splits on paragraph boundaries where possible so a chunk stays coherent; falls back to a hard
    char split for a single oversized paragraph. ``chunk_id`` = ``{doc_id}#{ordinal}``."""
    chunks: list[Chunk] = []
    for doc_id, body in sorted(documents.items()):
        if len(body) <= max_chars:
            chunks.append(Chunk(f"{doc_id}#0", doc_id, body))
            continue
        parts: list[str] = []
        cur = ""
        for para in body.split("\n\n"):
            if cur and len(cur) + len(para) + 2 > max_chars:
                parts.append(cur)
                cur = para
            else:
                cur = f"{cur}\n\n{para}" if cur else para
        if cur:
            parts.append(cur)
        # hard-split any part still over budget (a single huge paragraph)
        ordinal = 0
        for part in parts:
            for i in range(0, len(part), max_chars):
                chunks.append(Chunk(f"{doc_id}#{ordinal}", doc_id, part[i : i + max_chars]))
                ordinal += 1
    return chunks


# --------------------------------------------------------------------------- #
# k-means (deterministic, numpy — no sklearn dependency)
# --------------------------------------------------------------------------- #
def kmeans(X: np.ndarray, n_clusters: int, *, seed: int = 0, iters: int = 50) -> np.ndarray:
    """Return integer cluster labels (shape ``(n,)``) via seeded k-means++ (Lloyd's, ``iters`` max).

    Deterministic: a seeded RNG drives k-means++ seeding and all randomness, so identical ``X`` →
    identical labels. Empty clusters are re-seeded to the point farthest from its centroid. Cosine-
    friendly: with L2-normalized rows, euclidean k-means approximates spherical k-means."""
    n = X.shape[0]
    n_clusters = max(1, min(n_clusters, n))
    rng = np.random.default_rng(seed)

    # k-means++ init
    centers = np.empty((n_clusters, X.shape[1]), dtype=X.dtype)
    centers[0] = X[rng.integers(n)]
    closest = ((X - centers[0]) ** 2).sum(axis=1)
    for c in range(1, n_clusters):
        probs = closest / closest.sum() if closest.sum() > 0 else np.full(n, 1.0 / n)
        centers[c] = X[rng.choice(n, p=probs)]
        closest = np.minimum(closest, ((X - centers[c]) ** 2).sum(axis=1))

    labels = np.zeros(n, dtype=np.int64)
    for _ in range(iters):
        # assign
        d = ((X[:, None, :] - centers[None, :, :]) ** 2).sum(axis=2)
        new_labels = d.argmin(axis=1)
        if np.array_equal(new_labels, labels) and _ > 0:
            break
        labels = new_labels
        # update
        for c in range(n_clusters):
            members = X[labels == c]
            if len(members):
                centers[c] = members.mean(axis=0)
            else:  # re-seed an empty cluster to the globally worst-served point
                worst = (((X - centers[labels]) ** 2).sum(axis=1)).argmax()
                centers[c] = X[worst]
    return labels


def default_n_clusters(n_chunks: int) -> int:
    """Heuristic cluster count ≈ √n (design §3b), clamped to ``[2, n]``."""
    return max(2, min(n_chunks, round(n_chunks**0.5)))


# --------------------------------------------------------------------------- #
# C — map-reduce QFS (coverage-complete; no index)
# --------------------------------------------------------------------------- #
def global_answer_mapreduce(
    question: str,
    chunks: Sequence[Chunk],
    llm: LlmFn,
    *,
    map_batch: int = 5,
    reduce_fanin: int = 8,
    map_tokens: int = 400,
    answer_tokens: int = 1500,
) -> str:
    """C: map over ALL chunks (extract relevant points), then **hierarchically** reduce.

    Map: each ``map_batch`` of chunks → "extract points relevant to the question (or NONE)". Reduce:
    fold the surviving point-sets in ``reduce_fanin`` groups, then reduce-of-reduces until one set
    remains, then a final synthesis pass (≤``answer_tokens``). The hierarchical reduce is what lets
    C scale past a single overloaded synthesis (design §3a) without any community structure."""
    partials: list[str] = []
    for i in range(0, len(chunks), map_batch):
        block = "\n\n".join(f"[{j + 1}] {c.text}" for j, c in enumerate(chunks[i : i + map_batch]))
        m = llm(
            f"Extract points relevant to the question (or 'NONE').\n\nQuestion: {question}\n\n{block}\n\nPoints:",
            map_tokens,
        ).strip()
        if m and "NONE" not in m[:8].upper():
            partials.append(m)
    if not partials:
        return llm(f"Answer the question.\n\nQuestion: {question}\n\nAnswer:", answer_tokens).strip()

    # hierarchical reduce
    while len(partials) > reduce_fanin:
        nxt: list[str] = []
        for i in range(0, len(partials), reduce_fanin):
            grp = "\n\n".join(partials[i : i + reduce_fanin])
            nxt.append(
                llm(
                    f"Condense these points for the question, losing no distinct fact.\n\n"
                    f"Question: {question}\n\nPoints:\n{grp}\n\nCondensed points:",
                    answer_tokens,
                ).strip()
            )
        partials = nxt
    return llm(
        f"Synthesize a comprehensive global answer.\n\nQuestion: {question}\n\nPoints:\n"
        + "\n\n".join(partials)
        + "\n\nAnswer:",
        answer_tokens,
    ).strip()


# --------------------------------------------------------------------------- #
# D2 — depth-1 cluster-summary coverage index
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class CoverageNode:
    """A depth-1 cluster summary stored as a retrievable vector (design §3b — ``kind=coverage``)."""

    node_id: str
    summary: str
    embedding: np.ndarray
    member_chunk_ids: tuple[str, ...]


@dataclass
class CoverageIndex:
    """The side-by-side coverage index: coverage nodes retrieved by cosine over their summaries."""

    nodes: list[CoverageNode] = field(default_factory=list)

    def retrieve(self, query_embedding: np.ndarray, k: int) -> list[CoverageNode]:
        if not self.nodes:
            return []
        mat = np.vstack([n.embedding for n in self.nodes])
        sims = mat @ query_embedding  # embeddings are L2-normalized → dot == cosine
        order = sorted(range(len(self.nodes)), key=lambda i: (-float(sims[i]), self.nodes[i].node_id))
        return [self.nodes[i] for i in order[:k]]


def build_coverage_index(
    chunks: Sequence[Chunk],
    embed: EmbedFn,
    llm: LlmFn,
    *,
    n_clusters: int | None = None,
    seed: int = 0,
    summary_tokens: int = 400,
) -> CoverageIndex:
    """Build the depth-1 coverage index: embed chunks → k-means → LLM-summarize each cluster.

    The single LLM cost (one summary per cluster) is paid **once at ingest** — the rate-adaptive
    queue's work unit (design §3b). Coverage-node embedding = embedding of its summary text, so a
    global query retrieves themes by query↔summary similarity. Deterministic given ``seed``."""
    if not chunks:
        return CoverageIndex()
    X = np.vstack([embed(c.text) for c in chunks])
    n_clusters = n_clusters or default_n_clusters(len(chunks))
    labels = kmeans(X, n_clusters, seed=seed)

    nodes: list[CoverageNode] = []
    for c in range(int(labels.max()) + 1):
        members = [chunks[i] for i in range(len(chunks)) if labels[i] == c]
        if not members:
            continue
        joined = "\n\n".join(f"[{j + 1}] {m.text}" for j, m in enumerate(members))
        summary = llm(
            "Summarize the central themes, entities, and claims shared across these passages into a "
            "self-contained thematic report (no preamble).\n\n" + joined + "\n\nThematic report:",
            summary_tokens,
        ).strip()
        nodes.append(
            CoverageNode(
                node_id=f"cov-{c}",
                summary=summary,
                embedding=embed(summary),
                member_chunk_ids=tuple(m.chunk_id for m in members),
            )
        )
    return CoverageIndex(nodes)


def global_answer_d2(
    question: str,
    index: CoverageIndex,
    embed: EmbedFn,
    llm: LlmFn,
    *,
    k: int = 8,
    answer_tokens: int = 1500,
) -> str:
    """D2: retrieve top-``k`` coverage summaries (whole-corpus themes) → synthesize a global answer.

    Query-time is CPU-only ANN over the coverage vectors (no LLM until synthesis). Falls back to a
    direct answer if the index is empty (graceful degradation — design §3b "purely additive")."""
    hits = index.retrieve(embed(question), k)
    if not hits:
        return llm(f"Answer the question.\n\nQuestion: {question}\n\nAnswer:", answer_tokens).strip()
    ctx = "\n\n".join(f"[Theme {i + 1}] {h.summary}" for i, h in enumerate(hits))
    return llm(
        f"Using these whole-corpus thematic reports, synthesize a comprehensive global answer.\n\n"
        f"Question: {question}\n\nThemes:\n{ctx}\n\nAnswer:",
        answer_tokens,
    ).strip()


__all__ = [
    "Chunk",
    "CoverageIndex",
    "CoverageNode",
    "EmbedFn",
    "LlmFn",
    "bow_embedder",
    "build_coverage_index",
    "chunk_corpus",
    "default_n_clusters",
    "global_answer_d2",
    "global_answer_mapreduce",
    "kmeans",
]
