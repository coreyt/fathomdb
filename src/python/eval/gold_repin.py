#!/usr/bin/env python3
"""0.8.3 Slice 5 (D0a) — power-sized memory-class gold re-pin builder.

Produces the re-pinned gold + corpus manifest the Slice-10 priced parity pass
consumes, fixing the Slice-25 **N=0 gold** defect on the memory classes.

Design: ``dev/design/0.8.3-slice-5-design.md`` (§B). Contract:
``dev/design/0.8.3-mem0-parity.md`` §3 (LongMemEval-derived), §5 (power-sizing rule,
ε=0.05). Consumes the frozen :data:`eval.decision_rule_083.MEMORY_CLASSES`.

Build = **LongMemEval real labelled gold** (the authoritative base) **+** a
**local-Qwen ``gold_gen`` top-up** for any class short of ``n_min``, class-targeted
over the LME sessions of that class. Every query is ``_provenance``-tagged
(``lme-real`` | ``qwen-aug:<model>``). $0 OFFLINE-BUILD (local Qwen); the only
network seam is the answerer (cheap-validate only, elsewhere).

Resilient ([[priced-runs-need-resilience-before-spend]]): per-class augmentation is
checkpointed to ``<out>.checkpoint.json`` after each class and reused on ``--resume``;
the LLM call retries with backoff on transient empties / 429s.

Usage:
    python -m eval.gold_repin --out-gold dev/plans/runs/0.8.3-d0a-memory-gold.json \\
        --out-manifest dev/plans/runs/0.8.3-d0a-corpus-manifest.json \\
        --n-min 150 --gen-model qwen3.6-27b
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
import time
from collections import defaultdict
from pathlib import Path
from typing import Any

from eval import gold_gen
from eval.decision_rule_083 import EPS_NEAR_PARITY, MEMORY_CLASSES
from eval.r2_parity_eval import (
    LME_CLASS_MAP,
    NaiveRAGAdapter,
    _format_lme_session,
)

_DEFAULT_DATASET = "xiaowu0162/longmemeval-cleaned"


# --------------------------------------------------------------------------- #
# LongMemEval load (single streaming pass → docs + gold + per-class sessions)
# --------------------------------------------------------------------------- #


def load_lme(
    dataset: str,
    split: str,
    *,
    question_limit: int | None = None,
) -> tuple[dict[str, str], list[dict[str, Any]], dict[str, list[tuple[str, str]]]]:
    """Return ``(documents, gold_queries, class_sessions)`` from LongMemEval.

    * ``documents``      — ``{session_id: body}`` (all unique haystack sessions);
    * ``gold_queries``   — real LME gold as gold-file query dicts (``_provenance``
                           ``lme-real``);
    * ``class_sessions`` — ``{reporting_class: [(session_id, body), ...]}`` (the
                           haystack sessions of instances of that class — the
                           targeted augmentation source).
    """
    from datasets import load_dataset  # type: ignore[import]

    split_map = {
        "oracle": "longmemeval_oracle",
        "s": "longmemeval_s_cleaned",
        "m": "longmemeval_m_cleaned",
    }
    hf_split = split_map.get(split, split)
    ds = load_dataset(dataset, split=hf_split, streaming=True)

    documents: dict[str, str] = {}
    gold_queries: list[dict[str, Any]] = []
    class_sessions: dict[str, list[tuple[str, str]]] = defaultdict(list)
    seen_class_sid: set[tuple[str, str]] = set()

    for i, inst in enumerate(ds):
        if question_limit is not None and i >= question_limit:
            break
        sids = inst.get("haystack_session_ids") or []
        sessions = inst.get("haystack_sessions") or []
        for sid, turns in zip(sids, sessions):
            if sid not in documents:
                documents[sid] = _format_lme_session(turns)

        q_type = str(inst.get("question_type") or "unknown")
        reporting = LME_CLASS_MAP.get(q_type, "unknown")
        if reporting not in MEMORY_CLASSES:
            continue  # only the four resolution classes are re-pinned here

        answer_sids = [str(s) for s in (inst.get("answer_session_ids") or [])]
        answer_text = str(inst.get("answer") or "")
        gold_queries.append(
            {
                "query_id": str(inst.get("question_id") or f"lme-{i}"),
                "query": str(inst.get("question") or ""),
                "query_class": reporting,
                "required_evidence": [{"doc_id": s} for s in answer_sids],
                "answers": [answer_text] if answer_text else [],
                "_provenance": "lme-real",
                "_source": "longmemeval",
            }
        )
        for sid in sids:
            key = (reporting, sid)
            if key not in seen_class_sid:
                seen_class_sid.add(key)
                class_sessions[reporting].append((sid, documents[sid]))

    return documents, gold_queries, dict(class_sessions)


# --------------------------------------------------------------------------- #
# Augmentation (local-Qwen top-up of an under-n_min class) — resilient
# --------------------------------------------------------------------------- #


def _norm(text: str) -> str:
    return re.sub(r"\s+", " ", str(text).lower().strip())


def _call_with_backoff(
    sessions_batch: list[tuple[str, str]],
    query_class: str,
    *,
    base_url: str,
    api_key: str,
    model: str,
    retries: int = 3,
) -> list[dict[str, Any]]:
    """One targeted batch with exponential backoff on transient empties / 429s."""
    delay = 5.0
    for attempt in range(retries):
        out = gold_gen.generate_class_targeted(
            sessions_batch, query_class, base_url=base_url, api_key=api_key, model=model
        )
        if out:
            return out
        if attempt < retries - 1:
            print(f"[gold_repin]   empty batch ({query_class}); backoff {delay:.0f}s",
                  file=sys.stderr, flush=True)
            time.sleep(delay)
            delay *= 2
    return []


def augment_class(
    query_class: str,
    deficit: int,
    sessions: list[tuple[str, str]],
    corpus_doc_ids: set[str],
    existing_norms: set[str],
    *,
    base_url: str,
    api_key: str,
    model: str,
    batch_size: int = 4,
    max_batches: int = 60,
) -> list[dict[str, Any]]:
    """Generate up to ``deficit`` validated, deduped ``query_class`` queries from
    ``sessions``. Validates: class match, ≥1 ``required_evidence`` doc_id present in
    the corpus, non-empty answers, and not a duplicate of existing/generated gold."""
    accepted: list[dict[str, Any]] = []
    norms = set(existing_norms)
    n_batches = min(max_batches, math.ceil(len(sessions) / batch_size) if sessions else 0)
    for b in range(n_batches):
        if len(accepted) >= deficit:
            break
        batch = sessions[b * batch_size : (b + 1) * batch_size]
        if not batch:
            break
        raw = _call_with_backoff(batch, query_class, base_url=base_url, api_key=api_key, model=model)
        for q in raw:
            ev = [str(e.get("doc_id")) for e in (q.get("required_evidence") or []) if e.get("doc_id")]
            ev = [d for d in ev if d in corpus_doc_ids]
            ans = [str(a) for a in (q.get("answers") or []) if str(a).strip()]
            qtext = str(q.get("query", "")).strip()
            n = _norm(qtext)
            if not qtext or not ev or not ans or n in norms:
                continue
            norms.add(n)
            accepted.append(
                {
                    "query": qtext,
                    "query_class": query_class,
                    "required_evidence": [{"doc_id": d} for d in ev],
                    "answers": ans,
                    "_provenance": f"qwen-aug:{model}",
                    "_source": "gold_gen-targeted",
                }
            )
            if len(accepted) >= deficit:
                break
        print(f"[gold_repin]   {query_class}: +{len(accepted)}/{deficit} "
              f"(batch {b + 1}/{n_batches})", file=sys.stderr, flush=True)
    return accepted


# --------------------------------------------------------------------------- #
# Variance proxy (Slice-5 power signal for Slice-10's paired power-check)
# --------------------------------------------------------------------------- #


def variance_proxy(
    documents: dict[str, str],
    gold_queries: list[dict[str, Any]],
    *,
    k: int = 10,
    eps: float = EPS_NEAR_PARITY,
) -> dict[str, dict[str, Any]]:
    """Per-class single-arm naive-RAG strict recall@k variance + implied two-sample
    N for MDE ≤ ε. A coarse UNPAIRED upper-bound proxy (pairing shrinks it) — the
    real paired power-check is Slice 10."""
    adapter = NaiveRAGAdapter(documents)
    by_class: dict[str, list[float]] = defaultdict(list)
    for q in gold_queries:
        gold = [str(e["doc_id"]) for e in (q.get("required_evidence") or []) if e.get("doc_id")]
        if not gold:
            continue
        retrieved = {h.doc_id for h in adapter.retrieve(str(q.get("query", "")), k)}
        by_class[str(q.get("query_class"))].append(1.0 if all(g in retrieved for g in gold) else 0.0)

    out: dict[str, dict[str, Any]] = {}
    for cls, vals in by_class.items():
        n = len(vals)
        p = sum(vals) / n if n else 0.0
        var = p * (1.0 - p)
        implied = (
            math.ceil((1.96 * math.sqrt(2.0 * var) / eps) ** 2) if 0.0 < p < 1.0 else None
        )
        out[cls] = {
            "n_eval": n,
            "recall_at_k_proxy": round(p, 4),
            "outcome_variance": round(var, 4),
            "implied_n_for_mde_le_eps": implied,
        }
    return out


# --------------------------------------------------------------------------- #
# corpus hash + assembly
# --------------------------------------------------------------------------- #


def corpus_hash(documents: dict[str, str]) -> str:
    """Deterministic sha256 over the sorted ``doc_id\\nbody`` corpus."""
    h = hashlib.sha256()
    for did in sorted(documents):
        h.update(did.encode("utf-8"))
        h.update(b"\n")
        h.update(documents[did].encode("utf-8"))
        h.update(b"\n")
    return h.hexdigest()


def build_repin(
    *,
    dataset: str,
    split: str,
    n_min: int,
    base_url: str,
    api_key: str,
    gen_model: str,
    seed: int,
    out_gold: Path,
    out_manifest: Path,
    augment: bool = True,
    question_limit: int | None = None,
    resume: bool = True,
) -> dict[str, Any]:
    print(f"[gold_repin] loading {dataset} split={split}", file=sys.stderr, flush=True)
    documents, gold_queries, class_sessions = load_lme(dataset, split, question_limit=question_limit)
    corpus_doc_ids = set(documents)
    print(f"[gold_repin] LME: {len(documents)} sessions, {len(gold_queries)} real memory-class gold",
          file=sys.stderr, flush=True)

    by_class: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for q in gold_queries:
        by_class[str(q["query_class"])].append(q)

    ckpt_path = out_gold.with_suffix(".checkpoint.json")
    ckpt: dict[str, list[dict[str, Any]]] = {}
    if resume and ckpt_path.exists():
        ckpt = json.loads(ckpt_path.read_text(encoding="utf-8")).get("augmented", {})
        print(f"[gold_repin] resume: loaded checkpoint with classes {list(ckpt)}",
              file=sys.stderr, flush=True)

    augmented: dict[str, list[dict[str, Any]]] = {}
    for cls in MEMORY_CLASSES:
        have = len(by_class.get(cls, []))
        deficit = max(0, n_min - have)
        if cls in ckpt:
            augmented[cls] = ckpt[cls]
            print(f"[gold_repin] {cls}: reuse {len(ckpt[cls])} checkpointed aug",
                  file=sys.stderr, flush=True)
            continue
        if deficit == 0 or not augment:
            augmented[cls] = []
            continue
        print(f"[gold_repin] {cls}: real={have} deficit={deficit} → augmenting via {gen_model}",
              file=sys.stderr, flush=True)
        existing_norms = {_norm(q["query"]) for q in by_class.get(cls, [])}
        sessions = class_sessions.get(cls, [])
        augmented[cls] = augment_class(
            cls, deficit, sessions, corpus_doc_ids, existing_norms,
            base_url=base_url, api_key=api_key, model=gen_model,
        )
        ckpt[cls] = augmented[cls]
        ckpt_path.write_text(json.dumps({"augmented": ckpt}, indent=2), encoding="utf-8")

    # Assemble + assign deterministic ids.
    all_queries: list[dict[str, Any]] = []
    counter = 0
    for cls in sorted({*MEMORY_CLASSES, *by_class}):
        for q in [*by_class.get(cls, []), *augmented.get(cls, [])]:
            counter += 1
            q = dict(q)
            q.setdefault("query_id", f"d0a-{counter:04d}")
            all_queries.append(q)

    ch = corpus_hash(documents)
    per_class_counts = {c: len(by_class.get(c, [])) + len(augmented.get(c, [])) for c in MEMORY_CLASSES}
    synthetic_fraction = {
        c: round(len(augmented.get(c, [])) / per_class_counts[c], 4) if per_class_counts[c] else 0.0
        for c in MEMORY_CLASSES
    }
    var_proxy = variance_proxy(documents, all_queries)

    gold_doc = {
        "version": "0.8.3-d0a-repin-v1",
        "corpus_hash": ch,
        "qrels_version": f"repin-{ch[:8]}",
        "generator_model": gen_model,
        "seed": seed,
        "n_min": n_min,
        "source": f"{dataset}:{split}",
        "queries": all_queries,
    }
    out_gold.parent.mkdir(parents=True, exist_ok=True)
    out_gold.write_text(json.dumps(gold_doc, indent=2), encoding="utf-8")

    manifest = {
        "schema": "0.8.3-d0a-corpus-manifest-v1",
        "generated_by": "src/python/eval/gold_repin.py",
        "corpus_hash": ch,
        "qrels_version": f"repin-{ch[:8]}",
        "n_min": n_min,
        "per_class_gold_counts": per_class_counts,
        "synthetic_fraction": synthetic_fraction,
        "per_class_variance": var_proxy,
        "doc_source": {
            "dataset": dataset,
            "split": split,
            "n_sessions": len(documents),
            "sampling": "all unique haystack sessions of memory-class instances",
        },
        "generator_model": gen_model,
        "seed": seed,
        "gold_path": str(out_gold).replace(str(out_gold.parent.parent.parent.parent) + "/", ""),
        "memory_classes": list(MEMORY_CLASSES),
        "eps_near_parity": EPS_NEAR_PARITY,
        "power_note": (
            "n_min=150 is the design provisional BUILD target; the per_class_variance "
            "implied_n_for_mde_le_eps proxy shows MDE<=eps needs N>>150 — Slice 10 runs "
            "the real paired power-check and escalates to HITL if mde>eps (no parity "
            "claim on an under-powered class)."
        ),
    }
    out_manifest.parent.mkdir(parents=True, exist_ok=True)
    out_manifest.write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    print(f"[gold_repin] wrote {out_gold} ({len(all_queries)} queries) + {out_manifest}",
          file=sys.stderr, flush=True)
    print(f"[gold_repin] per_class_gold_counts={per_class_counts} corpus_hash={ch[:12]}",
          file=sys.stderr, flush=True)
    return manifest


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="0.8.3 D0a memory-class gold re-pin builder")
    p.add_argument("--dataset", default=_DEFAULT_DATASET)
    p.add_argument("--split", default="oracle", choices=["oracle", "s", "m"])
    p.add_argument("--n-min", type=int, default=150)
    p.add_argument("--base-url", default="http://localhost:4000/v1")
    p.add_argument("--api-key", default="sk-airlock-mk")
    p.add_argument("--gen-model", default="qwen3.6-27b")
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--out-gold", required=True)
    p.add_argument("--out-manifest", required=True)
    p.add_argument("--no-augment", action="store_true")
    p.add_argument("--no-resume", action="store_true")
    p.add_argument("--question-limit", type=int, default=None)
    a = p.parse_args(argv)

    build_repin(
        dataset=a.dataset, split=a.split, n_min=a.n_min,
        base_url=a.base_url, api_key=a.api_key, gen_model=a.gen_model, seed=a.seed,
        out_gold=Path(a.out_gold), out_manifest=Path(a.out_manifest),
        augment=not a.no_augment, question_limit=a.question_limit, resume=not a.no_resume,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
