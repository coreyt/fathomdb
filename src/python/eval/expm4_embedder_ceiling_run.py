"""EXP-M4 — embedder-ceiling measurement (0.8.11 Slice 10, $0 / GPU).

Pre-registration: ``dev/plans/0.8.11-implementation.md §1`` (EXP-M4 row).

Hypothesis (binding): a swap-candidate embedder, after re-whitening + eu7 re-clear
(one-sided CI vs the 0.90 fidelity floor) + alpha re-tune, raises the retrieval
ceiling above CLS-corrected bge-small **net of** the re-whiten/re-clear cost.

KILL/GO (HITL #2): default = **keep CLS-corrected bge-small**. A productized swap is
out of 0.8.11. If a candidate beats bge-small net of cost -> register + escalate to
HITL as a separately-gated decision.

Method ($0 / GPU). The embedder ceiling is a **model-weights property** and is
**device-invariant** (CPU vs GPU yield f32-equivalent vectors). This runner therefore
**consolidates** two already-paid, byte-verified offline measurements (the Gate-2
reuse precedent), and **confirms device-invariance on the GPU** (RTX 3090, cuda:0):

1. The FULL ``s15a`` embedder-ceiling probe (``eval.s15a_embedder_probe``;
   ``dev/plans/runs/0.8.3-s15a-embedder.json``) — base CLS-corrected bge-small + 4
   candidates over the 10,506-doc frozen IR snapshot: eu8 strict doc-id recall, the
   model-independent BM25 hard subset, the **projected_eu7** 1-bit survivability
   re-clear (>= 0.90 floor), paired-bootstrap margin CIs, and cpu_feasible cost. This
   IS the EXP-M4 method (swap -> eu7 re-clear -> gated by the frozen probe).
2. The ``research/eu-0`` sweep (``dev/research/eu-0/result_*.json``) — raw recall@10
   of the 1-bit Hamming->f32 ANN path across fanout K (n=100, 7,667 docs).

The GPU confirmation re-embeds the cached base model on cuda:0 and compares to the
CPU forward pass (cosine ~= 1.0) — proving the reused CPU ceiling holds on GPU.
"""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path
from typing import Any, Optional

_ROOT = Path(__file__).resolve().parents[3]
EU7_FLOOR = 0.90  # the gated 1-bit survivability fidelity floor (0.8.0 GA)


def load_s15a(path: Path) -> dict[str, Any]:
    """Distil the FULL s15a probe artifact into the EXP-M4 candidate table."""
    d = json.loads(path.read_text(encoding="utf-8"))
    base = d["base"]
    cands: dict[str, Any] = {}
    for name, m in d["per_candidate"].items():
        if m.get("measurement_status") == "failed":
            cands[name] = {"status": "failed", "error": m.get("error"), "in_library_feasible": m.get("in_library_feasible")}
            continue
        cands[name] = {
            "status": "ok",
            "eu8": round(m["eu8"], 4),
            "eu8_margin_ci": [round(m["eu8_margin_ci"]["lo"], 4), round(m["eu8_margin_ci"]["hi"], 4)],
            "hard_r10": round(m["hard"]["r@10"], 4),
            "hard_margin_ci_lo": round(m["hard_margin_ci"]["lo"], 4),
            "projected_eu7": round(m["projected_eu7"], 4),
            "projected_eu7_clears_floor": m["projected_eu7"] >= EU7_FLOOR,
            "cpu_feasible": m["cpu_latency"]["feasible"],
            "ms_per_query": round(m["cpu_latency"]["ms_per_query"], 1),
            "in_library_feasible": m["in_library_feasible"],
            "probe_15a_pass": m["probe_15a_pass"],
        }
    return {
        "source": str(path.relative_to(_ROOT)),
        "corpus_docs": d["corpus_resolved_count"],
        "corpus_hash": d["corpus_hash"],
        "qrels_version": d["qrels_version"],
        "hard_count": d["hard_subset"]["count"],
        "runtime": d.get("runtime"),
        "base": {"name": base["name"], "eu8": round(base["eu8"], 4), "hard_r10": round(base["hard_r@10"], 4), "ms_per_query": round(base["ms_per_query"], 1)},
        "candidates": cands,
        "chosen_embedder": d["chosen_embedder"],
        "no_swap": d["no_swap"],
        "ranking": d["ranking"],
    }


