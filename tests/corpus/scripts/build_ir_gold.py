#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""IR-C reuse tier: resolve existing eval QA into IR-B GoldSet files.

Transforms the human/dataset-authored QA under
`data/corpus-data/eval/<source>_qa.jsonl` into the corpus-pinned
`GoldSet` JSON schema consumed by the IR-B harness
(`tests/support/ir_eval.rs::parse_gold_set` / `validate_gold_set`).

This is the REUSE tier of the IR-C plan
(`dev/notes/IR-C-fact-level-gold-labels-research.md`): the ~4.5k QA pairs
whose `evidence_doc_ids` already resolve cleanly to the frozen 0.8.x-B
snapshot become zero-hallucination gold labels with no generation. The
GENERATE / REPAIR tiers (gap sources, qasper parser) are separate.

Outputs (gitignored — derived from cache-only/licensed sources, exactly
like the raw corpus; only this script is committed and reproducible):

  data/corpus-data/eval/ir_gold/<source>.gold.json   (one per source)
  data/corpus-data/eval/ir_gold/all.gold.json        (combined)

Class mapping (answer_type -> query_class, ir_eval.rs QueryClass):
  span | free_form -> exact_fact   (factoid: a specific fact is required)
  summary          -> exploratory  (open-ended: a discussion is required)
  abstain          -> negative     (not-found: MUST have empty denominator)

Determinism: queries are emitted sorted by query_id, so a re-run against
the same frozen corpus + eval QA reproduces a bit-identical file.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
SNAPSHOT = REPO / "tests/corpus/snapshot.json"
RAW_DIR = REPO / "data/corpus-data/raw"
EVAL_DIR = REPO / "data/corpus-data/eval"
OUT_DIR = EVAL_DIR / "ir_gold"

QRELS_VERSION = "ir-c-reused-v2"  # v2: query tracers + evidence-span locators
SOURCES = ("enronqa", "qaconv", "qmsum")

# answer_type -> query_class (must be one of ir_eval.rs QueryClass labels).
CLASS_FOR_ANSWER_TYPE = {
    "span": "exact_fact",
    "free_form": "exact_fact",
    "summary": "exploratory",
    "abstain": "negative",
}


def load_corpus_doc_ids() -> set[str]:
    ids: set[str] = set()
    for p in sorted(RAW_DIR.glob("*.jsonl")):
        with p.open() as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    ids.add(json.loads(line)["doc_id"])
                except (json.JSONDecodeError, KeyError):
                    pass
    return ids


def transform_row(source: str, row: dict, corpus_ids: set[str]) -> tuple[dict | None, str]:
    """Return (gold_query, reason_dropped). On success reason is ''."""
    qa_id = row.get("qa_id")
    question = (row.get("question") or "").strip()
    if not qa_id:
        return None, "missing qa_id"
    if not question:
        return None, "empty question"
    answer_type = row.get("answer_type")
    query_class = CLASS_FOR_ANSWER_TYPE.get(answer_type)
    if query_class is None:
        return None, f"unmapped answer_type={answer_type!r}"

    query_id = f"{source}:{qa_id}"

    if query_class == "negative":
        # Abstention class: denominator MUST be empty (validator §(d)). The
        # row's evidence (the doc that was checked and found lacking) is
        # deliberately NOT carried as required evidence.
        return {
            "query_id": query_id,
            "query": question,
            "query_class": "negative",
            "required_evidence": [],
            "expected_top_k_doc_ids": [],
            "relation_type": row.get("relation_type"),
            "source": source,
            "answer_type": answer_type,
            "query_origin": "human_dataset",  # reuse tier: dataset-authored question
        }, ""

    # Positive class: required evidence = resolving evidence_doc_ids.
    raw_ev = row.get("evidence_doc_ids") or []
    spans = row.get("evidence_spans") or []
    resolving = [d for d in raw_ev if d in corpus_ids]
    if not resolving:
        return None, "no evidence_doc_ids resolve to the frozen corpus"

    # WI-3a: carry the dataset evidence spans into the locator, per doc, so the
    # span-level diagnostic (passage<->evidence overlap) can be computed later.
    # Span offsets are emitted as-authored (relative to the source doc text); the
    # diagnostic that consumes them is responsible for body alignment.
    def spans_for(doc: str) -> list[dict]:
        return [
            {"doc_id": s["doc_id"], "start": s["start"], "end": s["end"]}
            for s in spans
            if s.get("doc_id") == doc
        ]

    required_evidence = []
    for i, d in enumerate(resolving):
        doc_spans = spans_for(d)
        locator = {"kind": "span" if doc_spans else "whole_body"}
        if doc_spans:
            locator["spans"] = doc_spans
        required_evidence.append(
            {
                "evidence_id": f"{query_id}#e{i}",
                "doc_id": d,
                "necessity": "required",
                "locator": locator,
            }
        )
    return {
        "query_id": query_id,
        "query": question,
        "query_class": query_class,
        "required_evidence": required_evidence,
        # Preserved legacy/fallback view; harmless when required_evidence is
        # present (ir_eval.rs §(f): never added on top of an evidence set).
        "expected_top_k_doc_ids": resolving,
        "relation_type": row.get("relation_type"),
        "source": source,
        "answer_type": answer_type,
        "query_origin": "human_dataset",  # reuse tier: dataset-authored question
    }, ""


