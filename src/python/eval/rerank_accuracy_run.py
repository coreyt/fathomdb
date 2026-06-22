"""0.8.3 Slice-20 CE-rerank ACCURACY arm runner — the realizable precision-lever
go/no-go on the ANSWER-ACCURACY axis (design ``dev/design/0.8.3-rerank-accuracy-arm.md``).

The gap-decomposition verdict named retrieval **precision** as the dominant lever
(perfect raw-gold retrieval recovers ~+0.39 — an UPPER bound) and showed that
**strict recall@K is blind** to it (gold in-window but buried). So the Slice-20
go/no-go must be measured on **answer accuracy**, not recall. This runner adds ONE
priced answer arm and reuses every already-paid comparison cell:

* **fathomdb_reranked** (NEW, priced) — per query, FathomDB's fused top-N pool
  (:data:`POOL_N`, pinned) → ``fathomdb.rerank(query, pool, rerank_depth=POOL_N)``
  (:class:`eval.ce_rerank_probe.FathomDBRerankAdapter`) → the reranked top-K
  (:data:`K`) bodies → the SAME identical-answerer (reader gpt-5.4 priced /
  gpt-5-nano cheap) → accuracy with the SAME :class:`eval.r2_parity_eval.PerClassScorer`.
* **fathomdb / mem0_oss** — REUSED from the D0b per-question checkpoint (already paid).
* **oracle_raw** — REUSED from the gap-decomposition per-question checkpoint (the
  perfect-raw-gold accuracy ceiling; for the non-gating ``oracle_headroom_captured``).

The gate is the paired ``(fathomdb_reranked − fathomdb)`` ACCURACY margin (per-class
+ pooled) fed through the frozen pure :mod:`eval.rerank_accuracy_rule` (PASS / FAIL /
INCONCLUSIVE + the GO flag). Reuse, do not reinvent: this runner reuses the reviewed
paired-bootstrap CI + MDE (:func:`eval.d0b_parity_run.class_delta` /
:func:`~eval.d0b_parity_run.paired_metric_deltas` /
:func:`~eval.d0b_parity_run.per_class_delta_table`), the pre-call $-cap
:class:`eval.gap_decomposition_run.BudgetLedger` + :func:`~eval.gap_decomposition_run.answer_with_budget`,
the per-question checkpoint/resume, and the D0b-checkpoint HARD-STOP cell loader
(:func:`eval.gap_decomposition_run.load_d0b_cells`).

Priced-run resilience ([[priced-runs-need-resilience-before-spend]]) — mandatory
BEFORE spend: per-question checkpoint + auto-resume, the pre-call projected $-cap
(``ledger + projected ≤ $30``, halt BEFORE the call), failure ≠ abstention (a
retry-exhausted cell is ABSENT, not a fabricated ``None``), and a completeness
citability gate (an incomplete / cap-aborted run is NON-CITABLE — no PASS/GO over a
partial prefix).

Pure helpers (``load_reused_cells`` / ``accuracy_margin_summary`` / ``per_arm_accuracy``
/ ``run_rerank_accuracy`` with injected fakes) are import-light + backend-free so the
unit tests run with fake adapters + a fake answerer (no DB, no LLM, no ``fathomdb``).
"""

from __future__ import annotations

import argparse
import json
import time
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Optional

from eval.ce_rerank_probe import CE_MODEL_NAME, CE_MODEL_REPO, FathomDBRerankAdapter
from eval.d0b_parity_run import (
    class_delta,
    fit_context,
    paired_metric_deltas,
    per_class_delta_table,
)
from eval.decision_rule_083 import MEMORY_CLASSES
from eval.gap_decomposition_run import (
    CONTEXT_CHAR_BUDGET,
    D0B_OPENING_BALANCE_USD,
    DEFAULT_MAX_OUTPUT_TOKENS,
    HARD_CAP_USD,
    BudgetExceeded,
    BudgetLedger,
    _context_contains_answer,
    answer_completeness,
    answer_retention,
    answer_with_budget,
    load_d0b_cells,
    price_for,
    resolve_reader,
)
from eval.m1_verdict_run import _atomic_write_json
from eval.r2_parity_eval import BaseAnswerer, GoldQuery, PerClassScorer
from eval.rerank_accuracy_rule import decide_rerank_accuracy

