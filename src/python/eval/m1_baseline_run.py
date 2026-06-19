"""M1 strong-baseline runner (Slice 5) — cheap-validate + bounded priced pilot.

Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]] / design
§8): cheap-validate the full 4-arm pipeline with the flash-lite reader first,
then a bounded priced pilot (HARD CAP N ≤ 100) with the strong reader to measure
the pooled ≥3-hop baseline F1/EM **variance**, which sizes the whole-rule power
simulation. **This runner never runs the full priced pass** — the orchestrator
brings the projection to HITL.

Every answerer call's token usage is captured so the $ is `tokens × price`, not a
guess. The answerer LLM is the one priced seam; retrieval/rerank/scoring is $0.
"""

from __future__ import annotations

import argparse
import json
import random
import threading
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Any, Optional

import numpy as np

from eval.m1_baseline import (
    ARM_NAMES,
    CHEAP_READER_DEFAULT,
    COMPARATOR_ARM,
    MUSIQUE_HASH,
    STRONG_READER_DEFAULT,
    BGEEncoder,
    FusedPoolReranker,
    Question,
    load_musique,
    retrieval_recall,
    run_baseline,
)
from eval.m1_power_sim import (
    FLAT_POSITIVE_LIFT,
    propose_material_f1_lift,
    required_n,
    simulate_p_go,
)
from eval.p0a_base_retrieval import AirlockAnswerer

#: Documented price assumptions ($ per 1M tokens). The proxy does not return a
#: price, so the projection applies this table to the *measured* token counts; it
#: is recorded in the artifact and is the HITL-auditable lever. (gemini-3.1-pro ≈
#: the 2.5-pro tier; flash-lite ≈ free-tier-cheap.)
PRICE_PER_1M: dict[str, tuple[float, float]] = {
    # model_id: (input_usd_per_1M, output_usd_per_1M)
    "gemini-3.1-pro": (1.25, 5.00),
    "gemini-3.1-flash-lite": (0.05, 0.20),
    "gemini-flash-lite": (0.05, 0.20),
}
_DEFAULT_PRICE = (1.25, 5.00)


