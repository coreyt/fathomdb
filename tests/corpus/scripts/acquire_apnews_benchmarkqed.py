#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire the Microsoft BenchmarkQED AP-News sensemaking corpus for FathomDB 0.8.4.

Source:  microsoft/benchmark-qed on GitHub — ``datasets/AP_news/``.
         https://github.com/microsoft/benchmark-qed
Blog:    "BenchmarkQED: Automated benchmarking of RAG systems" (Microsoft
         Research, June 2025).
License: **Microsoft Research License** — NON-COMMERCIAL, research-only, and
         explicitly NON-REDISTRIBUTABLE ("you may not distribute the data or your
         modifications"; "may not share/publish/distribute … to any third
         party"). EVAL-ONLY. Payload is written under data/corpus-data/
         (gitignored) and is NEVER committed, NEVER shipped. License saved as
         ``apnews_benchmarkqed/LICENSE``.

Role:    The 0.8.4 GraphRAG-sensemaking resolution (roadmap 0.8.4 §1) names this
         exact corpus + the AutoQ/AutoE harness. It was not in the repo; this
         brings it in for the S1 community-summary build + the G-HH-2 GraphRAG
         head-to-head (BenchmarkQED AutoQ/AutoE LLM-judge, bias controls).

Contents acquired:
  * raw_data.zip                  — 1,397 health-related AP news articles (JSON).
  * generated_questions_v1/*.json — AutoQ text questions (activity/data x global/local).
  * generated_questions_v2/*.json — AutoQ questions with assertions (local/global/linked).
  * LICENSE                        — the dataset license terms.
"""

from __future__ import annotations

import sys
import urllib.request
import zipfile
from pathlib import Path

_RAW = "https://raw.githubusercontent.com/microsoft/benchmark-qed/main/datasets"
_DEST = Path("data/corpus-data/raw/apnews_benchmarkqed")
_EXPECT_ARTICLES = 1397

_FILES = [
    ("AP_news/raw_data.zip", "raw_data.zip"),
    ("LICENSE", "LICENSE"),
    ("AP_news/generated_questions_v1/activity_global_questions_text.json", "generated_questions_v1/activity_global_questions_text.json"),
    ("AP_news/generated_questions_v1/activity_local_questions_text.json", "generated_questions_v1/activity_local_questions_text.json"),
    ("AP_news/generated_questions_v1/data_global_questions_text.json", "generated_questions_v1/data_global_questions_text.json"),
    ("AP_news/generated_questions_v1/data_local_questions_text.json", "generated_questions_v1/data_local_questions_text.json"),
    ("AP_news/generated_questions_v2/data_global_questions_assertions.json", "generated_questions_v2/data_global_questions_assertions.json"),
    ("AP_news/generated_questions_v2/data_linked_questions_assertions.json", "generated_questions_v2/data_linked_questions_assertions.json"),
    ("AP_news/generated_questions_v2/data_local_questions_assertions.json", "generated_questions_v2/data_local_questions_assertions.json"),
]


def _download(rel: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(f"{_RAW}/{rel}", timeout=300) as resp:  # noqa: S310 (pinned host)
        dest.write_bytes(resp.read())


def main() -> int:
    for rel, out in _FILES:
        dest = _DEST / out
        print(f"[acquire_apnews] {rel}", file=sys.stderr)
        _download(rel, dest)

    zpath = _DEST / "raw_data.zip"
    n_articles = sum(1 for n in zipfile.ZipFile(zpath).namelist() if n.endswith(".json"))
    print(f"[acquire_apnews] {n_articles} articles in raw_data.zip", file=sys.stderr)
    if n_articles != _EXPECT_ARTICLES:
        print(
            f"[acquire_apnews] WARNING: expected {_EXPECT_ARTICLES} articles, "
            f"got {n_articles} — upstream may have changed",
            file=sys.stderr,
        )
    print("[acquire_apnews] OK — EVAL-ONLY, gitignored, NON-REDISTRIBUTABLE, do not commit", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
