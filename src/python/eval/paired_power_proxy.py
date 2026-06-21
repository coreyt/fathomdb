#!/usr/bin/env python3
"""$0 LLM-free **paired-power** proxy for the 0.8.3 Mem0-parity corpus.

**Why this exists (Slice-10 RED precondition).** The Slice-5 re-pin manifest
(``0.8.3-d0a-corpus-manifest.json``) reported an ``implied_n_for_mde_le_eps`` per
class of 371–768 vs the built N≈150 — but that proxy is the **UNPAIRED** bound
(``var_diff = 2·p(1−p)``, i.e. it assumes the two arms are uncorrelated, ρ=0).
The 0.8.3 gate (``decision_rule_083.decide_083``) is a **paired** FathomDB−Mem0
test, and pairing shrinks the variance by ``(1−ρ)``. If the real arms are
positively correlated the true implied-N is far below the unpaired bound; if
ρ≈0 it is not. **Which side of the line ``multi_session`` and ``temporal`` fall
on decides whether the current corpus can support a parity claim at all** — so
this must be known *before* Slice 10 spends priced budget, not discovered during
it (cheap-validate-before-spend).

**What it does.** For each memory class it runs two genuinely-different $0
lexical retrieval arms (BM25 and TF-IDF-cosine — pure Python, deterministic) over
the gold, measures their per-query strict-recall@k agreement, and derives the
**measured outcome correlation ρ̂**. It then reports the implied-N as a function
of ρ — at ρ=0 it reproduces the Slice-5 unpaired number exactly; at ρ̂ it gives
the realistic paired estimate; and it brackets ``multi_session`` / ``temporal``
adequacy under both the LongMemEval-only corpus and the LongMemEval **+ LOCOMO**
combined corpus.

**Honest caveat (do not overclaim).** ρ̂ is measured between two *lexical* arms;
the real FathomDB-vs-Mem0 ρ may differ. So the verdict is three-way and
conservative: a class is ``robust`` only if powered even at ρ=0 (the unpaired
upper bound); ``powered-if-paired`` if powered at ρ̂ but not at ρ=0; and
``UNDERPOWERED`` if not powered even at ρ̂. The authoritative paired power-check
remains Slice 10 on the real arms — this proxy de-risks it, it does not replace
the frozen rule.

Footprint: EVAL-ONLY, $0, no LLM, no network at analysis time (LongMemEval doc
bodies are streamed once via ``datasets`` to build the haystack, same as the
Slice-5 builder; LOCOMO is local). Deterministic.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

from eval.decision_rule_083 import EPS_NEAR_PARITY, MEMORY_CLASSES
from eval.r2_parity_eval import NaiveRAGAdapter, _tokenize

_Z = 1.96  # 95% two-sided


# --------------------------------------------------------------------------- #
# Second arm — TF-IDF cosine (a different paradigm from BM25 → ρ̂ < 1)
# --------------------------------------------------------------------------- #


class TfidfCosineAdapter:
    """Deterministic pure-Python TF-IDF cosine retriever. Different length
    normalisation + no BM25 saturation than :class:`NaiveRAGAdapter`, so the two
    arms disagree on a non-trivial fraction of queries — that disagreement is the
    ρ̂ signal."""

    name = "tfidf_cos"

    def __init__(self, documents: dict[str, str]) -> None:
        n_docs = max(len(documents), 1)
        df: dict[str, int] = defaultdict(int)
        self._tf: dict[str, dict[str, int]] = {}
        for doc_id, body in documents.items():
            counts: dict[str, int] = defaultdict(int)
            for t in _tokenize(body):
                counts[t] += 1
            self._tf[doc_id] = counts
            for t in counts:
                df[t] += 1
        self._idf = {t: math.log(n_docs / n) + 1.0 for t, n in df.items()}
        # Pre-compute L2 norms of the tf-idf doc vectors.
        self._norm: dict[str, float] = {}
        for doc_id, counts in self._tf.items():
            s = sum((f * self._idf.get(t, 0.0)) ** 2 for t, f in counts.items())
            self._norm[doc_id] = math.sqrt(s) or 1e-9

    def retrieve(self, question: str, k: int) -> list[tuple[str, float]]:
        q_counts: dict[str, int] = defaultdict(int)
        for t in _tokenize(question):
            q_counts[t] += 1
        q_vec = {t: f * self._idf.get(t, 0.0) for t, f in q_counts.items()}
        q_norm = math.sqrt(sum(v * v for v in q_vec.values())) or 1e-9
        scores: dict[str, float] = defaultdict(float)
        for t, qw in q_vec.items():
            if qw == 0.0:
                continue
            for doc_id, counts in self._tf.items():
                f = counts.get(t)
                if f:
                    scores[doc_id] += qw * (f * self._idf.get(t, 0.0))
        ranked = sorted(
            ((d, s / (self._norm[d] * q_norm)) for d, s in scores.items()),
            key=lambda kv: kv[1],
            reverse=True,
        )[:k]
        return ranked


# --------------------------------------------------------------------------- #
# Per-class paired analysis
# --------------------------------------------------------------------------- #


def _strict_hit(retrieved_ids: list[str], gold_ids: list[str]) -> float:
    rset = set(retrieved_ids)
    return 1.0 if gold_ids and all(g in rset for g in gold_ids) else 0.0


def _popvar(vals: list[float]) -> float:
    n = len(vals)
    if n == 0:
        return 0.0
    m = sum(vals) / n
    return sum((v - m) ** 2 for v in vals) / n


def _implied_n(var_diff: float, eps: float) -> int | None:
    """Two-sample/paired N for MDE ≤ eps given the difference-outcome variance."""
    if var_diff <= 0.0:
        return 0
    return math.ceil((_Z / eps) ** 2 * var_diff)


def analyze(
    documents: dict[str, str],
    gold_queries: list[dict[str, Any]],
    *,
    k: int = 10,
    eps: float = EPS_NEAR_PARITY,
) -> dict[str, dict[str, Any]]:
    """Per-class paired-power analysis over ``documents`` + ``gold_queries``.

    Returns ``{class: {n, p_bm25, p_tfidf, rho_hat, paired_implied_n,
    unpaired_implied_n, implied_n_by_rho}}``. ``unpaired_implied_n`` reproduces
    the Slice-5 manifest number (ρ=0). ``implied_n_by_rho`` is the parity-level
    estimate ``ceil((z/eps)²·2·p̄·(1−p̄)·(1−ρ))`` for a grid of ρ — this isolates
    the *correlation* structure from the proxy arms' incidental performance gap.
    """
    bm25 = NaiveRAGAdapter(documents)
    tfidf = TfidfCosineAdapter(documents)

    succ: dict[str, list[tuple[float, float]]] = defaultdict(list)
    for q in gold_queries:
        gold_ids = [str(e["doc_id"]) for e in (q.get("required_evidence") or []) if e.get("doc_id")]
        gold_ids = [g for g in gold_ids if g in documents]
        if not gold_ids:
            continue
        question = str(q.get("query", ""))
        a = _strict_hit([h.doc_id for h in bm25.retrieve(question, k)], gold_ids)
        b = _strict_hit([d for d, _ in tfidf.retrieve(question, k)], gold_ids)
        succ[str(q.get("query_class"))].append((a, b))

    out: dict[str, dict[str, Any]] = {}
    rho_grid = [0.0, 0.3, 0.5, 0.7]
    for cls in sorted(succ):
        pairs = succ[cls]
        n = len(pairs)
        a_vals = [a for a, _ in pairs]
        b_vals = [b for _, b in pairs]
        pa = sum(a_vals) / n if n else 0.0
        pb = sum(b_vals) / n if n else 0.0
        var_a, var_b = _popvar(a_vals), _popvar(b_vals)
        cov = (sum(a * b for a, b in pairs) / n - pa * pb) if n else 0.0
        rho = cov / math.sqrt(var_a * var_b) if var_a > 0 and var_b > 0 else None
        d_vals = [a - b for a, b in pairs]
        var_d = _popvar(d_vals)
        # parity-level pooled p̄ (use BM25 arm recall — continuity with Slice-5)
        pbar = pa
        base = (_Z / eps) ** 2 * 2.0 * pbar * (1.0 - pbar)
        grid = {f"rho={r}": (math.ceil(base * (1.0 - r)) if base > 0 else 0) for r in rho_grid}
        if rho is not None:
            grid["rho=measured"] = math.ceil(base * (1.0 - rho)) if base > 0 else 0
        out[cls] = {
            "n": n,
            "p_bm25": round(pa, 4),
            "p_tfidf": round(pb, 4),
            "rho_hat": round(rho, 4) if rho is not None else None,
            "var_diff_empirical": round(var_d, 4),
            "paired_implied_n_empirical": _implied_n(var_d, eps),
            "unpaired_implied_n": _implied_n(2.0 * pbar * (1.0 - pbar), eps),
            "implied_n_by_rho": grid,
        }
    return out


def _verdict(available_n: int, cls_stats: dict[str, Any]) -> str:
    """Three-way conservative verdict for a class at a given available N."""
    unp = cls_stats["unpaired_implied_n"]
    grid = cls_stats["implied_n_by_rho"]
    paired = grid.get("rho=measured", grid.get("rho=0.5"))
    if unp is not None and available_n >= unp:
        return "robust"
    if paired is not None and available_n >= paired:
        return "powered-if-paired"
    return "UNDERPOWERED"


# --------------------------------------------------------------------------- #
# Corpus loading + CLI
# --------------------------------------------------------------------------- #


def _load_lme_docs_and_gold(
    repin_gold_path: str, dataset: str, split: str
) -> tuple[dict[str, str], list[dict[str, Any]]]:
    """Stream the LongMemEval haystack (for retrieval) and load the pinned D0a
    gold queries (incl. synthetic augments) from the re-pin file."""
    from eval.gold_repin import load_lme  # local import: pulls `datasets`

    documents, _real_gold, _sessions = load_lme(dataset, split)
    raw = json.loads(Path(repin_gold_path).read_text(encoding="utf-8"))
    return documents, list(raw.get("queries", []))


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="0.8.3 paired-power proxy (Slice-10 RED precondition)")
    p.add_argument("--repin-gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    p.add_argument("--lme-dataset", default="xiaowu0162/longmemeval-cleaned")
    p.add_argument("--lme-split", default="oracle")
    p.add_argument("--locomo", default="data/corpus-data/raw/locomo10.json")
    p.add_argument("--k", type=int, default=10)
    p.add_argument("--eps", type=float, default=EPS_NEAR_PARITY)
    p.add_argument("--out", default="dev/plans/runs/0.8.3-paired-power-proxy.json")
    p.add_argument("--no-lme", action="store_true", help="skip LME (LOCOMO-only run)")
    a = p.parse_args(argv)

    report: dict[str, Any] = {
        "schema": "0.8.3-paired-power-proxy-v1",
        "eps": a.eps,
        "k": a.k,
        "note": (
            "rho=0 reproduces the Slice-5 UNPAIRED implied_n; rho=measured is the "
            "lexical-arm paired estimate; the authoritative paired check is Slice 10."
        ),
        "sources": {},
        "combined_adequacy": {},
    }

    lme_stats: dict[str, Any] = {}
    locomo_stats: dict[str, Any] = {}

    if not a.no_lme:
        print("[paired-power] loading LongMemEval haystack + D0a gold …", file=sys.stderr, flush=True)
        lme_docs, lme_gold = _load_lme_docs_and_gold(a.repin_gold, a.lme_dataset, a.lme_split)
        lme_stats = analyze(lme_docs, lme_gold, k=a.k, eps=a.eps)
        report["sources"]["longmemeval"] = {
            "n_docs": len(lme_docs),
            "per_class": lme_stats,
        }

    if a.locomo and Path(a.locomo).exists():
        print("[paired-power] loading LOCOMO …", file=sys.stderr, flush=True)
        from eval.locomo_loader import load_locomo

        loco_docs, loco_gold = load_locomo(a.locomo)
        locomo_stats = analyze(loco_docs, loco_gold, k=a.k, eps=a.eps)
        report["sources"]["locomo"] = {
            "n_docs": len(loco_docs),
            "license": "CC-BY-NC-4.0 — EVAL-ONLY, not committed",
            "per_class": locomo_stats,
        }

    # Combined per-class adequacy: available N = LME-gold count + LOCOMO-gold
    # count; required N taken at rho=measured (fallback rho=0.5) from whichever
    # source has the class, preferring the higher (more conservative) requirement.
    for cls in MEMORY_CLASSES:
        lme = lme_stats.get(cls)
        loco = locomo_stats.get(cls)
        n_lme = (lme or {}).get("n", 0)
        n_loco = (loco or {}).get("n", 0)
        available = n_lme + n_loco
        # conservative required-N: max across the sources that cover the class
        reqs = []
        for s in (lme, loco):
            if not s:
                continue
            g = s["implied_n_by_rho"]
            reqs.append(g.get("rho=measured", g.get("rho=0.5")))
        required_paired = max([r for r in reqs if r is not None], default=None)
        unp = [s["unpaired_implied_n"] for s in (lme, loco) if s and s["unpaired_implied_n"] is not None]
        required_unpaired = max(unp, default=None)
        # verdict at the combined available N against the conservative requirement
        verdict = "no-data"
        if required_unpaired is not None and available >= required_unpaired:
            verdict = "robust"
        elif required_paired is not None and available >= required_paired:
            verdict = "powered-if-paired"
        elif required_paired is not None:
            verdict = "UNDERPOWERED"
        report["combined_adequacy"][cls] = {
            "n_longmemeval_gold": n_lme,
            "n_locomo_gold": n_loco,
            "n_available_combined": available,
            "required_n_unpaired": required_unpaired,
            "required_n_paired_est": required_paired,
            "verdict_combined": verdict,
            "lme_only_verdict": _verdict(n_lme, lme) if lme else "no-data",
        }

    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report["combined_adequacy"], indent=2))
    print(f"[paired-power] wrote {a.out}", file=sys.stderr, flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
