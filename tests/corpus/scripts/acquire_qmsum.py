#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire QMSum query-focused meeting summarization corpus.

Source:  github.com/Yale-LILY/QMSum (NAACL 2021).
Pinned:  commit 83d7768c1f2b4dfeb091385d3dc7e239b8e5bb7e (2023-08-29).
Repo LICENSE: MIT (covers the QMSum annotations).

UPSTREAM PROVENANCE CHAIN. The repo's MIT license covers the
query-summary *annotation* layer added by Yale-LILY. The underlying
meeting transcripts derive from three pre-existing corpora — AMI
(CC-BY-4.0), ICSI (CC-BY-NC variants), and Canadian Parliament
Standing Committee transcripts (Crown copyright, redistribution per
the Reproduction of Federal Law Order). Per research doc §1.2 this
chain has not been verified end-to-end for unrestricted commercial
redistribution, so QMSum-derived JSONL is treated as **cache-only**.
A future HITL pass can flip this to commit-OK if the chain is verified.

All produced JSONL lives at data/corpus-data/raw/qmsum.jsonl
(gitignored — see ../corpus-card.md §"CI artifact + cache layout").

The pipeline:
  1. Download the repo's archive tarball at the pinned SHA (faster
     than per-file fetches: ~14MB vs ~700 round trips).
  2. Parse per-meeting JSON files under data/Academic, data/Product,
     data/Committee (avoiding the data/ALL duplicate of the same
     meetings).
  3. For each meeting in canonical sorted-path order, emit up to 3
     docs:
       - the transcript (body = concatenated `speaker: content`),
       - the first general-query summary (title = query, body = answer),
       - the first specific-query summary,
     and stop the global walk at TARGET_COUNT.

  thread_id for all three docs from a meeting is the meeting_id;
  the query-summary docs set parent_doc_id to the transcript doc_id.