def build_source(source: str, corpus_ids: set[str]) -> tuple[list[dict], dict]:
    path = EVAL_DIR / f"{source}_qa.jsonl"
    if not path.exists():
        print(f"WARN: {path} missing — skipping {source}", file=sys.stderr)
        return [], {"rows": 0, "kept": 0, "dropped": {}}
    queries: list[dict] = []
    seen_qids: set[str] = set()
    rows = 0
    dropped: dict[str, int] = {}
    by_class: dict[str, int] = {}
    for line in path.open():
        line = line.strip()
        if not line:
            continue
        rows += 1
        row = json.loads(line)
        gq, reason = transform_row(source, row, corpus_ids)
        if gq is None:
            dropped[reason] = dropped.get(reason, 0) + 1
            continue
        if gq["query_id"] in seen_qids:
            dropped["duplicate query_id"] = dropped.get("duplicate query_id", 0) + 1
            continue
        seen_qids.add(gq["query_id"])
        by_class[gq["query_class"]] = by_class.get(gq["query_class"], 0) + 1
        queries.append(gq)
    queries.sort(key=lambda q: q["query_id"])
    stats = {"rows": rows, "kept": len(queries), "dropped": dropped, "by_class": by_class}
    return queries, stats


def gold_set(corpus_hash: str, source: str, queries: list[dict]) -> dict:
    return {
        "corpus_hash": corpus_hash,
        "qrels_version": QRELS_VERSION,
        "note": (
            f"IR-C reuse tier — resolved from data/corpus-data/eval/{source}_qa.jsonl "
            f"via tests/corpus/scripts/build_ir_gold.py. Zero-generation: every "
            f"required doc_id is a dataset-authored evidence pointer that resolves "
            f"to the frozen snapshot."
        ),
        "queries": queries,
    }


def validate_local(gs: dict) -> list[str]:
    """Mirror of ir_eval.rs::validate_gold_set — fail the build if any fire."""
    issues: list[str] = []
    if not gs["corpus_hash"].strip():
        issues.append("corpus_hash missing")
    if not gs["qrels_version"].strip():
        issues.append("qrels_version missing")
    seen: set[str] = set()
    for q in gs["queries"]:
        qid = q.get("query_id") or "<no id>"
        if not q["query"].strip():
            issues.append(f"{qid}: empty query")
        if qid in seen:
            issues.append(f"{qid}: duplicate query_id")
        seen.add(qid)
        ev_ids: set[str] = set()
        for e in q["required_evidence"]:
            if not e["doc_id"].strip():
                issues.append(f"{qid}: empty doc_id")
            if e["evidence_id"] in ev_ids:
                issues.append(f"{qid}: duplicate evidence_id {e['evidence_id']}")
            ev_ids.add(e["evidence_id"])
        req = {e["doc_id"] for e in q["required_evidence"] if e["necessity"] == "required"}
        if q["query_class"] == "negative":
            if req:
                issues.append(f"{qid}: negative class must have EMPTY denominator")
        elif not req:
            issues.append(f"{qid}: non-negative class has EMPTY denominator")
    return issues


def main() -> int:
    if not SNAPSHOT.exists():
        print(f"ERROR: {SNAPSHOT} missing — freeze the corpus first", file=sys.stderr)
        return 2
    corpus_hash = json.loads(SNAPSHOT.read_text())["corpus_hash"]
    print(f"frozen corpus_hash = {corpus_hash}")
    corpus_ids = load_corpus_doc_ids()
    print(f"corpus doc_ids     = {len(corpus_ids)}")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    all_queries: list[dict] = []
    any_issue = False
    for source in SOURCES:
        queries, stats = build_source(source, corpus_ids)
        gs = gold_set(corpus_hash, source, queries)
        issues = validate_local(gs)
        status = "OK" if not issues else f"INVALID ({len(issues)} issues)"
        print(
            f"\n{source}: rows={stats['rows']} kept={stats['kept']} "
            f"by_class={stats.get('by_class', {})} dropped={stats['dropped']} -> {status}"
        )
        if issues:
            any_issue = True
            for it in issues[:10]:
                print(f"   ! {it}", file=sys.stderr)
        out = OUT_DIR / f"{source}.gold.json"
        out.write_text(json.dumps(gs, indent=2, sort_keys=True) + "\n")
        print(f"   wrote {out.relative_to(REPO)}")
        all_queries.extend(queries)

    all_queries.sort(key=lambda q: q["query_id"])
    combined = gold_set(corpus_hash, "all-sources", all_queries)
    combined["note"] = (
        "IR-C reuse tier — combined enronqa+qaconv+qmsum, resolved via "
        "tests/corpus/scripts/build_ir_gold.py against the frozen snapshot."
    )
    issues = validate_local(combined)
    if issues:
        any_issue = True
        for it in issues[:10]:
            print(f"   ! combined: {it}", file=sys.stderr)
    out = OUT_DIR / "all.gold.json"
    out.write_text(json.dumps(combined, indent=2, sort_keys=True) + "\n")
    print(f"\ncombined: {len(all_queries)} queries -> {out.relative_to(REPO)}")

    if any_issue:
        print("\nBUILD FAILED: validator issues above", file=sys.stderr)
        return 1
    print("\nall gold sets valid (mirror of ir_eval.rs::validate_gold_set).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
