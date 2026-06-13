#!/usr/bin/env python3
"""IR-C R0: CPU cross-encoder latency benchmark.

Measures ms/pair p50 + p95 for TinyBERT-L-2 (~4 MB) and MiniLM-L6 (~22.7 MB)
on ≥1,000 random (query, passage) pairs sampled from the frozen corpus gold set.
Reads the existing IR-C-recall-cdf.json artifact, patches in the latency array
(two entries, one per model), and writes it back.

Usage:
    # From the repo root:
    python3 dev/scripts/ir_c_ce_latency.py

    # Custom gold set or artifact path:
    python3 dev/scripts/ir_c_ce_latency.py \
        --gold data/corpus-data/eval/ir_gold/all.gold.json \
        --artifact dev/plans/runs/IR-C-recall-cdf.json \
        --n-pairs 2000

Requirements:
    pip install flashrank            # primary — fast cross-encoder scoring
    # OR
    pip install sentence-transformers  # fallback

Note: If models are unavailable this script writes latency=null and exits 0
(treated as a HITL-flagged blocker per the slice prompt §3.3).
"""

import argparse
import json
import os
import platform
import random
import sys
import time
from pathlib import Path

MODELS = [
    {
        "name": "TinyBERT-L-2",
        "flashrank_id": "ms-marco-TinyBERT-L-2-v2",
        "sbert_id": "cross-encoder/ms-marco-TinyBERT-L-2",
        "approx_mb": 4,
    },
    {
        "name": "MiniLM-L6",
        "flashrank_id": "ms-marco-MiniLM-L6-v2",
        "sbert_id": "cross-encoder/ms-marco-MiniLM-L6-v2",
        "approx_mb": 23,
    },
]

REPO_ROOT = Path(__file__).resolve().parents[2]


def cpu_info() -> str:
    """Return a short CPU description for the hardware_note field."""
    info = platform.processor() or ""
    if not info and sys.platform == "linux":
        try:
            with open("/proc/cpuinfo") as f:
                for line in f:
                    if line.lower().startswith("model name"):
                        info = line.split(":", 1)[1].strip()
                        break
        except OSError:
            pass
    ncpu = os.cpu_count() or 1
    return f"{info or 'unknown'} ({ncpu} logical CPUs)"


def sample_pairs(gold_path: Path, raw_dir: Path, n: int, seed: int = 42) -> list[tuple[str, str]]:
    """Sample n random (query, passage) pairs from gold queries + raw corpus docs."""
    rng = random.Random(seed)

    # Load queries from the gold set.
    with open(gold_path) as f:
        gold = json.load(f)
    queries = [q["query"] for q in gold.get("queries", []) if q.get("query_class") != "negative"]
    if not queries:
        raise ValueError(f"No positive queries found in {gold_path}")

    # Load passages from the raw JSONL files.
    passages: list[str] = []
    for jsonl_path in sorted(raw_dir.glob("*.jsonl")):
        with open(jsonl_path) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    doc = json.loads(line)
                    body = doc.get("body", "").strip()
                    if body:
                        passages.append(body[:2000])  # truncate very long docs
                except json.JSONDecodeError:
                    continue
        if len(passages) >= n * 10:  # early exit — we have plenty
            break

    if not passages:
        raise ValueError(f"No passages found in {raw_dir}")

    pairs = []
    for _ in range(n):
        q = rng.choice(queries)
        p = rng.choice(passages)
        pairs.append((q, p))
    return pairs


def percentile(values: list[float], p: float) -> float:
    """Compute the p-th percentile of a sorted list."""
    sv = sorted(values)
    idx = (len(sv) - 1) * p / 100.0
    lo = int(idx)
    hi = min(lo + 1, len(sv) - 1)
    frac = idx - lo
    return sv[lo] * (1 - frac) + sv[hi] * frac


def benchmark_with_flashrank(model_id: str, pairs: list[tuple[str, str]]) -> list[float]:
    """Return per-pair latencies (ms) using FlashRank."""
    from flashrank import Ranker, RerankRequest  # type: ignore[import]

    ranker = Ranker(model_name=model_id, cache_dir=None)
    latencies_ms = []
    for query, passage in pairs:
        req = RerankRequest(query=query, passages=[{"id": "0", "text": passage}])
        t0 = time.perf_counter()
        _ = ranker.rerank(req)
        elapsed_ms = (time.perf_counter() - t0) * 1000.0
        latencies_ms.append(elapsed_ms)
    return latencies_ms


def benchmark_with_sbert(model_id: str, pairs: list[tuple[str, str]]) -> list[float]:
    """Return per-pair latencies (ms) using sentence-transformers CrossEncoder."""
    from sentence_transformers import CrossEncoder  # type: ignore[import]

    model = CrossEncoder(model_id)
    latencies_ms = []
    for query, passage in pairs:
        t0 = time.perf_counter()
        _ = model.predict([(query, passage)])
        elapsed_ms = (time.perf_counter() - t0) * 1000.0
        latencies_ms.append(elapsed_ms)
    return latencies_ms


