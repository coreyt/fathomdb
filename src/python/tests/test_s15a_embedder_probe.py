"""RED→GREEN tests for the 0.8.3 Slice-15a embedder-ceiling probe.

Design contract: ``dev/design/0.8.3-slice-15a-embedder-probe.md`` §8 (TDD plan).
The probe CONSUMES the frozen :func:`eval.decision_rule_083.probe_15a_pass`; these
tests exercise the pure helpers + the real-gate wiring, and (under the eu-0 venv,
CPU only) the embedding determinism + pooling correctness.

The system-under-test is imported LAZILY inside each test so that, before the
runner module exists, the suite REDs with a per-test ``ModuleNotFoundError``
(function absent) — never a collection error.
"""

from __future__ import annotations

import re
from pathlib import Path

import numpy as np
import pytest

# --------------------------------------------------------------------------- #
# (c) probe wiring truth-table — calls the REAL frozen probe_15a_pass.
# --------------------------------------------------------------------------- #


def _base_metrics() -> dict[str, float]:
    return {"eu8": 0.50, "hard": 0.30}


def _passing_cand() -> dict[str, object]:
    return {
        "eu8": 0.60,
        "hard": 0.40,
        "eu8_margin_ci_lo": 0.05,
        "hard_margin_ci_lo": 0.03,
        "projected_eu7": 0.95,
        "cpu_feasible": True,
    }


def test_probe_wiring_truth_table() -> None:
    from eval.decision_rule_083 import probe_15a_pass
    from eval.s15a_embedder_probe import build_probe_dicts, candidate_passes

    base = _base_metrics()

    # PASS: all four criteria hold.
    assert candidate_passes(_passing_cand(), base) is True

    # fail-on-eu8: eu8 margin CI lower bound not > 0.
    c = _passing_cand()
    c["eu8_margin_ci_lo"] = 0.0
    assert candidate_passes(c, base) is False

    # fail-on-hard: hard margin CI lower bound not > 0.
    c = _passing_cand()
    c["hard_margin_ci_lo"] = -0.01
    assert candidate_passes(c, base) is False

    # fail-on-cpu: not CPU-feasible.
    c = _passing_cand()
    c["cpu_feasible"] = False
    assert candidate_passes(c, base) is False

    # fail-on-eu7: 1-bit survivability below the frozen floor.
    c = _passing_cand()
    c["projected_eu7"] = 0.89
    assert candidate_passes(c, base) is False

    # The wiring must call the REAL probe_15a_pass, not a copy: identical dicts ->
    # identical verdict.
    cand_dict, base_dict = build_probe_dicts(_passing_cand(), base)
    assert candidate_passes(_passing_cand(), base) == probe_15a_pass(cand_dict, base_dict)


def test_choose_embedder_feasibility_and_no_swap() -> None:
    from eval.s15a_embedder_probe import BASE_NAME, choose_embedder

    # A passer that is NOT in_library_feasible must NOT be selected; fall back to
    # the next feasible passer.
    per_candidate = {
        "gte-base": {
            "probe_15a_pass": True,
            "in_library_feasible": False,
            "binding_margin": 0.10,
            "projected_eu7": 0.99,
        },
        "bge-base": {
            "probe_15a_pass": True,
            "in_library_feasible": True,
            "binding_margin": 0.04,
            "projected_eu7": 0.95,
        },
    }
    out = choose_embedder(per_candidate)
    assert out["chosen_embedder"] == "bge-base"
    assert out["no_swap"] is False

    # No qualifying passer -> no swap (CLS-corrected base).
    none_pass = {
        "bge-base": {
            "probe_15a_pass": False,
            "in_library_feasible": True,
            "binding_margin": -0.01,
            "projected_eu7": 0.95,
        }
    }
    out2 = choose_embedder(none_pass)
    assert out2["chosen_embedder"] == BASE_NAME
    assert out2["no_swap"] is True


# --------------------------------------------------------------------------- #
# (d) 1-bit eu7 proxy — hand-computed value on tiny fixed vectors.
# --------------------------------------------------------------------------- #