# --------------------------------------------------------------------------- #
# Frozen wiring (design §2/§3). Pinned + echoed in the output for auditability.
# --------------------------------------------------------------------------- #

#: The NEW priced arm: the CE rerank over the fused pool, answered.
RERANK_ARM: str = "fathomdb_reranked"
#: The baseline the gate is paired against (FathomDB, no rerank).
BASELINE_ARM: str = "fathomdb"

#: The cells REUSED (NOT recomputed) from the prior priced checkpoints.
#: ``fathomdb`` + ``mem0_oss`` from the D0b checkpoint; ``oracle_raw`` from the
#: gap-decomposition checkpoint (the perfect-raw-gold accuracy ceiling).
D0B_REUSED_ARMS: tuple[str, ...] = ("fathomdb", "mem0_oss")
GAP_REUSED_ARMS: tuple[str, ...] = ("oracle_raw",)
REUSED_ARMS: tuple[str, ...] = (*D0B_REUSED_ARMS, *GAP_REUSED_ARMS)

#: The four agentic-memory classes scored (same as decision_rule_083).
GAP_CLASSES: tuple[str, ...] = MEMORY_CLASSES

#: The fused top-N pool the CE reranks (N ≫ K). Pinned (== the recall probe's POOL_N).
POOL_N: int = 50
#: The CE rerank depth (rerank the whole pool).
RERANK_DEPTH: int = 50
#: The reranked top-K bodies fed to the answerer.
K: int = 10

#: Default paired-bootstrap resample count (deterministic given seed).
DEFAULT_N_BOOT = 2000

#: The published verdict token for a NON-CITABLE run (capped / incomplete). A run in
#: this state must NEVER emit a PASS / GO — an incomplete priced run is non-citable
#: until completeness is satisfied (mirror gap_decomposition_run).
ABORTED_VERDICT = "ABORTED_INCOMPLETE"
#: Answer-completeness floor: every answerable question's reranked arm must be
#: processed for the run to be citable. 1.0 = every answerable question reached.
ANSWER_COMPLETENESS_MIN = 1.0


# --------------------------------------------------------------------------- #
# Reused-cell loading (checkpoint or HARD-STOP via load_d0b_cells; design §2).
# --------------------------------------------------------------------------- #


def load_reused_cells(
    d0b_checkpoint: str | Path,
    gap_checkpoint: Optional[str | Path] = None,
) -> dict[tuple[str, str], dict[str, Any]]:
    """Load the already-paid comparison cells → ``{(qid, arm): {acc, answer}}``.

    ``fathomdb`` + ``mem0_oss`` come from the **D0b** per-question checkpoint;
    ``oracle_raw`` (the perfect-raw-gold ceiling, for the non-gating
    ``oracle_headroom_captured`` diagnostic) comes from the **gap-decomposition**
    per-question checkpoint when supplied. Both go through
    :func:`eval.gap_decomposition_run.load_d0b_cells`, which HARD-STOPS
    (:class:`~eval.gap_decomposition_run.CheckpointMissingRecords`) on an
    aggregate-only artifact with no per-question ``records`` — never an aggregate
    fallback (paired CIs against the baseline require per-question cells)."""
    cells = load_d0b_cells(d0b_checkpoint, arms=D0B_REUSED_ARMS)
    if gap_checkpoint is not None:
        cells.update(load_d0b_cells(gap_checkpoint, arms=GAP_REUSED_ARMS))
    return cells


# --------------------------------------------------------------------------- #
# Margin statistics (reuse the reviewed paired-bootstrap CI; no drift).
# --------------------------------------------------------------------------- #


def per_arm_accuracy(
    records: Sequence[Mapping[str, Any]],
    *,
    arm: str,
    cls: Optional[str] = None,
) -> Optional[float]:
    """Mean accuracy of ``arm`` over the records (optionally within ``cls``).

    Only cells whose ``acc[arm]`` is non-``None`` contribute (a missing/failed cell
    is excluded, never counted as 0). Returns ``None`` when the arm has no cell."""
    vals = [
        float((r.get("acc") or {})[arm])
        for r in records
        if (cls is None or r.get("reporting_class") == cls)
        and (r.get("acc") or {}).get(arm) is not None
    ]
    return round(sum(vals) / len(vals), 6) if vals else None