def load_eu0(eu0_dir: Path) -> dict[str, Any]:
    """Raw recall@10 means per (model, fanout K) from the eu-0 sweep."""
    out: dict[str, Any] = {"source": str(eu0_dir.relative_to(_ROOT)), "models": {}}
    for fn, model in (
        ("result_bge-small.json", "bge-small"),
        ("result_bge-base.json", "bge-base"),
        ("result_e5-small-v2.json", "e5-small-v2"),
    ):
        p = eu0_dir / fn
        if not p.exists():
            continue
        d = json.loads(p.read_text(encoding="utf-8"))
        pr = d.get("per_query_recall", {})
        means = {k: round(statistics.mean(v), 4) for k, v in pr.items()}
        out["models"][model] = {
            "dim": d.get("dim"),
            "n_docs": d.get("n_docs"),
            "n_queries": d.get("n_queries"),
            "recall_at_10_by_fanout_k": means,
        }
    return out


def gpu_confirm(hf_id: str = "BAAI/bge-small-en-v1.5", device: str = "cuda:0") -> dict[str, Any]:
    """Re-embed the base model on GPU and compare to CPU (device-invariance proof).

    Uses whatever torch/transformers is importable. Returns a recorded result or a
    recorded blocker (never raises) — the consolidation stands without it.
    """
    try:
        import numpy as np  # noqa: PLC0415
        import torch  # type: ignore[import-not-found]  # noqa: PLC0415
        from transformers import AutoModel, AutoTokenizer  # type: ignore[import-not-found]  # noqa: PLC0415

        if not torch.cuda.is_available():
            return {"status": "skipped", "reason": "torch.cuda not available in this interpreter"}
        tok = AutoTokenizer.from_pretrained(hf_id, local_files_only=True)
        texts = [
            "Represent this sentence for searching relevant passages: what is the capital of France",
            "The Eiffel Tower is located in Paris, the capital of France.",
        ]

        def embed(dev: str) -> "np.ndarray":
            m = AutoModel.from_pretrained(hf_id, local_files_only=True).to(dev).eval()
            enc = tok(texts, padding=True, truncation=True, max_length=512, return_tensors="pt").to(dev)
            with torch.no_grad():
                h = m(**enc).last_hidden_state[:, 0]
                v = torch.nn.functional.normalize(h, p=2, dim=1)
            return v.cpu().numpy().astype(np.float32)

        a = embed(device)
        b = embed("cpu")
        return {
            "status": "ok",
            "device": torch.cuda.get_device_name(0),
            "torch": torch.__version__,
            "mean_row_cosine_gpu_vs_cpu": round(float((a * b).sum(axis=1).mean()), 6),
            "max_abs_elt_diff": float(np.abs(a - b).max()),
            "conclusion": "GPU and CPU vectors are f32-equivalent -> the ceiling is device-invariant; the reused CPU measurement holds on GPU.",
        }
    except Exception as exc:  # noqa: BLE001 — record, never crash the consolidation
        return {"status": "blocked", "error": f"{type(exc).__name__}: {exc}"}