def test_projected_eu7_handcomputed() -> None:
    from eval.s15a_embedder_probe import projected_eu7

    docs = np.array(
        [
            [0.5, 0.5, 0.5, 0.5],   # d0: dot 2.0, bits 1111, ham 0
            [1.0, 1.0, -0.25, -0.25],  # d1: dot 1.5, bits 1100, ham 2
            [1.0, 0.5, 0.5, -1.0],  # d2: dot 1.0, bits 1110, ham 1
            [1.0, -0.2, -0.2, -0.1],  # d3: dot 0.5, bits 1000, ham 3
        ],
        dtype=np.float32,
    )
    q = np.array([[1.0, 1.0, 1.0, 1.0]], dtype=np.float32)
    # f32 top-2 = {d0,d1}; Hamming top-2 kept = {d0,d2}; rerank top-2 = {d0,d2};
    # overlap {d0} -> recall 1/2.
    val = projected_eu7(docs, q, hamming_k=2, top_k=2, mean_center=False)
    assert val == pytest.approx(0.5)

    # Identical f32 (queries == docs) with Hamming fanout >= N -> exact rerank ->
    # proxy 1.0, with or without mean-centering.
    assert projected_eu7(docs, docs, hamming_k=10, top_k=2, mean_center=False) == pytest.approx(1.0)
    assert projected_eu7(docs, docs, hamming_k=10, top_k=2, mean_center=True) == pytest.approx(1.0)


# --------------------------------------------------------------------------- #
# (e) paired bootstrap CI — deterministic, known values, sign properties.
# --------------------------------------------------------------------------- #


def test_paired_bootstrap_ci() -> None:
    from eval.s15a_embedder_probe import paired_bootstrap_ci

    # Strictly-positive paired delta (every query) -> every resample mean is 1.0
    # -> lo == hi == point == 1.0 (a hand-known value); ci_lo > 0.
    pos = paired_bootstrap_ci([1, 1, 1, 1], [0, 0, 0, 0], seed=7)
    assert pos == {"lo": 1.0, "hi": 1.0, "point": 1.0}
    assert pos["lo"] > 0.0

    # Zero paired delta everywhere -> ci_lo == 0.0 (<= 0).
    zero = paired_bootstrap_ci([1, 0, 1, 0], [1, 0, 1, 0], seed=7)
    assert zero == {"lo": 0.0, "hi": 0.0, "point": 0.0}
    assert zero["lo"] <= 0.0

    # Point estimate is the exact mean delta (hand-computable).
    mixed = paired_bootstrap_ci([1, 1, 1, 0], [0, 0, 0, 0], seed=7)
    assert mixed["point"] == pytest.approx(0.75)

    # Deterministic for a fixed seed (byte-identical across calls).
    a = paired_bootstrap_ci([1, 1, 0, 0, 1], [0, 1, 0, 1, 0], seed=123, n_resamples=500)
    b = paired_bootstrap_ci([1, 1, 0, 0, 1], [0, 1, 0, 1, 0], seed=123, n_resamples=500)
    assert a == b


def test_bm25_and_hard_subset_strict_all_of() -> None:
    from eval.s15a_embedder_probe import BM25Okapi, tokenize

    corpus = [
        "the quick brown fox jumps",
        "lazy dog sleeps all day",
        "quick foxes are clever animals",
    ]
    bm25 = BM25Okapi([tokenize(d) for d in corpus])
    top = bm25.topk(tokenize("quick fox"), 2)
    # doc 0 mentions both quick+fox -> must rank in the top-2; deterministic.
    assert 0 in top
    assert len(top) == 2

    # Tokenizer is lowercase + [a-z0-9]+ (punctuation/case stripped).
    assert tokenize("Hello, WORLD! 42a") == ["hello", "world", "42a"]


# --------------------------------------------------------------------------- #
# (b) pooling/prefix correctness — config table (pure) + known-vector (integration).
# --------------------------------------------------------------------------- #