def accuracy_margin_summary(
    records: Sequence[Mapping[str, Any]],
    *,
    classes: Sequence[str] = GAP_CLASSES,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
) -> dict[str, Any]:
    """The Slice-20 accuracy-arm verdict block (design §3): per-class + pooled paired
    ``(fathomdb_reranked − fathomdb)`` accuracy margin (point + bootstrap CI + MDE +
    n), the frozen :func:`~eval.rerank_accuracy_rule.decide_rerank_accuracy` decision
    on each (lever_realized + gap_to_mem0_closed + oracle_headroom_captured + GO),
    and each arm's mean accuracy. Reuses the reviewed
    :func:`eval.d0b_parity_run.class_delta` machinery so the statistic cannot drift.

    Deterministic given a fixed ``seed``."""
    margin_table = per_class_delta_table(
        records,
        metric="acc",
        comparators=(BASELINE_ARM,),
        classes=classes,
        treatment=RERANK_ARM,
        n_boot=n_boot,
        seed=seed,
    )[BASELINE_ARM]

    per_class: dict[str, Any] = {}
    for cls in classes:
        margin = margin_table[cls]
        decision = decide_rerank_accuracy(
            margin,
            acc_reranked=per_arm_accuracy(records, arm=RERANK_ARM, cls=cls),
            acc_fathomdb=per_arm_accuracy(records, arm=BASELINE_ARM, cls=cls),
            acc_mem0=per_arm_accuracy(records, arm="mem0_oss", cls=cls),
            acc_oracle_raw=per_arm_accuracy(records, arm="oracle_raw", cls=cls),
        )
        per_class[cls] = {"margin": margin, **decision}

    # Pooled: collect every per-question paired delta across the named classes, then
    # run the SAME bootstrap+MDE (identical machinery to the per-class one).
    pooled_deltas: list[float] = []
    for cls in classes:
        pooled_deltas += paired_metric_deltas(
            records, metric="acc", treatment=RERANK_ARM, comparator=BASELINE_ARM, cls=cls
        )
    pooled_margin = class_delta(pooled_deltas, n_boot=n_boot, seed=seed)
    pooled_decision = decide_rerank_accuracy(
        pooled_margin,
        acc_reranked=per_arm_accuracy(records, arm=RERANK_ARM),
        acc_fathomdb=per_arm_accuracy(records, arm=BASELINE_ARM),
        acc_mem0=per_arm_accuracy(records, arm="mem0_oss"),
        acc_oracle_raw=per_arm_accuracy(records, arm="oracle_raw"),
    )

    return {
        "rerank_arm": RERANK_ARM,
        "baseline_arm": BASELINE_ARM,
        "per_class": per_class,
        "pooled": {"margin": pooled_margin, **pooled_decision},
        "per_arm_accuracy": {
            arm: per_arm_accuracy(records, arm=arm)
            for arm in (RERANK_ARM, *REUSED_ARMS)
        },
    }


# --------------------------------------------------------------------------- #
# Orchestrator.
# --------------------------------------------------------------------------- #


