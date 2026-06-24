"""0.8.4 Tier-2 prototype $0 sanity run: build D2 coverage index + answer C/D2 on the 15-doc corpus.

Uses the LOCAL Qwen3.6-27B vLLM at :8000 ($0; thinking off) as the summarizer/reader and the default
BoW embedder. Goal: confirm the pipeline produces COHERENT coverage summaries and global answers —
NOT a quality verdict (that is the scale measurement, which needs a real embedder + scaled GraphRAG
index). Design: dev/design/0.8.4-closing-graphrag-gap.md §3.

Run:  FATHOMDB_TESTS_NO_REBUILD=1 PYTHONPATH=src/python python dev/plans/runs/0.8.4-tier2-prototype.py
"""

from __future__ import annotations

import json
import os
import tempfile
import urllib.request

import numpy as np

from eval.apnews_corpus import load_articles, load_autoq
from eval.tier2_coverage import (
    bow_embedder,
    build_coverage_index,
    chunk_corpus,
    default_n_clusters,
    global_answer_d2,
    global_answer_mapreduce,
)

QWEN_URL = "http://localhost:8000/v1/chat/completions"
N_DOCS = 15
USE_REAL_EMBEDDER = os.environ.get("FDB_BOW_EMBED") != "1"  # default: engine bge-small


def real_embedder():
    """FathomDB's OWN pinned embedder (fathomdb-bge-small-en-v1.5) as an EmbedFn.

    Uses the read-path `Engine.embed()` primitive so D2 clusters under the engine's
    real identity, not a parallel embedder. L2-normalized so dot == cosine."""
    from fathomdb import Engine

    eng = Engine.open(os.path.join(tempfile.mkdtemp(), "embed.sqlite"), use_default_embedder=True)

    def embed(text: str) -> np.ndarray:
        v = np.asarray(eng.embed(text), dtype=np.float64)
        n = float(np.linalg.norm(v))
        return v / n if n > 0.0 else v

    return embed, eng


def qwen(prompt: str, max_tokens: int) -> str:
    body = {
        "model": "qwen3.6-27b",
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.0,
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": False},
    }
    req = urllib.request.Request(
        QWEN_URL, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=240) as r:
        return json.loads(r.read())["choices"][0]["message"]["content"] or ""


def main() -> None:
    arts = load_articles()[:N_DOCS]
    docs = {a.doc_id: a.body for a in arts}
    chunks = chunk_corpus(docs)
    if USE_REAL_EMBEDDER:
        embed, _eng = real_embedder()
        emb_name = "engine bge-small (real)"
    else:
        embed, emb_name = bow_embedder(), "hashing BoW"
    nclust = default_n_clusters(len(chunks))
    print(f"corpus: {len(docs)} docs -> {len(chunks)} chunks -> {nclust} clusters "
          f"(embed={emb_name}, local Qwen $0)")

    print("\n=== building D2 coverage index (one Qwen summary per cluster) ===")
    index = build_coverage_index(chunks, embed, qwen, n_clusters=nclust, summary_tokens=300)
    for n in index.nodes:
        print(f"\n[{n.node_id}] ({len(n.member_chunk_ids)} chunks) {n.summary[:280]}...")

    q = next(q.question_text for q in load_autoq() if q.scope == "global")
    print(f"\n=== global question ===\n{q}")

    print("\n=== D2 answer (over coverage summaries) ===")
    d2 = global_answer_d2(q, index, embed, qwen, k=min(8, len(index.nodes)), answer_tokens=900)
    print(d2[:1200])

    print("\n=== C answer (map-reduce QFS over all chunks) ===")
    c = global_answer_mapreduce(q, chunks, qwen, answer_tokens=900)
    print(c[:1200])

    print(f"\nSANITY OK — D2 nodes={len(index.nodes)}, D2 ans={len(d2)}c, C ans={len(c)}c. $0 (local Qwen).")


if __name__ == "__main__":
    main()