def test_pooling_prefix_config_table() -> None:
    from eval.s15a_embedder_probe import MODELS

    expected = {
        "bge-small": ("cls", "Represent this sentence for searching relevant passages: ", ""),
        "bge-base": ("cls", "Represent this sentence for searching relevant passages: ", ""),
        "gte-base": ("cls", "", ""),
        "e5-base-v2": ("mean", "query: ", "passage: "),
        "nomic": ("mean", "search_query: ", "search_document: "),
    }
    for name, (pooling, qprefix, pprefix) in expected.items():
        cfg = MODELS[name]
        assert cfg.pooling == pooling, f"{name} pooling (mean-vs-CLS swap?)"
        assert cfg.query_prefix == qprefix, f"{name} query prefix"
        assert cfg.passage_prefix == pprefix, f"{name} passage prefix"

    # Base must be the CLS-corrected bge-small (the Slice-20 bug was MEAN pooling).
    assert MODELS["bge-small"].is_base is True
    assert MODELS["bge-small"].pooling == "cls"
    # gte-base is measured but NOT selectable (no candle encoder).
    assert MODELS["gte-base"].in_library_feasible is False


# --------------------------------------------------------------------------- #
# fix-1 [P2]#1 — in-process model cache (steady-state latency, no per-call reload).
# Torch-free: exercises the cache layer directly so it RUNs (not skips) in the
# gate venv (which has no torch). The end-to-end embed_texts proof is the
# integration test below (validated under the eu-0 CPU venv).
# --------------------------------------------------------------------------- #


def test_in_process_model_cache_loads_once(monkeypatch: pytest.MonkeyPatch) -> None:
    from eval import s15a_embedder_probe as m

    # An in-process cache must exist and start empty for this test.
    m._MODEL_CACHE.clear()

    calls: list[tuple[str, str | None, str]] = []

    def fake_uncached(cfg: object) -> tuple[str, str]:
        calls.append((cfg.hf_id, cfg.revision, cfg.pooling))  # type: ignore[attr-defined]
        return (f"tok::{cfg.hf_id}", f"model::{cfg.hf_id}")  # type: ignore[attr-defined]

    monkeypatch.setattr(m, "_load_model_uncached", fake_uncached)

    cfg = m.MODELS["bge-small"]
    a = m._load_model(cfg)
    b = m._load_model(cfg)
    # Loaded ONCE across two calls — steady-state reuse, not a per-call reload.
    assert len(calls) == 1
    # The very same cached (tokenizer, model) tuple is handed back.
    assert a is b

    # A distinct model loads independently (a separate cache key).
    cfg2 = m.MODELS["bge-base"]
    m._load_model(cfg2)
    m._load_model(cfg2)
    assert len(calls) == 2

    # Cache key is (hf_id, revision, pooling).
    keys = set(m._MODEL_CACHE.keys())
    assert (cfg.hf_id, cfg.revision or "default", cfg.pooling) in keys
    assert (cfg2.hf_id, cfg2.revision or "default", cfg2.pooling) in keys


# --------------------------------------------------------------------------- #
# fix-1 [P2]#2 — model revisions pinned to resolved 40-hex commit SHAs.
# Resolved from the local HF cache snapshot; offline + $0. The base bge-small is
# present in the default cache, so this RUNs (not skips) in the gate venv.
# --------------------------------------------------------------------------- #

_SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def _base_in_hf_cache() -> bool:
    from eval.s15a_embedder_probe import MODELS, resolve_revision

    return resolve_revision(MODELS["bge-small"].hf_id) is not None


def test_resolve_revision_from_local_cache() -> None:
    from eval.s15a_embedder_probe import MODELS, resolve_revision

    if not _base_in_hf_cache():
        pytest.skip("bge-small not in local HF cache")
    sha = resolve_revision(MODELS["bge-small"].hf_id)
    assert sha is not None
    assert _SHA_RE.match(sha), f"resolved revision is not a 40-hex SHA: {sha!r}"


def test_model_revisions_are_resolved_shas() -> None:
    from eval.s15a_embedder_probe import BASE_NAME, build_model_revisions

    if not _base_in_hf_cache():
        pytest.skip("bge-small not in local HF cache")
    revs = build_model_revisions([BASE_NAME])
    val = revs[BASE_NAME]
    # NOT the placeholder "default" — a pinned 40-hex commit SHA.
    assert val != "default"
    assert _SHA_RE.match(val), f"model_revisions[{BASE_NAME}] is not a 40-hex SHA: {val!r}"


