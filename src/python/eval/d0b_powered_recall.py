"""0.8.3 D0b Phase-B §SECONDARY — **powered** LME+LOCOMO strict Recall@K ($0).

The PRIMARY priced pass (``d0b_parity_run``) measures the identical-answerer
accuracy gap on the LongMemEval-only corpus. This module adds the **LLM-free,
$0** half of the two-level claim: per-class strict full-gold ``Recall@K`` on the
**powered** corpus — LongMemEval **+ LOCOMO** (the second pinned real
agentic-memory source, restoring power on ``multi_session`` / ``temporal``; see
``dev/plans/runs/0.8.3-corpus-adequacy-note.md``).

It does **not** touch the just-reviewed :mod:`eval.d0b_parity_run`; it *reuses*
that module's :func:`~eval.d0b_parity_run.per_class_delta_table` (paired bootstrap
CI + MDE) for the delta math, so the statistic cannot drift between the priced and
the free halves.

Two design points the corpus-adequacy note + the Slice-5 codex review make
load-bearing:

* **LOCOMO is CC-BY-NC, EVAL-ONLY, never committed** — the raw payload + any
  LOCOMO-derived gold stay gitignored under ``data/corpus-data/`` and are acquired
  on demand (``tests/corpus/scripts/acquire_locomo.py``). This module reads them at
  runtime; it persists only counts/deltas (no verbatim NC text).
* **The ≥2-session predicate (codex Slice-5 [P1#1]).** LOCOMO category-1
  (multi-hop) does NOT guarantee evidence spanning ≥2 distinct sessions, so before
  a question is counted toward ``multi_session`` it must clear
  :func:`filter_min_sessions` (drop cat-1 questions whose evidence resolves to <2
  distinct sessions); the dropped count is reported.

Pure helpers (:func:`strict_recall_at_k` / :func:`distinct_sessions` /
:func:`filter_min_sessions` / :func:`locomo_items` / :func:`recall_records` /
:func:`recall_summary`) are import-light + backend-free so the unit tests run with
fake adapters (no DB, no LLM, no ``mem0``, no ``fathomdb`` extension build). The
live corpus/adapter stand-up lives in :func:`main` (CLI-only).
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Mapping, Optional, Sequence

from eval.d0b_parity_run import TREATMENT_ARM, per_class_delta_table
from eval.decision_rule_083 import MEMORY_CLASSES

# --------------------------------------------------------------------------- #
# The recall unit + pure helpers (the TDD core — backend-free).
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class RecallItem:
    """One strict-recall query, normalized across LongMemEval + LOCOMO.

    ``gold_doc_ids`` are *session*-level corpus doc ids (the strict-recall join
    key); ``source`` is ``"lme"`` or ``"locomo"`` (the LME-only-vs-combined slice
    key); ``reporting_class`` is one of :data:`MEMORY_CLASSES`."""

    query_id: str
    reporting_class: str
    gold_doc_ids: tuple[str, ...]
    source: str
    question: str


def strict_recall_at_k(
    retrieved_ids: Sequence[str], gold_ids: Sequence[str], k: int
) -> float:
    """Strict full-gold ``Recall@K``: ``1.0`` iff EVERY ``gold_id`` is in the top-K
    retrieved ids, else ``0.0`` (the same all-or-nothing rule as the R2 harness).

    Empty gold never scores ``1.0`` (a question with no resolvable evidence cannot
    fabricate a hit)."""
    if not gold_ids:
        return 0.0
    top = set(retrieved_ids[:k])
    return 1.0 if all(g in top for g in gold_ids) else 0.0


def distinct_sessions(gold_doc_ids: Iterable[str]) -> int:
    """Number of DISTINCT session doc ids in a gold-evidence set (the ≥2-session
    predicate's measured quantity)."""
    return len(set(gold_doc_ids))


def filter_min_sessions(
    items: Sequence[RecallItem],
    *,
    min_sessions: int = 2,
    classes: Sequence[str] = ("multi_session",),
) -> tuple[list[RecallItem], int]:
    """Apply the ≥``min_sessions`` predicate (codex Slice-5 [P1#1]) to the named
    ``classes`` only: drop an item in one of those classes whose evidence spans
    fewer than ``min_sessions`` distinct sessions. Items in any OTHER class pass
    through untouched. Returns ``(kept, n_dropped)``."""
    target = set(classes)
    kept: list[RecallItem] = []
    dropped = 0
    for it in items:
        if it.reporting_class in target and distinct_sessions(it.gold_doc_ids) < min_sessions:
            dropped += 1
            continue
        kept.append(it)
    return kept, dropped


def locomo_items(gold_queries: Sequence[Mapping[str, Any]]) -> list[RecallItem]:
    """Convert :func:`eval.locomo_loader.load_locomo` gold dicts → :class:`RecallItem`
    (``source="locomo"``). Evidence doc ids come from ``required_evidence``."""
    out: list[RecallItem] = []
    for q in gold_queries:
        ev = tuple(str(e["doc_id"]) for e in (q.get("required_evidence") or []) if e.get("doc_id"))
        out.append(
            RecallItem(
                query_id=str(q["query_id"]),
                reporting_class=str(q["query_class"]),
                gold_doc_ids=ev,
                source="locomo",
                question=str(q.get("query", "")),
            )
        )
    return out


def lme_items(queries: Sequence[Any]) -> list[RecallItem]:
    """Convert :class:`eval.r2_parity_eval.GoldQuery` objects (the re-pinned LME
    gold) → :class:`RecallItem` (``source="lme"``)."""
    out: list[RecallItem] = []
    for q in queries:
        out.append(
            RecallItem(
                query_id=str(q.query_id),
                reporting_class=str(q.reporting_class),
                gold_doc_ids=tuple(str(g) for g in q.gold_doc_ids),
                source="lme",
                question=str(q.question),
            )
        )
    return out


def recall_records(
    items: Sequence[RecallItem],
    adapters: Mapping[str, Any],
    *,
    k: int = 10,
) -> list[dict[str, Any]]:
    """Per-item strict ``Recall@K`` for every arm → records consumable by
    :func:`eval.d0b_parity_run.per_class_delta_table` (keys ``reporting_class`` +
    ``recall``). Each adapter exposes ``retrieve(question, k) -> [Hit]``; retrieval
    is $0 (no answerer). The arm's ranked doc-ids are de-duplicated in rank order
    before the top-K cut."""
    records: list[dict[str, Any]] = []
    for it in items:
        rec: dict[str, Any] = {
            "qid": it.query_id,
            "reporting_class": it.reporting_class,
            "source": it.source,
            "gold": list(it.gold_doc_ids),
            "recall": {},
        }
        for arm, adapter in adapters.items():
            hits = adapter.retrieve(it.question, k)
            seen: list[str] = []
            for h in hits:
                if h.doc_id not in seen:
                    seen.append(h.doc_id)
            rec["recall"][arm] = strict_recall_at_k(seen, it.gold_doc_ids, k)
        records.append(rec)
    return records


def _per_arm_recall(
    records: Sequence[Mapping[str, Any]],
    *,
    arms: Sequence[str],
    classes: Sequence[str],
) -> dict[str, dict[str, dict[str, Any]]]:
    """``{arm: {class: {mean, n}}}`` raw per-arm strict-recall means (alongside the
    paired deltas — the absolute levels behind a delta)."""
    out: dict[str, dict[str, dict[str, Any]]] = {}
    for arm in arms:
        out[arm] = {}
        for cls in classes:
            vals = [
                float(r["recall"][arm])
                for r in records
                if r.get("reporting_class") == cls and arm in (r.get("recall") or {})
            ]
            n = len(vals)
            out[arm][cls] = {"mean": round(sum(vals) / n, 6) if n else None, "n": n}
    return out


def recall_summary(
    records: Sequence[Mapping[str, Any]],
    *,
    source_filter: Optional[str] = None,
    classes: Sequence[str] = MEMORY_CLASSES,
    treatment: str = TREATMENT_ARM,
    comparators: Sequence[str] = ("mem0_oss", "naive_rag"),
    n_boot: int = 2000,
    seed: int = 0,
) -> dict[str, Any]:
    """Per-class strict-recall summary for a corpus slice.

    ``source_filter`` (``"lme"`` / ``"locomo"`` / ``None``) restricts the records to
    one source before aggregating — so the SAME combined-corpus run yields both the
    LME-only view (``"lme"``) and the LME+LOCOMO view (``None``). Returns the raw
    per-arm per-class recall means + N, plus the paired ``treatment − comparator``
    recall deltas (reusing :func:`eval.d0b_parity_run.per_class_delta_table`: point
    + bootstrap CI + MDE + n)."""
    arms = [treatment, *comparators]
    if source_filter is not None:
        records = [r for r in records if r.get("source") == source_filter]
    per_class_n = {
        cls: sum(1 for r in records if r.get("reporting_class") == cls) for cls in classes
    }
    deltas = per_class_delta_table(
        records,
        metric="recall",
        comparators=comparators,
        classes=classes,
        treatment=treatment,
        n_boot=n_boot,
        seed=seed,
    )
    return {
        "source_filter": source_filter or "lme+locomo",
        "n_items": len(records),
        "per_class_n": per_class_n,
        "per_arm_recall": _per_arm_recall(records, arms=arms, classes=classes),
        "recall_deltas": deltas,
    }


# --------------------------------------------------------------------------- #
# Live corpus / adapter stand-up (CLI only — not exercised by the unit tests).
# --------------------------------------------------------------------------- #


def _bounded_build_mem0(documents: dict[str, str]) -> tuple[Optional[Any], Optional[dict[str, str]]]:
    """Best-effort Mem0-OSS stand-up over ``documents`` (isolated env). Returns
    ``(adapter, blocker)``; a failure is a CLEAN recorded blocker, never a crash."""
    try:
        from eval.r2_parity_eval import Mem0OSSAdapter

        mem0 = Mem0OSSAdapter.try_build()
        if mem0 is None or not mem0.available:
            return None, {"id": "mem0-oss-unavailable", "description": "mem0ai/backend not importable in this env"}
        mem0.ingest(documents)
        return mem0, None
    except Exception as exc:  # noqa: BLE001 — record, don't crash
        return None, {"id": "mem0-ingest-failed", "description": f"mem0 LOCOMO ingest failed: {exc}"}


def _bounded_build_fathomdb(
    documents: dict[str, str], db_path: str
) -> tuple[Optional[Any], Optional[dict[str, str]]]:
    """Best-effort FathomDB FTS ingest over the combined corpus. Recorded blocker on
    failure (e.g. the native extension missing in this env)."""
    try:
        from eval.r2_parity_eval import _build_fathomdb

        fdb, blk = _build_fathomdb(documents, Path(db_path))
        return fdb, blk
    except Exception as exc:  # noqa: BLE001
        return None, {"id": "fathomdb-ingest-failed", "description": f"fathomdb LOCOMO ingest failed: {exc}"}


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    ap = argparse.ArgumentParser(description="0.8.3 D0b powered LME+LOCOMO strict Recall@K ($0)")
    ap.add_argument("--gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    ap.add_argument("--locomo", default="data/corpus-data/raw/locomo10.json")
    ap.add_argument("--output", required=True)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--with-fathomdb", action="store_true", help="also ingest LOCOMO into FathomDB FTS")
    ap.add_argument("--with-mem0", action="store_true", help="also ingest LOCOMO into Mem0-OSS")
    ap.add_argument("--fathomdb-db", default="/tmp/d0b-powered-fathomdb.sqlite")
    args = ap.parse_args(argv)

    from eval.gold_repin import load_lme
    from eval.locomo_loader import load_locomo
    from eval.r2_parity_eval import NaiveRAGAdapter, load_repin_gold, session_id_of

    # --- gold ----------------------------------------------------------------
    _ch, _qv, lme_queries = load_repin_gold(Path(args.gold))
    lme_pool = lme_items(lme_queries)
    loco_docs, loco_gold = load_locomo(args.locomo)
    loco_pool_raw = locomo_items(loco_gold)
    loco_pool, n_dropped = filter_min_sessions(loco_pool_raw, min_sessions=2, classes=("multi_session",))
    print(
        f"[powered] LME items={len(lme_pool)} | LOCOMO items={len(loco_pool_raw)} "
        f"(>=2-session filter dropped {n_dropped} multi_session) -> {len(loco_pool)}",
        flush=True,
    )

    # --- combined corpus -----------------------------------------------------
    lme_docs, _gq, _cs = load_lme("xiaowu0162/longmemeval-cleaned", "oracle")
    # restrict LME haystack to the gold-relevant sessions (footprint), then union LOCOMO.
    lme_gold_sids = {session_id_of(g) for it in lme_pool for g in it.gold_doc_ids}
    documents: dict[str, str] = {sid: lme_docs[sid] for sid in lme_gold_sids if sid in lme_docs}
    documents.update(loco_docs)  # LOCOMO session bodies (own sample_id namespace)
    print(f"[powered] combined corpus sessions: LME(gold)={len(lme_gold_sids)} + LOCOMO={len(loco_docs)} = {len(documents)}", flush=True)

    items = [*lme_pool, *loco_pool]

    # --- arms ----------------------------------------------------------------
    adapters: dict[str, Any] = {"naive_rag": NaiveRAGAdapter(documents)}
    blockers: list[dict[str, str]] = []
    if args.with_fathomdb:
        fdb, blk = _bounded_build_fathomdb(documents, args.fathomdb_db)
        if fdb is not None:
            adapters["fathomdb"] = fdb
        if blk is not None:
            blockers.append(blk)
    if args.with_mem0:
        mem0, blk = _bounded_build_mem0(documents)
        if mem0 is not None:
            adapters["mem0_oss"] = mem0
        if blk is not None:
            blockers.append(blk)
    print(f"[powered] arms: {sorted(adapters)} | blockers: {[b['id'] for b in blockers]}", flush=True)

    comparators = tuple(a for a in ("mem0_oss", "naive_rag") if a in adapters)
    records = recall_records(items, adapters, k=args.k)

    art: dict[str, Any] = {
        "schema": "0.8.3-d0b-powered-recall-v1",
        "k": args.k,
        "arms_run": sorted(adapters),
        "treatment_arm": TREATMENT_ARM if TREATMENT_ARM in adapters else None,
        "comparator_arms": list(comparators),
        "locomo_2session_dropped": n_dropped,
        "n_lme_items": len(lme_pool),
        "n_locomo_items": len(loco_pool),
        "blockers": blockers,
        "lme_only": recall_summary(
            records, source_filter="lme", treatment=TREATMENT_ARM, comparators=comparators
        ),
        "lme_plus_locomo": recall_summary(
            records, source_filter=None, treatment=TREATMENT_ARM, comparators=comparators
        ),
    }
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(f"[powered] wrote {args.output} | {len(records)} items, arms={sorted(adapters)}", flush=True)
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
