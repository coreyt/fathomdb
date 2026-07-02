#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "huggingface_hub>=0.23",
# ]
# ///
"""Acquire WEC-Eng as an `event` corpus with native Wikidata QID join keys.

Source:  Intel/WEC-Eng on the HuggingFace Hub — Wikipedia Event Coreference,
         English (Eirew et al. 2021, NAACL, https://aclanthology.org/2021.naacl-main.198/).
         Cross-document event-coreference mentions extracted from English
         Wikipedia; each gold mention is anchored to a Wikipedia event page
         (the `coref_link` title).
License: Undeclared on the HF dataset card. Content derives from English
         Wikipedia text (CC-BY-SA-4.0); the extract-wec generator (Intel) is
         Apache-2.0. Treated as cache-only until the posture is clarified —
         produced JSONL is gitignored, never committed.
Pinned:  Intel/WEC-Eng@e9a0f74a (last-modified 2021-10-04T11:21:48Z).

This is the pattern-setter for cross-source acquisition: it brings a corpus
whose docs carry Wikidata **QIDs** in `entity_ids`, so it can interconnect with
other corpora on the shared QID spine. QIDs are derived by resolving each event
page's Wikipedia title -> its `wikibase_item` via the MediaWiki API (batched,
cached; no extra dependency downloads).

Canonical output per mention (CorpusDoc shape, see corpus-card.md):
  - source_type = "event"
  - title       = the event page title (coref_link)
  - body        = the detokenized mention-context passage
  - created_at  = fixed provenance constant (dataset snapshot date; deterministic)
  - entity_ids  = [EntityRef(id=<QID>, kind="qid", surface=<mention span>)]
                  for each resolved event page (empty when a title has no QID)

Config-driven / deterministic:
  --split (train|dev|test), --sample-size (bounded; default 3000), --seed.
  Sampling is a seeded draw over mention_id-sorted records; output order is
  mention_id-sorted -> byte-stable for a fixed (split, sample-size, seed).
  No wall-clock is read into any document.

Output: data/corpus-data/raw/wec_eng.jsonl  (gitignored).
Also updates: tests/corpus/scripts/manifest.json — adds/refreshes 'wec_eng'.

Usage:
    uv run tests/corpus/scripts/acquire_wec_eng.py --split train --sample-size 3000
"""

from __future__ import annotations

import argparse
import datetime
import json
import random
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import (  # noqa: E402
    CorpusDoc,
    EntityRef,
    corpus_data_dir,
    doc_id,
    write_jsonl,
)

# ── dataset coordinates (pinned) ──────────────────────────────────────────────
DATASET_ID = "Intel/WEC-Eng"
DATASET_REVISION = "e9a0f74a6fc2ec7a12ce66299dcffc693d2bf0d9"
DATASET_LAST_MODIFIED = "2021-10-04T11:21:48Z"

SPLIT_FILES = {
    "train": "Train_Event_gold_mentions.json",
    "dev": "Dev_Event_gold_mentions_validated.json",
    "test": "Test_Event_gold_mentions_validated.json",
}

LICENSE_SPDX = "LicenseRef-WEC-Eng-Undeclared"
PROVENANCE = f"hf:{DATASET_ID}@{DATASET_REVISION[:8]}"

# Fixed provenance date — the pinned HF snapshot's last-modified timestamp.
# Deterministic: never read the wall clock into a document.
CREATED_AT = "2021-10-04T11:21:48+00:00"

DEFAULT_SAMPLE_SIZE = 3000
DEFAULT_SEED = 20260702

WIKIPEDIA_API = "https://en.wikipedia.org/w/api.php"
WIKIPEDIA_BATCH = 50
WIKIPEDIA_THROTTLE_S = 1.0   # polite inter-batch delay
WIKIPEDIA_MAX_RETRIES = 6
USER_AGENT = "fathomdb-corpus-acquire/0.8 (WEC-Eng event QID join keys)"

OUT_PATH = corpus_data_dir() / "raw" / "wec_eng.jsonl"
MANIFEST_PATH = Path(__file__).resolve().parent / "manifest.json"


# ── conversion helpers (pure; unit-tested without network) ────────────────────

