#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "pandas>=1.5",
#   "numpy>=1.23",
#   "python-dateutil>=2.8",
# ]
# ///
"""Acquire TimelineQA (lifelog timeline QA) by LOCAL GENERATION for FathomDB.

Source:   facebookresearch/TimelineQA on GitHub — the lifelog generator
          (`src/generateDB.py`).  https://github.com/facebookresearch/TimelineQA
Paper:    Tan et al. 2023, "TimelineQA: A Benchmark for Question Answering over
          Timelines", Findings of ACL 2023 (arXiv:2306.01069).
License:  CC-BY-NC-4.0 — NON-COMMERCIAL.  EVAL-ONLY.  The generated payload is
          written under data/corpus-data/ (gitignored) and is NEVER committed,
          NEVER shipped.  Same EVAL-ONLY footprint posture as LOCOMO.  The repo
          LICENSE (Creative Commons Attribution-NonCommercial 4.0) is saved
          alongside the clone.

Role:     The strongest pure personal-timeline / daily-life fit.  Generates
          synthetic lifelogs for fictitious personas, fusing major life events
          (birth, relocation, marriage, travel) down to daily activities
          (meals, exercise, reading, TV, chat).  Each generated event carries a
          natural-language rendering plus gold atomic QA pairs.

Gold:     YES.  Every event in the generated timeline DB ships
          `atomic_qa_pairs` = [[question, answer], ...] (NL gold answers), with
          `text_template_based` as the retrievable evidence and `eid` as the
          episode id.  (The SQL-based MULTI-HOP gold is an optional extra step
          via the repo's multihopQA/multihopQA.py — it needs pandasql; it is NOT
          run here.  The atomic gold is sufficient for the daily-life eval.)

Generation:
  For each density in {sparse, medium, dense}, generate TIMELINEQA_PERSONAS
  personas (default 10) using the upstream seed convention (seed = 12345 + i),
  so the set aligns with the official benchmark scripts.  Idempotent: a persona
  whose output JSON already exists is skipped.

Output layout:
  data/corpus-data/raw/timelineqa/
    sparse/persona-000/sparse-000.json   (+ persona.json + *-log.csv tables)
    medium/persona-000/medium-000.json
    dense/persona-000/dense-000.json
    ...
    index.json   — summary (counts + per-persona atomic-QA totals)
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir  # noqa: E402

REPO_URL = "https://github.com/facebookresearch/TimelineQA.git"
REPO_PIN = "e0ff14f707ce07b1f656e61b2921059c3fc98168"

DENSITIES = ("sparse", "medium", "dense")
SEED_BASE = 12345
FINAL_YEAR = 2022
N_PERSONAS = int(os.environ.get("TIMELINEQA_PERSONAS", "10"))

LICENSE_SPDX = "CC-BY-NC-4.0"
LICENSE_NOTES = (
    "CC-BY-NC-4.0 — NON-COMMERCIAL (Tan et al. 2023, TimelineQA, Findings of "
    "ACL 2023, arXiv:2306.01069; Meta).  EVAL-ONLY, gitignored, NEVER committed "
    "or shipped.  Locally generated from the repo's generateDB.py."
)

CLONE_DIR = corpus_data_dir() / "downloads" / "TimelineQA"
OUT_DIR = corpus_data_dir() / "raw" / "timelineqa"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"


def ensure_clone() -> Path:
    if (CLONE_DIR / "src" / "generateDB.py").exists():
        print(f"[acquire_timelineqa] using existing clone at {CLONE_DIR}")
        return CLONE_DIR
    CLONE_DIR.parent.mkdir(parents=True, exist_ok=True)
    print(f"[acquire_timelineqa] cloning {REPO_URL} → {CLONE_DIR}")
    subprocess.run(["git", "clone", REPO_URL, str(CLONE_DIR)], check=True)
    subprocess.run(["git", "-C", str(CLONE_DIR), "checkout", REPO_PIN], check=True)
    # save license alongside
    try:
        lic = (CLONE_DIR / "LICENSE").read_bytes()
        (OUT_DIR).mkdir(parents=True, exist_ok=True)
        (OUT_DIR / "TimelineQA.LICENSE.txt").write_bytes(lic)
    except OSError:
        url = "https://raw.githubusercontent.com/facebookresearch/TimelineQA/main/LICENSE"
        with urllib.request.urlopen(url, timeout=60) as resp:  # noqa: S310
            (OUT_DIR).mkdir(parents=True, exist_ok=True)
            (OUT_DIR / "TimelineQA.LICENSE.txt").write_bytes(resp.read())
    return CLONE_DIR


def count_atomic_qa(db_path: Path) -> int:
    data = json.loads(db_path.read_text(encoding="utf-8"))
    return sum(
        len(ev.get("atomic_qa_pairs", []) or [])
        for date in data
        for ev in data[date].values()
    )


def generate_persona(src_dir: Path, density: str, i: int) -> tuple[Path, int]:
    seed = SEED_BASE + i
    pdir = OUT_DIR / density / f"persona-{i:03d}"
    out_name = f"{density}-{i:03d}.json"
    db_path = pdir / out_name
    if db_path.exists():
        print(f"[acquire_timelineqa] skip (exists) {density}/persona-{i:03d}")
        return db_path, count_atomic_qa(db_path)
    pdir.mkdir(parents=True, exist_ok=True)
    cmd = [
        sys.executable, "generateDB.py",
        "-y", str(FINAL_YEAR),
        "-s", str(seed),
        "-c", density,
        "-d", str(pdir.resolve()),
        "-o", out_name,
    ]
    subprocess.run(cmd, cwd=str(src_dir), check=True, stdout=subprocess.DEVNULL)
    n_qa = count_atomic_qa(db_path)
    print(f"[acquire_timelineqa] generated {density}/persona-{i:03d} (seed={seed}): {n_qa} atomic QA")
    return db_path, n_qa


def main() -> int:
    clone = ensure_clone()
    src_dir = clone / "src"
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    per_density: dict[str, dict] = {}
    total_qa = 0
    total_personas = 0
    for density in DENSITIES:
        n_qa_density = 0
        personas = []
        for i in range(N_PERSONAS):
            _, n_qa = generate_persona(src_dir, density, i)
            n_qa_density += n_qa
            total_qa += n_qa
            total_personas += 1
            personas.append({"persona": f"persona-{i:03d}", "seed": SEED_BASE + i, "atomic_qa": n_qa})
        per_density[density] = {"personas": len(personas), "atomic_qa": n_qa_density, "detail": personas}
        print(f"[acquire_timelineqa] {density}: {len(personas)} personas, {n_qa_density} atomic QA")

    index = {
        "final_year": FINAL_YEAR,
        "seed_base": SEED_BASE,
        "n_personas_per_density": N_PERSONAS,
        "total_personas": total_personas,
        "total_atomic_qa": total_qa,
        "per_density": per_density,
    }
    (OUT_DIR / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
    print(f"[acquire_timelineqa] TOTAL: {total_personas} personas, {total_qa} atomic QA")

    # ── update manifest ────────────────────────────────────────────────────────
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("timelineqa", {})
    acquired_at = existing.get("acquired_at", "")
    if not acquired_at:
        import datetime
        acquired_at = datetime.date.today().isoformat()
    manifest.setdefault("sources", {})["timelineqa"] = {
        "script": "acquire_timelineqa.py",
        "upstream": {
            "kind": "github_generator_local_run",
            "id": "facebookresearch/TimelineQA",
            "revision": REPO_PIN,
            "generator": "src/generateDB.py",
            "params": {
                "final_year": FINAL_YEAR,
                "seed_base": SEED_BASE,
                "n_personas_per_density": N_PERSONAS,
                "densities": list(DENSITIES),
            },
        },
        "license": LICENSE_SPDX,
        "license_notes": LICENSE_NOTES,
        "distribution": "cache",
        "output_dir": "data/corpus-data/raw/timelineqa/",
        "total_personas": total_personas,
        "total_atomic_qa": total_qa,
        "per_density": {k: {"personas": v["personas"], "atomic_qa": v["atomic_qa"]} for k, v in per_density.items()},
        "acquired_at": acquired_at,
        "note": (
            "Locally generated (non-deterministic hashes — seed-pinned content, "
            "but per-run file layout). Multi-hop SQL gold (multihopQA.py, needs "
            "pandasql) NOT generated here; atomic_qa_pairs gold is embedded."
        ),
        "role_note": "Daily-life / personal-timeline QA — strongest pure daily-life fit.",
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print("[acquire_timelineqa] updated manifest.json")
    print("[acquire_timelineqa] DONE — payload is gitignored (EVAL-ONLY, CC-BY-NC)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
