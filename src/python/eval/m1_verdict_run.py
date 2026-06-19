"""M1 adjudication VERDICT runner (Slice 20, stage 1) — cheap-validate + priced 5-arm pass.

Budget discipline (design §8 / [[0.8.1-budget-discipline-cheap-validate-and-ledger]]):
**cheap-validate** the 5-arm pipeline with the flash-lite reader over a tiny subset
first; only then the **priced** 5-arm pass with the strong reader over the
graph-covered answerable questions, **hard-capped at ~$10** (HITL-authorized stage-1
budget on the current 299-graph). Reuses ``run_baseline`` /
``CostTrackingAnswerer`` and the frozen ``decide()`` (imported, never redefined).

The one priced seam is the shared answerer; retrieval / PPR / rerank / scoring are
$0. Every artifact is pinned to ``musique_hash`` and never overwrites a prior run.
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any, Optional

from eval.m1_baseline import (
    CHEAP_READER_DEFAULT,
    MUSIQUE_HASH,
    STRONG_READER_DEFAULT,
    BGEEncoder,
    FusedPoolReranker,
)
from eval.m1_baseline_run import CostTrackingAnswerer
from eval.m1_verdict import (
    VERDICT_ARMS,
    load_graph_questions,
    ppr_divergence,
    prior_answers_from_artifact,
    run_verdict,
)

#: HARD stage-1 budget ceiling (USD). The HITL authorization is "~$10 on the
#: current 299-graph". The pre-flight projection must clear this with the
#: pilot-measured per-call cost before any priced call.
HARD_CAP_USD = 10.0

#: Pilot-measured strong-reader per (question×arm) call cost (runs/0.8.2-m1-baseline-pilot.json).
PILOT_PER_CALL_USD = 0.00646

#: Minimum answer-matrix completeness for a priced pass to be a citable verdict.
#: Below this, too many calls degraded to abstention (endpoint outage / rate-limit)
#: and the endpoint is deflated/biased — flag INVALID, do not report a verdict.
VALID_COMPLETENESS_FLOOR = 0.97

_DEFAULT_CORPUS = (
    Path(__file__).resolve().parents[3] / "data" / "corpus-data" / "raw" / "musique_dev.jsonl"
)
_DEFAULT_EXTRACTIONS = (
    Path(__file__).resolve().parents[3]
    / "data" / "corpus-data" / "graph-cache" / "0.8.2-m1-v1" / "extractions.json"
)


def _load_extractions(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def run(
    *,
    mode: str,
    reader: str,
    corpus: Path,
    extractions_path: Path,
    output: Path,
    k: int = 10,
    n_boot: int = 2000,
    seed: int = 0,
    answer_workers: int = 4,
    cheap_limit: int = 15,
    max_usd: float = HARD_CAP_USD,
    resume: Optional[Path] = None,
) -> dict[str, Any]:
    t0 = time.time()
    extractions = _load_extractions(extractions_path)
    questions = load_graph_questions(corpus, extractions)

    # Resume seam: reuse already-successful (qid, arm) answers from a prior artifact,
    # re-calling ONLY the previously-failed cells (deterministic answerer ⇒ identical
    # to a clean pass, but $0 for the cells that already succeeded).
    prior_answers = None
    if resume is not None:
        prior = json.loads(Path(resume).read_text(encoding="utf-8"))
        prior_answers = prior_answers_from_artifact(prior)
        n_reusable = sum(1 for v in prior_answers.values() if v is not None)
        print(
            f"[S20][RESUME] {n_reusable} prior non-None (qid,arm) cells reused from "
            f"{resume}; only failed/missing cells will be (re)called",
            flush=True,
        )
        if n_reusable == 0:
            print(
                "[S20][RESUME] WARNING: prior artifact persisted 0 answers "
                "(baseline_run absent) → this is a FULL re-run, not a fill-in",
                flush=True,
            )
    from collections import Counter

    hop_dist = dict(sorted(Counter(q.hop_count for q in questions).items()))
    print(
        f"[S20][LOAD] {len(questions)} graph-covered answerable Q "
        f"(hop dist {hop_dist}); musique_hash OK",
        flush=True,
    )

    # ----- $0 sanity guard: ppr_fusion must NOT be silently identical to BM25 ----
    div = ppr_divergence(questions, extractions, k=k)
    print(
        f"[S20][GUARD] ppr_fusion≠bm25 on {div['n_ppr_differs_from_bm25_topk']}/"
        f"{div['n_questions']} questions (fraction {div['fraction_differs']})",
        flush=True,
    )
    if div["silently_identical_to_bm25"]:
        raise SystemExit(
            "[S20][STOP] ppr_fusion is SILENTLY IDENTICAL to bm25 on every question "
            "— the comparison would be vacuous. Aborting before any priced call."
        )

    if mode == "cheap":
        # hop-stratified so the ≥3-hop endpoint has data (ids sort 2hop<3hop<4hop).
        per_hop = max(cheap_limit // 3, 2)
        sub: list[Any] = []
        for hop in (2, 3, 4):
            sub += [q for q in questions if q.hop_count == hop][:per_hop]
        questions = sub
        print(
            f"[S20][CHEAP] cheap-validate subset = {len(questions)} Q "
            f"(hop-stratified ~{per_hop}/hop; reader={reader})",
            flush=True,
        )
    else:
        # Pre-flight projection gate against the hard cap.
        projected = round(PILOT_PER_CALL_USD * len(questions) * len(VERDICT_ARMS), 2)
        print(
            f"[S20][PRICED] projected ≈ ${projected} "
            f"({len(questions)} Q × {len(VERDICT_ARMS)} arms × ${PILOT_PER_CALL_USD}/call); "
            f"cap=${max_usd}",
            flush=True,
        )
        if projected > max_usd + 1.0:  # 10% tolerance over the ~$10 soft target
            raise SystemExit(
                f"[S20][STOP] projected ${projected} exceeds the hard cap ${max_usd} (+$1 tol) "
                "— refusing the priced pass."
            )

    answerer = CostTrackingAnswerer(reader, timeout_s=240.0)
    if not answerer.available:
        raise SystemExit(
            f"[S20][STOP] answerer endpoint unreachable / reader {reader!r} unavailable "
            f"(base_url={answerer.base_url}) — do NOT fake answers"
        )

    encoder = BGEEncoder()
    reranker = FusedPoolReranker()

    def progress(done: int, total: int, _qr: Any) -> None:
        if done == 1 or done % 10 == 0 or done == total:
            print(
                f"[S20][{mode.upper()}] {done}/{total} calls={answerer.n_calls} "
                f"${answerer.usd():.4f} ({round(time.time() - t0, 1)}s)",
                flush=True,
            )

    art = run_verdict(
        questions,
        answerer,
        extractions,
        k=k,
        encoder=encoder,
        reranker=reranker,
        n_boot=n_boot,
        seed=seed,
        progress=progress,
        answer_workers=answer_workers,
        power_ok=False,  # stage 1: N≈144 ≪ 1165 required
        prior_answers=prior_answers,
    )

    art["cost"] = answerer.cost_block()
    art["ppr_divergence"] = div
    art["mode"] = mode
    art["reader_model"] = reader
    art["reader_model_mapping_note"] = (
        "slice prompt named gemini-3.1-pro-preview (strong) / gemini-2.5-flash-lite "
        f"(cheap); the airlock proxy serves neither exact id — closest available used: "
        f"strong={STRONG_READER_DEFAULT}, cheap={CHEAP_READER_DEFAULT}"
    )
    art["answer_workers"] = answer_workers
    art["k"] = k
    art["elapsed_s"] = round(time.time() - t0, 1)
    art["musique_hash"] = MUSIQUE_HASH

    # ---- answer-completeness validity guard ([[background-exit-masks-real-exit]]) ----
    # A corrupted priced pass (endpoint outage / rate-limit mid-run) degrades failed
    # calls to abstention (None ⇒ F1=0), which silently DEFLATES every arm and biases
    # the endpoint toward 0. Flag the run INVALID when the answer matrix is materially
    # incomplete — never present an underpopulated endpoint as a verdict.
    expected = len(questions) * len(VERDICT_ARMS)
    n_errors = int(art["cost"].get("n_errors", 0))
    completeness = round(1.0 - n_errors / max(expected, 1), 4)
    run_valid = completeness >= VALID_COMPLETENESS_FLOOR
    art["answer_completeness"] = {
        "expected_calls": expected,
        "n_errors": n_errors,
        "completeness": completeness,
        "floor": VALID_COMPLETENESS_FLOOR,
        "run_valid": run_valid,
    }
    art["run_valid"] = run_valid
    if not run_valid:
        art["INVALID"] = (
            f"INVALID priced pass — answer completeness {completeness} < "
            f"{VALID_COMPLETENESS_FLOOR}: {n_errors}/{expected} answerer calls FAILED "
            "(endpoint outage / rate-limit mid-run), degrading those (question,arm) cells "
            "to spurious abstentions. The endpoint numbers below are NOT a citable verdict "
            "— a clean re-run on a stable endpoint is required."
        )
        print(f"[S20][INVALID] {art['INVALID']}", flush=True)

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(
        f"[S20][{mode.upper()}] wrote {output} | cost ${answerer.usd():.4f} "
        f"({answerer.n_calls} calls, {answerer.n_errors} errors)",
        flush=True,
    )
    return art


# --------------------------------------------------------------------------- #
# Report writer
# --------------------------------------------------------------------------- #


def _fmt(x: Optional[float], nd: int = 4) -> str:
    return "—" if x is None else f"{x:.{nd}f}"


def write_report(art: dict[str, Any], path: Path, *, cheap_art: Optional[dict[str, Any]] = None) -> None:
    """Render the human-readable verdict report (the Slice-20 deliverable)."""
    ep = art["primary_endpoint"]
    pooled = ep["pooled_ge3hop"]
    five = art["five_arm_pooled_ge3hop"]
    cost = art.get("cost", {})
    stage2 = art["stage2_recommendation"]
    div = art.get("ppr_divergence", {})

    lines: list[str] = []
    lines.append("# 0.8.2 / M1 — adjudication verdict report (Slice 20, stage 1)")
    lines.append("")
    if not art.get("run_valid", True):
        comp = art.get("answer_completeness", {})
        lines.append(
            "> ## ⛔ INVALID PRICED PASS — NOT A CITABLE VERDICT\n"
            f"> The priced answerer pass is **corrupted**: only "
            f"**{comp.get('completeness')}** of the answer matrix completed "
            f"({comp.get('n_errors')}/{comp.get('expected_calls')} calls FAILED). The airlock "
            "endpoint **rate-limited (HTTP 429)** mid-run under concurrent load; every failed "
            "call degraded to an abstention (None ⇒ F1=0), which DEFLATES every arm and biases "
            "the ΔF1 endpoint toward 0. The failures clustered in the late 3-hop/4-hop questions "
            "— i.e. **inside the primary ≥3-hop cell**. **The numbers below are diagnostic only; "
            "do NOT cite them as the M1 verdict.** A clean re-run on a quota-recovered endpoint "
            "with low concurrency (workers ≤4) + the answer-completeness guard is required."
        )
        lines.append("")
    lines.append("")
    lines.append(
        f"> **Stage 1** — HITL-authorized ~$10 run on the current **{art.get('n_questions')}**-question "
        "graph (answerable only; **no unanswerable contrast set** → the confident-wrong guard is "
        "UNEVALUATED). Comparator = **fused-RRF (k=60)** (design AMENDED 2026-06-19). The graph arm = "
        "**ppr-fusion** (lexically-seeded Personalized PageRank fused with BM25)."
    )
    lines.append("")
    lines.append(f"- `musique_hash` = `{art.get('musique_hash')}`")
    lines.append(f"- reader = `{art.get('reader_model')}` · arms = {list(VERDICT_ARMS)}")
    lines.append(
        f"- ppr-fusion ≠ bm25 on **{div.get('n_ppr_differs_from_bm25_topk')}/"
        f"{div.get('n_questions')}** questions (fraction {div.get('fraction_differs')}) "
        "→ the graph arm is materially distinct from BM25 (not vacuous)."
    )
    lines.append("")

    # ---- 5-arm pooled ≥3-hop table ----
    lines.append("## 1. Five-arm pooled ≥3-hop F1 (the 144 answerable 3+4-hop questions)")
    lines.append("")
    lines.append("| arm | F1 | EM | n |")
    lines.append("|---|---|---|---|")
    for arm in VERDICT_ARMS:
        c = five.get(arm, {})
        marker = ""
        if arm == ep["comparator_arm"]:
            marker = " *(comparator)*"
        if arm == ep["treatment_arm"]:
            marker = " *(graph arm)*"
        lines.append(f"| `{arm}`{marker} | {_fmt(c.get('f1'))} | {_fmt(c.get('em'))} | {c.get('n')} |")
    lines.append("")

    # ---- the load-bearing ΔF1 ----
    lines.append("## 2. Primary endpoint — ΔF1 (ppr-fusion − fused-RRF), pooled ≥3-hop")
    lines.append("")
    lines.append(
        f"**ΔF1 = {_fmt(pooled['f1_delta'])}** "
        f"(paired-bootstrap 95% CI [{_fmt(pooled['f1_ci_low'])}, {_fmt(pooled['f1_ci_high'])}], "
        f"n_boot={ep['n_boot']}, n={pooled['n']})."
    )
    lines.append("")
    lines.append(
        f"ΔEM = {_fmt(pooled['em_delta'])} (CI [{_fmt(pooled['em_ci_low'])}, "
        f"{_fmt(pooled['em_ci_high'])}])."
    )
    lines.append("")

    # ---- per-hop + trend ----
    lines.append("## 3. Per-hop ΔF1 (secondary) + trend")
    lines.append("")
    lines.append("| hop | n | ΔF1 | CI low | CI high | ΔEM |")
    lines.append("|---|---|---|---|---|---|")
    for hop in ("2", "3", "4"):
        h = ep["per_hop"][hop]
        lines.append(
            f"| {hop} | {h['n']} | {_fmt(h['f1_delta'])} | {_fmt(h['f1_ci_low'])} | "
            f"{_fmt(h['f1_ci_high'])} | {_fmt(h['em_delta'])} |"
        )
    tr = ep["trend"]
    lines.append("")
    lines.append(
        f"ΔF1-vs-hop OLS slope = {_fmt(tr['slope'])}; **significantly negative? "
        f"{tr['neg_significant']}** (the trend gate vetoes only on a significantly negative slope)."
    )
    lines.append("")

    # ---- verdict ----
    lines.append("## 4. Verdict (mechanical, from the imported frozen `decide()`)")
    lines.append("")
    lines.append(f"```\ndecide_inputs = {json.dumps(art['decide_inputs'], indent=2)}\n```")
    lines.append("")
    lines.append(f"**`decide()` = {art['verdict']}** — via the **power gate** ({art['power_status']}).")
    lines.append("")
    lines.append(f"> {art['decision_rule_note']}")
    lines.append("")
    lines.append(f"> Confident-wrong guard: {art['confident_wrong_status']}")
    lines.append("")

    # ---- budget ----
    lines.append("## 5. Budget ($ ledger)")
    lines.append("")
    if cheap_art is not None:
        cc = cheap_art.get("cost", {})
        lines.append(
            f"- cheap-validate ({cheap_art.get('reader_model')}): "
            f"{cc.get('n_calls')} calls, ${cc.get('usd')}"
        )
    lines.append(
        f"- priced 5-arm pass (`{cost.get('model')}`): {cost.get('n_calls')} calls "
        f"({cost.get('n_errors')} errors), prompt={cost.get('prompt_tokens')} tok, "
        f"completion={cost.get('completion_tokens')} tok → **${cost.get('usd')}**"
    )
    cheap_usd = (cheap_art or {}).get("cost", {}).get("usd", 0.0) or 0.0
    total = round(float(cost.get("usd", 0.0)) + float(cheap_usd), 4)
    lines.append(f"- **cumulative this slice: ${total}** (cap ~$10)")
    lines.append("")

    # ---- stage-2 ----
    lines.append("## 6. Stage-2 recommendation (from the effect size, not `decide()`)")
    lines.append("")
    lines.append(f"**{stage2['recommendation']}** (run_stage2 = {stage2['run_stage2']}).")
    lines.append("")
    lines.append(stage2["rationale"])
    lines.append("")
    lines.append(
        "> `decide()` is formally NO_GO via the power gate (N≈144 ≪ 1165). The scientific read "
        "is the ΔF1 effect size + CI above and this stage-2 call."
    )
    lines.append("")

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")
    print(f"[S20][REPORT] wrote {path}", flush=True)


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="M1 Slice-20 verdict runner (cheap-validate / priced)")
    ap.add_argument("--mode", choices=["cheap", "priced"], required=True)
    ap.add_argument("--reader", default=None)
    ap.add_argument("--corpus", default=None)
    ap.add_argument("--extractions", default=None)
    ap.add_argument("--output", required=True)
    ap.add_argument("--report", default=None, help="write the report MD (priced mode)")
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--n-boot", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--workers", type=int, default=4,
                    help="concurrent answerer calls; keep LOW (≤4) to avoid 429 rate-limits")
    ap.add_argument("--cheap-limit", type=int, default=15)
    ap.add_argument("--max-usd", type=float, default=HARD_CAP_USD)
    ap.add_argument("--resume", default=None,
                    help="prior verdict artifact JSON: reuse its successful (qid,arm) "
                    "answers and (re)call ONLY the previously-failed cells")
    args = ap.parse_args(argv)

    corpus = Path(args.corpus) if args.corpus else _DEFAULT_CORPUS
    extractions_path = Path(args.extractions) if args.extractions else _DEFAULT_EXTRACTIONS
    reader = args.reader or (CHEAP_READER_DEFAULT if args.mode == "cheap" else STRONG_READER_DEFAULT)

    art = run(
        mode=args.mode,
        reader=reader,
        corpus=corpus,
        extractions_path=extractions_path,
        output=Path(args.output),
        k=args.k,
        n_boot=args.n_boot,
        seed=args.seed,
        answer_workers=args.workers,
        cheap_limit=args.cheap_limit,
        max_usd=args.max_usd,
        resume=Path(args.resume) if args.resume else None,
    )
    if args.report and args.mode == "priced":
        write_report(art, Path(args.report))
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
