"""M1 (0.8.2 Slice 10) — per-question knowledge-graph build over MuSiQue-Ans.

Builds the per-question graph the **PPR arm (Slice 15)** will traverse, over the
**same** pinned corpus the baseline (Slice 5) scores on
(``musique_hash 3cff37fd…``; see ``dev/plans/runs/0.8.2-m1-corpus-manifest.json``).

Pipeline (all $0 — local GPU extraction, no priced API):
  1. Reproduce the Slice-4 corpus (``acquire_musique.py``) and read its rows.
  2. **Extract** entities + fact-edges per MuSiQue **paragraph** (custom_id
     ``"{qid}#{idx}"``) by reusing ``eval.graph_arm_recall.extract_graph`` —
     Qwen3.6-27B via the Airlock vLLM **batch** gateway (``enable_thinking:false``,
     $0 local). Per-paragraph granularity preserves passage→entity membership that
     HippoRAG-style PPR (Slice 15) needs. Extraction is **cached incrementally**
     (keyed by paragraph custom_id) so a re-run extracts only missing paragraphs.
  3. **Load** into a FathomDB engine (``canonical_nodes`` / ``canonical_edges``),
     entities namespaced **per question** (``logical_id = "{qid}|ent:{norm}"``) so
     identically-named entities in different questions do NOT merge across the
     distractor sets. Each question is its own isolated subgraph (MuSiQue's
     distractor paragraphs are per-question, not a shared haystack).
  4. **Verify coverage** the way ``eval.verify_embed_db`` verifies embeds —
     drain/terminal status can lie ([[embed-completeness-and-gpu-readiness]]): a
     read-only ``mode=ro`` sqlite pass over ``canonical_nodes`` / ``canonical_edges``
     confirming **every sampled question has a non-empty graph** and the node/edge
     tables are populated.

**BODY-LESS EDGES (the load-bearing adaptation).** Edges are written **without** a
``body``. Setting an edge ``body`` makes the engine auto-enqueue ``edge_fact`` vector
projection (lib.rs), which (a) needs an embedder (else SchedulerError) and (b) hits a
StorageError scale bug at ~8-14k edges — the known 0.8.1 engine follow-up. The
relation still connects its two entities for PPR traversal; provenance rides the
edge's ``source_id`` (the question id) and the per-paragraph cache.

  NOTE on the reused asset: the Slice-10 prompt/plan states
  ``graph_arm_recall.build_graph_engine`` "builds edges WITH body by default" and that
  body-less is the opposite of its default. At this baseline (92ed09e) the asset
  ALREADY builds edges body-less (see its ``build_graph_engine`` — the edge dict has no
  ``body`` key). So body-less is **consistent with**, not opposite to, the current
  asset; this module keeps edges body-less **explicitly** regardless.

**PII redaction.** The reused ``extract_graph`` performs **no** PII redaction (there is
no redaction seam in the extractor or engine write path), so the standing
"disable pii_redact for synthetic extraction" rule is satisfied by construction; the
MuSiQue paragraphs are public synthetic-eval data (CC-BY-4.0).

CLI:
    python -m eval.m1_graph_build --n 300 --seed 20260617 \
        --db-dir data/corpus-data/graph-cache/0.8.2-m1-v1 \
        --output dev/plans/runs/0.8.2-m1-graph-coverage-n300.json
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import statistics
import sys
import time
from collections import defaultdict
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Callable, Optional, Sequence

# ``eval.graph_arm_recall`` carries the reused extractor + the entity normalizer.
from eval.graph_arm_recall import _norm, extract_graph

#: extractor identity recorded in artifacts (the local, $0 seam).
EXTRACTOR_MODEL_ID = "qwen3.6-27b (local vLLM batch, enable_thinking=false)"

#: default corpus path (Slice-4 materialized corpus; gitignored).
DEFAULT_CORPUS = "data/corpus-data/raw/musique_dev.jsonl"

#: the pinned corpus hash this build is valid for (Slice-4).
MUSIQUE_HASH = "3cff37fd7221506a343a125cf7ca20aab7cd09877e376122da9627e1b935b26f"


# --------------------------------------------------------------------------- #
# Corpus load + deterministic sampling
# --------------------------------------------------------------------------- #
def load_questions(corpus_path: str | Path) -> list[dict]:
    """Read the materialized MuSiQue corpus (one JSON object per line)."""
    rows: list[dict] = []
    with Path(corpus_path).open(encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def sample_questions(
    rows: Sequence[dict],
    *,
    n: Optional[int],
    seed: int,
    answerable_only: bool = True,
    log: Callable[[str], Any] = print,
) -> list[dict]:
    """Deterministically sample ``n`` questions, **stratified by hop count** so all of
    {2,3,4} are represented in proportion. ``n=None`` ⇒ take the whole (filtered) set.

    Determinism: sort each hop stratum by question id, then take a seed-rotated,
    evenly-strided slice. No silent caps — the sample size + per-hop breakdown are
    ``log()``-ed (bias-control §7 of the design)."""
    pool = [r for r in rows if (r.get("answerable") if answerable_only else True)]
    pool_kind = "answerable" if answerable_only else "all"
    by_hop: dict[int, list[dict]] = defaultdict(list)
    for r in pool:
        by_hop[int(r["hop_count"])].append(r)
    for h in by_hop:
        by_hop[h].sort(key=lambda r: r["id"])

    if n is None or n >= len(pool):
        out = sorted(pool, key=lambda r: r["id"])
        log(f"[S10][SAMPLE] full {pool_kind} set: {len(out)} questions "
            f"(per-hop { {h: len(by_hop[h]) for h in sorted(by_hop)} })")
        return out

    total = len(pool)
    out: list[dict] = []
    per_hop_taken: dict[int, int] = {}
    for h in sorted(by_hop):
        stratum = by_hop[h]
        # proportional allocation, deterministic stride + seed rotation
        take = max(1, round(n * len(stratum) / total)) if stratum else 0
        take = min(take, len(stratum))
        if take == 0:
            per_hop_taken[h] = 0
            continue
        stride = len(stratum) / take
        off = seed % len(stratum)
        idxs = sorted({int((off + i * stride)) % len(stratum) for i in range(take)})
        # stride collisions can under-fill; top up deterministically from the front
        j = 0
        while len(idxs) < take and j < len(stratum):
            if j not in idxs:
                idxs.append(j)
            j += 1
        idxs = sorted(set(idxs))[:take]
        out.extend(stratum[i] for i in idxs)
        per_hop_taken[h] = len(idxs)
    out.sort(key=lambda r: r["id"])
    log(f"[S10][SAMPLE] sampled {len(out)}/{total} {pool_kind} questions "
        f"(n requested={n}, seed={seed}; per-hop taken={ {h: per_hop_taken[h] for h in sorted(per_hop_taken)} })")
    return out


# --------------------------------------------------------------------------- #
# Extraction (per-paragraph, incrementally cached) — reuses extract_graph ($0)
# --------------------------------------------------------------------------- #
def paragraph_documents(questions: Sequence[dict]) -> dict[str, str]:
    """Flatten sampled questions to ``{ "{qid}#{idx}": "title\\ntext" }`` paragraph
    documents — the unit the extractor sees (passage-level granularity)."""
    docs: dict[str, str] = {}
    for q in questions:
        qid = q["id"]
        for p in q["paragraphs"]:
            cid = f"{qid}#{p['idx']}"
            title = (p.get("title") or "").strip()
            text = (p.get("text") or "").strip()
            docs[cid] = (f"{title}\n{text}" if title else text).strip()
    return docs


def extract_question_graphs(
    questions: Sequence[dict],
    cache_path: str | Path,
    *,
    chunk_size: int = 1500,
    max_tokens: int = 3072,
    log: Callable[[str], Any] = print,
) -> dict[str, dict]:
    """Extract per-paragraph graphs for the sampled questions, **caching
    incrementally** (keyed by paragraph custom_id). Only MISSING paragraphs are
    extracted; the cache is persisted after every chunk so a long build is
    resumable. Returns ``{ "{qid}#{idx}": {"entities":[...], "relations":[...]} }``
    for the requested paragraphs."""
    cache_path = Path(cache_path)
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    cached: dict[str, dict] = (
        json.loads(cache_path.read_text(encoding="utf-8")) if cache_path.exists() else {}
    )
    wanted = paragraph_documents(questions)
    missing = {cid: body for cid, body in wanted.items() if cid not in cached}
    log(f"[S10][EXTRACT] {len(wanted)} paragraphs wanted; "
        f"{len(wanted) - len(missing)} cached, {len(missing)} to extract")
    if missing:
        items = list(missing.items())
        for i in range(0, len(items), chunk_size):
            chunk = dict(items[i : i + chunk_size])
            log(f"[S10][EXTRACT] chunk {i // chunk_size + 1} "
                f"({len(chunk)} paragraphs, {i + len(chunk)}/{len(items)})")
            new = extract_graph(chunk, max_tokens=max_tokens, log=log)
            cached.update(new)
            cache_path.write_text(json.dumps(cached), encoding="utf-8")
            log(f"[S10][EXTRACT] cache persisted -> {cache_path} ({len(cached)} paragraphs)")
    return {cid: cached[cid] for cid in wanted if cid in cached}


# --------------------------------------------------------------------------- #
# Graph build — per-question namespaced, BODY-LESS edges
# --------------------------------------------------------------------------- #
def _qid_of(cid: str) -> str:
    """``"{qid}#{idx}"`` -> ``qid`` (MuSiQue ids carry no ``#``)."""
    return cid.rsplit("#", 1)[0]


def _qnorm(qid: str, name: Any) -> Optional[str]:
    """Per-question-namespaced entity logical_id: ``"{qid}|ent:{normalized name}"``.
    Returns None for an unusable name."""
    base = _norm(name)  # -> "ent:<normalized>" | None
    return f"{qid}|{base}" if base else None


def build_question_graph_engine(
    questions: Sequence[dict],
    para_graphs: dict[str, dict],
    db_path: str | Path,
    *,
    log: Callable[[str], Any] = print,
):
    """Load passages (docs), per-question entities, and **body-less** fact-edges into
    a FathomDB engine. Returns ``(engine, stats)``.

    Schema written:
      - docs:     kind="doc",    body=passage text, source_id=qid, logical_id="{qid}#{idx}"
      - entities: kind="entity", body=name,         source_id=qid, logical_id="{qid}|ent:{norm}"
      - edges:    kind="rel", from/to=entity logical_ids, source_id=qid,
                  logical_id="{qid}|e:{n}", **NO body** (edge_fact-projection-safe)
    """
    from fathomdb.engine import Engine

    db_path = Path(db_path)
    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()
    for suffix in ("-wal", "-shm"):
        side = Path(str(db_path) + suffix)
        if side.exists():
            side.unlink()

    engine = Engine.open(str(db_path), use_default_embedder=False)

    # group extracted paragraph graphs by question (stable order)
    qids = [q["id"] for q in sorted(questions, key=lambda r: r["id"])]
    para_by_q: dict[str, list[tuple[str, dict]]] = defaultdict(list)
    for cid, g in para_graphs.items():
        para_by_q[_qid_of(cid)].append((cid, g))

    n_doc = n_ent = n_edge = 0

    for qid in qids:
        q = next(r for r in questions if r["id"] == qid)

        # 1) passages (docs)
        doc_writes = [
            {
                "kind": "doc",
                "body": (f"{(p.get('title') or '').strip()}\n{(p.get('text') or '').strip()}").strip(),
                "source_id": qid,
                "logical_id": f"{qid}#{p['idx']}",
            }
            for p in q["paragraphs"]
        ]
        for i in range(0, len(doc_writes), 64):
            engine.write(doc_writes[i : i + 64])
        n_doc += len(doc_writes)

        # 2) entities (per-question namespaced; first display name wins)
        ent_name: dict[str, str] = {}
        for _cid, g in sorted(para_by_q.get(qid, []), key=lambda t: t[0]):
            for e in g.get("entities", []) or []:
                raw = e.get("name") if isinstance(e, dict) else e
                lid = _qnorm(qid, raw)
                if lid and lid not in ent_name:
                    ent_name[lid] = (raw if isinstance(raw, str) and raw.strip() else lid)
        ent_writes = [
            {"kind": "entity", "body": ent_name[lid], "source_id": qid, "logical_id": lid}
            for lid in sorted(ent_name)
        ]
        for i in range(0, len(ent_writes), 64):
            engine.write(ent_writes[i : i + 64])
        n_ent += len(ent_writes)

        # 3) edges (BODY-LESS; both endpoints must be known entities of THIS question)
        edge_writes = []
        e_n = 0
        for _cid, g in sorted(para_by_q.get(qid, []), key=lambda t: t[0]):
            for r in g.get("relations", []) or []:
                if not isinstance(r, dict):
                    continue
                f = _qnorm(qid, r.get("subject"))
                t = _qnorm(qid, r.get("object"))
                if not f or not t or f == t or f not in ent_name or t not in ent_name:
                    continue
                e_n += 1
                edge_writes.append({"edge": {
                    "kind": "rel", "from": f, "to": t, "source_id": qid,
                    "logical_id": f"{qid}|e:{e_n}",
                    # NO "body" — keeps edge_fact vector projection from firing.
                }})
        for i in range(0, len(edge_writes), 64):
            engine.write(edge_writes[i : i + 64])
        n_edge += len(edge_writes)

    engine.drain(timeout_s=300)
    stats = {"docs": n_doc, "entities": n_ent, "edges": n_edge, "questions": len(qids)}
    log(f"[S10][BUILD] docs={n_doc} entities={n_ent} edges={n_edge} "
        f"over {len(qids)} questions -> {db_path}")
    return engine, stats


# --------------------------------------------------------------------------- #
# Coverage verification (read-only sqlite — drain/terminal status can lie)
# --------------------------------------------------------------------------- #
@dataclass
class CoverageReport:
    db: str
    n_questions: int
    n_questions_nonempty: int  # >=1 entity node
    coverage: float  # n_questions_nonempty / n_questions
    n_nodes_total: int
    n_entity_nodes: int
    n_edges_total: int
    n_edges_with_body: int  # MUST be 0 (body-less invariant)
    median_entities: float
    median_edges: float
    per_question: dict[str, dict[str, int]] = field(default_factory=dict)
    empty_questions: list[str] = field(default_factory=list)
    min_coverage: float = 1.0

    @property
    def ok(self) -> bool:
        return (
            self.n_questions > 0
            and self.coverage >= self.min_coverage
            and self.n_entity_nodes > 0
            and self.n_edges_total > 0
            and self.n_edges_with_body == 0
        )

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["ok"] = self.ok
        return d


def _scalar(con: sqlite3.Connection, sql: str, params: Sequence[Any] = ()) -> int:
    row = con.execute(sql, tuple(params)).fetchone()
    return int(row[0]) if row and row[0] is not None else 0


def verify_coverage(
    db_path: str | Path,
    question_ids: Sequence[str],
    *,
    min_coverage: float = 1.0,
) -> CoverageReport:
    """Inspect the built graph DB read-only (``mode=ro``, NOT ``immutable=1`` — must
    see committed-but-uncheckpointed WAL rows while the engine still holds the DB),
    and report per-question entity/edge coverage. A non-empty graph == ≥1 entity node
    for the question. Mirrors ``verify_embed_db`` (terminal/drain status can lie; only
    per-row attribution is ground truth)."""
    db_path = str(db_path)
    con = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        n_nodes_total = _scalar(
            con, "SELECT count(*) FROM canonical_nodes WHERE superseded_at IS NULL")
        n_entity_nodes = _scalar(
            con, "SELECT count(*) FROM canonical_nodes "
                 "WHERE kind='entity' AND superseded_at IS NULL")
        n_edges_total = _scalar(
            con, "SELECT count(*) FROM canonical_edges WHERE superseded_at IS NULL")
        n_edges_with_body = _scalar(
            con, "SELECT count(*) FROM canonical_edges "
                 "WHERE superseded_at IS NULL AND body IS NOT NULL")

        ent_by_q: dict[str, int] = {}
        for qid, c in con.execute(
            "SELECT source_id, count(*) FROM canonical_nodes "
            "WHERE kind='entity' AND superseded_at IS NULL GROUP BY source_id"
        ):
            if qid is not None:
                ent_by_q[str(qid)] = int(c)
        edge_by_q: dict[str, int] = {}
        for qid, c in con.execute(
            "SELECT source_id, count(*) FROM canonical_edges "
            "WHERE kind='rel' AND superseded_at IS NULL GROUP BY source_id"
        ):
            if qid is not None:
                edge_by_q[str(qid)] = int(c)
    finally:
        con.close()

    per_question: dict[str, dict[str, int]] = {}
    empty: list[str] = []
    ent_counts: list[int] = []
    edge_counts: list[int] = []
    for qid in question_ids:
        ne = ent_by_q.get(qid, 0)
        nr = edge_by_q.get(qid, 0)
        per_question[qid] = {"entities": ne, "edges": nr}
        ent_counts.append(ne)
        edge_counts.append(nr)
        if ne == 0:
            empty.append(qid)

    n_q = len(question_ids)
    n_nonempty = n_q - len(empty)
    return CoverageReport(
        db=db_path,
        n_questions=n_q,
        n_questions_nonempty=n_nonempty,
        coverage=round(n_nonempty / n_q, 6) if n_q else 0.0,
        n_nodes_total=n_nodes_total,
        n_entity_nodes=n_entity_nodes,
        n_edges_total=n_edges_total,
        n_edges_with_body=n_edges_with_body,
        median_entities=float(statistics.median(ent_counts)) if ent_counts else 0.0,
        median_edges=float(statistics.median(edge_counts)) if edge_counts else 0.0,
        per_question=per_question,
        empty_questions=empty,
        min_coverage=min_coverage,
    )


class CoverageIncompleteError(RuntimeError):
    def __init__(self, message: str, report: CoverageReport) -> None:
        super().__init__(message)
        self.report = report


def assert_coverage(
    db_path: str | Path,
    question_ids: Sequence[str],
    *,
    min_coverage: float = 1.0,
) -> CoverageReport:
    """Raise :class:`CoverageIncompleteError` unless every sampled question has a
    non-empty graph and the node/edge tables are populated (body-less)."""
    report = verify_coverage(db_path, question_ids, min_coverage=min_coverage)
    if not report.ok:
        reasons = []
        if report.coverage < report.min_coverage:
            reasons.append(
                f"coverage {report.coverage:.4f} < {report.min_coverage} "
                f"({len(report.empty_questions)} empty: {report.empty_questions[:5]})")
        if report.n_entity_nodes == 0:
            reasons.append("no entity nodes")
        if report.n_edges_total == 0:
            reasons.append("no edges")
        if report.n_edges_with_body != 0:
            reasons.append(f"{report.n_edges_with_body} edges carry a body (body-less invariant violated)")
        raise CoverageIncompleteError("graph coverage incomplete: " + "; ".join(reasons), report)
    return report


# --------------------------------------------------------------------------- #
# Main
# --------------------------------------------------------------------------- #
def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(
        description="M1 Slice 10 — per-question MuSiQue graph build (Qwen extractor, body-less edges)")
    ap.add_argument("--corpus", default=DEFAULT_CORPUS)
    ap.add_argument("--n", type=int, default=300,
                    help="number of questions to sample (default 300; 0 ⇒ full set)")
    ap.add_argument("--seed", type=int, default=20260617)
    ap.add_argument("--answerable-only", dest="answerable_only", action="store_true", default=True)
    ap.add_argument("--include-unanswerable", dest="answerable_only", action="store_false")
    ap.add_argument("--db-dir", default="data/corpus-data/graph-cache/0.8.2-m1-v1")
    ap.add_argument("--output", required=True)
    ap.add_argument("--chunk-size", type=int, default=1500)
    ap.add_argument("--max-tokens", type=int, default=3072)
    args = ap.parse_args(argv)

    t0 = time.time()
    rows = load_questions(args.corpus)
    print(f"[S10][LOAD] {len(rows)} questions from {args.corpus}")

    questions = sample_questions(
        rows, n=(None if args.n == 0 else args.n), seed=args.seed,
        answerable_only=args.answerable_only)
    qids = [q["id"] for q in questions]

    db_dir = Path(args.db_dir)
    db_dir.mkdir(parents=True, exist_ok=True)
    cache_path = db_dir / "extractions.json"

    para_graphs = extract_question_graphs(
        questions, cache_path, chunk_size=args.chunk_size, max_tokens=args.max_tokens)

    db_path = db_dir / "graph.sqlite"
    engine, build_stats = build_question_graph_engine(questions, para_graphs, db_path)

    report = verify_coverage(db_path, qids, min_coverage=1.0)
    engine.close()

    artifact = {
        "schema": "0.8.2-m1-graph-coverage-v1",
        "phase": "0.8.2-slice-10",
        "generated_by": "eval.m1_graph_build",
        "musique_hash": MUSIQUE_HASH,
        "extractor_model_id": EXTRACTOR_MODEL_ID,
        "priced_api_calls": 0,
        "edge_body_policy": "body-less (edge_fact vector-projection-safe; see module docstring)",
        "sample": {
            "n_requested": args.n,
            "seed": args.seed,
            "answerable_only": args.answerable_only,
            "n_questions": len(qids),
        },
        "build_stats": build_stats,
        "coverage": report.to_dict(),
        "wall_seconds": round(time.time() - t0, 1),
    }
    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(artifact, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")

    verdict = "OK" if report.ok else "INCOMPLETE"
    print(f"[S10][VERIFY] [{verdict}] coverage={report.coverage:.4f} "
          f"({report.n_questions_nonempty}/{report.n_questions} non-empty) "
          f"entity_nodes={report.n_entity_nodes} edges={report.n_edges_total} "
          f"edges_with_body={report.n_edges_with_body} "
          f"median_entities={report.median_entities} median_edges={report.median_edges}")
    print(f"[S10][VERIFY] wrote {out_path}")
    if report.empty_questions:
        print(f"[S10][VERIFY] WARNING {len(report.empty_questions)} empty-graph questions: "
              f"{report.empty_questions[:10]}")
    return 0 if report.ok else 1


if __name__ == "__main__":
    sys.exit(main())
