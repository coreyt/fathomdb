"""0.8.3 D0b — per-class FathomDB − {Mem0, Graphiti/Zep, naive-RAG} parity runner.

THE TARGET (design `0.8.3-mem0-parity.md` §2/§4): per memory class, the external
``FathomDB − {mem0_oss, graphiti_zep, naive_rag}`` delta on **(a) strict full-gold
Recall@K (LLM-free, $0)** and **(b) identical-answerer accuracy (PRICED)**, each with
a paired 95% CI + a per-class paired MDE, on the re-pinned power-sized gold
(``repin_hash 2916cace``). The accuracy deltas feed the frozen
:func:`eval.decision_rule_083.decide_083` → REACHED / NOT_REACHED + per-class gap.

Resilience ([[priced-runs-need-resilience-before-spend]]) — REUSES
``m1_verdict_run``'s proven mechanisms, never a hand-rolled weaker one:

* :func:`eval.m1_verdict_run._atomic_write_json` — temp-file + ``os.replace`` atomic
  checkpoint; a kill mid-write can never corrupt the live sidecar.
* :func:`eval.m1_verdict_run._resolve_resume` — auto-resume ON BY DEFAULT from the
  ``<output>.checkpoint.json`` sidecar; a relaunch reuses every persisted ``(qid,arm)``
  answer cell (key-present incl. ``None`` = a real abstention, reused; absent = a prior
  failure, re-called) → zero re-spend.
* The answerer is :class:`eval.m1_baseline_run.CostTrackingAnswerer` — exponential
  backoff over transient 429/5xx; a cell fails only after ``max_retries`` are exhausted.
* **failure ≠ abstention**: a retry-exhausted cell is MISSING (absent key), counted in
  ``n_errors``, never persisted as a scored ``None`` (which would deflate the arm).
* **completeness validity guard**: a pass is citable only at completeness ≥
  :data:`VALID_COMPLETENESS_FLOOR`; below it the run is flagged INVALID.

The only priced seam is the shared answerer (the R2 identical-answerer invariant);
retrieval / recall / scoring / bootstrap is $0. Pure helpers (``paired_*`` /
``class_delta`` / ``per_class_delta_table`` / ``external_per_class_for_decide`` /
``answer_completeness``) are import-light + backend-free so the unit tests run with
fake adapters + a fake answerer (no DB, no LLM, no ``mem0``).
"""

from __future__ import annotations

import argparse
import math
import os
import time
from pathlib import Path
from typing import Any, Mapping, Optional, Sequence

from eval.decision_rule_083 import MEMORY_CLASSES, decide_083
from eval.m1_power_sim import _percentile_ci_high, _percentile_ci_low
from eval.m1_verdict_run import _atomic_write_json, _resolve_resume
from eval.r2_parity_eval import (
    BaseAnswerer,
    GoldQuery,
    PerClassScorer,
    load_repin_gold,
)

# --------------------------------------------------------------------------- #
# Frozen arm wiring (design §3).
# --------------------------------------------------------------------------- #

#: The treatment arm whose gap to every comparator is THE deliverable.
TREATMENT_ARM = "fathomdb"
#: The external comparators (design §3); Graphiti/Zep is the 2nd comparator (a
#: recorded blocker, not a hard dependency, if its local stand-up is intractable).
COMPARATOR_ARMS: tuple[str, ...] = ("mem0_oss", "graphiti_zep", "naive_rag")
#: The full arm order reported (treatment first).
ALL_ARMS: tuple[str, ...] = (TREATMENT_ARM, *COMPARATOR_ARMS)

#: The priced reader (design §3) + the ~$0 cheap-validate reader. The airlock proxy
#: serves these exact ids (verified at stand-up).
STRONG_READER_DEFAULT = os.environ.get("D0B_STRONG_READER", "gpt-5.4")
CHEAP_READER_DEFAULT = os.environ.get("D0B_CHEAP_READER", "gemini-flash-lite")