"""

from __future__ import annotations

import hashlib
import io
import json
import sys
import tarfile
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_data_dir, doc_id, write_jsonl  # noqa: E402

UPSTREAM_REPO = "Yale-LILY/QMSum"
UPSTREAM_SHA = "83d7768c1f2b4dfeb091385d3dc7e239b8e5bb7e"
ARCHIVE_URL = f"https://codeload.github.com/{UPSTREAM_REPO}/tar.gz/{UPSTREAM_SHA}"
PROVENANCE = f"github:{UPSTREAM_REPO}@{UPSTREAM_SHA}"
# Composite license tag: QMSum annotations MIT, upstream chain mixed.
LICENSE_SPDX = "LicenseRef-QMSum-MIT-with-upstream-chain"
TARGET_COUNT = 600

# Walk only the per-domain dirs (Academic / Product / Committee). data/ALL
# is just a re-pack of the same meetings — including it would double-count.
DOMAIN_DIRS = ("data/Academic/", "data/Product/", "data/Committee/")


def fetch_archive() -> bytes:
    print(f"fetching {ARCHIVE_URL}", flush=True)
    with urllib.request.urlopen(ARCHIVE_URL) as resp:
        data = resp.read()
    print(f"  archive size: {len(data)} bytes", flush=True)
    return data


def transcript_body(utterances: list[dict]) -> str:
    """Render utterances as `Speaker: content` newline-separated."""
    out = []
    for u in utterances:
        spk = (u.get("speaker") or "").strip() or "Speaker"
        content = (u.get("content") or "").strip()
        if content:
            out.append(f"{spk}: {content}")
    return "\n".join(out)


def make_docs_for_meeting(domain: str, meeting_id: str, payload: dict) -> list[CorpusDoc]:
    docs: list[CorpusDoc] = []
    transcripts = payload.get("meeting_transcripts") or []
    if not transcripts:
        return docs
    body = transcript_body(transcripts)
    if not body.strip():
        return docs
    transcript_doc_id = doc_id(PROVENANCE, f"transcript:{meeting_id}")
    docs.append(CorpusDoc(
        doc_id=transcript_doc_id,
        source_type="meeting",
        title=f"{domain} meeting: {meeting_id}",
        body=body,
        created_at="2021-01-01T00:00:00+00:00",  # QMSum release-date anchor (no per-meeting dates in source)
        modified_at=None,
        author_or_sender=None,
        recipients=[],
        people_mentions=sorted({u.get("speaker") for u in transcripts if u.get("speaker")}),
        project_mentions=[domain.lower()],
        tags=["qmsum-domain:" + domain, "qmsum:transcript"],
        url_or_external_id=f"qmsum:{domain}:{meeting_id}",
        thread_id=meeting_id,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    ))
    general = payload.get("general_query_list") or []
    if general:
        q = general[0]
        docs.append(CorpusDoc(
            doc_id=doc_id(PROVENANCE, f"general:{meeting_id}:0"),
            source_type="meeting",
            title=str(q.get("query", "")),
            body=str(q.get("answer", "")),
            created_at="2021-01-01T00:00:00+00:00",
            modified_at=None,
            author_or_sender=None,
            recipients=[],
            people_mentions=[],
            project_mentions=[domain.lower()],
            tags=["qmsum-domain:" + domain, "qmsum:general-summary", "relation:summarizes"],
            url_or_external_id=f"qmsum:{domain}:{meeting_id}:general:0",
            thread_id=meeting_id,
            parent_doc_id=transcript_doc_id,
            license=LICENSE_SPDX,
            provenance=PROVENANCE,
        ))
    specific = payload.get("specific_query_list") or []
    if specific:
        q = specific[0]
        docs.append(CorpusDoc(
            doc_id=doc_id(PROVENANCE, f"specific:{meeting_id}:0"),
            source_type="meeting",
            title=str(q.get("query", "")),
            body=str(q.get("answer", "")),
            created_at="2021-01-01T00:00:00+00:00",
            modified_at=None,
            author_or_sender=None,
            recipients=[],
            people_mentions=[],
            project_mentions=[domain.lower()],
            tags=["qmsum-domain:" + domain, "qmsum:specific-summary", "relation:summarizes"],
            url_or_external_id=f"qmsum:{domain}:{meeting_id}:specific:0",
            thread_id=meeting_id,
            parent_doc_id=transcript_doc_id,
            license=LICENSE_SPDX,
            provenance=PROVENANCE,
        ))
    return docs


def iter_meetings(archive: bytes):
    """Yield (domain, meeting_id, payload) in canonical sorted-path order."""
    # tarfile in streaming mode doesn't allow re-seek; we need sorted order, so
    # buffer payloads into a dict first.
    found: dict[str, tuple[str, dict]] = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tf:
        for m in tf:
            if not m.isfile() or not m.name.endswith(".json"):
                continue
            # First path component is "<repo>-<sha>/"; strip it.
            parts = m.name.split("/", 1)
            if len(parts) < 2:
                continue
            inner = parts[1]
            if not any(inner.startswith(d) for d in DOMAIN_DIRS):
                continue
            # Only the per-split dirs (train/val/test) hold per-meeting files.
            sub = inner.split("/")
            if len(sub) != 4 or sub[2] not in ("train", "val", "test"):
                continue
            domain = sub[1]
            meeting_id = sub[3].removesuffix(".json")
            f = tf.extractfile(m)
            if f is None:
                continue
            payload = json.loads(f.read().decode("utf-8"))
            found[inner] = (domain, meeting_id, payload)
    for path in sorted(found.keys()):
        d, mid, p = found[path]
        yield d, mid, p


def main() -> int:
    archive = fetch_archive()
    archive_sha = hashlib.sha256(archive).hexdigest()
    print(f"archive sha256: {archive_sha}", flush=True)

    out_path = corpus_data_dir() / "raw" / "qmsum.jsonl"

    docs: list[CorpusDoc] = []
    meetings_seen = 0
    for domain, meeting_id, payload in iter_meetings(archive):
        meetings_seen += 1
        for d in make_docs_for_meeting(domain, meeting_id, payload):
            docs.append(d)
            if len(docs) >= TARGET_COUNT:
                break
        if len(docs) >= TARGET_COUNT:
            break
    print(f"used {meetings_seen} meetings", flush=True)

    count, sha = write_jsonl(out_path, docs)
    print(f"wrote {count} docs to {out_path}")
    print(f"sha256 = {sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
