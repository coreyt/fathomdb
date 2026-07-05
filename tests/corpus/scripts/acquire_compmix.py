#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "huggingface_hub>=0.23",
# ]
# ///
"""Acquire CompMix as a `kb` corpus with native Wikidata QID join keys.

Source:  pchristm/CompMix on the HuggingFace Hub — CompMix, a benchmark for
         heterogeneous question answering over Wikidata (Christmann et al.,
         WWW-2024). Each record is a question over a Wikidata-centric knowledge
         base, carrying the question's Wikidata entities and the answer
         entities as native QIDs.
License: CC-BY-4.0 (Christmann et al., WWW-2024). Redistribution permitted with
         attribution; produced JSONL is gitignored, never committed (cache-only,
         no-leak).
Pinned:  pchristm/CompMix@398eb2b9 (snapshot last-modified 2023-06-19T03:20:00Z).

This is the 2nd native-QID corpus for cross-source acquisition: it brings
heterogeneous Wikidata-QA docs (source_type=kb) carrying native QIDs in
`entity_ids`, so it can interconnect with WEC-Eng / news on the shared QID
spine. Unlike WEC-Eng, no Wikipedia API resolution is needed — the QIDs are
native to the dataset (question `entities` + `answers`).

Canonical output per question (CorpusDoc shape, see corpus-card.md):
  - source_type = "kb"
  - title       = the question text
  - body        = question, or "<question> <answer_text>" when an answer text
                  is present
  - created_at  = fixed provenance constant (dataset snapshot date; deterministic)
  - entity_ids  = deduped union of the question `entities` then `answers`, each
                  EntityRef(id=<QID>, kind="qid", surface=<label>); empty when a
                  record carries no well-formed QID

Config-driven / deterministic:
  A typed CompMixConfig(split, sample_size, seed) resolved through the shared
  _config helper (--config file.yaml + dotted --override key=val); the baked
  default lives in configs/acquire-compmix.yaml. Sampling is a seeded draw over
  question_id-sorted records; output order is question_id-sorted -> byte-stable
  for a fixed (split, sample_size, seed). No wall-clock is read into any document.

Output: data/corpus-data/raw/compmix.jsonl  (gitignored).
Also updates: tests/corpus/scripts/manifest.json — adds/refreshes 'compmix'.

Usage:
    uv run tests/corpus/scripts/acquire_compmix.py
    uv run tests/corpus/scripts/acquire_compmix.py --override split=dev
    uv run tests/corpus/scripts/acquire_compmix.py --config my-compmix.yaml
"""

from __future__ import annotations

import argparse
import datetime
import io
import json
import random
import sys
import zipfile
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _config import add_config_cli, resolve_config  # noqa: E402
from _corpus_lib import (  # noqa: E402
    _QID_RE,
    CorpusDoc,
    EntityRef,
    corpus_data_dir,
    doc_id,
    write_jsonl,
)

# ── dataset coordinates (pinned) ──────────────────────────────────────────────
DATASET_ID = "pchristm/CompMix"
DATASET_REVISION = "398eb2b9dec2b14fba6f86789da63414b3eecd89"
DATASET_LAST_MODIFIED = "2023-06-19T03:20:00Z"

SPLIT_FILES = {
    "train": "train_set.zip",
    "dev": "dev_set.zip",
    "test": "test_set.zip",
}

LICENSE_SPDX = "CC-BY-4.0"
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"

# Fixed provenance date — the pinned HF snapshot member date.
# Deterministic: never read the wall clock into a document.
CREATED_AT = "2023-06-19T00:00:00+00:00"

DEFAULT_SAMPLE_SIZE = 5000
DEFAULT_SEED = 20260702

OUT_PATH = corpus_data_dir() / "raw" / "compmix.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"


@dataclass
class CompMixConfig:
    """Typed acquire config (mirrors the WEC-Eng exemplar; see scripts/README.md).

    Resolved via ``--config``/``--override`` through the shared ``_config``
    helper; every field is consumed-or-loudly-rejected.
    """

    split: str = "train"
    sample_size: int = DEFAULT_SAMPLE_SIZE
    seed: int = DEFAULT_SEED

    def validate(self) -> None:
        if self.split not in SPLIT_FILES:
            raise ValueError(
                f"split must be one of {sorted(SPLIT_FILES)}, got {self.split!r}"
            )
        if (
            not isinstance(self.sample_size, int)
            or isinstance(self.sample_size, bool)
            or self.sample_size <= 0
        ):
            raise ValueError(
                f"sample_size must be a positive int, got {self.sample_size!r}"
            )
        if not isinstance(self.seed, int) or isinstance(self.seed, bool):
            raise ValueError(f"seed must be an int, got {self.seed!r}")


# ── conversion helpers (pure; unit-tested without network) ────────────────────


def doc_body(record: dict) -> str:
    """Body text: the question alone, or "<question> <answer_text>"."""
    question = (record.get("question") or "").strip()
    answer_text = (record.get("answer_text") or "").strip()
    return f"{question} {answer_text}" if answer_text else question


def entity_refs(record: dict) -> list[EntityRef]:
    """Deduped native QIDs from the question `entities` then `answers`.

    QIDs are deduped by id, preserving the first occurrence's surface (entities
    are visited before answers). Ids that do not match ``Q\\d+`` are skipped
    (defensive); a record with no well-formed QID yields an empty list.
    """
    refs: list[EntityRef] = []
    seen: set[str] = set()
    for group in (record.get("entities") or [], record.get("answers") or []):
        for ent in group:
            qid = ent.get("id")
            if not qid or not _QID_RE.fullmatch(qid) or qid in seen:
                continue
            seen.add(qid)
            refs.append(EntityRef(id=qid, kind="qid", surface=ent.get("label") or None))
    return refs


