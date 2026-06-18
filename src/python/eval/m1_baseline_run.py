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

from eval.m1_baseline import (
    CHEAP_READER_DEFAULT,
    COMPARATOR_ARM,
    MUSIQUE_HASH,
    STRONG_READER_DEFAULT,
    BGEEncoder,
    EngineReranker,
    Question,
    load_musique,
    run_baseline,
)
from eval.m1_power_sim import required_n, simulate_p_go
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
    answerer = CostTrackingAnswerer(reader)
    if not answerer.available:
        raise SystemExit(
            f"answerer endpoint unreachable / reader {reader!r} unavailable "
            f"(base_url={answerer.base_url}); STOP — do not fake answers"
        )
    encoder = BGEEncoder()
    reranker = EngineReranker()

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

    # Power simulation from the measured pooled ≥3-hop comparator variance.
    ge3 = [r for r in art["paired_records"] if r["answerable"] and r["hop_count"] >= 3]
    base_f1 = [r["f1"][COMPARATOR_ARM] for r in ge3 if COMPARATOR_ARM in r["f1"]]
    base_em = [r["em"][COMPARATOR_ARM] for r in ge3 if COMPARATOR_ARM in r["em"]]
    hops = [r["hop_count"] for r in ge3 if COMPARATOR_ARM in r["f1"]]

    power: dict[str, Any] = {"note": "insufficient ≥3-hop answerable sample for power sim"}
    if len(base_f1) >= 5:
        shape_p_go = {
            shape: simulate_p_go(
                base_f1, base_em, hops, shape=shape, n=len(base_f1),
                n_trials=400, n_boot=600, seed=0,
            )
            for shape in ("flat_positive", "monotonic", "inverted_u")
        }
        req = required_n(base_f1, base_em, hops, shape="flat_positive", target=0.8,
                         n_trials=400, n_boot=600, seed=0)
        power = {
            "measured_pilot_cell_n": len(base_f1),
            "p_go_at_pilot_n_by_shape": shape_p_go,
            "required_n_flat_positive": req,
        }

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
    art["cost"] = answerer.cost_block()
    art["power_sim"] = power
    art["elapsed_s"] = round(time.time() - t0, 1)
    art["musique_hash"] = MUSIQUE_HASH

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(f"[S5][{mode.upper()}] wrote {output} | cost ${answerer.usd():.4f} "
          f"({answerer.n_calls} calls)")
    return art


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="M1 strong-baseline cheap-validate / pilot runner")
    ap.add_argument("--mode", choices=["cheap", "pilot"], required=True)
    ap.add_argument("--reader", default=None, help="answerer model id (defaults per mode)")
    ap.add_argument("--corpus", default=None)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--n-ge3", type=int, default=None)
    ap.add_argument("--n-2hop", type=int, default=None)
    ap.add_argument("--n-unans", type=int, default=None)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--workers", type=int, default=8, help="concurrent answerer calls")
    ap.add_argument("--output", required=True)
    args = ap.parse_args(argv)

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
        output=Path(args.output), answer_workers=args.workers,
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