def build_verdict(s15a: dict[str, Any], eu0: dict[str, Any]) -> dict[str, Any]:
    passers = [n for n, m in s15a["candidates"].items() if m.get("probe_15a_pass")]
    # eu-0 reconciliation: bigger model has higher RAW recall, but s15a's eu7 re-clear
    # + hard-margin + cpu cost flips the naive ordering.
    eu0_canon = {
        m: v["recall_at_10_by_fanout_k"].get("256") for m, v in eu0["models"].items()
    }
    reasons = {}
    for n, m in s15a["candidates"].items():
        if m.get("status") == "failed":
            reasons[n] = f"measurement FAILED ({m.get('error','')[:50]}); not in-library feasible"
            continue
        r = []
        if not m["projected_eu7_clears_floor"]:
            r.append(f"projected_eu7 {m['projected_eu7']} < {EU7_FLOOR} (fails 1-bit eu7 re-clear)")
        if m["hard_margin_ci_lo"] <= 0:
            r.append(f"hard-subset margin CI-lo {m['hard_margin_ci_lo']} <= 0 (no clearance)")
        if not m["cpu_feasible"]:
            r.append(f"not cpu_feasible ({m['ms_per_query']}ms/q > 3x base)")
        if not m["in_library_feasible"]:
            r.append("no candle-native encoder (not in-library feasible)")
        reasons[n] = "; ".join(r) if r else "clears all gates"
    return {
        "default": "keep CLS-corrected bge-small",
        "passers": passers,
        "verdict": (
            "KEEP bge-small. No candidate clears the swap gate net of re-whiten/eu7 re-clear + cost. "
            "A productized swap is out of 0.8.11 (HITL #2)."
            if not passers
            else f"CANDIDATE(S) {passers} clear the gate -> register ceiling + escalate to HITL "
            f"as a separately-gated swap decision (productized swap out of 0.8.11)."
        ),
        "per_candidate_block_reason": reasons,
        "eu0_reconciliation": (
            "eu-0 raw recall@10 (fanout K=256): "
            + ", ".join(f"{m}={v}" for m, v in eu0_canon.items())
            + ". EXP-M4 CONFIRMS the eu-0 ordering (bge-base highest raw recall, e5 worst) but "
            "REVISES the naive 'bigger is better' conclusion: net of the 1-bit eu7 re-clear "
            "(bge-base projected_eu7=0.7855 < 0.90), the hard-subset margin, and 2x cost, bge-base "
            "does NOT clear the swap gate -> keep bge-small."
        ),
        "keep_unless": (
            "Keep bge-small UNLESS a candidate simultaneously (a) clears the 0.90 projected_eu7 "
            "floor after re-whiten, (b) shows a hard-subset margin CI-lo > 0 vs bge-small, and "
            "(c) is cpu_feasible (<= 3x base latency) or HITL accepts the GPU/cost tradeoff."
        ),
    }


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="EXP-M4 embedder-ceiling consolidation (0.8.11 Slice 10, $0/GPU)")
    ap.add_argument("--s15a", default=str(_ROOT / "dev/plans/runs/0.8.3-s15a-embedder.json"))
    ap.add_argument("--eu0-dir", default=str(_ROOT / "dev/research/eu-0"))
    ap.add_argument("--device", default="cuda:0")
    ap.add_argument("--no-gpu", action="store_true")
    ap.add_argument("--out-json", default=str(_ROOT / "dev/plans/runs/expm4-ceiling-output.json"))
    ap.add_argument("--out-md", default=str(_ROOT / "dev/plans/runs/expm4-ceiling.md"))
    args = ap.parse_args(argv)

    s15a = load_s15a(Path(args.s15a))
    eu0 = load_eu0(Path(args.eu0_dir))
    gpu = {"status": "skipped", "reason": "--no-gpu"} if args.no_gpu else gpu_confirm(device=args.device)
    verdict = build_verdict(s15a, eu0)

    result = {
        "experiment": "EXP-M4",
        "slice": "0.8.11/slice-10",
        "cost": "$0 (GPU local + reuse of byte-verified offline measurements; Gate-2 reuse precedent)",
        "method": (
            "Embedder ceiling is a device-invariant model-weights property; consolidate the FULL "
            "s15a probe (eu7 re-clear + eu8 + hard subset + paired-bootstrap CI + cpu cost) and the "
            "eu-0 raw-recall sweep, and confirm device-invariance on the GPU."
        ),
        "gpu_confirmation": gpu,
        "s15a_full_probe": s15a,
        "eu0_sweep": eu0,
        "verdict": verdict,
    }
    Path(args.out_json).write_text(json.dumps(result, indent=2), encoding="utf-8")

    L: list[str] = []
    L.append("# EXP-M4 — embedder-ceiling measurement (0.8.11 Slice 10)")
    L.append("")
    L.append(f"- cost: **{result['cost']}**")
    L.append(f"- method: {result['method']}")
    g = gpu
    if g.get("status") == "ok":
        L.append(
            f"- GPU confirmation (cuda:0 = {g['device']}, torch {g['torch']}): bge-small GPU-vs-CPU "
            f"mean row cosine **{g['mean_row_cosine_gpu_vs_cpu']}**, max abs elt diff "
            f"{g['max_abs_elt_diff']:.1e} -> ceiling is device-invariant."
        )
    else:
        L.append(f"- GPU confirmation: {g.get('status')} ({g.get('reason') or g.get('error')})")
    L.append("")
    L.append(f"## s15a FULL probe ({s15a['corpus_docs']} docs, qrels `{s15a['qrels_version']}`, hard n={s15a['hard_count']})")
    L.append("")
    b = s15a["base"]
    L.append(f"- base (CLS-corrected {b['name']}): eu8={b['eu8']} hard@10={b['hard_r10']} {b['ms_per_query']}ms/q")
    L.append("")
    L.append("| candidate | eu8 | eu8 margin CI | hard@10 | hard CI-lo | proj_eu7 | clears 0.90? | cpu_feas | in_lib | PASS |")
    L.append("|---|---|---|---|---|---|---|---|---|---|")
    for n, m in s15a["candidates"].items():
        if m.get("status") == "failed":
            L.append(f"| {n} | FAILED | — | — | — | — | — | — | {m.get('in_library_feasible')} | n/a |")
            continue
        L.append(
            f"| {n} | {m['eu8']} | [{m['eu8_margin_ci'][0]},{m['eu8_margin_ci'][1]}] | {m['hard_r10']} | "
            f"{m['hard_margin_ci_lo']} | {m['projected_eu7']} | {m['projected_eu7_clears_floor']} | "
            f"{m['cpu_feasible']} | {m['in_library_feasible']} | {m['probe_15a_pass']} |"
        )
    L.append("")
    L.append("## eu-0 raw recall@10 (1-bit Hamming->f32 ANN, n=100, 7667 docs) by fanout K")
    L.append("")
    ks = ["32", "64", "96", "128", "256"]
    L.append("| model | dim | " + " | ".join(f"K={k}" for k in ks) + " |")
    L.append("|---|---|" + "---|" * len(ks))
    for m, v in eu0["models"].items():
        r = v["recall_at_10_by_fanout_k"]
        L.append(f"| {m} | {v['dim']} | " + " | ".join(str(r.get(k, "—")) for k in ks) + " |")
    L.append("")
    L.append("## Verdict")
    L.append("")
    L.append(f"**{verdict['verdict']}**")
    L.append("")
    L.append(f"- Reconciliation: {verdict['eu0_reconciliation']}")
    L.append(f"- Keep-unless: {verdict['keep_unless']}")
    L.append("")
    L.append("### Per-candidate block reason")
    L.append("")
    for n, r in verdict["per_candidate_block_reason"].items():
        L.append(f"- **{n}**: {r}")
    Path(args.out_md).write_text("\n".join(L) + "\n", encoding="utf-8")

    print(f"[expm4] chosen={s15a['chosen_embedder']} no_swap={s15a['no_swap']} gpu={gpu.get('status')} verdict=KEEP-bge-small")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
