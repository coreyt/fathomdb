#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire Test of Time (ToT) from Hugging Face for FathomDB.

Source:   baharef/ToT on Hugging Face Hub (configs: tot_semantic,
          tot_arithmetic, tot_semantic_large; each a single `test` split).
          Generator on GitHub: baharef/Test-of-Time.
Paper:    Fatemi et al. 2024, "Test of Time: A Benchmark for Evaluating LLMs on
          Temporal Reasoning" (Google, arXiv:2406.09170).
License:  CC-BY-4.0 (redistributable with attribution).  Project default keeps
          the payload out of git (gitignored).

Role:     Contamination-free SYNTHETIC temporal-reasoning benchmark.  Built to
          avoid pretraining contamination; cleanly separates temporal
          semantics/logic (tot_semantic) from temporal arithmetic
          (tot_arithmetic).  A leaf-level temporal probe to complement the
          conversational temporal slices (LongMemEval / LOCOMO).

Gold:     YES — explicit `label` (gold answer) on every example.
            - tot_semantic / tot_semantic_large: `label` = the gold entity id
              (string); `prompt` carries the temporal-fact context, `question`
              the query, plus question_type / sorting_type / graph_gen_algorithm.
            - tot_arithmetic: `label` = a stringified dict {'answer': ...}.

Output layout:
  data/corpus-data/raw/tot/
    tot_semantic.jsonl        (2,800)
    tot_arithmetic.jsonl      (1,850)
    tot_semantic_large.jsonl  (46,480)   total 51,130 examples
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir  # noqa: E402

# ── dataset coordinates ────────────────────────────────────────────────────────
REPO_ID = "baharef/ToT"
CONFIGS = ("tot_semantic", "tot_arithmetic", "tot_semantic_large")
DATASET_REVISION = None  # pin to a commit SHA after first run

EXPECT = {
    "tot_semantic": 2_800,
    "tot_arithmetic": 1_850,
    "tot_semantic_large": 46_480,
}

LICENSE_SPDX = "CC-BY-4.0"
LICENSE_NOTES = (
    "CC BY 4.0 (Fatemi et al. 2024, Test of Time, arXiv:2406.09170, Google).  "
    "Redistribution permitted with attribution.  Project default keeps the "
    "payload out of git (gitignored)."
)

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_DIR = corpus_data_dir() / "raw" / "tot"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"


def write_jsonl(path: Path, rows: list[dict]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            line = json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
    return hasher.hexdigest()


def main() -> int:
    from datasets import load_dataset  # type: ignore[import-not-found]

    per_config: dict[str, dict] = {}
    total = 0
    for cfg in CONFIGS:
        print(f"[acquire_tot] loading {REPO_ID}:{cfg} (test) …")
        ds = load_dataset(
            REPO_ID, cfg, split="test", revision=DATASET_REVISION, trust_remote_code=False
        )
        rows = [dict(r) for r in ds]
        n = len(rows)
        total += n
        if n != EXPECT[cfg]:
            print(f"[acquire_tot] WARN: {cfg} expected {EXPECT[cfg]}, got {n}")
        n_gold = sum(1 for r in rows if r.get("label") not in (None, ""))
        out = OUT_DIR / f"{cfg}.jsonl"
        sha = write_jsonl(out, rows)
        print(f"[acquire_tot] {cfg}: {n} examples ({n_gold} with gold label) → {out}  (sha256={sha[:16]}…)")
        per_config[cfg] = {
            "examples": n,
            "with_gold_label": n_gold,
            "output": f"data/corpus-data/raw/tot/{cfg}.jsonl",
            "sha256": sha,
        }

    print(f"[acquire_tot] TOTAL examples: {total}")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("test_of_time", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["test_of_time"] = {
        "script": "acquire_tot.py",
        "upstream": {
            "kind": "huggingface_dataset",
            "id": REPO_ID,
            "configs": list(CONFIGS),
            "split": "test",
            "revision": DATASET_REVISION,
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output_dir": "data/corpus-data/raw/tot/",
        "total_examples": total,
        "configs": per_config,
        "acquired_at": acquired_at,
        "role_note": (
            "Contamination-free synthetic temporal-reasoning probe "
            "(semantics/logic vs arithmetic).  Leaf-level temporal complement "
            "to the conversational LongMemEval/LOCOMO temporal slices."
        ),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_tot] updated manifest.json")
    print("[acquire_tot] DONE — payload is gitignored")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