#: Minimum answer-matrix completeness for a priced pass to be a citable verdict
#: (mirrors ``m1_verdict_run.VALID_COMPLETENESS_FLOOR``).
VALID_COMPLETENESS_FLOOR = 0.97

#: Default paired-bootstrap resample count for the CI (deterministic given seed).
DEFAULT_N_BOOT = 2000

#: Normal-approx paired MDE z-multipliers: two-sided α=0.05 + 80% power.
_Z_975 = 1.959963984540054
_Z_80 = 0.8416212335729143


# --------------------------------------------------------------------------- #
# Pure stats helpers (backend-free; the TDD core).
# --------------------------------------------------------------------------- #


def paired_metric_deltas(
    records: Sequence[Mapping[str, Any]],
    *,
    metric: str,
    treatment: str,
    comparator: str,
    cls: str,
) -> list[float]:
    """Per-question paired ``treatment − comparator`` deltas for ``metric`` in ``cls``.

    ``metric`` is ``"recall"`` or ``"acc"``. A question contributes a paired delta
    ONLY when both arms carry a non-``None`` value for that metric on it — so a
    missing/failed answerer cell (absent key) or an arm that did not run is excluded
    from the paired sample (it cannot fabricate a 0 delta)."""
    deltas: list[float] = []
    for r in records:
        if r.get("reporting_class") != cls:
            continue
        m = r.get(metric) or {}
        tv = m.get(treatment)
        cv = m.get(comparator)
        if tv is not None and cv is not None:
            deltas.append(float(tv) - float(cv))
    return deltas


def class_delta(deltas: Sequence[float], *, n_boot: int = DEFAULT_N_BOOT, seed: int = 0) -> dict[str, Any]:
    """``{point, ci_lo, ci_hi, mde, n}`` for a paired-delta sample.

    ``point`` = mean; ``ci_lo/ci_hi`` = percentile paired bootstrap (reuses the m1
    helpers, seed-deterministic); ``mde`` = normal-approx paired minimum detectable
    effect ``(z.975+z.80)·sd/√n`` (the §5 power quantity the rule gates on). ``n`` =
    sample size. n==0 → all-``None``; n==1 → degenerate point CI, mde ``None``."""
    import numpy as np

    n = len(deltas)
    if n == 0:
        return {"point": None, "ci_lo": None, "ci_hi": None, "mde": None, "n": 0}
    arr = np.asarray(deltas, dtype=float)
    point = float(arr.mean())
    if n == 1:
        return {"point": round(point, 6), "ci_lo": round(point, 6), "ci_hi": round(point, 6), "mde": None, "n": 1}
    rng = np.random.default_rng(seed)
    lo = _percentile_ci_low(arr, rng, n_boot=n_boot)
    hi = _percentile_ci_high(arr, rng, n_boot=n_boot)
    sd = float(arr.std(ddof=1))
    mde = (_Z_975 + _Z_80) * sd / math.sqrt(n)
    return {
        "point": round(point, 6),
        "ci_lo": round(lo, 6),
        "ci_hi": round(hi, 6),
        "mde": round(mde, 6),
        "n": n,
    }


def per_class_delta_table(
    records: Sequence[Mapping[str, Any]],
    *,
    metric: str,
    comparators: Sequence[str] = COMPARATOR_ARMS,
    classes: Sequence[str] = MEMORY_CLASSES,
    treatment: str = TREATMENT_ARM,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
) -> dict[str, dict[str, dict[str, Any]]]:
    """``{comparator: {class: class_delta}}`` for one ``metric`` over every class."""
    out: dict[str, dict[str, dict[str, Any]]] = {}
    for comp in comparators:
        out[comp] = {}
        for cls in classes:
            d = paired_metric_deltas(
                records, metric=metric, treatment=treatment, comparator=comp, cls=cls
            )
            out[comp][cls] = class_delta(d, n_boot=n_boot, seed=seed)
    return out


