#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "datasets>=3.0",
#   "pyarrow>=17",
# ]
# ///
"""Acquire HotpotQA corpus (distractor setting) for FathomDB 0.8.11.2 V-1.

Source:  hotpotqa/hotpot_qa on Hugging Face Hub — the canonical HotpotQA v1.1
         distribution (Yang et al. 2018).
Paper:   Yang et al. 2018, "HotpotQA: A Dataset for Diverse, Explainable
         Multi-hop Question Answering", EMNLP 2018 (arXiv:1809.09600).
License: CC-BY-SA-4.0 (per the HF dataset card license tag). License posture:
         commit-clean (the produced JSONL lives under data/corpus-data/, which
         is gitignored — only this script is tracked).
Pinned:  hotpotqa/hotpot_qa@1908d6af (last-modified 2025-08-11T10:16:27Z).

Acquires the 'distractor' config validation split (7 405 questions) — the
standard multi-hop evaluation setting in which each question is paired with 10
paragraphs (2 gold + 8 distractors).  This is the HotpotQA analog of the
MuSiQue distractor setting acquired by acquire_musique.py; the two together are
the V-1 multi_hop corpus pair (multi_hop = MuSiQue + HotpotQA).

Materialized output format per question:
  {
    "id": str,                  # native HotpotQA hex id, e.g. "5a8b57f2..."
    "question": str,
    "answer": str,
    "type": str,                # native: "comparison" | "bridge"
    "level": str,               # native: "easy" | "medium" | "hard"
    "supporting_facts": [
      {"title": str, "sent_id": int}   # the multi-hop SUPPORT GOLD
    ],
    "context": [
      {"title": str, "sentences": [str]}
    ]
  }

The native multi-hop support gold (`supporting_facts`) is retained verbatim:
each entry names the gold paragraph `title` plus the supporting sentence index
`sent_id` within that paragraph.  This is the HotpotQA analog of MuSiQue's
`question_decomposition` / `paragraph_support_idx` (the per-hop supporting
evidence).  No labels are synthesized — only the fields HotpotQA natively
provides are carried through; the parallel-list HF shape
(`{title:[...], sent_id:[...]}`) is zipped into a list of `{title, sent_id}`
records and the parallel-list `context` (`{title:[...], sentences:[[...]]}`)
into a list of `{title, sentences}` records for ergonomics, with no data added
or dropped.

Output: data/corpus-data/raw/hotpotqa_dev.jsonl  (gitignored).
Also writes:
  tests/corpus/scripts/manifest.json   — adds/updates the 'hotpotqa' entry
"""

from __future__ import annotations

import datetime
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir  # noqa: E402

# ── dataset coordinates (pinned) ──────────────────────────────────────────────
DATASET_ID = "hotpotqa/hotpot_qa"
DATASET_REVISION = "1908d6afbbead072334abe2965f91bd2709910ab"
DATASET_CONFIG = "distractor"   # 10-paragraph (2 gold + 8 distractor) eval setting
DATASET_SPLIT = "validation"

LICENSE_SPDX = "CC-BY-SA-4.0"
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"

# ── paths ─────────────────────────────────────────────────────────────────────
OUT_PATH = corpus_data_dir() / "raw" / "hotpotqa_dev.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"


# ── helpers ───────────────────────────────────────────────────────────────────

