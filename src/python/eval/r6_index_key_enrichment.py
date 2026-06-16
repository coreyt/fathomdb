"""R6 — Index-key enrichment recall@K vs BM25 (the post-graph-arm pivot lever).

Append each session's extracted entities/facts to ITS OWN doc's FTS content (one row
per doc — NOT separate entity rows, NOT a graph arm) and measure recall@K vs plain
BM25/FTS. Includes a length-matched PLACEBO control to separate "lexical bridge" from
"length artifact" (design §C / review #4). LLM-free, $0; reuses the cached Qwen3.6-27B
extraction. Design: dev/plans/runs/0.8.1-R6-index-key-enrichment-design.md.

Usage:
    python -m eval.r6_index_key_enrichment --per-class 10 \
        --graphs /tmp/gar_dry/extractions.json --output /tmp/r6_n40.json
"""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path
from typing import Optional

from eval.p0a_base_retrieval import (
    DEFAULT_DATASET,
    DEFAULT_SPLIT,
    SMOKE_CLASSES,
    load_lme_smoke,
    run_retrieval_loop,
)
from eval.r2_parity_eval import FathomDBAdapter, NaiveRAGAdapter, _make_doc_id_of


# --------------------------------------------------------------------------- #
# Enrichment (pure functions — pinned by tests/test_r6_enrichment.py)
# --------------------------------------------------------------------------- #
def _entities(graph: dict) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for e in graph.get("entities") or []:
        n = e.get("name") if isinstance(e, dict) else e
        if isinstance(n, str) and n.strip() and n not in seen:
            seen.add(n)
            out.append(n.strip())
    return out


def _facts(graph: dict) -> list[str]:
    out: list[str] = []
    for r in graph.get("relations") or []:
        if not isinstance(r, dict):
            continue
        toks = [str(r.get(k, "")).strip() for k in ("subject", "predicate", "object")]
        toks = [t for t in toks if t]
        if toks:
            out.append(" ".join(toks))
    return out


def enrich_doc(body: str, graph: dict) -> str:
    """Append the doc's extracted entities + fact-triples to its body (one row per
    doc). Deterministic; entity names deduped order-preserving; no-op on empty graph."""
    ents, facts = _entities(graph), _facts(graph)
    if not ents and not facts:
        return body
    blocks = []
    if ents:
        blocks.append("[entities] " + "; ".join(ents))
    if facts:
        blocks.append("[facts] " + "; ".join(facts))
    return body + "\n\n" + "\n".join(blocks)


def placebo_doc(body: str, graph: dict, *, foreign: list[str], seed: int) -> str:
    """Length-matched control: append the SAME token budget as the real enrichment,
    drawn from a FOREIGN vocab (deterministic). If recall moves ≈ as much as real
    enrichment, the effect is a length/corpus-stat artifact, not a lexical bridge."""
    real = enrich_doc(body, graph)
    if real == body:
        return body
    # Tokenize the foreign vocab to single whitespace tokens so the placebo is
    # EXACTLY length-matched (codex §9 [P2]: multi-word names would over-add tokens).
    tokens = [t for f in foreign for t in str(f).split() if t]
    n_added = len(real.split()) - len(body.split())
    if n_added <= 0 or not tokens:
        return body
    rng = random.Random(seed)
    sampled = [rng.choice(tokens) for _ in range(n_added)]
    return body + "\n\n" + " ".join(sampled)


# --------------------------------------------------------------------------- #
# Build (FTS engine over a given doc dict — pinned by the AC tests)
# --------------------------------------------------------------------------- #
def build_fts_engine(documents: dict[str, str], db_path: str):
    """Open an FTS-only FathomDB engine, write one doc row per session (kind="doc",
    no entities/edges), drain. Returns (engine, cursor_to_doc)."""
    from fathomdb.engine import Engine

    p = Path(db_path)
    if p.exists():
        p.unlink()
    eng = Engine.open(str(p), use_default_embedder=False)
    cursor_to_doc: dict[int, str] = {}
    items = list(documents.items())
    for i in range(0, len(items), 64):
        chunk = items[i : i + 64]
        receipt = eng.write([{"kind": "doc", "body": b} for _, b in chunk])
        for (sid, _b), cur in zip(chunk, receipt.row_cursors):
            cursor_to_doc[int(cur)] = sid
    eng.drain(timeout_s=300)
    return eng, cursor_to_doc