def external_per_class_for_decide(
    acc_delta_table: Mapping[str, Mapping[str, Mapping[str, Any]]],
    *,
    comparator: str = "mem0_oss",
    classes: Sequence[str] = MEMORY_CLASSES,
) -> dict[str, dict[str, float]]:
    """Build the :func:`decide_083` input from the ACCURACY ``fathomdb − <comparator>``
    deltas (the priced gate; design §4).

    ``decide_083`` requires a finite ``{point, ci_lo, ci_hi, mde}`` + int ``n`` for
    every :data:`MEMORY_CLASSES` entry. A class with no usable delta (n==0/1 → mde
    ``None`` — e.g. the comparator arm did not run) cannot be fed; raise loudly so the
    caller records a blocker rather than fabricate a verdict."""
    table = acc_delta_table.get(comparator, {})
    ext: dict[str, dict[str, float]] = {}
    for cls in classes:
        cd = table.get(cls)
        if cd is None or cd.get("point") is None or cd.get("mde") is None:
            n = cd.get("n") if cd else "missing"
            raise ValueError(
                f"cannot build decide_083 input: class {cls!r} has no usable "
                f"{comparator!r} accuracy delta (n={n}) — record a blocker, do not "
                "fabricate a verdict"
            )
        ext[cls] = {
            "point": float(cd["point"]),
            "ci_lo": float(cd["ci_lo"]),
            "ci_hi": float(cd["ci_hi"]),
            "mde": float(cd["mde"]),
            "n": int(cd["n"]),
        }
    return ext


def project_full_cost(
    *,
    prompt_tokens: int,
    completion_tokens: int,
    n_calls: int,
    price_in_per_1m: float,
    price_out_per_1m: float,
    n_questions_full: int,
    n_priced_arms: int,
    context_token_budget: Optional[int] = None,
    overhead_tokens: int = 60,
) -> dict[str, Any]:
    """$0 linear cost projection from a measured pilot (the phase-gate number).

    The answerer cost is input-token-dominated (long session bodies in context). Model
    per-call prompt tokens as ``overhead_tokens`` (template+question) + the context
    tokens, the latter capped at ``context_token_budget`` (the window-fit lever) when
    given. ``projected_full_usd`` = per-call cost × ``n_questions_full`` ×
    ``n_priced_arms`` (the full priced matrix). Pure arithmetic; no LLM call."""
    n_calls = max(n_calls, 1)
    avg_prompt = prompt_tokens / n_calls
    avg_comp = completion_tokens / n_calls
    ctx_tokens = max(avg_prompt - overhead_tokens, 0.0)
    if context_token_budget is not None:
        ctx_tokens = min(ctx_tokens, float(context_token_budget))
    proj_prompt = overhead_tokens + ctx_tokens
    cost_per_call = proj_prompt / 1e6 * price_in_per_1m + avg_comp / 1e6 * price_out_per_1m
    full_calls = n_questions_full * n_priced_arms
    return {
        "context_token_budget": context_token_budget,
        "projected_prompt_tokens_per_call": round(proj_prompt, 1),
        "cost_per_call_usd": round(cost_per_call, 6),
        "projected_full_calls": full_calls,
        "projected_full_usd": round(cost_per_call * full_calls, 2),
    }


def answer_completeness(
    records: Sequence[Mapping[str, Any]],
    *,
    arms: Sequence[str],
    n_errors: int,
    floor: float = VALID_COMPLETENESS_FLOOR,
) -> dict[str, Any]:
    """Completeness validity guard: fraction of expected answerer cells that did NOT
    fail. Expected = number of ``(question-with-answers, retrieving-arm)`` cells.

    ``n_errors`` is the retry-exhausted failure count (from the answerer). Below
    ``floor`` the matrix is materially incomplete (a class endpoint is under-populated)
    → ``run_valid=False`` (relaunch auto-resumes only the missing cells)."""
    expected = sum(1 for r in records if r.get("has_answers") for _ in arms)
    completeness = round(1.0 - n_errors / max(expected, 1), 4)
    return {
        "expected_calls": expected,
        "n_errors": int(n_errors),
        "completeness": completeness,
        "floor": floor,
        "run_valid": completeness >= floor,
    }