def benchmark_model(model_spec: dict, pairs: list[tuple[str, str]]) -> dict | None:
    """Benchmark one model; return latency dict or None if unavailable."""
    latencies_ms = None

    # Try FlashRank first.
    try:
        import flashrank  # type: ignore[import]  # noqa: F401
        print(f"  [{model_spec['name']}] using FlashRank ({model_spec['flashrank_id']})")
        latencies_ms = benchmark_with_flashrank(model_spec["flashrank_id"], pairs)
    except ImportError:
        pass
    except Exception as e:
        print(f"  [{model_spec['name']}] FlashRank failed: {e}")

    # Fallback to sentence-transformers.
    if latencies_ms is None:
        try:
            import sentence_transformers  # type: ignore[import]  # noqa: F401
            print(f"  [{model_spec['name']}] using sentence-transformers ({model_spec['sbert_id']})")
            latencies_ms = benchmark_with_sbert(model_spec["sbert_id"], pairs)
        except ImportError:
            pass
        except Exception as e:
            print(f"  [{model_spec['name']}] sentence-transformers failed: {e}")

    if latencies_ms is None:
        print(f"  [{model_spec['name']}] UNAVAILABLE — neither FlashRank nor sentence-transformers found")
        return None

    p50 = percentile(latencies_ms, 50)
    p95 = percentile(latencies_ms, 95)
    result = {
        "model": model_spec["name"],
        "ms_per_pair_p50": round(p50, 3),
        "ms_per_pair_p95": round(p95, 3),
        "n_pairs_sampled": len(latencies_ms),
        "hardware_note": cpu_info(),
    }
    print(f"  [{model_spec['name']}] p50={p50:.2f}ms p95={p95:.2f}ms n={len(latencies_ms)}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--gold",
        default=str(REPO_ROOT / "data/corpus-data/eval/ir_gold/all.gold.json"),
        help="Path to the gold set JSON",
    )
    parser.add_argument(
        "--artifact",
        default=str(REPO_ROOT / "dev/plans/runs/IR-C-recall-cdf.json"),
        help="Path to the CDF artifact to patch",
    )
    parser.add_argument(
        "--n-pairs", type=int, default=1000, help="Number of (query, passage) pairs to sample"
    )
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    args = parser.parse_args()

    gold_path = Path(args.gold)
    artifact_path = Path(args.artifact)
    raw_dir = REPO_ROOT / "data/corpus-data/raw"

    # Validate inputs.
    if not gold_path.exists():
        print(f"ERROR: gold set not found at {gold_path}", file=sys.stderr)
        print("  Run: python3 tests/corpus/scripts/build_ir_gold.py", file=sys.stderr)
        return 1
    if not artifact_path.exists():
        print(f"ERROR: CDF artifact not found at {artifact_path}", file=sys.stderr)
        print("  Run the CDF runner first: IRC_RUN=1 cargo test ...", file=sys.stderr)
        return 1
    if not raw_dir.is_dir():
        print(f"ERROR: corpus raw directory not found at {raw_dir}", file=sys.stderr)
        return 1

    # Sample pairs.
    print(f"Sampling {args.n_pairs} (query, passage) pairs from gold + corpus...")
    try:
        pairs = sample_pairs(gold_path, raw_dir, args.n_pairs, seed=args.seed)
    except Exception as e:
        print(f"ERROR sampling pairs: {e}", file=sys.stderr)
        return 1
    print(f"  Sampled {len(pairs)} pairs from {gold_path.name} + {raw_dir}")

    # Benchmark each model.
    print("\nBenchmarking cross-encoder models (CPU, single-pair inference)...")
    latency_entries = []
    all_unavailable = True
    for model_spec in MODELS:
        result = benchmark_model(model_spec, pairs)
        if result is not None:
            latency_entries.append(result)
            all_unavailable = False
        else:
            # Placeholder entry with null latencies.
            latency_entries.append({
                "model": model_spec["name"],
                "ms_per_pair_p50": None,
                "ms_per_pair_p95": None,
                "n_pairs_sampled": args.n_pairs,
                "hardware_note": cpu_info(),
                "error": "model unavailable — install flashrank or sentence-transformers",
            })

    # Load and patch the artifact.
    with open(artifact_path) as f:
        artifact = json.load(f)

    artifact["latency"] = latency_entries if latency_entries else None

    with open(artifact_path, "w") as f:
        json.dump(artifact, f, indent=2)
        f.write("\n")

    print(f"\nPatched {artifact_path}")

    if all_unavailable:
        print(
            "\nWARNING: All CE models unavailable — latency is null in artifact.\n"
            "  HITL blocker: install flashrank or sentence-transformers and re-run.",
            file=sys.stderr,
        )
        # Still exit 0: the recall CDF is the primary output; latency is secondary.
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