def mention_body(mention: dict) -> str:
    """Detokenized event-context passage for a WEC gold mention."""
    return " ".join(mention.get("mention_context") or [])


def wiki_url(title: str) -> str:
    return "https://en.wikipedia.org/wiki/" + urllib.parse.quote(title.replace(" ", "_"))


def build_doc(mention: dict, qid: str | None) -> CorpusDoc:
    """Convert a WEC-Eng gold mention into a canonical event CorpusDoc.

    `qid` is the Wikidata id resolved from the mention's event page
    (`coref_link`); None when the title has no `wikibase_item`, in which case
    `entity_ids` stays empty (additive, backward-compatible).
    """
    native_id = str(mention["mention_id"])
    surface = mention.get("tokens_str") or None
    coref_link = mention.get("coref_link") or None
    entity_ids = [EntityRef(id=qid, kind="qid", surface=surface)] if qid else []
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, native_id),
        source_type="event",
        title=coref_link,
        body=mention_body(mention),
        created_at=CREATED_AT,
        modified_at=None,
        author_or_sender=None,
        recipients=[],
        people_mentions=[],
        project_mentions=[],
        tags=["event-coref", f"coref_chain:{mention.get('coref_chain')}"],
        url_or_external_id=wiki_url(coref_link) if coref_link else None,
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
        entity_ids=entity_ids,
    )


def sample_records(records: list[dict], sample_size: int | None, seed: int) -> list[dict]:
    """Deterministic, bounded draw over records.

    Records are mention_id-sorted first (stable base order), a seeded sample is
    drawn, then re-sorted by mention_id so the written JSONL is byte-stable for
    a fixed (sample_size, seed). sample_size None or >= population returns all.
    """
    ordered = sorted(records, key=lambda r: str(r["mention_id"]))
    if sample_size is None or sample_size >= len(ordered):
        return ordered
    picked = random.Random(seed).sample(ordered, sample_size)
    picked.sort(key=lambda r: str(r["mention_id"]))
    return picked


# ── Wikipedia title -> Wikidata QID resolution (network; batched + cached) ────

def _resolve_title_batch(titles: list[str]) -> dict[str, str | None]:
    params = {
        "action": "query",
        "format": "json",
        "prop": "pageprops",
        "ppprop": "wikibase_item",
        "redirects": "1",
        "titles": "|".join(titles),
    }
    url = WIKIPEDIA_API + "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    for attempt in range(WIKIPEDIA_MAX_RETRIES):
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                payload = json.load(resp)
            break
        except urllib.error.HTTPError as exc:
            if exc.code not in (429, 503) or attempt == WIKIPEDIA_MAX_RETRIES - 1:
                raise
            backoff = float(exc.headers.get("Retry-After") or 2 ** attempt)
            print(f"  [retry] HTTP {exc.code}; sleeping {backoff:.0f}s", flush=True)
            time.sleep(backoff)

    query = payload.get("query", {})
    # Map the final (post-normalize/redirect) page title -> QID.
    final_title_to_qid: dict[str, str | None] = {}
    for page in query.get("pages", {}).values():
        final_title_to_qid[page.get("title")] = page.get("pageprops", {}).get(
            "wikibase_item"
        )
    # Follow normalize + redirect chains from each requested title.
    forward = {m["from"]: m["to"] for m in query.get("normalized", [])}
    for m in query.get("redirects", []):
        forward[m["from"]] = m["to"]

    out: dict[str, str | None] = {}
    for title in titles:
        cur = title
        seen = set()
        while cur in forward and cur not in seen:
            seen.add(cur)
            cur = forward[cur]
        out[title] = final_title_to_qid.get(cur)
    return out


def resolve_qids(titles: list[str]) -> dict[str, str | None]:
    """Resolve unique Wikipedia titles -> Wikidata QIDs (None if unresolved)."""
    unique = sorted({t for t in titles if t})
    resolved: dict[str, str | None] = {}
    for i in range(0, len(unique), WIKIPEDIA_BATCH):
        batch = unique[i : i + WIKIPEDIA_BATCH]
        resolved.update(_resolve_title_batch(batch))
        print(f"  resolved {min(i + WIKIPEDIA_BATCH, len(unique))}/{len(unique)} titles", flush=True)
        if i + WIKIPEDIA_BATCH < len(unique):
            time.sleep(WIKIPEDIA_THROTTLE_S)
    return resolved