def run_rerank_accuracy(
    *,
    queries: Sequence[GoldQuery],
    reused_cells: Mapping[tuple[str, str], Mapping[str, Any]],
    reranked_adapter: Optional[Any],
    answerer: BaseAnswerer,
    ledger: BudgetLedger,
    reader: str,
    output: Path,
    corpus_hash: Optional[str] = None,
    k: int = K,
    pool_n: int = POOL_N,
    rerank_depth: int = RERANK_DEPTH,
    budget: Optional[int] = CONTEXT_CHAR_BUDGET,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
    classes: Sequence[str] = GAP_CLASSES,
    checkpoint_path: Optional[Path] = None,
    checkpoint_every: int = 10,
    mode: str = "run",
) -> dict[str, Any]:
    """Run the CE-rerank accuracy arm + emit the per-class + pooled accuracy margin,
    the frozen decision (PASS/FAIL/INCONCLUSIVE + GO), the per-arm accuracy, the $
    ledger, and the citability gate.

    The reader answers ONLY the ``fathomdb_reranked`` arm; ``fathomdb`` / ``mem0_oss``
    / ``oracle_raw`` acc come from ``reused_cells`` (already paid — NOT recomputed).
    Every reader call passes the pre-call budget guard; a :class:`BudgetExceeded`
    checkpoints + HALTS (``aborted_for_cap``); a non-budget answerer failure leaves
    that cell ABSENT and the run continues (failure ≠ abstention)."""
    t0 = time.time()
    scorer = PerClassScorer()
    answerer_available = bool(getattr(answerer, "available", False))
    ckpt_path = checkpoint_path or output.with_suffix(".checkpoint.json")

    # Resume the reranked-arm answers from a prior checkpoint (membership = reuse
    # signal) + carry forward the cumulative $ spend (cap is per-EXPERIMENT).
    rmap: dict[tuple[str, str], Optional[str]] = {}
    if ckpt_path.exists():
        prior = json.loads(ckpt_path.read_text(encoding="utf-8"))
        prior_spent = prior.get("ledger_spent_usd")
        if prior_spent is not None:
            ledger.restore_spent(float(prior_spent))
        for r in prior.get("records") or []:
            qid = r.get("qid")
            if qid is None:
                continue
            ans = (r.get("answers") or {}).get(RERANK_ARM)
            if RERANK_ARM in (r.get("answers") or {}):
                rmap[(str(qid), RERANK_ARM)] = ans

    records: list[dict[str, Any]] = []
    aborted_for_cap = False

    def _checkpoint() -> None:
        # Persist the cumulative ledger spend ATOMICALLY with the records so a resume
        # restores the true per-experiment spend.
        _atomic_write_json(
            ckpt_path,
            {"records": records, "mode": mode, "reader": reader, "ledger_spent_usd": ledger.spent},
        )

    for i, q in enumerate(queries, start=1):
        rec: dict[str, Any] = {
            "qid": q.query_id,
            "reporting_class": q.reporting_class,
            "gold": list(q.gold_doc_ids),
            "has_answers": bool(q.answers),
            "answers": {},
            "acc": {},
            "context_has_gold": {},
        }
        # Reuse the paid fathomdb + mem0 + oracle_raw acc cells (NOT recomputed).
        for arm in REUSED_ARMS:
            cell = reused_cells.get((q.query_id, arm))
            if cell is not None and cell.get("acc") is not None:
                rec["acc"][arm] = float(cell["acc"])

        if answerer_available and q.answers and reranked_adapter is not None:
            hits = reranked_adapter.retrieve(q.question, k)
            bodies = [h.body for h in hits[:k] if h.body]
            ctx = fit_context(bodies, budget)
            rec["context_has_gold"][RERANK_ARM] = _context_contains_answer(ctx, q.answers)
            key = (q.query_id, RERANK_ARM)
            if key in rmap:
                ans = rmap[key]
                rec["answers"][RERANK_ARM] = ans
                rec["acc"][RERANK_ARM] = scorer.score_answer(list(q.answers), ans)
            else:
                try:
                    ans = answer_with_budget(
                        answerer, reader=reader, question=q.question, context=ctx, ledger=ledger
                    )
                except BudgetExceeded:
                    # A pre-call cap projection → clean cap-abort (checkpoint + stop).
                    aborted_for_cap = True
                    records.append(rec)
                    _checkpoint()
                    break
                except Exception:  # noqa: BLE001 — non-budget failure ≠ abstention
                    # A retry-exhausted 429/5xx or non-retryable HTTP error leaves this
                    # cell ABSENT (never fabricated); the run CONTINUES. The completeness
                    # gate then marks the artifact non-citable.
                    pass
                else:
                    rec["answers"][RERANK_ARM] = ans
                    rec["acc"][RERANK_ARM] = scorer.score_answer(list(q.answers), ans)

        records.append(rec)
        if i % checkpoint_every == 0 or i == len(queries):
            _checkpoint()

    summary = accuracy_margin_summary(records, classes=classes, n_boot=n_boot, seed=seed)

    # --- citability gate ----------------------------------------------------- #
    # An incomplete / cap-aborted priced run is NON-CITABLE: a low-variance prefix
    # must not emit a powered PASS/GO for an INCOMPLETE experiment. When aborted OR
    # answer-completeness is below the floor, suppress the verdict + GO and publish
    # ABORTED_INCOMPLETE.
    completeness = answer_completeness(records, queries, new_arms=(RERANK_ARM,))
    incomplete = aborted_for_cap or completeness < ANSWER_COMPLETENESS_MIN
    citable = not incomplete
    if aborted_for_cap:
        non_citable_reason: Optional[str] = "aborted_for_cap"
    elif incomplete:
        non_citable_reason = f"answer_completeness:{completeness:.4f}<{ANSWER_COMPLETENESS_MIN}"
    else:
        non_citable_reason = None

    if citable:
        verdict = str(summary["pooled"]["lever_realized"])
        go = bool(summary["pooled"]["go"])
    else:
        verdict = ABORTED_VERDICT
        go = False
        # Non-citable invariant: neutralize the NESTED decision fields too. A capped/incomplete
        # prefix can compute a powered nested pooled/per_class lever_realized="PASS" +
        # go=True that a downstream reader could publish despite citable=False. Suppress
        # the DECISION fields (lever_realized → ABORTED_VERDICT, go → False) + stamp the
        # reason on pooled + every per_class entry; the raw numeric margin stats
        # (point/ci/mde/n) survive untouched for forensics.
        for block in (summary["pooled"], *summary["per_class"].values()):
            block["lever_realized"] = ABORTED_VERDICT
            block["go"] = False
            block["reason"] = non_citable_reason

    retention = answer_retention(records, arm=RERANK_ARM)

    art: dict[str, Any] = {
        "schema": "0.8.3-rerank-accuracy-v1",
        "mode": mode,
        "reader_model": reader,
        "k": k,
        "pool_n": pool_n,
        "rerank_depth": rerank_depth,
        "ce_model_repo": CE_MODEL_REPO,
        "ce_model_name": CE_MODEL_NAME,
        "corpus_hash": corpus_hash,
        "context_char_budget": budget,
        "n_boot": n_boot,
        "seed": seed,
        "rerank_arm": RERANK_ARM,
        "baseline_arm": BASELINE_ARM,
        "reused_arms": list(REUSED_ARMS),
        "n_questions": len(records),
        "n_per_class": {c: sum(1 for r in records if r["reporting_class"] == c) for c in classes},
        "accuracy_margin": summary,
        "verdict": verdict,
        "go": go,
        "citable": citable,
        "run_valid": citable,
        "answer_completeness": completeness,
        "answer_completeness_min": ANSWER_COMPLETENESS_MIN,
        "non_citable_reason": non_citable_reason,
        "answer_retention": retention,
        "ledger": {
            "opening_balance_usd": ledger.opening_balance_usd,
            "hard_cap_usd": ledger.hard_cap_usd,
            "spent_usd": ledger.spent,
            "remaining_usd": ledger.remaining,
        },
        "aborted_for_cap": aborted_for_cap,
        "elapsed_s": round(time.time() - t0, 1),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(art, indent=2, default=str), encoding="utf-8")
    print(
        f"[RERANK-ACC][{mode.upper()}] wrote {output} | {len(records)} Q | "
        f"spent ${ledger.spent:.4f}/{ledger.hard_cap_usd:.0f} | "
        f"citable={citable} compl={completeness} | verdict={verdict} go={go}",
        flush=True,
    )
    return art