def materialize_row(raw: dict) -> dict:
    """Convert a raw HF dataset row to the canonical materialized format.

    The HF row stores `supporting_facts` and `context` as parallel-list structs;
    we zip them into lists-of-records (no data added or dropped) so each gold
    support fact is one `{title, sent_id}` and each context paragraph is one
    `{title, sentences}`.
    """
    sf = raw["supporting_facts"]
    supporting_facts = [
        {"title": title, "sent_id": int(sent_id)}
        for title, sent_id in zip(sf["title"], sf["sent_id"])
    ]
    ctx = raw["context"]
    context = [
        {"title": title, "sentences": list(sentences)}
        for title, sentences in zip(ctx["title"], ctx["sentences"])
    ]
    return {
        "id": raw["id"],
        "question": raw["question"],
        "answer": raw["answer"],
        "type": raw["type"],
        "level": raw["level"],
        "supporting_facts": supporting_facts,
        "context": context,
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

    print(f"[V-1][ACQUIRE] dataset:  {DATASET_ID}")
    print(f"[V-1][ACQUIRE] revision: {DATASET_REVISION[:8]}")
    print(f"[V-1][ACQUIRE] config:   {DATASET_CONFIG}  split: {DATASET_SPLIT}")
    print(f"[V-1][ACQUIRE] output:   {OUT_PATH}")
    print()

    ds = load_dataset(
        DATASET_ID,
        DATASET_CONFIG,
        split=DATASET_SPLIT,
        revision=DATASET_REVISION,
    )
    print(f"[V-1][ACQUIRE] {len(ds)} rows loaded; materializing…")

    # Materialize in HF iteration order (stable for a pinned revision → deterministic).
    rows = [materialize_row(dict(row)) for row in ds]

    # ── validation: every answerable row carries non-empty supporting_facts ─────
    # HotpotQA distractor/validation has NO unanswerable contrast set: every row
    # is answerable and must carry ≥1 supporting fact (the multi-hop support gold).
    missing_support = [r["id"] for r in rows if not r["supporting_facts"]]
    if missing_support:
        print(f"[V-1][WARN] {len(missing_support)} rows carry EMPTY supporting_facts")
        for qid in missing_support[:5]:
            print(f"  {qid!r}: no supporting_facts")
    else:
        print("[V-1][ACQUIRE] ✓ every row carries non-empty supporting_facts")

    # ── breakdown by type + level ───────────────────────────────────────────────
    by_type: dict[str, int] = {}
    by_level: dict[str, int] = {}
    for row in rows:
        by_type[row["type"]] = by_type.get(row["type"], 0) + 1
        by_level[row["level"]] = by_level.get(row["level"], 0) + 1

    print("[V-1][ACQUIRE] type breakdown:")
    for t in sorted(by_type):
        print(f"  {t}: {by_type[t]}")
    print("[V-1][ACQUIRE] level breakdown:")
    for lvl in sorted(by_level):
        print(f"  {lvl}: {by_level[lvl]}")
    print()

    # ── write corpus ────────────────────────────────────────────────────────────
    count, hotpot_hash = write_corpus(OUT_PATH, rows)
    print(f"[V-1][ACQUIRE] wrote {count} questions → {OUT_PATH}")
    print(f"[V-1][ACQUIRE] hotpotqa_hash (sha256): {hotpot_hash}")
    print()

    # ── update manifest.json ────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    # Preserve acquired_at from the existing entry so reproduce runs are
    # byte-stable: only record the date on the FIRST acquisition; subsequent
    # reproduces must not dirty the tracked manifest file.
    existing = manifest.get("sources", {}).get("hotpotqa", {})
    acquired_at = existing.get("acquired_at", datetime.date.today().isoformat())
    manifest["sources"]["hotpotqa"] = {
        "script": "acquire_hotpotqa.py",
        "upstream": {
            "kind": "huggingface_dataset",
            "id": DATASET_ID,
            "note": (
                "Canonical HotpotQA v1.1 (Yang et al. 2018). The 'distractor' "
                "config pairs each question with 10 paragraphs (2 gold + 8 "
                "distractors) — the standard multi-hop eval setting. Native "
                "supporting_facts (title + sentence index) retained as the "
                "multi-hop support gold."
            ),
            "config": DATASET_CONFIG,
            "split": DATASET_SPLIT,
            "revision": DATASET_REVISION,
            "last_modified": "2025-08-11T10:16:27Z",
        },
        "license": LICENSE_SPDX,
        "license_notes": (
            "CC-BY-SA-4.0 (per the hotpotqa/hotpot_qa HF dataset-card license "
            "tag; Yang et al. 2018, HotpotQA, EMNLP arXiv:1809.09600). "
            "License posture: commit-clean (produced JSONL is gitignored)."
        ),
        "distribution": "cache",
        "output": "data/corpus-data/raw/hotpotqa_dev.jsonl",
        "doc_count": count,
        "doc_count_breakdown": {
            "type_comparison": by_type.get("comparison", 0),
            "type_bridge": by_type.get("bridge", 0),
            "level_easy": by_level.get("easy", 0),
            "level_medium": by_level.get("medium", 0),
            "level_hard": by_level.get("hard", 0),
        },
        "sha256": hotpot_hash,
        "acquired_at": acquired_at,
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print(f"[V-1][ACQUIRE] updated manifest.json  (hotpotqa sha256={hotpot_hash[:16]}…)")
    print()
    print("[V-1][ACQUIRE] SUMMARY")
    print(f"  hotpotqa_hash : {hotpot_hash}")
    print(f"  total         : {count}")
    print(f"  comparison    : {by_type.get('comparison', 0)}  bridge: {by_type.get('bridge', 0)}")
    print(f"  empty support : {len(missing_support)}")
    print()
    print("[V-1][ACQUIRE] DONE — corpus pinned (EVAL-ONLY, gitignored, do not commit the payload).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