# --------------------------------------------------------------------------- #
# Integration (CPU, eu-0 venv): require torch; skip otherwise. Pin threads=1.
# --------------------------------------------------------------------------- #


# --------------------------------------------------------------------------- #
# fix-2 [P2]#1 — deterministic Hamming fanout (stable tie-break by doc index).
# argpartition picks an ARBITRARY subset of docs tied at the Kth Hamming
# distance; a tied near-floor candidate can then pass/fail projected_eu7 on an
# unpinned tie. The fix selects the index-ordered (lexicographic (hamming, idx))
# top-K candidate set. Torch-free: numpy-only on hand-built vectors.
# --------------------------------------------------------------------------- #


def test_projected_eu7_deterministic_hamming_fanout() -> None:
    from eval.s15a_embedder_probe import projected_eu7

    # q = all-ones, so 1-bit Hamming distance == the number of negative
    # components. The ham layout below is one where numpy's argpartition tie-break
    # at the fanout boundary (kk=9) keeps doc index 8 and DROPS doc index 6, while
    # a stable index-ordered fanout keeps index 6. We make index 6 the global
    # f32-exact top-1 (dot 29.99), so:
    #   * argpartition (current): index 6 excluded from the 9 Hamming candidates
    #     -> rerank top-1 = index 8 -> overlap with exact {6} = 0 -> proxy 0.0.
    #   * stable index-ordered fanout (fix): index 6 kept -> rerank top-1 = 6 ->
    #     overlap = 1 -> proxy 1.0.
    # ham per index: [0,0,3,0,2,0,1,1,1,1,0,0,0,0]
    docs = np.array(
        [
            [0.1, 0.1, 0.1, 0.1],     # 0  ham0
            [0.1, 0.1, 0.1, 0.1],     # 1  ham0
            [0.1, -0.1, -0.1, -0.1],  # 2  ham3
            [0.1, 0.1, 0.1, 0.1],     # 3  ham0
            [0.1, 0.1, -0.1, -0.1],   # 4  ham2
            [0.1, 0.1, 0.1, 0.1],     # 5  ham0
            [10.0, 10.0, 10.0, -0.01],  # 6  ham1  <- f32-exact best (dot 29.99)
            [0.2, 0.2, 0.2, -0.01],   # 7  ham1
            [5.0, 5.0, 5.0, -0.01],   # 8  ham1  <- argpartition keeps this instead
            [0.2, 0.2, 0.2, -0.01],   # 9  ham1
            [0.1, 0.1, 0.1, 0.1],     # 10 ham0
            [0.1, 0.1, 0.1, 0.1],     # 11 ham0
            [0.1, 0.1, 0.1, 0.1],     # 12 ham0
            [0.1, 0.1, 0.1, 0.1],     # 13 ham0
        ],
        dtype=np.float32,
    )
    q = np.array([[1.0, 1.0, 1.0, 1.0]], dtype=np.float32)

    # Deterministic index-ordered tie-break keeps the f32-best boundary doc.
    val = projected_eu7(docs, q, hamming_k=9, top_k=1, mean_center=False)
    assert val == pytest.approx(1.0)

    # Repeatable run-to-run (no unpinned tie -> identical value every call).
    again = projected_eu7(docs, q, hamming_k=9, top_k=1, mean_center=False)
    assert again == val


# --------------------------------------------------------------------------- #
# fix-2 [P2]#2 — vector cache key must include revision + input identity.
# --resume reused {model}.docs.npy/{model}.queries.npy on ROW COUNT alone, so a
# different resolved revision / changed input set silently fed STALE vectors into
# every gate metric. The fix writes a {model}.meta.json sidecar and reuses the
# cache ONLY when (revision, corpus_hash, n_docs, n_queries, pooling, dim) match.
# Torch-free: embed_texts is monkeypatched to a fresh-vector sentinel.
# --------------------------------------------------------------------------- #