def resume_map(prior: Mapping[str, Any]) -> dict[tuple[str, str], Optional[str]]:
    """``(qid, arm) -> answer`` from a prior checkpoint's ``records``.

    Membership (not value-non-``None``) is the reuse signal: a key-present ``None`` is
    a legitimate prior abstention (reused, scored, no re-call); an ABSENT key is a
    prior failure (re-called). Empty map → a full from-scratch pass."""
    out: dict[tuple[str, str], Optional[str]] = {}
    for r in prior.get("records") or []:
        qid = r.get("qid")
        if qid is None:
            continue
        for arm, ans in (r.get("answers") or {}).items():
            out[(str(qid), str(arm))] = ans
    return out


# --------------------------------------------------------------------------- #
# The resilient run loop.
# --------------------------------------------------------------------------- #


def _select_subset(queries: Sequence[GoldQuery], *, per_class: int, classes: Sequence[str]) -> list[GoldQuery]:
    """Stable per-class subset (cheap-validate / pilot): the first ``per_class``
    queries of each class, in gold order — spans all classes, deterministic."""
    out: list[GoldQuery] = []
    for cls in classes:
        out += [q for q in queries if q.reporting_class == cls][:per_class]
    return out


def fit_context(bodies: Sequence[str], budget_chars: Optional[int]) -> list[str]:
    """Window-fit the answerer context to a total ``budget_chars`` cap
    ([[priced-runs-need-resilience-before-spend]] — window-fit/chunk + the priced-cost
    lever). Bodies are added in retrieval-rank order until the budget is reached; the
    body that crosses it is truncated to the remaining budget (≥1 doc always kept).
    Applied IDENTICALLY across every arm, so the R2 same-context-budget invariant
    holds (any delta stays retrieval, not a per-arm context advantage). ``None`` /
    non-positive budget = no truncation."""
    if not budget_chars or budget_chars <= 0:
        return list(bodies)
    out: list[str] = []
    used = 0
    for b in bodies:
        if used >= budget_chars:
            break
        remaining = budget_chars - used
        if len(b) <= remaining:
            out.append(b)
            used += len(b)
        else:
            out.append(b[:remaining])
            used = budget_chars
            break
    if not out and bodies:  # always keep at least the top doc (truncated)
        out = [bodies[0][:budget_chars]]
    return out