def _fts_adapter(documents: dict[str, str], db_path: str) -> FathomDBAdapter:
    eng, cursor_to_doc = build_fts_engine(documents, db_path)
    return FathomDBAdapter(eng, doc_id_of=_make_doc_id_of(cursor_to_doc))


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #
def _pooled_r10(retrieval: dict, variant: str) -> Optional[float]:
    pc = retrieval.get(variant, {}).get("per_class", {})
    num = den = 0.0
    for m in pc.values():
        rv, ns = m.get("recall_at_10"), m.get("n_scored", 0)
        if rv is not None and ns:
            num += rv * ns
            den += ns
    return round(num / den, 3) if den else None


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="R6 index-key enrichment recall@K vs BM25")
    ap.add_argument("--per-class", type=int, default=10)
    ap.add_argument("--seed", type=int, default=20260614)
    ap.add_argument("--graphs", default="/tmp/gar_dry/extractions.json")
    ap.add_argument("--db-dir", default="/tmp/r6")
    ap.add_argument("--output", required=True)
    args = ap.parse_args(argv)

    db_dir = Path(args.db_dir)
    db_dir.mkdir(parents=True, exist_ok=True)
    smoke = load_lme_smoke(DEFAULT_DATASET, DEFAULT_SPLIT, per_class=args.per_class,
                           seed=args.seed, classes=SMOKE_CLASSES)
    graphs = json.loads(Path(args.graphs).read_text())
    graphs = {k: v for k, v in graphs.items() if k in smoke.documents}
    cov = sum(1 for s in smoke.documents if graphs.get(s))
    print(f"[load] {len(smoke.questions)} Q | {len(smoke.documents)} sessions | "
          f"{cov} have a cached graph ({cov / max(len(smoke.documents),1):.0%})")

    import hashlib

    enriched = {s: enrich_doc(b, graphs.get(s, {})) for s, b in smoke.documents.items()}
    # Global foreign vocab as single tokens. Per-doc placebo EXCLUDES the doc's own
    # entity tokens (codex §9 [P2] #1 — placebo must be foreign-only) and uses a
    # process-STABLE seed (hashlib, not the salted built-in hash — [P2] #2).
    global_tokens = [t for g in graphs.values() for n in _entities(g) for t in n.split()]

    def _placebo(s: str, b: str) -> str:
        own = {t for n in _entities(graphs.get(s, {})) for t in n.split()}
        pool = [t for t in global_tokens if t not in own]
        seed = args.seed ^ int.from_bytes(hashlib.blake2b(s.encode(), digest_size=4).digest(), "big")
        return placebo_doc(b, graphs.get(s, {}), foreign=pool, seed=seed)

    placebo = {s: _placebo(s, b) for s, b in smoke.documents.items()}

    systems = {
        "naive_bm25": NaiveRAGAdapter(smoke.documents),
        "naive_bm25_enriched": NaiveRAGAdapter(enriched),
        "fathomdb_fts_only": _fts_adapter(smoke.documents, str(db_dir / "fts_plain.sqlite")),
        "fathomdb_fts_enriched": _fts_adapter(enriched, str(db_dir / "fts_enriched.sqlite")),
        "fathomdb_fts_placebo": _fts_adapter(placebo, str(db_dir / "fts_placebo.sqlite")),
    }
    retrieval = run_retrieval_loop(smoke, systems)

    result = {
        "mode": "r6-index-key-enrichment",
        "n_questions": len(smoke.questions),
        "n_sessions": len(smoke.documents),
        "graph_coverage": cov,
        "variants": sorted(systems.keys()),
        "retrieval_loop": retrieval,
    }
    Path(args.output).write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(f"[r6] wrote {args.output}")

    pooled = {v: _pooled_r10(retrieval, v) for v in systems}
    print("\n=== pooled R@10 ===")
    for v in ("naive_bm25", "naive_bm25_enriched", "fathomdb_fts_only",
              "fathomdb_fts_enriched", "fathomdb_fts_placebo"):
        print(f"  {v:<24} {pooled.get(v)}")
    # Harness sanity (design §C): bm25 must reproduce ~0.70.
    if pooled.get("naive_bm25") is not None:
        print(f"\n[sanity] naive_bm25 pooled R@10 = {pooled['naive_bm25']} (report anchor ~0.70)")
    # Pre-registered primary endpoint + the placebo attribution guard.
    fe, fp, pl = pooled.get("fathomdb_fts_enriched"), pooled.get("fathomdb_fts_only"), pooled.get("fathomdb_fts_placebo")
    if None not in (fe, fp):
        print(f"[PRIMARY] FTS enriched − plain (pooled R@10) = {fe - fp:+.3f}")
    if None not in (pl, fp):
        print(f"[placebo] FTS placebo  − plain (pooled R@10) = {pl - fp:+.3f}  "
              f"(if ≈ primary → length artifact, not a lexical bridge)")
    for v in systems.values():
        eng = getattr(v, "_engine", None)
        if eng is not None:
            eng.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