# --------------------------------------------------------------------------- #
# CLI (live backends — not exercised by the unit tests).
# --------------------------------------------------------------------------- #


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    ap = argparse.ArgumentParser(description="0.8.3 Slice-20 CE-rerank ACCURACY arm runner")
    ap.add_argument("--mode", choices=["cheap", "full"], required=True)
    ap.add_argument("--reader", default=None,
                    help="airlock reader id; cheap-mode default routes off the priced "
                    "reader. Pass --reader gpt-5-nano for a $0-ish cheap-validate.")
    ap.add_argument("--gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    ap.add_argument("--d0b-checkpoint", required=True,
                    help="D0b per-question checkpoint (fathomdb + mem0_oss cells; "
                    "HARD-STOP if no per-question records)")
    ap.add_argument("--gap-checkpoint", default=None,
                    help="gap-decomposition per-question checkpoint (oracle_raw cells; "
                    "optional — only the non-gating oracle_headroom_captured needs it)")
    ap.add_argument("--output", required=True)
    ap.add_argument("--k", type=int, default=K)
    ap.add_argument("--pool-n", type=int, default=POOL_N)
    ap.add_argument("--context-char-budget", type=int, default=CONTEXT_CHAR_BUDGET)
    ap.add_argument("--per-class", type=int, default=None)
    ap.add_argument("--max-output-tokens", type=int, default=DEFAULT_MAX_OUTPUT_TOKENS)
    ap.add_argument("--opening-balance-usd", type=float, default=D0B_OPENING_BALANCE_USD,
                    help="cumulative 0.8.3 spend already paid (carried so the $30 cap "
                    "is per-PROGRAM, not per-run)")
    ap.add_argument("--hard-cap-usd", type=float, default=HARD_CAP_USD)
    ap.add_argument("--n-boot", type=int, default=DEFAULT_N_BOOT)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--fathomdb-db", default="/tmp/rerank-accuracy-fathomdb.sqlite")
    args = ap.parse_args(argv)

    import fathomdb  # the real CE reranker (CPU; rerank_depth>0 loads TinyBERT once)

    from eval.d0b_parity_run import _select_subset, build_documents_from_lme, build_live_adapters
    from eval.m1_baseline_run import CostTrackingAnswerer
    from eval.r2_parity_eval import load_repin_gold

    reader = resolve_reader(args.mode, args.reader)  # cheap-mode → cheap reader
    price_for(reader)  # fail closed BEFORE any backend stand-up

    corpus_hash, _qv, queries = load_repin_gold(Path(args.gold))
    if args.per_class:
        queries = _select_subset(queries, per_class=args.per_class, classes=GAP_CLASSES)

    # HARD-STOP here if a checkpoint lacks per-question records.
    reused_cells = load_reused_cells(args.d0b_checkpoint, args.gap_checkpoint)

    documents = build_documents_from_lme(queries)
    print(
        f"[RERANK-ACC][CLI] {len(queries)} queries, {len(documents)} sessions | "
        f"corpus_hash={corpus_hash[:12]} reused_cells={len(reused_cells)}",
        flush=True,
    )

    adapters, blockers = build_live_adapters(
        documents, want_mem0=False, want_graphiti=False, db_path=args.fathomdb_db,
    )
    base = adapters.get("fathomdb")
    if base is None:
        raise SystemExit(
            f"[RERANK-ACC][STOP] no fathomdb adapter (blockers={[b['id'] for b in blockers]})"
        )
    reranked_adapter = FathomDBRerankAdapter(
        base=base, rerank_fn=fathomdb.rerank, pool_n=args.pool_n, rerank_depth=args.pool_n,
    )

    answerer = CostTrackingAnswerer(reader, timeout_s=240.0)
    if not answerer.available:
        raise SystemExit(f"[RERANK-ACC][STOP] reader {reader!r} unavailable — do not fake answers")

    ledger = BudgetLedger(
        opening_balance_usd=args.opening_balance_usd,
        hard_cap_usd=args.hard_cap_usd,
        max_output_tokens=args.max_output_tokens,
    )

    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused_cells, reranked_adapter=reranked_adapter,
        answerer=answerer, ledger=ledger, reader=reader, output=Path(args.output),
        corpus_hash=corpus_hash, k=args.k, pool_n=args.pool_n, rerank_depth=args.pool_n,
        budget=args.context_char_budget, n_boot=args.n_boot, seed=args.seed, mode=args.mode,
    )
    art["blockers_encountered"] = blockers
    Path(args.output).write_text(json.dumps(art, indent=2, default=str), encoding="utf-8")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
