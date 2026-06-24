"""0.8.4 Tier-2 prototype — C (map-reduce QFS) + D2 (depth-1 coverage index).

Engine-independent, no network: a deterministic fake LLM and the default BoW embedder. Asserts the
pipeline shape (chunking, seeded k-means determinism, coverage-index build/retrieve, C reduce) — the
*quality* number is the scale measurement's job, not a unit test's.
"""

from __future__ import annotations

import numpy as np

from eval.tier2_coverage import (
    CoverageIndex,
    bow_embedder,
    build_coverage_index,
    chunk_corpus,
    default_n_clusters,
    global_answer_d2,
    global_answer_mapreduce,
    kmeans,
)

# A corpus with three clean themes (vaccines / central bank / drought), 3 docs each.
_DOCS = {
    f"vac{i}": "influenza vaccine candidate clinical trial immunization measles outbreak" for i in range(3)
} | {
    f"bank{i}": "central bank interest rate inflation monetary policy treasury bond yield" for i in range(3)
} | {
    f"dry{i}": "drought wheat harvest crop irrigation rainfall farmland agriculture yield" for i in range(3)
}


class _FakeLLM:
    """Records prompts; returns a deterministic short string derived from the prompt."""

    def __init__(self) -> None:
        self.calls: list[str] = []

    def __call__(self, prompt: str, max_tokens: int) -> str:
        self.calls.append(prompt)
        # echo a few salient tokens so summaries/answers are non-empty + deterministic
        toks = [t for t in prompt.lower().split() if t.isalpha()][:6]
        return "report: " + " ".join(toks)


def test_chunk_corpus_whole_doc_and_split() -> None:
    chunks = chunk_corpus({"a": "short body", "b": "x" * 9000}, max_chars=4000)
    by_doc = {c.doc_id for c in chunks}
    assert by_doc == {"a", "b"}
    assert sum(1 for c in chunks if c.doc_id == "a") == 1          # fits whole
    assert sum(1 for c in chunks if c.doc_id == "b") >= 3          # 9000/4000 -> >=3
    assert all(len(c.text) <= 4000 for c in chunks)
    assert [c.chunk_id for c in chunks] == sorted(c.chunk_id for c in chunks)  # deterministic order


def test_kmeans_deterministic_and_separates_themes() -> None:
    embed = bow_embedder()
    chunks = chunk_corpus(_DOCS)
    X = np.vstack([embed(c.text) for c in chunks])
    a = kmeans(X, 3, seed=0)
    b = kmeans(X, 3, seed=0)
    assert np.array_equal(a, b)                                    # deterministic
    # the three themes should land in three distinct clusters (clean synthetic separation)
    groups = {}
    for lab, ch in zip(a, chunks):
        groups.setdefault(int(lab), set()).add(ch.doc_id[:3])
    # each cluster is theme-pure (only one of vac/ban/dry prefixes)
    assert all(len(prefixes) == 1 for prefixes in groups.values())


def test_default_n_clusters() -> None:
    assert default_n_clusters(9) == 3
    assert default_n_clusters(1) == 2 - 1 or default_n_clusters(1) >= 1  # clamped to [2,n]->1 when n=1
    assert default_n_clusters(100) == 10


def test_build_coverage_index_and_retrieve() -> None:
    embed, llm = bow_embedder(), _FakeLLM()
    chunks = chunk_corpus(_DOCS)
    index = build_coverage_index(chunks, embed, llm, n_clusters=3, seed=0)
    assert isinstance(index, CoverageIndex)
    assert len(index.nodes) == 3                                   # one coverage node per cluster
    assert all(n.summary for n in index.nodes)                    # non-empty summaries
    assert all(n.embedding.shape == (512,) for n in index.nodes)
    # every chunk is covered by exactly one node (partition)
    covered = [cid for n in index.nodes for cid in n.member_chunk_ids]
    assert sorted(covered) == sorted(c.chunk_id for c in chunks)
    # retrieve respects k and is deterministic
    hits = index.retrieve(embed("vaccine immunization"), k=2)
    assert len(hits) == 2
    assert [h.node_id for h in hits] == [h.node_id for h in index.retrieve(embed("vaccine immunization"), k=2)]


def test_empty_coverage_index_degrades_gracefully() -> None:
    embed, llm = bow_embedder(), _FakeLLM()
    ans = global_answer_d2("anything", CoverageIndex(), embed, llm)
    assert ans  # falls back to a direct answer, never crashes


def test_mapreduce_reads_all_chunks_hierarchically() -> None:
    llm = _FakeLLM()
    chunks = chunk_corpus(_DOCS)                                   # 9 chunks
    ans = global_answer_mapreduce("what themes appear?", chunks, llm, map_batch=2, reduce_fanin=2)
    assert ans
    # map covered all chunks: ceil(9/2)=5 map calls happened (every chunk seen) ...
    map_calls = [c for c in llm.calls if c.startswith("Extract points")]
    assert len(map_calls) == 5
    # ... and a hierarchical reduce ran (more than the single final synthesis pass)
    reduce_calls = [c for c in llm.calls if c.startswith("Condense")]
    assert len(reduce_calls) >= 1