def _tiny_corpus_and_queries():  # type: ignore[no-untyped-def]
    from eval.s15a_embedder_probe import Corpus, GoldQuery

    corpus = Corpus(
        doc_ids=["d0", "d1", "d2"],
        bodies=["b0", "b1", "b2"],
        corpus_hash="HASH_A",
        resolved_count=3,
        sources=["s"],
    )
    queries = [
        GoldQuery(query_id="q0", text="t0", query_class="factual", required_doc_ids=["d0"]),
        GoldQuery(query_id="q1", text="t1", query_class="factual", required_doc_ids=["d1"]),
    ]
    return corpus, queries


_STALE = 7.0
_FRESH = 1.0


def _install_fresh_embedder(m: object, calls: list[str]):  # type: ignore[no-untyped-def]
    def fake_embed_texts(cfg, texts, *, is_query, **kw):  # type: ignore[no-untyped-def]
        calls.append("query" if is_query else "doc")
        return np.full((len(list(texts)), cfg.dim), _FRESH, dtype=np.float32)

    return fake_embed_texts


def test_cache_sidecar_revision_mismatch_reembeds(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    from dataclasses import replace

    from eval import s15a_embedder_probe as m

    corpus, queries = _tiny_corpus_and_queries()
    cfg = replace(m.MODELS["bge-small"], revision="REV_A", dim=4)

    cache_dir = tmp_path
    doc_npy = cache_dir / f"{cfg.name}.docs.npy"
    qry_npy = cache_dir / f"{cfg.name}.queries.npy"
    meta_path = cache_dir / f"{cfg.name}.meta.json"

    # Stale warm vectors with MATCHING row counts (the only thing current code
    # checks) but a sidecar pinning a DIFFERENT revision.
    m._atomic_save_npy(doc_npy, np.full((3, 4), _STALE, np.float32))
    m._atomic_save_npy(qry_npy, np.full((2, 4), _STALE, np.float32))
    import json as _json

    meta_path.write_text(
        _json.dumps(
            {
                "revision": "REV_OLD",
                "corpus_hash": "HASH_A",
                "n_docs": 3,
                "n_queries": 2,
                "pooling": cfg.pooling,
                "dim": 4,
            }
        ),
        encoding="utf-8",
    )

    calls: list[str] = []
    monkeypatch.setattr(m, "embed_texts", _install_fresh_embedder(m, calls))

    docs, qvecs = m.embed_model(
        cfg, corpus, queries, cache_dir=cache_dir, num_threads=1, batch_size=8, resume=True
    )
    # The stale cache must NOT be returned; the revision mismatch forces a re-embed.
    assert not np.array_equal(docs, np.full((3, 4), _STALE, np.float32))
    assert np.array_equal(docs, np.full((3, 4), _FRESH, np.float32))
    assert np.array_equal(qvecs, np.full((2, 4), _FRESH, np.float32))
    assert calls, "expected a re-embed (embed_texts called) on sidecar mismatch"


def test_cache_sidecar_match_reuses(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    from dataclasses import replace

    from eval import s15a_embedder_probe as m

    corpus, queries = _tiny_corpus_and_queries()
    cfg = replace(m.MODELS["bge-small"], revision="REV_A", dim=4)

    cache_dir = tmp_path
    doc_npy = cache_dir / f"{cfg.name}.docs.npy"
    qry_npy = cache_dir / f"{cfg.name}.queries.npy"
    meta_path = cache_dir / f"{cfg.name}.meta.json"

    m._atomic_save_npy(doc_npy, np.full((3, 4), _STALE, np.float32))
    m._atomic_save_npy(qry_npy, np.full((2, 4), _STALE, np.float32))
    import json as _json

    meta_path.write_text(
        _json.dumps(
            {
                "revision": "REV_A",
                "corpus_hash": "HASH_A",
                "n_docs": 3,
                "n_queries": 2,
                "pooling": cfg.pooling,
                "dim": 4,
            }
        ),
        encoding="utf-8",
    )

    calls: list[str] = []
    monkeypatch.setattr(m, "embed_texts", _install_fresh_embedder(m, calls))

    docs, qvecs = m.embed_model(
        cfg, corpus, queries, cache_dir=cache_dir, num_threads=1, batch_size=8, resume=True
    )
    # A fully-matching sidecar -> reuse the warm vectors, no re-embed.
    assert np.array_equal(docs, np.full((3, 4), _STALE, np.float32))
    assert np.array_equal(qvecs, np.full((2, 4), _STALE, np.float32))
    assert calls == [], "matching sidecar must reuse the cache (no embed_texts call)"


@pytest.mark.integration
def test_embed_texts_steady_state_no_reload(monkeypatch: pytest.MonkeyPatch) -> None:
    pytest.importorskip("torch")
    from eval import s15a_embedder_probe as m

    m._MODEL_CACHE.clear()
    real = m._load_model_uncached
    calls: list[str] = []

    def counting(cfg: object) -> tuple[object, object]:
        calls.append(cfg.hf_id)  # type: ignore[attr-defined]
        return real(cfg)  # type: ignore[arg-type]

    monkeypatch.setattr(m, "_load_model_uncached", counting)

    cfg = m.MODELS["bge-small"]
    m.embed_texts(cfg, ["one", "two", "three"], is_query=False, num_threads=1)
    m.embed_texts(cfg, ["four"], is_query=False, num_threads=1)
    m.measure_latency(cfg, ["a", "b", "c"], is_query=False, num_threads=1)
    # from_pretrained-equivalent (_load_model_uncached) called ONCE total across
    # two embed_texts calls AND a multi-text latency measurement.
    assert len(calls) == 1


@pytest.mark.integration
def test_model_cache_byte_identical_vectors() -> None:
    pytest.importorskip("torch")
    from eval import s15a_embedder_probe as m

    cfg = m.MODELS["bge-small"]
    texts = ["the quick brown fox", "a second sentence about memory retrieval"]

    m._MODEL_CACHE.clear()
    v_fresh = m.embed_texts(cfg, texts, is_query=False, num_threads=1)  # cold load
    v_cached = m.embed_texts(cfg, texts, is_query=False, num_threads=1)  # cached reuse
    # Cache changes ONLY caching, not numerics: cached == freshly-loaded.
    assert np.array_equal(v_fresh, v_cached)

    m._MODEL_CACHE.clear()
    v_reload = m.embed_texts(cfg, texts, is_query=False, num_threads=1)  # cold again
    assert np.array_equal(v_fresh, v_reload)


@pytest.mark.integration
def test_embedding_determinism_cpu() -> None:
    pytest.importorskip("torch")
    from eval.s15a_embedder_probe import MODELS, embed_texts

    cfg = MODELS["bge-small"]
    texts = ["the quick brown fox", "a second sentence about memory retrieval"]
    v1 = embed_texts(cfg, texts, is_query=False, num_threads=1)
    v2 = embed_texts(cfg, texts, is_query=False, num_threads=1)
    assert v1.dtype == np.float32
    assert v1.shape == (2, cfg.dim)
    # byte-identical under the pinned (threads=1) runtime.
    assert np.array_equal(v1, v2)
    # L2-normalized.
    norms = np.linalg.norm(v1, axis=1)
    assert np.allclose(norms, 1.0, atol=1e-5)


@pytest.mark.integration
def test_pooling_known_vector_cls_ne_mean() -> None:
    pytest.importorskip("torch")
    from dataclasses import replace

    from eval.s15a_embedder_probe import MODELS, embed_texts

    text = ["a sentence that should pool differently under cls vs mean"]
    cls_cfg = MODELS["bge-small"]
    mean_cfg = replace(cls_cfg, pooling="mean")
    v_cls = embed_texts(cls_cfg, text, is_query=False, num_threads=1)
    v_mean = embed_texts(mean_cfg, text, is_query=False, num_threads=1)
    # A mean-vs-CLS swap is a real difference (the Slice-20 bug); the vectors must
    # NOT be equal.
    assert not np.allclose(v_cls, v_mean, atol=1e-4)
