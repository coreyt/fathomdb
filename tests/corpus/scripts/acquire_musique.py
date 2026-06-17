#!/usr/bin/env python3
"""Acquire MuSiQue-Ans corpus (distractor setting) for FathomDB 0.8.2 M1.

Source:  bdsaglam/musique on Hugging Face Hub (re-hosts StonyBrookNLP/musique v1.0).
         StonyBrookNLP/musique is NOT directly accessible on the HF Hub (returns
         "Repository not found"); bdsaglam/musique provides the canonical v1.0
         data files (musique_ans_v1.0_dev.jsonl, musique_full_v1.0_dev.jsonl)
         under the same CC-BY-4.0 license.
License: CC-BY-4.0 (Trivedi et al. 2022, MuSiQue: TACL arXiv:2108.00573).
Pinned:  bdsaglam/musique@22873a40 (last-modified 2024-10-05T06:57:44Z).

Acquires the 'default' config validation split (4 834 questions total):
  - Answerable set (MuSiQue-Ans):       2 417 questions (answerable=True)
  - Unanswerable contrast set:          2 417 questions (answerable=False)
  Both sets have ~20 paragraphs each in the distractor setting.

Materialized output format per question:
  {
    "id": str,                  # e.g. "2hop__153573_109006"
    "question": str,
    "hop_count": int,           # 2 | 3 | 4 — parsed from ID prefix
    "answer": str,
    "answer_aliases": [str],
    "answerable": bool,
    "paragraphs": [
      {"idx": int, "title": str, "text": str, "is_supporting": bool}
    ]
  }

Hop count is parsed from the ID prefix (e.g. '2hop__…' → 2, '3hop1__…' → 3,
'4hop3__…' → 4). For answerable questions hop_count equals the number of
supporting paragraphs (MuSiQue construction property; verified on every run).

Output: data/corpus-data/raw/musique_dev.jsonl  (gitignored).
Also writes:
  tests/corpus/scripts/manifest.json           — adds 'musique' entry
  dev/plans/runs/0.8.2-m1-corpus-manifest.json — M1 manifest artifact
"""

from __future__ import annotations

import datetime
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir, repo_root  # noqa: E402

# ── dataset coordinates (pinned) ──────────────────────────────────────────────
DATASET_ID = "bdsaglam/musique"
DATASET_REVISION = "22873a405dd809893b22ada0b499299fb612d2df"
DATASET_CONFIG = "default"   # contains BOTH answerable + unanswerable questions
DATASET_SPLIT = "validation"

LICENSE_SPDX = "CC-BY-4.0"
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_PATH = corpus_data_dir() / "raw" / "musique_dev.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"
M1_MANIFEST_PATH = repo_root() / "dev" / "plans" / "runs" / "0.8.2-m1-corpus-manifest.json"


# ── helpers ───────────────────────────────────────────────────────────────────

def parse_hop_count(question_id: str) -> int:
    """Parse hop count from MuSiQue question ID prefix.

    Examples:
      '2hop__153573_109006'  → 2
      '3hop1__241001_568433' → 3
      '4hop3__...'           → 4
    """
    m = re.match(r"^(\d+)hop", question_id)
    if not m:
        raise ValueError(f"Cannot parse hop count from MuSiQue ID: {question_id!r}")
    return int(m.group(1))


def materialize_row(raw: dict) -> dict:
    """Convert a raw HF dataset row to the canonical materialized format."""
    hop_count = parse_hop_count(raw["id"])
    paragraphs = [
        {
            "idx": p["idx"],
            "title": p["title"],
            "text": p["paragraph_text"],
            "is_supporting": bool(p["is_supporting"]),
        }
        for p in raw["paragraphs"]
    ]
    return {
        "id": raw["id"],
        "question": raw["question"],
        "hop_count": hop_count,
        "answer": raw["answer"],
        "answer_aliases": list(raw.get("answer_aliases") or []),
        "answerable": bool(raw["answerable"]),
        "paragraphs": paragraphs,
    }


