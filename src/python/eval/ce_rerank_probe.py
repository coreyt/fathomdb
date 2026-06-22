"""0.8.3 CE-rerank precision probe ($0 / CPU / LLM-free) — the realizable Slice-20
lever test (design ``dev/design/0.8.3-ce-rerank-precision-probe.md``).

The gap-decomposition verdict named **retrieval precision** as the dominant lever
(perfect raw-gold retrieval recovers ~+0.39 — an UPPER bound). The realizable
precision lever FathomDB already ships is the **CE reranker** (``fathomdb.rerank``,
``cross-encoder/ms-marco-TinyBERT-L2-v2``). This probe measures how much of that
headroom a REAL CE rerank captures **before** committing the Slice-20 engine build
(cheap-validate-before-engine).

It **reuses** the LME+LOCOMO powered-recall harness (:mod:`eval.d0b_powered_recall`)
and its paired bootstrap CI (:func:`eval.d0b_parity_run.per_class_delta_table` /
:func:`~eval.d0b_parity_run.class_delta`) so the statistic cannot drift from the
reviewed harness. It adds exactly one arm:

* **fathomdb_rerank** — take FathomDB's fused top-N pool (:data:`POOL_N`, pinned),
  ``fathomdb.rerank(query, pool, rerank_depth=N)``, return the reranked order; the
  harness then dedupes + cuts to top-K. ``rerank_depth == 0`` ⇒ identity (no model
  load) ⇒ recall byte-identical to the ``fathomdb`` baseline.

The frozen pass criterion + the diagnostic headroom-captured live in the pure
:mod:`eval.ce_rerank_rule` (stdlib-only). The arms are ``fathomdb`` (baseline) ·
``fathomdb_rerank`` · ``naive_rag`` (floor). The gate is the paired
``(fathomdb_rerank − fathomdb)`` Recall@10 margin (per-class + pooled).

The pure helpers (:class:`FathomDBRerankAdapter`, :func:`rerank_margin_summary`,
:func:`pooled_margin`, :func:`load_oracle_gaps`, :func:`build_probe_artifact`) are
backend-free given an injected ``rerank_fn`` — the unit tests run with a fake
reranker (no model load) and fake adapters. The live corpus / real ``fathomdb.rerank``
stand-up lives in :func:`main` (CLI only).
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Mapping, Optional, Sequence

from eval.ce_rerank_rule import headroom_captured, probe_rerank_pass
from eval.d0b_parity_run import class_delta, paired_metric_deltas, per_class_delta_table
from eval.d0b_powered_recall import recall_records, recall_summary
from eval.decision_rule_083 import MEMORY_CLASSES
from eval.r2_parity_eval import Hit

# --------------------------------------------------------------------------- #
# Frozen probe parameters (pinned in the output for auditability; design §2).
# --------------------------------------------------------------------------- #

#: The fused top-N pool the CE reranks (N ≫ K). Pinned; recorded in the artifact.
POOL_N: int = 50

#: The treatment arm (CE rerank over the fused pool) and the baseline it is paired
#: against. The gated margin is ``(fathomdb_rerank − fathomdb)`` Recall@10.
RERANK_ARM: str = "fathomdb_rerank"
BASELINE_ARM: str = "fathomdb"
FLOOR_ARM: str = "naive_rag"

#: CE model identity (``src/rust/crates/fathomdb-embedder/src/candle_reranker.rs``).
CE_MODEL_REPO: str = "cross-encoder/ms-marco-TinyBERT-L2-v2"
CE_MODEL_NAME: str = "fathomdb-ms-marco-TinyBERT-L2-v2"

#: Type of a rerank callable: ``(query, passages, rerank_depth) -> [{"id","score"}]``
#: where ``passages`` is ``[{"id":int,"body":str,"score":float}]`` (``fathomdb.rerank``).
RerankFn = Callable[[str, list[dict[str, Any]], int], list[dict[str, Any]]]


# --------------------------------------------------------------------------- #
# The CE-rerank arm (backend-free given an injected ``rerank_fn``).
# --------------------------------------------------------------------------- #


@dataclass
class FathomDBRerankAdapter:
    """Wrap a base FathomDB retrieval adapter with a CE rerank over its top-N pool.

    ``retrieve(question, k)`` fetches the pinned :attr:`pool_n` fused pool from
    :attr:`base` (independent of the harness's pool factor so the rerank set is
    fixed), marshals it into ``[{"id":idx,"body":..,"score":..}]``, calls
    :attr:`rerank_fn` with ``rerank_depth =`` :attr:`rerank_depth`, then maps the
    reranked ids back to the original :class:`~eval.r2_parity_eval.Hit` objects
    (preserving ``doc_id``; the score becomes the CE-blended score). The harness's
    :func:`eval.d0b_powered_recall.recall_records` dedupes + cuts to top-K.

    ``rerank_depth == 0`` ⇒ the rerank identity contract ⇒ the returned order is the
    base pool order ⇒ recall byte-identical to the base arm.
    """

    base: Any
    rerank_fn: RerankFn
    pool_n: int = POOL_N
    rerank_depth: int = field(default=POOL_N)

    def retrieve(self, question: str, k: int) -> list[Hit]:
        # Fetch at least k, but pin the rerank pool to pool_n (the frozen N).
        n = max(k, self.pool_n)
        pool = self.base.retrieve(question, n)
        passages: list[dict[str, Any]] = [
            {"id": i, "body": h.body, "score": float(h.score)} for i, h in enumerate(pool)
        ]
        reranked = self.rerank_fn(question, passages, self.rerank_depth)
        out: list[Hit] = []
        for r in reranked:
            idx = int(r["id"])
            h = pool[idx]
            out.append(Hit(doc_id=h.doc_id, body=h.body, score=float(r["score"])))
        return out


# --------------------------------------------------------------------------- #
# Margin statistics (reuse the reviewed paired-bootstrap CI; no drift).
# --------------------------------------------------------------------------- #


def pooled_margin(
    records: Sequence[Mapping[str, Any]],
    *,
    treatment: str = RERANK_ARM,
    comparator: str = BASELINE_ARM,
    classes: Sequence[str] = MEMORY_CLASSES,
    n_boot: int = 2000,
    seed: int = 0,
) -> dict[str, Any]:
    """Pooled paired ``treatment − comparator`` Recall margin across all ``classes``.

    Collects every per-question paired delta (across the named classes) and runs the
    SAME :func:`eval.d0b_parity_run.class_delta` bootstrap+MDE used per class — so the
    pooled statistic is identical machinery to the per-class one (seed-deterministic)."""
    deltas: list[float] = []
    for cls in classes:
        deltas.extend(
            paired_metric_deltas(
                records, metric="recall", treatment=treatment, comparator=comparator, cls=cls
            )
        )
    return class_delta(deltas, n_boot=n_boot, seed=seed)


def rerank_margin_summary(
    records: Sequence[Mapping[str, Any]],
    *,
    oracle_gaps: Optional[Mapping[str, Optional[float]]] = None,
    classes: Sequence[str] = MEMORY_CLASSES,
    n_boot: int = 2000,
    seed: int = 0,
) -> dict[str, Any]:
    """The CE-rerank probe verdict block (design §3): per-class + pooled paired
    ``(fathomdb_rerank − fathomdb)`` Recall margin (point + bootstrap CI + MDE + n),
    the frozen :func:`~eval.ce_rerank_rule.probe_rerank_pass` verdict on each, and the
    **diagnostic** :func:`~eval.ce_rerank_rule.headroom_captured` per class.

    ``oracle_gaps`` maps a class → the gap-decomposition ``(oracle_raw − fathomdb)``
    headroom magnitude (``component_deltas[class]["RETRIEVAL"]["point"]``) or ``None``
    when the artifact is absent → headroom-captured is ``None`` (non-gating).
    """
    gaps = oracle_gaps or {}
    # Per-class margin via the reviewed table (treatment=rerank, comparator=baseline).
    margin_table = per_class_delta_table(
        records,
        metric="recall",
        comparators=(BASELINE_ARM,),
        classes=classes,
        treatment=RERANK_ARM,
        n_boot=n_boot,
        seed=seed,
    )[BASELINE_ARM]

    # Per-arm absolute recall levels (the numerator inputs for headroom-captured).
    per_arm = recall_summary(
        records,
        classes=classes,
        treatment=RERANK_ARM,
        comparators=(BASELINE_ARM, FLOOR_ARM),
        n_boot=n_boot,
        seed=seed,
    )["per_arm_recall"]

    per_class: dict[str, Any] = {}
    for cls in classes:
        margin = margin_table[cls]
        rr = per_arm.get(RERANK_ARM, {}).get(cls, {}).get("mean")
        fb = per_arm.get(BASELINE_ARM, {}).get(cls, {}).get("mean")
        hc = (
            headroom_captured(rr, fb, gaps.get(cls))
            if rr is not None and fb is not None
            else None
        )
        per_class[cls] = {
            "margin": margin,
            "verdict": probe_rerank_pass(margin),
            "rerank_recall": rr,
            "fathomdb_recall": fb,
            "oracle_gap": gaps.get(cls),
            "headroom_captured": hc,
        }

    pooled = pooled_margin(records, classes=classes, n_boot=n_boot, seed=seed)
    pooled_rr = None
    pooled_fb = None
    # Pooled absolute levels = mean over all per-question recall values across classes.
    rr_vals = [
        float(r["recall"][RERANK_ARM])
        for r in records
        if r.get("reporting_class") in set(classes) and RERANK_ARM in (r.get("recall") or {})
    ]
    fb_vals = [
        float(r["recall"][BASELINE_ARM])
        for r in records
        if r.get("reporting_class") in set(classes) and BASELINE_ARM in (r.get("recall") or {})
    ]
    if rr_vals:
        pooled_rr = round(sum(rr_vals) / len(rr_vals), 6)
    if fb_vals:
        pooled_fb = round(sum(fb_vals) / len(fb_vals), 6)
    pooled_gap = gaps.get("pooled")
    pooled_hc = (
        headroom_captured(pooled_rr, pooled_fb, pooled_gap)
        if pooled_rr is not None and pooled_fb is not None
        else None
    )

    return {
        "treatment_arm": RERANK_ARM,
        "baseline_arm": BASELINE_ARM,
        "per_class": per_class,
        "pooled": {
            "margin": pooled,
            "verdict": probe_rerank_pass(pooled),
            "rerank_recall": pooled_rr,
            "fathomdb_recall": pooled_fb,
            "oracle_gap": pooled_gap,
            "headroom_captured": pooled_hc,
        },
    }


def load_oracle_gaps(path: str | Path) -> dict[str, Optional[float]]:
    """Read the per-class ``(oracle_raw − fathomdb)`` retrieval headroom from the
    gap-decomposition artifact (``component_deltas[class]["RETRIEVAL"]["point"]`` —
    the perfect-raw-gold UPPER bound). Missing file or class ⇒ ``None`` for that
    class (the headroom-captured diagnostic is non-gating)."""
    p = Path(path)
    gaps: dict[str, Optional[float]] = {cls: None for cls in (*MEMORY_CLASSES, "pooled")}
    if not p.exists():
        return gaps
    try:
        art = json.loads(p.read_text(encoding="utf-8"))
        comp = art.get("component_deltas") or {}
        for cls in gaps:
            cd = (comp.get(cls) or {}).get("RETRIEVAL") or {}
            pt = cd.get("point")
            gaps[cls] = float(pt) if pt is not None else None
    except Exception:  # noqa: BLE001 — a malformed artifact must not fabricate gaps
        return {cls: None for cls in (*MEMORY_CLASSES, "pooled")}
    return gaps


def build_probe_artifact(
    records: Sequence[Mapping[str, Any]],
    *,
    k: int,
    pool_n: int,
    rerank_depth: int,
    corpus_hash: Optional[str],
    seed: int,
    n_boot: int,
    arms_run: Sequence[str],
    blockers: Sequence[Mapping[str, str]],
    oracle_gaps: Optional[Mapping[str, Optional[float]]] = None,
    oracle_source: Optional[str] = None,
    smoke: bool = False,
) -> dict[str, Any]:
    """Assemble the pinned, deterministic probe output artifact (design §2/§3)."""
    rerank_block = rerank_margin_summary(
        records, oracle_gaps=oracle_gaps, n_boot=n_boot, seed=seed
    )
    return {
        "schema": "0.8.3-ce-rerank-probe-v1",
        "smoke": smoke,
        "k": k,
        "pool_n": pool_n,
        "rerank_depth": rerank_depth,
        "ce_model_repo": CE_MODEL_REPO,
        "ce_model_name": CE_MODEL_NAME,
        "corpus_hash": corpus_hash,
        "seed": seed,
        "n_boot": n_boot,
        "n_items": len(records),
        "arms_run": list(arms_run),
        "oracle_headroom_source": oracle_source,
        "blockers": list(blockers),
        # The headline gate: (fathomdb_rerank − fathomdb) Recall@K margin + verdict.
        "rerank_margin": rerank_block,
        # The full strict-recall levels (lme-only + lme+locomo) for every arm.
        "lme_plus_locomo": recall_summary(
            records,
            source_filter=None,
            treatment=RERANK_ARM,
            comparators=(BASELINE_ARM, FLOOR_ARM),
            n_boot=n_boot,
            seed=seed,
        ),
    }


# --------------------------------------------------------------------------- #
# Live corpus / real-reranker stand-up (CLI only — not exercised by unit tests).
# --------------------------------------------------------------------------- #


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    ap = argparse.ArgumentParser(
        description="0.8.3 CE-rerank precision probe ($0 / CPU / LLM-free)"
    )
    ap.add_argument("--gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    ap.add_argument("--locomo", default="data/corpus-data/raw/locomo10.json")
    ap.add_argument("--output", required=True)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--pool-n", type=int, default=POOL_N)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--n-boot", type=int, default=2000)
    ap.add_argument("--fathomdb-db", default="/tmp/ce-rerank-probe-fathomdb.sqlite")
    ap.add_argument(
        "--oracle-headroom",
        default="dev/plans/runs/0.8.3-gap-decomposition-n606.json",
        help="gap-decomp artifact for the diagnostic headroom-captured (non-gating)",
    )
    ap.add_argument(
        "--smoke",
        type=int,
        default=0,
        help="if >0, cap each reporting_class to this many items (small end-to-end)",
    )
    args = ap.parse_args(argv)

    import fathomdb  # the real CE reranker (CPU; rerank_depth>0 loads TinyBERT once)

    from eval.d0b_powered_recall import filter_min_sessions, lme_items, locomo_items
    from eval.gold_repin import load_lme
    from eval.locomo_loader import load_locomo
    from eval.r2_parity_eval import NaiveRAGAdapter, load_repin_gold, session_id_of

    # --- gold ----------------------------------------------------------------
    corpus_hash, _qv, lme_queries = load_repin_gold(Path(args.gold))
    lme_pool = lme_items(lme_queries)
    loco_docs, loco_gold = load_locomo(args.locomo)
    loco_pool_raw = locomo_items(loco_gold)
    loco_pool, n_dropped = filter_min_sessions(
        loco_pool_raw, min_sessions=2, classes=("multi_session",)
    )
    items = [*lme_pool, *loco_pool]

    # --smoke: deterministic per-class cap (small end-to-end on real data).
    if args.smoke > 0:
        capped: list[Any] = []
        per_class_count: dict[str, int] = {}
        for it in items:
            c = per_class_count.get(it.reporting_class, 0)
            if c < args.smoke:
                capped.append(it)
                per_class_count[it.reporting_class] = c + 1
        items = capped
    print(
        f"[ce-rerank] items={len(items)} (LME={len(lme_pool)} LOCOMO={len(loco_pool)} "
        f"2-session-dropped={n_dropped}) smoke={args.smoke}",
        flush=True,
    )

    # --- combined corpus (footprint: LME gold sessions + LOCOMO) -------------
    lme_docs, _gq, _cs = load_lme("xiaowu0162/longmemeval-cleaned", "oracle")
    lme_gold_sids = {session_id_of(g) for it in lme_pool for g in it.gold_doc_ids}
    documents: dict[str, str] = {sid: lme_docs[sid] for sid in lme_gold_sids if sid in lme_docs}
    documents.update(loco_docs)
    print(f"[ce-rerank] corpus sessions = {len(documents)}", flush=True)

    # --- arms ----------------------------------------------------------------
    blockers: list[dict[str, str]] = []
    adapters: dict[str, Any] = {FLOOR_ARM: NaiveRAGAdapter(documents)}
    try:
        from eval.r2_parity_eval import _build_fathomdb

        fdb, blk = _build_fathomdb(documents, Path(args.fathomdb_db))
        if fdb is not None:
            adapters[BASELINE_ARM] = fdb
            adapters[RERANK_ARM] = FathomDBRerankAdapter(
                base=fdb,
                rerank_fn=fathomdb.rerank,
                pool_n=args.pool_n,
                rerank_depth=args.pool_n,
            )
        if blk is not None:
            blockers.append(blk)
    except Exception as exc:  # noqa: BLE001 — record a blocker, never crash
        blockers.append({"id": "fathomdb-ingest-failed", "description": str(exc)})

    print(
        f"[ce-rerank] arms={sorted(adapters)} blockers={[b['id'] for b in blockers]}",
        flush=True,
    )

    records = recall_records(items, adapters, k=args.k)
    oracle_gaps = load_oracle_gaps(args.oracle_headroom)
    oracle_source = (
        args.oracle_headroom if Path(args.oracle_headroom).exists() else None
    )

    art = build_probe_artifact(
        records,
        k=args.k,
        pool_n=args.pool_n,
        rerank_depth=args.pool_n,
        corpus_hash=corpus_hash,
        seed=args.seed,
        n_boot=args.n_boot,
        arms_run=sorted(adapters),
        blockers=blockers,
        oracle_gaps=oracle_gaps,
        oracle_source=oracle_source,
        smoke=args.smoke > 0,
    )
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(
        f"[ce-rerank] wrote {args.output} | pooled verdict="
        f"{art['rerank_margin']['pooled']['verdict']}",
        flush=True,
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
