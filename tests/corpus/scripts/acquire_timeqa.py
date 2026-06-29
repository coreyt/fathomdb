#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire TimeQA (Time-Sensitive QA) from GitHub for FathomDB.

Source:   wenhuchen/Time-Sensitive-QA on GitHub — the `dataset/` folder.
          https://github.com/wenhuchen/Time-Sensitive-QA
Paper:    Chen et al. 2021, "A Dataset for Answering Time-Sensitive Questions",
          NeurIPS 2021 (Datasets & Benchmarks) (arXiv:2108.06314).
License:  BSD-3-Clause (UCSB NLP group; stated in the repo README and LICENSE).
          Redistributable with attribution.  Project default keeps the payload
          out of git (gitignored).

Role:     Canonical time-sensitive QA: questions mined from WikiData
          time-evolving facts aligned to Wikipedia passages, in Easy (explicit
          time expression) and Hard (implicit) modes, with `[unanswerable]`
          targets.  A reading-comprehension temporal probe that complements the
          synthetic Test-of-Time and the conversational LongMemEval/LOCOMO
          temporal slices.

Acquired splits (the template-synthesized eval splits that carry gold):
  dev.easy.json, dev.hard.json, test.easy.json, test.hard.json
Each line/element: {"idx", "question", "context", "targets"} where `targets`
is the gold answer list (empty list / "[unanswerable]" marks unanswerable).
The huge `train.*.gzip` and `annotated_*` files are intentionally NOT pulled
(eval gold lives in dev/test; train is large and not needed for FathomDB eval).

Output layout:
  data/corpus-data/raw/timeqa/
    dev.easy.json  dev.hard.json  test.easy.json  test.hard.json
"""

from __future__ import annotations

import hashlib
import json
import sys
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir  # noqa: E402

_RAW = "https://raw.githubusercontent.com/wenhuchen/Time-Sensitive-QA/main/dataset"
FILES = ("dev.easy.json", "dev.hard.json", "test.easy.json", "test.hard.json")

LICENSE_SPDX = "BSD-3-Clause"
LICENSE_NOTES = (
    "BSD-3-Clause (Chen et al. 2021, TimeQA, NeurIPS 2021 D&B, arXiv:2108.06314; "
    "UCSB NLP group, stated in repo README + LICENSE).  Redistribution permitted "
    "with attribution.  Project default keeps the payload out of git (gitignored)."
)

OUT_DIR = corpus_data_dir() / "raw" / "timeqa"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"


def _download(url: str, dest: Path) -> bytes:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=300) as resp:  # noqa: S310 (pinned raw.githubusercontent host)
        data = resp.read()
    dest.write_bytes(data)
    return data


def _count_records(raw: bytes) -> tuple[int, int]:
    """Return (total_records, records_with_targets). TimeQA dataset files are
    either a JSON array of objects or newline-delimited JSON objects."""
    text = raw.decode("utf-8")
    stripped = text.lstrip()
    records: list = []
    if stripped.startswith("["):
        records = json.loads(text)
    else:
        for line in text.splitlines():
            line = line.strip()
            if line:
                records.append(json.loads(line))
    n = len(records)
    n_targets = sum(1 for r in records if isinstance(r, dict) and "targets" in r)
    return n, n_targets


def main() -> int:
    per_file: dict[str, dict] = {}
    total = 0
    for name in FILES:
        url = f"{_RAW}/{name}"
        dest = OUT_DIR / name
        print(f"[acquire_timeqa] downloading {url} …")
        data = _download(url, dest)
        n, n_targets = _count_records(data)
        sha = hashlib.sha256(data).hexdigest()
        total += n
        print(
            f"[acquire_timeqa] {name}: {n} QA ({n_targets} with targets gold), "
            f"{len(data)/1e6:.1f} MB, sha256={sha[:16]}…"
        )
        per_file[name] = {
            "qa_count": n,
            "with_targets": n_targets,
            "bytes": len(data),
            "sha256": sha,
            "output": f"data/corpus-data/raw/timeqa/{name}",
        }
    print(f"[acquire_timeqa] TOTAL QA across dev/test easy+hard: {total}")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("timeqa", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["timeqa"] = {
        "script": "acquire_timeqa.py",
        "upstream": {
            "kind": "github_repo_raw_files",
            "id": "wenhuchen/Time-Sensitive-QA",
            "path": "dataset/",
            "files": list(FILES),
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output_dir": "data/corpus-data/raw/timeqa/",
        "total_qa": total,
        "files": per_file,
        "acquired_at": acquired_at,
        "role_note": (
            "Time-sensitive reading-comprehension QA (Easy/Hard + unanswerable). "
            "Temporal-reasoning gold; complements Test-of-Time (synthetic leaf) "
            "and LongMemEval/LOCOMO temporal (conversational)."
        ),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_timeqa] updated manifest.json")
    print("[acquire_timeqa] DONE — payload is gitignored")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