def build_doc(record: dict) -> CorpusDoc:
    """Convert a CompMix QA record into a canonical kb CorpusDoc."""
    native_id = str(record["question_id"])
    question = record.get("question") or None
    domain = record.get("domain")
    answer_src = record.get("answer_src", "")
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, native_id),
        source_type="kb",
        title=question,
        body=doc_body(record),
        created_at=CREATED_AT,
        modified_at=None,
        author_or_sender=None,
        recipients=[],
        people_mentions=[],
        project_mentions=[],
        tags=[f"compmix-domain:{domain}", f"answer-src:{answer_src}"],
        url_or_external_id=None,
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
        entity_ids=entity_refs(record),
    )


def sample_records(
    records: list[dict], sample_size: int | None, seed: int
) -> list[dict]:
    """Deterministic, bounded draw over records.

    Records are question_id-sorted first (stable base order), a seeded sample is
    drawn, then re-sorted by question_id so the written JSONL is byte-stable for
    a fixed (sample_size, seed). sample_size None or >= population returns all.
    """
    ordered = sorted(records, key=lambda r: str(r["question_id"]))
    if sample_size is None or sample_size >= len(ordered):
        return ordered
    picked = random.Random(seed).sample(ordered, sample_size)
    picked.sort(key=lambda r: str(r["question_id"]))
    return picked


# ── upstream load (network; per-split zip -> in-memory jsonl member) ──────────


def load_split_records(split: str) -> list[dict]:
    """Download the pinned per-split zip and read its jsonl member in-memory."""
    from huggingface_hub import hf_hub_download  # type: ignore[import-not-found]

    zip_name = SPLIT_FILES[split]
    member = f"{split}_set.jsonl"
    local = hf_hub_download(
        DATASET_ID,
        zip_name,
        repo_type="dataset",
        revision=DATASET_REVISION,
    )
    records: list[dict] = []
    with zipfile.ZipFile(local) as zf:
        with zf.open(member) as fh:
            for line in io.TextIOWrapper(fh, encoding="utf-8"):
                line = line.strip()
                if line:
                    records.append(json.loads(line))
    return records


# ── main ──────────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(description="Acquire CompMix as a kb corpus.")
    add_config_cli(parser)
    args = parser.parse_args()
    cfg = resolve_config(CompMixConfig, args, CompMixConfig())

    split_file = SPLIT_FILES[cfg.split]
    print(f"[compmix] dataset:  {DATASET_ID}@{DATASET_REVISION[:8]}")
    print(f"[compmix] split:    {cfg.split} ({split_file})")
    print(f"[compmix] sample:   size={cfg.sample_size} seed={cfg.seed}")
    print(f"[compmix] output:   {OUT_PATH}")

    records = load_split_records(cfg.split)
    print(f"[compmix] loaded {len(records)} records from {split_file}")

    sampled = sample_records(records, cfg.sample_size, cfg.seed)
    print(f"[compmix] sampled {len(sampled)} records")

    docs = [build_doc(r) for r in sampled]
    count, sha = write_jsonl(OUT_PATH, docs)

    with_qid = sum(1 for d in docs if d.entity_ids)
    coverage = with_qid / count if count else 0.0
    distinct_qids = {ref.id for d in docs for ref in d.entity_ids}
    print(f"[compmix] wrote {count} docs -> {OUT_PATH}")
    print(f"[compmix] sha256 = {sha}")
    print(
        f"[compmix] QID coverage: {with_qid}/{count} ({coverage:.1%}); "
        f"{len(distinct_qids)} distinct QIDs"
    )
    examples = sorted(distinct_qids)[:5]
    if examples:
        print(f"[compmix] example QIDs: {', '.join(examples)}")

    # ── update manifest.json (preserve acquired_at for reproduce byte-stability) ─
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("compmix", {})
    acquired_at = existing.get("acquired_at", datetime.date.today().isoformat())
    manifest["sources"]["compmix"] = {
        "script": "acquire_compmix.py",
        "upstream": {
            "kind": "huggingface_dataset_zip",
            "id": DATASET_ID,
            "split": cfg.split,
            "file": split_file,
            "revision": DATASET_REVISION,
            "last_modified": DATASET_LAST_MODIFIED,
            "note": "native Wikidata QIDs from CompMix entities+answers",
        },
        "license": LICENSE_SPDX,
        "license_notes": (
            "CC-BY-4.0 (Christmann et al., CompMix, WWW-2024 — heterogeneous "
            "question answering over Wikidata). Redistribution permitted with "
            "attribution; produced JSONL is cache-only / no-leak (gitignored)."
        ),
        "distribution": "cache",
        "output": "data/corpus-data/raw/compmix.jsonl",
        "sample_size": cfg.sample_size,
        "seed": cfg.seed,
        "doc_count": count,
        "qid_coverage": round(coverage, 4),
        "docs_with_qid": with_qid,
        "distinct_qids": len(distinct_qids),
        "sha256": sha,
        "acquired_at": acquired_at,
        "role_note": (
            "2nd native-QID corpus: heterogeneous Wikidata-QA docs "
            "(source_type=kb) carrying native QIDs in entity_ids — the kb axis "
            "for cross-source interconnection with wec_eng/news."
        ),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print(f"[compmix] updated manifest.json (compmix sha256={sha[:16]}...)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