# ── main ──────────────────────────────────────────────────────────────────────

def main() -> int:
    from huggingface_hub import hf_hub_download  # type: ignore[import-not-found]

    parser = argparse.ArgumentParser(description="Acquire WEC-Eng as an event corpus.")
    parser.add_argument("--split", choices=sorted(SPLIT_FILES), default="train")
    parser.add_argument("--sample-size", type=int, default=DEFAULT_SAMPLE_SIZE)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    args = parser.parse_args()

    split_file = SPLIT_FILES[args.split]
    print(f"[wec_eng] dataset:  {DATASET_ID}@{DATASET_REVISION[:8]}")
    print(f"[wec_eng] split:    {args.split} ({split_file})")
    print(f"[wec_eng] sample:   size={args.sample_size} seed={args.seed}")
    print(f"[wec_eng] output:   {OUT_PATH}")

    local = hf_hub_download(
        DATASET_ID,
        split_file,
        repo_type="dataset",
        revision=DATASET_REVISION,
    )
    records = json.loads(Path(local).read_text(encoding="utf-8"))
    print(f"[wec_eng] loaded {len(records)} gold mentions from {split_file}")

    sampled = sample_records(records, args.sample_size, args.seed)
    print(f"[wec_eng] sampled {len(sampled)} mentions")

    titles = [r.get("coref_link") for r in sampled]
    qid_map = resolve_qids(titles)

    docs = [build_doc(r, qid_map.get(r.get("coref_link"))) for r in sampled]
    count, sha = write_jsonl(OUT_PATH, docs)

    with_qid = sum(1 for d in docs if d.entity_ids)
    coverage = with_qid / count if count else 0.0
    distinct_qids = {d.entity_ids[0].id for d in docs if d.entity_ids}
    print(f"[wec_eng] wrote {count} docs -> {OUT_PATH}")
    print(f"[wec_eng] sha256 = {sha}")
    print(f"[wec_eng] QID coverage: {with_qid}/{count} ({coverage:.1%}); "
          f"{len(distinct_qids)} distinct QIDs")
    examples = sorted(distinct_qids)[:5]
    if examples:
        print(f"[wec_eng] example QIDs: {', '.join(examples)}")

    # ── update manifest.json (preserve acquired_at for reproduce byte-stability) ─
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    existing = manifest.get("sources", {}).get("wec_eng", {})
    acquired_at = existing.get("acquired_at", datetime.date.today().isoformat())
    manifest["sources"]["wec_eng"] = {
        "script": "acquire_wec_eng.py",
        "upstream": {
            "kind": "huggingface_dataset_json_files",
            "id": DATASET_ID,
            "split": args.split,
            "file": split_file,
            "revision": DATASET_REVISION,
            "last_modified": DATASET_LAST_MODIFIED,
            "qid_resolution": "en.wikipedia.org MediaWiki API pageprops.wikibase_item",
        },
        "license": LICENSE_SPDX,
        "license_notes": (
            "Undeclared on the Intel/WEC-Eng HF card. Content derives from "
            "English Wikipedia text (CC-BY-SA-4.0); the extract-wec generator "
            "(Intel) is Apache-2.0 (Eirew et al. 2021, NAACL, "
            "https://aclanthology.org/2021.naacl-main.198/). Cache-only until "
            "the posture is clarified."
        ),
        "distribution": "cache",
        "output": "data/corpus-data/raw/wec_eng.jsonl",
        "sample_size": args.sample_size,
        "seed": args.seed,
        "doc_count": count,
        "qid_coverage": round(coverage, 4),
        "docs_with_qid": with_qid,
        "distinct_qids": len(distinct_qids),
        "sha256": sha,
        "acquired_at": acquired_at,
        "role_note": (
            "Cross-source pattern-setter: event-centric passages carrying native "
            "Wikidata QIDs in entity_ids, the shared join spine for CompMix / "
            "MultiHop-RAG / S2ORC cross-source interconnection."
        ),
    }
    MANIFEST_PATH.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print(f"[wec_eng] updated manifest.json (wec_eng sha256={sha[:16]}...)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
