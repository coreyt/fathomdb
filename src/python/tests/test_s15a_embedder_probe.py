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
# Integration (CPU, eu-0 venv): require torch; skip otherwise. Pin threads=1.
# --------------------------------------------------------------------------- #


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