def run_d0b(
    *,
    mode: str,
    reader: str,
    output: Path,
    queries: Optional[Sequence[GoldQuery]] = None,
    gold_path: Optional[Path] = None,
    adapters: Optional[Mapping[str, Any]] = None,
    answerer: Optional[BaseAnswerer] = None,
    k: int = 10,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
    per_class: Optional[int] = None,
    checkpoint_path: Optional[Path] = None,
    checkpoint_every: int = 10,
    resume: Optional[Path] = None,
    max_usd: float = 1.0,
    eu7_recall: float = 0.896,
    latency_ok: bool = True,
    classes: Sequence[str] = MEMORY_CLASSES,
    context_char_budget: Optional[int] = None,
    on_progress: Optional[Any] = None,
) -> dict[str, Any]:
    """Run the D0b parity matrix resiliently and emit the per-class deltas + verdict.

    ``adapters`` maps arm-name → an object exposing ``retrieve(question, k) -> [Hit]``
    (inject fakes in tests / the live adapters from the CLI). ``answerer`` is the shared
    :class:`BaseAnswerer` (inject a fake in tests / ``CostTrackingAnswerer`` from the
    CLI). When omitted, ``queries`` is loaded from ``gold_path`` — but ``adapters`` /
    ``answerer`` must always be supplied (this function builds no backend)."""
    t0 = time.time()
    if adapters is None:
        raise ValueError("run_d0b requires `adapters` (the per-arm retrieve seam)")
    if answerer is None:
        raise ValueError("run_d0b requires `answerer` (the shared identical-answerer seam)")
    if queries is None:
        if gold_path is None:
            raise ValueError("run_d0b requires `queries` or `gold_path`")
        _ch, _qv, queries = load_repin_gold(gold_path)

    if mode in ("cheap", "pilot") and per_class is not None:
        queries = _select_subset(queries, per_class=per_class, classes=classes)

    arms = [a for a in ALL_ARMS if a in adapters]
    scorer = PerClassScorer()
    answerer_available = bool(getattr(answerer, "available", False))

    ckpt_path = checkpoint_path or output.with_suffix(".checkpoint.json")
    prior = _resolve_resume(output, resume, ckpt_path)
    rmap: dict[tuple[str, str], Optional[str]] = {}
    if prior is not None and prior.exists():
        import json

        rmap = resume_map(json.loads(prior.read_text(encoding="utf-8")))
        print(
            f"[D0b][AUTO-RESUME] reusing {len(rmap)} persisted (qid,arm) answer cells "
            f"from {prior} (absent cells re-called; zero re-spend on reused)",
            flush=True,
        )

    def _usd() -> float:
        fn = getattr(answerer, "usd", None)
        return float(fn()) if callable(fn) else 0.0

    records: list[dict[str, Any]] = []
    n_errors = 0
    aborted_for_cap = False

    def _checkpoint() -> None:
        _atomic_write_json(ckpt_path, {"records": records, "mode": mode, "reader": reader})

    for i, q in enumerate(queries, start=1):
        rec: dict[str, Any] = {
            "qid": q.query_id,
            "reporting_class": q.reporting_class,
            "gold": list(q.gold_doc_ids),
            "has_answers": bool(q.answers),
            "recall": {},
            "answers": {},
            "acc": {},
        }
        for arm in arms:
            hits = adapters[arm].retrieve(q.question, k)
            if q.gold_doc_ids:
                retrieved = {h.doc_id for h in hits}
                rec["recall"][arm] = 1.0 if all(g in retrieved for g in q.gold_doc_ids) else 0.0
            if answerer_available and q.answers:
                key = (q.query_id, arm)
                if key in rmap:
                    ans = rmap[key]  # reuse (incl. a None abstention) — $0
                else:
                    ctx = fit_context([h.body for h in hits if h.body], context_char_budget)
                    try:
                        ans = answerer.answer(q.question, ctx)
                    except Exception:  # noqa: BLE001 — retry-exhausted = a MISSING cell
                        n_errors += 1
                        continue  # absent key → re-called on the next resume; not scored
                rec["answers"][arm] = ans
                rec["acc"][arm] = scorer.score_answer(list(q.answers), ans)
        records.append(rec)

        if i % checkpoint_every == 0 or i == len(queries):
            _checkpoint()
        if on_progress is not None:
            on_progress(i, len(queries), _usd())
        if answerer_available and _usd() > max_usd:
            aborted_for_cap = True
            _checkpoint()
            print(
                f"[D0b][CAP] spend ${_usd():.4f} exceeded hard cap ${max_usd} at "
                f"question {i}/{len(queries)} — stopping (checkpoint saved; resume to continue)",
                flush=True,
            )
            break

    recall_table = per_class_delta_table(
        records, metric="recall", comparators=COMPARATOR_ARMS, classes=classes, n_boot=n_boot, seed=seed
    )
    acc_table = (
        per_class_delta_table(
            records, metric="acc", comparators=COMPARATOR_ARMS, classes=classes, n_boot=n_boot, seed=seed
        )
        if answerer_available
        else None
    )

    comp = answer_completeness(records, arms=arms, n_errors=n_errors)

    decide_result: Any = None
    decide_error: Optional[str] = None
    if acc_table is not None:
        try:
            ext = external_per_class_for_decide(acc_table, comparator="mem0_oss", classes=classes)
            decide_result = decide_083(ext, eu7_recall=eu7_recall, latency_ok=latency_ok)
        except Exception as exc:  # noqa: BLE001 — record, do not crash the run
            decide_error = str(exc)

    cost_fn = getattr(answerer, "cost_block", None)
    cost_block = cost_fn() if callable(cost_fn) else {"model": reader, "usd": _usd()}
    cost_block.setdefault("n_errors", n_errors)

    art: dict[str, Any] = {
        "schema": "0.8.3-d0b-parity-v1",
        "mode": mode,
        "reader_model": reader,
        "k": k,
        "context_char_budget": context_char_budget,
        "n_boot": n_boot,
        "seed": seed,
        "arms_run": arms,
        "n_questions": len(records),
        "n_per_class": {cls: sum(1 for r in records if r["reporting_class"] == cls) for cls in classes},
        "treatment_arm": TREATMENT_ARM,
        "comparator_arms": list(COMPARATOR_ARMS),
        "recall_deltas": recall_table,
        "accuracy_deltas": acc_table,
        "decide_083": decide_result,
        "decide_083_error": decide_error,
        "answer_completeness": comp,
        "run_valid": comp["run_valid"] and not aborted_for_cap,
        "aborted_for_cap": aborted_for_cap,
        "cost": cost_block,
        "elapsed_s": round(time.time() - t0, 1),
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    import json

    output.write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(
        f"[D0b][{mode.upper()}] wrote {output} | {len(records)} Q, arms={arms}, "
        f"${_usd():.4f}, completeness={comp['completeness']}, "
        f"decide={(decide_result or {}).get('verdict') if decide_result else decide_error}",
        flush=True,
    )
    return art


# --------------------------------------------------------------------------- #
# Live document / adapter stand-up (CLI only — not exercised by the unit tests).
# --------------------------------------------------------------------------- #


def build_documents_from_lme(
    queries: Sequence[GoldQuery],
    *,
    dataset: str = "xiaowu0162/longmemeval-cleaned",
    split: str = "oracle",
    distractor_cap: int = 0,
) -> dict[str, str]:
    """Load LME sessions and restrict to the gold-relevant sessions of ``queries``
    (footprint + Mem0 per-``add()`` cost), optionally padded with up to
    ``distractor_cap`` extra sessions for retrieval realism. CLI-only."""
    from eval.r2_parity_eval import load_longmemeval, session_id_of

    documents, _q = load_longmemeval(dataset, split)
    gold_sessions = {session_id_of(g) for q in queries for g in q.gold_doc_ids}
    out: dict[str, str] = {sid: documents[sid] for sid in gold_sessions if sid in documents}
    if distractor_cap > 0:
        for sid in documents:
            if sid in out:
                continue
            out[sid] = documents[sid]
            if len(out) >= len(gold_sessions) + distractor_cap:
                break
    return out


def build_live_adapters(
    documents: dict[str, str],
    *,
    want_fathomdb: bool = True,
    want_mem0: bool = True,
    want_graphiti: bool = True,
    db_path: str = "/tmp/d0b-fathomdb.sqlite",
) -> tuple[dict[str, Any], list[dict[str, str]]]:
    """Best-effort live adapter stand-up. Returns ``(adapters, blockers)``; a backend
    that cannot stand up is a recorded blocker, never a crash (Graphiti especially —
    2nd comparator, design §3). naive_rag is always available (pure-Python BM25)."""
    from eval.r2_parity_eval import NaiveRAGAdapter

    adapters: dict[str, Any] = {"naive_rag": NaiveRAGAdapter(documents)}
    blockers: list[dict[str, str]] = []

    if want_fathomdb:
        from eval.r2_parity_eval import _build_fathomdb

        fdb, blk = _build_fathomdb(documents, Path(db_path))
        if fdb is not None:
            adapters["fathomdb"] = fdb
        if blk is not None:
            blockers.append(blk)

    if want_mem0:
        from eval.r2_parity_eval import Mem0OSSAdapter

        mem0 = Mem0OSSAdapter.try_build()
        if mem0 is not None and mem0.available:
            try:
                mem0.ingest(documents)
                adapters["mem0_oss"] = mem0
            except Exception as exc:  # noqa: BLE001
                blockers.append({"id": "mem0-ingest-failed", "description": f"mem0.ingest failed: {exc}"})
        else:
            blockers.append(
                {
                    "id": "mem0-oss-unavailable",
                    "description": "mem0ai/backend not importable; install mem0ai+chromadb+sentence-transformers into the ISOLATED env",
                }
            )

    if want_graphiti:
        try:
            from eval.graphiti_local import build_graphiti_adapter  # type: ignore[import-not-found]

            g = build_graphiti_adapter(documents)
            if g is not None:
                adapters["graphiti_zep"] = g
            else:
                blockers.append({"id": "graphiti-zep-unavailable", "description": "graphiti adapter returned None"})
        except Exception as exc:  # noqa: BLE001 — Graphiti is the 2nd comparator (not a hard dep)
            blockers.append(
                {
                    "id": "graphiti-zep-unavailable",
                    "description": f"Graphiti/Zep local stand-up not available ({exc}); running the gap with the arms present",
                }
            )

    return adapters, blockers


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    import json

    from eval.m1_baseline_run import CostTrackingAnswerer

    ap = argparse.ArgumentParser(description="0.8.3 D0b parity runner (cheap-validate / pilot / full)")
    ap.add_argument("--mode", choices=["cheap", "pilot", "full"], required=True)
    ap.add_argument("--reader", default=None)
    ap.add_argument("--gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    ap.add_argument("--output", required=True)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--context-char-budget", type=int, default=None,
                    help="window-fit cap on total answerer context chars (priced-cost lever; "
                    "applied identically across arms)")
    ap.add_argument("--per-class", type=int, default=None, help="cheap/pilot per-class subset size")
    ap.add_argument("--distractor-cap", type=int, default=0)
    ap.add_argument("--max-usd", type=float, default=1.0)
    ap.add_argument("--checkpoint", default=None)
    ap.add_argument("--checkpoint-every", type=int, default=10)
    ap.add_argument("--resume", default=None)
    ap.add_argument("--no-fathomdb", action="store_true")
    ap.add_argument("--no-mem0", action="store_true")
    ap.add_argument("--no-graphiti", action="store_true")
    ap.add_argument("--blockers-out", default=None)
    args = ap.parse_args(argv)

    reader = args.reader or (CHEAP_READER_DEFAULT if args.mode == "cheap" else STRONG_READER_DEFAULT)
    _ch, _qv, queries = load_repin_gold(Path(args.gold))
    if args.mode in ("cheap", "pilot") and args.per_class:
        queries = _select_subset(queries, per_class=args.per_class, classes=MEMORY_CLASSES)

    documents = build_documents_from_lme(queries, distractor_cap=args.distractor_cap)
    print(f"[D0b][CLI] {len(queries)} queries, {len(documents)} sessions in corpus", flush=True)
    adapters, blockers = build_live_adapters(
        documents,
        want_fathomdb=not args.no_fathomdb,
        want_mem0=not args.no_mem0,
        want_graphiti=not args.no_graphiti,
    )
    print(f"[D0b][CLI] adapters: {sorted(adapters)} | blockers: {[b['id'] for b in blockers]}", flush=True)
    answerer = CostTrackingAnswerer(reader, timeout_s=240.0)
    if not answerer.available:
        raise SystemExit(f"[D0b][STOP] answerer {reader!r} unavailable — do not fake answers")

    art = run_d0b(
        mode=args.mode,
        reader=reader,
        output=Path(args.output),
        queries=queries,
        adapters=adapters,
        answerer=answerer,
        k=args.k,
        checkpoint_path=Path(args.checkpoint) if args.checkpoint else None,
        checkpoint_every=args.checkpoint_every,
        resume=Path(args.resume) if args.resume else None,
        max_usd=args.max_usd,
        context_char_budget=args.context_char_budget,
        on_progress=lambda i, n, usd: print(f"[D0b][{args.mode}] {i}/{n} ${usd:.4f}", flush=True)
        if (i == 1 or i % 5 == 0 or i == n)
        else None,
    )
    art["blockers_encountered"] = blockers
    Path(args.output).write_text(json.dumps(art, indent=2), encoding="utf-8")
    if args.blockers_out:
        Path(args.blockers_out).write_text(json.dumps(blockers, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