class CostTrackingAnswerer(AirlockAnswerer):
    """:class:`AirlockAnswerer` that accumulates token usage + call counts."""

    def __init__(self, model_id: str, *, timeout_s: float = 120.0) -> None:
        super().__init__(model_id, timeout_s=timeout_s)
        self.n_calls = 0
        self.n_errors = 0
        self.prompt_tokens = 0
        self.completion_tokens = 0
        self._lock = threading.Lock()

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        import urllib.request

        from eval.r2_parity_eval import normalize_answer

        payload = json.dumps(
            {
                "model": self.model_id,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0,
                "seed": 0,
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            self.base_url.rstrip("/") + "/chat/completions",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        with urllib.request.urlopen(req, timeout=self._timeout) as resp:  # noqa: S310
            body = json.loads(resp.read().decode("utf-8"))
        usage = body.get("usage") or {}
        with self._lock:
            self.prompt_tokens += int(usage.get("prompt_tokens", 0))
            self.completion_tokens += int(usage.get("completion_tokens", 0))
            self.n_calls += 1
        return normalize_answer(body["choices"][0]["message"]["content"])

    def usd(self) -> float:
        pin, pout = PRICE_PER_1M.get(self.model_id, _DEFAULT_PRICE)
        return round(self.prompt_tokens / 1e6 * pin + self.completion_tokens / 1e6 * pout, 4)

    def cost_block(self) -> dict[str, Any]:
        pin, pout = PRICE_PER_1M.get(self.model_id, _DEFAULT_PRICE)
        return {
            "model": self.model_id,
            "n_calls": self.n_calls,
            "n_errors": self.n_errors,
            "prompt_tokens": self.prompt_tokens,
            "completion_tokens": self.completion_tokens,
            "price_per_1m_input_usd": pin,
            "price_per_1m_output_usd": pout,
            "usd": self.usd(),
        }


#: The available MuSiQue-Ans answerable pool per hop (the hard ceiling on any
#: feasible cell). From the pinned corpus: 760 (3-hop) + 405 (4-hop) answerable.
CORPUS_ANSWERABLE_GE3HOP = 1165
CORPUS_ANSWERABLE_TOTAL = 2417

#: Power-sim grid extended high enough to locate a required-N even at the wide
#: measured baseline variance (per-question F1 is near-binary ⇒ sd≈0.45).
_POWER_GRID = (50, 100, 150, 200, 300, 400, 600, 800, 1200, 1600, 2000, 3000, 5000, 8000)


def build_power_block(paired_records: list[dict[str, Any]], cost: dict[str, Any]) -> dict[str, Any]:
    """Whole-rule power sim + the full-pass $ projection, from a run's records.

    Pure post-processing over the measured pooled ≥3-hop comparator F1/EM — **no
    priced call** — so it can be re-derived from a saved artifact (the model was
    corrected after the priced pilot; this lets the artifact be regenerated for
    free). Returns P(GO) at the pilot N for each effect shape, the required N for
    P(GO) ≥ 0.8 under flat-positive (+ a rho-sensitivity sweep), an on-corpus
    feasibility verdict, and the projected $ for the full baseline pass.
    """
    ge3 = [r for r in paired_records if r["answerable"] and r["hop_count"] >= 3]
    base_f1 = [r["f1"][COMPARATOR_ARM] for r in ge3 if COMPARATOR_ARM in r["f1"]]
    base_em = [r["em"][COMPARATOR_ARM] for r in ge3 if COMPARATOR_ARM in r["em"]]
    hops = [r["hop_count"] for r in ge3 if COMPARATOR_ARM in r["f1"]]
    if len(base_f1) < 5:
        return {"note": "insufficient ≥3-hop answerable sample for power sim"}

    pilot_n = len(base_f1)
    shape_p_go = {
        shape: simulate_p_go(base_f1, base_em, hops, shape=shape, n=pilot_n,
                             n_trials=400, n_boot=600, seed=0)
        for shape in ("flat_positive", "monotonic", "inverted_u")
    }
    req = required_n(base_f1, base_em, hops, shape="flat_positive", target=0.8,
                     grid=_POWER_GRID, n_trials=400, n_boot=600, seed=0)
    rho_sweep = {
        f"rho_{rho}": required_n(base_f1, base_em, hops, shape="flat_positive",
                                 target=0.8, rho=rho, grid=_POWER_GRID,
                                 n_trials=400, n_boot=600, seed=0)["required_n"]
        for rho in (0.3, 0.5, 0.7)
    }

    # Analytic cross-check (normal-approx, paired CI-excludes-0 at ~80% power):
    # required_N ≈ (sd_pair * (z_0.975 + z_0.80) / lift)^2 — a sanity anchor for
    # the bootstrap required_n above (both must say "≫ corpus" for a +0.03 effect).
    import math as _math

    sd_f1 = float(np.std(np.asarray(base_f1, dtype=float), ddof=1)) if pilot_n > 1 else 0.0
    sd_pair = sd_f1 * _math.sqrt(2 * (1 - 0.5))
    analytic_n = int(_math.ceil((sd_pair * (1.96 + 0.84) / FLAT_POSITIVE_LIFT) ** 2)) if sd_pair else None

    # MDE + proposed MATERIAL_F1_LIFT at the FULL feasible ≥3-hop corpus (N=1165),
    # paired, power 0.8, reported across ρ∈{0.5,0.7}. PROPOSED ONLY — decide()/
    # MATERIAL_F1_LIFT untouched; HITL confirms the value at the next gate.
    proposed_mde = propose_material_f1_lift(sd_f1, CORPUS_ANSWERABLE_GE3HOP, rhos=(0.5, 0.7))

    # $ projection: per (question×arm) call cost from the pilot.
    n_calls = max(int(cost.get("n_calls", 0)), 1)
    per_call_usd = round(float(cost.get("usd", 0.0)) / n_calls, 6)
    n_arms = len(ARM_NAMES)
    req_n = req["required_n"]
    feasible_on_corpus = req_n is not None and req_n <= CORPUS_ANSWERABLE_GE3HOP
    max_feasible_n = CORPUS_ANSWERABLE_GE3HOP
    p_go_at_full_corpus = next(
        (c["p_go"] for c in req["curve"] if c["n"] >= max_feasible_n), req["curve"][-1]["p_go"]
    )

    def proj(n: int) -> float:
        return round(per_call_usd * n * n_arms, 2)

    return {
        "measured_pilot_cell_n": pilot_n,
        "measured_comparator_mean_f1": round(sum(base_f1) / pilot_n, 4),
        "sd_baseline_f1": shape_p_go["flat_positive"]["sd_baseline_f1"],
        "p_go_at_pilot_n_by_shape": shape_p_go,
        "required_n_flat_positive": req,
        "analytic_required_n_normal_approx_rho0.5": analytic_n,
        "rho_sensitivity_required_n": rho_sweep,
        "mde_at_full_corpus_n1165": proposed_mde,
        "corpus_answerable_ge3hop": CORPUS_ANSWERABLE_GE3HOP,
        "feasible_on_corpus": feasible_on_corpus,
        "p_go_if_whole_ge3hop_corpus_run": p_go_at_full_corpus,
        "projection": {
            "per_call_usd": per_call_usd,
            "n_baseline_arms": n_arms,
            "projected_full_pass_usd_at_required_n": (proj(req_n) if req_n else None),
            "projected_usd_whole_ge3hop_corpus": proj(CORPUS_ANSWERABLE_GE3HOP),
            "projected_usd_whole_answerable_corpus": proj(CORPUS_ANSWERABLE_TOTAL),
            "note": (
                "projected_full_pass_usd = required_N x #baseline_arms x per_call_usd. "
                "If required_N > corpus_answerable_ge3hop the power target is "
                "UNREACHABLE on this corpus for the +0.03 effect — HITL decision: "
                "raise the detectable effect size, accept the whole-corpus run at the "
                "stated sub-0.8 P(GO), or redirect (design §0 reserved follow-on 1-4)."
            ),
        },
    }


def select_sample(
    questions: Sequence[Question],
    *,
    n_ge3_answerable: int,
    n_2hop_answerable: int,
    n_unanswerable_ge3: int,
    seed: int = 0,
) -> list[Question]:
    """Deterministic, hop-stratified sample. The pooled ≥3-hop answerable cell is
    sized to ``n_ge3_answerable`` (split 3/4-hop ~ the corpus ratio) — that cell
    is the power-sim variance source; the 2-hop + unanswerable sets are context."""
    rng = random.Random(seed)

    def pick(pred: Any, n: int) -> list[Question]:
        pool = sorted([q for q in questions if pred(q)], key=lambda q: q.id)
        return pool if len(pool) <= n else rng.sample(pool, n)

    n3 = round(n_ge3_answerable * 0.65)
    n4 = n_ge3_answerable - n3
    sample: list[Question] = []
    sample += pick(lambda q: q.answerable and q.hop_count == 3, n3)
    sample += pick(lambda q: q.answerable and q.hop_count == 4, n4)
    sample += pick(lambda q: q.answerable and q.hop_count == 2, n_2hop_answerable)
    sample += pick(lambda q: (not q.answerable) and q.hop_count >= 3, n_unanswerable_ge3)
    sample.sort(key=lambda q: q.id)
    return sample


def recall_sample(questions: Sequence[Question], *, n: int, seed: int = 0) -> list[Question]:
    """Deterministic ≥3-hop answerable sample for the $0 recall comparison
    (decoupled from the priced cap; sized only by CPU budget)."""
    pool = sorted([q for q in questions if q.answerable and q.hop_count >= 3], key=lambda q: q.id)
    if len(pool) <= n:
        return pool
    return random.Random(seed).sample(pool, n)


def run(
    corpus: Path,
    *,
    mode: str,
    reader: str,
    k: int,
    n_ge3: int,
    n_2hop: int,
    n_unans: int,
    seed: int,
    output: Path,
    answer_workers: int = 8,
    n_recall: int = 0,
) -> dict[str, Any]:
    t0 = time.time()
    questions = load_musique(corpus)
    sample = select_sample(
        questions,
        n_ge3_answerable=n_ge3,
        n_2hop_answerable=n_2hop,
        n_unanswerable_ge3=n_unans,
        seed=seed,
    )
    # 240 s per call: the strong reader is a reasoning model; some hard multi-hop
    # questions exceed the 120 s default. A still-too-slow call degrades to an
    # abstention (run_baseline catches it) rather than crashing the priced pass.
    answerer = CostTrackingAnswerer(reader, timeout_s=240.0)
    if not answerer.available:
        raise SystemExit(
            f"answerer endpoint unreachable / reader {reader!r} unavailable "
            f"(base_url={answerer.base_url}); STOP — do not fake answers"
        )
    encoder = BGEEncoder()
    reranker = FusedPoolReranker()

    n_done = 0

    def progress(done: int, total: int, _qr: Any) -> None:
        nonlocal n_done
        n_done = done
        if done == 1 or done % 10 == 0 or done == total:
            print(
                f"[S5][{mode.upper()}] {done}/{total} "
                f"calls={answerer.n_calls} ${answerer.usd():.4f} "
                f"({round(time.time() - t0, 1)}s)",
                flush=True,
            )

    art = run_baseline(sample, answerer, k=k, encoder=encoder, reranker=reranker,
                        progress=progress, answer_workers=answer_workers)

    art["cost"] = answerer.cost_block()
    art["power_sim"] = build_power_block(art["paired_records"], art["cost"])

    # $0 retrieval-recall comparison (gold supporting-passage recall@K per arm) —
    # the cheaper, lower-variance signal for whether the CE rerank helps/hurts
    # bridge-passage retrieval. No LLM; CPU-only over a ≥3-hop answerable sample.
    if n_recall > 0:
        rsample = recall_sample(questions, n=n_recall, seed=seed)
        print(f"[S5][{mode.upper()}] recall pass over {len(rsample)} ≥3-hop answerable Q ($0) ...",
              flush=True)
        art["retrieval_recall"] = retrieval_recall(rsample, encoder, reranker, ks=(1, 2, 3, 5, 10))
    art["mode"] = mode
    art["reader_model"] = reader
    art["reader_model_mapping_note"] = (
        "slice prompt named gemini-3.1-pro-preview (strong) / gemini-2.5-flash-lite "
        "(cheap); the local airlock proxy serves neither exact id, so the closest "
        f"available id is used: strong={STRONG_READER_DEFAULT}, cheap={CHEAP_READER_DEFAULT}"
    )
    art["answer_workers"] = answer_workers
    art["sample_selection"] = {
        "n_ge3_answerable": n_ge3, "n_2hop_answerable": n_2hop,
        "n_unanswerable_ge3": n_unans, "seed": seed, "n_total": len(sample),
        "no_silent_cap": "sampling logged; truncated run labelled, not full coverage",
    }
    art["elapsed_s"] = round(time.time() - t0, 1)
    art["musique_hash"] = MUSIQUE_HASH

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(f"[S5][{mode.upper()}] wrote {output} | cost ${answerer.usd():.4f} "
          f"({answerer.n_calls} calls)")
    return art


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="M1 strong-baseline cheap-validate / pilot runner")
    ap.add_argument("--mode", choices=["cheap", "pilot"], default=None)
    ap.add_argument(
        "--recompute",
        default=None,
        help="path to an existing artifact JSON: re-derive the power_sim block "
        "(corrected model) in place — NO priced call",
    )
    ap.add_argument(
        "--recompute-recall",
        default=None,
        dest="recompute_recall",
        help="path to an existing artifact JSON: re-run the $0 retrieval recall "
        "and add all_bridges_present_at_k alongside recall_at_k — NO priced call; "
        "uses the same n/seed as the stored retrieval_recall block",
    )
    ap.add_argument("--reader", default=None, help="answerer model id (defaults per mode)")
    ap.add_argument("--corpus", default=None)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--n-ge3", type=int, default=None)
    ap.add_argument("--n-2hop", type=int, default=None)
    ap.add_argument("--n-unans", type=int, default=None)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--workers", type=int, default=8, help="concurrent answerer calls")
    ap.add_argument("--recall-n", type=int, default=0,
                    help="$0 retrieval-recall comparison over N ≥3-hop answerable Q (CPU-only)")
    ap.add_argument("--output", default=None)
    args = ap.parse_args(argv)

    # Recompute-only path: re-derive the power block on a saved artifact, $0.
    if args.recompute:
        path = Path(args.recompute)
        art = json.loads(path.read_text(encoding="utf-8"))
        art["power_sim"] = build_power_block(art["paired_records"], art["cost"])
        out = Path(args.output) if args.output else path
        out.write_text(json.dumps(art, indent=2), encoding="utf-8")
        print(f"[S5][RECOMPUTE] re-derived power_sim → {out} (no priced call)")
        return 0

    # Recompute-recall path: re-run the $0 retrieval recall (+ all_bridges@K) on a saved artifact.
    # Uses the same recall_sample(n, seed) that produced the stored retrieval_recall block.
    # No LLM calls; requires the corpus + BGE encoder cache.
    if args.recompute_recall:
        path = Path(args.recompute_recall)
        art = json.loads(path.read_text(encoding="utf-8"))
        corpus_path = Path(args.corpus) if args.corpus else (
            Path(__file__).resolve().parents[3] / "data" / "corpus-data" / "raw" / "musique_dev.jsonl"
        )
        questions = load_musique(corpus_path)
        stored_recall = art.get("retrieval_recall") or {}
        n = int(stored_recall.get("n_questions_with_gold", 150))
        seed = int((art.get("sample_selection") or {}).get("seed", 0))
        rsample = recall_sample(questions, n=n, seed=seed)
        encoder = BGEEncoder()
        reranker = FusedPoolReranker()
        print(f"[S5][RECOMPUTE-RECALL] re-running $0 retrieval over {len(rsample)} Q "
              f"(n={n}, seed={seed}) — computing recall@K + all_bridges@K ...", flush=True)
        art["retrieval_recall"] = retrieval_recall(rsample, encoder, reranker, ks=(1, 2, 3, 5, 10))
        out = Path(args.output) if args.output else path
        out.write_text(json.dumps(art, indent=2), encoding="utf-8")
        print(f"[S5][RECOMPUTE-RECALL] updated retrieval_recall + all_bridges_present_at_k → {out} ($0)")
        return 0

    if args.mode is None:
        raise SystemExit("--mode is required unless --recompute or --recompute-recall is given")
    if not args.output:
        raise SystemExit("--output is required")

    corpus = Path(args.corpus) if args.corpus else (
        Path(__file__).resolve().parents[3] / "data" / "corpus-data" / "raw" / "musique_dev.jsonl"
    )
    if args.mode == "cheap":
        reader = args.reader or CHEAP_READER_DEFAULT
        n_ge3 = args.n_ge3 if args.n_ge3 is not None else 24
        n_2hop = args.n_2hop if args.n_2hop is not None else 8
        n_unans = args.n_unans if args.n_unans is not None else 8
    else:  # pilot — HARD CAP: total ≤ 100
        reader = args.reader or STRONG_READER_DEFAULT
        n_ge3 = args.n_ge3 if args.n_ge3 is not None else 60
        n_2hop = args.n_2hop if args.n_2hop is not None else 20
        n_unans = args.n_unans if args.n_unans is not None else 20

    total = n_ge3 + n_2hop + n_unans
    if total > 100:
        raise SystemExit(f"sample total {total} exceeds the pilot HARD CAP of 100 — refusing")

    run(
        corpus, mode=args.mode, reader=reader, k=args.k,
        n_ge3=n_ge3, n_2hop=n_2hop, n_unans=n_unans, seed=args.seed,
        output=Path(args.output), answer_workers=args.workers, n_recall=args.recall_n,
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