def write_corpus(path: Path, rows: list[dict]) -> tuple[int, str]:
    """Write materialized corpus to JSONL (sort_keys for determinism); return (count, sha256)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    count = 0
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            line = json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
            count += 1
    return count, hasher.hexdigest()


# ── main ──────────────────────────────────────────────────────────────────────

def main() -> int:
    from datasets import load_dataset  # type: ignore[import-not-found]

    print(f"[S4][ACQUIRE] dataset:  {DATASET_ID}")
    print(f"[S4][ACQUIRE] revision: {DATASET_REVISION[:8]}")
    print(f"[S4][ACQUIRE] config:   {DATASET_CONFIG}  split: {DATASET_SPLIT}")
    print(f"[S4][ACQUIRE] output:   {OUT_PATH}")
    print()

    ds = load_dataset(
        DATASET_ID,
        DATASET_CONFIG,
        split=DATASET_SPLIT,
        revision=DATASET_REVISION,
    )
    print(f"[S4][ACQUIRE] {len(ds)} rows loaded; materializing…")

    # Materialize in HF iteration order (stable for a pinned revision → deterministic).
    rows = [materialize_row(dict(row)) for row in ds]

    # ── validation: hop_count == supporting para count for answerable questions ──
    mismatches = []
    for row in rows:
        if row["answerable"]:
            n_supporting = sum(1 for p in row["paragraphs"] if p["is_supporting"])
            if row["hop_count"] != n_supporting:
                mismatches.append((row["id"], row["hop_count"], n_supporting))
    if mismatches:
        print(f"[S4][WARN] hop_count != supporting_count for {len(mismatches)} questions")
        for qid, hc, sc in mismatches[:5]:
            print(f"  {qid!r}: hop_count={hc} supporting={sc}")
    else:
        print("[S4][ACQUIRE] ✓ hop_count == supporting_count for all answerable questions")

    # ── breakdown ─────────────────────────────────────────────────────────────
    hop_total: dict[int, int] = {}
    hop_ans: dict[int, int] = {}
    hop_unans: dict[int, int] = {}
    answerable_total = 0
    unanswerable_total = 0

    for row in rows:
        h = row["hop_count"]
        hop_total[h] = hop_total.get(h, 0) + 1
        if row["answerable"]:
            hop_ans[h] = hop_ans.get(h, 0) + 1
            answerable_total += 1
        else:
            hop_unans[h] = hop_unans.get(h, 0) + 1
            unanswerable_total += 1

    print("[S4][ACQUIRE] hop breakdown:")
    for h in sorted(hop_total):
        print(f"  {h}-hop: total={hop_total[h]}  answerable={hop_ans.get(h, 0)}  "
              f"unanswerable={hop_unans.get(h, 0)}")
    print(f"[S4][ACQUIRE] answerable={answerable_total}  unanswerable={unanswerable_total}")
    print()

    # ── write corpus ──────────────────────────────────────────────────────────
    count, musique_hash = write_corpus(OUT_PATH, rows)
    print(f"[S4][ACQUIRE] wrote {count} questions → {OUT_PATH}")
    print(f"[S4][ACQUIRE] musique_hash (sha256): {musique_hash}")
    print()

    # ── update manifest.json ──────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    manifest["sources"]["musique"] = {
        "script": "acquire_musique.py",
        "upstream": {
            "kind": "huggingface_dataset",
            "id": DATASET_ID,
            "note": (
                "re-hosts StonyBrookNLP/musique v1.0 data files; "
                "StonyBrookNLP/musique is not accessible on HF Hub (returns 404). "
                "Preferred HF path; fall back = download musique_full_v1.0_dev.jsonl "
                "directly from the bdsaglam/musique HF file store."
            ),
            "config": DATASET_CONFIG,
            "split": DATASET_SPLIT,
            "revision": DATASET_REVISION,
            "last_modified": "2024-10-05T06:57:44Z",
        },
        "license": LICENSE_SPDX,
        "license_notes": (
            "CC-BY-4.0 (Trivedi et al. 2022, MuSiQue, TACL arXiv:2108.00573). "
            "bdsaglam/musique re-hosts the canonical v1.0 files. "
            "License posture: commit-clean."
        ),
        "distribution": "cache",
        "output": "data/corpus-data/raw/musique_dev.jsonl",
        "doc_count": count,
        "doc_count_breakdown": {
            "answerable": answerable_total,
            "unanswerable": unanswerable_total,
            "hop_2": hop_total.get(2, 0),
            "hop_3": hop_total.get(3, 0),
            "hop_4": hop_total.get(4, 0),
        },
        "sha256": musique_hash,
        "acquired_at": datetime.date.today().isoformat(),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print(f"[S4][ACQUIRE] updated manifest.json  (musique sha256={musique_hash[:16]}…)")

    # ── write M1 corpus manifest ───────────────────────────────────────────────
    m1_manifest = {
        "schema": "0.8.2-m1-corpus-manifest-v1",
        "generated_by": "tests/corpus/scripts/acquire_musique.py",
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "musique_hash": musique_hash,
        "source": {
            "hf_id": DATASET_ID,
            "revision": DATASET_REVISION,
            "config": DATASET_CONFIG,
            "split": DATASET_SPLIT,
            "note": (
                "bdsaglam/musique re-hosts StonyBrookNLP/musique v1.0 under CC-BY-4.0. "
                "Both answerable (MuSiQue-Ans) and unanswerable contrast sets included "
                "via the 'answerable' field."
            ),
        },
        "materialized_corpus_path": "data/corpus-data/raw/musique_dev.jsonl",
        "acquire_command": (
            "cd <repo_root> && ./.venv/bin/python tests/corpus/scripts/acquire_musique.py"
        ),
        "counts": {
            "total": count,
            "answerable": answerable_total,
            "unanswerable": unanswerable_total,
        },
        "per_hop_counts": {
            str(h): hop_total.get(h, 0) for h in (2, 3, 4)
        },
        "per_hop_answerable": {
            str(h): {
                "answerable": hop_ans.get(h, 0),
                "unanswerable": hop_unans.get(h, 0),
            }
            for h in (2, 3, 4)
        },
    }
    M1_MANIFEST_PATH.parent.mkdir(parents=True, exist_ok=True)
    M1_MANIFEST_PATH.write_text(
        json.dumps(m1_manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print(f"[S4][ACQUIRE] wrote M1 corpus manifest → {M1_MANIFEST_PATH}")
    print()
    print("[S4][ACQUIRE] SUMMARY")
    print(f"  musique_hash : {musique_hash}")
    print(f"  total        : {count}")
    print(f"  answerable   : {answerable_total}  unanswerable: {unanswerable_total}")
    for h in (2, 3, 4):
        print(f"  {h}-hop       : {hop_total.get(h, 0)} total  "
              f"({hop_ans.get(h, 0)} ans / {hop_unans.get(h, 0)} unans)")
    print()
    print("[S4][ACQUIRE] DONE — corpus pinned. Run the TDD tests to verify:")
    print("  FATHOMDB_TESTS_NO_REBUILD=1 ./.venv/bin/python -m pytest "
          "tests/corpus/scripts/test_acquire_musique.py -q")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
